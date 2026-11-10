//! Firma del rol de cada usuario (issue #86). Ver "Firma criptográfica del rol"
//! en BACKEND.md.
//!
//! El rol de una fila de `usuarios` solo vale si su firma (`email|rol|firmado_en`)
//! fue hecha con la llave del Administrador del proyecto. Así, escribir un rol a
//! mano en la base de datos no sirve de nada.

use serde::Serialize;

use crate::db::usuarios::Usuario;
use crate::error::{Error, Resultado};
use crate::identidad::llaves::{self, LlaveAdmin};
use crate::modelo::{normalizar_email, Rol};

/// Rol que se aplica cuando la firma no coincide (el mínimo).
pub const ROL_DEGRADADO: Rol = Rol::Desarrollador;

/// Qué tan de fiar es el rol de una fila de `usuarios`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verificacion {
    /// Firma válida: el rol se aplica tal cual.
    Verificado,
    /// Hay firma pero no coincide (fila editada a mano): se degrada a
    /// Desarrollador y la interfaz avisa "rol no verificado".
    NoVerificado,
    /// Sin firma: la persona queda bloqueada hasta que la confirmen.
    Pendiente,
}

/// Un usuario con el rol que de verdad se le aplica.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsuarioVerificado {
    #[serde(flatten)]
    pub usuario: Usuario,
    pub verificacion: Verificacion,
    /// Rol que se aplica: el firmado, Desarrollador si la firma no coincide, o
    /// `None` si está pendiente (no puede entrar).
    pub rol_efectivo: Option<Rol>,
}

impl UsuarioVerificado {
    pub fn puede_entrar(&self) -> bool {
        self.rol_efectivo.is_some()
    }
}

/// Texto que se firma. `None` si algún campo trae `|`, que permitiría armar el
/// mismo texto con otros datos.
fn mensaje_rol(email: &str, rol: Rol, firmado_en: &str) -> Option<String> {
    let email = normalizar_email(email);
    if email.is_empty() || email.contains('|') || firmado_en.is_empty() || firmado_en.contains('|')
    {
        return None;
    }
    Some(format!("{email}|{}|{firmado_en}", rol.como_texto()))
}

/// Firma el rol de una persona con la llave del Administrador.
pub fn firmar_rol(
    llave: &LlaveAdmin,
    email: &str,
    rol: Rol,
    firmado_en: &str,
) -> Resultado<String> {
    let mensaje = mensaje_rol(email, rol, firmado_en).ok_or_else(|| {
        Error::Validacion("El correo o la fecha de la firma no son válidos".into())
    })?;
    Ok(llave.firmar(&mensaje))
}

/// Estado de la firma de una fila, contra la llave pública del Administrador.
pub fn verificar_firma(publica_admin: &str, usuario: &Usuario) -> Verificacion {
    let firma = match usuario.firma.as_deref().map(str::trim) {
        None | Some("") => return Verificacion::Pendiente,
        Some(firma) => firma,
    };
    let valida = usuario
        .firmado_en
        .as_deref()
        .and_then(|firmado_en| mensaje_rol(&usuario.email, usuario.rol, firmado_en))
        .is_some_and(|mensaje| llaves::verificar(publica_admin, &mensaje, firma));
    if valida {
        Verificacion::Verificado
    } else {
        Verificacion::NoVerificado
    }
}

/// Revisa la firma de un usuario y decide qué rol se le aplica.
pub fn verificar_usuario(publica_admin: &str, usuario: Usuario) -> UsuarioVerificado {
    let verificacion = verificar_firma(publica_admin, &usuario);
    let rol_efectivo = match verificacion {
        Verificacion::Verificado => Some(usuario.rol),
        Verificacion::NoVerificado => Some(ROL_DEGRADADO),
        Verificacion::Pendiente => None,
    };
    UsuarioVerificado {
        usuario,
        verificacion,
        rol_efectivo,
    }
}

/// Lo mismo para toda la tabla `usuarios`.
pub fn verificar_usuarios(publica_admin: &str, usuarios: Vec<Usuario>) -> Vec<UsuarioVerificado> {
    usuarios
        .into_iter()
        .map(|u| verificar_usuario(publica_admin, u))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FECHA: &str = "2026-11-10T10:00:00Z";

    fn llave() -> (tempfile::TempDir, LlaveAdmin, String) {
        let dir = tempfile::tempdir().unwrap();
        let publica = llaves::generar_llaves_admin(dir.path(), None).unwrap();
        let llave = llaves::cargar_llave_admin(dir.path(), None).unwrap();
        (dir, llave, publica)
    }

    fn usuario(email: &str, rol: Rol) -> Usuario {
        Usuario {
            email: email.into(),
            nombre: "Ana".into(),
            apellido: "Lopez".into(),
            rol,
            fecha_alta: "2026-11-10".into(),
            firma: None,
            firmado_en: None,
            firmado_por: None,
            llave_publica: None,
            certificado: None,
        }
    }

    fn firmado(llave: &LlaveAdmin, email: &str, rol: Rol) -> Usuario {
        Usuario {
            firma: Some(firmar_rol(llave, email, rol, FECHA).unwrap()),
            firmado_en: Some(FECHA.into()),
            ..usuario(email, rol)
        }
    }

    #[test]
    fn firma_valida_aplica_el_rol_tal_cual() {
        let (_dir, llave, publica) = llave();
        for rol in Rol::TODOS {
            let v = verificar_usuario(&publica, firmado(&llave, "ana@virustf.dev", *rol));
            assert_eq!(v.verificacion, Verificacion::Verificado);
            assert_eq!(v.rol_efectivo, Some(*rol));
            assert!(v.puede_entrar());
        }
        // El correo se compara sin distinguir mayúsculas.
        let mut u = firmado(&llave, "ana@virustf.dev", Rol::Lider);
        u.email = "Ana@VirusTF.dev".into();
        assert_eq!(verificar_firma(&publica, &u), Verificacion::Verificado);
    }

    #[test]
    fn firma_que_no_coincide_degrada_a_desarrollador() {
        let (_dir, llave, publica) = llave();
        let base = firmado(&llave, "ana@virustf.dev", Rol::Tester);

        // Rol cambiado a mano.
        let v = verificar_usuario(
            &publica,
            Usuario {
                rol: Rol::Administrador,
                ..base.clone()
            },
        );
        assert_eq!(v.verificacion, Verificacion::NoVerificado);
        assert_eq!(v.rol_efectivo, Some(Rol::Desarrollador));
        assert!(v.puede_entrar());

        // Firma copiada de otra persona, fecha cambiada, sin fecha, basura.
        let alterados = [
            Usuario {
                email: "otro@virustf.dev".into(),
                ..base.clone()
            },
            Usuario {
                firmado_en: Some("2027-01-01T00:00:00Z".into()),
                ..base.clone()
            },
            Usuario {
                firmado_en: None,
                ..base.clone()
            },
            Usuario {
                firma: Some("no es una firma".into()),
                ..base.clone()
            },
        ];
        for u in alterados {
            assert_eq!(verificar_firma(&publica, &u), Verificacion::NoVerificado);
        }

        // Firmada con una llave que no es la del Administrador del proyecto.
        let (_otro, intrusa, _) = self::llave();
        let u = firmado(&intrusa, "ana@virustf.dev", Rol::Administrador);
        assert_eq!(verificar_firma(&publica, &u), Verificacion::NoVerificado);
    }

    #[test]
    fn firma_vacia_deja_bloqueado() {
        let (_dir, _llave, publica) = llave();
        for firma in [None, Some(String::new()), Some("   ".to_string())] {
            let v = verificar_usuario(
                &publica,
                Usuario {
                    firma,
                    ..usuario("ana@virustf.dev", Rol::Lider)
                },
            );
            assert_eq!(v.verificacion, Verificacion::Pendiente);
            assert_eq!(v.rol_efectivo, None);
            assert!(!v.puede_entrar());
        }
    }

    #[test]
    fn el_separador_no_se_puede_colar_en_los_campos() {
        let (_dir, llave, publica) = llave();
        assert!(firmar_rol(&llave, "a@x.dev|Administrador", Rol::Tester, FECHA).is_err());
        assert!(firmar_rol(&llave, "a@x.dev", Rol::Tester, "t|Tester|t").is_err());
        assert!(firmar_rol(&llave, "", Rol::Tester, FECHA).is_err());
        // Aunque alguien tuviera una firma de ese texto, no se acepta.
        let trampa = Usuario {
            firmado_en: Some("t|Tester|t".into()),
            firma: Some(llave.firmar("a@x.dev|Administrador|t|Tester|t")),
            ..usuario("a@x.dev", Rol::Administrador)
        };
        assert_eq!(
            verificar_firma(&publica, &trampa),
            Verificacion::NoVerificado
        );
    }

    #[test]
    fn verifica_la_tabla_entera_y_viaja_al_frontend() {
        let (_dir, llave, publica) = llave();
        let lista = verificar_usuarios(
            &publica,
            vec![
                firmado(&llave, "admin@virustf.dev", Rol::Administrador),
                usuario("nuevo@virustf.dev", Rol::Tester),
            ],
        );
        assert_eq!(lista.len(), 2);
        let json = serde_json::to_value(&lista).unwrap();
        assert_eq!(json[0]["email"], "admin@virustf.dev");
        assert_eq!(json[0]["verificacion"], "verificado");
        assert_eq!(json[0]["rol_efectivo"], "Administrador");
        assert_eq!(json[1]["verificacion"], "pendiente");
        assert!(json[1]["rol_efectivo"].is_null());
    }
}
