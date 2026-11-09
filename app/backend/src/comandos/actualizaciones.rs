//! Actualizar la app sin desinstalarla (apartado "Actualizar versión" de Ajustes).
//!
//! La app consulta `latest.json` en las Releases de GitHub. Si hay una versión
//! mayor, descarga el instalador, verifica su firma con la llave pública que va
//! en `tauri.conf.json` y lo instala encima. Los datos (`.db`, `ops/`) viven
//! fuera de la carpeta de instalación, así que no se pierden.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{Error, Resultado};
use crate::estado::AppState;

/// Evento que recibe el frontend mientras se descarga la versión nueva.
pub const EVENTO_PROGRESO: &str = "actualizacion-progreso";

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct InfoActualizacion {
    /// Versión instalada ahora.
    pub version_actual: String,
    /// Versión nueva disponible; `None` si la app ya está al día.
    pub version_nueva: Option<String>,
    /// Qué trae la versión nueva (texto de la Release).
    pub notas: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct Progreso {
    descargado: u64,
    /// Tamaño total en bytes, si el servidor lo informa.
    total: Option<u64>,
}

fn error(e: impl std::fmt::Display) -> Error {
    Error::Actualizacion(e.to_string())
}

/// Revisa si hay una versión nueva. No descarga nada.
#[tauri::command]
pub async fn buscar_actualizacion(app: AppHandle) -> Resultado<InfoActualizacion> {
    let version_actual = app.package_info().version.to_string();
    let nueva = app.updater().map_err(error)?.check().await.map_err(error)?;
    Ok(InfoActualizacion {
        version_actual,
        version_nueva: nueva.as_ref().map(|u| u.version.clone()),
        notas: nueva.and_then(|u| u.body),
    })
}

/// Descarga e instala la versión nueva y reinicia la app.
/// Se rechaza si hay una operación de `cm` corriendo.
#[tauri::command]
pub async fn instalar_actualizacion(app: AppHandle, estado: State<'_, AppState>) -> Resultado<()> {
    if estado.hay_operacion_en_curso() {
        return Err(Error::Validacion(
            "Hay una operación en curso. Espera a que termine para actualizar.".into(),
        ));
    }
    let Some(nueva) = app.updater().map_err(error)?.check().await.map_err(error)? else {
        return Err(Error::Validacion("Ya tienes la última versión.".into()));
    };

    let mut descargado: u64 = 0;
    let avisar = app.clone();
    nueva
        .download_and_install(
            move |bytes, total| {
                descargado += bytes as u64;
                let _ = avisar.emit(EVENTO_PROGRESO, Progreso { descargado, total });
            },
            || {},
        )
        .await
        .map_err(error)?;

    app.restart();
}
