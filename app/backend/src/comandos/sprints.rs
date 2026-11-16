//! Comandos Tauri de planificación: sprints y backlogs (issue #113).

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::sprints::Sprint;
use crate::db::tareas::Tarea;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, hoy_local};
use crate::sprints;
use crate::tareas::AutorTarea;

/// Lista todos los sprints y backlogs del proyecto (#113).
#[tauri::command]
pub fn listar_sprints(estado: State<'_, AppState>) -> Resultado<Vec<Sprint>> {
    listar_sprints_en(&estado)
}

/// Versión sin envoltorio Tauri de [`listar_sprints`].
pub fn listar_sprints_en(estado: &AppState) -> Resultado<Vec<Sprint>> {
    let _yo = estado.exigir_permiso("listar_sprints")?;
    estado.con_db(|db| sprints::listar(db.conexion()))
}

/// Crea un sprint (`con_fecha = true`, requiere `duracion_dias`) o backlog
/// (`con_fecha = false`), exclusivo Líder/Administrador (#113).
#[tauri::command]
pub fn crear_sprint(
    estado: State<'_, AppState>,
    nombre: String,
    descripcion: String,
    con_fecha: bool,
    duracion_dias: Option<u32>,
) -> Resultado<Sprint> {
    crear_sprint_en(&estado, &nombre, &descripcion, con_fecha, duracion_dias)
}

/// Versión sin envoltorio Tauri de [`crear_sprint`].
pub fn crear_sprint_en(
    estado: &AppState,
    nombre: &str,
    descripcion: &str,
    con_fecha: bool,
    duracion_dias: Option<u32>,
) -> Resultado<Sprint> {
    let yo = estado.exigir_permiso("crear_sprint")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::crear_sprint(
            db.conexion(),
            &ops,
            &autor,
            &sprints::DatosNuevoSprint {
                nombre,
                descripcion,
                con_fecha,
                duracion_dias,
            },
            &ahora_iso(),
            &hoy_local(),
        )
    })
}

/// Asigna o quita una tarea de un sprint (`sprint_id = None` para quitarla, #113).
#[tauri::command]
pub fn asignar_tarea_a_sprint(
    estado: State<'_, AppState>,
    tarea_id: String,
    sprint_id: Option<String>,
) -> Resultado<Tarea> {
    asignar_tarea_a_sprint_en(&estado, &tarea_id, sprint_id.as_deref())
}

/// Versión sin envoltorio Tauri de [`asignar_tarea_a_sprint`].
pub fn asignar_tarea_a_sprint_en(
    estado: &AppState,
    tarea_id: &str,
    sprint_id: Option<&str>,
) -> Resultado<Tarea> {
    let yo = estado.exigir_permiso("asignar_tarea_a_sprint")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::asignar_tarea_a_sprint(
            db.conexion(),
            &ops,
            &autor,
            tarea_id,
            sprint_id,
            &ahora_iso(),
        )
    })
}

/// Asigna o quita una tarea de un backlog (`backlog_id = None` para quitarla, #113).
#[tauri::command]
pub fn asignar_tarea_a_backlog(
    estado: State<'_, AppState>,
    tarea_id: String,
    backlog_id: Option<String>,
) -> Resultado<Tarea> {
    asignar_tarea_a_backlog_en(&estado, &tarea_id, backlog_id.as_deref())
}

/// Versión sin envoltorio Tauri de [`asignar_tarea_a_backlog`].
pub fn asignar_tarea_a_backlog_en(
    estado: &AppState,
    tarea_id: &str,
    backlog_id: Option<&str>,
) -> Resultado<Tarea> {
    let yo = estado.exigir_permiso("asignar_tarea_a_backlog")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::asignar_tarea_a_backlog(
            db.conexion(),
            &ops,
            &autor,
            tarea_id,
            backlog_id,
            &ahora_iso(),
        )
    })
}

/// Mueve al sprint todas las tareas libres del backlog (`backlog_id = None` toma las sin backlog, #113).
#[tauri::command]
pub fn asignar_backlog_a_sprint(
    estado: State<'_, AppState>,
    backlog_id: Option<String>,
    sprint_id: String,
) -> Resultado<Vec<Tarea>> {
    asignar_backlog_a_sprint_en(&estado, backlog_id.as_deref(), &sprint_id)
}

/// Versión sin envoltorio Tauri de [`asignar_backlog_a_sprint`].
pub fn asignar_backlog_a_sprint_en(
    estado: &AppState,
    backlog_id: Option<&str>,
    sprint_id: &str,
) -> Resultado<Vec<Tarea>> {
    let yo = estado.exigir_permiso("asignar_backlog_a_sprint")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::asignar_backlog_a_sprint(
            db.conexion(),
            &ops,
            &autor,
            backlog_id,
            sprint_id,
            &ahora_iso(),
        )
    })
}
