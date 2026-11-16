//! Carpeta compartida del proyecto (Drive/OneDrive) en esta máquina, donde vive
//! `ops/` (issue #91), escaneo de `ops/` (#92) y compactación con snapshot (#94).

use chrono::Utc;
use serde::Serialize;
use tauri::State;

use crate::checkins;
use crate::cm::lectura::rama_actual_workspace;
use crate::comandos::datos::carpeta_abierta;
use crate::db::tareas as db_tareas;
use crate::error::{Error, Resultado};
use crate::estado::{llave_publica_del_proyecto, AppState};
use crate::eventos::{self, ResumenCompactacion, ResumenSincronizacion};
use crate::identidad::equipo::Firmante;
use crate::modelo::{ahora_iso, hoy_local, normalizar_email, EstadoTarea, Rol};
use crate::ramas::{self, RAMA_DEV};
use crate::servidor::{conectar, Changeset, LlaveroSistema};
use crate::tareas::{self, AutorTarea, InfoDobleToma};

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

/// Evento Tauri emitido al detectar cambios nuevos o cierres en Unity VC (#104).
pub const EVENTO_UNITY_ACTUALIZADO: &str = "unity-vc-actualizado";

/// Resumen de una revisión en Unity VC (#104).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResumenPolling {
    pub rama_actual: Option<String>,
    pub tareas_modificadas: Vec<String>,
    pub tareas_cerradas: Vec<String>,
}

impl ResumenPolling {
    pub fn hubo_cambios(&self) -> bool {
        self.rama_actual.is_some()
            || !self.tareas_modificadas.is_empty()
            || !self.tareas_cerradas.is_empty()
    }
}

/// Revisa las ramas activas en Unity VC, detecta check-ins nuevos (`modificada`),
/// cierres por merge (`tarea_cerrada_por_deteccion`) y la rama actual del workspace (#104).
#[tauri::command]
pub async fn revisar_actualizaciones(
    estado: State<'_, AppState>,
    tarea_id: Option<String>,
) -> Resultado<ResumenPolling> {
    revisar_actualizaciones_en(&estado, tarea_id.as_deref())
}

/// Versión sin envoltorio Tauri de [`revisar_actualizaciones`] (para el polling
/// en segundo plano y pruebas reales).
pub fn revisar_actualizaciones_en(
    estado: &AppState,
    tarea_id: Option<&str>,
) -> Resultado<ResumenPolling> {
    let yo = estado.exigir_permiso("revisar_actualizaciones")?;
    let Some(_guarda) = estado.iniciar_polling() else {
        return Ok(ResumenPolling::default());
    };

    let mut resumen = ResumenPolling::default();
    if let Ok(ws) = estado.workspace_abierto() {
        resumen.rama_actual = rama_actual_workspace(estado, &ws).ok().map(|w| w.rama);
    }

    let Ok(repo) = estado.repo_abierto() else {
        return Ok(resumen);
    };
    let filtro = tarea_id.map(str::trim).filter(|s| !s.is_empty());

    // 1. Snapshot rápido de tareas activas con rama asociada.
    let candidatas: Vec<(String, String, Option<i64>, String)> = estado.con_db(|db| {
        let todas = db_tareas::listar(db.conexion())?;
        let mut lista = Vec::new();
        for t in &todas {
            if t.eliminada || t.estado == EstadoTarea::Terminada {
                continue;
            }
            if let Some(id_buscado) = filtro {
                if t.id != id_buscado {
                    continue;
                }
            }
            let Some(ref rama) = t.rama_asociada else {
                continue;
            };
            let rama_padre = t
                .tarea_padre_id
                .as_deref()
                .and_then(|pid| todas.iter().find(|p| p.id == pid))
                .and_then(|p| p.rama_asociada.clone())
                .unwrap_or_else(|| RAMA_DEV.to_string());
            lista.push((
                t.id.clone(),
                rama.clone(),
                t.ultimo_changeset_visto,
                rama_padre,
            ));
        }
        Ok(lista)
    })?;

    if candidatas.is_empty() {
        return Ok(resumen);
    }

    // 2. Consultas de red / `cm` fuera del lock de SQLite.
    let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    let mut resultados: Vec<(String, Option<i64>, Vec<Changeset>, bool)> =
        Vec::with_capacity(candidatas.len());

    for (id, rama, ultimo_cs, rama_padre) in candidatas {
        let changesets = cliente
            .changesets_de_rama(&repo.repo, &rama)?
            .unwrap_or_default();
        let cerrada =
            ramas::rama_cerrada_hacia_padre(estado, &repo_cm, &rama, &rama_padre).unwrap_or(false);
        resultados.push((id, ultimo_cs, changesets, cerrada));
    }

    // 3. Aplicar resultados en SQLite (y `ops/` si se cerró alguna rama).
    let ops = carpeta_abierta(estado)
        .ok()
        .and_then(|carpeta| eventos::carpeta_ops(&carpeta).ok());
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    let ahora = ahora_iso();
    let hoy = hoy_local();

    estado.con_db(|db| {
        for (id, ultimo_cs, changesets, cerrada) in resultados {
            let nuevos = checkins::guardar_changesets(db.conexion(), &id, &changesets)?;
            let max_cs = changesets.iter().map(|c| c.changeset_id).max();
            if max_cs > ultimo_cs || nuevos > 0 {
                let hay_nuevos = max_cs
                    .zip(ultimo_cs)
                    .map(|(m, u)| m > u)
                    .unwrap_or(nuevos > 0);
                if tareas::actualizar_deteccion_local(db.conexion(), &id, max_cs, hay_nuevos)?
                    && hay_nuevos
                {
                    resumen.tareas_modificadas.push(id.clone());
                }
            }
            if cerrada {
                if let Some(ref carpeta_ops) = ops {
                    tareas::cerrar_por_deteccion(
                        db.conexion(),
                        carpeta_ops,
                        &autor,
                        &id,
                        &ahora,
                        &hoy,
                    )?;
                    resumen.tareas_cerradas.push(id);
                }
            }
        }
        Ok(())
    })?;

    Ok(resumen)
}
