//! Inicio de sesión de Unity VC **por el navegador** (Unity ID), sin abrir el asistente
//! `plastic --configure` de Plastic SCM.
//!
//! Es el mismo flujo que usa el cliente oficial:
//! 1. Se abre en el navegador `www.plasticscm.com/api/oauth/unityid/authorize/session/<id>`, que lleva
//!    al inicio de sesión de Unity.
//! 2. Se consulta `www.plasticscm.com/api/oauth/token/session/<id>` hasta que devuelve
//!    `user`, `accessToken` y `authAccessToken`.
//! 3. Con `authAccessToken` se piden a `services.unity.com` las organizaciones de la cuenta.
//! 4. La sesión se guarda **donde la guarda el cliente oficial** (`tokens.conf`, `client.conf`,
//!    `profiles.conf` y `unityorgs.conf` en `%LocalAppData%\plastic4`). Sin el perfil SSO de
//!    `profiles.conf`, `cm` no autentica contra `org@unity` (lo que usa BranchBoard) aunque `org@cloud`
//!    sí funcione: iniciar sesión en BranchBoard la deja iniciada
//!    en Plastic SCM / Unity VC, y una sesión iniciada allí la ve BranchBoard (`cm whoami`).
//!
//! El token se escribe directo en los archivos (no por argumentos de `cm configure`, que lo dejarían
//! a la vista en la lista de procesos) y nunca se registra en logs.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::error::{Error, Resultado};
use crate::instalacion;

const SITIO_PLASTIC: &str = "https://www.plasticscm.com";
const SERVICIOS_UNITY: &str = "https://services.unity.com";
const ESPERA_ENTRE_CONSULTAS: Duration = Duration::from_secs(2);
const TIEMPO_POR_PETICION: Duration = Duration::from_secs(20);

/// Sesión obtenida en el navegador, lista para guardarse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SesionSso {
    pub email: String,
    pub token: String,
    pub organizaciones: Vec<Organizacion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organizacion {
    /// Nombre (slug) de la organización, p. ej. `MiguelM17`.
    pub nombre: String,
    /// Id "genesis" de la organización, p. ej. `4673426565751`.
    pub genesis_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RespuestaToken {
    user: Option<String>,
    access_token: Option<String>,
    auth_access_token: Option<String>,
}

#[derive(Deserialize)]
struct RespuestaIntercambio {
    token: String,
}

#[derive(Deserialize)]
struct RespuestaOrganizaciones {
    #[serde(default)]
    organizations: Vec<OrganizacionUnity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrganizacionUnity {
    name: String,
    genesis_id: serde_json::Value,
}

/// Abre el navegador, espera a que la persona inicie sesión (hasta `tiempo_maximo`) y guarda la sesión
/// en `carpeta_plastic4`. Devuelve el correo de la cuenta.
pub fn iniciar_y_guardar(carpeta_plastic4: &Path, tiempo_maximo: Duration) -> Resultado<String> {
    let sesion = iniciar_por_navegador(tiempo_maximo)?;
    guardar_sesion(carpeta_plastic4, &sesion)?;
    Ok(sesion.email)
}

fn iniciar_por_navegador(tiempo_maximo: Duration) -> Resultado<SesionSso> {
    let id = nuevo_id_sesion();
    instalacion::abrir_url(&format!(
        "{SITIO_PLASTIC}/api/oauth/unityid/authorize/session/{id}"
    ))?;

    let agente = ureq::AgentBuilder::new()
        .timeout(TIEMPO_POR_PETICION)
        .build();
    let url_token = format!("{SITIO_PLASTIC}/api/oauth/token/session/{id}");
    let limite = Instant::now() + tiempo_maximo;
    let (email, token, token_unity) = loop {
        // Mientras no se termina en el navegador responde `{"error": …}` (sesión aún sin token).
        if let Ok(r) = agente.get(&url_token).call() {
            if let Ok(t) = r.into_json::<RespuestaToken>() {
                if let (Some(u), Some(a), Some(b)) = (t.user, t.access_token, t.auth_access_token) {
                    break (u.trim().to_lowercase(), a, b);
                }
            }
        }
        if Instant::now() >= limite {
            return Err(Error::Validacion(
                "No se completó el inicio de sesión de Unity en el navegador a tiempo. Vuelve a intentarlo.".into(),
            ));
        }
        std::thread::sleep(ESPERA_ENTRE_CONSULTAS);
    };

    let organizaciones = organizaciones_de_la_cuenta(&agente, &token_unity)?;
    if organizaciones.is_empty() {
        return Err(Error::ServidorUnityVc(
            "Tu cuenta de Unity no pertenece a ninguna organización de Unity Version Control."
                .into(),
        ));
    }
    Ok(SesionSso {
        email,
        token,
        organizaciones,
    })
}

fn organizaciones_de_la_cuenta(
    agente: &ureq::Agent,
    token_unity: &str,
) -> Resultado<Vec<Organizacion>> {
    let error = |paso: &str| {
        Error::ServidorUnityVc(format!(
            "No se pudieron leer tus organizaciones de Unity ({paso})."
        ))
    };
    let servicio = agente
        .post(&format!(
            "{SERVICIOS_UNITY}/api/auth/v1/genesis-token-exchange/unity"
        ))
        .send_json(serde_json::json!({ "token": token_unity }))
        .map_err(|_| error("intercambio"))?
        .into_json::<RespuestaIntercambio>()
        .map_err(|_| error("intercambio"))?
        .token;
    let respuesta = agente
        .get(&format!(
            "{SERVICIOS_UNITY}/api/unity/legacy/v1/users/me/organizations"
        ))
        .set("Authorization", &format!("Bearer {servicio}"))
        .call()
        .map_err(|_| error("lista"))?
        .into_json::<RespuestaOrganizaciones>()
        .map_err(|_| error("lista"))?;
    Ok(respuesta
        .organizations
        .into_iter()
        .filter_map(|o| {
            let genesis_id = match o.genesis_id {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                _ => return None,
            };
            let valido = |s: &str| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            };
            (valido(&o.name) && valido(&genesis_id)).then_some(Organizacion {
                nombre: o.name,
                genesis_id,
            })
        })
        .collect())
}

/// Id de sesión aleatorio con forma de UUID v4.
fn nuevo_id_sesion() -> String {
    let mut b: [u8; 16] = rand::random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Escribe la sesión en los archivos del cliente oficial de Unity VC.
pub fn guardar_sesion(carpeta: &Path, sesion: &SesionSso) -> Resultado<()> {
    std::fs::create_dir_all(carpeta)
        .map_err(|e| Error::Archivos(format!("No se pudo crear {}: {e}", carpeta.display())))?;
    let leer = |nombre: &str| std::fs::read_to_string(carpeta.join(nombre)).unwrap_or_default();
    let escribir = |nombre: &str, texto: &str| {
        std::fs::write(carpeta.join(nombre), texto)
            .map_err(|e| Error::Archivos(format!("No se pudo guardar la sesión en {nombre}: {e}")))
    };
    escribir(
        "tokens.conf",
        &tokens_conf_con_sesion(&leer("tokens.conf"), sesion),
    )?;
    escribir(
        "unityorgs.conf",
        &unityorgs_conf_con(&leer("unityorgs.conf"), &sesion.organizaciones),
    )?;
    escribir(
        "client.conf",
        &client_conf_con_sesion(&leer("client.conf"), sesion),
    )?;
    let perfiles = leer("profiles.conf");
    escribir(
        "profiles.conf",
        &profiles_conf_con_sesion(perfiles.trim_start_matches('\u{feff}'), sesion),
    )?;
    Ok(())
}

/// Servidores `org@cloud` de la sesión: por id genesis (como el asistente oficial) y por nombre.
fn servidores(sesion: &SesionSso) -> Vec<String> {
    sesion
        .organizaciones
        .iter()
        .flat_map(|o| {
            [
                format!("{}@cloud", o.genesis_id),
                format!("{}@cloud", o.nombre),
            ]
        })
        .collect()
}

/// `tokens.conf` con una línea `server=… user=… token=… seiddata=…` por servidor de la sesión.
/// Quita las líneas viejas de esos servidores y la de `*@cloud` (no sirve para operar en remoto).
pub fn tokens_conf_con_sesion(actual: &str, sesion: &SesionSso) -> String {
    let nuevos = servidores(sesion);
    let servidor_de = |l: &str| {
        l.split_whitespace()
            .find_map(|p| p.strip_prefix("server="))
            .map(str::to_string)
    };
    let mut lineas: Vec<String> = actual
        .lines()
        .filter(|l| match servidor_de(l) {
            Some(s) => s != "*@cloud" && !nuevos.iter().any(|n| n.eq_ignore_ascii_case(&s)),
            None => !l.trim().is_empty(),
        })
        .map(str::to_string)
        .collect();
    if !lineas.iter().any(|l| l.starts_with("//")) {
        lineas.insert(
            0,
            "// This file is automatically generated by Unity VCS to store tokens".into(),
        );
    }
    for s in nuevos {
        lineas.push(format!(
            "server={s} user={0} token={1} seiddata={0}",
            sesion.email, sesion.token
        ));
    }
    lineas.join("\n") + "\n"
}

/// `profiles.conf` con un perfil SSO (correo de la cuenta) por servidor de la sesión. Reemplaza los
/// perfiles viejos de esos servidores y deja los demás.
pub fn profiles_conf_con_sesion(actual: &str, sesion: &SesionSso) -> String {
    let nuevos = servidores(sesion);
    let xml = if actual.contains("<Profiles>") {
        actual.to_string()
    } else {
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ServerProfileData xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\">\n  <Profiles>\n  </Profiles>\n</ServerProfileData>".to_string()
    };

    // Quitar los perfiles de los servidores que se van a escribir (con su sangría y salto de línea).
    let mut limpio = String::with_capacity(xml.len());
    let mut resto = xml.as_str();
    while let Some(ini) = resto.find("<ServerProfile>") {
        let Some(rel_fin) = resto[ini..].find("</ServerProfile>") else {
            break;
        };
        let fin = ini + rel_fin + "</ServerProfile>".len();
        let bloque = &resto[ini..fin];
        let servidor = valor_etiqueta(bloque, "Server").unwrap_or_default();
        let quitar = nuevos.iter().any(|n| n.eq_ignore_ascii_case(&servidor));
        if quitar {
            limpio.push_str(resto[..ini].trim_end_matches([' ', '\t']));
            resto = &resto[fin..];
            resto = resto
                .strip_prefix("\r\n")
                .or_else(|| resto.strip_prefix('\n'))
                .unwrap_or(resto);
        } else {
            limpio.push_str(&resto[..fin]);
            resto = &resto[fin..];
        }
    }
    limpio.push_str(resto);

    let perfiles: String = nuevos
        .iter()
        .map(|s| {
            format!(
                "    <ServerProfile>\n      <Name>{s}_SSOWorkingMode_{}</Name>\n      <Server>{s}</Server>\n      <WorkingMode>SSOWorkingMode</WorkingMode>\n      <SecurityConfig>{}</SecurityConfig>\n      <IsDefault>false</IsDefault>\n    </ServerProfile>\n",
                nuevo_id_sesion(),
                escapar_xml(&sesion.email)
            )
        })
        .collect();
    match limpio.find("</Profiles>") {
        Some(p) => {
            let inicio_linea = limpio[..p].rfind('\n').map(|i| i + 1).unwrap_or(p);
            format!(
                "{}{perfiles}{}",
                &limpio[..inicio_linea],
                &limpio[inicio_linea..]
            )
        }
        None => limpio,
    }
}

/// `unityorgs.conf` con la equivalencia `genesis:nombre` de cada organización.
pub fn unityorgs_conf_con(actual: &str, orgs: &[Organizacion]) -> String {
    let mut lineas: Vec<String> = actual
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    if lineas.is_empty() {
        lineas.push("// This file is automatically generated by plastic to store the mapping between organization slugs and genesis organization ids. Do not edit it manually.".into());
        lineas.push("// The format is gensis-organization-id:slug".into());
    }
    for o in orgs {
        let linea = format!("{}:{}", o.genesis_id, o.nombre);
        lineas.retain(|l| !l.starts_with(&format!("{}:", o.genesis_id)));
        lineas.push(linea);
    }
    lineas.join("\n") + "\n"
}

/// `client.conf` en modo SSO con el correo de la cuenta. Si el servidor por defecto no es de una de
/// sus organizaciones (o es `*@cloud`), pasa a la primera.
pub fn client_conf_con_sesion(actual: &str, sesion: &SesionSso) -> String {
    let mut xml = if actual.contains("<ClientConfigData") {
        actual.to_string()
    } else {
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ClientConfigData xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n</ClientConfigData>".to_string()
    };
    let servidores = servidores(sesion);
    let servidor_actual = valor_etiqueta(&xml, "WorkspaceServer").unwrap_or_default();
    if !servidores
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&servidor_actual))
    {
        let primera = &sesion.organizaciones[0].nombre;
        xml = fijar_etiqueta(&xml, "WorkspaceServer", &format!("{primera}@cloud"));
    }
    xml = fijar_etiqueta(&xml, "WorkingMode", "SSOWorkingMode");
    fijar_etiqueta(&xml, "SecurityConfig", &escapar_xml(&sesion.email))
}

fn valor_etiqueta(xml: &str, etiqueta: &str) -> Option<String> {
    let ini = format!("<{etiqueta}>");
    let fin = format!("</{etiqueta}>");
    let a = xml.find(&ini)? + ini.len();
    let b = a + xml[a..].find(&fin)?;
    Some(xml[a..b].to_string())
}

fn fijar_etiqueta(xml: &str, etiqueta: &str, valor: &str) -> String {
    let ini = format!("<{etiqueta}>");
    let fin = format!("</{etiqueta}>");
    if let Some(p) = xml.find(&ini) {
        let a = p + ini.len();
        if let Some(rel) = xml[a..].find(&fin) {
            return format!("{}{}{}", &xml[..a], valor, &xml[a + rel..]);
        }
    }
    if let Some(p) = xml
        .find(&format!("<{etiqueta} />"))
        .or_else(|| xml.find(&format!("<{etiqueta}/>")))
    {
        let largo = xml[p..].find('>').map(|i| i + 1).unwrap_or(0);
        return format!("{}{ini}{valor}{fin}{}", &xml[..p], &xml[p + largo..]);
    }
    match xml.find("</ClientConfigData>") {
        Some(p) => format!("{}  {ini}{valor}{fin}\n{}", &xml[..p], &xml[p..]),
        None => xml.to_string(),
    }
}

fn escapar_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sesion() -> SesionSso {
        SesionSso {
            email: "ana@correo.com".into(),
            token: "TOK".into(),
            organizaciones: vec![Organizacion {
                nombre: "MiOrg".into(),
                genesis_id: "123".into(),
            }],
        }
    }

    #[test]
    fn tokens_conf_reemplaza_lineas_de_la_org_y_quita_comodin() {
        let antes = "// cabecera\nserver=*@cloud user=x token=a seiddata=x\nserver=123@cloud user=viejo token=b seiddata=viejo\nserver=otra@cloud user=y token=c seiddata=y\n";
        let despues = tokens_conf_con_sesion(antes, &sesion());
        assert!(despues.starts_with("// cabecera\n"));
        assert!(!despues.contains("*@cloud"));
        assert!(!despues.contains("viejo"));
        assert!(despues.contains("server=otra@cloud user=y token=c seiddata=y"));
        assert!(despues
            .contains("server=123@cloud user=ana@correo.com token=TOK seiddata=ana@correo.com"));
        assert!(despues
            .contains("server=MiOrg@cloud user=ana@correo.com token=TOK seiddata=ana@correo.com"));
    }

    #[test]
    fn tokens_conf_vacio_lleva_cabecera() {
        let t = tokens_conf_con_sesion("", &sesion());
        assert!(t.starts_with("// This file is automatically generated by Unity VCS"));
    }

    #[test]
    fn client_conf_pasa_a_sso_con_el_correo_y_cambia_el_comodin() {
        let antes = "<ClientConfigData>\n  <WorkspaceServer>*@cloud</WorkspaceServer>\n  <WorkingMode>SSOWorkingMode</WorkingMode>\n  <SecurityConfig></SecurityConfig>\n  <MergeTools>x</MergeTools>\n</ClientConfigData>";
        let despues = client_conf_con_sesion(antes, &sesion());
        assert!(despues.contains("<WorkspaceServer>MiOrg@cloud</WorkspaceServer>"));
        assert!(despues.contains("<SecurityConfig>ana@correo.com</SecurityConfig>"));
        assert!(despues.contains("<MergeTools>x</MergeTools>"));
    }

    #[test]
    fn client_conf_respeta_un_servidor_de_la_cuenta() {
        let antes = "<ClientConfigData>\n  <WorkspaceServer>123@cloud</WorkspaceServer>\n</ClientConfigData>";
        let despues = client_conf_con_sesion(antes, &sesion());
        assert!(despues.contains("<WorkspaceServer>123@cloud</WorkspaceServer>"));
        assert!(despues.contains("<WorkingMode>SSOWorkingMode</WorkingMode>"));
        assert!(despues.contains("<SecurityConfig>ana@correo.com</SecurityConfig>"));
    }

    #[test]
    fn profiles_conf_reemplaza_los_de_la_org_y_deja_los_demas() {
        let antes = "<ServerProfileData>\n  <Profiles>\n    <ServerProfile>\n      <Name>local_x</Name>\n      <Server>local</Server>\n      <SecurityConfig>otro@correo.com</SecurityConfig>\n    </ServerProfile>\n    <ServerProfile>\n      <Name>MiOrg@cloud_viejo</Name>\n      <Server>MiOrg@cloud</Server>\n      <SecurityConfig></SecurityConfig>\n    </ServerProfile>\n  </Profiles>\n</ServerProfileData>";
        let despues = profiles_conf_con_sesion(antes, &sesion());
        assert!(despues.contains("<Server>local</Server>"));
        assert!(!despues.contains("MiOrg@cloud_viejo"));
        assert_eq!(despues.matches("<Server>MiOrg@cloud</Server>").count(), 1);
        assert_eq!(despues.matches("<Server>123@cloud</Server>").count(), 1);
        assert_eq!(
            despues
                .matches("<SecurityConfig>ana@correo.com</SecurityConfig>")
                .count(),
            2
        );
        assert!(despues.ends_with("  </Profiles>\n</ServerProfileData>"));
    }

    #[test]
    fn profiles_conf_vacio_se_crea() {
        let p = profiles_conf_con_sesion("", &sesion());
        assert!(p.contains("<ServerProfileData"));
        assert!(p.contains("<WorkingMode>SSOWorkingMode</WorkingMode>"));
    }

    #[test]
    fn unityorgs_agrega_la_equivalencia_sin_duplicar() {
        let antes = "// cabecera\n123:Vieja\n9:otra\n";
        let despues = unityorgs_conf_con(antes, &sesion().organizaciones);
        assert!(despues.contains("123:MiOrg"));
        assert!(!despues.contains("123:Vieja"));
        assert!(despues.contains("9:otra"));
    }

    #[test]
    fn id_de_sesion_tiene_forma_de_uuid_v4() {
        let id = nuevo_id_sesion();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }
}
