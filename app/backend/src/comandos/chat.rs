//! Comandos Tauri del chat interno del equipo (issue #119).

use tauri::State;

use crate::chat::{self, EstadoChat};
use crate::comandos::datos::carpeta_abierta;
use crate::db::chat::{GrupoChat, Mensaje};
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::ahora_iso;

/// Devuelve grupos, mensajes visibles, eliminaciones locales y conversaciones sin leer (#119).
#[tauri::command]
pub fn obtener_chat(estado: State<'_, AppState>) -> Resultado<EstadoChat> {
    obtener_chat_en(&estado)
}

/// Versión sin envoltorio Tauri de [`obtener_chat`].
pub fn obtener_chat_en(estado: &AppState) -> Resultado<EstadoChat> {
    let yo = estado.exigir_permiso("obtener_chat")?;
    estado.con_db(|db| chat::obtener_estado(db.conexion(), &yo.email))
}

/// Conversaciones con al menos un mensaje sin leer del usuario actual (#119).
#[tauri::command]
pub fn conversaciones_sin_leer(estado: State<'_, AppState>) -> Resultado<Vec<String>> {
    conversaciones_sin_leer_en(&estado)
}

/// Versión sin envoltorio Tauri de [`conversaciones_sin_leer`].
pub fn conversaciones_sin_leer_en(estado: &AppState) -> Resultado<Vec<String>> {
    let yo = estado.exigir_permiso("conversaciones_sin_leer")?;
    estado.con_db(|db| chat::conversaciones_sin_leer(db.conexion(), &yo.email))
}

/// Envía un mensaje a `"grupo"`, a un grupo creado o en privado a un compañero (#119).
#[tauri::command]
pub fn enviar_chat(estado: State<'_, AppState>, para: String, texto: String) -> Resultado<Mensaje> {
    enviar_chat_en(&estado, &para, &texto)
}

/// Versión sin envoltorio Tauri de [`enviar_chat`].
pub fn enviar_chat_en(estado: &AppState, para: &str, texto: &str) -> Resultado<Mensaje> {
    let yo = estado.exigir_permiso("enviar_chat")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    estado.con_db(|db| chat::enviar(db.conexion(), &ops, &yo.email, para, texto, &ahora_iso()))
}

/// Crea un grupo de chat con los miembros indicados (siempre incluye al creador, #119).
#[tauri::command]
pub fn crear_grupo_chat(
    estado: State<'_, AppState>,
    nombre: String,
    miembros: Vec<String>,
) -> Resultado<GrupoChat> {
    crear_grupo_chat_en(&estado, &nombre, &miembros)
}

/// Versión sin envoltorio Tauri de [`crear_grupo_chat`].
pub fn crear_grupo_chat_en(
    estado: &AppState,
    nombre: &str,
    miembros: &[String],
) -> Resultado<GrupoChat> {
    let yo = estado.exigir_permiso("crear_grupo_chat")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    estado.con_db(|db| {
        chat::crear_grupo(
            db.conexion(),
            &ops,
            &yo.email,
            nombre,
            miembros,
            &ahora_iso(),
        )
    })
}

/// Marca como leídos todos los mensajes de una conversación para esta máquina (local #119).
#[tauri::command]
pub fn marcar_chat_leido(estado: State<'_, AppState>, conversacion: String) -> Resultado<usize> {
    marcar_chat_leido_en(&estado, &conversacion)
}

/// Versión sin envoltorio Tauri de [`marcar_chat_leido`].
pub fn marcar_chat_leido_en(estado: &AppState, conversacion: &str) -> Resultado<usize> {
    let yo = estado.exigir_permiso("marcar_chat_leido")?;
    estado.con_db(|db| chat::marcar_leido(db.conexion(), &yo.email, conversacion))
}

/// Oculta una conversación en esta máquina dejando visibles futuros mensajes (local #119).
#[tauri::command]
pub fn eliminar_chat(estado: State<'_, AppState>, conversacion: String) -> Resultado<()> {
    eliminar_chat_en(&estado, &conversacion)
}

/// Versión sin envoltorio Tauri de [`eliminar_chat`].
pub fn eliminar_chat_en(estado: &AppState, conversacion: &str) -> Resultado<()> {
    let yo = estado.exigir_permiso("eliminar_chat")?;
    estado.con_db(|db| chat::eliminar(db.conexion(), &yo.email, conversacion, &ahora_iso()))
}
