//! Llamadas HTTP a la API con el PAT. La red la hace el proceso Rust, nunca la
//! webview (SEGURIDAD.md). El token viaja solo en `Authorization: Bearer`, por
//! HTTPS, y no aparece en ningún mensaje ni en `Debug`.

use std::fmt;
use std::io::Read;
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

/// Un check-in (changeset), como lo devuelve `GET …/changesets?branchName=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changeset {
    #[serde(rename = "changesetId")]
    pub changeset_id: i64,
    /// Quién hizo el check-in (el correo de su cuenta de Unity).
    #[serde(default)]
    pub owner: String,
    /// Fecha en UTC, ej. `2026-11-12T17:20:04Z`.
    pub date: String,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Serialize)]
struct CrearRama<'a> {
    name: &'a str,
    changeset: i64,
    comment: &'a str,
}

/// Cuerpo de `POST …/mergeto/report` y `POST …/mergeto` (probado 2026-11-15, #107).
#[derive(Serialize)]
struct CuerpoMergeTo<'a> {
    #[serde(rename = "sourceType")]
    source_type: &'static str,
    source: &'a str,
    destination: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<&'a str>,
}

/// Lo que se usa de `POST …/mergeto/report` (probado 2026-11-15, #107). Cuando
/// la rama ya está conectada al destino, `resultConflicts` no viene y
/// `resultStatus` es `MergeNotNeededAlreadyConnected`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ReporteMerge {
    #[serde(default)]
    pub result: ResultadoReporte,
    #[serde(rename = "mergeAllowed", default)]
    pub merge_allowed: bool,
    #[serde(rename = "mergeIssues", default)]
    pub merge_issues: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ResultadoReporte {
    #[serde(rename = "resultStatus", default)]
    pub result_status: String,
    #[serde(rename = "resultConflicts", default)]
    pub result_conflicts: ConflictosReporte,
}

/// Solo las listas que significan "hay que resolverlo a mano en Unity VC".
/// En `fileConflicts` la ruta va dentro de `src`/`dst`; en las demás, directo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ConflictosReporte {
    #[serde(rename = "fileConflicts", default)]
    pub file_conflicts: Vec<CambioReporte>,
    #[serde(rename = "evilTwins", default)]
    pub evil_twins: Vec<CambioReporte>,
    #[serde(rename = "changeDeleteConflicts", default)]
    pub change_delete_conflicts: Vec<CambioReporte>,
    #[serde(rename = "deleteChangeConflicts", default)]
    pub delete_change_conflicts: Vec<CambioReporte>,
    #[serde(rename = "divergentMoveConflicts", default)]
    pub divergent_move_conflicts: Vec<CambioReporte>,
    #[serde(rename = "moveDeleteConflicts", default)]
    pub move_delete_conflicts: Vec<CambioReporte>,
    #[serde(rename = "deleteMoveConflicts", default)]
    pub delete_move_conflicts: Vec<CambioReporte>,
    #[serde(rename = "addMoveConflicts", default)]
    pub add_move_conflicts: Vec<CambioReporte>,
    #[serde(rename = "moveAddConflicts", default)]
    pub move_add_conflicts: Vec<CambioReporte>,
}

/// Un cambio del reporte: su `path`, o el de su `src`/`dst` si viene anidado.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct CambioReporte {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub src: Option<Box<CambioReporte>>,
    #[serde(default)]
    pub dst: Option<Box<CambioReporte>>,
}

impl CambioReporte {
    fn ruta(&self) -> &str {
        if !self.path.is_empty() {
            return &self.path;
        }
        [&self.src, &self.dst]
            .into_iter()
            .flatten()
            .map(|c| c.ruta())
            .find(|r| !r.is_empty())
            .unwrap_or("")
    }
}

impl ReporteMerge {
    /// Rutas que habría que resolver a mano, sin repetir, en el orden del reporte.
    pub fn archivos_en_conflicto(&self) -> Vec<String> {
        let c = &self.result.result_conflicts;
        let mut rutas: Vec<String> = Vec::new();
        for cambio in [
            &c.file_conflicts,
            &c.evil_twins,
            &c.change_delete_conflicts,
            &c.delete_change_conflicts,
            &c.divergent_move_conflicts,
            &c.move_delete_conflicts,
            &c.delete_move_conflicts,
            &c.add_move_conflicts,
            &c.move_add_conflicts,
        ]
        .into_iter()
        .flatten()
        {
            let ruta = cambio.ruta();
            if !ruta.is_empty() && !rutas.iter().any(|r| r == ruta) {
                rutas.push(ruta.to_string());
            }
        }
        rutas
    }

    /// `true` si la rama ya está conectada al destino (no hay nada que mergear).
    pub fn nada_que_mergear(&self) -> bool {
        self.result.result_status.starts_with("MergeNotNeeded")
    }
}

/// Respuesta de `POST …/mergeto` (probado 2026-11-15, #107): `OK` con el
/// changeset creado en el destino, `Conflicts` (changeset 0 y un mensaje que
/// pide un "Merge from" desde un workspace) o `MergeNotNeeded` (ya estaba).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResultadoMergeTo {
    pub status: String,
    #[serde(rename = "changesetNumber", default)]
    pub changeset_number: i64,
    #[serde(default)]
    pub message: Option<String>,
}

impl ResultadoMergeTo {
    pub fn fue_ok(&self) -> bool {
        self.status == "OK"
    }

    pub fn nada_que_mergear(&self) -> bool {
        self.status == "MergeNotNeeded"
    }

    pub fn hubo_conflicto(&self) -> bool {
        self.status == "Conflicts"
    }
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

    /// Check-ins de la rama `ruta`, o `None` si la rama no existe.
    pub fn changesets_de_rama(&self, repo: &str, ruta: &str) -> Resultado<Option<Vec<Changeset>>> {
        let url = format!(
            "/repos/{}/changesets?branchName={}",
            codificar(repo),
            codificar(ruta)
        );
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

    /// Reporte del merge de la rama `origen` hacia `destino` (`POST …/mergeto/report`),
    /// sin tocar nada: dice qué archivos chocarían. Las dos rutas van completas.
    pub fn reporte_merge(
        &self,
        repo: &str,
        origen: &str,
        destino: &str,
    ) -> Resultado<ReporteMerge> {
        self.post_mergeto(repo, "/mergeto/report", origen, destino, None)
    }

    /// Merge del lado del servidor de `origen` hacia `destino` (`POST …/mergeto`).
    /// Un conflicto no es error HTTP: llega como `status: "Conflicts"`.
    pub fn merge_a(
        &self,
        repo: &str,
        origen: &str,
        destino: &str,
        comentario: &str,
    ) -> Resultado<ResultadoMergeTo> {
        self.post_mergeto(repo, "/mergeto", origen, destino, Some(comentario))
    }

    fn post_mergeto<T: DeserializeOwned>(
        &self,
        repo: &str,
        sufijo: &str,
        origen: &str,
        destino: &str,
        comentario: Option<&str>,
    ) -> Resultado<T> {
        let url = format!("/repos/{}{sufijo}", codificar(repo));
        let cuerpo = CuerpoMergeTo {
            source_type: "Branch",
            source: origen,
            destination: destino,
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

    /// Contenido versionado, nunca una ruta del disco. No se interpreta como
    /// JSON: las claves del código fuente no son metadatos de autenticación.
    /// `None` si excede el límite de memoria del visor.
    pub fn revision_bytes(&self, repo: &str, revision: i64) -> Resultado<Option<Vec<u8>>> {
        const MAX_BYTES: u64 = 1024 * 1024;
        let ruta = format!("/repos/{}/revisions/{revision}/raw", codificar(repo));
        let respuesta = match self.preparar("GET", &ruta).call() {
            Ok(r) => r,
            Err(e) => {
                leer_respuesta(Err(e))?.map_err(|f| error_http(f.codigo, &f.cuerpo))?;
                return Err(Error::ServidorUnityVc("No se pudo leer el archivo".into()));
            }
        };
        let mut bytes = Vec::new();
        respuesta
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::ServidorUnityVc("No se pudo leer el archivo".into()))?;
        Ok((bytes.len() as u64 <= MAX_BYTES).then_some(bytes))
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
    fn el_changeset_se_lee_de_la_respuesta_real() {
        // Forma de `GET …/changesets?branchName=%2Fmain%2Fdev%2FT-101` (probado 2026-11-14).
        let lista: Vec<Changeset> = sanear::interpretar(
            r#"[{"id":14,"changesetId":1,"parentChangesetId":0,"branch":"/main/dev/T-101","branchId":13,
                "date":"2026-11-12T17:20:04Z","guid":"x","owner":"ana@virustf.dev",
                "comment":"T-101: agrega LEEME.txt","changesetType":"changeset","labels":[]}]"#,
        )
        .unwrap();
        assert_eq!(
            lista,
            [Changeset {
                changeset_id: 1,
                owner: "ana@virustf.dev".into(),
                date: "2026-11-12T17:20:04Z".into(),
                comment: Some("T-101: agrega LEEME.txt".into()),
            }]
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

    #[test]
    fn el_reporte_de_merge_lista_los_archivos_en_conflicto() {
        // Forma de `POST …/mergeto/report` con un archivo modificado en las dos ramas
        // (probado 2026-11-15, #107; recortado y sin `owner`). El `path` va en `src`/`dst`.
        let con_conflicto: ReporteMerge = sanear::interpretar(
            r#"{"result":{"ancestorCsetIds":[65],"resultStatus":"AncestorCalculated","resultConflicts":{
                "addsToApply":[],"filesModifiedOnSource":[],
                "fileConflicts":[{"src":{"base":{"changeset":65,"owner":{"data":"x","password":"TOKENeyJ"}},
                  "path":"/it107.txt","status":"Changed","revInfo":{"changeset":66}},
                  "dst":{"path":"/it107.txt","status":"Changed","revInfo":{"changeset":67}},
                  "base":{"changeset":65},"dstDiff":{"path":"/it107.txt"},"srcDiff":{"path":"/it107.txt"}}],
                "evilTwins":[{"path":"/gemelo.txt"}],"changeDeleteConflicts":[]},"virtualAncestors":[]},
              "conflictsThatCannotBeSolved":false,"mergeAllowed":true,"mergeIssues":"OK"}"#,
        )
        .unwrap();
        assert_eq!(
            con_conflicto.archivos_en_conflicto(),
            ["/it107.txt", "/gemelo.txt"]
        );
        assert!(con_conflicto.merge_allowed);
        assert!(!con_conflicto.nada_que_mergear());

        // Solo un archivo agregado: nada que resolver a mano.
        let limpio: ReporteMerge = sanear::interpretar(
            r#"{"result":{"ancestorCsetIds":[67],"resultStatus":"AncestorCalculated","resultConflicts":{
                "addsToApply":[{"path":"/it107b.txt","status":"Added"}],"fileConflicts":[]},"virtualAncestors":[]},
              "conflictsThatCannotBeSolved":false,"mergeAllowed":true,"mergeIssues":"OK"}"#,
        )
        .unwrap();
        assert!(limpio.archivos_en_conflicto().is_empty());
        assert!(!limpio.nada_que_mergear());

        // Rama ya conectada al destino: no viene `resultConflicts`.
        let ya_mergeada: ReporteMerge = sanear::interpretar(
            r#"{"result":{"resultStatus":"MergeNotNeededAlreadyConnected"},"conflictsThatCannotBeSolved":false,"mergeAllowed":true,"mergeIssues":"OK"}"#,
        )
        .unwrap();
        assert!(ya_mergeada.archivos_en_conflicto().is_empty());
        assert!(ya_mergeada.nada_que_mergear());
    }

    #[test]
    fn el_resultado_del_merge_distingue_ok_conflicto_y_nada_que_mergear() {
        // Formas de `POST …/mergeto` (probado 2026-11-15, #107).
        let ok: ResultadoMergeTo =
            sanear::interpretar(r#"{"status":"OK","changesetNumber":69}"#).unwrap();
        assert!(ok.fue_ok());
        assert_eq!(ok.changeset_number, 69);

        let conflicto: ResultadoMergeTo = sanear::interpretar(
            r#"{"status":"Conflicts","message":"The \"Merge-to\" operation cannot be executed because the merge from changeset cs:66@Rep to branch br:/main/dev/padre has conflicts.\n        It is necessary to run a \"Merge from\" operation from a workspace to resolve those conflicts.","changesetNumber":0}"#,
        )
        .unwrap();
        assert!(conflicto.hubo_conflicto());
        assert!(!conflicto.fue_ok());
        assert!(conflicto.message.unwrap().contains("Merge from"));

        let nada: ResultadoMergeTo =
            sanear::interpretar(r#"{"status":"MergeNotNeeded","changesetNumber":0}"#).unwrap();
        assert!(nada.nada_que_mergear());
        assert!(!nada.hubo_conflicto());
    }

    #[test]
    fn el_cuerpo_del_mergeto_va_como_lo_espera_la_api() {
        let con = serde_json::to_value(CuerpoMergeTo {
            source_type: "Branch",
            source: "/main/dev/T-101/T-101-1",
            destination: "/main/dev/T-101",
            comment: Some("T-101-1: merge"),
        })
        .unwrap();
        assert_eq!(
            con,
            serde_json::json!({"sourceType":"Branch","source":"/main/dev/T-101/T-101-1","destination":"/main/dev/T-101","comment":"T-101-1: merge"})
        );
        let sin = serde_json::to_value(CuerpoMergeTo {
            source_type: "Branch",
            source: "/a",
            destination: "/b",
            comment: None,
        })
        .unwrap();
        assert!(sin.get("comment").is_none());
    }
}
