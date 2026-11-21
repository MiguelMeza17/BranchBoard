//! URL base de la API de una organización.
//!
//! Cada organización de Unity VC Cloud vive en una región (ej. US East:
//! `https://prd-azure-eastus-01-cloud.plasticscm.com:7178`). El cliente de Unity VC
//! ya la anota en `%LOCALAPPDATA%\plastic4\cloudregions.conf` la primera vez que
//! usa la organización, con líneas como:
//!
//! ```text
//! organization=MiguelM17@cloud server=…:8787 version=2 encrypted=False webserver=https://…:7178
//! ```
//!
//! El `webserver` es la base de la API (probado el 2026-11-14 contra BranchBoard-Pruebas).

use std::path::PathBuf;

use crate::error::{Error, Resultado};
use crate::servidor::nombre_organizacion;

/// `https://…:7178/api/v1/organizations/<org>` para la organización `org`
/// (acepta `org`, `org@unity` u `org@cloud`).
pub fn url_api(org: &str) -> Resultado<String> {
    Ok(armar_url_api(&webserver(org)?, org))
}

/// `https://…:7178`, el servidor de la región de `org` (lo pide la Edge Function `login-unity`, #237).
pub fn webserver(org: &str) -> Resultado<String> {
    let ruta = ruta_cloudregions().ok_or_else(sin_region)?;
    let texto = std::fs::read_to_string(&ruta).map_err(|_| sin_region())?;
    interpretar_regiones(&texto, org).ok_or_else(sin_region)
}

/// Lista las organizaciones presentes en `%LOCALAPPDATA%\plastic4\cloudregions.conf` (#124).
pub fn organizaciones_en_cloudregions() -> Vec<String> {
    let Some(ruta) = ruta_cloudregions() else {
        return Vec::new();
    };
    let Ok(texto) = std::fs::read_to_string(&ruta) else {
        return Vec::new();
    };
    interpretar_organizaciones(&texto)
}

fn interpretar_organizaciones(texto: &str) -> Vec<String> {
    let mut orgs = Vec::new();
    for linea in texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.is_empty())
    {
        for par in linea.split_whitespace() {
            if let Some(("organization", valor)) = par.split_once('=') {
                let nombre = nombre_organizacion(valor);
                if !nombre.is_empty()
                    && !orgs.iter().any(|o: &String| o.eq_ignore_ascii_case(nombre))
                {
                    orgs.push(nombre.to_string());
                }
            }
        }
    }
    orgs
}

/// Página de proyectos de la organización en el Unity Cloud Dashboard (#205):
/// con el id numérico va directo a la organización; sin él, a la portada,
/// donde la persona elige la organización.
pub fn url_proyectos_unity_cloud(id_organizacion: Option<&str>) -> String {
    match id_organizacion {
        Some(id) if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) => {
            format!("https://cloud.unity.com/home/organizations/{id}/projects")
        }
        _ => "https://cloud.unity.com/home".to_string(),
    }
}

/// Id numérico de la organización `org` según `cloudregions.conf` (#205): el
/// cliente lo anota como `organization=<id>@cloud` en las máquinas que entraron
/// por Unity ID. Si la organización está por nombre, se toma el id que está en
/// el mismo `server` solo cuando ese servidor tiene exactamente un nombre y un
/// id (el cliente anota la misma organización de las dos formas). Si no está,
/// vale el único id del archivo.
pub fn id_organizacion(org: &str) -> Option<String> {
    let ruta = ruta_cloudregions()?;
    let texto = std::fs::read_to_string(ruta).ok()?;
    interpretar_id_organizacion(&texto, org)
}

fn interpretar_id_organizacion(texto: &str, org: &str) -> Option<String> {
    let buscada = nombre_organizacion(org).to_lowercase();
    let nombres = interpretar_organizaciones(texto);
    let es_id = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    if nombres
        .iter()
        .any(|n| n.to_lowercase() == buscada && es_id(n))
    {
        return Some(buscada);
    }
    if nombres.iter().any(|n| n.to_lowercase() == buscada) {
        let lineas = interpretar_organizaciones_y_servidor(texto);
        let servidor = lineas
            .iter()
            .find(|(n, _)| n.to_lowercase() == buscada)
            .map(|(_, s)| s.clone())?;
        let mismo: Vec<&String> = lineas
            .iter()
            .filter(|(_, s)| !s.is_empty() && s.eq_ignore_ascii_case(&servidor))
            .map(|(n, _)| n)
            .collect();
        let mut ids = mismo.iter().filter(|n| es_id(n));
        let nombres_en_servidor = mismo.iter().filter(|n| !es_id(n)).count();
        return match (ids.next(), ids.next(), nombres_en_servidor) {
            (Some(unico), None, 1) => Some((*unico).clone()),
            _ => None,
        };
    }
    let mut ids = nombres.iter().filter(|n| es_id(n));
    match (ids.next(), ids.next()) {
        (Some(unico), None) => Some(unico.clone()),
        _ => None,
    }
}

/// Pares `(organización, server)` de cada línea activa de `cloudregions.conf`,
/// sin repetir organización (#205).
fn interpretar_organizaciones_y_servidor(texto: &str) -> Vec<(String, String)> {
    let mut pares: Vec<(String, String)> = Vec::new();
    for linea in texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.is_empty())
    {
        let mut org = None;
        let mut servidor = String::new();
        for par in linea.split_whitespace() {
            match par.split_once('=') {
                Some(("organization", valor)) => org = Some(nombre_organizacion(valor).to_string()),
                Some(("server", valor)) => servidor = valor.to_string(),
                _ => {}
            }
        }
        if let Some(org) = org.filter(|o| !o.is_empty()) {
            if !pares.iter().any(|(o, _)| o.eq_ignore_ascii_case(&org)) {
                pares.push((org, servidor));
            }
        }
    }
    pares
}

fn armar_url_api(base: &str, org: &str) -> String {
    format!(
        "{}/api/v1/organizations/{}",
        base.trim_end_matches('/'),
        nombre_organizacion(org)
    )
}

fn ruta_cloudregions() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".plastic4")))?;
    let en_windows = local.join("plastic4").join("cloudregions.conf");
    if en_windows.exists() {
        return Some(en_windows);
    }
    let directo = local.join("cloudregions.conf");
    directo.exists().then_some(directo)
}

fn sin_region() -> Error {
    Error::ServidorUnityVc(
        "No se encontró el servidor de la organización. Abre el proyecto una vez con Unity Version Control y vuelve a intentar".into(),
    )
}

/// El `webserver` de la línea de `org`, si existe y es `https://`.
///
/// **Corregido (2026-11-15, #107)**: en una máquina que entró a la organización
/// por Unity ID, el cliente anota la línea con el **id numérico** de la
/// organización (`organization=4673426565751@cloud …`) y no con su nombre, así
/// que por nombre no aparece. Si ninguna línea coincide y todas las que hay
/// apuntan al mismo `webserver` (una sola región), se usa ese: la API acepta el
/// nombre de la organización en la URL igual (probado con `GET …/repos`).
fn interpretar_regiones(texto: &str, org: &str) -> Option<String> {
    let buscada = nombre_organizacion(org).to_lowercase();
    // (organización, webserver si es https)
    let lineas: Vec<(String, Option<String>)> = texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.is_empty())
        .map(|linea| {
            let mut organizacion = "";
            let mut webserver = None;
            for par in linea.split_whitespace() {
                match par.split_once('=') {
                    Some(("organization", v)) => organizacion = v,
                    Some(("webserver", v)) => webserver = Some(v.trim_end_matches('/')),
                    _ => {}
                }
            }
            (
                nombre_organizacion(organizacion).to_lowercase(),
                webserver
                    .filter(|w| w.starts_with("https://"))
                    .map(str::to_string),
            )
        })
        .collect();
    // Por nombre: lo que diga su línea (aunque sea nada, si no es https).
    if let Some((_, web)) = lineas.iter().find(|(o, _)| *o == buscada) {
        return web.clone();
    }
    // Sin línea por nombre: la única región https que haya.
    let mut distintos: Vec<&str> = lineas.iter().filter_map(|(_, w)| w.as_deref()).collect();
    distintos.sort_unstable();
    distintos.dedup();
    match distintos.as_slice() {
        [unico] => Some((*unico).to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EJEMPLO: &str = "\
// This file is automatically generated by plastic
// organization=codice@cloud server=europe-cloud.plasticscm.com:7777 version=2 encrypted=true webserver=https://europe-cloud.plasticscm.com:6666
organization=MiguelM17@cloud server=prd-azure-eastus-01-cloud.plasticscm.com:8787 version=2 encrypted=False webserver=https://prd-azure-eastus-01-cloud.plasticscm.com:7178
organization=otra@cloud server=x:8787 version=2 encrypted=False webserver=http://inseguro:7178
";

    #[test]
    fn encuentra_el_webserver_de_la_organizacion() {
        let base = "https://prd-azure-eastus-01-cloud.plasticscm.com:7178";
        assert_eq!(
            interpretar_regiones(EJEMPLO, "MiguelM17@unity").as_deref(),
            Some(base)
        );
        assert_eq!(
            interpretar_regiones(EJEMPLO, "miguelm17").as_deref(),
            Some(base)
        );
    }

    #[test]
    fn ignora_comentarios_otras_organizaciones_y_http() {
        assert_eq!(
            interpretar_regiones(EJEMPLO, "codice").as_deref(),
            Some("https://prd-azure-eastus-01-cloud.plasticscm.com:7178"),
            "su línea está comentada: no cuenta por nombre, pero hay una sola región"
        );
        assert_eq!(
            interpretar_regiones(EJEMPLO, "otra"),
            None,
            "sin https no se usa"
        );
        assert_eq!(
            interpretar_regiones(EJEMPLO, "nadie").as_deref(),
            Some("https://prd-azure-eastus-01-cloud.plasticscm.com:7178"),
            "una sola región https: se usa aunque la organización no esté por nombre"
        );
        assert_eq!(interpretar_regiones("", "MiguelM17"), None);
    }

    #[test]
    fn sin_la_organizacion_por_nombre_usa_la_unica_region_o_nada() {
        // Así queda el archivo en una máquina que entró por Unity ID (2026-11-15): solo el id.
        let por_id = "organization=4673426565751@cloud server=prd-azure-eastus-01-cloud.plasticscm.com:8787 version=2 encrypted=False webserver=https://prd-azure-eastus-01-cloud.plasticscm.com:7178\n";
        assert_eq!(
            interpretar_regiones(por_id, "MiguelM17@unity").as_deref(),
            Some("https://prd-azure-eastus-01-cloud.plasticscm.com:7178")
        );
        // Dos regiones distintas: no se adivina.
        let dos = "organization=1@cloud webserver=https://a:7178\norganization=2@cloud webserver=https://b:7178\n";
        assert_eq!(interpretar_regiones(dos, "MiguelM17"), None);
        assert_eq!(
            interpretar_regiones(dos, "2").as_deref(),
            Some("https://b:7178")
        );
        // Solo http: tampoco.
        assert_eq!(
            interpretar_regiones("organization=1@cloud webserver=http://a:7178\n", "x"),
            None
        );
    }

    #[test]
    fn arma_la_url_de_la_api() {
        assert_eq!(
            armar_url_api("https://x.com:7178/", "MiguelM17@unity"),
            "https://x.com:7178/api/v1/organizations/MiguelM17"
        );
    }

    #[test]
    fn extrae_organizaciones_sin_repetir() {
        assert_eq!(
            interpretar_organizaciones(EJEMPLO),
            vec!["MiguelM17".to_string(), "otra".to_string()]
        );
    }

    #[test]
    #[ignore = "lee el cloudregions.conf de esta máquina: correr a mano con BB_PRUEBA_ORG"]
    fn entorno_real_id_de_la_organizacion_de_esta_maquina() {
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let id = id_organizacion(&org);
        println!(
            "id de {org}: {id:?} -> {}",
            url_proyectos_unity_cloud(id.as_deref())
        );
        assert!(
            url_api(&org).is_ok(),
            "la región de {org} tiene que resolverse"
        );
    }

    #[test]
    fn id_de_la_organizacion_y_url_del_dashboard() {
        let por_id = "organization=4673426565751@cloud server=x:8787 version=2 encrypted=False webserver=https://x:7178\n";
        assert_eq!(
            interpretar_id_organizacion(por_id, "MiguelM17@unity").as_deref(),
            Some("4673426565751"),
            "una sola línea numérica: es la de esa organización"
        );
        assert_eq!(
            interpretar_id_organizacion(por_id, "4673426565751").as_deref(),
            Some("4673426565751")
        );
        assert_eq!(
            interpretar_id_organizacion(EJEMPLO, "MiguelM17"),
            None,
            "está por nombre y no hay id en su servidor: no se conoce"
        );
        let nombre_e_id = "organization=MiguelM17@cloud server=prd-eastus:8787 version=2
organization=4673426565751@cloud server=prd-eastus:8787 version=2
organization=otra@cloud server=europa:8787 version=2
";
        assert_eq!(
            interpretar_id_organizacion(nombre_e_id, "MiguelM17@unity").as_deref(),
            Some("4673426565751"),
            "por nombre y por id en el mismo servidor: es la misma organización"
        );
        assert_eq!(
            interpretar_id_organizacion(nombre_e_id, "otra"),
            None,
            "el id de otro servidor no se le asigna"
        );
        let ambiguo = "organization=MiguelM17@cloud server=s:1
organization=111@cloud server=s:1
organization=222@cloud server=s:1
";
        assert_eq!(interpretar_id_organizacion(ambiguo, "MiguelM17"), None);
        let dos_nombres = "organization=MiguelM17@cloud server=s:1
organization=otra@cloud server=s:1
organization=111@cloud server=s:1
";
        assert_eq!(
            interpretar_id_organizacion(dos_nombres, "MiguelM17"),
            None,
            "dos nombres y un id en el mismo servidor: no se sabe de cuál es"
        );
        let dos =
            "organization=1@cloud webserver=https://a\norganization=2@cloud webserver=https://b\n";
        assert_eq!(interpretar_id_organizacion(dos, "otra"), None);
        assert_eq!(interpretar_id_organizacion("", "x"), None);

        assert_eq!(
            url_proyectos_unity_cloud(Some("4673426565751")),
            "https://cloud.unity.com/home/organizations/4673426565751/projects"
        );
        assert_eq!(
            url_proyectos_unity_cloud(None),
            "https://cloud.unity.com/home"
        );
        assert_eq!(
            url_proyectos_unity_cloud(Some("x/../y")),
            "https://cloud.unity.com/home",
            "solo dígitos van a la URL"
        );
    }
}
