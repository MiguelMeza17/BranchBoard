//! Base de datos SQLite local (una por proyecto y por máquina).
//!
//! El `.db` vive en la carpeta de datos local (`%LocalAppData%\BranchBoard\`),
//! nunca en Drive/OneDrive: es desechable y se reconstruye desde `ops/`
//! (ver "Sincronización entre máquinas" en BACKEND.md).

pub mod configuracion;
pub mod ediciones;
mod migraciones;
pub mod tareas;
pub mod usuarios;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{Error, Resultado};

pub use migraciones::VERSION_ESQUEMA;

/// Nombre del archivo de base de datos dentro de la carpeta de cada proyecto.
pub const NOMBRE_DB: &str = "branchboard.db";

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::BaseDatos(e.to_string())
    }
}

/// Carpeta local de datos de un proyecto: `<datos>/proyectos/<id>/`.
/// El `id` se limpia para que nunca pueda salirse de la carpeta de datos.
pub fn carpeta_proyecto(carpeta_datos: &Path, id_proyecto: &str) -> Resultado<PathBuf> {
    let limpio: String = id_proyecto
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if limpio.is_empty() || limpio.chars().all(|c| c == '_') {
        return Err(Error::Validacion(
            "El proyecto no tiene un identificador válido".into(),
        ));
    }
    Ok(carpeta_datos.join("proyectos").join(limpio))
}

/// Conexión a la base de datos de un proyecto, ya migrada a la última versión.
#[derive(Debug)]
pub struct Db {
    conexion: Connection,
}

impl Db {
    /// Abre (o crea) el `.db` de un proyecto y aplica las migraciones que falten.
    pub fn abrir(carpeta_proyecto: &Path) -> Resultado<Self> {
        std::fs::create_dir_all(carpeta_proyecto)?;
        Self::preparar(Connection::open(carpeta_proyecto.join(NOMBRE_DB))?)
    }

    /// Base de datos en memoria, para pruebas.
    pub fn en_memoria() -> Resultado<Self> {
        Self::preparar(Connection::open_in_memory()?)
    }

    fn preparar(mut conexion: Connection) -> Resultado<Self> {
        // WAL: lecturas y escrituras no se bloquean entre sí. Llaves foráneas activas.
        conexion.pragma_update(None, "journal_mode", "WAL")?;
        conexion.pragma_update(None, "foreign_keys", "ON")?;
        migraciones::aplicar(&mut conexion)?;
        Ok(Self { conexion })
    }

    pub fn conexion(&self) -> &Connection {
        &self.conexion
    }

    pub fn conexion_mut(&mut self) -> &mut Connection {
        &mut self.conexion
    }

    /// Versión del esquema que tiene este `.db`.
    pub fn version(&self) -> Resultado<u32> {
        Ok(migraciones::version(&self.conexion)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crea_el_db_y_queda_en_la_ultima_version() {
        let dir = tempfile::tempdir().unwrap();
        let carpeta = carpeta_proyecto(dir.path(), "VirusTF").unwrap();
        let db = Db::abrir(&carpeta).unwrap();
        assert_eq!(db.version().unwrap(), VERSION_ESQUEMA);
        assert!(carpeta.join(NOMBRE_DB).exists());
    }

    #[test]
    fn borrar_el_db_y_volver_a_abrir_lo_crea_de_cero() {
        let dir = tempfile::tempdir().unwrap();
        let carpeta = carpeta_proyecto(dir.path(), "VirusTF").unwrap();
        {
            let db = Db::abrir(&carpeta).unwrap();
            db.conexion()
                .execute("INSERT INTO meta (clave, valor) VALUES ('prueba', '1')", [])
                .unwrap();
        }
        for f in std::fs::read_dir(&carpeta).unwrap() {
            std::fs::remove_file(f.unwrap().path()).unwrap();
        }
        let db = Db::abrir(&carpeta).unwrap();
        assert_eq!(db.version().unwrap(), VERSION_ESQUEMA);
        let filas: i64 = db
            .conexion()
            .query_row("SELECT COUNT(*) FROM meta", [], |f| f.get(0))
            .unwrap();
        assert_eq!(filas, 0);
    }

    #[test]
    fn reabrir_no_repite_migraciones_ni_pierde_datos() {
        let dir = tempfile::tempdir().unwrap();
        let carpeta = carpeta_proyecto(dir.path(), "VirusTF").unwrap();
        {
            let db = Db::abrir(&carpeta).unwrap();
            db.conexion()
                .execute("INSERT INTO meta (clave, valor) VALUES ('prueba', '1')", [])
                .unwrap();
        }
        let db = Db::abrir(&carpeta).unwrap();
        let valor: String = db
            .conexion()
            .query_row("SELECT valor FROM meta WHERE clave = 'prueba'", [], |f| {
                f.get(0)
            })
            .unwrap();
        assert_eq!(valor, "1");
    }

    #[test]
    fn el_id_del_proyecto_no_puede_salirse_de_la_carpeta() {
        let base = Path::new("datos");
        let carpeta = carpeta_proyecto(base, "../../otra").unwrap();
        assert!(carpeta.starts_with(base.join("proyectos")));
        assert_eq!(carpeta.components().count(), 3);
        assert!(carpeta_proyecto(base, "  ").is_err());
        assert!(carpeta_proyecto(base, "../..").is_err());
    }
}
