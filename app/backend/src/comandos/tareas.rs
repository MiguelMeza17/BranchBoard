//! Comandos Tauri de tareas (issues #95, #96): `listar_tareas`, `crear_tarea`,
//! `crear_tarea_con_subtareas`, `editar_tarea` y `tomar_tarea`.

use std::path::Path;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::commits::{CambioPrestado, Commit};
use crate::db::ediciones::EdicionPendiente;
use crate::db::tareas::Tarea;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, ahora_local_segundo, hoy_local, EstadoTarea, TipoTarea};
use crate::ramas;
use crate::servidor::{conectar, LlaveroSistema};
use crate::tareas::{
    self, AutorTarea, CambiosTarea, ConflictoMerge, DatosCheckin, DatosNuevaTarea,
    DatosNuevaTareaConSubtareas, DatosRamaDesdeEspera, DatosTraerCambios, PendientesRama,
    ResultadoAutorizacion, ResultadoCambioEstado, ResultadoProponerEdicion,
};
use crate::unity::{self, ResultadoAbrirUnity};

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
    let diffs_checkin = if let Ok(repo) = estado.repo_abierto() {
        let hay_prestamos = estado
            .con_db(|db| {
                Ok(!crate::db::commits::prestamos_vigentes(db.conexion(), &tarea.id)?.is_empty())
            })
            .unwrap_or(false);
        if hay_prestamos {
            let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
            ramas::archivos_changeset(estado, Some(&workspace), &repo_cm, cs).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    estado.con_db(|db| {
        let commit = tareas::registrar_checkin(
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
        )?;
        if !diffs_checkin.is_empty() {
            tareas::actualizar_tocados_de_prestamos(
                db.conexion(),
                &tarea.id,
                Some(&[]),
                &[(cs, diffs_checkin)],
            )?;
        }
        Ok(commit)
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

/// Lista los cambios prestados de `tarea_id`, o todos los del proyecto si es `None` (#108).
#[tauri::command]
pub fn listar_cambios_prestados(
    estado: State<'_, AppState>,
    tarea_id: Option<String>,
) -> Resultado<Vec<CambioPrestado>> {
    listar_cambios_prestados_en(&estado, tarea_id.as_deref())
}

/// Versión sin envoltorio Tauri de [`listar_cambios_prestados`].
pub fn listar_cambios_prestados_en(
    estado: &AppState,
    tarea_id: Option<&str>,
) -> Resultado<Vec<CambioPrestado>> {
    let _yo = estado.exigir_permiso("listar_cambios_prestados")?;
    estado.con_db(|db| tareas::listar_cambios_prestados(db.conexion(), tarea_id))
}

/// `traer_cambios(destino_id, origen_id, hash?)`: trae trabajo prestado desde `origen_id`
/// (`None` = todos sus commits, `Some(cs)` = puntual) mediante `cm merge --cherrypicking`,
/// guarda el changeset del merge y los archivos que llegaron y lo publica en `ops/` (#108).
#[tauri::command]
pub async fn traer_cambios(
    estado: State<'_, AppState>,
    destino_id: String,
    origen_id: String,
    hash: Option<String>,
) -> Resultado<CambioPrestado> {
    traer_cambios_en(&estado, &destino_id, &origen_id, hash.as_deref())
}

/// Versión sin envoltorio Tauri de [`traer_cambios`].
pub fn traer_cambios_en(
    estado: &AppState,
    destino_id: &str,
    origen_id: &str,
    hash: Option<&str>,
) -> Resultado<CambioPrestado> {
    let yo = estado.exigir_permiso("traer_cambios")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let repo = estado.repo_abierto()?;
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let (info, cs_puntual) = estado.con_db(|db| {
        tareas::preparar_traer_cambios(db.conexion(), &autor, destino_id, origen_id, hash)
    })?;

    let comentario = match cs_puntual {
        Some(cs) => format!(
            "Préstamo de {} (cs:{cs}) hacia {}",
            info.origen_id, info.destino_id
        ),
        None => format!(
            "Préstamo de {} (todos) hacia {}",
            info.origen_id, info.destino_id
        ),
    };
    let resultado = ramas::traer_cambios_workspace(
        estado,
        &workspace,
        &repo_cm,
        &info.rama_destino,
        &info.rama_origen,
        cs_puntual,
        &comentario,
    )?;

    let hash_str = cs_puntual.map(|n| n.to_string());
    estado.con_db(|db| {
        tareas::registrar_traer_cambios(
            db.conexion(),
            &ops,
            &autor,
            &DatosTraerCambios {
                destino_id: &info.destino_id,
                origen_id: &info.origen_id,
                hash: hash_str.as_deref(),
                changeset_merge: resultado.changeset,
                archivos: resultado.archivos,
            },
            &ahora_iso(),
        )
    })
}

/// `devolver_cambios_prestados(prestamo_id)`: ejecuta un merge sustractivo en Unity VC
/// sobre el `changeset_merge` guardado y marca el préstamo como devuelto (conservándolo
/// como historial en `cambios_prestados`, #108).
#[tauri::command]
pub async fn devolver_cambios_prestados(
    estado: State<'_, AppState>,
    prestamo_id: String,
) -> Resultado<CambioPrestado> {
    devolver_cambios_prestados_en(&estado, &prestamo_id)
}

/// Versión sin envoltorio Tauri de [`devolver_cambios_prestados`].
pub fn devolver_cambios_prestados_en(
    estado: &AppState,
    prestamo_id: &str,
) -> Resultado<CambioPrestado> {
    let yo = estado.exigir_permiso("devolver_cambios_prestados")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let (prestamo, rama_destino, cs_merge) = estado
        .con_db(|db| tareas::preparar_devolver_cambios(db.conexion(), &autor, prestamo_id))?;

    let comentario = format!(
        "Devuelve préstamo {} de {} (cs:{cs_merge})",
        prestamo.id, prestamo.origen_id
    );
    ramas::devolver_cambios_workspace(estado, &workspace, &rama_destino, cs_merge, &comentario)?;

    estado.con_db(|db| {
        tareas::registrar_devolver_cambios(db.conexion(), &ops, &autor, &prestamo.id, &ahora_iso())
    })
}

/// `eliminar_cambios_ajenos(prestamo_id)`: restaura los archivos prestados modificados
/// a la revisión que llegó en `changeset_merge`, crea un check-in automático si modificó
/// el workspace respecto a la cabeza y publica `eliminar_cambios_ajenos` en `ops/` (#109).
#[tauri::command]
pub async fn eliminar_cambios_ajenos(
    estado: State<'_, AppState>,
    prestamo_id: String,
) -> Resultado<CambioPrestado> {
    eliminar_cambios_ajenos_en(&estado, &prestamo_id)
}

/// Versión sin envoltorio Tauri de [`eliminar_cambios_ajenos`].
pub fn eliminar_cambios_ajenos_en(
    estado: &AppState,
    prestamo_id: &str,
) -> Resultado<CambioPrestado> {
    let yo = estado.exigir_permiso("eliminar_cambios_ajenos")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let repo = estado.repo_abierto()?;
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };

    let (prestamo, rama_destino, cs_merge) = estado.con_db(|db| {
        tareas::preparar_eliminar_cambios_ajenos(db.conexion(), &autor, prestamo_id)
    })?;

    let comentario = format!(
        "Se eliminaron cambios ajenos (prestados de {})",
        prestamo.origen_id
    );
    let cs_limpio = ramas::eliminar_cambios_ajenos_workspace(
        estado,
        &workspace,
        &repo_cm,
        &rama_destino,
        cs_merge,
        &prestamo.archivos_tocados,
        &comentario,
    )?;

    estado.con_db(|db| {
        tareas::registrar_eliminar_cambios_ajenos(
            db.conexion(),
            &ops,
            &autor,
            &prestamo.id,
            cs_limpio,
            &ahora_iso(),
        )
    })
}

/// Lista los check-ins cacheados en SQLite de una tarea o de todo el proyecto (#112).
#[tauri::command]
pub fn listar_commits(
    estado: State<'_, AppState>,
    tarea_id: Option<String>,
) -> Resultado<Vec<Commit>> {
    listar_commits_en(&estado, tarea_id.as_deref())
}

/// Versión sin envoltorio Tauri de [`listar_commits`].
pub fn listar_commits_en(estado: &AppState, tarea_id: Option<&str>) -> Resultado<Vec<Commit>> {
    estado.exigir_permiso("listar_commits")?;
    estado.con_db(
        |db| match tarea_id.map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => crate::db::commits::de_tarea(db.conexion(), id),
            None => crate::db::commits::listar_todos(db.conexion()),
        },
    )
}

/// Versión sin envoltorio Tauri que cambia de rama/changeset y ubica Unity sin abrir GUI (#112).
pub fn abrir_en_unity_preparar_en(
    estado: &AppState,
    tarea_id: &str,
    rama: &str,
    changeset: Option<&str>,
) -> Resultado<ResultadoAbrirUnity> {
    let _yo = estado.exigir_permiso("abrir_en_unity")?;
    let workspace = estado.workspace_abierto()?;
    let (lanzador, estado_ws) = estado
        .con_db(|db| unity::preparar_apertura(estado, db, &workspace, tarea_id, rama, changeset))?;
    Ok(ResultadoAbrirUnity {
        rama_actual: estado_ws.rama,
        changeset: estado_ws.changeset,
        ejecutable: lanzador.ejecutable.to_string_lossy().to_string(),
        usa_hub: lanzador.es_hub,
    })
}

/// Versión sin envoltorio Tauri completa de [`abrir_en_unity`] (#112).
pub fn abrir_en_unity_en(
    estado: &AppState,
    tarea_id: &str,
    rama: &str,
    changeset: Option<&str>,
) -> Resultado<ResultadoAbrirUnity> {
    let _yo = estado.exigir_permiso("abrir_en_unity")?;
    let workspace = estado.workspace_abierto()?;
    let (lanzador, estado_ws) = estado
        .con_db(|db| unity::preparar_apertura(estado, db, &workspace, tarea_id, rama, changeset))?;
    unity::lanzar_unity(&lanzador, &workspace)?;
    Ok(ResultadoAbrirUnity {
        rama_actual: estado_ws.rama,
        changeset: estado_ws.changeset,
        ejecutable: lanzador.ejecutable.to_string_lossy().to_string(),
        usa_hub: lanzador.es_hub,
    })
}

/// Cambia el workspace a la rama/changeset de la tarea y abre Unity Editor o Unity Hub (#112).
#[tauri::command]
pub async fn abrir_en_unity(
    estado: State<'_, AppState>,
    tarea_id: String,
    rama: String,
    changeset: Option<String>,
) -> Resultado<ResultadoAbrirUnity> {
    abrir_en_unity_en(&estado, &tarea_id, &rama, changeset.as_deref())
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
