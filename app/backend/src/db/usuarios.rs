//! Tabla `usuarios`: quién está en el proyecto y con qué rol. Con el proyecto en el servidor es la
//! copia de `miembros` (#241); desde #243 no lleva firmas.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};
use crate::modelo::{normalizar_email, Rol};

/// Una fila de `usuarios`, con los mismos campos que `Usuario` en el frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usuario {
    pub email: String,
    pub nombre: String,
    pub apellido: String,
    pub rol: Rol,
    /// `AAAA-MM-DD`: cuándo quedó registrado en el proyecto.
    pub fecha_alta: String,
}

const COLUMNAS: &str = "email, nombre, apellido, rol, fecha_alta";

fn de_fila(f: &Row<'_>) -> rusqlite::Result<Usuario> {
    Ok(Usuario {
        email: f.get(0)?,
        nombre: f.get(1)?,
        apellido: f.get(2)?,
        rol: f.get(3)?,
        fecha_alta: f.get(4)?,
    })
}

/// Nombre y apellido a partir del correo: `ana.lopez@…` → ("Ana", "Lopez").
pub fn nombre_desde_email(email: &str) -> (String, String) {
    let usuario = email.split('@').next().unwrap_or_default();
    let mut partes = usuario
        .split(['.', '_', '-'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut letras = p.chars();
            match letras.next() {
                Some(primera) => primera.to_uppercase().collect::<String>() + letras.as_str(),
                None => String::new(),
            }
        });
    let nombre = partes.next().unwrap_or_default();
    let apellido = partes.collect::<Vec<_>>().join(" ");
    (nombre, apellido)
}

/// Inserta un usuario nuevo. Falla si el correo ya existe (sin distinguir mayúsculas).
pub fn insertar(c: &Connection, usuario: &Usuario) -> Resultado<()> {
    let email = normalizar_email(&usuario.email);
    if email.is_empty() || !email.contains('@') {
        return Err(Error::Validacion("El correo no es válido".into()));
    }
    if buscar(c, &email)?.is_some() {
        return Err(Error::Validacion(format!(
            "Ya existe un usuario con el correo {email}"
        )));
    }
    c.execute(
        &format!(
            "INSERT INTO usuarios ({COLUMNAS}) VALUES (?1, ?2, ?3, ?4, ?5)"
        ),
        params![
            email,
            usuario.nombre,
            usuario.apellido,
            usuario.rol,
            usuario.fecha_alta,
        ],
    )?;
    Ok(())
}

pub fn buscar(c: &Connection, email: &str) -> Resultado<Option<Usuario>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS} FROM usuarios WHERE email = ?1"),
        [normalizar_email(email)],
        de_fila,
    )
    .optional()?)
}

/// Todos los usuarios, ordenados por nombre.
pub fn listar(c: &Connection) -> Resultado<Vec<Usuario>> {
    let mut consulta = c.prepare(&format!(
        "SELECT {COLUMNAS} FROM usuarios ORDER BY nombre COLLATE NOCASE, apellido COLLATE NOCASE, email"
    ))?;
    let filas = consulta.query_map([], de_fila)?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Cambia el rol.
pub fn actualizar_rol(c: &Connection, email: &str, rol: Rol) -> Resultado<()> {
    let cambiadas = c.execute(
        "UPDATE usuarios SET rol = ?2 WHERE email = ?1",
        params![normalizar_email(email), rol],
    )?;
    exigir_una(cambiadas, email)
}

/// Deja la tabla igual a `filas` (la copia de `miembros` del servidor, #241), todo o nada.
pub fn reemplazar_todos(c: &Connection, filas: &[Usuario]) -> Resultado<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("DELETE FROM usuarios", [])?;
    for u in filas {
        insertar(&tx, u)?;
    }
    tx.commit()?;
    Ok(())
}

/// Quita a la persona del proyecto.
pub fn eliminar(c: &Connection, email: &str) -> Resultado<()> {
    let cambiadas = c.execute(
        "DELETE FROM usuarios WHERE email = ?1",
        [normalizar_email(email)],
    )?;
    exigir_una(cambiadas, email)
}

fn exigir_una(cambiadas: usize, email: &str) -> Resultado<()> {
    if cambiadas == 0 {
        Err(Error::NoEncontrado(format!("el usuario {email}")))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn usuario(email: &str, rol: Rol) -> Usuario {
        let (nombre, apellido) = nombre_desde_email(email);
        Usuario {
            email: email.into(),
            nombre,
            apellido,
            rol,
            fecha_alta: "2026-11-09".into(),
        }
    }

    #[test]
    fn insertar_y_leer() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &usuario("Ana.Lopez@virustf.dev", Rol::Desarrollador)).unwrap();
        let leido = buscar(c, "ANA.LOPEZ@VIRUSTF.DEV").unwrap().unwrap();
        assert_eq!(leido.email, "ana.lopez@virustf.dev");
        assert_eq!(
            (leido.nombre.as_str(), leido.apellido.as_str()),
            ("Ana", "Lopez")
        );
        assert_eq!(leido.rol, Rol::Desarrollador);
        assert_eq!(leido.firma, None);
    }

    #[test]
    fn correo_repetido_se_rechaza_sin_distinguir_mayusculas() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &usuario("ana@virustf.dev", Rol::Tester)).unwrap();
        let error = insertar(c, &usuario("Ana@VirusTF.dev", Rol::Tester)).unwrap_err();
        assert_eq!(error.codigo(), "validacion");
        assert!(insertar(c, &usuario("sin-arroba", Rol::Tester)).is_err());
    }

    #[test]
    fn cambiar_rol_firma_y_eliminar() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        insertar(c, &usuario("zoe@virustf.dev", Rol::Desarrollador)).unwrap();
        insertar(c, &usuario("ana@virustf.dev", Rol::Lider)).unwrap();
        actualizar_rol(
            c,
            "zoe@virustf.dev",
            Rol::Tester,
            Some("firma"),
            Some("2026-11-09T10:00:00Z"),
            Some("Ana@virustf.dev"),
        )
        .unwrap();
        actualizar_certificado(c, "ana@virustf.dev", Some("llave"), Some("cert")).unwrap();

        let todos = listar(c).unwrap();
        assert_eq!(
            todos.iter().map(|u| u.nombre.as_str()).collect::<Vec<_>>(),
            ["Ana", "Zoe"]
        );
        assert_eq!(todos[1].rol, Rol::Tester);
        assert_eq!(todos[1].firmado_por.as_deref(), Some("ana@virustf.dev"));
        assert_eq!(todos[0].certificado.as_deref(), Some("cert"));

        eliminar(c, "zoe@virustf.dev").unwrap();
        assert_eq!(listar(c).unwrap().len(), 1);
        assert_eq!(
            eliminar(c, "zoe@virustf.dev").unwrap_err().codigo(),
            "no_encontrado"
        );
    }

    #[test]
    fn un_rol_desconocido_no_entra() {
        let db = Db::en_memoria().unwrap();
        let resultado = db.conexion().execute(
            "INSERT INTO usuarios (email, nombre, apellido, rol, fecha_alta) VALUES ('x@y.z', 'X', '', 'Jefe', '2026-11-09')",
            [],
        );
        assert!(resultado.is_err());
    }

    #[test]
    fn nombre_desde_el_correo() {
        assert_eq!(
            nombre_desde_email("ana.lopez@x.dev"),
            ("Ana".into(), "Lopez".into())
        );
        assert_eq!(
            nombre_desde_email("sebas@x.dev"),
            ("Sebas".into(), String::new())
        );
        assert_eq!(
            nombre_desde_email("juan_de-la.cruz@x.dev"),
            ("Juan".into(), "De La Cruz".into())
        );
    }
}
