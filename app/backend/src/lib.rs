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
//! - `proyecto` — ajustes del proyecto entero: nombre, configuración e imagen.
//! - `modelo` — tipos del dominio (roles, estados…), iguales a los del frontend.
//! - `error` — error común `{ codigo, mensaje }`.
//! - `estado` — estado compartido de la app.

pub mod avance;
pub mod cambios_directos;
pub mod chat;
pub mod checkins;
pub mod cm;
pub mod comandos;
pub mod db;
pub mod diff;
pub mod documentos;
pub mod entrantes;
pub mod error;
pub mod estado;
pub mod eventos;
pub mod identidad;
pub mod instalacion;
pub mod modelo;
pub mod notificaciones;
pub mod proyecto;
pub mod ramas;
pub mod servidor;
pub mod sprints;
pub mod tareas;
pub mod unity;
pub mod warp;

use tauri::{Emitter, Manager, WindowEvent};

use comandos::{
    acceso, actualizaciones, chat as comandos_chat, cm as comandos_cm, datos,
    documentos as comandos_documentos, eventos as comandos_eventos,
    identidad as comandos_identidad, notificaciones as comandos_notificaciones,
    proyecto as comandos_proyecto, sistema, sprints as comandos_sprints, tareas as comandos_tareas,
    usuarios, vistos as comandos_vistos, warp as comandos_warp,
};

/// Cadencia del temporizador en segundo plano mientras la app está abierta (#104, #117).
const INTERVALO_POLLING_SEGUNDOS: u64 = 15;

fn disparar_polling(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let estado = app.state::<estado::AppState>();
        match comandos_eventos::revisar_actualizaciones_en(&estado, None) {
            Ok(resumen) => {
                if resumen.hubo_cambios() {
                    let _ = app.emit(comandos_eventos::EVENTO_UNITY_ACTUALIZADO, &resumen);
                }
                if let Some(ref alerta) = resumen.alerta_rama_ajena {
                    let _ = app.emit(comandos_eventos::EVENTO_RAMA_AJENA, alerta);
                }
            }
            Err(error::Error::SesionUnityVcCaducada) => {
                let _ = app.emit(cm::sesion::EVENTO_SESION_CADUCADA, ());
            }
            Err(_) => {}
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
                std::thread::sleep(std::time::Duration::from_secs(INTERVALO_POLLING_SEGUNDOS));
                disparar_polling(handle.clone());
            });
            Ok(())
        })
        .on_window_event(manejar_evento_ventana)
        .invoke_handler(tauri::generate_handler![
            sistema::ping,
            comandos::diff::diff_rama,
            comandos::diff::diff_changeset,
            comandos::diff::diff_archivo,
            entrantes::cambios_entrantes,
            entrantes::unir_cambios_entrantes,
            sistema::hay_operacion_en_curso,
            sistema::elegir_carpeta,
            comandos_warp::estado_warp,
            comandos_warp::alternar_warp,
            comandos_warp::instalar_warp,
            comandos_cm::estado_cm,
            comandos_cm::instalar_cm,
            comandos_cm::sesion_unity_vc,
            comandos_cm::iniciar_sesion_unity_vc,
            comandos_cm::reconectar_unity_vc,
            comandos_cm::cerrar_sesion_unity_vc,
            comandos_cm::listar_workspaces_locales,
            comandos_cm::listar_tus_proyectos,
            comandos_cm::listar_proyectos_servidor,
            comandos_cm::clonar_proyecto,
            comandos_cm::crear_proyecto,
            comandos_cm::traer_proyecto,
            comandos_cm::abrir_unity_cloud,
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
            cambios_directos::trasladar_cambios_directos,
            comandos_tareas::volver_a_dev,
            comandos_tareas::asignar_revision,
            comandos_tareas::autorizar_merge,
            comandos_tareas::rechazar_revision,
            comandos_tareas::listar_commits,
            comandos_tareas::abrir_en_unity,
            comandos_tareas::abrir_resolucion_conflicto,
            comandos_sprints::listar_sprints,
            comandos_sprints::crear_sprint,
            comandos_sprints::asignar_tarea_a_sprint,
            comandos_sprints::asignar_tarea_a_backlog,
            comandos_sprints::asignar_backlog_a_sprint,
            comandos_sprints::cerrar_sprint,
            comandos_sprints::datos_avance_sprint,
            comandos_sprints::datos_avance_backlog,
            comandos_sprints::datos_avance_general,
            comandos_notificaciones::obtener_notificaciones,
            comandos_notificaciones::marcar_leida,
            comandos_notificaciones::desmarcar_leida,
            comandos_notificaciones::marcar_todas_leidas,
            comandos_notificaciones::enviar_mensaje,
            comandos_notificaciones::solicitar_accion_bd,
            comandos_chat::obtener_chat,
            comandos_chat::conversaciones_sin_leer,
            comandos_chat::enviar_chat,
            comandos_chat::crear_grupo_chat,
            comandos_chat::marcar_chat_leido,
            comandos_chat::eliminar_chat,
            comandos_documentos::listar_documentos,
            comandos_documentos::subir_documentos,
            comandos_documentos::aprobar_documento,
            comandos_documentos::eliminar_documento,
            comandos_documentos::abrir_documento,
            comandos_proyecto::obtener_configuracion_proyecto,
            comandos_proyecto::renombrar_proyecto,
            comandos_proyecto::eliminar_proyecto,
            comandos_proyecto::actualizar_configuracion_proyecto,
            comandos_proyecto::subir_imagen_proyecto,
            comandos_vistos::marcar_visto,
            comandos_vistos::obtener_vistos,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar BranchBoard");
}
