//! Comandos Tauri de Cloudflare WARP (#122): son libres (corren antes de
//! iniciar sesión o entrar a un proyecto).

use tauri::State;

use crate::error::Resultado;
use crate::estado::AppState;
use crate::warp::{self, EstadoWarp, ResultadoInstalacionWarp};

#[tauri::command]
pub async fn estado_warp(estado: State<'_, AppState>) -> Resultado<EstadoWarp> {
    warp::estado_warp(&estado)
}

#[tauri::command]
pub async fn alternar_warp(estado: State<'_, AppState>) -> Resultado<EstadoWarp> {
    warp::alternar_warp(&estado)
}

#[tauri::command]
pub async fn instalar_warp(estado: State<'_, AppState>) -> Resultado<ResultadoInstalacionWarp> {
    warp::instalar_warp(&estado)
}
