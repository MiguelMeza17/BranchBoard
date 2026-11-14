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

    /// `GET <url_api><ruta>`. `ruta` empieza con `/` y ya va codificada.
    pub fn get<T: DeserializeOwned>(&self, ruta: &str) -> Resultado<T> {
        let respuesta = self
            .agente
            .get(&format!("{}{ruta}", self.url_api))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/json")
            .call();
        let cuerpo = leer_respuesta(respuesta)?;
        sanear::interpretar(&cuerpo)
    }
}

/// 2xx → cuerpo; 401 → `token_servidor_invalido`; otro código → error solo con
/// el código HTTP y el `message` saneado; sin red → mensaje claro.
fn leer_respuesta(respuesta: Result<ureq::Response, ureq::Error>) -> Resultado<String> {
    match respuesta {
        Ok(r) => r.into_string().map_err(|_| {
            Error::ServidorUnityVc("No se pudo leer la respuesta del servidor".into())
        }),
        Err(ureq::Error::Status(401, _)) => Err(Error::TokenServidorInvalido),
        Err(ureq::Error::Status(codigo, r)) => {
            let cuerpo = r.into_string().unwrap_or_default();
            Err(error_http(codigo, &cuerpo))
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
