//! Tabla `usuarios`: quién está en el proyecto y con qué rol.
//! La firma del rol se verifica en `identidad` (issue #86); aquí solo se guarda y se lee.

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
    /// Firma Ed25519 de `email|rol|firmado_en`; `None` = pendiente de confirmar.
    pub firma: Option<String>,
    pub firmado_en: Option<String>,
    /// Quién firmó la fila (Administrador o un Líder certificado).
    pub firmado_por: Option<String>,
    /// Solo Líderes: su llave pública y el certificado que le dio el Administrador.
    pub llave_publica: Option<String>,
    pub certificado: Option<String>,
}

const COLUMNAS: &str = "email, nombre, apellido, rol, fecha_alta, firma, firmado_en, firmado_por, llave_publica, certificado";

fn de_fila(f: &Row<'_>) -> rusqlite::Result<Usuario> {
    Ok(Usuario {
        email: f.get(0)?,
        nombre: f.get(1)?,
        apellido: f.get(2)?,
        rol: f.get(3)?,
        fecha_alta: f.get(4)?,
        firma: f.get(5)?,
        firmado_en: f.get(6)?,
        firmado_por: f.get(7)?,
        llave_publica: f.get(8)?,
        certificado: f.get(9)?,
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
            "INSERT INTO usuarios ({COLUMNAS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            email,
            usuario.nombre,
            usuario.apellido,
            usuario.rol,
            usuario.fecha_alta,
            usuario.firma,
            usuario.firmado_en,
            usuario.firmado_por.as_deref().map(normalizar_email),
            usuario.llave_publica,
            usuario.certificado,
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

/// Cambia el rol y su firma (van juntos: la firma cubre `email|rol|firmado_en`).
pub fn actualizar_rol(
    c: &Connection,
    email: &str,
    rol: Rol,
    firma: Option<&str>,
    firmado_en: Option<&str>,
    firmado_por: Option<&str>,
) -> Resultado<()> {
    let cambiadas = c.execute(
        "UPDATE usuarios SET rol = ?2, firma = ?3, firmado_en = ?4, firmado_por = ?5 WHERE email = ?1",
        params![
            normalizar_email(email),
            rol,
            firma,
            firmado_en,
            firmado_por.map(normalizar_email)
        ],
    )?;
    exigir_una(cambiadas, email)
}

/// Guarda (o quita, con `None`) la llave pública de un Líder y su certificado.
pub fn actualizar_certificado(
    c: &Connection,
    email: &str,
    llave_publica: Option<&str>,
    certificado: Option<&str>,
) -> Resultado<()> {
    let cambiadas = c.execute(
        "UPDATE usuarios SET llave_publica = ?2, certificado = ?3 WHERE email = ?1",
        params![normalizar_email(email), llave_publica, certificado],
    )?;
    exigir_una(cambiadas, email)
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
            firma: None,
            firmado_en: None,
            firmado_por: None,
            llave_publica: None,
            certificado: None,
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
