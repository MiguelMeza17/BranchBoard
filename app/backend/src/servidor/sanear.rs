//! Saneado de respuestas de la API (obligatorio, ver "API REST del servidor de
//! Unity VC: manejo de tokens" en SEGURIDAD.md).
//!
//! Algunas respuestas (ej. `mergeto/report`) traen el token de sesión de otros
//! usuarios en `owner.password`. Por eso toda respuesta:
//! 1. se lee como JSON y se le **borran** las claves con secretos, a cualquier profundidad;
//! 2. después se convierte a un struct propio que solo tiene los campos que se usan.
//!
//! Nunca se guarda, registra ni reenvía el JSON crudo.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, Resultado};

/// Claves que se borran siempre (se comparan en minúsculas).
const CLAVES_SECRETAS: &[&str] = &["password", "token", "accesstoken", "refreshtoken", "secret"];

/// Borra, a cualquier profundidad, toda clave que pueda traer un secreto.
pub fn borrar_secretos(valor: &mut Value) {
    match valor {
        Value::Object(mapa) => {
            mapa.retain(|clave, _| !CLAVES_SECRETAS.contains(&clave.to_lowercase().as_str()));
            mapa.values_mut().for_each(borrar_secretos);
        }
        Value::Array(lista) => lista.iter_mut().for_each(borrar_secretos),
        _ => {}
    }
}

/// Cuerpo de una respuesta → struct propio, ya sin secretos.
pub fn interpretar<T: DeserializeOwned>(cuerpo: &str) -> Resultado<T> {
    let mut valor: Value = serde_json::from_str(cuerpo).map_err(|_| respuesta_rara())?;
    borrar_secretos(&mut valor);
    serde_json::from_value(valor).map_err(|_| respuesta_rara())
}

/// Lo único que se toma del cuerpo de un error: `message` (o `status`), saneado y corto.
pub fn mensaje_de_error(cuerpo: &str) -> Option<String> {
    let mut valor: Value = serde_json::from_str(cuerpo).ok()?;
    borrar_secretos(&mut valor);
    let texto = ["message", "Message", "status", "error"]
        .iter()
        .find_map(|c| valor.get(*c).and_then(Value::as_str))?;
    let texto: String = texto.trim().chars().take(200).collect();
    (!texto.is_empty()).then_some(texto)
}

fn respuesta_rara() -> Error {
    Error::ServidorUnityVc("El servidor respondió algo que BranchBoard no entiende".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[test]
    fn borra_el_token_de_los_autores_a_cualquier_profundidad() {
        // Forma de `mergeto/report` (probado 2026-11-12), con un token falso.
        let mut v = json!({
            "conflicts": [{
                "src": { "revInfo": { "owner": { "name": "ana", "password": "TOKENeyJfalso" } } },
                "dst": { "base": { "owner": { "Password": "TOKENeyJfalso", "name": "luis" } } }
            }],
            "accessToken": "x", "refreshToken": "y", "path": "/Assets/a.cs"
        });
        borrar_secretos(&mut v);
        let texto = v.to_string();
        assert!(!texto.contains("TOKEN"), "{texto}");
        assert!(!texto.to_lowercase().contains("password"));
        assert!(v.get("accessToken").is_none() && v.get("refreshToken").is_none());
        assert_eq!(v["conflicts"][0]["src"]["revInfo"]["owner"]["name"], "ana");
        assert_eq!(v["path"], "/Assets/a.cs");
    }

    #[test]
    fn interpreta_a_un_struct_con_solo_lo_necesario() {
        #[derive(Debug, Deserialize)]
        struct Repo {
            name: String,
        }
        let repos: Vec<Repo> =
            interpretar(r#"[{"name":"A/A","owner":{"password":"s"},"id":1},{"name":"B/B"}]"#)
                .unwrap();
        assert_eq!(
            repos.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["A/A", "B/B"]
        );
        let error = interpretar::<Vec<Repo>>("<html>no es json</html>").unwrap_err();
        assert_eq!(error.codigo(), "servidor_unity_vc");
        assert!(
            !error.to_string().contains("html"),
            "no repite el cuerpo crudo"
        );
    }

    #[test]
    fn el_mensaje_de_error_no_trae_secretos_ni_es_largo() {
        assert_eq!(
            mensaje_de_error(r#"{"message":"Branch already exists","password":"x"}"#).as_deref(),
            Some("Branch already exists")
        );
        assert_eq!(mensaje_de_error(r#"{"password":"TOKENeyJ"}"#), None);
        assert_eq!(mensaje_de_error("no es json"), None);
        let largo = format!(r#"{{"message":"{}"}}"#, "a".repeat(500));
        assert_eq!(mensaje_de_error(&largo).unwrap().len(), 200);
    }
}
