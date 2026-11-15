//! Carpeta compartida del proyecto (Drive/OneDrive) en esta máquina, donde vive
//! `ops/` (issue #91), escaneo de `ops/` (#92) y compactación con snapshot (#94).

use chrono::Utc;
use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::error::{Error, Resultado};
use crate::estado::{llave_publica_del_proyecto, AppState};
use crate::eventos::{self, ResumenCompactacion, ResumenSincronizacion};
use crate::identidad::equipo::Firmante;
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
        // Con la llave del Administrador se verifican los eventos firmados (#93).
        let publica = llave_publica_del_proyecto(db)?;
        eventos::sincronizar(db.conexion(), &ops, &carpeta, Utc::now(), |c, e| {
            eventos::lectura::aplicar_evento(c, &publica, e)
        })
    })
}

/// Junta los eventos de más de 30 días en `ops/snapshot_<fecha>.json` (firmado
/// con la llave del Administrador) y los borra de `ops/` (#94). Solo
/// Administrador; no corre solo.
#[tauri::command]
pub fn compactar_eventos(
    estado: State<'_, AppState>,
    passphrase: Option<String>,
) -> Resultado<ResumenCompactacion> {
    let yo = estado.exigir_permiso("compactar_eventos")?;
    let Some(_escaneo) = estado.iniciar_sincronizacion() else {
        return Err(Error::Validacion(
            "Hay una sincronización de eventos en curso. Intenta de nuevo en unos segundos.".into(),
        ));
    };
    let carpeta = carpeta_abierta(&estado)?;
    let ops = eventos::carpeta_ops(&carpeta)?;
    estado.con_db(|db| {
        let publica = llave_publica_del_proyecto(db)?;
        let llave = estado.llave_firma(db, &yo, passphrase.as_deref())?;
        let firmante = Firmante {
            email: &yo.email,
            rol: yo.rol,
            llave: &llave,
        };
        eventos::compactar(db.conexion(), &ops, &publica, &firmante, Utc::now())
    })
}
