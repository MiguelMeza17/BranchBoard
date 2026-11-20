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

/// Abre la ventana "Guardar como" del sistema para elegir dónde crear una carpeta **nueva** y escribir su
/// nombre (el explorador de carpetas no deja elegir una que todavía no existe). Devuelve la ruta completa
/// de la carpeta a crear, o `None` si la persona cancela; no crea nada.
#[tauri::command]
pub async fn elegir_carpeta_nueva(
    app: AppHandle,
    titulo: Option<String>,
    nombre: Option<String>,
) -> Resultado<Option<String>> {
    let mut dialogo = app.dialog().file();
    if let Some(titulo) = titulo.filter(|t| !t.trim().is_empty()) {
        dialogo = dialogo.set_title(titulo);
    }
    if let Some(nombre) = nombre.filter(|n| !n.trim().is_empty()) {
        dialogo = dialogo.set_file_name(nombre);
    }
    let Some(ruta) = dialogo.blocking_save_file() else {
        return Ok(None);
    };
    let ruta = ruta
        .into_path()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    Ok(Some(ruta.to_string_lossy().to_string()))
}

/// Abre el explorador de archivos del sistema para elegir uno o varios archivos
/// y devuelve sus rutas, o `None` si la persona cancela. Igual que
/// [`elegir_carpeta`]: la webview solo recibe las rutas, el backend las lee.
#[tauri::command]
pub async fn elegir_archivos(
    app: AppHandle,
    titulo: Option<String>,
) -> Resultado<Option<Vec<String>>> {
    let mut dialogo = app.dialog().file();
    if let Some(titulo) = titulo.filter(|t| !t.trim().is_empty()) {
        dialogo = dialogo.set_title(titulo);
    }
    let Some(archivos) = dialogo.blocking_pick_files() else {
        return Ok(None);
    };
    let rutas = archivos
        .into_iter()
        .map(|a| {
            a.into_path()
                .map(|r| r.to_string_lossy().to_string())
                .map_err(|e| Error::Archivos(e.to_string()))
        })
        .collect::<Resultado<Vec<_>>>()?;
    Ok(Some(rutas))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_responde_pong() {
        assert_eq!(ping().unwrap(), "pong");
    }
}
