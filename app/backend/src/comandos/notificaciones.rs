//! Comandos Tauri de notificaciones de la campana (issue #116).

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::notificaciones::Notificacion;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::ahora_iso;
use crate::notificaciones;

/// Lista todas las notificaciones del usuario actual (de la más reciente a la más antigua, #116).
#[tauri::command]
pub fn obtener_notificaciones(estado: State<'_, AppState>) -> Resultado<Vec<Notificacion>> {
    obtener_notificaciones_en(&estado)
}

/// Versión sin envoltorio Tauri de [`obtener_notificaciones`].
pub fn obtener_notificaciones_en(estado: &AppState) -> Resultado<Vec<Notificacion>> {
    let yo = estado.exigir_permiso("obtener_notificaciones")?;
    estado.con_db(|db| notificaciones::listar_de_usuario(db.conexion(), &yo.email))
}

/// Marca una notificación propia como leída (local, #116).
#[tauri::command]
pub fn marcar_leida(estado: State<'_, AppState>, notificacion_id: String) -> Resultado<()> {
    marcar_leida_en(&estado, &notificacion_id)
}

/// Versión sin envoltorio Tauri de [`marcar_leida`].
pub fn marcar_leida_en(estado: &AppState, notificacion_id: &str) -> Resultado<()> {
    let yo = estado.exigir_permiso("marcar_leida")?;
    estado
        .con_db(|db| notificaciones::marcar_leida(db.conexion(), &yo.email, notificacion_id, true))
}

/// Vuelve a marcar una notificación propia como no leída (local, #116).
#[tauri::command]
pub fn desmarcar_leida(estado: State<'_, AppState>, notificacion_id: String) -> Resultado<()> {
    desmarcar_leida_en(&estado, &notificacion_id)
}

/// Versión sin envoltorio Tauri de [`desmarcar_leida`].
pub fn desmarcar_leida_en(estado: &AppState, notificacion_id: &str) -> Resultado<()> {
    let yo = estado.exigir_permiso("desmarcar_leida")?;
    estado
        .con_db(|db| notificaciones::marcar_leida(db.conexion(), &yo.email, notificacion_id, false))
}

/// Marca todas las notificaciones del usuario actual como leídas (local, #116).
#[tauri::command]
pub fn marcar_todas_leidas(estado: State<'_, AppState>) -> Resultado<usize> {
    marcar_todas_leidas_en(&estado)
}

/// Versión sin envoltorio Tauri de [`marcar_todas_leidas`].
pub fn marcar_todas_leidas_en(estado: &AppState) -> Resultado<usize> {
    let yo = estado.exigir_permiso("marcar_todas_leidas")?;
    estado.con_db(|db| notificaciones::marcar_todas_leidas(db.conexion(), &yo.email))
}

/// Envía un mensaje manual (máx. 500 caracteres) a otro miembro del equipo vía `ops/` (#118).
#[tauri::command]
pub fn enviar_mensaje(
    estado: State<'_, AppState>,
    para: String,
    texto: String,
) -> Resultado<Notificacion> {
    enviar_mensaje_en(&estado, &para, &texto)
}

/// Versión sin envoltorio Tauri de [`enviar_mensaje`].
pub fn enviar_mensaje_en(estado: &AppState, para: &str, texto: &str) -> Resultado<Notificacion> {
    let yo = estado.exigir_permiso("enviar_mensaje")?;
    estado.con_db(|db| {
        notificaciones::enviar_mensaje(db.conexion(), &yo.email, para, texto, &ahora_iso())
    })
}

/// Solicita revisión o restauración de la base de datos a todos los Administradores (#118).
#[tauri::command]
pub fn solicitar_accion_bd(
    estado: State<'_, AppState>,
    motivo: String,
) -> Resultado<Vec<Notificacion>> {
    solicitar_accion_bd_en(&estado, &motivo)
}

/// Versión sin envoltorio Tauri de [`solicitar_accion_bd`].
pub fn solicitar_accion_bd_en(estado: &AppState, motivo: &str) -> Resultado<Vec<Notificacion>> {
    let yo = estado.exigir_permiso("solicitar_accion_bd")?;
    estado.con_db(|db| {
        notificaciones::solicitar_accion_bd(
            db.conexion(),
            &yo.email,
            yo.rol,
            motivo,
            &ahora_iso(),
        )
    })
}
