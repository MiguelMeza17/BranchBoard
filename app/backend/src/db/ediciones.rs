//! Tabla `ediciones_pendientes`: una edición de título y/o detalles que ya está
//! aplicada en la tarea y espera que un Líder/Administrador la confirme o la revierta.
//! Solo puede haber **una** por tarea: una segunda edición se suma a la misma fila.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};

/// Campo → valor (ej. `"titulo" → "Nuevo título"`).
pub type Campos = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdicionPendiente {
    pub id: String,
    pub tarea_id: String,
    /// Valores que ya están aplicados en la tarea.
    pub campos_nuevos: Campos,
    /// Valores de antes de la edición; se restauran si se rechaza.
    pub campos_anteriores: Campos,
    pub motivo: Option<String>,
    pub creado_por: String,
    pub fecha: String,
}

fn de_fila(f: &Row<'_>) -> rusqlite::Result<EdicionPendiente> {
    let json = |i: usize| -> rusqlite::Result<Campos> {
        let texto: String = f.get(i)?;
        serde_json::from_str(&texto).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    Ok(EdicionPendiente {
        id: f.get(0)?,
        tarea_id: f.get(1)?,
        campos_nuevos: json(2)?,
        campos_anteriores: json(3)?,
        motivo: f.get(4)?,
        creado_por: f.get(5)?,
        fecha: f.get(6)?,
    })
}

const SELECCION: &str = "SELECT id, tarea_id, campos_nuevos, campos_anteriores, motivo, creado_por, fecha FROM ediciones_pendientes";

fn a_json(campos: &Campos) -> Resultado<String> {
    serde_json::to_string(campos).map_err(|e| Error::Interno(e.to_string()))
}

pub fn de_tarea(c: &Connection, tarea_id: &str) -> Resultado<Option<EdicionPendiente>> {
    Ok(c.query_row(
        &format!("{SELECCION} WHERE tarea_id = ?1"),
        [tarea_id],
        de_fila,
    )
    .optional()?)
}

pub fn listar(c: &Connection) -> Resultado<Vec<EdicionPendiente>> {
    let mut consulta = c.prepare(&format!("{SELECCION} ORDER BY rowid"))?;
    let filas = consulta.query_map([], de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Registra una edición. Si la tarea ya tenía una pendiente, se **suma** a ella:
/// los campos nuevos se combinan y se conservan los valores anteriores **originales**.
/// Devuelve la fila tal como quedó.
pub fn guardar_o_sumar(c: &Connection, edicion: &EdicionPendiente) -> Resultado<EdicionPendiente> {
    let resultado = match de_tarea(c, &edicion.tarea_id)? {
        Some(mut existente) => {
            for (campo, valor) in &edicion.campos_nuevos {
                existente.campos_nuevos.insert(campo.clone(), valor.clone());
            }
            for (campo, valor) in &edicion.campos_anteriores {
                existente
                    .campos_anteriores
                    .entry(campo.clone())
                    .or_insert_with(|| valor.clone());
            }
            if edicion.motivo.is_some() {
                existente.motivo = edicion.motivo.clone();
            }
            c.execute(
                "UPDATE ediciones_pendientes SET campos_nuevos = ?2, campos_anteriores = ?3, motivo = ?4 WHERE id = ?1",
                params![
                    existente.id,
                    a_json(&existente.campos_nuevos)?,
                    a_json(&existente.campos_anteriores)?,
                    existente.motivo
                ],
            )?;
            existente
        }
        None => {
            c.execute(
                "INSERT INTO ediciones_pendientes (id, tarea_id, campos_nuevos, campos_anteriores, motivo, creado_por, fecha)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    edicion.id,
                    edicion.tarea_id,
                    a_json(&edicion.campos_nuevos)?,
                    a_json(&edicion.campos_anteriores)?,
                    edicion.motivo,
                    edicion.creado_por,
                    edicion.fecha
                ],
            )?;
            edicion.clone()
        }
    };
    Ok(resultado)
}

/// Quita la edición pendiente de una tarea (al aprobarla o rechazarla).
/// Devuelve la fila que había, si había.
pub fn quitar(c: &Connection, tarea_id: &str) -> Resultado<Option<EdicionPendiente>> {
    let existente = de_tarea(c, tarea_id)?;
    c.execute(
        "DELETE FROM ediciones_pendientes WHERE tarea_id = ?1",
        [tarea_id],
    )?;
    Ok(existente)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{tareas, Db};
    use crate::modelo::TipoTarea;

    fn campos(pares: &[(&str, &str)]) -> Campos {
        pares
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn edicion(id: &str, nuevos: &[(&str, &str)], anteriores: &[(&str, &str)]) -> EdicionPendiente {
        EdicionPendiente {
            id: id.into(),
            tarea_id: "T-101".into(),
            campos_nuevos: campos(nuevos),
            campos_anteriores: campos(anteriores),
            motivo: None,
            creado_por: "zoe@virustf.dev".into(),
            fecha: "2026-11-09".into(),
        }
    }

    fn db_con_tarea() -> Db {
        let db = Db::en_memoria().unwrap();
        tareas::insertar(
            db.conexion(),
            &tareas::Tarea::nueva(
                "T-101",
                "Original",
                TipoTarea::Audio,
                "ana@virustf.dev",
                "2026-11-09",
            ),
        )
        .unwrap();
        db
    }

    #[test]
    fn la_tarea_queda_marcada_con_edicion_pendiente() {
        let db = db_con_tarea();
        let c = db.conexion();
        assert!(!tareas::obtener(c, "T-101").unwrap().edicion_pendiente);
        guardar_o_sumar(
            c,
            &edicion("E-1", &[("titulo", "Nuevo")], &[("titulo", "Original")]),
        )
        .unwrap();
        assert!(tareas::obtener(c, "T-101").unwrap().edicion_pendiente);
        assert_eq!(listar(c).unwrap().len(), 1);
        let quitada = quitar(c, "T-101").unwrap().unwrap();
        assert_eq!(quitada.campos_anteriores, campos(&[("titulo", "Original")]));
        assert!(!tareas::obtener(c, "T-101").unwrap().edicion_pendiente);
        assert_eq!(quitar(c, "T-101").unwrap(), None);
    }

    #[test]
    fn una_segunda_edicion_se_suma_y_conserva_lo_original() {
        let db = db_con_tarea();
        let c = db.conexion();
        guardar_o_sumar(
            c,
            &edicion("E-1", &[("titulo", "Nuevo")], &[("titulo", "Original")]),
        )
        .unwrap();
        let sumada = guardar_o_sumar(
            c,
            &edicion(
                "E-2",
                &[("titulo", "Más nuevo"), ("detalles", "Con detalle")],
                &[("titulo", "Nuevo"), ("detalles", "")],
            ),
        )
        .unwrap();
        assert_eq!(sumada.id, "E-1");
        assert_eq!(
            sumada.campos_nuevos,
            campos(&[("titulo", "Más nuevo"), ("detalles", "Con detalle")])
        );
        assert_eq!(
            sumada.campos_anteriores,
            campos(&[("titulo", "Original"), ("detalles", "")])
        );
        assert_eq!(listar(c).unwrap(), vec![sumada]);
    }

    #[test]
    fn borrar_la_tarea_borra_su_edicion() {
        let db = db_con_tarea();
        let c = db.conexion();
        guardar_o_sumar(c, &edicion("E-1", &[("titulo", "N")], &[("titulo", "O")])).unwrap();
        tareas::borrar_fila(c, "T-101").unwrap();
        assert!(listar(c).unwrap().is_empty());
    }
}
