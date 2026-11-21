//! Usuarios y roles de un proyecto en el servidor (#241): la tabla `miembros`, las invitaciones y las
//! funciones `agregar_miembro`, `cambiar_rol`, `quitar_miembro` y `quitar_invitacion` (#234, migración
//! `20261121061500_invitaciones.sql`). El servidor revisa el rol de quien pide cada cosa; la app solo
//! traduce y guarda una copia local en `usuarios` ([`filas_locales`]).

use serde::Deserialize;
use serde_json::{json, Value};

use super::cliente::ClienteSupabase;
use crate::db::usuarios::Usuario;
use crate::error::{Error, Resultado};
use crate::modelo::{normalizar_email, Rol};
use crate::servidor::pat::Llavero;

/// Perfil de un miembro (lo que deja ver la regla `perfiles_leer`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Perfil {
    pub id: String,
    pub correo: String,
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub apellido: String,
}

/// Una fila de `miembros` con su perfil.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Miembro {
    pub rol: Rol,
    pub agregado_en: String,
    pub usuario: Perfil,
}

/// Una invitación a alguien que todavía no tiene cuenta (solo las ven Líderes y Administradores).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Invitacion {
    pub correo: String,
    pub rol: Rol,
    pub invitado_en: String,
}

/// Quiénes están en el proyecto y quiénes están invitados.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Equipo {
    pub miembros: Vec<Miembro>,
    pub invitaciones: Vec<Invitacion>,
}

/// `miembros` tiene dos llaves hacia `perfiles` (`usuario_id` y `agregado_por`): hay que decir cuál.
pub fn ruta_miembros(proyecto: &str) -> String {
    format!(
        "/rest/v1/miembros?select=rol,agregado_en,usuario:perfiles!miembros_usuario_id_fkey(id,correo,nombre,apellido)&proyecto_id=eq.{proyecto}"
    )
}

pub fn ruta_invitaciones(proyecto: &str) -> String {
    format!("/rest/v1/invitaciones?select=correo,rol,invitado_en&proyecto_id=eq.{proyecto}&order=invitado_en")
}

/// El proyecto tiene que ser un uuid: va dentro de una URL.
fn exigir_uuid(proyecto: &str) -> Resultado<&str> {
    let p = proyecto.trim();
    let ok = p.len() == 36
        && p.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if ok {
        Ok(p)
    } else {
        Err(Error::Validacion(
            "El id del proyecto en el servidor no es válido".into(),
        ))
    }
}

fn como<T: serde::de::DeserializeOwned>(v: Value) -> Resultado<T> {
    serde_json::from_value(v)
        .map_err(|_| Error::Servidor("respuesta que la app no entiende".into()))
}

/// Lee miembros e invitaciones del proyecto.
pub fn leer_equipo<L: Llavero>(cliente: &ClienteSupabase<L>, proyecto: &str) -> Resultado<Equipo> {
    let p = exigir_uuid(proyecto)?;
    Ok(Equipo {
        miembros: como(cliente.llamar("GET", &ruta_miembros(p), None)?)?,
        invitaciones: como(cliente.llamar("GET", &ruta_invitaciones(p), None)?)?,
    })
}

/// Agrega a alguien por su correo. `Some(id)` si ya tenía cuenta y entró; `None` si quedó invitado.
pub fn agregar<L: Llavero>(
    cliente: &ClienteSupabase<L>,
    proyecto: &str,
    correo: &str,
    rol: Rol,
) -> Resultado<Option<String>> {
    let r = cliente.llamar(
        "POST",
        "/rest/v1/rpc/agregar_miembro",
        Some(&json!({
            "p_proyecto": exigir_uuid(proyecto)?,
            "p_correo": normalizar_email(correo),
            "p_rol": rol,
        })),
    )?;
    Ok(r.as_str().map(str::to_string))
}

pub fn cambiar_rol<L: Llavero>(
    cliente: &ClienteSupabase<L>,
    proyecto: &str,
    usuario_id: &str,
    rol: Rol,
) -> Resultado<()> {
    cliente.llamar(
        "POST",
        "/rest/v1/rpc/cambiar_rol",
        Some(
            &json!({ "p_proyecto": exigir_uuid(proyecto)?, "p_usuario": usuario_id, "p_rol": rol }),
        ),
    )?;
    Ok(())
}

pub fn quitar<L: Llavero>(
    cliente: &ClienteSupabase<L>,
    proyecto: &str,
    usuario_id: &str,
) -> Resultado<()> {
    cliente.llamar(
        "POST",
        "/rest/v1/rpc/quitar_miembro",
        Some(&json!({ "p_proyecto": exigir_uuid(proyecto)?, "p_usuario": usuario_id })),
    )?;
    Ok(())
}

pub fn quitar_invitacion<L: Llavero>(
    cliente: &ClienteSupabase<L>,
    proyecto: &str,
    correo: &str,
) -> Resultado<()> {
    cliente.llamar(
        "POST",
        "/rest/v1/rpc/quitar_invitacion",
        Some(
            &json!({ "p_proyecto": exigir_uuid(proyecto)?, "p_correo": normalizar_email(correo) }),
        ),
    )?;
    Ok(())
}

impl Equipo {
    /// El id de la cuenta de `correo`, si es miembro.
    pub fn id_de(&self, correo: &str) -> Option<&str> {
        let correo = normalizar_email(correo);
        self.miembros
            .iter()
            .find(|m| normalizar_email(&m.usuario.correo) == correo)
            .map(|m| m.usuario.id.as_str())
    }

    /// `true` si `correo` está invitado (sin cuenta todavía).
    pub fn esta_invitado(&self, correo: &str) -> bool {
        let correo = normalizar_email(correo);
        self.invitaciones
            .iter()
            .any(|i| normalizar_email(&i.correo) == correo)
    }
}

/// La copia local (`usuarios`) de los miembros: sin firmas (los roles los hace cumplir el servidor). Sin
/// nombre en el perfil (cuentas de Unity VC, #237), se saca del correo como en las altas de antes.
pub fn filas_locales(equipo: &Equipo) -> Vec<Usuario> {
    equipo
        .miembros
        .iter()
        .map(|m| {
            let email = normalizar_email(&m.usuario.correo);
            let (nombre, apellido) = if m.usuario.nombre.trim().is_empty() {
                crate::db::usuarios::nombre_desde_email(&email)
            } else {
                (
                    m.usuario.nombre.trim().to_string(),
                    m.usuario.apellido.trim().to_string(),
                )
            };
            Usuario {
                email,
                nombre,
                apellido,
                rol: m.rol,
                fecha_alta: m.agregado_en.chars().take(10).collect(),
                firma: None,
                firmado_en: None,
                firmado_por: None,
                llave_publica: None,
                certificado: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "11111111-2222-4333-8444-555555555555";

    fn equipo() -> Equipo {
        // Forma de las respuestas de PostgREST (ver `ruta_miembros`).
        let miembros = json!([
            {"rol": "Administrador", "agregado_en": "2026-11-21T10:00:00.123+00:00",
             "usuario": {"id": "a0000000-0000-4000-8000-000000000001", "correo": "ana@virustf.dev",
                         "nombre": "Ana", "apellido": "López"}},
            {"rol": "Colaborador externo", "agregado_en": "2026-11-22T08:00:00+00:00",
             "usuario": {"id": "b0000000-0000-4000-8000-000000000002", "correo": "Beto.Ruiz@VirusTF.dev",
                         "nombre": "", "apellido": ""}}
        ]);
        let invitaciones = json!([
            {"correo": "nuevo@virustf.dev", "rol": "Tester", "invitado_en": "2026-11-22T09:00:00+00:00"}
        ]);
        Equipo {
            miembros: como(miembros).unwrap(),
            invitaciones: como(invitaciones).unwrap(),
        }
    }

    #[test]
    fn las_rutas_llevan_el_proyecto_y_la_llave_correcta() {
        assert!(ruta_miembros(P).contains("perfiles!miembros_usuario_id_fkey"));
        assert!(ruta_miembros(P).ends_with(&format!("proyecto_id=eq.{P}")));
        assert!(ruta_invitaciones(P).contains(&format!("proyecto_id=eq.{P}")));
        assert!(exigir_uuid(P).is_ok());
        for malo in [
            "",
            "x",
            "11111111-2222-4333-8444-55555555555&",
            &format!("{P}&rol=eq.x"),
        ] {
            assert!(exigir_uuid(malo).is_err(), "{malo}");
        }
    }

    #[test]
    fn la_copia_local_no_lleva_firmas_y_completa_el_nombre() {
        let filas = filas_locales(&equipo());
        assert_eq!(filas.len(), 2);
        assert_eq!(filas[0].email, "ana@virustf.dev");
        assert_eq!(filas[0].rol, Rol::Administrador);
        assert_eq!(filas[0].fecha_alta, "2026-11-21");
        assert!(filas
            .iter()
            .all(|u| u.firma.is_none() && u.certificado.is_none()));
        // Sin nombre en el perfil (cuenta de Unity VC): sale del correo.
        assert_eq!(filas[1].email, "beto.ruiz@virustf.dev");
        assert_eq!(
            (filas[1].nombre.as_str(), filas[1].apellido.as_str()),
            ("Beto", "Ruiz")
        );
        assert_eq!(filas[1].rol, Rol::ColaboradorExterno);
    }

    #[test]
    fn buscar_por_correo_sin_importar_mayusculas() {
        let e = equipo();
        assert_eq!(
            e.id_de("BETO.ruiz@virustf.dev"),
            Some("b0000000-0000-4000-8000-000000000002")
        );
        assert_eq!(e.id_de("nuevo@virustf.dev"), None);
        assert!(e.esta_invitado("Nuevo@VirusTF.dev"));
        assert!(!e.esta_invitado("ana@virustf.dev"));
    }

    #[test]
    fn un_rol_desconocido_es_un_error_del_servidor() {
        let r: Resultado<Vec<Invitacion>> =
            como(json!([{"correo": "x@y.z", "rol": "Jefe", "invitado_en": "2026"}]));
        assert!(matches!(r, Err(Error::Servidor(_))));
    }
}
