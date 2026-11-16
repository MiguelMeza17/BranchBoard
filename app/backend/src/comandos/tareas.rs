//! Comandos Tauri de tareas (issues #95, #96): `listar_tareas`, `crear_tarea`,
//! `crear_tarea_con_subtareas`, `editar_tarea` y `tomar_tarea`.

use std::path::Path;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::commits::Commit;
use crate::db::ediciones::EdicionPendiente;
use crate::db::tareas::Tarea;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, ahora_local_segundo, hoy_local, EstadoTarea, TipoTarea};
use crate::ramas;
use crate::servidor::{conectar, LlaveroSistema};
use crate::tareas::{
    self, AutorTarea, CambiosTarea, DatosCheckin, DatosNuevaTarea, DatosNuevaTareaConSubtareas,
    DatosRamaDesdeEspera, PendientesRama, ResultadoCambioEstado, ResultadoProponerEdicion,
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

/// Lista las ediciones pendientes de aprobación (#102).
#[tauri::command]
pub fn listar_ediciones_pendientes(
    estado: State<'_, AppState>,
) -> Resultado<Vec<EdicionPendiente>> {
    listar_ediciones_pendientes_en(&estado)
}

/// Lo mismo que [`listar_ediciones_pendientes`], sin el envoltorio de Tauri.
pub fn listar_ediciones_pendientes_en(estado: &AppState) -> Resultado<Vec<EdicionPendiente>> {
    let _yo = estado.exigir_permiso("listar_ediciones_pendientes")?;
    estado.con_db(|db| tareas::listar_ediciones_pendientes(db.conexion()))
}

/// `proponer_edicion(id, titulo, detalles, motivo?)`: cualquier rol edita título/detalles en `'por hacer'` (#102).
#[tauri::command]
pub async fn proponer_edicion(
    estado: State<'_, AppState>,
    id: String,
    titulo: String,
    detalles: String,
    motivo: Option<String>,
) -> Resultado<ResultadoProponerEdicion> {
    proponer_edicion_en(&estado, &id, &titulo, &detalles, motivo.as_deref())
}

/// Lo mismo que [`proponer_edicion`], sin el envoltorio de Tauri.
pub fn proponer_edicion_en(
    estado: &AppState,
    id: &str,
    titulo: &str,
    detalles: &str,
    motivo: Option<&str>,
) -> Resultado<ResultadoProponerEdicion> {
    con_ops(estado, "proponer_edicion", |c, ops, autor| {
        tareas::proponer_edicion(
            c,
            ops,
            autor,
            id,
            titulo,
            detalles,
            motivo,
            &ahora_iso(),
            &hoy_local(),
        )
    })
}

/// `aprobar_modificacion(id)`: Líder/Admin confirma la edición pendiente (#102).
#[tauri::command]
pub async fn aprobar_modificacion(estado: State<'_, AppState>, id: String) -> Resultado<Tarea> {
    aprobar_modificacion_en(&estado, &id)
}

/// Lo mismo que [`aprobar_modificacion`], sin el envoltorio de Tauri.
pub fn aprobar_modificacion_en(estado: &AppState, id: &str) -> Resultado<Tarea> {
    con_ops(estado, "aprobar_modificacion", |c, ops, autor| {
        tareas::aprobar_modificacion(c, ops, autor, id, &ahora_iso())
    })
}

/// `aprobar_modificaciones(ids)`: Líder/Admin confirma varias ediciones pendientes a la vez (#102).
#[tauri::command]
pub async fn aprobar_modificaciones(
    estado: State<'_, AppState>,
    ids: Vec<String>,
) -> Resultado<Vec<Tarea>> {
    aprobar_modificaciones_en(&estado, &ids)
}

/// Lo mismo que [`aprobar_modificaciones`], sin el envoltorio de Tauri.
pub fn aprobar_modificaciones_en(estado: &AppState, ids: &[String]) -> Resultado<Vec<Tarea>> {
    con_ops(estado, "aprobar_modificaciones", |c, ops, autor| {
        tareas::aprobar_modificaciones(c, ops, autor, ids, &ahora_iso())
    })
}

/// `rechazar_modificacion(id)`: Líder/Admin rechaza la edición pendiente y restaura los valores anteriores (#102).
#[tauri::command]
pub async fn rechazar_modificacion(estado: State<'_, AppState>, id: String) -> Resultado<Tarea> {
    rechazar_modificacion_en(&estado, &id)
}

/// Lo mismo que [`rechazar_modificacion`], sin el envoltorio de Tauri.
pub fn rechazar_modificacion_en(estado: &AppState, id: &str) -> Resultado<Tarea> {
    con_ops(estado, "rechazar_modificacion", |c, ops, autor| {
        tareas::rechazar_modificacion(c, ops, autor, id, &ahora_iso())
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

/// Lista los cambios sin check-in del workspace (`cm status`) indicando si el workspace
/// apunta a `rama` (guardia de rama, #105).
#[tauri::command]
pub fn cambios_pendientes(
    estado: State<'_, AppState>,
    rama: Option<String>,
) -> Resultado<PendientesRama> {
    cambios_pendientes_en(&estado, rama.as_deref())
}

/// Versión sin envoltorio Tauri de [`cambios_pendientes`].
pub fn cambios_pendientes_en(estado: &AppState, rama: Option<&str>) -> Resultado<PendientesRama> {
    let _yo = estado.exigir_permiso("cambios_pendientes")?;
    let workspace = estado.workspace_abierto()?;
    let actual = crate::cm::lectura::rama_actual_workspace(estado, &workspace)?;
    let en_rama_esperada = rama.map_or(true, |r| {
        actual
            .rama
            .eq_ignore_ascii_case(r.trim().trim_end_matches('/'))
    });
    let archivos = if en_rama_esperada {
        ramas::cambios_pendientes(estado, &workspace)?
    } else {
        Vec::new()
    };
    Ok(PendientesRama {
        rama_actual: actual.rama,
        en_rama_esperada,
        archivos,
    })
}

/// Cambia el workspace local hacia la rama asociada de `id` si no hay cambios sin check-in (#105).
#[tauri::command]
pub async fn cambiar_a_rama_tarea(estado: State<'_, AppState>, id: String) -> Resultado<String> {
    cambiar_a_rama_tarea_en(&estado, &id)
}

/// Versión sin envoltorio Tauri de [`cambiar_a_rama_tarea`].
pub fn cambiar_a_rama_tarea_en(estado: &AppState, id: &str) -> Resultado<String> {
    let _yo = estado.exigir_permiso("cambiar_a_rama_tarea")?;
    let workspace = estado.workspace_abierto()?;
    let rama = estado.con_db(|db| tareas::rama_para_cambiar(db.conexion(), id))?;
    let pendientes = ramas::cambios_pendientes(estado, &workspace)?;
    if !pendientes.is_empty() {
        return Err(Error::Validacion(format!(
            "Tienes {} cambio(s) sin check-in en esta rama. Haz check-in o deshazlos antes de cambiar de rama",
            pendientes.len()
        )));
    }
    ramas::cambiar_a_rama(estado, &workspace, &rama)?;
    Ok(rama)
}

/// `hacer_checkin(rama, titulo, descripcion, tipo)`: ejecuta `cm checkin` en la rama
/// de una tarea sin subtareas (con guardia de rama actual y responsable) y guarda el
/// commit con `tipo` en SQLite + `ops/` (#105).
#[tauri::command]
pub async fn hacer_checkin(
    estado: State<'_, AppState>,
    rama: String,
    titulo: String,
    descripcion: String,
    tipo: TipoTarea,
) -> Resultado<Commit> {
    hacer_checkin_en(&estado, &rama, &titulo, &descripcion, tipo)
}

/// Versión sin envoltorio Tauri de [`hacer_checkin`].
pub fn hacer_checkin_en(
    estado: &AppState,
    rama: &str,
    titulo: &str,
    descripcion: &str,
    tipo: TipoTarea,
) -> Resultado<Commit> {
    let yo = estado.exigir_permiso("hacer_checkin")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let tarea = estado.con_db(|db| tareas::tarea_para_checkin(db.conexion(), &yo.email, rama))?;
    let rama_tarea = tarea
        .rama_asociada
        .as_deref()
        .ok_or_else(|| Error::Validacion("La tarea no tiene rama asociada".into()))?;
    let comentario = tareas::armar_comentario_checkin(titulo, descripcion, tipo)?;

    let cs = ramas::hacer_checkin_workspace(estado, &workspace, rama_tarea, &comentario)?;
    estado.con_db(|db| {
        tareas::registrar_checkin(
            db.conexion(),
            &ops,
            &autor,
            &DatosCheckin {
                tarea_id: &tarea.id,
                changeset: cs,
                titulo,
                descripcion,
                tipo,
            },
            &ahora_iso(),
            &ahora_local_segundo(),
        )
    })
}

/// `crear_rama_desde_espera(espera_id, titulo, detalles, tipo)`: convierte una
/// subtarea `"En espera"` en subtarea normal, crea su rama desde la cabeza del padre
/// y mueve el workspace + los cambios pendientes hacia ella con `cm switch --noinput` (#106).
#[tauri::command]
pub async fn crear_rama_desde_espera(
    estado: State<'_, AppState>,
    espera_id: String,
    titulo: String,
    detalles: String,
    tipo: TipoTarea,
) -> Resultado<Tarea> {
    crear_rama_desde_espera_en(&estado, &espera_id, &titulo, &detalles, tipo)
}

/// Versión sin envoltorio Tauri de [`crear_rama_desde_espera`].
pub fn crear_rama_desde_espera_en(
    estado: &AppState,
    espera_id: &str,
    titulo: &str,
    detalles: &str,
    tipo: TipoTarea,
) -> Resultado<Tarea> {
    let yo = estado.exigir_permiso("crear_rama_desde_espera")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let repo = estado.repo_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let (espera, rama_padre, asignado) = estado.con_db(|db| {
        tareas::preparar_rama_desde_espera(
            db.conexion(),
            &yo.email,
            yo.rol,
            espera_id,
            titulo,
            tipo,
        )
    })?;

    let actual = crate::cm::lectura::rama_actual_workspace(estado, &workspace)?;
    if !actual.rama.eq_ignore_ascii_case(&rama_padre) {
        return Err(Error::Validacion(format!(
            "Tu workspace está en la rama {}, pero los cambios en espera son de la rama {rama_padre} — cambia de rama antes de continuar",
            actual.rama
        )));
    }

    let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
    let rama_hija = ramas::crear_rama_tarea(
        &cliente,
        &repo.repo,
        Some(&rama_padre),
        &espera.id,
        titulo.trim(),
    )?;

    ramas::cambiar_a_rama_llevando_cambios(estado, &workspace, &rama_hija.name)?;

    estado.con_db(|db| {
        tareas::convertir_espera_en_subtarea(
            db.conexion(),
            &ops,
            &autor,
            &DatosRamaDesdeEspera {
                espera_id: &espera.id,
                titulo,
                detalles,
                tipo,
                asignado_a: &asignado,
                rama: &rama_hija.name,
                ultimo_changeset_visto: rama_hija.head_changeset,
            },
            &ahora_iso(),
        )
    })
}
