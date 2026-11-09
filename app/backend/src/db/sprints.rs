//! Tablas `sprints` y `reportes_sprint`.
//!
//! Un sprint (`con_fecha = true`) tiene duración y fecha de fin; un backlog
//! (`con_fecha = false`) es un grupo de tareas con nombre, sin fecha de fin.
//! `reportes_sprint` guarda la **copia de las tareas** tal como estaban al
//! cerrarse el sprint o al llegar su fecha de fin: Reportes y la gráfica de un
//! sprint cerrado se arman con esa copia, no con las tareas vivas.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::tareas::Tarea;
use crate::error::{Error, Resultado};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sprint {
    pub id: String,
    pub nombre: String,
    pub descripcion: String,
    /// `true` = sprint; `false` = backlog.
    pub con_fecha: bool,
    /// `AAAA-MM-DD`; siempre tiene valor.
    pub fecha_inicio: String,
    /// Solo sprints.
    pub duracion_dias: Option<u32>,
    /// Solo sprints: `fecha_inicio + duracion_dias`, calculada al crearlo.
    pub fecha_fin: Option<String>,
    pub cerrado: bool,
    pub fecha_cierre_real: Option<String>,
    pub creado_por: String,
    /// No se guarda en `sprints`: copia de sus tareas del reporte más reciente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporte: Option<Vec<Tarea>>,
}

/// Una fila de `reportes_sprint`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReporteSprint {
    pub sprint_id: String,
    /// `AAAA-MM-DD` en que se tomó la copia.
    pub fecha: String,
    /// `true` si es el reporte del cierre; `false` si es el de la fecha de fin
    /// de un sprint que siguió abierto.
    pub de_cierre: bool,
    pub tareas: Vec<Tarea>,
}

const COLUMNAS: &str = "id, nombre, descripcion, con_fecha, fecha_inicio, duracion_dias, fecha_fin, cerrado, fecha_cierre_real, creado_por";

fn de_fila(f: &Row<'_>) -> rusqlite::Result<Sprint> {
    Ok(Sprint {
        id: f.get(0)?,
        nombre: f.get(1)?,
        descripcion: f.get(2)?,
        con_fecha: f.get(3)?,
        fecha_inicio: f.get(4)?,
        duracion_dias: f.get(5)?,
        fecha_fin: f.get(6)?,
        cerrado: f.get(7)?,
        fecha_cierre_real: f.get(8)?,
        creado_por: f.get(9)?,
        reporte: None,
    })
}

fn validar(s: &Sprint) -> Resultado<()> {
    if s.nombre.trim().is_empty() {
        return Err(Error::Validacion("El nombre es obligatorio".into()));
    }
    if s.con_fecha && (s.duracion_dias.unwrap_or(0) == 0 || s.fecha_fin.is_none()) {
        return Err(Error::Validacion(
            "Un sprint necesita su duración en días".into(),
        ));
    }
    if !s.con_fecha && (s.duracion_dias.is_some() || s.fecha_fin.is_some()) {
        return Err(Error::Validacion(
            "Un backlog no tiene duración ni fecha de fin".into(),
        ));
    }
    Ok(())
}

pub fn insertar(c: &Connection, sprint: &Sprint) -> Resultado<()> {
    validar(sprint)?;
    if buscar(c, &sprint.id)?.is_some() {
        return Err(Error::Validacion(format!("Ya existe {}", sprint.id)));
    }
    c.execute(
        &format!(
            "INSERT INTO sprints ({COLUMNAS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            sprint.id,
            sprint.nombre,
            sprint.descripcion,
            sprint.con_fecha,
            sprint.fecha_inicio,
            sprint.duracion_dias,
            sprint.fecha_fin,
            sprint.cerrado,
            sprint.fecha_cierre_real,
            sprint.creado_por
        ],
    )?;
    Ok(())
}

/// Guarda los campos que cambian después de crear el sprint.
pub fn actualizar(c: &Connection, sprint: &Sprint) -> Resultado<()> {
    validar(sprint)?;
    let cambiadas = c.execute(
        "UPDATE sprints SET nombre = ?2, descripcion = ?3, cerrado = ?4, fecha_cierre_real = ?5 WHERE id = ?1",
        params![
            sprint.id,
            sprint.nombre,
            sprint.descripcion,
            sprint.cerrado,
            sprint.fecha_cierre_real
        ],
    )?;
    if cambiadas == 0 {
        return Err(Error::NoEncontrado(format!("el sprint {}", sprint.id)));
    }
    Ok(())
}

/// Sprint o backlog por id, sin su reporte.
pub fn buscar(c: &Connection, id: &str) -> Resultado<Option<Sprint>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS} FROM sprints WHERE id = ?1"),
        [id],
        de_fila,
    )
    .optional()?)
}

pub fn obtener(c: &Connection, id: &str) -> Resultado<Sprint> {
    buscar(c, id)?.ok_or_else(|| Error::NoEncontrado(format!("el sprint {id}")))
}

/// Todos los sprints y backlogs en orden de creación, cada uno con la copia de
/// tareas de su reporte más reciente (si tiene).
pub fn listar(c: &Connection) -> Resultado<Vec<Sprint>> {
    let mut consulta = c.prepare(&format!("SELECT {COLUMNAS} FROM sprints ORDER BY rowid"))?;
    let mut sprints: Vec<Sprint> = consulta
        .query_map([], de_fila)?
        .collect::<rusqlite::Result<_>>()?;
    for sprint in &mut sprints {
        sprint.reporte = ultimo_reporte(c, &sprint.id)?.map(|r| r.tareas);
    }
    Ok(sprints)
}

/// Siguiente id libre: `S-<n>` para sprints, `B-<n>` para backlogs.
pub fn siguiente_id(c: &Connection, con_fecha: bool) -> Resultado<String> {
    let prefijo = if con_fecha { "S-" } else { "B-" };
    let mut consulta = c.prepare("SELECT id FROM sprints")?;
    let ids: Vec<String> = consulta
        .query_map([], |f| f.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mayor = ids
        .iter()
        .filter_map(|id| id.strip_prefix(prefijo)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    Ok(format!("{prefijo}{}", mayor + 1))
}

/// Guarda la copia de las tareas de un sprint en una fecha.
pub fn guardar_reporte(c: &Connection, reporte: &ReporteSprint) -> Resultado<()> {
    let tareas =
        serde_json::to_string(&reporte.tareas).map_err(|e| Error::Interno(e.to_string()))?;
    c.execute(
        "INSERT INTO reportes_sprint (sprint_id, fecha, de_cierre, tareas) VALUES (?1, ?2, ?3, ?4)",
        params![reporte.sprint_id, reporte.fecha, reporte.de_cierre, tareas],
    )?;
    Ok(())
}

fn reporte_de_fila(f: &Row<'_>) -> rusqlite::Result<ReporteSprint> {
    let texto: String = f.get(3)?;
    Ok(ReporteSprint {
        sprint_id: f.get(0)?,
        fecha: f.get(1)?,
        de_cierre: f.get(2)?,
        tareas: serde_json::from_str(&texto).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}

/// Reportes de un sprint, del más antiguo al más reciente.
pub fn reportes(c: &Connection, sprint_id: &str) -> Resultado<Vec<ReporteSprint>> {
    let mut consulta = c.prepare(
        "SELECT sprint_id, fecha, de_cierre, tareas FROM reportes_sprint WHERE sprint_id = ?1 ORDER BY id",
    )?;
    let filas = consulta.query_map([sprint_id], reporte_de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

pub fn ultimo_reporte(c: &Connection, sprint_id: &str) -> Resultado<Option<ReporteSprint>> {
    Ok(c.query_row(
        "SELECT sprint_id, fecha, de_cierre, tareas FROM reportes_sprint WHERE sprint_id = ?1 ORDER BY id DESC LIMIT 1",
        [sprint_id],
        reporte_de_fila,
    )
    .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::modelo::{EstadoTarea, TipoTarea};

    fn sprint(id: &str) -> Sprint {
        Sprint {
            id: id.into(),
            nombre: "Sprint 1".into(),
            descripcion: String::new(),
            con_fecha: true,
            fecha_inicio: "2026-11-09".into(),
            duracion_dias: Some(14),
            fecha_fin: Some("2026-11-23".into()),
            cerrado: false,
            fecha_cierre_real: None,
            creado_por: "ana@virustf.dev".into(),
            reporte: None,
        }
    }

    fn backlog(id: &str) -> Sprint {
        Sprint {
            nombre: "Arte".into(),
            con_fecha: false,
            duracion_dias: None,
            fecha_fin: None,
            ..sprint(id)
        }
    }

    #[test]
    fn insertar_leer_y_cerrar() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &sprint("S-1")).unwrap();
        insertar(c, &backlog("B-1")).unwrap();
        assert_eq!(obtener(c, "S-1").unwrap(), sprint("S-1"));
        assert_eq!(obtener(c, "B-1").unwrap(), backlog("B-1"));

        let mut cerrado = sprint("S-1");
        cerrado.cerrado = true;
        cerrado.fecha_cierre_real = Some("2026-11-20".into());
        actualizar(c, &cerrado).unwrap();
        assert_eq!(obtener(c, "S-1").unwrap(), cerrado);
        assert_eq!(
            actualizar(c, &sprint("S-9")).unwrap_err().codigo(),
            "no_encontrado"
        );
    }

    #[test]
    fn validaciones() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        let mut sin_duracion = sprint("S-1");
        sin_duracion.duracion_dias = None;
        assert!(insertar(c, &sin_duracion).is_err());
        let mut backlog_con_fin = backlog("B-1");
        backlog_con_fin.fecha_fin = Some("2026-12-01".into());
        assert!(insertar(c, &backlog_con_fin).is_err());
        let mut sin_nombre = sprint("S-1");
        sin_nombre.nombre = " ".into();
        assert!(insertar(c, &sin_nombre).is_err());
        insertar(c, &sprint("S-1")).unwrap();
        assert!(insertar(c, &sprint("S-1")).is_err());
    }

    #[test]
    fn ids_separados_para_sprints_y_backlogs() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        assert_eq!(siguiente_id(c, true).unwrap(), "S-1");
        assert_eq!(siguiente_id(c, false).unwrap(), "B-1");
        insertar(c, &sprint("S-3")).unwrap();
        insertar(c, &backlog("B-1")).unwrap();
        assert_eq!(siguiente_id(c, true).unwrap(), "S-4");
        assert_eq!(siguiente_id(c, false).unwrap(), "B-2");
    }

    #[test]
    fn el_reporte_es_una_copia_de_las_tareas() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &sprint("S-1")).unwrap();
        insertar(c, &backlog("B-1")).unwrap();
        let mut tarea = Tarea::nueva(
            "T-101",
            "Menú principal",
            TipoTarea::UiInterfaz,
            "ana@virustf.dev",
            "2026-11-09",
        );
        tarea.sprint_id = Some("S-1".into());
        let copia = |fecha: &str, de_cierre: bool, tareas: Vec<Tarea>| ReporteSprint {
            sprint_id: "S-1".into(),
            fecha: fecha.into(),
            de_cierre,
            tareas,
        };
        guardar_reporte(c, &copia("2026-11-23", false, vec![tarea.clone()])).unwrap();
        tarea.estado = EstadoTarea::Terminada;
        tarea.fecha_completada = Some("2026-11-25".into());
        guardar_reporte(c, &copia("2026-11-25", true, vec![tarea.clone()])).unwrap();

        let todos = reportes(c, "S-1").unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].tareas[0].estado, EstadoTarea::PorHacer);
        assert!(todos[1].de_cierre);

        let lista = listar(c).unwrap();
        assert_eq!(lista[0].reporte.as_ref().unwrap()[0], tarea);
        assert_eq!(lista[1].reporte, None);
    }
}
