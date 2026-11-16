//! Comandos Tauri de notificaciones de la campana (issue #116).

use tauri::State;

use crate::db::notificaciones::Notificacion;
use crate::error::Resultado;
use crate::estado::AppState;
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
