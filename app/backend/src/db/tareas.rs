//! Tabla `tareas`. Los campos son los de `Tarea` en el frontend (`src/types.ts`),
//! más `ultimo_changeset_visto` (interno, para la escucha de Unity VC).
//!
//! Aquí solo se guarda y se lee; las reglas de negocio (estados, permisos,
//! cascadas) viven en los comandos.

use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};
use crate::modelo::{EstadoTarea, TipoTarea};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tarea {
    /// `T-101` para una tarea general; `T-101-2` para su segunda subtarea.
    pub id: String,
    pub titulo: String,
    /// Texto opcional; vacío si no hay.
    pub detalles: String,
    pub tipo: TipoTarea,
    pub tarea_padre_id: Option<String>,
    pub archivo_ruta: Option<String>,
    pub estado: EstadoTarea,
    pub pendiente_aprobacion: bool,
    /// `None` hasta que alguien toma la tarea; formato `task/<id>-<nombre>`.
    pub rama_asociada: Option<String>,
    /// `Some("rama")` desde que se tomó.
    pub modo_trabajo: Option<String>,
    /// Indicador local de "hay cambios nuevos".
    pub modificada: bool,
    /// Último changeset de la rama que ya vio esta máquina.
    #[serde(default)]
    pub ultimo_changeset_visto: Option<i64>,
    pub creado_por: String,
    /// `AAAA-MM-DD`, fecha local del equipo.
    pub fecha_creacion: String,
    pub asignado_a: Option<String>,
    pub lider_revision_asignado: Option<String>,
    pub sprint_id: Option<String>,
    pub backlog_id: Option<String>,
    pub con_fecha: bool,
    pub fecha_limite: Option<String>,
    pub fecha_completada: Option<String>,
    pub eliminada: bool,
    pub justificacion_eliminacion: Option<String>,
    pub fecha_eliminacion: Option<String>,
    pub motivo_rechazo: Option<String>,
    pub reporte_testing_origen_id: Option<String>,
    pub tarea_origen_id: Option<String>,
    #[serde(default)]
    pub reaperturas: u32,
    /// No se guarda: `true` si la tarea tiene una fila en `ediciones_pendientes`.
    #[serde(default)]
    pub edicion_pendiente: bool,
}

impl Tarea {
    /// Tarea recién creada: "por hacer", sin rama ni responsable.
    pub fn nueva(id: &str, titulo: &str, tipo: TipoTarea, creado_por: &str, fecha: &str) -> Self {
        Tarea {
            id: id.into(),
            titulo: titulo.into(),
            detalles: String::new(),
            tipo,
            tarea_padre_id: None,
            archivo_ruta: None,
            estado: EstadoTarea::PorHacer,
            pendiente_aprobacion: false,
            rama_asociada: None,
            modo_trabajo: None,
            modificada: false,
            ultimo_changeset_visto: None,
            creado_por: creado_por.into(),
            fecha_creacion: fecha.into(),
            asignado_a: None,
            lider_revision_asignado: None,
            sprint_id: None,
            backlog_id: None,
            con_fecha: false,
            fecha_limite: None,
            fecha_completada: None,
            eliminada: false,
            justificacion_eliminacion: None,
            fecha_eliminacion: None,
            motivo_rechazo: None,
            reporte_testing_origen_id: None,
            tarea_origen_id: None,
            reaperturas: 0,
            edicion_pendiente: false,
        }
    }
}

/// Columnas guardadas, en el orden de `valores` y `de_fila`.
const COLUMNAS: [&str; 28] = [
    "id",
    "titulo",
    "detalles",
    "tipo",
    "tarea_padre_id",
    "archivo_ruta",
    "estado",
    "pendiente_aprobacion",
    "rama_asociada",
    "modo_trabajo",
    "modificada",
    "ultimo_changeset_visto",
    "creado_por",
    "fecha_creacion",
    "asignado_a",
    "lider_revision_asignado",
    "sprint_id",
    "backlog_id",
    "con_fecha",
    "fecha_limite",
    "fecha_completada",
    "eliminada",
    "justificacion_eliminacion",
    "fecha_eliminacion",
    "motivo_rechazo",
    "reporte_testing_origen_id",
    "tarea_origen_id",
    "reaperturas",
];

fn valores(t: &Tarea) -> [&dyn ToSql; 28] {
    [
        &t.id,
        &t.titulo,
        &t.detalles,
        &t.tipo,
        &t.tarea_padre_id,
        &t.archivo_ruta,
        &t.estado,
        &t.pendiente_aprobacion,
        &t.rama_asociada,
        &t.modo_trabajo,
        &t.modificada,
        &t.ultimo_changeset_visto,
        &t.creado_por,
        &t.fecha_creacion,
        &t.asignado_a,
        &t.lider_revision_asignado,
        &t.sprint_id,
        &t.backlog_id,
        &t.con_fecha,
        &t.fecha_limite,
        &t.fecha_completada,
        &t.eliminada,
        &t.justificacion_eliminacion,
        &t.fecha_eliminacion,
        &t.motivo_rechazo,
        &t.reporte_testing_origen_id,
        &t.tarea_origen_id,
        &t.reaperturas,
    ]
}

fn de_fila(f: &Row<'_>) -> rusqlite::Result<Tarea> {
    Ok(Tarea {
        id: f.get(0)?,
        titulo: f.get(1)?,
        detalles: f.get(2)?,
        tipo: f.get(3)?,
        tarea_padre_id: f.get(4)?,
        archivo_ruta: f.get(5)?,
        estado: f.get(6)?,
        pendiente_aprobacion: f.get(7)?,
        rama_asociada: f.get(8)?,
        modo_trabajo: f.get(9)?,
        modificada: f.get(10)?,
        ultimo_changeset_visto: f.get(11)?,
        creado_por: f.get(12)?,
        fecha_creacion: f.get(13)?,
        asignado_a: f.get(14)?,
        lider_revision_asignado: f.get(15)?,
        sprint_id: f.get(16)?,
        backlog_id: f.get(17)?,
        con_fecha: f.get(18)?,
        fecha_limite: f.get(19)?,
        fecha_completada: f.get(20)?,
        eliminada: f.get(21)?,
        justificacion_eliminacion: f.get(22)?,
        fecha_eliminacion: f.get(23)?,
        motivo_rechazo: f.get(24)?,
        reporte_testing_origen_id: f.get(25)?,
        tarea_origen_id: f.get(26)?,
        reaperturas: f.get(27)?,
        edicion_pendiente: f.get(28)?,
    })
}

/// `SELECT` de todas las columnas más `edicion_pendiente` calculada.
fn seleccion() -> String {
    let columnas = COLUMNAS.map(|c| format!("t.{c}")).join(", ");
    format!(
        "SELECT {columnas}, EXISTS (SELECT 1 FROM ediciones_pendientes e WHERE e.tarea_id = t.id) FROM tareas t"
    )
}

fn exigir_titulo(t: &Tarea) -> Resultado<()> {
    if t.titulo.trim().is_empty() {
        return Err(Error::Validacion("El título es obligatorio".into()));
    }
    Ok(())
}

pub fn insertar(c: &Connection, tarea: &Tarea) -> Resultado<()> {
    exigir_titulo(tarea)?;
    if buscar(c, &tarea.id)?.is_some() {
        return Err(Error::Validacion(format!(
            "Ya existe la tarea {}",
            tarea.id
        )));
    }
    let marcas = (1..=COLUMNAS.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    c.execute(
        &format!(
            "INSERT INTO tareas ({}) VALUES ({marcas})",
            COLUMNAS.join(", ")
        ),
        valores(tarea),
    )?;
    Ok(())
}

/// Guarda todos los campos de una tarea que ya existe.
pub fn actualizar(c: &Connection, tarea: &Tarea) -> Resultado<()> {
    exigir_titulo(tarea)?;
    let asignaciones = COLUMNAS
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, col)| format!("{col} = ?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let cambiadas = c.execute(
        &format!("UPDATE tareas SET {asignaciones} WHERE id = ?1"),
        valores(tarea),
    )?;
    if cambiadas == 0 {
        return Err(Error::NoEncontrado(format!("la tarea {}", tarea.id)));
    }
    Ok(())
}

pub fn buscar(c: &Connection, id: &str) -> Resultado<Option<Tarea>> {
    Ok(
        c.query_row(&format!("{} WHERE t.id = ?1", seleccion()), [id], de_fila)
            .optional()?,
    )
}

/// Como `buscar`, pero falla con `no_encontrado` si no existe.
pub fn obtener(c: &Connection, id: &str) -> Resultado<Tarea> {
    buscar(c, id)?.ok_or_else(|| Error::NoEncontrado(format!("la tarea {id}")))
}

/// Todas las tareas (también las de la Papelera), en orden de creación.
pub fn listar(c: &Connection) -> Resultado<Vec<Tarea>> {
    let mut consulta = c.prepare(&format!("{} ORDER BY t.rowid", seleccion()))?;
    let filas = consulta.query_map([], de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Subtareas directas de una tarea (incluye las eliminadas).
pub fn hijas(c: &Connection, id_padre: &str) -> Resultado<Vec<Tarea>> {
    let mut consulta = c.prepare(&format!(
        "{} WHERE t.tarea_padre_id = ?1 ORDER BY t.rowid",
        seleccion()
    ))?;
    let filas = consulta.query_map([id_padre], de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Borra la fila por completo (eliminación definitiva desde la Papelera).
pub fn borrar_fila(c: &Connection, id: &str) -> Resultado<()> {
    if c.execute("DELETE FROM tareas WHERE id = ?1", [id])? == 0 {
        return Err(Error::NoEncontrado(format!("la tarea {id}")));
    }
    Ok(())
}

/// Siguiente id libre: `T-<n>` para una tarea general, `<padre>-<n>` para una subtarea.
/// Cuenta también las eliminadas, para no reutilizar números.
pub fn siguiente_id(c: &Connection, id_padre: Option<&str>) -> Resultado<String> {
    let (prefijo, ids): (String, Vec<String>) = match id_padre {
        Some(padre) => {
            let mut consulta = c.prepare("SELECT id FROM tareas WHERE tarea_padre_id = ?1")?;
            let ids = consulta
                .query_map([padre], |f| f.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            (format!("{padre}-"), ids)
        }
        None => {
            let mut consulta = c.prepare("SELECT id FROM tareas WHERE tarea_padre_id IS NULL")?;
            let ids = consulta
                .query_map([], |f| f.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            ("T-".to_string(), ids)
        }
    };
    let mayor = ids
        .iter()
        .filter_map(|id| id.strip_prefix(&prefijo)?.parse::<u32>().ok())
        .max()
        .unwrap_or(if id_padre.is_some() { 0 } else { 100 });
    Ok(format!("{prefijo}{}", mayor + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn tarea(id: &str) -> Tarea {
        Tarea::nueva(
            id,
            "Movimiento del jugador",
            TipoTarea::Scripting,
            "ana@virustf.dev",
            "2026-11-09",
        )
    }

    #[test]
    fn insertar_leer_y_actualizar_todos_los_campos() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &tarea("T-101")).unwrap();
        assert_eq!(obtener(c, "T-101").unwrap(), tarea("T-101"));

        let mut t = tarea("T-101");
        t.detalles = "Con salto doble".into();
        t.estado = EstadoTarea::EnProduccion;
        t.asignado_a = Some("zoe@virustf.dev".into());
        t.rama_asociada = Some("task/T-101-movimiento".into());
        t.modo_trabajo = Some("rama".into());
        t.ultimo_changeset_visto = Some(42);
        t.sprint_id = Some("S-1".into());
        t.backlog_id = Some("B-1".into());
        t.con_fecha = true;
        t.fecha_limite = Some("2026-12-01".into());
        t.reaperturas = 2;
        t.modificada = true;
        actualizar(c, &t).unwrap();
        assert_eq!(obtener(c, "T-101").unwrap(), t);
    }

    #[test]
    fn validaciones_basicas() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &tarea("T-101")).unwrap();
        assert_eq!(
            insertar(c, &tarea("T-101")).unwrap_err().codigo(),
            "validacion"
        );
        let mut sin_titulo = tarea("T-102");
        sin_titulo.titulo = "  ".into();
        assert!(insertar(c, &sin_titulo).is_err());
        assert_eq!(
            actualizar(c, &tarea("T-999")).unwrap_err().codigo(),
            "no_encontrado"
        );
        assert_eq!(obtener(c, "T-999").unwrap_err().codigo(), "no_encontrado");
        let mut modo_raro = tarea("T-103");
        modo_raro.modo_trabajo = Some("checkin".into());
        assert!(insertar(c, &modo_raro).is_err());
    }

    #[test]
    fn hijas_listado_y_borrado() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &tarea("T-101")).unwrap();
        for id in ["T-101-1", "T-101-2"] {
            let mut sub = tarea(id);
            sub.tarea_padre_id = Some("T-101".into());
            sub.tipo = TipoTarea::SinDefinir;
            insertar(c, &sub).unwrap();
        }
        assert_eq!(listar(c).unwrap().len(), 3);
        let subs = hijas(c, "T-101").unwrap();
        assert_eq!(
            subs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["T-101-1", "T-101-2"]
        );
        assert_eq!(subs[0].tipo, TipoTarea::SinDefinir);
        borrar_fila(c, "T-101-1").unwrap();
        assert_eq!(hijas(c, "T-101").unwrap().len(), 1);
        assert!(borrar_fila(c, "T-101-1").is_err());
    }

    #[test]
    fn siguiente_id_no_reutiliza_numeros() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        assert_eq!(siguiente_id(c, None).unwrap(), "T-101");
        insertar(c, &tarea("T-101")).unwrap();
        insertar(c, &tarea("T-205")).unwrap();
        assert_eq!(siguiente_id(c, None).unwrap(), "T-206");
        assert_eq!(siguiente_id(c, Some("T-101")).unwrap(), "T-101-1");
        let mut sub = tarea("T-101-3");
        sub.tarea_padre_id = Some("T-101".into());
        sub.eliminada = true;
        insertar(c, &sub).unwrap();
        assert_eq!(siguiente_id(c, Some("T-101")).unwrap(), "T-101-4");
        assert_eq!(siguiente_id(c, None).unwrap(), "T-206");
    }

    #[test]
    fn se_serializa_con_los_campos_del_frontend() {
        let json = serde_json::to_value(tarea("T-101")).unwrap();
        assert_eq!(json["estado"], "por hacer");
        assert_eq!(json["tipo"], "Scripting");
        assert_eq!(json["edicion_pendiente"], false);
        assert!(json["tarea_padre_id"].is_null());
    }
}
