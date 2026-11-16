//! Comandos Tauri de estado, instalación y sesión de Unity Version Control (`cm`, #123, #124).

use tauri::State;

use crate::cm::instalacion::{self, EstadoCm, ResultadoInstalacionCm};
use crate::cm::sesion::{self, EstadoSesionUnityVc, ResultadoReconexionUnityVc};
use crate::error::Resultado;
use crate::estado::AppState;

#[tauri::command]
pub async fn estado_cm() -> Resultado<EstadoCm> {
    Ok(instalacion::estado_cm())
}

#[tauri::command]
pub async fn instalar_cm(estado: State<'_, AppState>) -> Resultado<ResultadoInstalacionCm> {
    instalacion::instalar_cm(&estado)
}

#[tauri::command]
pub async fn sesion_unity_vc(estado: State<'_, AppState>) -> Resultado<EstadoSesionUnityVc> {
    sesion::sesion_unity_vc(&estado)
}

#[tauri::command]
pub async fn iniciar_sesion_unity_vc(
    estado: State<'_, AppState>,
    forzar: Option<bool>,
) -> Resultado<EstadoSesionUnityVc> {
    sesion::iniciar_sesion_unity_vc(&estado, forzar.unwrap_or(false))
}

#[tauri::command]
pub async fn reconectar_unity_vc(
    estado: State<'_, AppState>,
    org: Option<String>,
) -> Resultado<ResultadoReconexionUnityVc> {
    sesion::reconectar_unity_vc(&estado, org.as_deref())
}

#[tauri::command]
pub async fn cerrar_sesion_unity_vc(estado: State<'_, AppState>) -> Resultado<usize> {
    sesion::cerrar_sesion_unity_vc(&estado)
}
