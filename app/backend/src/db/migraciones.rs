//! Migraciones del esquema. Cada entrada de `MIGRACIONES` lleva el `.db` una
//! versión más arriba; la versión actual se guarda en `PRAGMA user_version`.
//!
//! Reglas: **nunca** editar una migración ya publicada; los cambios van en una
//! entrada nueva al final.

use rusqlite::Connection;

/// Una migración por versión, en orden. La posición `i` lleva de la versión `i` a la `i + 1`.
const MIGRACIONES: &[&str] = &[
    // 1 — datos internos de la base (clave/valor).
    "CREATE TABLE meta (
        clave TEXT PRIMARY KEY,
        valor TEXT NOT NULL
    ) STRICT;",
];

/// Versión del esquema a la que queda un `.db` recién abierto.
pub const VERSION_ESQUEMA: u32 = MIGRACIONES.len() as u32;

pub fn version(conexion: &Connection) -> rusqlite::Result<u32> {
    conexion.pragma_query_value(None, "user_version", |f| f.get(0))
}

/// Aplica las migraciones que le falten al `.db`. Cada una va en su propia
/// transacción: si falla, el `.db` queda en la versión anterior, intacto.
pub fn aplicar(conexion: &mut Connection) -> crate::error::Resultado<()> {
    let actual = version(conexion)?;
    if actual > VERSION_ESQUEMA {
        return Err(crate::error::Error::BaseDatos(format!(
            "La base de datos es de una versión más nueva de BranchBoard (esquema {actual}, esta app maneja hasta {VERSION_ESQUEMA}). Actualiza la app."
        )));
    }
    for (i, sql) in MIGRACIONES.iter().enumerate().skip(actual as usize) {
        let tx = conexion.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as u32)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rechaza_un_db_de_una_version_mas_nueva() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "user_version", VERSION_ESQUEMA + 1)
            .unwrap();
        assert!(aplicar(&mut c).is_err());
    }

    #[test]
    fn aplicar_dos_veces_no_falla() {
        let mut c = Connection::open_in_memory().unwrap();
        aplicar(&mut c).unwrap();
        aplicar(&mut c).unwrap();
        assert_eq!(version(&c).unwrap(), VERSION_ESQUEMA);
    }
}
