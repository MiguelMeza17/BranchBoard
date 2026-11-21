//! Lectura de diferencias del servidor, sin cambiar el workspace (#152).

use serde::{Deserialize, Serialize};

use crate::db::tareas;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::servidor::{self, ClienteServidor};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineaDiff {
    pub tipo: String,
    pub texto: String,
    pub anterior: Option<usize>,
    pub nueva: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivoDiff {
    pub ruta: String,
    pub ruta_anterior: Option<String>,
    pub estado: String,
    pub binario: bool,
    pub aviso: Option<String>,
    /// Se carga al elegir el archivo, para no descargar toda la rama.
    pub lineas: Option<Vec<LineaDiff>>,
    #[serde(skip)]
    revision_anterior: i64,
    #[serde(skip)]
    revision_nueva: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diferencias {
    pub archivos: Vec<ArchivoDiff>,
}

#[derive(Deserialize)]
struct DiffServidor {
    diffs: Vec<GrupoServidor>,
}

#[derive(Deserialize)]
struct GrupoServidor {
    name: String,
    files: Vec<ArchivoServidor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchivoServidor {
    path: String,
    #[serde(default)]
    src_path: Option<String>,
    #[serde(default)]
    source_rev: i64,
    #[serde(default)]
    dst_rev: i64,
    #[serde(default)]
    repo_name: String,
    #[serde(rename = "type")]
    tipo: String,
}

fn segmento(texto: &str) -> String {
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

fn numero_changeset(hash: &str) -> Resultado<i64> {
    if hash.is_empty() || !hash.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Validacion(
            "El changeset debe ser un número de Unity VC.".into(),
        ));
    }
    hash.parse()
        .map_err(|_| Error::Validacion("El changeset no es válido.".into()))
}

fn es_binario(ruta: &str, tipo: &str) -> bool {
    let extension = ruta
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    tipo != "TextFile"
        || matches!(
            extension.as_str(),
            "fbx"
                | "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "wav"
                | "mp3"
                | "ogg"
                | "mp4"
                | "mov"
                | "dll"
                | "exe"
                | "zip"
                | "pdf"
                | "blend"
                | "glb"
        )
}

fn archivos_de_servidor(diff: DiffServidor, repo: &str) -> Vec<ArchivoDiff> {
    let mut archivos = Vec::new();
    for grupo in diff.diffs {
        for f in grupo.files {
            if f.tipo == "Directory" {
                continue;
            }
            let aviso = (!f.repo_name.is_empty() && f.repo_name != repo)
                .then(|| "El contenido pertenece a otro repositorio (Xlink).".into());
            archivos.push(ArchivoDiff {
                binario: es_binario(&f.path, &f.tipo),
                ruta: f.path,
                ruta_anterior: f.src_path.filter(|p| !p.is_empty()),
                estado: match grupo.name.as_str() {
                    "Added" | "Copied" => "agregado",
                    "Deleted" => "borrado",
                    "Moved" => "movido",
                    _ => "modificado",
                }
                .into(),
                aviso,
                lineas: None,
                revision_anterior: f.source_rev,
                revision_nueva: f.dst_rev,
            });
        }
    }
    archivos.sort_by(|a, b| a.ruta.cmp(&b.ruta));
    archivos
}

/// El hash debe pertenecer a la rama de la tarea, no solo a su repositorio.
fn leer_lista(
    cliente: &ClienteServidor,
    repo: &str,
    rama: &str,
    hash: Option<&str>,
) -> Resultado<Diferencias> {
    let ruta = if let Some(hash) = hash {
        let n = numero_changeset(hash)?;
        let propios = cliente.changesets_de_rama(repo, rama)?.unwrap_or_default();
        if !propios.iter().any(|c| c.changeset_id == n) {
            return Err(Error::Validacion(
                "Ese check-in no pertenece a la rama de esta tarea.".into(),
            ));
        }
        format!("/repos/{}/diff/changeset/{n}", segmento(repo))
    } else {
        format!("/repos/{}/diff/branch/{}", segmento(repo), segmento(rama))
    };
    let diff: DiffServidor = cliente.get(&ruta)?;
    Ok(Diferencias {
        archivos: archivos_de_servidor(diff, repo),
    })
}

fn texto_revision(
    cliente: &ClienteServidor,
    repo: &str,
    revision: i64,
) -> Resultado<Option<String>> {
    if revision <= 0 {
        return Ok(Some(String::new()));
    }
    let Some(bytes) = cliente.revision_bytes(repo, revision)? else {
        return Ok(None);
    };
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 {
            return Ok(None);
        }
        let le = bytes[0] == 0xff;
        let unidades: Vec<_> = bytes[2..]
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        return Ok(String::from_utf16(&unidades).ok());
    }
    if bytes.contains(&0) {
        return Ok(None);
    }
    Ok(String::from_utf8(bytes)
        .ok()
        .map(|s| s.trim_start_matches('\u{feff}').to_string()))
}

fn completar_archivo(
    cliente: &ClienteServidor,
    repo: &str,
    mut archivo: ArchivoDiff,
) -> Resultado<ArchivoDiff> {
    if archivo.binario || archivo.aviso.is_some() {
        return Ok(archivo);
    }
    let anterior = texto_revision(cliente, repo, archivo.revision_anterior)?;
    let nueva = texto_revision(cliente, repo, archivo.revision_nueva)?;
    match (anterior, nueva) {
        (Some(a), Some(b)) if a.lines().count() + b.lines().count() <= 20_000 => {
            archivo.lineas = Some(comparar_lineas(&a, &b));
        }
        (Some(_), Some(_)) => {
            archivo.aviso = Some("El archivo supera el límite de 20 000 líneas del visor.".into())
        }
        _ => {
            archivo.aviso = Some(
                "Contenido binario, codificación no compatible o archivo mayor de 1 MiB.".into(),
            )
        }
    }
    Ok(archivo)
}

/// Comparación por líneas con memoria acotada. Para archivos muy largos conserva
/// el prefijo/sufijo común y representa el bloque central como reemplazo.
pub fn comparar_lineas(anterior: &str, nueva: &str) -> Vec<LineaDiff> {
    let a: Vec<_> = anterior.split_inclusive('\n').collect();
    let b: Vec<_> = nueva.split_inclusive('\n').collect();
    let (n, m) = (a.len(), b.len());
    let mut salida = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut emitir = |tipo: &str, texto: &str, anterior, nueva| {
        salida.push(LineaDiff {
            tipo: tipo.into(),
            texto: texto.into(),
            anterior,
            nueva,
        })
    };
    if (n + 1).saturating_mul(m + 1) <= 4_000_000 {
        let mut tabla = vec![0u32; (n + 1) * (m + 1)];
        for x in (0..n).rev() {
            for y in (0..m).rev() {
                tabla[x * (m + 1) + y] = if a[x] == b[y] {
                    1 + tabla[(x + 1) * (m + 1) + y + 1]
                } else {
                    tabla[(x + 1) * (m + 1) + y].max(tabla[x * (m + 1) + y + 1])
                };
            }
        }
        while i < n || j < m {
            if i < n && j < m && a[i] == b[j] {
                emitir("igual", a[i], Some(i + 1), Some(j + 1));
                i += 1;
                j += 1;
            } else if i < n
                && (j == m || tabla[(i + 1) * (m + 1) + j] >= tabla[i * (m + 1) + j + 1])
            {
                emitir("borrada", a[i], Some(i + 1), None);
                i += 1;
            } else {
                emitir("agregada", b[j], None, Some(j + 1));
                j += 1;
            }
        }
    } else {
        while i < n && j < m && a[i] == b[j] {
            emitir("igual", a[i], Some(i + 1), Some(j + 1));
            i += 1;
            j += 1;
        }
        let mut fin = 0;
        while fin < n - i && fin < m - j && a[n - fin - 1] == b[m - fin - 1] {
            fin += 1;
        }
        while i < n - fin {
            emitir("borrada", a[i], Some(i + 1), None);
            i += 1;
        }
        while j < m - fin {
            emitir("agregada", b[j], None, Some(j + 1));
            j += 1;
        }
        while i < n {
            emitir("igual", a[i], Some(i + 1), Some(j + 1));
            i += 1;
            j += 1;
        }
    }
    salida
}

/// Comparte las validaciones de los tres comandos: permisos, tarea y rama.
pub fn consultar_en(
    estado: &AppState,
    id: &str,
    hash: Option<&str>,
    archivo: Option<&str>,
) -> Resultado<Diferencias> {
    let yo = estado.exigir_permiso(if hash.is_some() {
        "diff_changeset"
    } else {
        "diff_rama"
    })?;
    if let Some(h) = hash {
        numero_changeset(h)?;
    }
    let tarea = estado.con_db(|db| tareas::obtener(db.conexion(), id))?;
    let rama = tarea
        .rama_asociada
        .as_deref()
        .filter(|r| !tarea.eliminada && r.starts_with("/main/dev/"))
        .ok_or_else(|| Error::Validacion("La tarea no tiene una rama disponible.".into()))?;
    let repo = estado.repo_abierto()?;
    let cliente = servidor::conectar(estado, &servidor::LlaveroSistema, &yo.email, &repo.org)?;
    let mut diff = leer_lista(&cliente, &repo.repo, rama, hash)?;
    if let Some(ruta) = archivo {
        let archivo = diff
            .archivos
            .into_iter()
            .find(|a| a.ruta == ruta)
            .ok_or_else(|| {
                Error::Validacion("El archivo no forma parte de estas diferencias.".into())
            })?;
        diff.archivos = vec![completar_archivo(&cliente, &repo.repo, archivo)?];
    }
    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparar_conserva_ambos_textos_y_numeros_incluso_sin_salto_final() {
        for (a, b) in [
            ("", "nuevo\n"),
            ("viejo\n", ""),
            ("igual\nviejo\nfin", "igual\nnuevo\nfin"),
            ("uno\r\ndos\n", "uno\ndos\n"),
            ("a\na\nb\n", "b\na\n"),
            ("fin", "fin\n"),
            ("", ""),
        ] {
            let lineas = comparar_lineas(a, b);
            assert_eq!(
                lineas
                    .iter()
                    .filter(|l| l.tipo != "agregada")
                    .map(|l| l.texto.as_str())
                    .collect::<String>(),
                a
            );
            assert_eq!(
                lineas
                    .iter()
                    .filter(|l| l.tipo != "borrada")
                    .map(|l| l.texto.as_str())
                    .collect::<String>(),
                b
            );
            let (mut n, mut m) = (1, 1);
            for l in lineas {
                if l.tipo != "agregada" {
                    assert_eq!(l.anterior, Some(n));
                    n += 1;
                } else {
                    assert_eq!(l.anterior, None);
                }
                if l.tipo != "borrada" {
                    assert_eq!(l.nueva, Some(m));
                    m += 1;
                } else {
                    assert_eq!(l.nueva, None);
                }
            }
        }
        let lineas = comparar_lineas("inicio\nviejo\nfin\n", "inicio\nnuevo\nfin\n");
        assert_eq!(
            lineas.iter().map(|l| l.tipo.as_str()).collect::<Vec<_>>(),
            ["igual", "borrada", "agregada", "igual"]
        );
    }

    #[test]
    fn comparacion_grande_acota_memoria_y_conserva_contenido() {
        let a = format!("inicio\n{}fin\n", "viejo\n".repeat(2100));
        let b = format!("inicio\n{}fin\n", "nuevo\n".repeat(2100));
        let lineas = comparar_lineas(&a, &b);
        assert_eq!(lineas.first().unwrap().tipo, "igual");
        assert_eq!(lineas.last().unwrap().tipo, "igual");
        assert_eq!(
            lineas
                .iter()
                .filter(|l| l.tipo != "agregada")
                .map(|l| l.texto.as_str())
                .collect::<String>(),
            a
        );
        assert_eq!(
            lineas
                .iter()
                .filter(|l| l.tipo != "borrada")
                .map(|l| l.texto.as_str())
                .collect::<String>(),
            b
        );
    }

    #[test]
    fn interpreta_estados_binarios_directorios_y_renombres_sin_exponer_metadatos() {
        let diff = servidor::sanear::interpretar(r#"{"owner":{"password":"oculto"},"diffs":[
            {"name":"Added","files":[{"path":"/carpeta","type":"Directory"},{"path":"/foto.png","type":"BinaryFile","dstRev":8}]},
            {"name":"Changed","files":[{"path":"/codigo.cs","type":"TextFile","sourceRev":2,"dstRev":3}]},
            {"name":"Deleted","files":[{"path":"/borrado.txt","type":"TextFile","sourceRev":4}]},
            {"name":"Moved","files":[{"path":"/nuevo.md","srcPath":"/viejo.md","type":"TextFile","sourceRev":6,"dstRev":6}]}
        ]}"#).unwrap();
        let archivos = archivos_de_servidor(diff, "Prueba");
        assert_eq!(archivos.len(), 4);
        assert_eq!(archivos[0].estado, "borrado");
        assert_eq!(archivos[1].estado, "modificado");
        assert!(archivos[2].binario);
        assert_eq!(archivos[3].ruta_anterior.as_deref(), Some("/viejo.md"));
        assert!(es_binario("/modelo.fbx", "TextFile"));
        assert!(!es_binario("/shader.shader", "TextFile"));
        let json = serde_json::to_string(&archivos).unwrap();
        assert!(!json.contains("oculto"));
        assert!(!json.contains("revision_anterior"));
    }

    #[test]
    fn cambios_y_roles_se_validan_antes_de_consultar_el_servidor() {
        for hash in [
            "",
            "-1",
            "1/../../",
            "1@otro",
            "1 --opcion",
            "9223372036854775808",
        ] {
            assert!(numero_changeset(hash).is_err());
        }
        assert_eq!(numero_changeset("42").unwrap(), 42);
        assert!(consultar_en(&AppState::default(), "T-1", None, None).is_err());
        use crate::identidad::permisos::{exigir, UsuarioActual};
        for rol in crate::modelo::Rol::TODOS {
            let usuario = UsuarioActual {
                email: "ejemplo@virustf.dev".into(),
                rol: *rol,
            };
            assert!(exigir("diff_changeset", &usuario).is_ok());
            assert_eq!(
                exigir("diff_rama", &usuario).is_ok(),
                matches!(
                    rol,
                    crate::modelo::Rol::Administrador | crate::modelo::Rol::Lider
                )
            );
        }
    }

    #[test]
    #[ignore = "lectura real de BranchBoard-Pruebas con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_diff_lectura() {
        use crate::{
            cm::{lectura, Comando},
            identidad::acceso::{self, RepoUnityVc},
            modelo::{ahora_iso, hoy_local, TipoTarea},
        };
        let estado = AppState::default();
        let org = std::env::var("BB_PRUEBA_ORG").unwrap();
        let repo = std::env::var("BB_PRUEBA_REPO").unwrap();
        assert!(
            repo.starts_with("BranchBoard-Pruebas/"),
            "solo se permite el repo de pruebas"
        );
        let workspaces_antes = Comando::cm()
            .args(["wk", "list"])
            .exigir_exito(&estado)
            .unwrap();
        let email = lectura::whoami(&estado).unwrap();
        use crate::servidor::pat::{cuenta_llavero, Llavero, Pat};
        struct PatDePrueba {
            cuenta: String,
            org: String,
            previo: Option<String>,
        }
        impl Drop for PatDePrueba {
            fn drop(&mut self) {
                if let Ok(Some(valor)) = servidor::LlaveroSistema.leer(&self.cuenta) {
                    if let Ok(pat) = serde_json::from_str::<Pat>(&valor) {
                        if self.previo.as_deref() != Some(pat.id.as_str()) {
                            servidor::pat::revocar_pat(&AppState::default(), &self.org, &pat.id);
                            let _ = servidor::LlaveroSistema.borrar(&self.cuenta);
                        }
                    }
                }
            }
        }
        let cuenta = cuenta_llavero(&email, &org);
        let _pat = PatDePrueba {
            previo: servidor::LlaveroSistema
                .leer(&cuenta)
                .unwrap()
                .map(|v| serde_json::from_str::<Pat>(&v).unwrap().id),
            cuenta,
            org: format!("{}@unity", servidor::nombre_organizacion(&org)),
        };
        let cliente = servidor::conectar(&estado, &servidor::LlaveroSistema, &email, &org).unwrap();
        let rama =
            std::env::var("BB_PRUEBA_RAMA_DIFF").unwrap_or_else(|_| "/main/dev/T-101".into());
        let cs = cliente
            .changesets_de_rama(&repo, &rama)
            .unwrap()
            .unwrap()
            .first()
            .unwrap()
            .changeset_id;
        let datos = tempfile::tempdir().unwrap();
        let repo_id = RepoUnityVc::desde_especificacion(&format!(
            "{repo}@{}@unity",
            servidor::nombre_organizacion(&org)
        ))
        .unwrap();
        let es_dueno = cliente
            .listar_repositorios()
            .unwrap()
            .iter()
            .any(|r| r.name == repo && crate::modelo::normalizar_email(&r.owner) == email);
        let (_, db) = acceso::entrar(
            datos.path(),
            &repo_id,
            &email,
            || Ok(es_dueno),
            &ahora_iso(),
            &hoy_local(),
        )
        .unwrap();
        let db = db.expect("la cuenta de prueba debe ser dueña del repositorio");
        let mut tarea = tareas::Tarea::nueva(
            "T-152",
            "Lectura diff",
            TipoTarea::Scripting,
            &email,
            &hoy_local(),
        );
        tarea.rama_asociada = Some(rama);
        tareas::insertar(db.conexion(), &tarea).unwrap();
        estado.abrir_proyecto(db);
        estado.fijar_usuario(Some(&email));
        // El comando no necesita workspace: solo repo, tarea y rol firmado.
        estado.fijar_workspace(None, Some(repo_id));
        let por_rama = consultar_en(&estado, &tarea.id, None, None).unwrap();
        let por_cs = consultar_en(&estado, &tarea.id, Some(&cs.to_string()), None).unwrap();
        assert!(!por_rama.archivos.is_empty());
        assert!(!por_cs.archivos.is_empty());
        for hash in [None, Some(cs.to_string())] {
            let lista = consultar_en(&estado, &tarea.id, hash.as_deref(), None).unwrap();
            let texto = lista
                .archivos
                .iter()
                .find(|a| !a.binario && a.aviso.is_none())
                .unwrap();
            let detalle =
                consultar_en(&estado, &tarea.id, hash.as_deref(), Some(&texto.ruta)).unwrap();
            assert!(!detalle.archivos[0].lineas.as_ref().unwrap().is_empty());
        }
        // Fixture original de #82: cs:5 modifica LEEME.txt en dev. Se lee sin
        // hacer checkout ni tocar ese changeset compartido.
        let modificado = leer_lista(&cliente, &repo, "/main/dev", Some("5")).unwrap();
        let archivo = modificado
            .archivos
            .into_iter()
            .find(|a| a.estado == "modificado")
            .unwrap();
        let detalle = completar_archivo(&cliente, &repo, archivo).unwrap();
        let lineas = detalle.lineas.unwrap();
        assert!(lineas.iter().any(|l| l.tipo == "borrada"));
        assert!(lineas.iter().any(|l| l.tipo == "agregada"));
        assert!(
            consultar_en(&estado, &tarea.id, Some("0"), None).is_err(),
            "changeset de otra rama rechazado"
        );
        assert!(consultar_en(&estado, &tarea.id, None, Some("/fuera-del-diff.cs")).is_err());
        assert_eq!(
            Comando::cm()
                .args(["wk", "list"])
                .exigir_exito(&estado)
                .unwrap(),
            workspaces_antes
        );
        assert!(estado.workspace_abierto().is_err());
        estado.cerrar_proyecto();
        println!("Diff real: rama y cs:{cs}, lista y texto correctos; check-in ajeno y ruta ajena rechazados; sin crear ni mover workspaces ni ramas.");
    }
}
