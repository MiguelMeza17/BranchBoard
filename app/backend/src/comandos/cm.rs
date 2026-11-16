//! Comandos Tauri de estado e instalación de Unity Version Control (`cm`, #123).

use tauri::State;

use crate::cm::instalacion::{self, EstadoCm, ResultadoInstalacionCm};
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
