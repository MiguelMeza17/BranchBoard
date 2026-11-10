//! Backend de BranchBoard (Tauri). Diseño completo en `documentacion/BACKEND.md`.
//!
//! Organización:
//! - `comandos/` — lo que el frontend llama con `invoke`, por área.
//! - `db` — SQLite local.
//! - `cm` — Unity Version Control (subproceso `cm`).
//! - `eventos` — sincronización por `ops/`.
//! - `identidad` — quién es el usuario, firmas y permisos.
//! - `modelo` — tipos del dominio (roles, estados…), iguales a los del frontend.
//! - `error` — error común `{ codigo, mensaje }`.
//! - `estado` — estado compartido de la app.

pub mod cm;
pub mod comandos;
pub mod db;
pub mod error;
pub mod estado;
pub mod eventos;
pub mod identidad;
pub mod modelo;

use tauri::{Emitter, Manager, WindowEvent};

use comandos::{actualizaciones, datos, identidad as comandos_identidad, sistema, usuarios};

/// Con una operación de `cm` corriendo, la ventana no se cierra: se cancela el
/// cierre y se le avisa al frontend (ver "Bloquear el cierre" en BACKEND.md).
fn bloquear_cierre_en_operacion(ventana: &tauri::Window, evento: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = evento {
        let estado = ventana.state::<estado::AppState>();
        if estado.hay_operacion_en_curso() {
            api.prevent_close();
            let _ = ventana.emit(sistema::EVENTO_CIERRE_BLOQUEADO, ());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(estado::AppState::default())
        .on_window_event(bloquear_cierre_en_operacion)
        .invoke_handler(tauri::generate_handler![
            sistema::ping,
            sistema::hay_operacion_en_curso,
            actualizaciones::buscar_actualizacion,
            actualizaciones::instalar_actualizacion,
            datos::listar_backups,
            datos::restaurar_backup,
            comandos_identidad::exportar_llave_firma,
            usuarios::listar_usuarios,
            usuarios::crear_usuario,
            usuarios::cambiar_rol,
            usuarios::confirmar_usuario,
            usuarios::eliminar_usuario,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar BranchBoard");
}
