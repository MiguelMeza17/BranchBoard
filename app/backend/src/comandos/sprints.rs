//! Comandos Tauri de planificación: sprints y backlogs (issue #113).

use tauri::State;

use crate::avance::{self, AvanceBacklog, AvanceSprint};
use crate::db::sprints::Sprint;
use crate::db::tareas::Tarea;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::modelo::{ahora_iso, hoy_local};
use crate::sprints;
use crate::tareas::AutorTarea;

/// Lista todos los sprints y backlogs del proyecto, evaluando primero si algún
/// sprint alcanzó su fecha de fin (#113, #114).
#[tauri::command]
pub fn listar_sprints(estado: State<'_, AppState>) -> Resultado<Vec<Sprint>> {
    listar_sprints_en(&estado)
}

/// Evalúa los sprints vencidos del proyecto abierto (#114) y publica lo que corresponda.
pub(crate) fn evaluar_cierres_en(estado: &AppState, email: &str) -> Resultado<Vec<String>> {
    estado.con_db(|db| {
        sprints::evaluar_cierre_sprints(db.conexion(), email, &ahora_iso(), &hoy_local())
    })
}

/// Versión sin envoltorio Tauri de [`listar_sprints`].
pub fn listar_sprints_en(estado: &AppState) -> Resultado<Vec<Sprint>> {
    let yo = estado.exigir_permiso("listar_sprints")?;
    let _ = evaluar_cierres_en(estado, &yo.email)?;
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
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::crear_sprint(
            db.conexion(),
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
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::asignar_tarea_a_sprint(db.conexion(), &autor, tarea_id, sprint_id, &ahora_iso())
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
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::asignar_tarea_a_backlog(db.conexion(), &autor, tarea_id, backlog_id, &ahora_iso())
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
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::asignar_backlog_a_sprint(
            db.conexion(),
            &autor,
            backlog_id,
            sprint_id,
            &ahora_iso(),
        )
    })
}

/// Cierra manualmente un sprint o backlog (exclusivo Administrador, incluso antes de `fecha_fin`, #114).
#[tauri::command]
pub fn cerrar_sprint(estado: State<'_, AppState>, sprint_id: String) -> Resultado<Sprint> {
    cerrar_sprint_en(&estado, &sprint_id)
}

/// Versión sin envoltorio Tauri de [`cerrar_sprint`].
pub fn cerrar_sprint_en(estado: &AppState, sprint_id: &str) -> Resultado<Sprint> {
    let yo = estado.exigir_permiso("cerrar_sprint")?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    estado.con_db(|db| {
        sprints::cerrar_sprint(db.conexion(), &autor, sprint_id, &ahora_iso(), &hoy_local())
    })
}

fn calcular_avance<T>(
    estado: &AppState,
    permiso: &str,
    calcular: impl FnOnce(&rusqlite::Connection, &str) -> Resultado<T>,
) -> Resultado<T> {
    let yo = estado.exigir_permiso(permiso)?;
    let _ = evaluar_cierres_en(estado, &yo.email)?;
    let hoy = hoy_local();
    estado.con_db(|db| calcular(db.conexion(), &hoy))
}

/// Datos para la gráfica de Avance de un sprint (`con_fecha = true`, #115).
#[tauri::command]
pub fn datos_avance_sprint(
    estado: State<'_, AppState>,
    sprint_id: String,
) -> Resultado<AvanceSprint> {
    datos_avance_sprint_en(&estado, &sprint_id)
}

/// Versión sin envoltorio Tauri de [`datos_avance_sprint`].
pub fn datos_avance_sprint_en(estado: &AppState, sprint_id: &str) -> Resultado<AvanceSprint> {
    calcular_avance(estado, "datos_avance_sprint", |c, hoy| {
        avance::datos_sprint(c, sprint_id, hoy)
    })
}

/// Datos para la gráfica de Avance de un backlog (`con_fecha = false`, #115).
#[tauri::command]
pub fn datos_avance_backlog(
    estado: State<'_, AppState>,
    backlog_id: String,
) -> Resultado<AvanceBacklog> {
    datos_avance_backlog_en(&estado, &backlog_id)
}

/// Versión sin envoltorio Tauri de [`datos_avance_backlog`].
pub fn datos_avance_backlog_en(estado: &AppState, backlog_id: &str) -> Resultado<AvanceBacklog> {
    calcular_avance(estado, "datos_avance_backlog", |c, hoy| {
        avance::datos_backlog(c, backlog_id, hoy)
    })
}

/// Datos para la gráfica General de Avance (todas las tareas en algún sprint o backlog, #115).
#[tauri::command]
pub fn datos_avance_general(estado: State<'_, AppState>) -> Resultado<AvanceBacklog> {
    datos_avance_general_en(&estado)
}

/// Versión sin envoltorio Tauri de [`datos_avance_general`].
pub fn datos_avance_general_en(estado: &AppState) -> Resultado<AvanceBacklog> {
    calcular_avance(estado, "datos_avance_general", |c, hoy| {
        avance::datos_general(c, hoy)
    })
}
