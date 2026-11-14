//! Comandos generales de la app (no dependen de un proyecto).

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{Error, Resultado};
use crate::estado::AppState;

/// Evento que recibe el frontend cuando se intentó cerrar la ventana con una
/// operación en curso y el cierre se canceló.
pub const EVENTO_CIERRE_BLOQUEADO: &str = "cierre-bloqueado";

/// Comando de prueba: confirma que el frontend puede llamar al backend.
#[tauri::command]
pub fn ping() -> Resultado<String> {
    Ok("pong".to_string())
}

/// `true` mientras haya una operación larga corriendo; el frontend lo consulta
/// antes de dejar cerrar la ventana.
#[tauri::command]
pub fn hay_operacion_en_curso(estado: State<'_, AppState>) -> Resultado<bool> {
    Ok(estado.hay_operacion_en_curso())
}

/// Abre el explorador de carpetas del sistema y devuelve la carpeta elegida,
/// o `None` si la persona cancela. Lo abre el backend (como al exportar la
/// llave), así la webview no necesita acceso al disco.
#[tauri::command]
pub async fn elegir_carpeta(app: AppHandle, titulo: Option<String>) -> Resultado<Option<String>> {
    let mut dialogo = app.dialog().file();
    if let Some(titulo) = titulo.filter(|t| !t.trim().is_empty()) {
        dialogo = dialogo.set_title(titulo);
    }
    let Some(carpeta) = dialogo.blocking_pick_folder() else {
        return Ok(None);
    };
    let ruta = carpeta
        .into_path()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    Ok(Some(ruta.to_string_lossy().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_responde_pong() {
        assert_eq!(ping().unwrap(), "pong");
    }
}
