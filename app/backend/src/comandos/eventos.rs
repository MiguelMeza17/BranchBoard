//! Carpeta compartida del proyecto (Drive/OneDrive) en esta máquina, donde vive `ops/` (issue #91),
//! escaneo de `ops/` (#92) y sincronización con el servidor (#238, #239). La compactación con
//! snapshots firmados (#94) se quitó con las firmas en #243.

use chrono::Utc;
use serde::Serialize;
use tauri::State;

use crate::checkins;
use crate::cm::lectura::rama_actual_workspace;
use crate::comandos::datos::carpeta_abierta;
use crate::db::configuracion::{self, CLAVE_PROYECTO_SERVIDOR};
use crate::db::{commits, tareas as db_tareas};
use crate::eventos::recepcion;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos::{self, ResumenSincronizacion};
use crate::modelo::{ahora_iso, hoy_local, normalizar_email, EstadoTarea, Rol, TipoTarea};
use crate::ramas::{self, RAMA_DEV};
use crate::servidor::{conectar, Changeset, LlaveroSistema};
use crate::tareas::{self, AlertaRamaAjena, AutorTarea, InfoDobleToma};

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

/// Trae los cambios del equipo y los aplica: los de `ops/` si hay carpeta compartida (#92) y los del
/// servidor si el proyecto está vinculado (#239; antes sube la bandeja de salida). El frontend lo llama
/// al entrar al proyecto, al volver a primer plano y cada 2 minutos; además el backend consulta el
/// servidor cada 5 s (ver `lib.rs`). Si ya hay una sincronización en curso, no hace nada.
#[tauri::command]
pub fn sincronizar_eventos(estado: State<'_, AppState>) -> Resultado<ResumenSincronizacion> {
    let yo = estado.exigir_permiso("sincronizar_eventos")?;
    let Some(_escaneo) = estado.iniciar_sincronizacion() else {
        return Ok(ResumenSincronizacion::default());
    };
    let carpeta = carpeta_abierta(&estado)?;
    let mut resumen = if eventos::carpeta_compartida(&carpeta)?.is_some() {
        sincronizar_ops(&estado, &yo.email, &carpeta)?
    } else {
        ResumenSincronizacion::default()
    };
    // Sin internet o sin sesión del servidor no es un error de este comando: se reintenta en 5 s.
    if let Ok(servidor) = sincronizar_con_servidor_sin_guardia(&estado) {
        resumen.sumar(&servidor.recibidos);
    }
    let _ = crate::comandos::sprints::evaluar_cierres_en(&estado, &yo.email)?;
    Ok(resumen)
}

/// Escaneo de `ops/` (#92), hasta que #244 quite la carpeta compartida.
fn sincronizar_ops(
    estado: &AppState,
    email: &str,
    carpeta: &std::path::Path,
) -> Resultado<ResumenSincronizacion> {
    let ops = eventos::carpeta_ops(carpeta)?;
    estado.con_db(|db| {
        eventos::lectura::sincronizar_con_doble_toma(
            db.conexion(),
            &ops,
            carpeta,
            Utc::now(),
            eventos::lectura::aplicar_evento,
            |info| revisar_y_cerrar_rama_perdedor(estado, email, info),
        )
    })
}

/// Evento Tauri emitido cuando llegaron y se aplicaron cambios del equipo desde el servidor (#239).
/// El frontend sube `versionDatos` para que las pantallas vuelvan a pedir sus datos.
pub const EVENTO_EVENTOS_RECIBIDOS: &str = "eventos-recibidos";

/// Cada cuánto se consulta el servidor mientras la app está abierta (decisión de #239: ~5 s).
pub const INTERVALO_SERVIDOR_SEGUNDOS: u64 = 5;

/// Resultado de una vuelta con el servidor (#238 + #239).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResumenServidor {
    /// Cómo quedó la bandeja de salida después de subir.
    pub bandeja: eventos::bandeja::EstadoBandeja,
    /// Lo que se aplicó de lo que bajó.
    pub recibidos: ResumenSincronizacion,
    /// Si no se pudo bajar (sin internet, sin sesión, servidor pausado…). Lo que ya estaba en la copia
    /// local igual se intenta aplicar.
    pub error_bajada: Option<String>,
}

/// Una vuelta con el servidor: sube la bandeja de salida y después baja y aplica lo nuevo. `None` si ya
/// hay una sincronización en curso.
pub fn sincronizar_con_servidor_en(estado: &AppState) -> Resultado<Option<ResumenServidor>> {
    let Some(_escaneo) = estado.iniciar_sincronizacion() else {
        return Ok(None);
    };
    sincronizar_con_servidor_sin_guardia(estado).map(Some)
}

/// Como [`sincronizar_con_servidor_en`], para quien ya tiene la guardia de sincronización.
///
/// Primero se sube y después se baja, en la misma vuelta y con la misma guardia: así una toma propia
/// está o en la bandeja o ya en el servidor, nunca en el medio (lo necesita la doble toma, ver
/// `tareas::buscar_toma_rival_servidor`).
fn sincronizar_con_servidor_sin_guardia(estado: &AppState) -> Resultado<ResumenServidor> {
    let proyecto = estado
        .con_db(|db| configuracion::leer(db.conexion(), CLAVE_PROYECTO_SERVIDOR))?
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let bandeja = subir_bandeja_en(estado)?;
    let mut resumen = ResumenServidor {
        bandeja,
        ..Default::default()
    };
    let Some(proyecto) = proyecto else {
        return Ok(resumen);
    };
    let cliente = crate::supabase::ClienteSupabase::new(
        crate::supabase::Entorno::actual().proyecto(),
        LlaveroSistema,
    );
    if let Err(e) = recepcion::bajar(
        |f| estado.con_db(|db| f(db.conexion())),
        &proyecto,
        |desde| {
            cliente
                .llamar("GET", &recepcion::ruta_eventos(&proyecto, desde), None)
                .and_then(recepcion::filas_desde_json)
        },
        |ids| {
            cliente
                .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                .and_then(recepcion::correos_desde_json)
        },
    ) {
        resumen.error_bajada = Some(e.to_string());
    }
    let email = estado.exigir_permiso("sincronizar_eventos").ok().map(|yo| yo.email);
    let carpeta = carpeta_abierta(estado).ok();
    resumen.recibidos = estado.con_db(|db| {
        recepcion::aplicar(
            db.conexion(),
            &proyecto,
            carpeta.as_deref(),
            Utc::now(),
            eventos::lectura::aplicar_evento,
            |info| match &email {
                Some(email) => revisar_y_cerrar_rama_perdedor(estado, email, info),
                None => Ok(false),
            },
        )
    })?;
    Ok(resumen)
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

/// Evento Tauri con el estado de la bandeja de salida después de cada intento de subida (#238).
pub const EVENTO_BANDEJA_SALIDA: &str = "bandeja-salida";

/// Sube al servidor lo que espera en la bandeja de salida del proyecto abierto (#238). Lo llama el
/// temporizador de 15 s. Sin proyecto abierto da error (no hay bandeja).
pub fn subir_bandeja_en(estado: &AppState) -> Resultado<eventos::bandeja::EstadoBandeja> {
    let cliente = crate::supabase::ClienteSupabase::new(
        crate::supabase::Entorno::actual().proyecto(),
        LlaveroSistema,
    );
    eventos::bandeja::subir(
        |f| estado.con_db(|db| f(db.conexion())),
        |filas| cliente.insertar_eventos(filas),
    )
}

/// Cuántos cambios del proyecto abierto esperan subir al servidor (#238). Sin proyecto abierto, 0.
#[tauri::command]
pub async fn eventos_por_subir(estado: State<'_, AppState>) -> Resultado<u64> {
    match estado.con_db(|db| eventos::bandeja::pendientes(db.conexion())) {
        Err(Error::Validacion(_)) => Ok(0),
        otro => otro,
    }
}

/// Evento Tauri emitido al detectar cambios nuevos o cierres en Unity VC (#104).
pub const EVENTO_UNITY_ACTUALIZADO: &str = "unity-vc-actualizado";

/// Evento Tauri emitido al detectar cambios significativos en una rama ajena o en `dev` (#111).
pub const EVENTO_RAMA_AJENA: &str = "alerta-rama-ajena";

/// Resumen de una revisión en Unity VC (#104, #106, #109, #111).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResumenPolling {
    pub rama_actual: Option<String>,
    pub tareas_modificadas: Vec<String>,
    pub tareas_cerradas: Vec<String>,
    pub espera_creada: Option<String>,
    pub espera_quitada: Option<String>,
    pub tocados_detectados: Vec<String>,
    pub alerta_rama_ajena: Option<AlertaRamaAjena>,
    pub sprints_actualizados: Vec<String>,
    pub inactividad_notificada: Vec<String>,
    /// Tareas rojas creadas o con check-ins directos nuevos en `/main` o `/main/dev` (#225).
    pub cambios_directos: Vec<String>,
}

impl ResumenPolling {
    pub fn hubo_cambios(&self) -> bool {
        !self.cambios_directos.is_empty()
            || self.rama_actual.is_some()
            || !self.tareas_modificadas.is_empty()
            || !self.tareas_cerradas.is_empty()
            || self.espera_creada.is_some()
            || self.espera_quitada.is_some()
            || !self.tocados_detectados.is_empty()
            || self.alerta_rama_ajena.is_some()
            || !self.sprints_actualizados.is_empty()
            || !self.inactividad_notificada.is_empty()
    }
}

/// Revisa las ramas activas en Unity VC, detecta check-ins nuevos (`modificada`),
/// cierres por merge (`tarea_cerrada_por_deteccion`), cambios ajenos tocados (#109),
/// la subtarea `"En espera"` y la rama actual (#104, #106).
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
    let ws_abierto = estado.workspace_abierto().ok();
    if let Some(ref ws) = ws_abierto {
        resumen.rama_actual = rama_actual_workspace(estado, ws).ok().map(|w| w.rama);
    }
    let pendientes_ws = if resumen.rama_actual.is_some() {
        ws_abierto
            .as_deref()
            .and_then(|ws| ramas::cambios_pendientes(estado, ws).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let Ok(repo) = estado.repo_abierto() else {
        resumen.sprints_actualizados =
            crate::comandos::sprints::evaluar_cierres_en(estado, &yo.email)?;
        return Ok(resumen);
    };
    let filtro = tarea_id.map(str::trim).filter(|s| !s.is_empty());

    // 1. Snapshot rápido de tareas activas con rama asociada (y piso de préstamos vigentes).
    type CandidataPolling = (String, String, Option<i64>, String, Option<i64>);
    let candidatas: Vec<CandidataPolling> = estado.con_db(|db| {
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
            let vigentes = commits::prestamos_vigentes(db.conexion(), &t.id)?;
            let piso_prestamos = (!vigentes.is_empty()).then(|| {
                vigentes
                    .iter()
                    .filter_map(|p| {
                        p.changeset_limpio
                            .into_iter()
                            .chain(p.changeset_merge)
                            .max()
                    })
                    .min()
                    .unwrap_or(0)
            });
            lista.push((
                t.id.clone(),
                rama.clone(),
                t.ultimo_changeset_visto,
                rama_padre,
                piso_prestamos,
            ));
        }
        Ok(lista)
    })?;

    // 2. Consultas de red / `cm` fuera del lock de SQLite.
    let yo_email = normalizar_email(&yo.email);
    type ResultadoPolling = (
        String,
        String,
        Option<i64>,
        Vec<Changeset>,
        bool,
        Vec<(i64, Vec<String>)>,
    );
    let mut resultados: Vec<ResultadoPolling> = Vec::with_capacity(candidatas.len());
    let mut commits_recientes_rama_ws: Vec<String> = Vec::new();
    if !candidatas.is_empty() {
        let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
        let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
        for (id, rama, ultimo_cs, rama_padre, piso_prestamos) in candidatas {
            let changesets = cliente
                .changesets_de_rama(&repo.repo, &rama)?
                .unwrap_or_default();
            let cerrada = ramas::rama_cerrada_hacia_padre(estado, &repo_cm, &rama, &rama_padre)
                .unwrap_or(false);
            let mut diffs_prestamos = Vec::new();
            if let Some(piso_min) = piso_prestamos {
                for cs in &changesets {
                    if cs.changeset_id > piso_min {
                        let rutas = ramas::archivos_changeset(
                            estado,
                            ws_abierto.as_deref(),
                            &repo_cm,
                            cs.changeset_id,
                        )
                        .unwrap_or_default();
                        diffs_prestamos.push((cs.changeset_id, rutas));
                    }
                }
            }
            let es_rama_ws = resumen
                .rama_actual
                .as_deref()
                .map(|r| r.eq_ignore_ascii_case(&rama))
                .unwrap_or(false);
            if es_rama_ws {
                for cs in &changesets {
                    let es_nuevo = ultimo_cs.map(|u| cs.changeset_id > u).unwrap_or(false);
                    if es_nuevo && normalizar_email(&cs.owner) == yo_email {
                        if let Some((_, existentes)) = diffs_prestamos
                            .iter()
                            .find(|(cid, _)| *cid == cs.changeset_id)
                        {
                            commits_recientes_rama_ws.extend(existentes.iter().cloned());
                        } else if let Ok(rutas) = ramas::archivos_changeset(
                            estado,
                            ws_abierto.as_deref(),
                            &repo_cm,
                            cs.changeset_id,
                        ) {
                            commits_recientes_rama_ws.extend(rutas);
                        }
                    }
                }
            }
            resultados.push((id, rama, ultimo_cs, changesets, cerrada, diffs_prestamos));
        }
    }

    // 3. Aplicar resultados en SQLite (y `ops/` si se cerró alguna rama o hay cambios En espera).
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
        for (id, rama, ultimo_cs, changesets, cerrada, diffs_prestamos) in resultados {
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
            let en_esta_rama = resumen
                .rama_actual
                .as_deref()
                .map(|r| r.eq_ignore_ascii_case(&rama))
                .unwrap_or(false);
            let pendientes_rama = en_esta_rama.then_some(pendientes_ws.as_slice());
            if tareas::actualizar_tocados_de_prestamos(
                db.conexion(),
                &id,
                pendientes_rama,
                &diffs_prestamos,
            )? {
                resumen.tocados_detectados.push(id.clone());
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

        if let Some(ref rama_ws) = resumen.rama_actual {
            let todas_ahora = db_tareas::listar(db.conexion())?;
            resumen.alerta_rama_ajena = tareas::evaluar_alerta_rama_ajena(
                &todas_ahora,
                &yo_email,
                rama_ws,
                &pendientes_ws,
                &commits_recientes_rama_ws,
            );
            if let Some(padre) = todas_ahora.iter().find(|t| {
                !t.eliminada
                    && t.estado != EstadoTarea::Terminada
                    && filtro.map_or(true, |f| f == t.id)
                    && t.rama_asociada
                        .as_deref()
                        .map(|r| r.eq_ignore_ascii_case(rama_ws))
                        .unwrap_or(false)
            }) {
                let hijas_padre: Vec<&db_tareas::Tarea> = todas_ahora
                    .iter()
                    .filter(|h| {
                        !h.eliminada && h.tarea_padre_id.as_deref() == Some(padre.id.as_str())
                    })
                    .collect();
                let tiene_normales = hijas_padre.iter().any(|h| h.tipo != TipoTarea::SinDefinir);
                let espera_existente = hijas_padre
                    .iter()
                    .copied()
                    .find(|h| h.tipo == TipoTarea::SinDefinir);

                if tiene_normales {
                    if let Some(ref carpeta_ops) = ops {
                        if !pendientes_ws.is_empty() && espera_existente.is_none() {
                            let creada = tareas::crear_en_espera(
                                db.conexion(),
                                carpeta_ops,
                                &autor,
                                &padre.id,
                                pendientes_ws.len(),
                                &ahora,
                                &hoy,
                            )?;
                            resumen.espera_creada = Some(creada.id);
                        } else if pendientes_ws.is_empty() {
                            if let Some(esp) = espera_existente {
                                let mia = esp.asignado_a.as_deref() == Some(yo_email.as_str())
                                    || esp.creado_por == yo_email;
                                if mia {
                                    tareas::quitar_en_espera(
                                        db.conexion(),
                                        carpeta_ops,
                                        &autor,
                                        &esp.id,
                                        &ahora,
                                    )?;
                                    resumen.espera_quitada = Some(esp.id.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(ref carpeta_ops) = ops {
            let inactivas = crate::notificaciones::evaluar_inactividad(
                db.conexion(),
                carpeta_ops,
                &yo.email,
                filtro,
                &ahora,
                &hoy,
            )?;
            resumen.inactividad_notificada =
                inactivas.into_iter().filter_map(|n| n.tarea_id).collect();
        }

        Ok(())
    })?;

    // 4. Check-ins directos en `/main` y `/main/dev`, leídos del servidor (#225).
    resumen.cambios_directos = crate::cambios_directos::revisar_en(estado, &yo.email)?;

    resumen.sprints_actualizados = crate::comandos::sprints::evaluar_cierres_en(estado, &yo.email)?;
    Ok(resumen)
}
