//! Comandos de los proyectos sin Unity VC (ver `crate::proyecto_local`).
//! Crear, abrir, importar y listar son libres (todavía no hay proyecto ni rol);
//! exportar es exclusivo del Administrador.

use std::path::Path;

use serde::Serialize;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::comandos::acceso::carpeta_datos;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::Entrada;
use crate::modelo::{ahora_iso, hoy_local};
use crate::proyecto_local::{self, NuevoProyectoLocal, ProyectoLocal, EXTENSION_EXPORTADO};

/// Respuesta de `crear_proyecto_local`: dónde quedó y lo que ve quien lo creó.
#[derive(Debug, Serialize)]
pub struct ProyectoLocalCreado {
    pub ruta: String,
    pub entrada: Entrada,
}

/// Crea el proyecto en una carpeta nueva dentro de `datos.carpeta` y lo abre;
/// quien lo crea queda como su Administrador.
#[tauri::command]
pub async fn crear_proyecto_local(
    app: AppHandle,
    estado: State<'_, AppState>,
    datos: NuevoProyectoLocal,
) -> Resultado<ProyectoLocalCreado> {
    let carpeta = proyecto_local::crear(&datos, &ahora_iso(), &hoy_local())?;
    let entrada =
        proyecto_local::abrir(&estado, &carpeta_datos(&app)?, &carpeta, &datos.clave, true)?;
    Ok(ProyectoLocalCreado {
        ruta: carpeta.to_string_lossy().to_string(),
        entrada,
    })
}

/// Abre el proyecto local de `carpeta` (debe tener la marca de BranchBoard) con su contraseña.
#[tauri::command]
pub async fn abrir_proyecto_local(
    app: AppHandle,
    estado: State<'_, AppState>,
    carpeta: String,
    clave: String,
) -> Resultado<Entrada> {
    proyecto_local::abrir(
        &estado,
        &carpeta_datos(&app)?,
        Path::new(&carpeta),
        &clave,
        false,
    )
}

/// Proyectos locales que ya se abrieron en esta máquina y siguen en su carpeta.
#[tauri::command]
pub async fn listar_proyectos_locales(app: AppHandle) -> Resultado<Vec<ProyectoLocal>> {
    Ok(proyecto_local::listar_recientes(&carpeta_datos(&app)?))
}

/// Guarda el proyecto local abierto como `.bbproyecto`. El backend abre
/// "Guardar como"; devuelve la ruta, o `None` si se cancela.
#[tauri::command]
pub async fn exportar_proyecto_local(
    app: AppHandle,
    estado: State<'_, AppState>,
) -> Resultado<Option<String>> {
    estado.exigir_permiso("exportar_proyecto_local")?;
    let carpeta = proyecto_local::carpeta_abierta(&estado)?;
    let nombre = carpeta
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "proyecto".into());
    let Some(destino) = app
        .dialog()
        .file()
        .set_title("Exportar proyecto")
        .set_file_name(format!("{nombre}.{EXTENSION_EXPORTADO}"))
        .add_filter("Proyecto de BranchBoard", &[EXTENSION_EXPORTADO])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let ruta = destino
        .into_path()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    let _operacion = estado.iniciar_operacion();
    proyecto_local::exportar(&estado, &ruta)?;
    Ok(Some(ruta.to_string_lossy().to_string()))
}

/// Pide un `.bbproyecto` y lo descomprime dentro de `carpeta`. Devuelve el
/// proyecto importado (para abrirlo con su contraseña), o `None` si se cancela.
#[tauri::command]
pub async fn importar_proyecto_local(
    app: AppHandle,
    estado: State<'_, AppState>,
    carpeta: String,
) -> Resultado<Option<ProyectoLocal>> {
    let Some(archivo) = app
        .dialog()
        .file()
        .set_title("Importar proyecto")
        .add_filter("Proyecto de BranchBoard", &[EXTENSION_EXPORTADO])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let ruta = archivo
        .into_path()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    let _operacion = estado.iniciar_operacion();
    proyecto_local::importar(&ruta, &carpeta).map(Some)
}
