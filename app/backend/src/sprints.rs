//! Reglas de planificación: creación y listado de sprints/backlogs y asignación
//! de tareas (issue #113). Cada mutación compartida pasa por `ops/` y se aplica
//! igual en todas las máquinas mediante [`aplicar_evento`].

use std::path::Path;

use chrono::{Days, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::sprints::{self as db_sprints, Sprint};
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::{normalizar_email, TipoTarea};
use crate::tareas::{rol_verificado_de, AutorTarea};

/// Tipos de evento que maneja este módulo.
pub const TIPOS: &[&str] = &[
    "crear_sprint",
    "asignar_tarea_a_sprint",
    "asignar_tarea_a_backlog",
    "asignar_backlog_a_sprint",
];

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

fn exigir_lider_o_admin_evento(c: &Connection, publica_admin: &str, email: &str) -> Resultado<()> {
    match rol_verificado_de(c, publica_admin, email)? {
        Some(rol) if rol.es_lider_o_admin() => Ok(()),
        _ => Err(Error::SinPermiso),
    }
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
    ops: &Path,
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
    eventos::publicar(c, ops, &ev, |tx| aplicar_crear_sprint(tx, &sprint))?;
    db_sprints::obtener(c, &sprint.id)
}

/// Asigna una tarea a `sprint_id` (`None` la quita de su sprint actual), solo Líder/Admin (#113).
/// Rechaza asignarla si ya pertenece a otro sprint.
pub fn asignar_tarea_a_sprint(
    c: &Connection,
    ops: &Path,
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
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_asignar_tarea_a_sprint(tx, &payload)
    })?;
    db_tareas::obtener(c, tarea_id)
}

/// Asigna una tarea a `backlog_id` (`None` la quita de su backlog actual), solo Líder/Admin (#113).
/// Rechaza asignarla si ya pertenece a otro backlog.
pub fn asignar_tarea_a_backlog(
    c: &Connection,
    ops: &Path,
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
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_asignar_tarea_a_backlog(tx, &payload)
    })?;
    db_tareas::obtener(c, tarea_id)
}

/// Pasa a `sprint_id` todas las tareas libres (`sprint_id = null`, no eliminadas y no `En espera`)
/// que están en `backlog_id` (`None` = tareas sin backlog), solo Líder/Admin (#113).
pub fn asignar_backlog_a_sprint(
    c: &Connection,
    ops: &Path,
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
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_asignar_backlog_a_sprint(tx, &payload)
    })?;

    let mut actualizadas = Vec::with_capacity(elegibles.len());
    for id in elegibles {
        actualizadas.push(db_tareas::obtener(c, &id)?);
    }
    Ok(actualizadas)
}

/// Reproduce un evento de planificación recibido de `ops/`.
pub fn aplicar_evento(c: &Connection, publica_admin: &str, evento: &Evento) -> Resultado<()> {
    exigir_lider_o_admin_evento(c, publica_admin, &evento.usuario)?;
    match evento.tipo.as_str() {
        "crear_sprint" => {
            let sprint: Sprint = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_sprint inválido: {e}")))?;
            aplicar_crear_sprint(c, &sprint)
        }
        "asignar_tarea_a_sprint" => {
            let payload: PayloadAsignarTareaGrupo = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("asignar_tarea_a_sprint inválido: {e}")))?;
            aplicar_asignar_tarea_a_sprint(c, &payload)
        }
        "asignar_tarea_a_backlog" => {
            let payload: PayloadAsignarTareaGrupo = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("asignar_tarea_a_backlog inválido: {e}")))?;
            aplicar_asignar_tarea_a_backlog(c, &payload)
        }
        "asignar_backlog_a_sprint" => {
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
    use crate::eventos::carpeta_ops;
    use crate::eventos::configurar_carpeta_compartida;
    use crate::eventos::lectura::{self, sincronizar};
    use crate::identidad::equipo::{self, Firmante};
    use crate::identidad::firmas;
    use crate::identidad::llaves::{
        cargar_llave_admin, fijar_llave_publica_admin, generar_llaves_admin,
    };
    use crate::modelo::Rol;
    use crate::tareas::{crear, crear_en_espera, eliminar, DatosNuevaTarea};

    const ADMIN: &str = "admin@estudio.com";
    const LIDER: &str = "lider@estudio.com";
    const DEV: &str = "dev@estudio.com";
    const TESTER: &str = "tester@estudio.com";
    const AHORA: &str = "2026-11-16T10:00:00.000Z";
    const HOY: &str = "2026-11-16";

    struct Entorno {
        _tmp: TempDir,
        db_a: Db,
        ops: PathBuf,
        local_a: PathBuf,
        db_b: Db,
        local_b: PathBuf,
        publica: String,
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
        configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
        configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
        let ops = carpeta_ops(&local_a).unwrap();

        let publica = generar_llaves_admin(&local_a, None).unwrap();
        fijar_llave_publica_admin(&local_b, &publica).unwrap();
        let privada = cargar_llave_admin(&local_a, None).unwrap();

        let db_a = Db::abrir(&local_a).unwrap();
        let db_b = Db::abrir(&local_b).unwrap();

        let admin_fila = crate::db::usuarios::Usuario {
            email: ADMIN.into(),
            nombre: "Admin".into(),
            apellido: "Uno".into(),
            rol: Rol::Administrador,
            fecha_alta: HOY.into(),
            firma: Some(firmas::firmar_rol(&privada, ADMIN, Rol::Administrador, AHORA).unwrap()),
            firmado_en: Some(AHORA.into()),
            firmado_por: Some(ADMIN.into()),
            llave_publica: None,
            certificado: None,
        };
        usuarios::insertar(db_a.conexion(), &admin_fila).unwrap();
        usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

        let firmante = Firmante {
            email: ADMIN,
            rol: Rol::Administrador,
            llave: &privada,
        };
        for (idx, (correo, rol)) in [
            (LIDER, Rol::Lider),
            (DEV, Rol::Desarrollador),
            (TESTER, Rol::Tester),
        ]
        .into_iter()
        .enumerate()
        {
            let ts = format!("2026-11-16T10:00:0{idx}.000Z");
            equipo::crear(
                db_a.conexion(),
                &ops,
                &publica,
                &firmante,
                correo,
                rol,
                &ts,
                HOY,
            )
            .unwrap();
        }
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        sincronizar(db_b.conexion(), &ops, &local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, &publica, ev)
        })
        .unwrap();

        Entorno {
            _tmp: tmp,
            db_a,
            ops,
            local_a,
            db_b,
            local_b,
            publica,
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
            &e.ops,
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
            &e.ops,
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
                &e.ops,
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
                &e.ops,
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
                &e.ops,
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
            &e.ops,
            &autor_lider(),
            &t.id,
            Some(&s1.id),
            "2026-11-16T10:01:08.000Z",
        )
        .unwrap();
        let t = asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
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
                &e.ops,
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
                &e.ops,
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
            &e.ops,
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
                &e.ops,
                &autor_lider(),
                &t.id,
                Some(&s2.id),
                "2026-11-16T10:01:13.000Z",
            ),
            Err(Error::Validacion(_))
        ));
        let t = asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t.id,
            None,
            "2026-11-16T10:01:14.000Z",
        )
        .unwrap();
        let t = asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
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
            &e.ops,
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
                &e.ops,
                &autor_lider(),
                &en_espera.id,
                Some(&s1.id),
                "2026-11-16T10:01:18.000Z",
            ),
            Err(Error::Validacion(_))
        ));

        eliminar(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t.id,
            "Obsol",
            "2026-11-16T10:01:19.000Z",
        )
        .unwrap();
        assert!(matches!(
            asignar_tarea_a_backlog(
                e.db_a.conexion(),
                &e.ops,
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
            &e.ops,
            &autor_lider(),
            &t_libre_b1.id,
            Some(&b1.id),
            "2026-11-16T10:01:09.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_ocupada_b1.id,
            Some(&b1.id),
            "2026-11-16T10:01:10.000Z",
        )
        .unwrap();
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_ocupada_b1.id,
            Some(&s2.id),
            "2026-11-16T10:01:11.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_eliminada_b1.id,
            Some(&b1.id),
            "2026-11-16T10:01:12.000Z",
        )
        .unwrap();
        eliminar(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_eliminada_b1.id,
            "Descartada",
            "2026-11-16T10:01:13.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_en_b2.id,
            Some(&b2.id),
            "2026-11-16T10:01:14.000Z",
        )
        .unwrap();

        // Pasar B1 -> S1 solo mueve `t_libre_b1`.
        let movidas = asignar_backlog_a_sprint(
            e.db_a.conexion(),
            &e.ops,
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
            &e.ops,
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
            &e.ops,
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
        eventos::escribir_evento(&e.ops, &ev_falso).unwrap();

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b = sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, &e.publica, ev)
        })
        .unwrap();
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
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_sprints_y_backlogs() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::sprints::{
            asignar_backlog_a_sprint_en, asignar_tarea_a_backlog_en, asignar_tarea_a_sprint_en,
            crear_sprint_en, listar_sprints_en,
        };
        use crate::estado::{llave_publica_del_proyecto, AppState};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
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

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let (publica, admin_fila) = estado_a
                .con_db(|db| {
                    let pub_k = llave_publica_del_proyecto(db)?;
                    let todos = usuarios::listar(db.conexion())?;
                    Ok((pub_k, todos.into_iter().next().unwrap()))
                })
                .unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let llave_admin = cargar_llave_admin(&local_a, None).unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            estado_a
                .con_db(|db_a| {
                    let firmante_admin = Firmante {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                        llave: &llave_admin,
                    };
                    equipo::crear(
                        db_a.conexion(),
                        &ops_a,
                        &publica,
                        &firmante_admin,
                        DEV,
                        Rol::Desarrollador,
                        "2026-11-15T10:00:01.000Z",
                        HOY,
                    )?;
                    equipo::crear(
                        db_a.conexion(),
                        &ops_a,
                        &publica,
                        &firmante_admin,
                        LIDER,
                        Rol::Lider,
                        "2026-11-15T10:00:02.000Z",
                        HOY,
                    )?;
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
                        &ops_a,
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
                        &ops_a,
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
                        &ops_a,
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
                            &ops_a,
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
                            &ops_a,
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
            eventos::escribir_evento(&ops_a, &ev_falso).unwrap();

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 3. Máquina B sincroniza desde OneDrive y obtiene idénticos sprints y tareas.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            println!("B sincroniza planificación desde OneDrive: {sync_b:?}");
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
