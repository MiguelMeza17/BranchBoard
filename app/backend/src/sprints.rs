//! Reglas de planificación: creación y listado de sprints/backlogs y asignación
//! de tareas (issue #113). Cada mutación compartida pasa por `ops/` y se aplica
//! igual en todas las máquinas mediante [`aplicar_evento`].

use chrono::{Days, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::sprints::{self as db_sprints, ReporteSprint, Sprint};
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::{normalizar_email, EstadoTarea, Rol, TipoTarea};
use crate::tareas::{rol_de, AutorTarea};

/// Tipos de evento que maneja este módulo.
pub const TIPOS: &[&str] = &[
    "crear_sprint",
    "cerrar_sprint",
    "cerrar_sprint_automatico",
    "reporte_sprint_fecha_fin",
    "asignar_tarea_a_sprint",
    "asignar_tarea_a_backlog",
    "asignar_backlog_a_sprint",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PayloadCerrarSprint {
    sprint_id: String,
    fecha: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PayloadAsignarTareaGrupo {
    tarea_id: String,
    grupo_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PayloadAsignarBacklogASprint {
    backlog_id: Option<String>,
    sprint_id: String,
}

fn limpiar_opcional(valor: Option<String>) -> Option<String> {
    valor.and_then(|s| {
        let t = s.trim().to_string();
        (!t.is_empty()).then_some(t)
    })
}

fn exigir_lider_o_admin_autor(autor: &AutorTarea<'_>) -> Resultado<()> {
    if !autor.rol.es_lider_o_admin() {
        return Err(Error::SinPermiso);
    }
    Ok(())
}

fn exigir_lider_o_admin_evento(c: &Connection, email: &str) -> Resultado<()> {
    match rol_de(c, email)? {
        Some(rol) if rol.es_lider_o_admin() => Ok(()),
        _ => Err(Error::SinPermiso),
    }
}

fn exigir_admin_autor(autor: &AutorTarea<'_>) -> Resultado<()> {
    if autor.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }
    Ok(())
}

fn exigir_admin_evento(c: &Connection, email: &str) -> Resultado<()> {
    match rol_de(c, email)? {
        Some(Rol::Administrador) => Ok(()),
        _ => Err(Error::SinPermiso),
    }
}

fn exigir_miembro_evento(c: &Connection, email: &str) -> Resultado<()> {
    if rol_de(c, email)?.is_none() {
        return Err(Error::SinPermiso);
    }
    Ok(())
}

fn obtener_con_reporte(c: &Connection, id: &str) -> Resultado<Sprint> {
    let mut sprint = db_sprints::obtener(c, id)?;
    sprint.reporte = db_sprints::ultimo_reporte(c, id)?.map(|r| r.tareas);
    Ok(sprint)
}

fn snapshot_tareas_grupo(c: &Connection, sprint: &Sprint) -> Resultado<Vec<Tarea>> {
    Ok(db_tareas::listar(c)?
        .into_iter()
        .filter(|t| {
            !t.eliminada
                && if sprint.con_fecha {
                    t.sprint_id.as_deref() == Some(sprint.id.as_str())
                } else {
                    t.backlog_id.as_deref() == Some(sprint.id.as_str())
                }
        })
        .collect())
}

fn liberar_tareas_sin_terminar_de_sprint(c: &Connection, sprint_id: &str) -> Resultado<()> {
    for mut t in db_tareas::listar(c)? {
        if t.sprint_id.as_deref() == Some(sprint_id) && t.estado != EstadoTarea::Terminada {
            t.sprint_id = None;
            db_tareas::actualizar(c, &t)?;
        }
    }
    Ok(())
}

/// Suma `dias` días calendario a `fecha` (`AAAA-MM-DD`).
pub fn sumar_dias(fecha: &str, dias: u32) -> Resultado<String> {
    let base = NaiveDate::parse_from_str(fecha.trim(), "%Y-%m-%d")
        .map_err(|_| Error::Validacion(format!("Fecha no válida: {fecha}")))?;
    let resultado = base
        .checked_add_days(Days::new(u64::from(dias)))
        .ok_or_else(|| Error::Validacion("La duración del sprint excede el rango válido".into()))?;
    Ok(resultado.format("%Y-%m-%d").to_string())
}

/// Lista todos los sprints y backlogs en orden de creación (#113).
pub fn listar(c: &Connection) -> Resultado<Vec<Sprint>> {
    db_sprints::listar(c)
}

/// Datos para crear un sprint (`con_fecha = true`) o backlog (`con_fecha = false`).
#[derive(Debug, Clone)]
pub struct DatosNuevoSprint<'a> {
    pub nombre: &'a str,
    pub descripcion: &'a str,
    pub con_fecha: bool,
    pub duracion_dias: Option<u32>,
}

/// Crea un sprint (`con_fecha = true`, con `duracion_dias >= 1` y `fecha_fin` calculada)
/// o un backlog (`con_fecha = false`, sin duración ni fecha de fin), solo Líder/Admin (#113).
pub fn crear_sprint(
    c: &Connection,
    autor: &AutorTarea<'_>,
    datos: &DatosNuevoSprint<'_>,
    ahora: &str,
    hoy: &str,
) -> Resultado<Sprint> {
    exigir_lider_o_admin_autor(autor)?;
    let nombre = datos.nombre.trim();
    if nombre.is_empty() {
        return Err(Error::Validacion("El nombre es obligatorio".into()));
    }
    let (duracion, fecha_fin) = if datos.con_fecha {
        let dias = datos.duracion_dias.filter(|&d| d > 0).ok_or_else(|| {
            Error::Validacion("Un sprint necesita una duración mayor a cero días".into())
        })?;
        (Some(dias), Some(sumar_dias(hoy, dias)?))
    } else {
        if datos.duracion_dias.is_some() {
            return Err(Error::Validacion(
                "Un backlog no tiene duración ni fecha de fin".into(),
            ));
        }
        (None, None)
    };

    let sprint = Sprint {
        id: db_sprints::siguiente_id(c, datos.con_fecha)?,
        nombre: nombre.to_string(),
        descripcion: datos.descripcion.trim().to_string(),
        con_fecha: datos.con_fecha,
        fecha_inicio: hoy.to_string(),
        duracion_dias: duracion,
        fecha_fin,
        cerrado: false,
        fecha_cierre_real: None,
        creado_por: normalizar_email(autor.email),
        reporte: None,
    };

    let mut ev = Evento::nuevo(
        "crear_sprint",
        Some(&sprint.id),
        &normalizar_email(autor.email),
        serde_json::to_value(&sprint)
            .map_err(|e| Error::Interno(format!("no se pudo serializar crear_sprint: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_crear_sprint(tx, &sprint))?;
    db_sprints::obtener(c, &sprint.id)
}

/// Asigna una tarea a `sprint_id` (`None` la quita de su sprint actual), solo Líder/Admin (#113).
/// Rechaza asignarla si ya pertenece a otro sprint.
pub fn asignar_tarea_a_sprint(
    c: &Connection,
    autor: &AutorTarea<'_>,
    tarea_id: &str,
    sprint_id: Option<&str>,
    ahora: &str,
) -> Resultado<Tarea> {
    exigir_lider_o_admin_autor(autor)?;
    let tarea_id = tarea_id.trim();
    let payload = PayloadAsignarTareaGrupo {
        tarea_id: tarea_id.to_string(),
        grupo_id: limpiar_opcional(sprint_id.map(str::to_string)),
    };
    let mut ev = Evento::nuevo(
        "asignar_tarea_a_sprint",
        Some(tarea_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar asignar_tarea_a_sprint: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_asignar_tarea_a_sprint(tx, &payload))?;
    db_tareas::obtener(c, tarea_id)
}

/// Asigna una tarea a `backlog_id` (`None` la quita de su backlog actual), solo Líder/Admin (#113).
/// Rechaza asignarla si ya pertenece a otro backlog.
pub fn asignar_tarea_a_backlog(
    c: &Connection,
    autor: &AutorTarea<'_>,
    tarea_id: &str,
    backlog_id: Option<&str>,
    ahora: &str,
) -> Resultado<Tarea> {
    exigir_lider_o_admin_autor(autor)?;
    let tarea_id = tarea_id.trim();
    let payload = PayloadAsignarTareaGrupo {
        tarea_id: tarea_id.to_string(),
        grupo_id: limpiar_opcional(backlog_id.map(str::to_string)),
    };
    let mut ev = Evento::nuevo(
        "asignar_tarea_a_backlog",
        Some(tarea_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar asignar_tarea_a_backlog: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_asignar_tarea_a_backlog(tx, &payload))?;
    db_tareas::obtener(c, tarea_id)
}

/// Pasa a `sprint_id` todas las tareas libres (`sprint_id = null`, no eliminadas y no `En espera`)
/// que están en `backlog_id` (`None` = tareas sin backlog), solo Líder/Admin (#113).
pub fn asignar_backlog_a_sprint(
    c: &Connection,
    autor: &AutorTarea<'_>,
    backlog_id: Option<&str>,
    sprint_id: &str,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    exigir_lider_o_admin_autor(autor)?;
    let backlog = limpiar_opcional(backlog_id.map(str::to_string));
    let sprint_id = sprint_id.trim().to_string();
    let payload = PayloadAsignarBacklogASprint {
        backlog_id: backlog,
        sprint_id: sprint_id.clone(),
    };
    validar_asignacion_backlog_a_sprint(c, &payload)?;

    let elegibles: Vec<String> = tareas_libres_de_backlog(c, payload.backlog_id.as_deref())?
        .into_iter()
        .map(|t| t.id)
        .collect();
    if elegibles.is_empty() {
        return Ok(Vec::new());
    }

    let mut ev = Evento::nuevo(
        "asignar_backlog_a_sprint",
        Some(&sprint_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar asignar_backlog_a_sprint: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_asignar_backlog_a_sprint(tx, &payload))?;

    let mut actualizadas = Vec::with_capacity(elegibles.len());
    for id in elegibles {
        actualizadas.push(db_tareas::obtener(c, &id)?);
    }
    Ok(actualizadas)
}

/// Cierra manualmente un sprint (incluso antes de su `fecha_fin`) o un backlog,
/// guardando la copia de sus tareas como reporte final y dejando en su backlog
/// (con `sprint_id = null`) las tareas no terminadas (#114). Exclusivo Administrador.
pub fn cerrar_sprint(
    c: &Connection,
    autor: &AutorTarea<'_>,
    sprint_id: &str,
    ahora: &str,
    hoy: &str,
) -> Resultado<Sprint> {
    exigir_admin_autor(autor)?;
    let id = sprint_id.trim();
    let payload = PayloadCerrarSprint {
        sprint_id: id.to_string(),
        fecha: hoy.to_string(),
    };
    let mut ev = Evento::nuevo(
        "cerrar_sprint",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar cerrar_sprint: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_cerrar_sprint(tx, &payload))?;
    obtener_con_reporte(c, id)
}

/// Revisa todos los sprints abiertos con fecha de fin vencida (`fecha_fin <= hoy`):
/// - si todas sus tareas activas están en `Terminada`, los cierra automáticamente
///   guardando el reporte final;
/// - si falta alguna tarea, genera el reporte de la fecha de fin (sin duplicar si
///   ya existía) y deja el sprint abierto hasta que terminen todas o lo cierre el
///   Administrador (#114). Backlogs (`con_fecha = false`) nunca se cierran solos.
pub fn evaluar_cierre_sprints(
    c: &Connection,
    usuario_email: &str,
    ahora: &str,
    hoy: &str,
) -> Resultado<Vec<String>> {
    let email = normalizar_email(usuario_email);
    let mut actualizados = Vec::new();
    for sprint in db_sprints::listar(c)? {
        if !sprint.con_fecha || sprint.cerrado {
            continue;
        }
        let Some(ref fecha_fin) = sprint.fecha_fin else {
            continue;
        };
        if hoy < fecha_fin.as_str() {
            continue;
        }
        let tareas_sprint = snapshot_tareas_grupo(c, &sprint)?;
        let todas_terminadas = tareas_sprint
            .iter()
            .all(|t| t.estado == EstadoTarea::Terminada);
        let previos = db_sprints::reportes(c, &sprint.id)?;

        if todas_terminadas {
            let max_comp = tareas_sprint
                .iter()
                .filter_map(|t| t.fecha_completada.as_deref())
                .max();
            let fecha_cierre = if previos.is_empty()
                && (max_comp.is_none() || max_comp <= Some(fecha_fin.as_str()))
            {
                fecha_fin.clone()
            } else if let Some(f) = max_comp.filter(|&f| f > fecha_fin.as_str() && f <= hoy) {
                f.to_string()
            } else {
                hoy.to_string()
            };
            let payload = PayloadCerrarSprint {
                sprint_id: sprint.id.clone(),
                fecha: fecha_cierre,
            };
            publicar_o_aplicar_cierre(
                c,
                &email,
                ahora,
                "cerrar_sprint_automatico",
                &payload,
                aplicar_cerrar_sprint_automatico,
            )?;
            actualizados.push(sprint.id);
        } else if !previos.iter().any(|r| !r.de_cierre) {
            let payload = PayloadCerrarSprint {
                sprint_id: sprint.id.clone(),
                fecha: fecha_fin.clone(),
            };
            publicar_o_aplicar_cierre(
                c,
                &email,
                ahora,
                "reporte_sprint_fecha_fin",
                &payload,
                aplicar_reporte_fecha_fin,
            )?;
            actualizados.push(sprint.id);
        }
    }
    Ok(actualizados)
}

/// Publica el cierre automático o el reporte del día como evento (sube al servidor por la bandeja si el
/// proyecto está vinculado, #238).
fn publicar_o_aplicar_cierre(
    c: &Connection,
    email: &str,
    ahora: &str,
    tipo: &str,
    payload: &PayloadCerrarSprint,
    aplicar: fn(&Connection, &PayloadCerrarSprint) -> Resultado<()>,
) -> Resultado<()> {
    let mut ev = Evento::nuevo(
        tipo,
        Some(&payload.sprint_id),
        email,
        serde_json::to_value(payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar {tipo}: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar(tx, payload))?;
    Ok(())
}

/// Reproduce un evento de planificación recibido de otra PC (#239).
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    match evento.tipo.as_str() {
        "crear_sprint" => {
            exigir_lider_o_admin_evento(c, &evento.usuario)?;
            let sprint: Sprint = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_sprint inválido: {e}")))?;
            aplicar_crear_sprint(c, &sprint)
        }
        "cerrar_sprint" => {
            exigir_admin_evento(c, &evento.usuario)?;
            let payload: PayloadCerrarSprint = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("cerrar_sprint inválido: {e}")))?;
            aplicar_cerrar_sprint(c, &payload)
        }
        "cerrar_sprint_automatico" => {
            exigir_miembro_evento(c, &evento.usuario)?;
            let payload: PayloadCerrarSprint = serde_json::from_value(evento.payload.clone())
                .map_err(|e| {
                    Error::Validacion(format!("cerrar_sprint_automatico inválido: {e}"))
                })?;
            aplicar_cerrar_sprint_automatico(c, &payload)
        }
        "reporte_sprint_fecha_fin" => {
            exigir_miembro_evento(c, &evento.usuario)?;
            let payload: PayloadCerrarSprint = serde_json::from_value(evento.payload.clone())
                .map_err(|e| {
                    Error::Validacion(format!("reporte_sprint_fecha_fin inválido: {e}"))
                })?;
            aplicar_reporte_fecha_fin(c, &payload)
        }
        "asignar_tarea_a_sprint" => {
            exigir_lider_o_admin_evento(c, &evento.usuario)?;
            let payload: PayloadAsignarTareaGrupo = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("asignar_tarea_a_sprint inválido: {e}")))?;
            aplicar_asignar_tarea_a_sprint(c, &payload)
        }
        "asignar_tarea_a_backlog" => {
            exigir_lider_o_admin_evento(c, &evento.usuario)?;
            let payload: PayloadAsignarTareaGrupo = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("asignar_tarea_a_backlog inválido: {e}")))?;
            aplicar_asignar_tarea_a_backlog(c, &payload)
        }
        "asignar_backlog_a_sprint" => {
            exigir_lider_o_admin_evento(c, &evento.usuario)?;
            let payload: PayloadAsignarBacklogASprint =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("asignar_backlog_a_sprint inválido: {e}"))
                })?;
            aplicar_asignar_backlog_a_sprint(c, &payload)
        }
        otro => Err(Error::Validacion(format!(
            "tipo de evento de planificación no soportado: {otro}"
        ))),
    }
}

fn aplicar_cerrar_sprint(c: &Connection, payload: &PayloadCerrarSprint) -> Resultado<()> {
    let mut sprint = db_sprints::obtener(c, payload.sprint_id.trim())?;
    if sprint.cerrado {
        return Err(Error::Validacion(format!(
            "El sprint {} ya está cerrado",
            sprint.id
        )));
    }
    let snapshot = snapshot_tareas_grupo(c, &sprint)?;
    let reporte = ReporteSprint {
        sprint_id: sprint.id.clone(),
        fecha: payload.fecha.clone(),
        de_cierre: true,
        tareas: snapshot,
    };
    db_sprints::guardar_reporte(c, &reporte)?;
    sprint.cerrado = true;
    sprint.fecha_cierre_real = Some(payload.fecha.clone());
    db_sprints::actualizar(c, &sprint)?;
    if sprint.con_fecha {
        liberar_tareas_sin_terminar_de_sprint(c, &sprint.id)?;
    }
    Ok(())
}

fn aplicar_cerrar_sprint_automatico(
    c: &Connection,
    payload: &PayloadCerrarSprint,
) -> Resultado<()> {
    let mut sprint = db_sprints::obtener(c, payload.sprint_id.trim())?;
    if !sprint.con_fecha {
        return Err(Error::Validacion(format!(
            "{} es un backlog y no se cierra automáticamente",
            sprint.id
        )));
    }
    if sprint.cerrado {
        return Ok(());
    }
    let Some(ref fecha_fin) = sprint.fecha_fin else {
        return Err(Error::Validacion("El sprint no tiene fecha de fin".into()));
    };
    if payload.fecha.as_str() < fecha_fin.as_str() {
        return Err(Error::Validacion(format!(
            "El sprint {} aún no llega a su fecha de fin ({fecha_fin})",
            sprint.id
        )));
    }
    let snapshot = snapshot_tareas_grupo(c, &sprint)?;
    if snapshot.iter().any(|t| t.estado != EstadoTarea::Terminada) {
        return Err(Error::Validacion(format!(
            "Todavía hay tareas sin terminar en {}",
            sprint.id
        )));
    }
    let reporte = ReporteSprint {
        sprint_id: sprint.id.clone(),
        fecha: payload.fecha.clone(),
        de_cierre: true,
        tareas: snapshot,
    };
    db_sprints::guardar_reporte(c, &reporte)?;
    sprint.cerrado = true;
    sprint.fecha_cierre_real = Some(payload.fecha.clone());
    db_sprints::actualizar(c, &sprint)?;
    liberar_tareas_sin_terminar_de_sprint(c, &sprint.id)?;
    Ok(())
}

fn aplicar_reporte_fecha_fin(c: &Connection, payload: &PayloadCerrarSprint) -> Resultado<()> {
    let sprint = db_sprints::obtener(c, payload.sprint_id.trim())?;
    if !sprint.con_fecha {
        return Err(Error::Validacion(format!(
            "{} es un backlog y no genera reporte por fecha de fin",
            sprint.id
        )));
    }
    if sprint.cerrado {
        return Ok(());
    }
    let Some(ref fecha_fin) = sprint.fecha_fin else {
        return Err(Error::Validacion("El sprint no tiene fecha de fin".into()));
    };
    if payload.fecha.as_str() != fecha_fin.as_str() {
        return Err(Error::Validacion(format!(
            "La fecha del reporte ({}) no coincide con fecha_fin ({fecha_fin})",
            payload.fecha
        )));
    }
    if db_sprints::reportes(c, &sprint.id)?
        .iter()
        .any(|r| !r.de_cierre)
    {
        return Ok(());
    }
    let snapshot = snapshot_tareas_grupo(c, &sprint)?;
    db_sprints::guardar_reporte(
        c,
        &ReporteSprint {
            sprint_id: sprint.id,
            fecha: payload.fecha.clone(),
            de_cierre: false,
            tareas: snapshot,
        },
    )
}

fn aplicar_crear_sprint(c: &Connection, sprint: &Sprint) -> Resultado<()> {
    if sprint.cerrado || sprint.fecha_cierre_real.is_some() {
        return Err(Error::Validacion(
            "Un sprint o backlog nuevo debe crearse abierto".into(),
        ));
    }
    db_sprints::insertar(c, sprint)
}

fn aplicar_asignar_tarea_a_sprint(
    c: &Connection,
    payload: &PayloadAsignarTareaGrupo,
) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, payload.tarea_id.trim())?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.tipo == TipoTarea::SinDefinir {
        return Err(Error::Validacion(
            "La subtarea En espera aún no puede asignarse a un sprint".into(),
        ));
    }
    if let Some(ref destino_id) = payload.grupo_id {
        let sprint = db_sprints::obtener(c, destino_id)?;
        if !sprint.con_fecha {
            return Err(Error::Validacion(format!(
                "{} es un backlog, no un sprint",
                sprint.id
            )));
        }
        if sprint.cerrado {
            return Err(Error::Validacion(format!(
                "El sprint {} ya está cerrado",
                sprint.id
            )));
        }
        if let Some(ref actual) = tarea.sprint_id {
            if actual != destino_id {
                return Err(Error::Validacion(
                    "La tarea ya está en otro sprint; primero hay que quitarla de ese".into(),
                ));
            }
        }
    }
    tarea.sprint_id = payload.grupo_id.clone();
    db_tareas::actualizar(c, &tarea)
}

fn aplicar_asignar_tarea_a_backlog(
    c: &Connection,
    payload: &PayloadAsignarTareaGrupo,
) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, payload.tarea_id.trim())?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.tipo == TipoTarea::SinDefinir {
        return Err(Error::Validacion(
            "La subtarea En espera aún no puede asignarse a un backlog".into(),
        ));
    }
    if let Some(ref destino_id) = payload.grupo_id {
        let backlog = db_sprints::obtener(c, destino_id)?;
        if backlog.con_fecha {
            return Err(Error::Validacion(format!(
                "{} es un sprint con fecha, no un backlog",
                backlog.id
            )));
        }
        if backlog.cerrado {
            return Err(Error::Validacion(format!(
                "El backlog {} ya está cerrado",
                backlog.id
            )));
        }
        if let Some(ref actual) = tarea.backlog_id {
            if actual != destino_id {
                return Err(Error::Validacion(
                    "La tarea ya está en otro backlog; primero hay que quitarla de ese".into(),
                ));
            }
        }
    }
    tarea.backlog_id = payload.grupo_id.clone();
    db_tareas::actualizar(c, &tarea)
}

fn validar_asignacion_backlog_a_sprint(
    c: &Connection,
    payload: &PayloadAsignarBacklogASprint,
) -> Resultado<()> {
    let sprint = db_sprints::obtener(c, &payload.sprint_id)?;
    if !sprint.con_fecha {
        return Err(Error::Validacion(format!(
            "{} es un backlog, no un sprint",
            sprint.id
        )));
    }
    if sprint.cerrado {
        return Err(Error::Validacion(format!(
            "El sprint {} ya está cerrado",
            sprint.id
        )));
    }
    if let Some(ref bid) = payload.backlog_id {
        let backlog = db_sprints::obtener(c, bid)?;
        if backlog.con_fecha {
            return Err(Error::Validacion(format!(
                "{} no es un backlog",
                backlog.id
            )));
        }
        if backlog.cerrado {
            return Err(Error::Validacion(format!(
                "El backlog {} ya está cerrado",
                backlog.id
            )));
        }
    }
    Ok(())
}

fn tareas_libres_de_backlog(c: &Connection, backlog_id: Option<&str>) -> Resultado<Vec<Tarea>> {
    Ok(db_tareas::listar(c)?
        .into_iter()
        .filter(|t| {
            !t.eliminada
                && t.tipo != TipoTarea::SinDefinir
                && t.sprint_id.is_none()
                && t.backlog_id.as_deref() == backlog_id
        })
        .collect())
}

fn aplicar_asignar_backlog_a_sprint(
    c: &Connection,
    payload: &PayloadAsignarBacklogASprint,
) -> Resultado<()> {
    validar_asignacion_backlog_a_sprint(c, payload)?;
    for mut t in tareas_libres_de_backlog(c, payload.backlog_id.as_deref())? {
        t.sprint_id = Some(payload.sprint_id.clone());
        db_tareas::actualizar(c, &t)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use crate::db::sprints as db_sprints;
    use crate::db::tareas as db_tareas;
    use crate::db::{usuarios, Db};

    use crate::modelo::{hoy_local, EstadoTarea, Rol};
    use crate::tareas::{cambiar_estado, crear, crear_en_espera, eliminar, DatosNuevaTarea};

    const ADMIN: &str = "admin@estudio.com";
    const LIDER: &str = "lider@estudio.com";
    const DEV: &str = "dev@estudio.com";
    const TESTER: &str = "tester@estudio.com";
    const HOY: &str = "2026-11-16";

    struct Entorno {
        _tmp: TempDir,
        db_a: Db,
        local_a: PathBuf,
        db_b: Db,
    }

    fn setup() -> Entorno {
        let tmp = TempDir::new().unwrap();
        let compartida = tmp.path().join("compartida");
        std::fs::create_dir_all(&compartida).unwrap();
        let local_a = tmp
            .path()
            .join("datos_a")
            .join("proyectos")
            .join("estudio-juego");
        let local_b = tmp
            .path()
            .join("datos_b")
            .join("proyectos")
            .join("estudio-juego");

        let db_a = Db::abrir(&local_a).unwrap();
        let db_b = Db::abrir(&local_b).unwrap();

        let admin_fila = crate::db::usuarios::Usuario {
            email: ADMIN.into(),
            nombre: "Admin".into(),
            apellido: "Uno".into(),
            rol: Rol::Administrador,
            fecha_alta: HOY.into(),
        };
        usuarios::insertar(db_a.conexion(), &admin_fila).unwrap();
        usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

        for (correo, rol) in [
            (LIDER, Rol::Lider),
            (DEV, Rol::Desarrollador),
            (TESTER, Rol::Tester),
        ] {
            {
                let u = crate::identidad::equipo::alta_de_prueba(db_a.conexion(), correo, rol, HOY);
                crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                u
            };
        }
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora).unwrap();

        Entorno {
            _tmp: tmp,
            db_a,
            local_a,
            db_b,
        }
    }

    fn autor_lider() -> AutorTarea<'static> {
        AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        }
    }

    fn autor_dev() -> AutorTarea<'static> {
        AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        }
    }

    fn crear_tarea_ok(e: &Entorno, titulo: &str, ts: &str) -> Tarea {
        crear(
            e.db_a.conexion(),
            &autor_lider(),
            &DatosNuevaTarea {
                titulo: titulo.into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
            },
            ts,
            HOY,
        )
        .unwrap()
    }

    fn nuevo_sprint(
        e: &Entorno,
        autor: &AutorTarea<'_>,
        nombre: &str,
        descripcion: &str,
        con_fecha: bool,
        duracion_dias: Option<u32>,
        ts: &str,
    ) -> Resultado<Sprint> {
        crear_sprint(
            e.db_a.conexion(),
            autor,
            &DatosNuevoSprint {
                nombre,
                descripcion,
                con_fecha,
                duracion_dias,
            },
            ts,
            HOY,
        )
    }

    #[test]
    fn crear_sprint_y_backlog_calculan_campos_y_validan() {
        let e = setup();
        let s1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Sprint 1",
            "Primer sprint",
            true,
            Some(14),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        assert_eq!(s1.id, "S-1");
        assert!(s1.con_fecha);
        assert_eq!(s1.fecha_inicio, "2026-11-16");
        assert_eq!(s1.duracion_dias, Some(14));
        assert_eq!(s1.fecha_fin.as_deref(), Some("2026-11-30"));
        assert!(!s1.cerrado);

        let b1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Backlog Ideas",
            "Sin fecha",
            false,
            None,
            "2026-11-16T10:01:05.000Z",
        )
        .unwrap();
        assert_eq!(b1.id, "B-1");
        assert!(!b1.con_fecha);
        assert_eq!(b1.duracion_dias, None);
        assert_eq!(b1.fecha_fin, None);

        // Permiso exclusivo Líder/Admin.
        assert!(matches!(
            nuevo_sprint(
                &e,
                &autor_dev(),
                "No permitido",
                "",
                true,
                Some(7),
                "2026-11-16T10:01:10.000Z",
            ),
            Err(Error::SinPermiso)
        ));
        assert!(matches!(
            nuevo_sprint(
                &e,
                &AutorTarea {
                    email: TESTER,
                    rol: Rol::Tester,
                },
                "No permitido tester",
                "",
                false,
                None,
                "2026-11-16T10:01:11.000Z",
            ),
            Err(Error::SinPermiso)
        ));

        // Validaciones de nombre y duración.
        assert!(matches!(
            nuevo_sprint(
                &e,
                &autor_lider(),
                "   ",
                "",
                true,
                Some(7),
                "2026-11-16T10:01:12.000Z",
            ),
            Err(Error::Validacion(_))
        ));
        assert!(matches!(
            nuevo_sprint(
                &e,
                &autor_lider(),
                "Sin dias",
                "",
                true,
                Some(0),
                "2026-11-16T10:01:13.000Z",
            ),
            Err(Error::Validacion(_))
        ));
        assert!(matches!(
            nuevo_sprint(
                &e,
                &autor_lider(),
                "Backlog con dias",
                "",
                false,
                Some(5),
                "2026-11-16T10:01:14.000Z",
            ),
            Err(Error::Validacion(_))
        ));

        let lista = listar(e.db_a.conexion()).unwrap();
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0].id, "S-1");
        assert_eq!(lista[1].id, "B-1");
    }

    #[test]
    fn asignar_y_quitar_tarea_en_sprint_y_backlog_respeta_exclusividad_y_validaciones() {
        let e = setup();
        let s1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "S1",
            "",
            true,
            Some(7),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        let mut s2 = nuevo_sprint(
            &e,
            &autor_lider(),
            "S2",
            "",
            true,
            Some(7),
            "2026-11-16T10:01:01.000Z",
        )
        .unwrap();
        let b1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "B1",
            "",
            false,
            None,
            "2026-11-16T10:01:02.000Z",
        )
        .unwrap();
        let b2 = nuevo_sprint(
            &e,
            &autor_lider(),
            "B2",
            "",
            false,
            None,
            "2026-11-16T10:01:03.000Z",
        )
        .unwrap();
        let t = crear_tarea_ok(&e, "Tarea planificada", "2026-11-16T10:01:04.000Z");

        // Rechaza tipo incorrecto y roles no autorizados.
        assert!(matches!(
            asignar_tarea_a_sprint(
                e.db_a.conexion(),
                &autor_dev(),
                &t.id,
                Some(&s1.id),
                "2026-11-16T10:01:05.000Z",
            ),
            Err(Error::SinPermiso)
        ));
        assert!(matches!(
            asignar_tarea_a_sprint(
                e.db_a.conexion(),
                &autor_lider(),
                &t.id,
                Some(&b1.id),
                "2026-11-16T10:01:06.000Z",
            ),
            Err(Error::Validacion(_))
        ));
        assert!(matches!(
            asignar_tarea_a_backlog(
                e.db_a.conexion(),
                &autor_lider(),
                &t.id,
                Some(&s1.id),
                "2026-11-16T10:01:07.000Z",
            ),
            Err(Error::Validacion(_))
        ));

        // Asigna a S1 y B1 simultáneamente; no permite saltar directo a S2 ni B2 sin quitar antes.
        let t = asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t.id,
            Some(&s1.id),
            "2026-11-16T10:01:08.000Z",
        )
        .unwrap();
        let t = asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t.id,
            Some(&b1.id),
            "2026-11-16T10:01:09.000Z",
        )
        .unwrap();
        assert_eq!(t.sprint_id.as_deref(), Some("S-1"));
        assert_eq!(t.backlog_id.as_deref(), Some("B-1"));

        assert!(matches!(
            asignar_tarea_a_sprint(
                e.db_a.conexion(),
                &autor_lider(),
                &t.id,
                Some(&s2.id),
                "2026-11-16T10:01:10.000Z",
            ),
            Err(Error::Validacion(_))
        ));
        assert!(matches!(
            asignar_tarea_a_backlog(
                e.db_a.conexion(),
                &autor_lider(),
                &t.id,
                Some(&b2.id),
                "2026-11-16T10:01:11.000Z",
            ),
            Err(Error::Validacion(_))
        ));

        // Al quitar de S1/B1, puede entrar a S2/B2; un sprint cerrado rechaza nuevas tareas.
        s2.cerrado = true;
        db_sprints::actualizar(e.db_a.conexion(), &s2).unwrap();
        let t = asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t.id,
            None,
            "2026-11-16T10:01:12.000Z",
        )
        .unwrap();
        assert_eq!(t.sprint_id, None);
        assert!(matches!(
            asignar_tarea_a_sprint(
                e.db_a.conexion(),
                &autor_lider(),
                &t.id,
                Some(&s2.id),
                "2026-11-16T10:01:13.000Z",
            ),
            Err(Error::Validacion(_))
        ));
        let t = asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t.id,
            None,
            "2026-11-16T10:01:14.000Z",
        )
        .unwrap();
        let t = asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t.id,
            Some(&b2.id),
            "2026-11-16T10:01:15.000Z",
        )
        .unwrap();
        assert_eq!(t.backlog_id.as_deref(), Some("B-2"));

        // Subtarea En espera y tarea eliminada se rechazan.
        let mut t_padre = crear_tarea_ok(&e, "Padre", "2026-11-16T10:01:16.000Z");
        t_padre.rama_asociada = Some("/main/dev/T-102-padre".into());
        db_tareas::actualizar(e.db_a.conexion(), &t_padre).unwrap();
        let en_espera = crear_en_espera(
            e.db_a.conexion(),
            &autor_dev(),
            &t_padre.id,
            1,
            "2026-11-16T10:01:17.000Z",
            HOY,
        )
        .unwrap();
        assert!(matches!(
            asignar_tarea_a_sprint(
                e.db_a.conexion(),
                &autor_lider(),
                &en_espera.id,
                Some(&s1.id),
                "2026-11-16T10:01:18.000Z",
            ),
            Err(Error::Validacion(_))
        ));

        eliminar(
            e.db_a.conexion(),
            &autor_lider(),
            &t.id,
            "Obsol",
            "2026-11-16T10:01:19.000Z",
        )
        .unwrap();
        assert!(matches!(
            asignar_tarea_a_backlog(
                e.db_a.conexion(),
                &autor_lider(),
                &t.id,
                None,
                "2026-11-16T10:01:20.000Z",
            ),
            Err(Error::Validacion(_))
        ));
    }

    #[test]
    fn asignar_backlog_a_sprint_mueve_solo_libres_sin_tocar_otras_y_sincroniza() {
        let e = setup();
        let s1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Sprint A",
            "",
            true,
            Some(10),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        let s2 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Sprint B",
            "",
            true,
            Some(10),
            "2026-11-16T10:01:01.000Z",
        )
        .unwrap();
        let b1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Backlog 1",
            "",
            false,
            None,
            "2026-11-16T10:01:02.000Z",
        )
        .unwrap();
        let b2 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Backlog 2",
            "",
            false,
            None,
            "2026-11-16T10:01:03.000Z",
        )
        .unwrap();

        let t_libre_b1 = crear_tarea_ok(&e, "Libre en B1", "2026-11-16T10:01:04.000Z");
        let t_ocupada_b1 = crear_tarea_ok(&e, "Ocupada en S2", "2026-11-16T10:01:05.000Z");
        let t_eliminada_b1 = crear_tarea_ok(&e, "Borrada en B1", "2026-11-16T10:01:06.000Z");
        let t_sin_backlog = crear_tarea_ok(&e, "Sin backlog", "2026-11-16T10:01:07.000Z");
        let t_en_b2 = crear_tarea_ok(&e, "En B2", "2026-11-16T10:01:08.000Z");

        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t_libre_b1.id,
            Some(&b1.id),
            "2026-11-16T10:01:09.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t_ocupada_b1.id,
            Some(&b1.id),
            "2026-11-16T10:01:10.000Z",
        )
        .unwrap();
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t_ocupada_b1.id,
            Some(&s2.id),
            "2026-11-16T10:01:11.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t_eliminada_b1.id,
            Some(&b1.id),
            "2026-11-16T10:01:12.000Z",
        )
        .unwrap();
        eliminar(
            e.db_a.conexion(),
            &autor_lider(),
            &t_eliminada_b1.id,
            "Descartada",
            "2026-11-16T10:01:13.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t_en_b2.id,
            Some(&b2.id),
            "2026-11-16T10:01:14.000Z",
        )
        .unwrap();

        // Pasar B1 -> S1 solo mueve `t_libre_b1`.
        let movidas = asignar_backlog_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            Some(&b1.id),
            &s1.id,
            "2026-11-16T10:01:15.000Z",
        )
        .unwrap();
        assert_eq!(movidas.len(), 1);
        assert_eq!(movidas[0].id, t_libre_b1.id);
        assert_eq!(movidas[0].sprint_id.as_deref(), Some("S-1"));
        assert_eq!(
            db_tareas::obtener(e.db_a.conexion(), &t_ocupada_b1.id)
                .unwrap()
                .sprint_id
                .as_deref(),
            Some("S-2")
        );
        assert_eq!(
            db_tareas::obtener(e.db_a.conexion(), &t_eliminada_b1.id)
                .unwrap()
                .sprint_id,
            None
        );

        // Sin pendientes libres devuelve vacío.
        let repetidas = asignar_backlog_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            Some(&b1.id),
            &s1.id,
            "2026-11-16T10:01:16.000Z",
        )
        .unwrap();
        assert!(repetidas.is_empty());

        // `backlog_id = None` mueve solo las tareas sin backlog y sin sprint (`t_sin_backlog`).
        let sin_backlog = asignar_backlog_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            None,
            &s1.id,
            "2026-11-16T10:01:17.000Z",
        )
        .unwrap();
        assert_eq!(sin_backlog.len(), 1);
        assert_eq!(sin_backlog[0].id, t_sin_backlog.id);
        assert_eq!(
            db_tareas::obtener(e.db_a.conexion(), &t_en_b2.id)
                .unwrap()
                .sprint_id,
            None
        );

        // Evento falso de un Desarrollador inyectado en ops/ debe descartarse en B.
        let mut ev_falso = Evento::nuevo(
            "crear_sprint",
            Some("S-99"),
            DEV,
            serde_json::to_value(&Sprint {
                id: "S-99".into(),
                nombre: "Falso".into(),
                descripcion: String::new(),
                con_fecha: true,
                fecha_inicio: HOY.into(),
                duracion_dias: Some(5),
                fecha_fin: Some("2026-11-21".into()),
                cerrado: false,
                fecha_cierre_real: None,
                creado_por: DEV.into(),
                reporte: None,
            })
            .unwrap(),
        );
        ev_falso.timestamp = "2026-11-16T10:01:18.000Z".into();
        crate::eventos::servidor_de_prueba::agregar(&ev_falso);

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b =
            crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();
        assert_eq!(sync_b.descartados, 1);
        assert_eq!(
            listar(e.db_b.conexion()).unwrap(),
            listar(e.db_a.conexion()).unwrap()
        );
        assert_eq!(
            db_tareas::listar(e.db_b.conexion()).unwrap(),
            db_tareas::listar(e.db_a.conexion()).unwrap()
        );
        let _ = e.local_a;
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #113): entra de verdad al
    /// repositorio `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` con
    /// `cm workspace create` + `entrar_en`, configura una subcarpeta real en
    /// OneDrive (`BB_PRUEBA_CARPETA_COMPARTIDA`), ejerce los comandos Tauri
    /// `crear_sprint_en`, `listar_sprints_en`, `asignar_tarea_a_backlog_en`,
    /// `asignar_tarea_a_sprint_en` y `asignar_backlog_a_sprint_en` (cálculo de
    /// `fecha_fin`, permisos por rol, exclusividad de un sprint por tarea y paso
    /// masivo solo de tareas libres) y verifica que una segunda máquina B
    /// sincroniza por OneDrive exactamente los mismos sprints, backlogs y tareas
    /// mientras descarta eventos no autorizados. Borra workspace y carpeta al
    /// terminar.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive \
    ///   cargo test --lib entorno_real_sprints_y_backlogs -- --ignored --nocapture --test-threads=1
    /// ```
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_sprints_y_backlogs() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::sprints::{
            asignar_backlog_a_sprint_en, asignar_tarea_a_backlog_en, asignar_tarea_a_sprint_en,
            crear_sprint_en, listar_sprints_en,
        };
        use crate::estado::AppState;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube_tmp = tempfile::tempdir().unwrap();
        let nube = nube_tmp.path().to_path_buf();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-113-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-113-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            estado_a
                .con_db(|db_a| {
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            DEV,
                            Rol::Desarrollador,
                            HOY,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })?;
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            LIDER,
                            Rol::Lider,
                            HOY,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })?;
                    Ok(())
                })
                .unwrap();

            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));

            // 1. Administrador (vía comandos Tauri *_en) crea S-1 (14 días), S-2 (7 días) y B-1.
            let s1 =
                crear_sprint_en(&estado_a, "Sprint Piloto", "Meta real", true, Some(14)).unwrap();
            pausa();
            assert_eq!(s1.id, "S-1");
            assert_eq!(s1.duracion_dias, Some(14));
            assert_eq!(
                s1.fecha_fin.as_deref(),
                Some(sumar_dias(&s1.fecha_inicio, 14).unwrap().as_str())
            );
            let s2 =
                crear_sprint_en(&estado_a, "Sprint Reserva", "Siguiente", true, Some(7)).unwrap();
            pausa();
            let b1 = crear_sprint_en(&estado_a, "Backlog Real", "Pendientes", false, None).unwrap();
            pausa();
            assert_eq!(b1.id, "B-1");
            assert!(b1.fecha_fin.is_none());

            let lista_a = listar_sprints_en(&estado_a).unwrap();
            assert_eq!(lista_a.len(), 3);

            // 2. Creamos tareas (1 asignada solo a B-1, otra en B-1 y ya ocupada en S-2, y sin backlog).
            let (t1, t2, t3) = estado_a
                .con_db(|db_a| {
                    let t1 = crear(
                        db_a.conexion(),
                        &autor_lider(),
                        &DatosNuevaTarea {
                            titulo: "Tarea libre de B-1".into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        "2026-11-15T10:01:00.000Z",
                        HOY,
                    )?;
                    let t2 = crear(
                        db_a.conexion(),
                        &autor_lider(),
                        &DatosNuevaTarea {
                            titulo: "Tarea ya en S-2".into(),
                            detalles: String::new(),
                            tipo: TipoTarea::UiInterfaz,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        "2026-11-15T10:01:01.000Z",
                        HOY,
                    )?;
                    let t3 = crear(
                        db_a.conexion(),
                        &autor_lider(),
                        &DatosNuevaTarea {
                            titulo: "Tarea sin backlog".into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Audio,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        "2026-11-15T10:01:02.000Z",
                        HOY,
                    )?;
                    // Permiso rechazado al Desarrollador en lógica de dominio.
                    assert!(matches!(
                        crear_sprint(
                            db_a.conexion(),
                            &autor_dev(),
                            &DatosNuevoSprint {
                                nombre: "Ilegal",
                                descripcion: "",
                                con_fecha: true,
                                duracion_dias: Some(5),
                            },
                            "2026-11-15T10:01:03.000Z",
                            HOY,
                        ),
                        Err(Error::SinPermiso)
                    ));
                    assert!(matches!(
                        super::asignar_tarea_a_sprint(
                            db_a.conexion(),
                            &autor_dev(),
                            &t1.id,
                            Some(&s1.id),
                            "2026-11-15T10:01:04.000Z",
                        ),
                        Err(Error::SinPermiso)
                    ));
                    Ok((t1, t2, t3))
                })
                .unwrap();

            // Asignaciones vía comandos Tauri reales.
            asignar_tarea_a_backlog_en(&estado_a, &t1.id, Some(&b1.id)).unwrap();
            pausa();
            asignar_tarea_a_backlog_en(&estado_a, &t2.id, Some(&b1.id)).unwrap();
            pausa();
            asignar_tarea_a_sprint_en(&estado_a, &t2.id, Some(&s2.id)).unwrap();
            pausa();

            // Exclusividad de sprint: no deja mover t2 a S-1 sin quitar de S-2 antes.
            assert!(asignar_tarea_a_sprint_en(&estado_a, &t2.id, Some(&s1.id)).is_err());

            // Paso masivo B-1 -> S-1 solo mueve a t1, respetando a t2 (S-2) y t3 (sin backlog).
            let movidas = asignar_backlog_a_sprint_en(&estado_a, Some(&b1.id), &s1.id).unwrap();
            pausa();
            assert_eq!(movidas.len(), 1);
            assert_eq!(movidas[0].id, t1.id);
            assert_eq!(movidas[0].sprint_id.as_deref(), Some("S-1"));

            // Quitar t2 de S-2 y pasar las sin-backlog (t3) a S-1.
            let t2_suelta = asignar_tarea_a_sprint_en(&estado_a, &t2.id, None).unwrap();
            pausa();
            assert_eq!(t2_suelta.sprint_id, None);
            let sin_bl = asignar_backlog_a_sprint_en(&estado_a, None, &s1.id).unwrap();
            pausa();
            assert_eq!(sin_bl.len(), 1);
            assert_eq!(sin_bl[0].id, t3.id);

            // Inyectamos en OneDrive un evento falso de Desarrollador para verificar rechazo.
            let mut ev_falso = Evento::nuevo(
                "crear_sprint",
                Some("S-999"),
                DEV,
                serde_json::to_value(&Sprint {
                    id: "S-999".into(),
                    nombre: "Sprint Inyectado".into(),
                    descripcion: String::new(),
                    con_fecha: true,
                    fecha_inicio: HOY.into(),
                    duracion_dias: Some(3),
                    fecha_fin: Some("2026-11-19".into()),
                    cerrado: false,
                    fecha_cierre_real: None,
                    creado_por: DEV.into(),
                    reporte: None,
                })
                .unwrap(),
            );
            ev_falso.timestamp = crate::modelo::ahora_iso();
            crate::eventos::servidor_de_prueba::agregar(&ev_falso);

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 3. Máquina B sincroniza desde OneDrive y obtiene idénticos sprints y tareas.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b =
                crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora).unwrap();
            println!("B sincroniza planificación por el servidor de prueba: {sync_b:?}");
            assert_eq!(sync_b.descartados, 1);

            let sprints_a = listar_sprints_en(&estado_a).unwrap();
            let sprints_b = listar(db_b.conexion()).unwrap();
            assert_eq!(sprints_b, sprints_a);

            let tareas_a = estado_a
                .con_db(|db| db_tareas::listar(db.conexion()))
                .unwrap();
            let tareas_b = db_tareas::listar(db_b.conexion()).unwrap();
            assert_eq!(tareas_b, tareas_a);
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    fn autor_admin() -> AutorTarea<'static> {
        AutorTarea {
            email: ADMIN,
            rol: Rol::Administrador,
        }
    }

    #[test]
    fn cerrar_sprint_solo_admin_guarda_copia_y_libera_pendientes() {
        let e = setup();
        let s1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Sprint Anticipado",
            "Meta",
            true,
            Some(14),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        let b1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Backlog Base",
            "",
            false,
            None,
            "2026-11-16T10:01:01.000Z",
        )
        .unwrap();

        let t_hecha = crear_tarea_ok(&e, "Ya hecha", "2026-11-16T10:01:02.000Z");
        let t_libre = crear_tarea_ok(&e, "Aun en produccion", "2026-11-16T10:01:03.000Z");

        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &autor_lider(),
            &t_libre.id,
            Some(&b1.id),
            "2026-11-16T10:01:04.000Z",
        )
        .unwrap();
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t_hecha.id,
            Some(&s1.id),
            "2026-11-16T10:01:05.000Z",
        )
        .unwrap();
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t_libre.id,
            Some(&s1.id),
            "2026-11-16T10:01:06.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &autor_lider(),
            &t_hecha.id,
            EstadoTarea::Terminada,
            "2026-11-16T10:01:07.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &autor_dev(),
            &t_libre.id,
            EstadoTarea::EnProduccion,
            "2026-11-16T10:01:08.000Z",
        )
        .unwrap();

        // Líder y Desarrollador no pueden cerrar sprints ni backlogs.
        assert!(matches!(
            cerrar_sprint(
                e.db_a.conexion(),
                &autor_lider(),
                &s1.id,
                "2026-11-16T10:01:09.000Z",
                "2026-11-20",
            ),
            Err(Error::SinPermiso)
        ));
        assert!(matches!(
            cerrar_sprint(
                e.db_a.conexion(),
                &autor_dev(),
                &b1.id,
                "2026-11-16T10:01:10.000Z",
                "2026-11-20",
            ),
            Err(Error::SinPermiso)
        ));

        // Administrador cierra S-1 antes de fecha_fin (2026-11-30): cierra el 2026-11-20.
        let cerrado = cerrar_sprint(
            e.db_a.conexion(),
            &autor_admin(),
            &s1.id,
            "2026-11-16T10:01:11.000Z",
            "2026-11-20",
        )
        .unwrap();
        assert!(cerrado.cerrado);
        assert_eq!(cerrado.fecha_cierre_real.as_deref(), Some("2026-11-20"));
        let copia = cerrado.reporte.as_ref().expect("debe guardar copia");
        assert_eq!(copia.len(), 2);
        assert_eq!(
            copia.iter().find(|t| t.id == t_libre.id).unwrap().estado,
            EstadoTarea::EnProduccion
        );

        // La hecha conserva sprint_id; la inacabada queda libre (sprint_id = None) en su backlog.
        let hech_ahora = db_tareas::obtener(e.db_a.conexion(), &t_hecha.id).unwrap();
        let lib_ahora = db_tareas::obtener(e.db_a.conexion(), &t_libre.id).unwrap();
        assert_eq!(hech_ahora.sprint_id.as_deref(), Some("S-1"));
        assert_eq!(lib_ahora.sprint_id, None);
        assert_eq!(lib_ahora.backlog_id.as_deref(), Some("B-1"));

        // Cambiar la tarea libre después NO altera la copia histórica del reporte.
        cambiar_estado(
            e.db_a.conexion(),
            &autor_lider(),
            &t_libre.id,
            EstadoTarea::Terminada,
            "2026-11-16T10:01:12.000Z",
        )
        .unwrap();
        let s1_lista = listar(e.db_a.conexion())
            .unwrap()
            .into_iter()
            .find(|s| s.id == s1.id)
            .unwrap();
        assert_eq!(
            s1_lista
                .reporte
                .unwrap()
                .iter()
                .find(|t| t.id == t_libre.id)
                .unwrap()
                .estado,
            EstadoTarea::EnProduccion
        );

        // No se puede volver a cerrar ni meter más tareas.
        assert!(matches!(
            cerrar_sprint(
                e.db_a.conexion(),
                &autor_admin(),
                &s1.id,
                "2026-11-16T10:01:13.000Z",
                "2026-11-21",
            ),
            Err(Error::Validacion(_))
        ));

        // El Administrador también puede cerrar manualmente el backlog B-1.
        let b1_cerrado = cerrar_sprint(
            e.db_a.conexion(),
            &autor_admin(),
            &b1.id,
            "2026-11-16T10:01:14.000Z",
            "2026-11-22",
        )
        .unwrap();
        assert!(b1_cerrado.cerrado);
        assert_eq!(b1_cerrado.fecha_cierre_real.as_deref(), Some("2026-11-22"));

        // Sincroniza a B y descarta evento falso de Líder intentando cerrar sin ser Admin.
        let mut ev_falso = Evento::nuevo(
            "cerrar_sprint",
            Some(&s1.id),
            LIDER,
            serde_json::to_value(&PayloadCerrarSprint {
                sprint_id: s1.id.clone(),
                fecha: "2026-11-23".into(),
            })
            .unwrap(),
        );
        ev_falso.timestamp = "2026-11-16T10:01:15.000Z".into();
        crate::eventos::servidor_de_prueba::agregar(&ev_falso);

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b =
            crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();
        assert_eq!(sync_b.descartados, 1);
        assert_eq!(
            listar(e.db_b.conexion()).unwrap(),
            listar(e.db_a.conexion()).unwrap()
        );
        let _ = e.local_a;
    }

    #[test]
    fn fecha_fin_cierra_solo_si_todo_terminado_y_sino_deja_abierto_con_reporte() {
        let e = setup();
        let s1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Todo Listo",
            "",
            true,
            Some(5),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap(); // fin 2026-11-21
        let s2 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Con Pendiente",
            "",
            true,
            Some(5),
            "2026-11-16T10:01:01.000Z",
        )
        .unwrap(); // fin 2026-11-21
        let b1 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Backlog Eterno",
            "",
            false,
            None,
            "2026-11-16T10:01:02.000Z",
        )
        .unwrap();

        let t1 = crear_tarea_ok(&e, "T S1", "2026-11-16T10:01:03.000Z");
        let t2a = crear_tarea_ok(&e, "T S2 hecha", "2026-11-16T10:01:04.000Z");
        let t2b = crear_tarea_ok(&e, "T S2 en revision", "2026-11-16T10:01:05.000Z");

        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t1.id,
            Some(&s1.id),
            "2026-11-16T10:01:06.000Z",
        )
        .unwrap();
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t2a.id,
            Some(&s2.id),
            "2026-11-16T10:01:07.000Z",
        )
        .unwrap();
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &autor_lider(),
            &t2b.id,
            Some(&s2.id),
            "2026-11-16T10:01:08.000Z",
        )
        .unwrap();

        cambiar_estado(
            e.db_a.conexion(),
            &autor_lider(),
            &t1.id,
            EstadoTarea::Terminada,
            "2026-11-16T10:01:09.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &autor_lider(),
            &t2a.id,
            EstadoTarea::Terminada,
            "2026-11-16T10:01:10.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &autor_dev(),
            &t2b.id,
            EstadoTarea::EnRevision,
            "2026-11-16T10:01:11.000Z",
        )
        .unwrap();

        // Antes de fecha_fin (2026-11-20) no cambia nada.
        assert!(evaluar_cierre_sprints(
            e.db_a.conexion(),
            DEV,
            "2026-11-16T10:01:12.000Z",
            "2026-11-20",
        )
        .unwrap()
        .is_empty());

        // Al llegar fecha_fin (2026-11-21): S-1 cierra solo; S-2 genera reporte del día pero sigue abierto.
        let cam = evaluar_cierre_sprints(
            e.db_a.conexion(),
            DEV,
            "2026-11-16T10:01:13.000Z",
            "2026-11-21",
        )
        .unwrap();
        assert_eq!(cam, vec![s1.id.clone(), s2.id.clone()]);

        let s1_fin = db_sprints::obtener(e.db_a.conexion(), &s1.id).unwrap();
        assert!(s1_fin.cerrado);
        assert_eq!(s1_fin.fecha_cierre_real.as_deref(), Some("2026-11-21"));

        let s2_fin = db_sprints::obtener(e.db_a.conexion(), &s2.id).unwrap();
        assert!(!s2_fin.cerrado);
        assert_eq!(s2_fin.fecha_cierre_real, None);
        let reps_s2 = db_sprints::reportes(e.db_a.conexion(), &s2.id).unwrap();
        assert_eq!(reps_s2.len(), 1);
        assert!(!reps_s2[0].de_cierre);
        assert_eq!(reps_s2[0].fecha, "2026-11-21");
        assert_eq!(reps_s2[0].tareas.len(), 2);

        // Correr otra vez al día siguiente sin cambios no duplica el reporte de S-2.
        assert!(evaluar_cierre_sprints(
            e.db_a.conexion(),
            LIDER,
            "2026-11-16T10:01:14.000Z",
            "2026-11-22",
        )
        .unwrap()
        .is_empty());
        assert_eq!(
            db_sprints::reportes(e.db_a.conexion(), &s2.id)
                .unwrap()
                .len(),
            1
        );

        // Cuando termina t2b el 2026-11-23, S-2 se cierra solo con reporte final.
        cambiar_estado(
            e.db_a.conexion(),
            &autor_lider(),
            &t2b.id,
            EstadoTarea::Terminada,
            "2026-11-23T10:01:15.000Z",
        )
        .unwrap();

        let cierre_s2 = evaluar_cierre_sprints(
            e.db_a.conexion(),
            LIDER,
            "2026-11-23T10:01:16.000Z",
            "2026-11-23",
        )
        .unwrap();
        assert_eq!(cierre_s2, vec![s2.id.clone()]);
        let s2_cerrado = db_sprints::obtener(e.db_a.conexion(), &s2.id).unwrap();
        assert!(s2_cerrado.cerrado);
        assert_eq!(s2_cerrado.fecha_cierre_real.as_deref(), Some("2026-11-23"));
        let reps_final = db_sprints::reportes(e.db_a.conexion(), &s2.id).unwrap();
        assert_eq!(reps_final.len(), 2);
        assert!(reps_final[1].de_cierre);

        // El backlog nunca se cierra solo.
        assert!(
            !db_sprints::obtener(e.db_a.conexion(), &b1.id)
                .unwrap()
                .cerrado
        );

        // Un evento forjado de auto-cierre antes de fecha_fin se descarta en B.
        let s3 = nuevo_sprint(
            &e,
            &autor_lider(),
            "Futuro",
            "",
            true,
            Some(30),
            "2026-11-23T10:01:17.000Z",
        )
        .unwrap();
        let mut ev_premature = Evento::nuevo(
            "cerrar_sprint_automatico",
            Some(&s3.id),
            DEV,
            serde_json::to_value(&PayloadCerrarSprint {
                sprint_id: s3.id.clone(),
                fecha: "2026-11-20".into(),
            })
            .unwrap(),
        );
        ev_premature.timestamp = "2026-11-23T10:01:18.000Z".into();
        crate::eventos::servidor_de_prueba::agregar(&ev_premature);

        let ahora = DateTime::parse_from_rfc3339("2026-11-23T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b =
            crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();
        assert_eq!(sync_b.descartados, 1);
        assert_eq!(
            listar(e.db_b.conexion()).unwrap(),
            listar(e.db_a.conexion()).unwrap()
        );
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #114): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// prueba:
    /// 1. `cerrar_sprint_en` (solo Administrador, incluso antes de `fecha_fin`):
    ///    guarda la copia congelada de las tareas del sprint, conserva `sprint_id`
    ///    en las terminadas y deja libres (`sprint_id = null`) en su backlog las
    ///    que no estaban terminadas; rechaza cierre manual a Líder/Desarrollador.
    /// 2. Sprints cuya `fecha_fin` coincide con hoy (`hoy_local()`):
    ///    - `revisar_actualizaciones_en` / `listar_sprints_en` cierra automáticamente
    ///      el sprint cuyas tareas ya están todas en `Terminada` (`de_cierre = true`).
    ///    - El sprint con tarea sin terminar genera el reporte del día
    ///      (`de_cierre = false`) y sigue abierto sin duplicar reportes; al
    ///      terminar la última tarea se cierra solo con su reporte final.
    /// 3. La máquina B sincroniza vía OneDrive todos los eventos y descarta un
    ///    `cerrar_sprint` forjado de Líder, llegando exactamente al mismo estado.
    ///
    /// Al terminar borra el workspace y la subcarpeta de OneDrive.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_cierre_sprints_y_reportes() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::sprints::{
            asignar_tarea_a_backlog_en, asignar_tarea_a_sprint_en, cerrar_sprint_en,
            crear_sprint_en, listar_sprints_en,
        };
        use crate::comandos::tareas::cambiar_estado_en;
        use crate::estado::AppState;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube_tmp = tempfile::tempdir().unwrap();
        let nube = nube_tmp.path().to_path_buf();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-114-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-114-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let hoy_real = hoy_local();
            let hace_7_dias = (chrono::Local::now().date_naive() - Days::new(7))
                .format("%Y-%m-%d")
                .to_string();
            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));
            let ahora_pausa = || {
                pausa();
                crate::modelo::ahora_iso()
            };

            estado_a
                .con_db(|db_a| {
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            LIDER,
                            Rol::Lider,
                            &hoy_real,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })?;
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            DEV,
                            Rol::Desarrollador,
                            &hoy_real,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })?;
                    Ok(())
                })
                .unwrap();

            // S-1: sprint futuro para cierre manual anticipado. B-1: backlog para verificar tareas liberadas.
            pausa();
            let s1 = crear_sprint_en(
                &estado_a,
                "Sprint Anticipado",
                "Cierre manual",
                true,
                Some(14),
            )
            .unwrap();
            pausa();
            let b1 = crear_sprint_en(&estado_a, "Backlog Destino", "", false, None).unwrap();
            pausa();

            // S-2 y S-3: sprints creados hace 7 días con duración 7 -> fecha_fin == hoy_local().
            let (s2, s3, t_s1_ok, t_s1_pend, t_s2_ok, t_s3_pend) = estado_a
                .con_db(|db_a| {
                    let s2 = crear_sprint(
                        db_a.conexion(),
                        &autor_lider(),
                        &DatosNuevoSprint {
                            nombre: "Sprint Auto Cierre",
                            descripcion: "Todas terminadas hoy",
                            con_fecha: true,
                            duracion_dias: Some(7),
                        },
                        &ahora_pausa(),
                        &hace_7_dias,
                    )?;
                    let s3 = crear_sprint(
                        db_a.conexion(),
                        &autor_lider(),
                        &DatosNuevoSprint {
                            nombre: "Sprint Con Pendiente Hoy",
                            descripcion: "Genera reporte hoy y sigue abierto",
                            con_fecha: true,
                            duracion_dias: Some(7),
                        },
                        &ahora_pausa(),
                        &hace_7_dias,
                    )?;
                    let mk = |titulo: &str| {
                        crear(
                            db_a.conexion(),
                            &autor_lider(),
                            &DatosNuevaTarea {
                                titulo: titulo.into(),
                                detalles: String::new(),
                                tipo: TipoTarea::Scripting,
                                tarea_padre_id: None,
                                sprint_id: None,
                                backlog_id: None,
                                con_fecha: false,
                                fecha_limite: None,
                            },
                            &ahora_pausa(),
                            &hoy_real,
                        )
                    };
                    let t_s1_ok = mk("S1 terminada")?;
                    let t_s1_pend = mk("S1 vuelve a backlog")?;
                    let t_s2_ok = mk("S2 terminada")?;
                    let t_s3_pend = mk("S3 pendiente al corte")?;

                    asignar_tarea_a_sprint(
                        db_a.conexion(),
                        &autor_lider(),
                        &t_s2_ok.id,
                        Some(&s2.id),
                        &ahora_pausa(),
                    )?;
                    asignar_tarea_a_sprint(
                        db_a.conexion(),
                        &autor_lider(),
                        &t_s3_pend.id,
                        Some(&s3.id),
                        &ahora_pausa(),
                    )?;
                    Ok((s2, s3, t_s1_ok, t_s1_pend, t_s2_ok, t_s3_pend))
                })
                .unwrap();

            asignar_tarea_a_backlog_en(&estado_a, &t_s1_pend.id, Some(&b1.id)).unwrap();
            pausa();
            asignar_tarea_a_sprint_en(&estado_a, &t_s1_ok.id, Some(&s1.id)).unwrap();
            pausa();
            asignar_tarea_a_sprint_en(&estado_a, &t_s1_pend.id, Some(&s1.id)).unwrap();
            pausa();
            assert!(matches!(
                estado_a.con_db(|db_a| {
                    cerrar_sprint(
                        db_a.conexion(),
                        &autor_lider(),
                        &s1.id,
                        &crate::modelo::ahora_iso(),
                        &hoy_real,
                    )
                }),
                Err(Error::SinPermiso)
            ));

            cambiar_estado_en(&estado_a, &t_s1_ok.id, EstadoTarea::Terminada).unwrap();
            pausa();
            cambiar_estado_en(&estado_a, &t_s1_pend.id, EstadoTarea::EnProduccion).unwrap();
            pausa();
            cambiar_estado_en(&estado_a, &t_s2_ok.id, EstadoTarea::Terminada).unwrap();
            pausa();
            cambiar_estado_en(&estado_a, &t_s3_pend.id, EstadoTarea::EnProduccion).unwrap();
            pausa();

            // 1. Cierre manual anticipado de S-1 por Administrador.
            let s1_cerrado = cerrar_sprint_en(&estado_a, &s1.id).unwrap();
            pausa();
            assert!(s1_cerrado.cerrado);
            assert_eq!(
                s1_cerrado.fecha_cierre_real.as_deref(),
                Some(hoy_real.as_str())
            );
            assert_eq!(s1_cerrado.reporte.as_ref().unwrap().len(), 2);

            estado_a
                .con_db(|db_a| {
                    assert_eq!(
                        db_tareas::obtener(db_a.conexion(), &t_s1_ok.id)?
                            .sprint_id
                            .as_deref(),
                        Some("S-1")
                    );
                    let libre = db_tareas::obtener(db_a.conexion(), &t_s1_pend.id)?;
                    assert_eq!(libre.sprint_id, None);
                    assert_eq!(libre.backlog_id.as_deref(), Some("B-1"));
                    Ok(())
                })
                .unwrap();

            // 2. El polling real (revisar_actualizaciones_en) evalúa fecha_fin == hoy_local():
            // S-2 se cierra solo; S-3 genera reporte de hoy y sigue abierto.
            let poll = revisar_actualizaciones_en(&estado_a, None).unwrap();
            pausa();
            assert!(poll.sprints_actualizados.contains(&s2.id));
            assert!(poll.sprints_actualizados.contains(&s3.id));

            let lista_mid = listar_sprints_en(&estado_a).unwrap();
            let s2_mid = lista_mid.iter().find(|s| s.id == s2.id).unwrap();
            let s3_mid = lista_mid.iter().find(|s| s.id == s3.id).unwrap();
            assert!(s2_mid.cerrado);
            assert!(!s3_mid.cerrado);
            assert_eq!(s3_mid.reporte.as_ref().unwrap().len(), 1);
            assert_eq!(
                s3_mid.reporte.as_ref().unwrap()[0].estado,
                EstadoTarea::EnProduccion
            );

            // Correr polling otra vez no duplica reportes de S-3.
            let poll2 = revisar_actualizaciones_en(&estado_a, None).unwrap();
            assert!(poll2.sprints_actualizados.is_empty());

            // Terminar S-3 -> listar_sprints_en lo cierra solo con su reporte final.
            cambiar_estado_en(&estado_a, &t_s3_pend.id, EstadoTarea::Terminada).unwrap();
            pausa();
            let lista_fin = listar_sprints_en(&estado_a).unwrap();
            pausa();
            let s3_fin = lista_fin.iter().find(|s| s.id == s3.id).unwrap();
            assert!(s3_fin.cerrado);
            assert_eq!(
                s3_fin.reporte.as_ref().unwrap()[0].estado,
                EstadoTarea::Terminada
            );

            // También se inyecta evento forjado de Líder cerrando B-1 en OneDrive para confirmar descarte.
            let mut ev_falso = Evento::nuevo(
                "cerrar_sprint",
                Some(&b1.id),
                LIDER,
                serde_json::to_value(&PayloadCerrarSprint {
                    sprint_id: b1.id.clone(),
                    fecha: hoy_real.clone(),
                })
                .unwrap(),
            );
            ev_falso.timestamp = crate::modelo::ahora_iso();
            crate::eventos::servidor_de_prueba::agregar(&ev_falso);

            std::thread::sleep(std::time::Duration::from_secs(2));

            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b =
                crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora).unwrap();
            println!("B sincroniza cierres y reportes por el servidor de prueba: {sync_b:?}");
            if sync_b.descartados != 1 {
                eprintln!(
                    "eventos.log B:\n{}",
                    std::fs::read_to_string(local_b.join("eventos.log")).unwrap_or_default()
                );
            }
            assert_eq!(sync_b.descartados, 1);

            let sprints_a = listar_sprints_en(&estado_a).unwrap();
            let sprints_b = listar(db_b.conexion()).unwrap();
            assert_eq!(sprints_b, sprints_a);

            let tareas_a = estado_a
                .con_db(|db| db_tareas::listar(db.conexion()))
                .unwrap();
            let tareas_b = db_tareas::listar(db_b.conexion()).unwrap();
            assert_eq!(tareas_b, tareas_a);
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
