//! Comandos Tauri de tareas (issues #95, #96): `listar_tareas`, `crear_tarea`,
//! `crear_tarea_con_subtareas`, `editar_tarea` y `tomar_tarea`.

use std::path::Path;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::tareas::Tarea;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, hoy_local, EstadoTarea};
use crate::ramas;
use crate::servidor::{conectar, LlaveroSistema};
use crate::tareas::{
    self, AutorTarea, CambiosTarea, ConflictoMerge, DatosNuevaTarea, DatosNuevaTareaConSubtareas,
    ResultadoAutorizacion, ResultadoCambioEstado,
};

fn con_ops<T>(
    estado: &AppState,
    comando: &str,
    f: impl FnOnce(&rusqlite::Connection, &Path, &AutorTarea<'_>) -> Resultado<T>,
) -> Resultado<T> {
    let yo = estado.exigir_permiso(comando)?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| f(db.conexion(), &ops, &autor))
}

/// Todas las tareas del proyecto abierto (incluye las de la Papelera y el campo
/// calculado `edicion_pendiente`).
#[tauri::command]
pub fn listar_tareas(estado: State<'_, AppState>) -> Resultado<Vec<Tarea>> {
    let _yo = estado.exigir_permiso("listar_tareas")?;
    estado.con_db(|db| tareas::listar(db.conexion()))
}

#[tauri::command]
pub fn crear_tarea(estado: State<'_, AppState>, datos: DatosNuevaTarea) -> Resultado<Tarea> {
    con_ops(&estado, "crear_tarea", |c, ops, autor| {
        tareas::crear(c, ops, autor, &datos, &ahora_iso(), &hoy_local())
    })
}

#[tauri::command]
pub fn crear_tarea_con_subtareas(
    estado: State<'_, AppState>,
    datos: DatosNuevaTareaConSubtareas,
) -> Resultado<Vec<Tarea>> {
    con_ops(&estado, "crear_tarea_con_subtareas", |c, ops, autor| {
        tareas::crear_con_subtareas(c, ops, autor, &datos, &ahora_iso(), &hoy_local())
    })
}

#[tauri::command]
pub fn editar_tarea(
    estado: State<'_, AppState>,
    id: String,
    campos: CambiosTarea,
) -> Resultado<Tarea> {
    con_ops(&estado, "editar_tarea", |c, ops, autor| {
        tareas::editar(c, ops, autor, &id, &campos, &ahora_iso())
    })
}

/// `tomar_tarea(id)`: todos los roles (Colaborador externo solo en tareas
/// individuales). Crea en Unity VC las ramas que falten en la cadena (desde la
/// tarea general hasta `id`), cambia el workspace local a la rama de `id` con
/// `cm switch`, y publica el evento `tomar_tarea` en `ops/`.
/// Devuelve todas las tareas de la cadena actualizadas.
#[tauri::command]
pub async fn tomar_tarea(estado: State<'_, AppState>, id: String) -> Resultado<Vec<Tarea>> {
    tomar_en(&estado, &id)
}

/// Lo mismo que [`tomar_tarea`], sin el envoltorio de Tauri (para pruebas en el
/// entorno real).
pub fn tomar_en(estado: &AppState, id: &str) -> Resultado<Vec<Tarea>> {
    let yo = estado.exigir_permiso("tomar_tarea")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let repo = estado.repo_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let cadena = estado.con_db(|db| tareas::cadena_para_tomar(db.conexion(), &autor, id))?;

    // Antes de crear nada en el servidor, revisamos que el workspace pueda cambiar de rama.
    let pendientes = ramas::cambios_pendientes(estado, &workspace)?;
    if !pendientes.is_empty() {
        return Err(Error::Validacion(format!(
            "Tienes {} cambio(s) sin check-in en esta rama. Haz check-in o deshazlos antes de cambiar de rama",
            pendientes.len()
        )));
    }

    let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
    let creadas = ramas::crear_ramas_de_cadena(&cliente, &repo.repo, &cadena)?;

    let id_limpio = id.trim();
    let rama_final = creadas
        .iter()
        .find(|(tid, _)| tid == id_limpio)
        .map(|(_, r)| r.name.clone())
        .or_else(|| cadena.last().and_then(|t| t.rama.clone()))
        .ok_or_else(|| Error::Interno("No se determinó la rama de la tarea tomada".into()))?;

    ramas::cambiar_a_rama(estado, &workspace, &rama_final)?;

    estado.con_db(|db| {
        tareas::registrar_toma(db.conexion(), &ops, &autor, &cadena, &creadas, &ahora_iso())
    })
}

/// `volver_a_dev()`: cambia el workspace local a `/main/dev` (`cm switch`).
/// Se usa desde la notificación de doble toma revertida ("Volver mi workspace a dev", #97).
#[tauri::command]
pub async fn volver_a_dev(estado: State<'_, AppState>) -> Resultado<String> {
    let _yo = estado.exigir_permiso("volver_a_dev")?;
    let workspace = estado.workspace_abierto()?;
    let actual = ramas::cambiar_a_rama(&estado, &workspace, ramas::RAMA_DEV)?;
    Ok(actual.rama)
}

/// `asignar_tarea(id, email)`: Líder/Admin asigna la tarea a otra persona (#98).
/// Crea en Unity VC las ramas que falten (sin tocar el workspace de quien asigna),
/// publica el evento `asignar_tarea` en `ops/` y genera notificación `tarea_asignada`.
#[tauri::command]
pub async fn asignar_tarea(
    estado: State<'_, AppState>,
    id: String,
    email: String,
) -> Resultado<Vec<Tarea>> {
    asignar_en(&estado, &id, &email)
}

/// Lo mismo que [`asignar_tarea`], sin el envoltorio de Tauri (para pruebas en el
/// entorno real).
pub fn asignar_en(estado: &AppState, id: &str, email_destinatario: &str) -> Resultado<Vec<Tarea>> {
    let yo = estado.exigir_permiso("asignar_tarea")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let repo = estado.repo_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let cadena = estado
        .con_db(|db| tareas::cadena_para_asignar(db.conexion(), &autor, id, email_destinatario))?;

    let creadas = if cadena.iter().any(|p| p.rama.is_none()) {
        let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
        ramas::crear_ramas_de_cadena(&cliente, &repo.repo, &cadena)?
    } else {
        Vec::new()
    };

    estado.con_db(|db| {
        tareas::registrar_asignacion(
            db.conexion(),
            &ops,
            &autor,
            &cadena,
            &creadas,
            email_destinatario,
            &ahora_iso(),
        )
    })
}

/// `desasignar_tarea(id)`: suelta una tarea (todos los roles) si ni ella ni sus
/// subtareas tienen check-ins (#98).
#[tauri::command]
pub async fn desasignar_tarea(estado: State<'_, AppState>, id: String) -> Resultado<Tarea> {
    desasignar_en(&estado, &id)
}

/// Lo mismo que [`desasignar_tarea`], sin el envoltorio de Tauri.
pub fn desasignar_en(estado: &AppState, id: &str) -> Resultado<Tarea> {
    con_ops(estado, "desasignar_tarea", |c, ops, autor| {
        tareas::desasignar(c, ops, autor, id, &ahora_iso())
    })
}

/// `cambiar_estado(id, estado)`: cambia el estado de una tarea con todas las
/// reglas de subtareas, progreso, préstamos y rol (#99).
#[tauri::command]
pub async fn cambiar_estado(
    estado: State<'_, AppState>,
    id: String,
    nuevo_estado: EstadoTarea,
) -> Resultado<ResultadoCambioEstado> {
    cambiar_estado_en(&estado, &id, nuevo_estado)
}

/// Lo mismo que [`cambiar_estado`], sin el envoltorio de Tauri (para pruebas en
/// el entorno real).
pub fn cambiar_estado_en(
    estado: &AppState,
    id: &str,
    nuevo_estado: EstadoTarea,
) -> Resultado<ResultadoCambioEstado> {
    con_ops(estado, "cambiar_estado", |c, ops, autor| {
        tareas::cambiar_estado(c, ops, autor, id, nuevo_estado, &ahora_iso())
    })
}

/// `borrar_progreso(id)`: revierte en Unity VC las ramas del subárbol hasta su
/// changeset inicial (descartando cambios pendientes sin check-in) y borra
/// commits/préstamos en SQLite + `ops/` dejando la tarea en `'por hacer'` (#100).
#[tauri::command]
pub async fn borrar_progreso(estado: State<'_, AppState>, id: String) -> Resultado<Vec<Tarea>> {
    borrar_progreso_en(&estado, &id)
}

/// Lo mismo que [`borrar_progreso`], sin el envoltorio de Tauri (para pruebas en
/// el entorno real).
pub fn borrar_progreso_en(estado: &AppState, id: &str) -> Resultado<Vec<Tarea>> {
    let yo = estado.exigir_permiso("borrar_progreso")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let repo = estado.repo_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let ramas_a_revertir = estado.con_db(|db| tareas::ramas_de_subarbol(db.conexion(), id))?;
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    for rama in &ramas_a_revertir {
        ramas::revertir_rama_a_base(
            estado,
            &workspace,
            &repo_cm,
            rama,
            &format!("Revertir progreso de {}", id.trim()),
        )?;
    }

    estado.con_db(|db| tareas::borrar_progreso(db.conexion(), &ops, &autor, id, &ahora_iso()))
}

/// `reabrir_tarea(id, motivo)`: devuelve una tarea `'terminada'` a `'en revisión'`
/// con motivo obligatorio (máx. 3 veces; luego solo Administrador) y notifica a
/// Líderes y Administradores (`tarea_reabierta`, #100).
#[tauri::command]
pub async fn reabrir_tarea(
    estado: State<'_, AppState>,
    id: String,
    motivo: String,
) -> Resultado<Tarea> {
    reabrir_en(&estado, &id, &motivo)
}

/// Lo mismo que [`reabrir_tarea`], sin el envoltorio de Tauri.
pub fn reabrir_en(estado: &AppState, id: &str, motivo: &str) -> Resultado<Tarea> {
    con_ops(estado, "reabrir_tarea", |c, ops, autor| {
        tareas::reabrir(c, ops, autor, id, motivo, &ahora_iso())
    })
}

/// `aprobar_tarea(id)`: Líder/Admin aprueba una tarea pendiente (`pendiente_aprobacion = false`) (#101).
#[tauri::command]
pub async fn aprobar_tarea(estado: State<'_, AppState>, id: String) -> Resultado<Tarea> {
    aprobar_tarea_en(&estado, &id)
}

/// Lo mismo que [`aprobar_tarea`], sin el envoltorio de Tauri.
pub fn aprobar_tarea_en(estado: &AppState, id: &str) -> Resultado<Tarea> {
    con_ops(estado, "aprobar_tarea", |c, ops, autor| {
        tareas::aprobar_tarea(c, ops, autor, id, &ahora_iso())
    })
}

/// `aprobar_tareas(ids)`: Líder/Admin aprueba varias tareas pendientes a la vez (#101).
#[tauri::command]
pub async fn aprobar_tareas(
    estado: State<'_, AppState>,
    ids: Vec<String>,
) -> Resultado<Vec<Tarea>> {
    aprobar_tareas_en(&estado, &ids)
}

/// Lo mismo que [`aprobar_tareas`], sin el envoltorio de Tauri.
pub fn aprobar_tareas_en(estado: &AppState, ids: &[String]) -> Resultado<Vec<Tarea>> {
    con_ops(estado, "aprobar_tareas", |c, ops, autor| {
        tareas::aprobar_tareas(c, ops, autor, ids, &ahora_iso())
    })
}

/// `editar_y_aprobar_tarea(id, campos)`: Líder/Admin edita y aprueba una tarea pendiente en un solo comando (#101).
#[tauri::command]
pub async fn editar_y_aprobar_tarea(
    estado: State<'_, AppState>,
    id: String,
    campos: CambiosTarea,
) -> Resultado<Tarea> {
    editar_y_aprobar_en(&estado, &id, &campos)
}

/// Lo mismo que [`editar_y_aprobar_tarea`], sin el envoltorio de Tauri.
pub fn editar_y_aprobar_en(estado: &AppState, id: &str, campos: &CambiosTarea) -> Resultado<Tarea> {
    con_ops(estado, "editar_y_aprobar_tarea", |c, ops, autor| {
        tareas::editar_y_aprobar(c, ops, autor, id, campos, &ahora_iso())
    })
}

/// `advertir_tarea_pendiente(id, mensaje)`: Líder/Admin avisa al creador sin aprobarla (#101).
#[tauri::command]
pub async fn advertir_tarea_pendiente(
    estado: State<'_, AppState>,
    id: String,
    mensaje: String,
) -> Resultado<()> {
    advertir_pendiente_en(&estado, &id, &mensaje)
}

/// Lo mismo que [`advertir_tarea_pendiente`], sin el envoltorio de Tauri.
pub fn advertir_pendiente_en(estado: &AppState, id: &str, mensaje: &str) -> Resultado<()> {
    con_ops(estado, "advertir_tarea_pendiente", |c, ops, autor| {
        tareas::advertir_pendiente(c, ops, autor, id, mensaje, &ahora_iso())
    })
}

/// Tareas en la Papelera ordenadas de la más reciente a la más antigua (#103).
#[tauri::command]
pub fn listar_papelera(estado: State<'_, AppState>) -> Resultado<Vec<Tarea>> {
    listar_papelera_en(&estado)
}

/// Lo mismo que [`listar_papelera`], sin el envoltorio de Tauri.
pub fn listar_papelera_en(estado: &AppState) -> Resultado<Vec<Tarea>> {
    let _yo = estado.exigir_permiso("listar_papelera")?;
    estado.con_db(|db| tareas::listar_papelera(db.conexion()))
}

/// `eliminar_tarea(id, justificacion)`: manda a Papelera con cascada sobre
/// subtareas activas, marca sus ramas en Unity VC y notifica a los creadores (#103).
#[tauri::command]
pub async fn eliminar_tarea(
    estado: State<'_, AppState>,
    id: String,
    justificacion: String,
) -> Resultado<Vec<Tarea>> {
    eliminar_tarea_en(&estado, &id, &justificacion)
}

/// Versión sin envoltorio Tauri de [`eliminar_tarea`] (para pruebas reales).
pub fn eliminar_tarea_en(
    estado: &AppState,
    id: &str,
    justificacion: &str,
) -> Resultado<Vec<Tarea>> {
    let yo = estado.exigir_permiso("eliminar_tarea")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    let afectadas = estado.con_db(|db| {
        tareas::eliminar(db.conexion(), &ops, &autor, id, justificacion, &ahora_iso())
    })?;
    if let Ok(repo) = estado.repo_abierto() {
        let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
        for t in &afectadas {
            if let Some(ref rama) = t.rama_asociada {
                let motivo = t
                    .justificacion_eliminacion
                    .as_deref()
                    .unwrap_or(justificacion);
                ramas::marcar_rama_eliminada(estado, &repo_cm, rama, motivo)?;
            }
        }
    }
    Ok(afectadas)
}

/// `eliminar_definitivamente(id)`: borra de SQLite una tarea en Papelera y sus
/// subtareas en Papelera, e intenta purgar sus ramas en Unity VC de hojas a raíz (#103).
#[tauri::command]
pub async fn eliminar_definitivamente(
    estado: State<'_, AppState>,
    id: String,
) -> Resultado<Vec<Tarea>> {
    eliminar_definitivamente_en(&estado, &id)
}

/// Versión sin envoltorio Tauri de [`eliminar_definitivamente`] (para pruebas reales).
pub fn eliminar_definitivamente_en(estado: &AppState, id: &str) -> Resultado<Vec<Tarea>> {
    let yo = estado.exigir_permiso("eliminar_definitivamente")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    let purgadas = estado.con_db(|db| {
        tareas::eliminar_definitivamente(db.conexion(), &ops, &autor, id, &ahora_iso())
    })?;
    if let Ok(repo) = estado.repo_abierto() {
        let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
        let workspace = estado.workspace_abierto().ok();
        for t in purgadas.iter().rev() {
            if let Some(ref rama) = t.rama_asociada {
                let _ =
                    ramas::purgar_rama_definitiva(estado, workspace.as_deref(), &repo_cm, rama)?;
            }
        }
    }
    Ok(purgadas)
}

/// `asignar_revision(id, email)`: elige el revisor de una tarea en revisión (#107).
/// Líder/Admin mientras no tenga revisor; solo el Administrador si ya tiene.
#[tauri::command]
pub async fn asignar_revision(
    estado: State<'_, AppState>,
    id: String,
    email: String,
) -> Resultado<Tarea> {
    asignar_revision_en(&estado, &id, &email)
}

/// Lo mismo que [`asignar_revision`], sin el envoltorio de Tauri.
pub fn asignar_revision_en(estado: &AppState, id: &str, email: &str) -> Resultado<Tarea> {
    con_ops(estado, "asignar_revision", |c, ops, autor| {
        tareas::asignar_revision(c, ops, autor, id, email, &ahora_iso())
    })
}

/// `autorizar_merge(id)`: merge del lado del servidor (API REST, `mergeto`) de la
/// rama de la tarea hacia `/main/dev` (tarea general) o hacia la rama de su tarea
/// padre (subtarea), sin tocar el workspace de quien autoriza (#107).
///
/// Primero valida en SQLite (revisión, subtareas terminadas, sin préstamos), después
/// pide el reporte (`mergeto/report`): si hay archivos en conflicto no se intenta el
/// merge y la tarea no cambia (`conflicto` en la respuesta). Si el servidor lo
/// hace (`OK`, o `MergeNotNeeded` porque ya se conectó a mano), la tarea pasa a
/// `terminada` y el evento `autorizar_merge` viaja por `ops/`.
#[tauri::command]
pub async fn autorizar_merge(
    estado: State<'_, AppState>,
    id: String,
) -> Resultado<ResultadoAutorizacion> {
    autorizar_merge_en(&estado, &id)
}

/// Lo mismo que [`autorizar_merge`], sin el envoltorio de Tauri (para pruebas en
/// el entorno real).
pub fn autorizar_merge_en(estado: &AppState, id: &str) -> Resultado<ResultadoAutorizacion> {
    let yo = estado.exigir_permiso("autorizar_merge")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let repo = estado.repo_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let plan = estado.con_db(|db| tareas::plan_de_autorizacion(db.conexion(), &autor, id))?;

    let _guarda = estado.iniciar_operacion();
    let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
    let reporte = cliente.reporte_merge(&repo.repo, &plan.origen, &plan.destino)?;
    let archivos = reporte.archivos_en_conflicto();
    if !archivos.is_empty() {
        return Ok(ResultadoAutorizacion {
            tareas: estado.con_db(|db| tareas::listar(db.conexion()))?,
            conflicto: Some(ConflictoMerge {
                origen: plan.origen,
                destino: plan.destino,
                archivos,
                mensaje: "Unity VC no puede unir estos archivos solo. Resuélvelo desde un workspace (Merge from) y vuelve a autorizar, o rechaza la revisión para que lo corrija el responsable".into(),
            }),
        });
    }

    let changeset = if reporte.nada_que_mergear() {
        None
    } else {
        let comentario = format!(
            "{}: {} (merge autorizado desde BranchBoard)",
            plan.tarea.id, plan.tarea.titulo
        );
        let resultado = cliente.merge_a(&repo.repo, &plan.origen, &plan.destino, &comentario)?;
        if resultado.hubo_conflicto() {
            return Ok(ResultadoAutorizacion {
                tareas: estado.con_db(|db| tareas::listar(db.conexion()))?,
                conflicto: Some(ConflictoMerge {
                    origen: plan.origen,
                    destino: plan.destino,
                    archivos: Vec::new(),
                    mensaje: resultado.message.unwrap_or_else(|| {
                        "El servidor de Unity VC no pudo hacer el merge por conflictos".into()
                    }),
                }),
            });
        }
        if resultado.nada_que_mergear() {
            None
        } else if resultado.fue_ok() {
            Some(resultado.changeset_number)
        } else {
            return Err(Error::ServidorUnityVc(format!(
                "El merge respondió '{}'",
                resultado.status
            )));
        }
    };

    let tareas = estado.con_db(|db| {
        tareas::registrar_merge_autorizado(
            db.conexion(),
            &ops,
            &autor,
            id,
            &plan.destino,
            changeset,
            &ahora_iso(),
        )
    })?;
    Ok(ResultadoAutorizacion {
        tareas,
        conflicto: None,
    })
}

/// `rechazar_revision(id, motivo)`: la tarea y sus subtareas vuelven a producción,
/// queda `motivo_rechazo` y se notifica al responsable (#107).
#[tauri::command]
pub async fn rechazar_revision(
    estado: State<'_, AppState>,
    id: String,
    motivo: String,
) -> Resultado<Vec<Tarea>> {
    rechazar_revision_en(&estado, &id, &motivo)
}

/// Lo mismo que [`rechazar_revision`], sin el envoltorio de Tauri.
pub fn rechazar_revision_en(estado: &AppState, id: &str, motivo: &str) -> Resultado<Vec<Tarea>> {
    con_ops(estado, "rechazar_revision", |c, ops, autor| {
        tareas::rechazar_revision(c, ops, autor, id, motivo, &ahora_iso())
    })
}
