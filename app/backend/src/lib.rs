//! Backend de BranchBoard (Tauri). Diseño completo en `documentacion/BACKEND.md`.
//!
//! Organización:
//! - `comandos/` — lo que el frontend llama con `invoke`, por área.
//! - `db` — SQLite local.
//! - `cm` — Unity Version Control (subproceso `cm`).
//! - `eventos` — sincronización por `ops/`.
//! - `identidad` — quién es el usuario, firmas y permisos.
//! - `error` — error común `{ codigo, mensaje }`.
//! - `estado` — estado compartido de la app.

pub mod cm;
pub mod comandos;
pub mod db;
pub mod error;
pub mod estado;
pub mod eventos;
pub mod identidad;

use comandos::{actualizaciones, sistema};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(estado::AppState::default())
        .invoke_handler(tauri::generate_handler![
            sistema::ping,
            sistema::hay_operacion_en_curso,
            actualizaciones::buscar_actualizacion,
            actualizaciones::instalar_actualizacion,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar BranchBoard");
}
