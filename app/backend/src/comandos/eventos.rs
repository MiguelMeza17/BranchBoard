//! Carpeta compartida del proyecto (Drive/OneDrive) en esta máquina, donde vive
//! `ops/` (issue #91), y escaneo de `ops/` (#92). Todos los roles.

use chrono::Utc;
use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos::{self, ResumenSincronizacion};
use crate::modelo::Rol;

/// Ruta de la carpeta compartida del proyecto abierto, o `None` si no se configuró.
#[tauri::command]
pub fn carpeta_compartida(estado: State<'_, AppState>) -> Resultado<Option<String>> {
    estado.exigir_permiso("carpeta_compartida")?;
    Ok(eventos::carpeta_compartida(&carpeta_abierta(&estado)?)?
        .map(|r| r.to_string_lossy().to_string()))
}

/// Guarda la carpeta compartida del proyecto abierto en esta máquina y devuelve
/// la ruta completa que quedó guardada.
#[tauri::command]
pub fn configurar_carpeta_compartida(
    estado: State<'_, AppState>,
    ruta: String,
) -> Resultado<String> {
    let yo = estado.exigir_permiso("configurar_carpeta_compartida")?;
    // Solo el Administrador deja una carpeta nueva como la del proyecto (ver `eventos`).
    let guardada = eventos::configurar_carpeta_compartida(
        &carpeta_abierta(&estado)?,
        &ruta,
        yo.rol == Rol::Administrador,
    )?;
    Ok(guardada.to_string_lossy().to_string())
}

/// Escanea `ops/` y aplica lo que le falta a esta máquina (#92). El frontend lo
/// llama al entrar al proyecto, al volver a primer plano y cada 2 minutos. Si ya
/// hay un escaneo en curso, no hace nada. Sin carpeta compartida, tampoco (se
/// pide al entrar).
#[tauri::command]
pub fn sincronizar_eventos(estado: State<'_, AppState>) -> Resultado<ResumenSincronizacion> {
    estado.exigir_permiso("sincronizar_eventos")?;
    let Some(_escaneo) = estado.iniciar_sincronizacion() else {
        return Ok(ResumenSincronizacion::default());
    };
    let carpeta = carpeta_abierta(&estado)?;
    if eventos::carpeta_compartida(&carpeta)?.is_none() {
        return Ok(ResumenSincronizacion::default());
    }
    let ops = eventos::carpeta_ops(&carpeta)?;
    estado.con_db(|db| {
        eventos::sincronizar(
            db.conexion(),
            &ops,
            &carpeta,
            Utc::now(),
            eventos::lectura::aplicar_evento,
        )
    })
}
