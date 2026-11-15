//! Comandos Tauri de tareas (issue #95): `listar_tareas`, `crear_tarea`,
//! `crear_tarea_con_subtareas` y `editar_tarea`.

use std::path::Path;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::tareas::Tarea;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, hoy_local};
use crate::tareas::{self, AutorTarea, CambiosTarea, DatosNuevaTarea, DatosNuevaTareaConSubtareas};

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
