//! Backend de BranchBoard (Tauri). Diseño completo en `documentacion/BACKEND.md`.
//!
//! Organización:
//! - `comandos/` — lo que el frontend llama con `invoke`, por área.
//! - `db` — SQLite local.
//! - `checkins` — check-ins de la rama de una tarea, copiados a `commits`.
//! - `cm` — Unity Version Control (subproceso `cm`).
//! - `ramas` — ramas de tareas y sus protecciones.
//! - `servidor` — API REST del servidor de Unity VC (con PAT).
//! - `eventos` — sincronización por `ops/`.
//! - `identidad` — quién es el usuario, firmas y permisos.
//! - `modelo` — tipos del dominio (roles, estados…), iguales a los del frontend.
//! - `error` — error común `{ codigo, mensaje }`.
//! - `estado` — estado compartido de la app.

pub mod checkins;
pub mod cm;
pub mod comandos;
pub mod db;
pub mod error;
pub mod estado;
pub mod eventos;
pub mod identidad;
pub mod modelo;
pub mod ramas;
pub mod servidor;
pub mod sprints;
pub mod tareas;

use tauri::{Emitter, Manager, WindowEvent};

use comandos::{
    acceso, actualizaciones, datos, eventos as comandos_eventos, identidad as comandos_identidad,
    sistema, sprints as comandos_sprints, tareas as comandos_tareas, usuarios,
};

fn disparar_polling(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let estado = app.state::<estado::AppState>();
        if let Ok(resumen) = comandos_eventos::revisar_actualizaciones_en(&estado, None) {
            if resumen.hubo_cambios() {
                let _ = app.emit(comandos_eventos::EVENTO_UNITY_ACTUALIZADO, &resumen);
            }
            if let Some(ref alerta) = resumen.alerta_rama_ajena {
                let _ = app.emit(comandos_eventos::EVENTO_RAMA_AJENA, alerta);
            }
        }
    });
}

/// Con una operación de `cm` corriendo, la ventana no se cierra: se cancela el
/// cierre y se le avisa al frontend (ver "Bloquear el cierre" en BACKEND.md).
fn manejar_evento_ventana(ventana: &tauri::Window, evento: &WindowEvent) {
    match evento {
        WindowEvent::CloseRequested { api, .. } => {
            let estado = ventana.state::<estado::AppState>();
            if estado.hay_operacion_en_curso() {
                api.prevent_close();
                let _ = ventana.emit(sistema::EVENTO_CIERRE_BLOQUEADO, ());
            }
        }
        WindowEvent::Focused(true) => {
            disparar_polling(ventana.app_handle().clone());
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(estado::AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(15));
                disparar_polling(handle.clone());
            });
            Ok(())
        })
        .on_window_event(manejar_evento_ventana)
        .invoke_handler(tauri::generate_handler![
            sistema::ping,
            sistema::hay_operacion_en_curso,
            sistema::elegir_carpeta,
            acceso::entrar_a_proyecto,
            actualizaciones::buscar_actualizacion,
            actualizaciones::instalar_actualizacion,
            datos::listar_backups,
            datos::restaurar_backup,
            comandos_eventos::carpeta_compartida,
            comandos_eventos::configurar_carpeta_compartida,
            comandos_eventos::sincronizar_eventos,
            comandos_eventos::revisar_actualizaciones,
            comandos_eventos::compactar_eventos,
            comandos_identidad::exportar_llave_firma,
            usuarios::listar_usuarios,
            usuarios::crear_usuario,
            usuarios::cambiar_rol,
            usuarios::confirmar_usuario,
            usuarios::eliminar_usuario,
            comandos_tareas::listar_tareas,
            comandos_tareas::crear_tarea,
            comandos_tareas::crear_tarea_con_subtareas,
            comandos_tareas::editar_tarea,
            comandos_tareas::tomar_tarea,
            comandos_tareas::asignar_tarea,
            comandos_tareas::desasignar_tarea,
            comandos_tareas::cambiar_estado,
            comandos_tareas::borrar_progreso,
            comandos_tareas::reabrir_tarea,
            comandos_tareas::aprobar_tarea,
            comandos_tareas::aprobar_tareas,
            comandos_tareas::editar_y_aprobar_tarea,
            comandos_tareas::advertir_tarea_pendiente,
            comandos_tareas::listar_ediciones_pendientes,
            comandos_tareas::proponer_edicion,
            comandos_tareas::aprobar_modificacion,
            comandos_tareas::aprobar_modificaciones,
            comandos_tareas::rechazar_modificacion,
            comandos_tareas::listar_papelera,
            comandos_tareas::eliminar_tarea,
            comandos_tareas::eliminar_definitivamente,
            comandos_tareas::cambios_pendientes,
            comandos_tareas::cambiar_a_rama_tarea,
            comandos_tareas::hacer_checkin,
            comandos_tareas::crear_rama_desde_espera,
            comandos_tareas::listar_cambios_prestados,
            comandos_tareas::traer_cambios,
            comandos_tareas::devolver_cambios_prestados,
            comandos_tareas::eliminar_cambios_ajenos,
            comandos_tareas::volver_a_dev,
            comandos_sprints::listar_sprints,
            comandos_sprints::crear_sprint,
            comandos_sprints::asignar_tarea_a_sprint,
            comandos_sprints::asignar_tarea_a_backlog,
            comandos_sprints::asignar_backlog_a_sprint,
            comandos_sprints::cerrar_sprint,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar BranchBoard");
}
