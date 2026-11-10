//! Tabla `documentos`: archivos del proyecto (no versionados en Unity VC) que
//! viven en la carpeta compartida. Lo que sube un Líder/Administrador queda
//! publicado; lo demás queda `pendiente` hasta su visto bueno.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};

/// Una fila de `documentos`, con los campos de `Documento` en el frontend más
/// `ruta`, el nombre interno del archivo (no sale al frontend).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Documento {
    pub id: String,
    /// Nombre original del archivo, solo como dato.
    pub nombre: String,
    /// Bytes.
    pub tamano: u64,
    pub subido_por: String,
    pub fecha: String,
    pub pendiente: bool,
    /// Ruta relativa a la carpeta de documentos del proyecto; la genera el backend.
    #[serde(skip)]
    pub ruta: String,
}

const COLUMNAS: &str = "id, nombre, tamano, subido_por, fecha, pendiente, ruta";

fn de_fila(f: &Row<'_>) -> rusqlite::Result<Documento> {
    Ok(Documento {
        id: f.get(0)?,
        nombre: f.get(1)?,
        tamano: f.get(2)?,
        subido_por: f.get(3)?,
        fecha: f.get(4)?,
        pendiente: f.get(5)?,
        ruta: f.get(6)?,
    })
}

pub fn insertar(c: &Connection, d: &Documento) -> Resultado<()> {
    if d.nombre.trim().is_empty() || d.ruta.trim().is_empty() {
        return Err(Error::Validacion("El documento no tiene nombre".into()));
    }
    if buscar(c, &d.id)?.is_some() {
        return Err(Error::Validacion(format!("Ya existe {}", d.id)));
    }
    c.execute(
        &format!("INSERT INTO documentos ({COLUMNAS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
        params![
            d.id,
            d.nombre,
            d.tamano,
            d.subido_por,
            d.fecha,
            d.pendiente,
            d.ruta
        ],
    )?;
    Ok(())
}

pub fn buscar(c: &Connection, id: &str) -> Resultado<Option<Documento>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS} FROM documentos WHERE id = ?1"),
        [id],
        de_fila,
    )
    .optional()?)
}

pub fn obtener(c: &Connection, id: &str) -> Resultado<Documento> {
    buscar(c, id)?.ok_or_else(|| Error::NoEncontrado(format!("el documento {id}")))
}

/// Todos los documentos, en orden de subida.
pub fn listar(c: &Connection) -> Resultado<Vec<Documento>> {
    let mut consulta = c.prepare(&format!("SELECT {COLUMNAS} FROM documentos ORDER BY rowid"))?;
    let filas = consulta.query_map([], de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Los que esperan el visto bueno de un Líder/Administrador (apartado Pendientes).
pub fn pendientes(c: &Connection) -> Resultado<Vec<Documento>> {
    Ok(listar(c)?.into_iter().filter(|d| d.pendiente).collect())
}

/// Visto bueno a un documento pendiente.
pub fn aprobar(c: &Connection, id: &str) -> Resultado<()> {
    let documento = obtener(c, id)?;
    if !documento.pendiente {
        return Err(Error::Validacion(
            "Ese documento ya estaba publicado".into(),
        ));
    }
    c.execute("UPDATE documentos SET pendiente = 0 WHERE id = ?1", [id])?;
    Ok(())
}

/// Quita la fila. Devuelve el documento borrado para que el comando pueda
/// borrar también su archivo.
pub fn borrar(c: &Connection, id: &str) -> Resultado<Documento> {
    let documento = obtener(c, id)?;
    c.execute("DELETE FROM documentos WHERE id = ?1", [id])?;
    Ok(documento)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn documento(id: &str, pendiente: bool) -> Documento {
        Documento {
            id: id.into(),
            nombre: "GDD.pdf".into(),
            tamano: 2048,
            subido_por: "zoe@virustf.dev".into(),
            fecha: "2026-11-09".into(),
            pendiente,
            ruta: format!("{id}.pdf"),
        }
    }

    #[test]
    fn subir_aprobar_y_borrar() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &documento("D-1", true)).unwrap();
        insertar(c, &documento("D-2", false)).unwrap();
        assert!(insertar(c, &documento("D-1", true)).is_err());
        assert_eq!(listar(c).unwrap().len(), 2);
        assert_eq!(pendientes(c).unwrap(), vec![documento("D-1", true)]);

        aprobar(c, "D-1").unwrap();
        assert!(pendientes(c).unwrap().is_empty());
        assert!(aprobar(c, "D-1").is_err());
        assert_eq!(aprobar(c, "D-9").unwrap_err().codigo(), "no_encontrado");

        let borrado = borrar(c, "D-2").unwrap();
        assert_eq!(borrado.ruta, "D-2.pdf");
        assert_eq!(listar(c).unwrap().len(), 1);
        assert!(borrar(c, "D-2").is_err());
    }

    #[test]
    fn la_ruta_interna_no_sale_al_frontend() {
        let json = serde_json::to_value(documento("D-1", true)).unwrap();
        assert_eq!(json["nombre"], "GDD.pdf");
        assert!(json.get("ruta").is_none());
    }

    #[test]
    fn dos_documentos_no_comparten_archivo() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &documento("D-1", true)).unwrap();
        let mut otro = documento("D-2", true);
        otro.ruta = "D-1.pdf".into();
        assert!(insertar(c, &otro).is_err());
    }
}
