//! Comandos Tauri de estado, instalación, sesión y proyectos de Unity VC (`cm`, #123, #124, #125).

use tauri::{AppHandle, Emitter, State};

use crate::cm::instalacion::{self, EstadoCm, ResultadoInstalacionCm};
use crate::cm::proyectos::{
    self, ProyectoServidor, ResultadoProyectoNuevo, WorkspaceProyecto, EVENTO_PROGRESO_CLONADO,
};
use crate::cm::sesion::{self, EstadoSesionUnityVc, ResultadoReconexionUnityVc};
use crate::comandos::acceso::carpeta_datos;
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

#[tauri::command]
pub async fn listar_workspaces_locales(
    app: AppHandle,
    estado: State<'_, AppState>,
) -> Resultado<Vec<WorkspaceProyecto>> {
    let datos = carpeta_datos(&app)?;
    proyectos::listar_workspaces_locales_en(&estado, &datos)
}

#[tauri::command]
pub async fn listar_tus_proyectos(
    app: AppHandle,
    estado: State<'_, AppState>,
) -> Resultado<Vec<WorkspaceProyecto>> {
    let datos = carpeta_datos(&app)?;
    proyectos::listar_tus_proyectos_en(&estado, &datos)
}

#[tauri::command]
pub async fn listar_proyectos_servidor(
    app: AppHandle,
    estado: State<'_, AppState>,
) -> Resultado<Vec<ProyectoServidor>> {
    let datos = carpeta_datos(&app)?;
    proyectos::listar_proyectos_servidor_en(&estado, &datos)
}

#[tauri::command]
pub async fn clonar_proyecto(
    app: AppHandle,
    estado: State<'_, AppState>,
    repo: String,
    carpeta: String,
) -> Resultado<WorkspaceProyecto> {
    let datos = carpeta_datos(&app)?;
    proyectos::clonar_proyecto_en(&estado, &datos, &repo, &carpeta, |p| {
        let _ = app.emit(EVENTO_PROGRESO_CLONADO, &p);
    })
}

#[tauri::command]
pub async fn crear_proyecto(
    app: AppHandle,
    estado: State<'_, AppState>,
    nombre: String,
    carpeta: String,
    proyecto_unity: Option<String>,
    org: Option<String>,
) -> Resultado<ResultadoProyectoNuevo> {
    let datos = carpeta_datos(&app)?;
    proyectos::crear_proyecto_en(
        &estado,
        &datos,
        &nombre,
        &carpeta,
        proyecto_unity.as_deref(),
        org.as_deref(),
    )
}

#[tauri::command]
pub async fn traer_proyecto(
    app: AppHandle,
    estado: State<'_, AppState>,
    carpeta: String,
) -> Resultado<ResultadoProyectoNuevo> {
    let datos = carpeta_datos(&app)?;
    proyectos::traer_proyecto_en(&estado, &datos, std::path::Path::new(&carpeta))
}

/// `abrir_unity_cloud(org)`: abre en el navegador la página de proyectos de la
/// organización en el Unity Cloud Dashboard, para crear ahí un proyecto padre
/// nuevo (`cm` no puede, #205). Devuelve la URL abierta. Sin el id numérico de
/// la organización (solo está en `cloudregions.conf` cuando se entró por Unity
/// ID) abre la portada del Dashboard.
#[tauri::command]
pub async fn abrir_unity_cloud(org: Option<String>) -> Resultado<String> {
    let id = org
        .as_deref()
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .and_then(crate::servidor::region::id_organizacion);
    let url = crate::servidor::region::url_proyectos_unity_cloud(id.as_deref());
    crate::instalacion::abrir_url(&url)?;
    Ok(url)
}
