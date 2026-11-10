//! Tabla `notificaciones`: lo que ve la campana. La notificación viaja por `ops/`;
//! `leida` es local de cada máquina (quien la lee es su destinatario).

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};
use crate::modelo::TipoNotificacion;

/// Una fila de `notificaciones`, con los mismos campos que en el frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notificacion {
    pub id: String,
    pub tipo: TipoNotificacion,
    /// Email del destinatario.
    pub para: String,
    pub titulo: String,
    pub cuerpo: String,
    pub leida: bool,
    pub fecha: String,
    pub tarea_id: Option<String>,
    /// Ofrece un botón de acción directa (ej. aprobar).
    pub accionable: bool,
}

const COLUMNAS: &str = "id, tipo, para, titulo, cuerpo, leida, fecha, tarea_id, accionable";

fn de_fila(f: &Row<'_>) -> rusqlite::Result<Notificacion> {
    Ok(Notificacion {
        id: f.get(0)?,
        tipo: f.get(1)?,
        para: f.get(2)?,
        titulo: f.get(3)?,
        cuerpo: f.get(4)?,
        leida: f.get(5)?,
        fecha: f.get(6)?,
        tarea_id: f.get(7)?,
        accionable: f.get(8)?,
    })
}

pub fn insertar(c: &Connection, n: &Notificacion) -> Resultado<()> {
    if n.titulo.trim().is_empty() {
        return Err(Error::Validacion("La notificación no tiene título".into()));
    }
    if buscar(c, &n.id)?.is_some() {
        return Err(Error::Validacion(format!("Ya existe {}", n.id)));
    }
    c.execute(
        &format!(
            "INSERT INTO notificaciones ({COLUMNAS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        ),
        params![
            n.id,
            n.tipo,
            n.para,
            n.titulo,
            n.cuerpo,
            n.leida,
            n.fecha,
            n.tarea_id,
            n.accionable
        ],
    )?;
    Ok(())
}

pub fn buscar(c: &Connection, id: &str) -> Resultado<Option<Notificacion>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS} FROM notificaciones WHERE id = ?1"),
        [id],
        de_fila,
    )
    .optional()?)
}

/// Notificaciones de una persona, de la más reciente a la más antigua.
pub fn de_usuario(c: &Connection, para: &str) -> Resultado<Vec<Notificacion>> {
    let mut consulta = c.prepare(&format!(
        "SELECT {COLUMNAS} FROM notificaciones WHERE para = ?1 ORDER BY fecha DESC, rowid DESC"
    ))?;
    let filas = consulta.query_map([para], de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Cuántas tiene sin leer (el contador de la campana).
pub fn sin_leer(c: &Connection, para: &str) -> Resultado<u32> {
    Ok(c.query_row(
        "SELECT COUNT(*) FROM notificaciones WHERE para = ?1 AND leida = 0",
        [para],
        |f| f.get(0),
    )?)
}

pub fn marcar_leida(c: &Connection, id: &str, leida: bool) -> Resultado<()> {
    let cambiadas = c.execute(
        "UPDATE notificaciones SET leida = ?2 WHERE id = ?1",
        params![id, leida],
    )?;
    if cambiadas == 0 {
        return Err(Error::NoEncontrado(format!("la notificación {id}")));
    }
    Ok(())
}

/// Marca como leídas todas las de una persona. Devuelve cuántas cambió.
pub fn marcar_todas_leidas(c: &Connection, para: &str) -> Resultado<usize> {
    Ok(c.execute(
        "UPDATE notificaciones SET leida = 1 WHERE para = ?1 AND leida = 0",
        [para],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn notificacion(id: &str, para: &str, fecha: &str) -> Notificacion {
        Notificacion {
            id: id.into(),
            tipo: TipoNotificacion::TareaAsignada,
            para: para.into(),
            titulo: "Te asignaron T-101".into(),
            cuerpo: String::new(),
            leida: false,
            fecha: fecha.into(),
            tarea_id: Some("T-101".into()),
            accionable: false,
        }
    }

    #[test]
    fn lista_por_destinatario_de_la_mas_reciente() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(
            c,
            &notificacion("N-1", "ana@virustf.dev", "2026-11-09T10:00:00Z"),
        )
        .unwrap();
        insertar(
            c,
            &notificacion("N-2", "ana@virustf.dev", "2026-11-10T10:00:00Z"),
        )
        .unwrap();
        insertar(
            c,
            &notificacion("N-3", "zoe@virustf.dev", "2026-11-10T11:00:00Z"),
        )
        .unwrap();
        let de_ana = de_usuario(c, "ana@virustf.dev").unwrap();
        assert_eq!(
            de_ana.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["N-2", "N-1"]
        );
        assert_eq!(sin_leer(c, "ana@virustf.dev").unwrap(), 2);
        assert!(insertar(c, &notificacion("N-1", "ana@virustf.dev", "2026-11-09")).is_err());
    }

    #[test]
    fn marcar_y_desmarcar_leidas() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(
            c,
            &notificacion("N-1", "ana@virustf.dev", "2026-11-09T10:00:00Z"),
        )
        .unwrap();
        insertar(
            c,
            &notificacion("N-2", "ana@virustf.dev", "2026-11-10T10:00:00Z"),
        )
        .unwrap();
        marcar_leida(c, "N-1", true).unwrap();
        assert!(buscar(c, "N-1").unwrap().unwrap().leida);
        assert_eq!(sin_leer(c, "ana@virustf.dev").unwrap(), 1);
        marcar_leida(c, "N-1", false).unwrap();
        assert_eq!(marcar_todas_leidas(c, "ana@virustf.dev").unwrap(), 2);
        assert_eq!(sin_leer(c, "ana@virustf.dev").unwrap(), 0);
        assert_eq!(
            marcar_leida(c, "N-9", true).unwrap_err().codigo(),
            "no_encontrado"
        );
    }

    #[test]
    fn el_tipo_viaja_como_en_el_frontend() {
        let json =
            serde_json::to_value(notificacion("N-1", "ana@virustf.dev", "2026-11-09")).unwrap();
        assert_eq!(json["tipo"], "tarea_asignada");
        assert_eq!(TipoNotificacion::TODOS.len(), 14);
        assert_eq!(TipoNotificacion::desde_texto("correo"), None);
    }
}
