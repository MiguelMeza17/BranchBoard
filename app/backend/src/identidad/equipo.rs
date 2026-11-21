//! El equipo de un proyecto en la pantalla Usuarios (issues #90 y #241). Ver "Comandos de usuarios y
//! ajustes" y "Usuarios y roles por el servidor (#241)" en BACKEND.md.
//!
//! Los roles los cambian **solo las funciones del servidor** (`equipo_servidor`, #241) y la tabla
//! `usuarios` es su copia. Hasta #243 los cambios de equipo de los proyectos por Drive/OneDrive
//! viajaban por `ops/` como eventos firmados por el Administrador o un Líder certificado. Sin firmas
//! nadie podría probar quién los mandó, así que esos eventos ya no se aceptan
//! (`eventos::TIPOS_RETIRADOS`): en un proyecto que todavía va por Drive el equipo se puede ver pero no
//! cambiar hasta pasarlo al servidor (#245).

use rusqlite::Connection;
use serde::Serialize;

use crate::db::usuarios;
use crate::error::Resultado;
use crate::modelo::Rol;

/// Tipos de evento de usuarios que viajaban por `ops/` (retirados en #243).
pub const TIPOS: &[&str] = &[
    "crear_usuario",
    "cambiar_rol",
    "confirmar_usuario",
    "eliminar_usuario",
];

/// Roles que un Líder puede asignar.
const ROLES_BASE: &[Rol] = &[Rol::Desarrollador, Rol::Tester, Rol::ColaboradorExterno];
/// Roles que el Administrador puede asignar (con el servidor también Administrador, #241).
const ROLES_ADMIN: &[Rol] = &[
    Rol::Administrador,
    Rol::Lider,
    Rol::Desarrollador,
    Rol::Tester,
    Rol::ColaboradorExterno,
];

/// Roles que `actor` puede darle a una persona con rol `actual` (`None` = alta nueva). Vacío = no
/// puede cambiarla. Igual que `rolesAsignablesA` del frontend y `privado.puede_asignar` del servidor.
pub fn roles_asignables(actor: Rol, actual: Option<Rol>) -> &'static [Rol] {
    match (actor, actual) {
        (Rol::Administrador, _) => ROLES_ADMIN,
        (Rol::Lider, None) => ROLES_BASE,
        (Rol::Lider, Some(rol)) if ROLES_BASE.contains(&rol) => ROLES_BASE,
        _ => &[],
    }
}

/// `true` si `actor` puede quitar del proyecto a alguien con rol `objetivo`. El servidor además no
/// deja quitar al último Administrador.
pub fn puede_eliminar(actor: Rol, objetivo: Rol) -> bool {
    match actor {
        Rol::Administrador => true,
        Rol::Lider => matches!(objetivo, Rol::Desarrollador | Rol::ColaboradorExterno),
        _ => false,
    }
}

/// Una fila de la pantalla Usuarios. Sin correo para quien no es Líder ni Administrador.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsuarioVisible {
    pub email: Option<String>,
    pub nombre: String,
    pub apellido: String,
    pub rol: Rol,
    pub fecha_alta: String,
    /// Proyecto en el servidor (#241): invitado por correo, todavía sin cuenta. Solo lo ven Líder y
    /// Administrador; no tiene acceso hasta que se registre.
    pub invitado: bool,
}

/// `listar_usuarios`: todos los roles; solo Líder y Administrador ven correos.
pub fn listar(c: &Connection, rol_de_quien_pide: Rol) -> Resultado<Vec<UsuarioVisible>> {
    let ve_correos = rol_de_quien_pide.es_lider_o_admin();
    Ok(usuarios::listar(c)?
        .into_iter()
        .map(|u| UsuarioVisible {
            email: ve_correos.then(|| u.email.clone()),
            nombre: u.nombre,
            apellido: u.apellido,
            rol: u.rol,
            fecha_alta: u.fecha_alta,
            invitado: false,
        })
        .collect())
}

/// Solo para pruebas: da de alta a `email` con `rol` en la base local, como lo dejaría la copia de
/// `miembros` del servidor.
#[cfg(test)]
pub fn alta_de_prueba(c: &Connection, email: &str, rol: Rol, hoy: &str) -> usuarios::Usuario {
    let email = crate::modelo::normalizar_email(email);
    let (nombre, apellido) = usuarios::nombre_desde_email(&email);
    let u = usuarios::Usuario {
        email,
        nombre,
        apellido,
        rol,
        fecha_alta: hoy.to_string(),
    };
    usuarios::insertar(c, &u).unwrap();
    u
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn roles_asignables_como_en_el_frontend_y_el_servidor() {
        use Rol::*;
        assert_eq!(roles_asignables(Administrador, None), ROLES_ADMIN);
        assert_eq!(
            roles_asignables(Administrador, Some(Administrador)),
            ROLES_ADMIN
        );
        assert_eq!(roles_asignables(Lider, None), ROLES_BASE);
        assert_eq!(roles_asignables(Lider, Some(Tester)), ROLES_BASE);
        assert!(roles_asignables(Lider, Some(Lider)).is_empty());
        assert!(roles_asignables(Desarrollador, None).is_empty());
        assert!(puede_eliminar(Administrador, Lider));
        assert!(puede_eliminar(Administrador, Administrador));
        assert!(puede_eliminar(Lider, ColaboradorExterno));
        assert!(!puede_eliminar(Lider, Tester));
        assert!(!puede_eliminar(Tester, Desarrollador));
    }

    #[test]
    fn solo_lider_y_administrador_ven_los_correos() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        alta_de_prueba(c, "Ana.Lopez@VirusTF.dev", Rol::Administrador, "2026-11-21");
        alta_de_prueba(c, "beto@virustf.dev", Rol::Tester, "2026-11-21");
        let lider = listar(c, Rol::Lider).unwrap();
        assert_eq!(lider.len(), 2);
        assert_eq!(lider[0].email.as_deref(), Some("ana.lopez@virustf.dev"));
        assert_eq!(
            (lider[0].nombre.as_str(), lider[0].apellido.as_str()),
            ("Ana", "Lopez")
        );
        let tester = listar(c, Rol::Tester).unwrap();
        assert!(tester.iter().all(|u| u.email.is_none() && !u.invitado));
        assert_eq!(tester[1].rol, Rol::Tester);
    }
}
