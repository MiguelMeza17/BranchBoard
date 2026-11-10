//! Firma del rol de cada usuario (issues #86 y #88). Ver "Firma criptográfica
//! del rol" en BACKEND.md.
//!
//! El rol de una fila de `usuarios` solo vale si su firma (`email|rol|firmado_en`)
//! la hizo el Administrador del proyecto, o un Líder con certificado del
//! Administrador y solo para Desarrollador, Tester o Colaborador externo. Así,
//! escribir un rol a mano en la base de datos no sirve de nada.

use std::collections::HashMap;

use serde::Serialize;

use crate::db::usuarios::Usuario;
use crate::error::{Error, Resultado};
use crate::identidad::llaves::{self, LlavePrivada};
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

/// Firma el rol de una persona con la llave del Administrador (o de un Líder,
/// mejor con [`firmar_rol_como_lider`]).
pub fn firmar_rol(
    llave: &LlavePrivada,
    email: &str,
    rol: Rol,
    firmado_en: &str,
) -> Resultado<String> {
    let mensaje = mensaje_rol(email, rol, firmado_en).ok_or_else(|| {
        Error::Validacion("El correo o la fecha de la firma no son válidos".into())
    })?;
    Ok(llave.firmar(&mensaje))
}

/// Roles que un Líder puede firmar (issue #88). Nunca Líder ni Administrador.
pub fn rol_asignable_por_lider(rol: Rol) -> bool {
    matches!(
        rol,
        Rol::Desarrollador | Rol::Tester | Rol::ColaboradorExterno
    )
}

/// Firma un rol con la llave de un Líder. Rechaza los roles que un Líder no
/// puede asignar (la verificación también los rechazaría).
pub fn firmar_rol_como_lider(
    llave_lider: &LlavePrivada,
    email: &str,
    rol: Rol,
    firmado_en: &str,
) -> Resultado<String> {
    if !rol_asignable_por_lider(rol) {
        return Err(Error::SinPermiso);
    }
    firmar_rol(llave_lider, email, rol, firmado_en)
}

/// Texto del certificado de un Líder: `email_lider|llave_publica|fecha`.
fn mensaje_certificado(email: &str, llave_publica: &str, fecha: &str) -> Option<String> {
    let email = normalizar_email(email);
    let llave_publica = llave_publica.trim();
    if [email.as_str(), llave_publica, fecha]
        .iter()
        .any(|campo| campo.is_empty() || campo.contains('|'))
    {
        return None;
    }
    Some(format!("{email}|{llave_publica}|{fecha}"))
}

/// El Administrador certifica la llave pública de un Líder al nombrarlo. Lo que
/// devuelve va en la columna `certificado`: `fecha|firma`.
pub fn certificar_lider(
    llave_admin: &LlavePrivada,
    email_lider: &str,
    llave_publica_lider: &str,
    fecha: &str,
) -> Resultado<String> {
    let mensaje =
        mensaje_certificado(email_lider, llave_publica_lider, fecha).ok_or_else(|| {
            Error::Validacion(
                "El correo, la llave o la fecha del certificado no son válidos".into(),
            )
        })?;
    Ok(format!("{fecha}|{}", llave_admin.firmar(&mensaje)))
}

/// `true` si la fila trae una firma de `email|rol|firmado_en` hecha con `publica`.
fn firma_hecha_con(publica: &str, usuario: &Usuario) -> bool {
    let (Some(firma), Some(firmado_en)) = (usuario.firma.as_deref(), usuario.firmado_en.as_deref())
    else {
        return false;
    };
    mensaje_rol(&usuario.email, usuario.rol, firmado_en)
        .is_some_and(|mensaje| llaves::verificar(publica, &mensaje, firma.trim()))
}

/// Un Líder con certificado válido del Administrador.
#[derive(Debug, Clone)]
struct LiderCertificado {
    llave_publica: String,
    /// Desde cuándo vale su llave (fecha del certificado).
    desde: String,
    /// Si ya no es Líder: desde cuándo (fecha en que el Administrador firmó su
    /// rol nuevo). Sus firmas de antes siguen valiendo; las de después, no.
    revocado_desde: Option<String>,
}

/// Verifica las filas de `usuarios` contra la llave del Administrador y las de
/// los Líderes que él certificó. Se arma con la tabla entera, porque la firma de
/// una fila puede depender de la fila de otro (el Líder que la firmó).
#[derive(Debug, Clone)]
pub struct Verificador {
    publica_admin: String,
    lideres: HashMap<String, LiderCertificado>,
}

impl Verificador {
    pub fn nuevo(publica_admin: &str, usuarios: &[Usuario]) -> Self {
        let lideres = usuarios
            .iter()
            .filter_map(|u| {
                Some((
                    normalizar_email(&u.email),
                    certificado_valido(publica_admin, u)?,
                ))
            })
            .collect();
        Self {
            publica_admin: publica_admin.trim().to_string(),
            lideres,
        }
    }

    /// Estado de la firma de una fila.
    pub fn verificar_firma(&self, usuario: &Usuario) -> Verificacion {
        if usuario
            .firma
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            return Verificacion::Pendiente;
        }
        if firma_hecha_con(&self.publica_admin, usuario) || self.firmada_por_lider(usuario) {
            Verificacion::Verificado
        } else {
            Verificacion::NoVerificado
        }
    }

    fn firmada_por_lider(&self, usuario: &Usuario) -> bool {
        let Some(lider) = usuario
            .firmado_por
            .as_deref()
            .and_then(|por| self.lideres.get(&normalizar_email(por)))
        else {
            return false;
        };
        let Some(firmado_en) = usuario.firmado_en.as_deref() else {
            return false;
        };
        // Fechas ISO 8601 en UTC: compararlas como texto es compararlas en el tiempo.
        rol_asignable_por_lider(usuario.rol)
            && firmado_en >= lider.desde.as_str()
            && lider
                .revocado_desde
                .as_deref()
                .map_or(true, |revocado| firmado_en <= revocado)
            && firma_hecha_con(&lider.llave_publica, usuario)
    }

    /// Revisa la firma de un usuario y decide qué rol se le aplica.
    pub fn verificar_usuario(&self, usuario: Usuario) -> UsuarioVerificado {
        let verificacion = self.verificar_firma(&usuario);
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
}

/// El certificado de una fila, si su llave la certificó el Administrador y la
/// propia fila (rol incluido) la firmó el Administrador directamente.
fn certificado_valido(publica_admin: &str, usuario: &Usuario) -> Option<LiderCertificado> {
    let llave_publica = usuario.llave_publica.as_deref()?.trim();
    let (desde, firma) = usuario.certificado.as_deref()?.trim().split_once('|')?;
    let mensaje = mensaje_certificado(&usuario.email, llave_publica, desde)?;
    if !llaves::verificar(publica_admin, &mensaje, firma)
        || !firma_hecha_con(publica_admin, usuario)
    {
        return None;
    }
    let revocado_desde = (usuario.rol != Rol::Lider)
        .then(|| usuario.firmado_en.clone())
        .flatten();
    Some(LiderCertificado {
        llave_publica: llave_publica.to_string(),
        desde: desde.to_string(),
        revocado_desde,
    })
}

/// Verifica toda la tabla `usuarios`.
pub fn verificar_usuarios(publica_admin: &str, usuarios: Vec<Usuario>) -> Vec<UsuarioVerificado> {
    let verificador = Verificador::nuevo(publica_admin, &usuarios);
    usuarios
        .into_iter()
        .map(|u| verificador.verificar_usuario(u))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FECHA: &str = "2026-11-10T10:00:00Z";

    fn llave() -> (tempfile::TempDir, LlavePrivada, String) {
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

    fn firmado(llave: &LlavePrivada, email: &str, rol: Rol) -> Usuario {
        firmado_en(llave, email, rol, FECHA)
    }

    fn firmado_en(llave: &LlavePrivada, email: &str, rol: Rol, fecha: &str) -> Usuario {
        Usuario {
            firma: Some(firmar_rol(llave, email, rol, fecha).unwrap()),
            firmado_en: Some(fecha.into()),
            ..usuario(email, rol)
        }
    }

    // Una sola fila, sin Líderes en la tabla: solo cuenta la firma del Administrador.
    fn verificar_usuario(publica: &str, u: Usuario) -> UsuarioVerificado {
        Verificador::nuevo(publica, &[]).verificar_usuario(u)
    }

    fn verificar_firma(publica: &str, u: &Usuario) -> Verificacion {
        Verificador::nuevo(publica, &[]).verificar_firma(u)
    }

    const LIDER: &str = "lider@virustf.dev";
    const CERTIFICADO_EN: &str = "2026-11-01T09:00:00Z";

    /// Administrador, y un Líder con su llave, su certificado y su fila firmada
    /// por el Administrador con el rol `rol_lider` en la fecha `lider_firmado_en`.
    struct Equipo {
        _dirs: (tempfile::TempDir, tempfile::TempDir),
        admin: LlavePrivada,
        publica_admin: String,
        lider: LlavePrivada,
        fila_lider: Usuario,
    }

    fn equipo(rol_lider: Rol, lider_firmado_en: &str) -> Equipo {
        let (dir_admin, admin, publica_admin) = llave();
        let dir_lider = tempfile::tempdir().unwrap();
        let publica_lider = llaves::generar_llaves_lider(dir_lider.path(), None).unwrap();
        let lider = llaves::cargar_llave_lider(dir_lider.path(), None).unwrap();
        let fila_lider = Usuario {
            llave_publica: Some(publica_lider.clone()),
            certificado: Some(
                certificar_lider(&admin, LIDER, &publica_lider, CERTIFICADO_EN).unwrap(),
            ),
            ..firmado_en(&admin, LIDER, rol_lider, lider_firmado_en)
        };
        Equipo {
            _dirs: (dir_admin, dir_lider),
            admin,
            publica_admin,
            lider,
            fila_lider,
        }
    }

    impl Equipo {
        fn firmado_por_lider(&self, email: &str, rol: Rol, fecha: &str) -> Usuario {
            Usuario {
                firma: Some(firmar_rol(&self.lider, email, rol, fecha).unwrap()),
                firmado_en: Some(fecha.into()),
                firmado_por: Some(LIDER.into()),
                ..usuario(email, rol)
            }
        }

        fn verificar(&self, fila: Usuario) -> Verificacion {
            let tabla = vec![self.fila_lider.clone(), fila];
            verificar_usuarios(&self.publica_admin, tabla)[1].verificacion
        }
    }

    #[test]
    fn un_lider_certificado_firma_desarrollador_tester_y_colaborador() {
        let e = equipo(Rol::Lider, CERTIFICADO_EN);
        for rol in [Rol::Desarrollador, Rol::Tester, Rol::ColaboradorExterno] {
            let fila = e.firmado_por_lider("nuevo@virustf.dev", rol, FECHA);
            assert_eq!(e.verificar(fila), Verificacion::Verificado, "{rol}");
        }
        // La fila del propio Líder también es de confianza.
        let lista = verificar_usuarios(&e.publica_admin, vec![e.fila_lider.clone()]);
        assert_eq!(lista[0].rol_efectivo, Some(Rol::Lider));
    }

    #[test]
    fn un_lider_no_firma_lider_ni_administrador() {
        let e = equipo(Rol::Lider, CERTIFICADO_EN);
        for rol in [Rol::Lider, Rol::Administrador] {
            assert!(firmar_rol_como_lider(&e.lider, "x@virustf.dev", rol, FECHA).is_err());
            // Aunque la firma se haga a mano, no se acepta.
            let fila = e.firmado_por_lider("x@virustf.dev", rol, FECHA);
            assert_eq!(e.verificar(fila), Verificacion::NoVerificado, "{rol}");
        }
        assert!(firmar_rol_como_lider(&e.lider, "x@virustf.dev", Rol::Tester, FECHA).is_ok());
    }

    #[test]
    fn sin_certificado_valido_la_firma_del_lider_no_vale() {
        let e = equipo(Rol::Lider, CERTIFICADO_EN);
        let fila = e.firmado_por_lider("nuevo@virustf.dev", Rol::Tester, FECHA);

        // Sin la fila del Líder en la tabla.
        assert_eq!(
            verificar_usuarios(&e.publica_admin, vec![fila.clone()])[0].verificacion,
            Verificacion::NoVerificado
        );

        let con_lider = |fila_lider: Usuario| {
            verificar_usuarios(&e.publica_admin, vec![fila_lider, fila.clone()])[1].verificacion
        };
        // Certificado que el Líder se hizo él mismo.
        let publica_lider = e.fila_lider.llave_publica.clone().unwrap();
        let autocertificado =
            certificar_lider(&e.lider, LIDER, &publica_lider, CERTIFICADO_EN).unwrap();
        assert_eq!(
            con_lider(Usuario {
                certificado: Some(autocertificado),
                ..e.fila_lider.clone()
            }),
            Verificacion::NoVerificado
        );
        // Otra llave pública puesta a mano junto al certificado legítimo.
        let (_d, _otra, intrusa) = llave();
        assert_eq!(
            con_lider(Usuario {
                llave_publica: Some(intrusa),
                ..e.fila_lider.clone()
            }),
            Verificacion::NoVerificado
        );
        // Rol de Líder escrito a mano: su fila no la firmó el Administrador con ese rol.
        assert_eq!(
            con_lider(Usuario {
                rol: Rol::Lider,
                ..firmado(&e.admin, LIDER, Rol::Desarrollador)
            }),
            Verificacion::NoVerificado
        );
        // El certificado legítimo sí vale.
        assert_eq!(con_lider(e.fila_lider.clone()), Verificacion::Verificado);
    }

    #[test]
    fn no_vale_una_firma_anterior_al_certificado() {
        let e = equipo(Rol::Lider, CERTIFICADO_EN);
        let fila = e.firmado_por_lider("nuevo@virustf.dev", Rol::Tester, "2026-10-01T00:00:00Z");
        assert_eq!(e.verificar(fila), Verificacion::NoVerificado);
    }

    #[test]
    fn quitar_el_rol_de_lider_revoca_sus_firmas_nuevas() {
        // El 2026-11-05 el Administrador lo pasó a Desarrollador.
        let e = equipo(Rol::Desarrollador, "2026-11-05T12:00:00Z");
        let antes = e.firmado_por_lider("a@virustf.dev", Rol::Tester, "2026-11-03T08:00:00Z");
        let despues = e.firmado_por_lider("b@virustf.dev", Rol::Tester, "2026-11-06T08:00:00Z");
        assert_eq!(e.verificar(antes), Verificacion::Verificado);
        assert_eq!(e.verificar(despues), Verificacion::NoVerificado);

        // Eliminado del equipo: su fila ya no está y ninguna de sus firmas vale.
        let fila = e.firmado_por_lider("a@virustf.dev", Rol::Tester, "2026-11-03T08:00:00Z");
        assert_eq!(
            verificar_usuarios(&e.publica_admin, vec![fila])[0].verificacion,
            Verificacion::NoVerificado
        );
    }

    #[test]
    fn el_certificado_rechaza_el_separador() {
        let (_dir, admin, _) = llave();
        assert!(certificar_lider(&admin, "a@x.dev|b", "llave", FECHA).is_err());
        assert!(certificar_lider(&admin, "a@x.dev", "", FECHA).is_err());
        assert!(certificar_lider(&admin, "a@x.dev", "llave", "f|g").is_err());
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
