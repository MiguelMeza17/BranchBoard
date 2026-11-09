//! Tabla `configuracion_proyecto`: ajustes del proyecto entero, clave/valor.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::Resultado;
use crate::modelo::normalizar_email;

/// Nombre del proyecto dentro de BranchBoard (no el del repositorio de Unity VC).
pub const CLAVE_NOMBRE_PROYECTO: &str = "nombre_proyecto";
/// Ruta de la imagen del proyecto dentro de la carpeta compartida.
pub const CLAVE_ICONO_PROYECTO: &str = "icono_proyecto";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ajuste {
    pub clave: String,
    pub valor: String,
    pub actualizado_por: String,
    pub fecha_actualizacion: String,
}

/// Crea el ajuste o reemplaza su valor.
pub fn guardar(c: &Connection, clave: &str, valor: &str, por: &str, fecha: &str) -> Resultado<()> {
    c.execute(
        "INSERT INTO configuracion_proyecto (clave, valor, actualizado_por, fecha_actualizacion)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (clave) DO UPDATE SET
            valor = excluded.valor,
            actualizado_por = excluded.actualizado_por,
            fecha_actualizacion = excluded.fecha_actualizacion",
        params![clave, valor, normalizar_email(por), fecha],
    )?;
    Ok(())
}

pub fn leer(c: &Connection, clave: &str) -> Resultado<Option<String>> {
    Ok(c.query_row(
        "SELECT valor FROM configuracion_proyecto WHERE clave = ?1",
        [clave],
        |f| f.get(0),
    )
    .optional()?)
}

pub fn listar(c: &Connection) -> Resultado<Vec<Ajuste>> {
    let mut consulta = c.prepare(
        "SELECT clave, valor, actualizado_por, fecha_actualizacion FROM configuracion_proyecto ORDER BY clave",
    )?;
    let filas = consulta.query_map([], |f| {
        Ok(Ajuste {
            clave: f.get(0)?,
            valor: f.get(1)?,
            actualizado_por: f.get(2)?,
            fecha_actualizacion: f.get(3)?,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn guardar_reemplaza_el_valor() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        assert_eq!(leer(c, CLAVE_NOMBRE_PROYECTO).unwrap(), None);
        guardar(
            c,
            CLAVE_NOMBRE_PROYECTO,
            "VirusTF",
            "Admin@x.dev",
            "2026-11-09",
        )
        .unwrap();
        guardar(
            c,
            CLAVE_NOMBRE_PROYECTO,
            "Virus TF 2",
            "admin@x.dev",
            "2026-11-10",
        )
        .unwrap();
        assert_eq!(
            leer(c, CLAVE_NOMBRE_PROYECTO).unwrap().as_deref(),
            Some("Virus TF 2")
        );
        let todos = listar(c).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].actualizado_por, "admin@x.dev");
        assert_eq!(todos[0].fecha_actualizacion, "2026-11-10");
    }
}
