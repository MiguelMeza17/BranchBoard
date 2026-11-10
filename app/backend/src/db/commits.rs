//! Tablas `commits` y `cambios_prestados`.
//!
//! Los check-ins viven en Unity VC: `commits` es la **copia de lo que se lee con
//! `cm log`** para mostrarlos rápido, y se puede reconstruir siempre. Un
//! préstamo (`cambios_prestados`) es lo que una tarea trajo de otra con "Traer
//! cambios"; se quita antes de mandar la tarea a revisión y queda como historial.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};
use crate::modelo::TipoTarea;

/// Un check-in de la rama de una tarea, igual que `Commit` en el frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,
    pub tarea_id: String,
    /// Changeset de Unity VC.
    pub hash: String,
    /// Primera línea del comentario.
    pub mensaje: String,
    /// Resto del comentario.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descripcion: Option<String>,
    /// Elegido al hacer check-in desde BranchBoard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo: Option<TipoTarea>,
    pub autor: String,
    pub fecha: String,
}

/// Una fila de `cambios_prestados`, igual que `CambioTraido` en el frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CambioPrestado {
    pub id: String,
    /// Tarea que recibió los cambios.
    pub tarea_id: String,
    /// Tarea de donde salieron.
    pub origen_id: String,
    /// Commit puntual; `None` = todos los cambios de la tarea origen.
    pub hash: Option<String>,
    pub autor: String,
    pub fecha: String,
    /// Archivos que llegaron con el préstamo.
    pub archivos: Vec<String>,
    /// Archivos prestados que se modificaron después en esta rama.
    pub archivos_tocados: Vec<String>,
    /// `None` = los cambios siguen en la rama.
    pub quitado_en: Option<String>,
    pub quitado_por: Option<String>,
}

const COLUMNAS_COMMIT: &str = "id, tarea_id, hash, mensaje, descripcion, tipo, autor, fecha";

fn commit_de_fila(f: &Row<'_>) -> rusqlite::Result<Commit> {
    Ok(Commit {
        id: f.get(0)?,
        tarea_id: f.get(1)?,
        hash: f.get(2)?,
        mensaje: f.get(3)?,
        descripcion: f.get(4)?,
        tipo: f.get(5)?,
        autor: f.get(6)?,
        fecha: f.get(7)?,
    })
}

/// Guarda un check-in. Si ya había uno con el mismo changeset en la tarea, lo
/// reemplaza (la copia siempre refleja lo último leído de Unity VC).
pub fn guardar(c: &Connection, commit: &Commit) -> Resultado<()> {
    if commit.hash.trim().is_empty() {
        return Err(Error::Validacion("El check-in no tiene changeset".into()));
    }
    c.execute(
        &format!(
            "INSERT INTO commits ({COLUMNAS_COMMIT}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (tarea_id, hash) DO UPDATE SET
               id = excluded.id, mensaje = excluded.mensaje, descripcion = excluded.descripcion,
               tipo = excluded.tipo, autor = excluded.autor, fecha = excluded.fecha"
        ),
        params![
            commit.id,
            commit.tarea_id,
            commit.hash,
            commit.mensaje,
            commit.descripcion,
            commit.tipo,
            commit.autor,
            commit.fecha
        ],
    )?;
    Ok(())
}

/// Check-ins de una tarea, del más antiguo al más reciente.
pub fn de_tarea(c: &Connection, tarea_id: &str) -> Resultado<Vec<Commit>> {
    let mut consulta = c.prepare(&format!(
        "SELECT {COLUMNAS_COMMIT} FROM commits WHERE tarea_id = ?1 ORDER BY rowid"
    ))?;
    let filas = consulta.query_map([tarea_id], commit_de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

pub fn buscar(c: &Connection, id: &str) -> Resultado<Option<Commit>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS_COMMIT} FROM commits WHERE id = ?1"),
        [id],
        commit_de_fila,
    )
    .optional()?)
}

/// Borra la copia de los check-ins de una tarea (para volver a leerla de `cm log`).
pub fn borrar_de_tarea(c: &Connection, tarea_id: &str) -> Resultado<usize> {
    Ok(c.execute("DELETE FROM commits WHERE tarea_id = ?1", [tarea_id])?)
}

const COLUMNAS_PRESTAMO: &str =
    "id, tarea_id, origen_id, hash, autor, fecha, archivos, archivos_tocados, quitado_en, quitado_por";

fn lista_de_json(f: &Row<'_>, i: usize) -> rusqlite::Result<Vec<String>> {
    let texto: String = f.get(i)?;
    serde_json::from_str(&texto).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn lista_a_json(lista: &[String]) -> Resultado<String> {
    serde_json::to_string(lista).map_err(|e| Error::Interno(e.to_string()))
}

fn prestamo_de_fila(f: &Row<'_>) -> rusqlite::Result<CambioPrestado> {
    Ok(CambioPrestado {
        id: f.get(0)?,
        tarea_id: f.get(1)?,
        origen_id: f.get(2)?,
        hash: f.get(3)?,
        autor: f.get(4)?,
        fecha: f.get(5)?,
        archivos: lista_de_json(f, 6)?,
        archivos_tocados: lista_de_json(f, 7)?,
        quitado_en: f.get(8)?,
        quitado_por: f.get(9)?,
    })
}

pub fn insertar_prestamo(c: &Connection, prestamo: &CambioPrestado) -> Resultado<()> {
    if prestamo.tarea_id == prestamo.origen_id {
        return Err(Error::Validacion(
            "Una tarea no puede traer cambios de sí misma".into(),
        ));
    }
    if buscar_prestamo(c, &prestamo.id)?.is_some() {
        return Err(Error::Validacion(format!("Ya existe {}", prestamo.id)));
    }
    c.execute(
        &format!(
            "INSERT INTO cambios_prestados ({COLUMNAS_PRESTAMO}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            prestamo.id,
            prestamo.tarea_id,
            prestamo.origen_id,
            prestamo.hash,
            prestamo.autor,
            prestamo.fecha,
            lista_a_json(&prestamo.archivos)?,
            lista_a_json(&prestamo.archivos_tocados)?,
            prestamo.quitado_en,
            prestamo.quitado_por
        ],
    )?;
    Ok(())
}

pub fn buscar_prestamo(c: &Connection, id: &str) -> Resultado<Option<CambioPrestado>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS_PRESTAMO} FROM cambios_prestados WHERE id = ?1"),
        [id],
        prestamo_de_fila,
    )
    .optional()?)
}

pub fn obtener_prestamo(c: &Connection, id: &str) -> Resultado<CambioPrestado> {
    buscar_prestamo(c, id)?.ok_or_else(|| Error::NoEncontrado(format!("el préstamo {id}")))
}

/// Todos los préstamos que recibió una tarea (vigentes y ya devueltos), en orden.
pub fn prestamos_de_tarea(c: &Connection, tarea_id: &str) -> Resultado<Vec<CambioPrestado>> {
    let mut consulta = c.prepare(&format!(
        "SELECT {COLUMNAS_PRESTAMO} FROM cambios_prestados WHERE tarea_id = ?1 ORDER BY rowid"
    ))?;
    let filas = consulta.query_map([tarea_id], prestamo_de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Préstamos que siguen en la rama de la tarea.
pub fn prestamos_vigentes(c: &Connection, tarea_id: &str) -> Resultado<Vec<CambioPrestado>> {
    Ok(prestamos_de_tarea(c, tarea_id)?
        .into_iter()
        .filter(|p| p.quitado_en.is_none())
        .collect())
}

/// Guarda qué archivos prestados se tocaron después en la rama.
pub fn marcar_tocados(c: &Connection, id: &str, archivos_tocados: &[String]) -> Resultado<()> {
    let cambiadas = c.execute(
        "UPDATE cambios_prestados SET archivos_tocados = ?2 WHERE id = ?1",
        params![id, lista_a_json(archivos_tocados)?],
    )?;
    if cambiadas == 0 {
        return Err(Error::NoEncontrado(format!("el préstamo {id}")));
    }
    Ok(())
}

/// Marca el préstamo como devuelto (sus cambios ya no están en la rama).
pub fn quitar_prestamo(c: &Connection, id: &str, quitado_por: &str, fecha: &str) -> Resultado<()> {
    let prestamo = obtener_prestamo(c, id)?;
    if prestamo.quitado_en.is_some() {
        return Err(Error::Validacion(
            "Esos cambios ya se habían devuelto".into(),
        ));
    }
    c.execute(
        "UPDATE cambios_prestados SET quitado_en = ?2, quitado_por = ?3 WHERE id = ?1",
        params![id, fecha, quitado_por],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{tareas, Db};

    fn db_con_tareas() -> Db {
        let db = Db::en_memoria().unwrap();
        for id in ["T-101", "T-102"] {
            tareas::insertar(
                db.conexion(),
                &tareas::Tarea::nueva(
                    id,
                    "Tarea",
                    TipoTarea::Scripting,
                    "ana@virustf.dev",
                    "2026-11-09",
                ),
            )
            .unwrap();
        }
        db
    }

    fn commit(id: &str, hash: &str) -> Commit {
        Commit {
            id: id.into(),
            tarea_id: "T-101".into(),
            hash: hash.into(),
            mensaje: "Menú principal".into(),
            descripcion: None,
            tipo: Some(TipoTarea::Scripting),
            autor: "ana@virustf.dev".into(),
            fecha: "2026-11-09T10:00:00Z".into(),
        }
    }

    fn prestamo(id: &str) -> CambioPrestado {
        CambioPrestado {
            id: id.into(),
            tarea_id: "T-101".into(),
            origen_id: "T-102".into(),
            hash: None,
            autor: "ana@virustf.dev".into(),
            fecha: "2026-11-10T09:00:00Z".into(),
            archivos: vec!["Assets/Player.cs".into(), "Assets/Enemy.cs".into()],
            archivos_tocados: vec![],
            quitado_en: None,
            quitado_por: None,
        }
    }

    #[test]
    fn guardar_commits_y_releer_reemplaza_el_mismo_changeset() {
        let db = db_con_tareas();
        let c = db.conexion();
        guardar(c, &commit("C-1", "41")).unwrap();
        guardar(c, &commit("C-2", "42")).unwrap();
        let mut corregido = commit("C-1", "41");
        corregido.mensaje = "Menú principal (fix)".into();
        corregido.descripcion = Some("Detalle".into());
        guardar(c, &corregido).unwrap();

        let lista = de_tarea(c, "T-101").unwrap();
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0], corregido);
        assert_eq!(buscar(c, "C-2").unwrap(), Some(commit("C-2", "42")));
        assert!(guardar(c, &commit("C-3", " ")).is_err());
        assert!(de_tarea(c, "T-102").unwrap().is_empty());
    }

    #[test]
    fn el_commit_viaja_sin_campos_opcionales_vacios() {
        let json = serde_json::to_value(commit("C-1", "41")).unwrap();
        assert_eq!(json["tipo"], "Scripting");
        assert!(json.get("descripcion").is_none());
    }

    #[test]
    fn borrar_la_tarea_o_la_copia_borra_sus_commits() {
        let db = db_con_tareas();
        let c = db.conexion();
        guardar(c, &commit("C-1", "41")).unwrap();
        assert_eq!(borrar_de_tarea(c, "T-101").unwrap(), 1);
        guardar(c, &commit("C-1", "41")).unwrap();
        tareas::borrar_fila(c, "T-101").unwrap();
        assert!(buscar(c, "C-1").unwrap().is_none());
    }

    #[test]
    fn prestamos_vigentes_tocados_y_devueltos() {
        let db = db_con_tareas();
        let c = db.conexion();
        insertar_prestamo(c, &prestamo("P-1")).unwrap();
        assert!(insertar_prestamo(c, &prestamo("P-1")).is_err());
        let mut propio = prestamo("P-2");
        propio.origen_id = "T-101".into();
        assert!(insertar_prestamo(c, &propio).is_err());

        assert_eq!(obtener_prestamo(c, "P-1").unwrap(), prestamo("P-1"));
        marcar_tocados(c, "P-1", &["Assets/Player.cs".to_string()]).unwrap();
        assert_eq!(
            obtener_prestamo(c, "P-1").unwrap().archivos_tocados,
            vec!["Assets/Player.cs".to_string()]
        );
        assert_eq!(prestamos_vigentes(c, "T-101").unwrap().len(), 1);

        quitar_prestamo(c, "P-1", "ana@virustf.dev", "2026-11-11T08:00:00Z").unwrap();
        let devuelto = obtener_prestamo(c, "P-1").unwrap();
        assert_eq!(devuelto.quitado_por.as_deref(), Some("ana@virustf.dev"));
        assert!(prestamos_vigentes(c, "T-101").unwrap().is_empty());
        assert_eq!(prestamos_de_tarea(c, "T-101").unwrap().len(), 1);
        assert!(quitar_prestamo(c, "P-1", "ana@virustf.dev", "2026-11-12").is_err());
        assert_eq!(
            marcar_tocados(c, "P-9", &[]).unwrap_err().codigo(),
            "no_encontrado"
        );
    }
}
