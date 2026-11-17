//! Visor de diferencias de solo lectura (#152).
use crate::{
    diff::{self, ArchivoDiff, Diferencias},
    error::Resultado,
    estado::AppState,
};
use tauri::State;

#[tauri::command]
pub async fn diff_rama(estado: State<'_, AppState>, tarea_id: String) -> Resultado<Diferencias> {
    diff::consultar_en(&estado, &tarea_id, None, None)
}

#[tauri::command]
pub async fn diff_changeset(
    estado: State<'_, AppState>,
    tarea_id: String,
    hash: String,
) -> Resultado<Diferencias> {
    diff::consultar_en(&estado, &tarea_id, Some(&hash), None)
}

#[tauri::command]
pub async fn diff_archivo(
    estado: State<'_, AppState>,
    tarea_id: String,
    hash: Option<String>,
    ruta: String,
) -> Resultado<ArchivoDiff> {
    let mut diff = diff::consultar_en(&estado, &tarea_id, hash.as_deref(), Some(&ruta))?;
    Ok(diff.archivos.remove(0))
}
