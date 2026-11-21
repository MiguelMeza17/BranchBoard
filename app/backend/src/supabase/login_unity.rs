//! Al iniciar sesión con Unity VC, abrir también la sesión del servidor de BranchBoard (#237).
//!
//! El token que se manda a la Edge Function `login-unity` es el de la sesión de `cm` que dejó el login por
//! el navegador (`sso_navegador.rs`) en `tokens.conf`. Se lee solo para mandarlo por HTTPS al servidor de
//! BranchBoard, que se lo pregunta a Unity VC; no se copia a ningún otro lado ni se registra.

use std::path::Path;

use super::{ClienteSupabase, Entorno};
use crate::error::{Error, Resultado};
use crate::servidor::pat::{Llavero, LlaveroSistema};
use crate::servidor::region;

/// Organización y token de la sesión de Unity VC de `correo`, leídos de `tokens.conf`.
#[derive(PartialEq, Eq)]
pub struct CredencialUnity {
    pub org: String,
    pub token: String,
}

impl std::fmt::Debug for CredencialUnity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredencialUnity")
            .field("org", &self.org)
            .field("token", &"<oculto>")
            .finish()
    }
}

/// Busca en `tokens.conf` (`server=<org>@cloud user=<correo> token=<token> …`) la línea de `correo`.
/// Prefiere la que nombra la organización por su nombre (`MiguelM17`) sobre la del id numérico
/// (`4673426565751`): las dos traen el mismo token, pero el nombre es el que usa la API.
pub fn credencial_en_tokens_conf(texto: &str, correo: &str) -> Option<CredencialUnity> {
    let correo = correo.trim().to_lowercase();
    let mut encontradas: Vec<CredencialUnity> = texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
        .filter_map(|linea| {
            let (mut servidor, mut usuario, mut token) = ("", "", "");
            for par in linea.split_whitespace() {
                match par.split_once('=') {
                    Some(("server", v)) => servidor = v,
                    Some(("user", v)) => usuario = v,
                    Some(("token", v)) => token = v,
                    _ => {}
                }
            }
            let org = servidor.strip_suffix("@cloud")?;
            (usuario.to_lowercase() == correo && !org.is_empty() && !token.is_empty()).then(|| {
                CredencialUnity {
                    org: org.to_string(),
                    token: token.to_string(),
                }
            })
        })
        .collect();
    encontradas.sort_by_key(|c| c.org.chars().all(|ch| ch.is_ascii_digit()));
    encontradas.into_iter().next()
}

pub(crate) fn credencial_de(carpeta_plastic4: &Path, correo: &str) -> Resultado<CredencialUnity> {
    let texto = std::fs::read_to_string(carpeta_plastic4.join("tokens.conf"))
        .map_err(|_| Error::SinSesionUnityVc)?;
    credencial_en_tokens_conf(&texto, correo).ok_or(Error::SinSesionUnityVc)
}

/// Deja abierta la sesión del servidor para `correo`. Si ya hay una guardada de ese correo no toca la red
/// (devuelve `false`); si no, la pide a `login-unity` con el token de Unity VC (devuelve `true`).
///
/// El token de `tokens.conf` dura ~1 h y `cm` lo renueva solo cuando habla con el servidor. Si Unity lo
/// rechaza, se llama a `renovar_token(org)` (un comando remoto de `cm`) y se reintenta una vez con el nuevo.
pub fn asegurar_sesion_servidor<L: Llavero>(
    cliente: &ClienteSupabase<L>,
    carpeta_plastic4: &Path,
    correo: &str,
    webserver: impl Fn(&str) -> Resultado<String>,
    renovar_token: impl Fn(&str) -> Resultado<()>,
) -> Resultado<bool> {
    let correo = correo.trim().to_lowercase();
    if let Some(sesion) = cliente.sesion_guardada()? {
        if sesion.correo == correo {
            return Ok(false);
        }
    }
    let credencial = credencial_de(carpeta_plastic4, &correo)?;
    let servidor = webserver(&credencial.org)?;
    match cliente.iniciar_sesion_con_unity(&credencial.token, &credencial.org, &servidor) {
        Err(Error::SesionUnityVcCaducada) => {
            renovar_token(&credencial.org)?;
            let nueva = credencial_de(carpeta_plastic4, &correo)?;
            cliente.iniciar_sesion_con_unity(&nueva.token, &nueva.org, &servidor)?;
        }
        otro => {
            otro?;
        }
    }
    Ok(true)
}

/// Renueva el token de `tokens.conf` con un comando remoto de `cm` (lo renueva solo al usarse; probado
/// el 2026-11-21: después de `cm repo list <org>@unity` el token cambia).
pub fn renovar_token_cm(estado: &crate::estado::AppState, org: &str) -> Resultado<()> {
    crate::cm::ejecutor::Comando::cm()
        .arg("repo")
        .arg("list")
        .arg(format!("{org}@unity"))
        .exigir_exito(estado)
        .map(|_| ())
}

/// [`asegurar_sesion_servidor`] con el proyecto de Supabase del entorno, el llavero del sistema y la
/// carpeta `plastic4` real.
pub fn asegurar_sesion_servidor_del_equipo(
    estado: &crate::estado::AppState,
    correo: &str,
) -> Resultado<bool> {
    let carpeta = crate::cm::sesion::carpeta_config_plastic().ok_or(Error::SinSesionUnityVc)?;
    let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
    asegurar_sesion_servidor(&cliente, &carpeta, correo, region::webserver, |org| {
        renovar_token_cm(estado, org)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const TOKENS: &str = "// This file is automatically generated by Unity VCS to store tokens\n\
        server=4673426565751@cloud user=ana@virustf.dev token=TOKENeyJuno seiddata=ana@virustf.dev\n\
        server=VirusTF@cloud user=Ana@VirusTF.dev token=TOKENeyJuno seiddata=ana@virustf.dev\n\
        server=Otra@cloud user=beto@virustf.dev token=TOKENeyJbeto seiddata=beto@virustf.dev\n\
        server=local:8087 user=ana@virustf.dev token=TOKENlocal seiddata=ana@virustf.dev\n";

    #[test]
    fn toma_la_organizacion_por_nombre_y_el_token_del_correo() {
        let c = credencial_en_tokens_conf(TOKENS, " ANA@virustf.dev ").unwrap();
        assert_eq!(c.org, "VirusTF");
        assert_eq!(c.token, "TOKENeyJuno");
        let b = credencial_en_tokens_conf(TOKENS, "beto@virustf.dev").unwrap();
        assert_eq!((b.org.as_str(), b.token.as_str()), ("Otra", "TOKENeyJbeto"));
    }

    #[test]
    fn sin_linea_del_correo_o_solo_servidor_local_no_hay_credencial() {
        assert_eq!(credencial_en_tokens_conf(TOKENS, "nadie@virustf.dev"), None);
        let solo_local = "server=local:8087 user=ana@virustf.dev token=TOKENlocal\n";
        assert_eq!(
            credencial_en_tokens_conf(solo_local, "ana@virustf.dev"),
            None
        );
    }

    #[test]
    fn con_solo_el_id_numerico_lo_usa() {
        let t = "server=4673426565751@cloud user=ana@virustf.dev token=TOKENeyJuno\n";
        assert_eq!(
            credencial_en_tokens_conf(t, "ana@virustf.dev").unwrap().org,
            "4673426565751"
        );
    }

    #[test]
    fn debug_no_muestra_el_token() {
        let c = credencial_en_tokens_conf(TOKENS, "ana@virustf.dev").unwrap();
        assert!(!format!("{c:?}").contains("TOKENeyJ"));
    }

    #[derive(Default)]
    struct LlaveroMemoria(RefCell<HashMap<String, String>>);
    impl Llavero for LlaveroMemoria {
        fn leer(&self, cuenta: &str) -> Resultado<Option<String>> {
            Ok(self.0.borrow().get(cuenta).cloned())
        }
        fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()> {
            self.0.borrow_mut().insert(cuenta.into(), valor.into());
            Ok(())
        }
        fn borrar(&self, cuenta: &str) -> Resultado<()> {
            self.0.borrow_mut().remove(cuenta);
            Ok(())
        }
    }

    #[test]
    fn con_sesion_guardada_del_mismo_correo_no_toca_la_red() {
        let llavero = LlaveroMemoria::default();
        let sesion = super::super::Sesion {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expira_en: 0,
            usuario_id: "u".into(),
            correo: "ana@virustf.dev".into(),
            nombre: String::new(),
            apellido: String::new(),
        };
        llavero
            .guardar(
                "supabase-branchboard-pruebas",
                &serde_json::to_string(&sesion).unwrap(),
            )
            .unwrap();
        let cliente = ClienteSupabase::new(&super::super::entorno::PRUEBAS, llavero);
        let carpeta = tempfile::tempdir().unwrap();
        // Sin tokens.conf y con un `webserver` que falla: si intentara la red, daría error.
        let r = asegurar_sesion_servidor(
            &cliente,
            carpeta.path(),
            "Ana@VirusTF.dev",
            |_| Err(Error::Interno("no debía llamarse".into())),
            |_| Err(Error::Interno("no debía llamarse".into())),
        );
        assert!(matches!(r, Ok(false)));
    }

    #[test]
    fn sin_tokens_conf_pide_iniciar_sesion_de_unity() {
        let cliente =
            ClienteSupabase::new(&super::super::entorno::PRUEBAS, LlaveroMemoria::default());
        let carpeta = tempfile::tempdir().unwrap();
        let r = asegurar_sesion_servidor(
            &cliente,
            carpeta.path(),
            "ana@virustf.dev",
            |_| Ok("https://x.plasticscm.com:7178".into()),
            |_| Ok(()),
        );
        assert!(matches!(r, Err(Error::SinSesionUnityVc)));
    }

    /// Contra branchboard-pruebas con la sesión real de Unity VC de este equipo: abre la sesión del
    /// servidor con `login-unity` y la usa en la API de datos. Al final cierra la sesión; la cuenta que
    /// queda en pruebas se borra a mano después (ver BACKEND.md).
    #[test]
    #[ignore = "toca branchboard-pruebas y la sesión real de Unity VC: correr a mano con --ignored"]
    fn entorno_real_login_con_unity_abre_la_sesion_del_servidor() {
        let estado = crate::estado::AppState::default();
        let correo = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let carpeta = crate::cm::sesion::carpeta_config_plastic().unwrap();
        let cliente =
            ClienteSupabase::new(&super::super::entorno::PRUEBAS, LlaveroMemoria::default());

        let abrio =
            asegurar_sesion_servidor(&cliente, &carpeta, &correo, region::webserver, |org| {
                renovar_token_cm(&estado, org)
            })
            .unwrap();
        assert!(abrio, "sin sesión guardada debía pedirla");
        let sesion = cliente.sesion_guardada().unwrap().unwrap();
        assert_eq!(sesion.correo, correo);

        // La sesión vale en la API de datos (perfil propio, reglas de #234).
        let perfiles = cliente
            .llamar("GET", "/rest/v1/perfiles?select=correo", None)
            .unwrap();
        assert_eq!(perfiles, serde_json::json!([{ "correo": correo }]));

        // La segunda vez no vuelve a pedirla.
        assert!(
            !asegurar_sesion_servidor(&cliente, &carpeta, &correo, region::webserver, |org| {
                renovar_token_cm(&estado, org)
            })
            .unwrap()
        );

        // Un token que Unity rechaza → hay que volver a iniciar sesión con Unity.
        let otro = ClienteSupabase::new(&super::super::entorno::PRUEBAS, LlaveroMemoria::default());
        let servidor = region::webserver(&credencial_de(&carpeta, &correo).unwrap().org).unwrap();
        assert!(matches!(
            otro.iniciar_sesion_con_unity("TOKENnovale", "MiguelM17", &servidor),
            Err(Error::SesionUnityVcCaducada)
        ));

        cliente.cerrar_sesion().unwrap();
        println!(
            "login-unity verificado para {correo} (cuenta de prueba creada en branchboard-pruebas)"
        );
    }

    /// Token de `tokens.conf` vencido: Unity lo rechaza, se renueva (aquí se simula escribiendo el token
    /// real en un `tokens.conf` temporal, como haría `cm`) y el segundo intento abre la sesión.
    #[test]
    #[ignore = "toca branchboard-pruebas y la sesión real de Unity VC: correr a mano con --ignored"]
    fn entorno_real_token_vencido_se_renueva_y_se_reintenta() {
        let estado = crate::estado::AppState::default();
        let correo = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let real = crate::cm::sesion::carpeta_config_plastic().unwrap();
        renovar_token_cm(&estado, "MiguelM17").unwrap();
        let bueno = credencial_de(&real, &correo).unwrap();

        let falsa = tempfile::tempdir().unwrap();
        let linea = |token: &str| {
            format!(
                "server={}@cloud user={correo} token={token}
",
                bueno.org
            )
        };
        std::fs::write(falsa.path().join("tokens.conf"), linea("TOKENvencido")).unwrap();
        let renovaciones = RefCell::new(0);
        let cliente =
            ClienteSupabase::new(&super::super::entorno::PRUEBAS, LlaveroMemoria::default());
        let abrio =
            asegurar_sesion_servidor(&cliente, falsa.path(), &correo, region::webserver, |_| {
                *renovaciones.borrow_mut() += 1;
                std::fs::write(falsa.path().join("tokens.conf"), linea(&bueno.token))?;
                Ok(())
            })
            .unwrap();
        assert!(abrio);
        assert_eq!(*renovaciones.borrow(), 1);
        assert_eq!(cliente.sesion_guardada().unwrap().unwrap().correo, correo);
        cliente.cerrar_sesion().unwrap();
    }
}
