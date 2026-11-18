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
    /// `None` hasta que alguien toma la tarea; ruta completa de la rama, ej. `/main/dev/T-101-combate` (#83).
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
    /// Fecha/hora ISO de asignación o paso a producción; interno para inactividad (#117).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actividad_desde: Option<String>,
    /// No se guarda: `true` si la tarea tiene una fila en `ediciones_pendientes`.
    #[serde(default)]
    pub edicion_pendiente: bool,
    /// No se guarda en `tareas`: la fila de `cambios_directos` si es una tarea roja
    /// por check-ins directos en `/main` o `/main/dev` (#225).
    #[serde(default)]
    pub cambios_directos: Option<CambiosDirectos>,
}

/// Un check-in hecho directamente en `/main` o `/main/dev` (fuera de BranchBoard, #225).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesetDirecto {
    pub changeset: i64,
    /// Changeset anterior en la misma rama (`parentChangesetId` de la API).
    #[serde(default)]
    pub padre: Option<i64>,
    /// Correo de la cuenta de Unity que hizo el check-in.
    pub autor: String,
    /// Fecha en UTC, como la da la API (`2026-11-12T17:20:04Z`).
    pub fecha: String,
    #[serde(default)]
    pub comentario: Option<String>,
    #[serde(default)]
    pub archivos: Vec<String>,
}

/// Datos de una tarea roja (#225): en qué rama están los cambios y si ya se
/// trasladaron a la rama de la tarea y se revirtieron de ahí.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CambiosDirectos {
    /// `/main` o `/main/dev`.
    pub rama: String,
    /// Hora ISO en que se detectó (la del evento): desempata dos detecciones a la vez.
    pub detectado: String,
    pub changesets: Vec<ChangesetDirecto>,
    /// Rama a la que se trasladaron, `None` mientras sigan solo en `rama`.
    #[serde(default)]
    pub rama_traslado: Option<String>,
    /// Changesets que los revirtieron en `rama` (merge sustractivo).
    #[serde(default)]
    pub changesets_reversion: Vec<i64>,
    #[serde(default)]
    pub trasladado_en: Option<String>,
    #[serde(default)]
    pub trasladado_por: Option<String>,
}

impl CambiosDirectos {
    /// Sigue en rojo: los cambios todavía están solo en `main`/`dev`.
    pub fn pendiente(&self) -> bool {
        self.trasladado_en.is_none()
    }

    pub fn incluye(&self, changeset: i64) -> bool {
        self.changesets.iter().any(|c| c.changeset == changeset)
    }
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
            actividad_desde: None,
            edicion_pendiente: false,
            cambios_directos: None,
        }
    }
}

/// Columnas guardadas, en el orden de `valores` y `de_fila`.
const COLUMNAS: [&str; 29] = [
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
    "actividad_desde",
];

fn valores(t: &Tarea) -> [&dyn ToSql; 29] {
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
        &t.actividad_desde,
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
        actividad_desde: f.get(28)?,
        edicion_pendiente: f.get(29)?,
        cambios_directos: match f.get::<_, Option<String>>(30)? {
            Some(json) => Some(serde_json::from_str(&json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    30,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?),
            None => None,
        },
    })
}

/// `SELECT` de todas las columnas más `edicion_pendiente` y `cambios_directos` calculados.
fn seleccion() -> String {
    let columnas = COLUMNAS.map(|c| format!("t.{c}")).join(", ");
    format!(
        "SELECT {columnas}, EXISTS (SELECT 1 FROM ediciones_pendientes e WHERE e.tarea_id = t.id),
            (SELECT json_object(
                'rama', d.rama, 'detectado', d.detectado, 'changesets', json(d.changesets),
                'rama_traslado', d.rama_traslado,
                'changesets_reversion', json(d.changesets_reversion),
                'trasladado_en', d.trasladado_en, 'trasladado_por', d.trasladado_por)
             FROM cambios_directos d WHERE d.tarea_id = t.id)
         FROM tareas t"
    )
}

/// Guarda (o reemplaza) la fila de `cambios_directos` de una tarea roja (#225).
pub fn guardar_cambios_directos(
    c: &Connection,
    tarea_id: &str,
    datos: &CambiosDirectos,
) -> Resultado<()> {
    let changesets = serde_json::to_string(&datos.changesets)
        .map_err(|e| Error::Interno(format!("no se pudo guardar cambios_directos: {e}")))?;
    let reversion = serde_json::to_string(&datos.changesets_reversion)
        .map_err(|e| Error::Interno(format!("no se pudo guardar cambios_directos: {e}")))?;
    c.execute(
        "INSERT INTO cambios_directos (tarea_id, rama, detectado, changesets, rama_traslado,
             changesets_reversion, trasladado_en, trasladado_por)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (tarea_id) DO UPDATE SET rama = ?2, detectado = ?3, changesets = ?4,
             rama_traslado = ?5, changesets_reversion = ?6, trasladado_en = ?7, trasladado_por = ?8",
        rusqlite::params![
            tarea_id,
            datos.rama,
            datos.detectado,
            changesets,
            datos.rama_traslado,
            reversion,
            datos.trasladado_en,
            datos.trasladado_por
        ],
    )?;
    Ok(())
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

/// Tareas en la Papelera, ordenadas de la eliminación más reciente a la más
/// antigua (a igual fecha, por `id` descendente).
pub fn listar_papelera(c: &Connection) -> Resultado<Vec<Tarea>> {
    let mut consulta = c.prepare(&format!(
        "{} WHERE t.eliminada = 1 ORDER BY COALESCE(t.fecha_eliminacion, '') DESC, t.id DESC",
        seleccion()
    ))?;
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
        t.rama_asociada = Some("/main/dev/T-101-movimiento".into());
        t.modo_trabajo = Some("rama".into());
        t.ultimo_changeset_visto = Some(42);
        t.sprint_id = Some("S-1".into());
        t.backlog_id = Some("B-1".into());
        t.con_fecha = true;
        t.fecha_limite = Some("2026-12-01".into());
        t.reaperturas = 2;
        t.modificada = true;
        t.actividad_desde = Some("2026-11-10T09:30:00Z".into());
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
