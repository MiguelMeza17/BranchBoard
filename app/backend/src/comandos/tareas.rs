//! Comandos Tauri de tareas (issues #95, #96): `listar_tareas`, `crear_tarea`,
//! `crear_tarea_con_subtareas`, `editar_tarea` y `tomar_tarea`.

use std::path::Path;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::tareas::Tarea;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, hoy_local};
use crate::ramas;
use crate::servidor::{conectar, LlaveroSistema};
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
