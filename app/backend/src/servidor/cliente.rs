//! Llamadas HTTP a la API con el PAT. La red la hace el proceso Rust, nunca la
//! webview (SEGURIDAD.md). El token viaja solo en `Authorization: Bearer`, por
//! HTTPS, y no aparece en ningún mensaje ni en `Debug`.

use std::fmt;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};
use crate::servidor::sanear;

const TIEMPO_MAXIMO: Duration = Duration::from_secs(30);

/// Un repositorio, como lo devuelve `GET /repos` (solo lo que se usa).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repositorio {
    /// `Proyecto/Repo`, ej. `BranchBoard-Pruebas/BranchBoard-Pruebas`.
    pub name: String,
    /// Correo del dueño del repositorio (probado 2026-11-14: el de la cuenta dueña).
    #[serde(default)]
    pub owner: String,
}

/// Una rama, como la devuelven `GET`/`POST …/branches` (solo lo que se usa).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rama {
    pub id: i64,
    /// Ruta completa, ej. `/main/dev/T-101-combate`.
    pub name: String,
    /// Changeset de la cabeza de la rama.
    #[serde(rename = "headChangeset")]
    pub head_changeset: i64,
}

#[derive(Serialize)]
struct CrearRama<'a> {
    name: &'a str,
    changeset: i64,
    comment: &'a str,
}

/// Lo que no fue 2xx: el código y el cuerpo, para decidir antes de volverlo error.
struct Fallo {
    codigo: u16,
    cuerpo: String,
}

/// Cliente de la API de una organización, con su PAT.
pub struct ClienteServidor {
    /// `https://…:7178/api/v1/organizations/<org>`
    url_api: String,
    token: String,
    agente: ureq::Agent,
}

impl fmt::Debug for ClienteServidor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClienteServidor")
            .field("url_api", &self.url_api)
            .field("token", &"<oculto>")
            .finish()
    }
}

impl ClienteServidor {
    pub fn nuevo(url_api: &str, token: &str) -> Resultado<Self> {
        if !url_api.starts_with("https://") {
            return Err(Error::Interno(
                "La API de Unity VC solo se usa por HTTPS".into(),
            ));
        }
        Ok(ClienteServidor {
            url_api: url_api.trim_end_matches('/').to_string(),
            token: token.to_string(),
            agente: ureq::AgentBuilder::new().timeout(TIEMPO_MAXIMO).build(),
        })
    }

    /// Repositorios de la organización a los que tiene acceso el dueño del PAT.
    /// También sirve para comprobar que el PAT funciona.
    pub fn listar_repositorios(&self) -> Resultado<Vec<Repositorio>> {
        self.get("/repos")
    }

    /// La rama `ruta` (ej. `/main/dev`) del repositorio `repo` (`Proyecto/Repo`),
    /// o `None` si no existe.
    pub fn rama(&self, repo: &str, ruta: &str) -> Resultado<Option<Rama>> {
        let url = format!("/repos/{}/branches/{}", codificar(repo), codificar(ruta));
        match leer_respuesta(self.preparar("GET", &url).call())? {
            Ok(cuerpo) => sanear::interpretar(&cuerpo).map(Some),
            Err(f) if rama_no_existe(&f) => Ok(None),
            Err(f) => Err(error_http(f.codigo, &f.cuerpo)),
        }
    }

    /// Crea la rama `ruta` (completa, ej. `/main/dev/T-101-combate`) a partir del
    /// changeset `changeset` de su rama padre. La padre sale de la ruta.
    pub fn crear_rama(
        &self,
        repo: &str,
        ruta: &str,
        changeset: i64,
        comentario: &str,
    ) -> Resultado<Rama> {
        let url = format!("/repos/{}/branches", codificar(repo));
        let cuerpo = CrearRama {
            name: ruta,
            changeset,
            comment: comentario,
        };
        let respuesta = self.preparar("POST", &url).send_json(&cuerpo);
        let texto = leer_respuesta(respuesta)?.map_err(|f| error_http(f.codigo, &f.cuerpo))?;
        sanear::interpretar(&texto)
    }

    /// `GET <url_api><ruta>`. `ruta` empieza con `/` y ya va codificada.
    pub fn get<T: DeserializeOwned>(&self, ruta: &str) -> Resultado<T> {
        let texto = leer_respuesta(self.preparar("GET", ruta).call())?
            .map_err(|f| error_http(f.codigo, &f.cuerpo))?;
        sanear::interpretar(&texto)
    }

    fn preparar(&self, metodo: &str, ruta: &str) -> ureq::Request {
        self.agente
            .request(metodo, &format!("{}{ruta}", self.url_api))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/json")
    }
}

/// Codifica un segmento de la URL: `Proyecto/Repo` → `Proyecto%2FRepo`.
fn codificar(texto: &str) -> String {
    texto
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// La API responde 404, o 400 con `BR_NOT_FOUND`, si la rama no existe.
fn rama_no_existe(f: &Fallo) -> bool {
    f.codigo == 404 || sanear::clave_de_error(&f.cuerpo).as_deref() == Some("BR_NOT_FOUND")
}

/// 2xx → `Ok(Ok(cuerpo))`; 401 → `token_servidor_invalido`; otro código →
/// `Ok(Err(Fallo))` para que quien llama decida; sin red → mensaje claro.
fn leer_respuesta(
    respuesta: Result<ureq::Response, ureq::Error>,
) -> Resultado<Result<String, Fallo>> {
    match respuesta {
        Ok(r) => r.into_string().map(Ok).map_err(|_| {
            Error::ServidorUnityVc("No se pudo leer la respuesta del servidor".into())
        }),
        Err(ureq::Error::Status(401, _)) => Err(Error::TokenServidorInvalido),
        Err(ureq::Error::Status(codigo, r)) => {
            let cuerpo = r.into_string().unwrap_or_default();
            Ok(Err(Fallo { codigo, cuerpo }))
        }
        Err(ureq::Error::Transport(_)) => Err(Error::ServidorUnityVc(
            "No se pudo conectar con el servidor de Unity VC. Revisa tu conexión".into(),
        )),
    }
}

fn error_http(codigo: u16, cuerpo: &str) -> Error {
    Error::ServidorUnityVc(match sanear::mensaje_de_error(cuerpo) {
        Some(m) => format!("HTTP {codigo}: {m}"),
        None => format!("HTTP {codigo}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_no_muestra_el_token() {
        let c = ClienteServidor::nuevo("https://x.com:7178/api/v1/organizations/o", "eyJsecreto")
            .unwrap();
        let texto = format!("{c:?}");
        assert!(!texto.contains("eyJsecreto"), "{texto}");
        assert!(texto.contains("<oculto>"));
    }

    #[test]
    fn codifica_repos_y_ramas_para_la_url() {
        assert_eq!(
            codificar("BranchBoard-Pruebas/BranchBoard-Pruebas"),
            "BranchBoard-Pruebas%2FBranchBoard-Pruebas"
        );
        assert_eq!(
            codificar("/main/dev/T-101-combate"),
            "%2Fmain%2Fdev%2FT-101-combate"
        );
        assert_eq!(codificar("a b?#ñ"), "a%20b%3F%23%C3%B1");
    }

    #[test]
    fn reconoce_rama_inexistente() {
        let br = Fallo {
            codigo: 400,
            cuerpo: r#"{"error":{"message":"x"},"keyMessage":"BR_NOT_FOUND"}"#.into(),
        };
        assert!(rama_no_existe(&br));
        assert!(rama_no_existe(&Fallo {
            codigo: 404,
            cuerpo: String::new()
        }));
        assert!(!rama_no_existe(&Fallo {
            codigo: 400,
            cuerpo: r#"{"keyMessage":"OTRO"}"#.into()
        }));
    }

    #[test]
    fn la_rama_se_lee_de_la_respuesta_real() {
        // Forma de `GET …/branches/%2Fmain%2Fdev` (probado 2026-11-14, owner quitado).
        let r: Rama = sanear::interpretar(r#"{"id":12,"name":"/main/dev","repositoryId":"12713155","headChangeset":8,"type":"branch","isMain":false}"#).unwrap();
        assert_eq!(
            r,
            Rama {
                id: 12,
                name: "/main/dev".into(),
                head_changeset: 8
            }
        );
    }

    #[test]
    fn solo_https() {
        let error = ClienteServidor::nuevo("http://x.com", "t").unwrap_err();
        assert_eq!(error.codigo(), "interno");
    }

    #[test]
    fn el_error_http_solo_lleva_codigo_y_mensaje_saneado() {
        let e = error_http(
            409,
            r#"{"message":"Ya existe","owner":{"password":"TOKENeyJ"}}"#,
        );
        assert_eq!(e.codigo(), "servidor_unity_vc");
        assert_eq!(
            e.to_string(),
            "El servidor de Unity VC respondió con un error: HTTP 409: Ya existe"
        );
        let sin_json = error_http(500, "<html>TOKENeyJ</html>");
        assert!(!sin_json.to_string().contains("TOKEN"));
    }
}
