//! Token de acceso personal (PAT) de Unity VC para la API.
//!
//! - Se crea con la sesión SSO que `cm` ya tiene (`cm accesstoken create` +
//!   `reveal`), sin pedirle nada a la persona. BranchBoard nunca corre
//!   `cm configure` ni toca `tokens.conf` (ver #159).
//! - Se guarda en el llavero del sistema (Windows Credential Manager), uno por
//!   correo + organización, junto con su `id` para poder revocarlo. Nunca en la
//!   SQLite, en `ops/` ni en archivos.
//! - Si la API responde 401, se borra, se revoca y se crea otro, una sola vez.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::cm::Comando;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::modelo::normalizar_email;
use crate::servidor::{nombre_organizacion, region, ClienteServidor};

/// Nombre del servicio en el llavero del sistema.
const SERVICIO_LLAVERO: &str = "BranchBoard";

/// Un PAT: su `id` (se puede mostrar y guardar) y el secreto (nunca se muestra).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pat {
    pub id: String,
    token: String,
}

impl fmt::Debug for Pat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pat")
            .field("id", &self.id)
            .field("token", &"<oculto>")
            .finish()
    }
}

impl Pat {
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Dónde se guardan los PAT. En la app es el llavero del sistema; en las pruebas, memoria.
pub trait Llavero {
    fn leer(&self, cuenta: &str) -> Resultado<Option<String>>;
    fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()>;
    fn borrar(&self, cuenta: &str) -> Resultado<()>;
}

/// Llavero del sistema operativo (crate `keyring`).
#[derive(Debug, Default, Clone, Copy)]
pub struct LlaveroSistema;

impl LlaveroSistema {
    fn entrada(cuenta: &str) -> Resultado<keyring::Entry> {
        keyring::Entry::new(SERVICIO_LLAVERO, cuenta).map_err(error_llavero)
    }
}

impl Llavero for LlaveroSistema {
    fn leer(&self, cuenta: &str) -> Resultado<Option<String>> {
        match Self::entrada(cuenta)?.get_password() {
            Ok(valor) => Ok(Some(valor)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(error_llavero(e)),
        }
    }

    fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()> {
        Self::entrada(cuenta)?
            .set_password(valor)
            .map_err(error_llavero)
    }

    fn borrar(&self, cuenta: &str) -> Resultado<()> {
        match Self::entrada(cuenta)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(error_llavero(e)),
        }
    }
}

/// El error del llavero nunca lleva el valor guardado.
fn error_llavero(e: keyring::Error) -> Error {
    let tipo = match e {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            "no se pudo usar el llavero del sistema"
        }
        _ => "el llavero del sistema rechazó la operación",
    };
    Error::Interno(format!("Token de Unity VC: {tipo}"))
}

/// Clave del PAT en el llavero: correo + organización.
pub fn cuenta_llavero(email: &str, org: &str) -> String {
    format!(
        "{}|{}",
        normalizar_email(email),
        nombre_organizacion(org).to_lowercase()
    )
}

/// Cliente de la API listo para usar, para `email` (el de `cm whoami`) en `org`.
/// Usa el PAT del llavero; si no hay o ya no sirve, crea uno nuevo.
pub fn conectar(
    estado: &AppState,
    llavero: &dyn Llavero,
    email: &str,
    org: &str,
) -> Resultado<ClienteServidor> {
    let url_api = region::url_api(org)?;
    let org_cm = format!("{}@unity", nombre_organizacion(org));
    let pat = asegurar_pat(
        llavero,
        &cuenta_llavero(email, org),
        || crear_pat(estado, &org_cm),
        |pat| {
            ClienteServidor::nuevo(&url_api, pat.token())?
                .listar_repositorios()
                .map(|_| ())
        },
        |id| revocar_pat(estado, &org_cm, id),
    )?;
    ClienteServidor::nuevo(&url_api, pat.token())
}

/// El orden de [`conectar`], sin `cm` ni red, para poder probarlo:
/// 1. PAT del llavero que funciona → ese.
/// 2. Si responde 401 (vencido o revocado) → se borra, se revoca y se sigue.
/// 3. Se crea uno, se guarda y se prueba. Si el nuevo también da 401, se borra y es error.
fn asegurar_pat(
    llavero: &dyn Llavero,
    cuenta: &str,
    crear: impl Fn() -> Resultado<Pat>,
    probar: impl Fn(&Pat) -> Resultado<()>,
    revocar: impl Fn(&str),
) -> Resultado<Pat> {
    if let Some(guardado) = llavero.leer(cuenta)? {
        match serde_json::from_str::<Pat>(&guardado) {
            Ok(pat) => match probar(&pat) {
                Ok(()) => return Ok(pat),
                Err(Error::TokenServidorInvalido) => {
                    llavero.borrar(cuenta)?;
                    revocar(&pat.id);
                }
                Err(e) => return Err(e),
            },
            // Guardado por otra versión o dañado: se reemplaza.
            Err(_) => llavero.borrar(cuenta)?,
        }
    }

    let nuevo = crear()?;
    let json = serde_json::to_string(&nuevo).map_err(|e| Error::Interno(e.to_string()))?;
    llavero.guardar(cuenta, &json)?;
    match probar(&nuevo) {
        Ok(()) => Ok(nuevo),
        Err(e) => {
            if matches!(e, Error::TokenServidorInvalido) {
                llavero.borrar(cuenta)?;
                revocar(&nuevo.id);
            }
            Err(e)
        }
    }
}

/// `cm accesstoken create "BranchBoard (<máquina>)" <org>@unity --format={id}` + `reveal`.
fn crear_pat(estado: &AppState, org_cm: &str) -> Resultado<Pat> {
    let salida = Comando::cm()
        .args(["accesstoken", "create"])
        .arg(descripcion_pat())
        .arg(org_cm)
        .arg("--format={id}")
        .ejecutar(estado)?;
    let id = interpretar_id(&salida.stdout).filter(|_| salida.exito()).ok_or_else(|| {
        Error::Cm(
            "No se pudo crear el token de acceso de Unity VC. Si no eres el dueño de la organización, puede que su Administrador tenga que permitirlo (cm accesstoken admin allowlist)".into(),
        )
    })?;

    let revelado = Comando::cm()
        .args(["accesstoken", "reveal"])
        .arg(&id)
        .arg(org_cm)
        .ejecutar(estado)?;
    let token = interpretar_token(&revelado.stdout).filter(|_| revelado.exito());
    match token {
        Some(token) => Ok(Pat { id, token }),
        None => {
            revocar_pat(estado, org_cm, &id);
            Err(Error::Cm(
                "No se pudo leer el token de acceso de Unity VC".into(),
            ))
        }
    }
}

/// Revoca sin fallar: si no se puede, el PAT igual queda fuera del llavero.
fn revocar_pat(estado: &AppState, org_cm: &str, id: &str) {
    let _ = Comando::cm()
        .args(["accesstoken", "revoke"])
        .arg(id)
        .arg(org_cm)
        .ejecutar(estado);
}

fn descripcion_pat() -> String {
    let maquina = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "este equipo".into());
    format!("BranchBoard ({maquina})")
}

/// Última línea no vacía con forma de id (`8-4-4-4-12` en hexadecimal).
fn interpretar_id(stdout: &str) -> Option<String> {
    let linea = stdout.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let partes: Vec<&str> = linea.split('-').collect();
    let largos_ok = partes.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12]);
    let hex_ok = partes
        .iter()
        .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()));
    (largos_ok && hex_ok).then(|| linea.to_string())
}

/// El token revelado: una sola palabra, sin espacios.
fn interpretar_token(stdout: &str) -> Option<String> {
    let linea = stdout.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    (!linea.contains(char::is_whitespace) && linea.len() >= 20).then(|| linea.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct LlaveroMemoria(RefCell<HashMap<String, String>>);

    impl Llavero for LlaveroMemoria {
        fn leer(&self, c: &str) -> Resultado<Option<String>> {
            Ok(self.0.borrow().get(c).cloned())
        }
        fn guardar(&self, c: &str, v: &str) -> Resultado<()> {
            self.0.borrow_mut().insert(c.into(), v.into());
            Ok(())
        }
        fn borrar(&self, c: &str) -> Resultado<()> {
            self.0.borrow_mut().remove(c);
            Ok(())
        }
    }

    fn pat(id: &str, token: &str) -> Pat {
        Pat {
            id: id.into(),
            token: token.into(),
        }
    }

    const CUENTA: &str = "ana@x.dev|org";

    #[test]
    fn sin_pat_guardado_crea_uno_y_lo_guarda() {
        let llavero = LlaveroMemoria::default();
        let creados = RefCell::new(0);
        let p = asegurar_pat(
            &llavero,
            CUENTA,
            || {
                *creados.borrow_mut() += 1;
                Ok(pat("id1", "bueno"))
            },
            |_| Ok(()),
            |_| panic!("no revoca"),
        )
        .unwrap();
        assert_eq!(p.id, "id1");
        assert_eq!(*creados.borrow(), 1);
        assert!(llavero.leer(CUENTA).unwrap().unwrap().contains("id1"));
    }

    #[test]
    fn el_pat_guardado_que_funciona_se_reusa() {
        let llavero = LlaveroMemoria::default();
        llavero
            .guardar(
                CUENTA,
                &serde_json::to_string(&pat("id1", "bueno")).unwrap(),
            )
            .unwrap();
        let p = asegurar_pat(
            &llavero,
            CUENTA,
            || panic!("no crea"),
            |_| Ok(()),
            |_| panic!(),
        )
        .unwrap();
        assert_eq!(p.token(), "bueno");
    }

    #[test]
    fn con_401_lo_revoca_y_crea_otro() {
        let llavero = LlaveroMemoria::default();
        llavero
            .guardar(
                CUENTA,
                &serde_json::to_string(&pat("viejo", "vencido")).unwrap(),
            )
            .unwrap();
        let revocados = RefCell::new(Vec::new());
        let p = asegurar_pat(
            &llavero,
            CUENTA,
            || Ok(pat("nuevo", "bueno")),
            |p| match p.token() {
                "vencido" => Err(Error::TokenServidorInvalido),
                _ => Ok(()),
            },
            |id| revocados.borrow_mut().push(id.to_string()),
        )
        .unwrap();
        assert_eq!(p.id, "nuevo");
        assert_eq!(*revocados.borrow(), ["viejo"]);
        assert!(llavero.leer(CUENTA).unwrap().unwrap().contains("nuevo"));
    }

    #[test]
    fn si_el_nuevo_tambien_da_401_no_se_queda_en_un_ciclo() {
        let llavero = LlaveroMemoria::default();
        let creados = RefCell::new(0);
        let error = asegurar_pat(
            &llavero,
            CUENTA,
            || {
                *creados.borrow_mut() += 1;
                Ok(pat("n", "malo"))
            },
            |_| Err(Error::TokenServidorInvalido),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.codigo(), "token_servidor_invalido");
        assert_eq!(*creados.borrow(), 1);
        assert_eq!(
            llavero.leer(CUENTA).unwrap(),
            None,
            "no deja el malo guardado"
        );
    }

    #[test]
    fn sin_red_no_borra_el_pat_ni_crea_otro() {
        let llavero = LlaveroMemoria::default();
        llavero
            .guardar(
                CUENTA,
                &serde_json::to_string(&pat("id1", "bueno")).unwrap(),
            )
            .unwrap();
        let error = asegurar_pat(
            &llavero,
            CUENTA,
            || panic!("no crea"),
            |_| Err(Error::ServidorUnityVc("sin red".into())),
            |_| panic!("no revoca"),
        )
        .unwrap_err();
        assert_eq!(error.codigo(), "servidor_unity_vc");
        assert!(llavero.leer(CUENTA).unwrap().is_some());
    }

    #[test]
    fn un_valor_danado_en_el_llavero_se_reemplaza() {
        let llavero = LlaveroMemoria::default();
        llavero.guardar(CUENTA, "no es json").unwrap();
        let p = asegurar_pat(&llavero, CUENTA, || Ok(pat("n", "t")), |_| Ok(()), |_| {}).unwrap();
        assert_eq!(p.id, "n");
    }

    #[test]
    fn cuenta_del_llavero_por_correo_y_organizacion() {
        assert_eq!(
            cuenta_llavero("Ana@X.dev", "MiguelM17@unity"),
            "ana@x.dev|miguelm17"
        );
    }

    #[test]
    fn debug_del_pat_no_muestra_el_secreto() {
        let texto = format!("{:?}", pat("id1", "eyJsecreto"));
        assert!(texto.contains("id1") && !texto.contains("eyJsecreto"));
    }

    #[test]
    fn interpreta_la_salida_de_cm() {
        let id = "696c9c97-1234-4abc-9def-0123456789ab";
        assert_eq!(interpretar_id(&format!("{id}\r\n")).as_deref(), Some(id));
        assert_eq!(interpretar_id("Error: no tiene permisos"), None);
        assert_eq!(interpretar_id(""), None);
        let token = "eyJhbGciOiJIUzI1NiJ9.e30.firma";
        assert_eq!(
            interpretar_token(&format!("{token}\r\n")).as_deref(),
            Some(token)
        );
        assert_eq!(interpretar_token("Error: el token no existe"), None);
    }

    /// **Prueba en el entorno real** (CLAUDE.md): crea un PAT de verdad, lo guarda
    /// en el llavero, llama a la API, fuerza un 401 y comprueba que se renueva;
    /// al final revoca y borra todo. Necesita sesión de `cm` con acceso a la organización:
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 cargo test entorno_real -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG"]
    fn entorno_real_crear_usar_y_renovar_el_pat() {
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let org_cm = format!("{}@unity", nombre_organizacion(&org));
        let estado = AppState::default();
        let email = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let llavero = LlaveroSistema;
        let cuenta = cuenta_llavero(&email, &org);
        // Empezar sin PAT: si ya había uno (ej. de otra prueba), se revoca antes de
        // quitarlo del llavero, para no dejarlo vivo en el servidor.
        if let Some(previo) = llavero.leer(&cuenta).unwrap() {
            if let Ok(previo) = serde_json::from_str::<Pat>(&previo) {
                revocar_pat(&estado, &org_cm, &previo.id);
            }
        }
        llavero.borrar(&cuenta).unwrap();

        // 1. Sin nada guardado: crea, guarda y la API responde.
        let cliente = conectar(&estado, &llavero, &email, &org).unwrap();
        let repos = cliente.listar_repositorios().unwrap();
        println!(
            "repositorios: {:?}",
            repos.iter().map(|r| &r.name).collect::<Vec<_>>()
        );
        assert!(repos
            .iter()
            .any(|r| r.name.starts_with("BranchBoard-Pruebas")));
        let primero: Pat = serde_json::from_str(&llavero.leer(&cuenta).unwrap().unwrap()).unwrap();

        // 2. Segunda vez: reusa el mismo PAT, no crea otro.
        conectar(&estado, &llavero, &email, &org).unwrap();
        let segundo: Pat = serde_json::from_str(&llavero.leer(&cuenta).unwrap().unwrap()).unwrap();
        assert_eq!(primero.id, segundo.id, "reusó el PAT guardado");

        // 3. Token dañado en el llavero → 401 → se revoca y se crea otro.
        let danado = Pat {
            id: primero.id.clone(),
            token: format!("x{}", primero.token),
        };
        llavero
            .guardar(&cuenta, &serde_json::to_string(&danado).unwrap())
            .unwrap();
        conectar(&estado, &llavero, &email, &org)
            .unwrap()
            .listar_repositorios()
            .unwrap();
        let tercero: Pat = serde_json::from_str(&llavero.leer(&cuenta).unwrap().unwrap()).unwrap();
        assert_ne!(tercero.id, primero.id, "creó un PAT nuevo tras el 401");
        println!(
            "401 → PAT renovado (id {}… → {}…)",
            &primero.id[..8],
            &tercero.id[..8]
        );

        // Limpieza: nada queda vivo ni en el llavero.
        revocar_pat(&estado, &org_cm, &tercero.id);
        llavero.borrar(&cuenta).unwrap();
        assert_eq!(llavero.leer(&cuenta).unwrap(), None);
    }
}
