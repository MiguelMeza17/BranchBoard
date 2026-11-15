//! Carpeta compartida del proyecto (Drive/OneDrive) en esta máquina, donde vive
//! `ops/` (issue #91), escaneo de `ops/` (#92) y compactación con snapshot (#94).

use chrono::Utc;
use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::error::{Error, Resultado};
use crate::estado::{llave_publica_del_proyecto, AppState};
use crate::eventos::{self, ResumenCompactacion, ResumenSincronizacion};
use crate::identidad::equipo::Firmante;
use crate::modelo::{normalizar_email, Rol};
use crate::ramas;
use crate::servidor::{conectar, LlaveroSistema};
use crate::tareas::InfoDobleToma;

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
    let yo = estado.exigir_permiso("sincronizar_eventos")?;
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
        eventos::lectura::sincronizar_con_doble_toma(
            db.conexion(),
            &ops,
            &carpeta,
            Utc::now(),
            &publica,
            |c, e| eventos::lectura::aplicar_evento(c, &publica, e),
            |info| revisar_y_cerrar_rama_perdedor(&estado, &yo.email, info),
        )
    })
}

/// Comprueba en Unity VC si la rama del perdedor en una doble toma (#97)
/// tiene changesets reales más allá del inicial. Si no tiene y es distinta a la
/// del ganador, la borra/cierra en el servidor.
pub fn revisar_y_cerrar_rama_perdedor(
    estado: &AppState,
    email: &str,
    info: &InfoDobleToma,
) -> Resultado<bool> {
    let Ok(repo) = estado.repo_abierto() else {
        return Ok(false);
    };
    let cliente = conectar(estado, &LlaveroSistema, email, &repo.org)?;
    let cs_inicial = info.cs_inicial_perdedor.unwrap_or(0);
    let changesets = cliente
        .changesets_de_rama(&repo.repo, &info.rama_perdedor)?
        .unwrap_or_default();
    let misma_rama = info.rama_perdedor == info.rama_ganador;
    let tiene_changesets = changesets.iter().any(|cs| {
        cs.changeset_id > cs_inicial
            && (!misma_rama || normalizar_email(&cs.owner) == info.perdedor)
    });
    if !tiene_changesets && !misma_rama && cliente.rama(&repo.repo, &info.rama_perdedor)?.is_some()
    {
        let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
        ramas::borrar_rama(estado, &repo_cm, &info.rama_perdedor)?;
    }
    Ok(tiene_changesets)
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
