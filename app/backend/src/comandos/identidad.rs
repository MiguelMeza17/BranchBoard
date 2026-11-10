//! Comandos de identidad y firmas. **Exclusivos del Administrador**: el chequeo
//! de rol se conecta aquí cuando exista (issue #87).

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::comandos::datos::carpeta_abierta;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::llaves;

/// Nombre que se propone para el respaldo de la llave.
const NOMBRE_RESPALDO: &str = "branchboard-llave-admin.bbkey";

/// Respaldo de la llave privada del Administrador, cifrado con `passphrase`.
/// El propio backend abre "Guardar como" y escribe el archivo: la llave nunca
/// pasa por la webview. Devuelve `false` si la persona cancela.
#[tauri::command]
pub async fn exportar_llave_firma(
    app: AppHandle,
    estado: State<'_, AppState>,
    passphrase: String,
) -> Resultado<bool> {
    let exportado = llaves::exportar_llave_firma(&carpeta_abierta(&estado)?, &passphrase)?;
    let Some(destino) = app
        .dialog()
        .file()
        .set_file_name(NOMBRE_RESPALDO)
        .add_filter("Llave de BranchBoard", &["bbkey"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let ruta = destino
        .into_path()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    std::fs::write(ruta, exportado)?;
    Ok(true)
}
