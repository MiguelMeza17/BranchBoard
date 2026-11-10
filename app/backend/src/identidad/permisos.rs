//! Chequeo de rol en cada comando (issue #87). Ver "Roles y permisos" en
//! SEGURIDAD.md y la tabla "Contrato con el frontend" en BACKEND.md.
//!
//! El frontend esconde lo que un rol no puede hacer, pero eso no protege nada:
//! cualquiera puede llamar a un comando saltándose la interfaz. Por eso cada
//! comando que depende del rol empieza con `estado.exigir_permiso("comando")?`,
//! y los roles de cada comando están **todos en una sola tabla**, [`PERMISOS`].
//! Un comando que no está en la tabla se niega siempre.
//!
//! Lo que depende de la tarea concreta (ej. "solo el creador puede editar su
//! tarea pendiente") lo revisa cada comando después, con el usuario que devuelve
//! `exigir_permiso`.

use rusqlite::Connection;
use serde::Serialize;

use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::identidad::firmas::{verificar_usuarios, Verificacion};
use crate::modelo::{normalizar_email, Rol};

/// Quién puede usar un comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceso {
    /// No depende de un proyecto ni de un rol (ej. buscar actualizaciones).
    Libre,
    /// Solo estos roles (rol verificado por la firma, ver `firmas`).
    Roles(&'static [Rol]),
}

pub const TODOS: &[Rol] = Rol::TODOS;
pub const LIDER_O_ADMIN: &[Rol] = &[Rol::Administrador, Rol::Lider];
pub const SOLO_ADMIN: &[Rol] = &[Rol::Administrador];

/// Todos los comandos de Tauri y quién puede usarlos. Al agregar un comando a
/// `lib.rs` hay que agregarlo aquí (una prueba lo exige).
pub const PERMISOS: &[(&str, Acceso)] = &[
    ("ping", Acceso::Libre),
    ("hay_operacion_en_curso", Acceso::Libre),
    ("buscar_actualizacion", Acceso::Libre),
    ("instalar_actualizacion", Acceso::Libre),
    ("listar_backups", Acceso::Roles(SOLO_ADMIN)),
    ("restaurar_backup", Acceso::Roles(SOLO_ADMIN)),
    ("exportar_llave_firma", Acceso::Roles(SOLO_ADMIN)),
    // Usuarios (issue #90). El detalle de quién cambia a quién está en `identidad::equipo`.
    ("listar_usuarios", Acceso::Roles(TODOS)),
    ("crear_usuario", Acceso::Roles(LIDER_O_ADMIN)),
    ("cambiar_rol", Acceso::Roles(LIDER_O_ADMIN)),
    ("eliminar_usuario", Acceso::Roles(LIDER_O_ADMIN)),
    ("confirmar_usuario", Acceso::Roles(SOLO_ADMIN)),
];

/// Quién puede usar `comando`; `None` si no está en la tabla.
pub fn acceso(comando: &str) -> Option<Acceso> {
    PERMISOS
        .iter()
        .find(|(nombre, _)| *nombre == comando)
        .map(|(_, acceso)| *acceso)
}

/// La persona que está usando la app, con el rol que de verdad se le aplica.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsuarioActual {
    pub email: String,
    pub rol: Rol,
    /// `false` si su firma no coincide y se le aplica Desarrollador.
    pub rol_verificado: bool,
}

/// Busca a `email` (el de `cm whoami`) en la tabla `usuarios` y verifica su rol.
/// Sin fila, o con la fila pendiente de confirmar, no puede usar nada.
pub fn usuario_actual(
    c: &Connection,
    publica_admin: &str,
    email: &str,
) -> Resultado<UsuarioActual> {
    let email = normalizar_email(email);
    let verificado = verificar_usuarios(publica_admin, usuarios::listar(c)?)
        .into_iter()
        .find(|u| u.usuario.email == email)
        .ok_or(Error::SinPermiso)?;
    let rol = verificado.rol_efectivo.ok_or(Error::SinPermiso)?;
    Ok(UsuarioActual {
        email,
        rol,
        rol_verificado: verificado.verificacion == Verificacion::Verificado,
    })
}

/// `Ok` si `usuario` puede usar `comando` según [`PERMISOS`].
pub fn exigir(comando: &str, usuario: &UsuarioActual) -> Resultado<()> {
    match acceso(comando) {
        Some(Acceso::Libre) => Ok(()),
        Some(Acceso::Roles(roles)) if roles.contains(&usuario.rol) => Ok(()),
        _ => Err(Error::SinPermiso),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::usuarios::Usuario;
    use crate::db::Db;
    use crate::identidad::{firmas, llaves};

    const FECHA: &str = "2026-11-10T10:00:00Z";

    fn fila(email: &str, rol: Rol) -> Usuario {
        Usuario {
            email: email.into(),
            nombre: "N".into(),
            apellido: "A".into(),
            rol,
            fecha_alta: "2026-11-10".into(),
            firma: None,
            firmado_en: None,
            firmado_por: None,
            llave_publica: None,
            certificado: None,
        }
    }

    /// Base con un Administrador, un Tester, alguien con el rol cambiado a mano
    /// y alguien pendiente de confirmar.
    fn proyecto() -> (tempfile::TempDir, Db, String) {
        let dir = tempfile::tempdir().unwrap();
        let publica = llaves::generar_llaves_admin(dir.path(), None).unwrap();
        let admin = llaves::cargar_llave_admin(dir.path(), None).unwrap();
        let firmada = |email: &str, rol: Rol| Usuario {
            firma: Some(firmas::firmar_rol(&admin, email, rol, FECHA).unwrap()),
            firmado_en: Some(FECHA.into()),
            ..fila(email, rol)
        };
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        usuarios::insertar(c, &firmada("admin@virustf.dev", Rol::Administrador)).unwrap();
        usuarios::insertar(c, &firmada("tester@virustf.dev", Rol::Tester)).unwrap();
        usuarios::insertar(
            c,
            &Usuario {
                rol: Rol::Administrador,
                ..firmada("tramposo@virustf.dev", Rol::Tester)
            },
        )
        .unwrap();
        usuarios::insertar(c, &fila("nuevo@virustf.dev", Rol::Lider)).unwrap();
        (dir, db, publica)
    }

    #[test]
    fn el_rol_sale_de_la_firma() {
        let (_dir, db, publica) = proyecto();
        let c = db.conexion();
        let admin = usuario_actual(c, &publica, "Admin@VirusTF.dev").unwrap();
        assert_eq!(
            (admin.rol, admin.rol_verificado),
            (Rol::Administrador, true)
        );
        assert_eq!(admin.email, "admin@virustf.dev");

        // Se escribió Administrador a mano: se le aplica Desarrollador.
        let tramposo = usuario_actual(c, &publica, "tramposo@virustf.dev").unwrap();
        assert_eq!(
            (tramposo.rol, tramposo.rol_verificado),
            (Rol::Desarrollador, false)
        );

        // Pendiente de confirmar o fuera del proyecto: nada.
        for email in ["nuevo@virustf.dev", "nadie@virustf.dev"] {
            assert_eq!(
                usuario_actual(c, &publica, email).unwrap_err().codigo(),
                "sin_permiso"
            );
        }
    }

    #[test]
    fn exigir_aplica_la_tabla() {
        let (_dir, db, publica) = proyecto();
        let c = db.conexion();
        let admin = usuario_actual(c, &publica, "admin@virustf.dev").unwrap();
        let tester = usuario_actual(c, &publica, "tester@virustf.dev").unwrap();
        let tramposo = usuario_actual(c, &publica, "tramposo@virustf.dev").unwrap();

        assert!(exigir("restaurar_backup", &admin).is_ok());
        for u in [&tester, &tramposo] {
            let error = exigir("restaurar_backup", u).unwrap_err();
            assert_eq!(error.codigo(), "sin_permiso");
            assert!(exigir("ping", u).is_ok());
        }
        // Un comando que no está en la tabla se niega, también al Administrador.
        assert!(exigir("comando_inventado", &admin).is_err());
    }

    /// Los comandos registrados en `lib.rs` (`generate_handler!`).
    fn comandos_registrados() -> Vec<String> {
        let fuente = include_str!("../lib.rs");
        let lista = fuente
            .split("generate_handler![")
            .nth(1)
            .and_then(|resto| resto.split(']').next())
            .expect("lib.rs debe tener generate_handler![...]");
        lista
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(|c| c.rsplit("::").next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn todo_comando_registrado_esta_en_la_tabla_y_al_reves() {
        let registrados = comandos_registrados();
        assert!(registrados.len() >= PERMISOS.len());
        for comando in &registrados {
            assert!(
                acceso(comando).is_some(),
                "falta `{comando}` en identidad::permisos::PERMISOS"
            );
        }
        for (comando, _) in PERMISOS {
            assert!(
                registrados.iter().any(|r| r == comando),
                "`{comando}` está en PERMISOS pero no en lib.rs"
            );
        }
    }
}
