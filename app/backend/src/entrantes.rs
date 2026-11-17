//! Cambios entrantes (incoming changes, #110): lo que ya llegó a la rama de una
//! tarea en el servidor (merges de sus subtareas terminadas) y que el workspace
//! de esta máquina todavía no tiene. Es local: no pasa por `ops/`.
//!
//! **Se unen todos o ninguno** (decidido con Miguel el 2026-11-16, comprobado en
//! vivo): `cm update --changeset=N` trae también todo lo anterior a `N`, y traer
//! una sola subtarea con cherry-pick + check-in deja la rama padre con dos
//! cabezas (el merge oficial queda colgado y aparece un check-in directo). Por eso
//! [`unir_en`] corre `cm update .` y trae la rama hasta su cabeza.

use std::path::Path;

use serde::Serialize;

use crate::cm::lectura::rama_actual_workspace;
use crate::cm::Comando;
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::ramas;

/// Un changeset de la rama que el workspace todavía no tiene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangesetEntrante {
    pub changeset: i64,
    pub autor: String,
    pub fecha: String,
    pub comentario: String,
    /// Rama de la que vino, si este changeset es el merge de otra rama.
    pub rama_origen: Option<String>,
    /// Subtarea de BranchBoard dueña de `rama_origen`, si se reconoce.
    pub tarea_origen: Option<String>,
}

/// Cambios entrantes de la rama de una tarea, vistos desde el workspace de esta máquina.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CambiosEntrantes {
    /// Rama de la tarea.
    pub rama: String,
    /// Rama en la que está el workspace.
    pub rama_actual: String,
    /// Si el workspace está en la rama de la tarea. Si no, no se puede saber qué le
    /// falta ni unirlo: hay que cambiar de rama antes.
    pub en_rama: bool,
    /// Changeset en el que está el workspace.
    pub changeset_workspace: Option<i64>,
    /// Último changeset de la rama, si el workspace está atrasado.
    pub cabeza: Option<i64>,
    /// Lo que falta, del más antiguo al más nuevo. `cm update .` los trae todos juntos.
    pub changesets: Vec<ChangesetEntrante>,
}

#[tauri::command]
pub async fn cambios_entrantes(
    estado: tauri::State<'_, AppState>,
    tarea_id: String,
) -> Resultado<CambiosEntrantes> {
    consultar_en(&estado, &tarea_id)
}

#[tauri::command]
pub async fn unir_cambios_entrantes(
    estado: tauri::State<'_, AppState>,
    tarea_id: String,
) -> Resultado<CambiosEntrantes> {
    unir_en(&estado, &tarea_id)
}

/// Qué le falta al workspace de la rama de `tarea_id`.
pub fn consultar_en(estado: &AppState, tarea_id: &str) -> Resultado<CambiosEntrantes> {
    estado.exigir_permiso("cambios_entrantes")?;
    let (tarea, todas) = tarea_y_todas(estado, tarea_id)?;
    let rama = rama_de(&tarea)?;
    let workspace = estado.workspace_abierto()?;
    calcular(estado, &workspace, &rama, &todas)
}

/// Une al workspace **todos** los cambios entrantes de la rama de `tarea_id`
/// (`cm update .`). Exige estar en esa rama y sin cambios sin check-in.
pub fn unir_en(estado: &AppState, tarea_id: &str) -> Resultado<CambiosEntrantes> {
    estado.exigir_permiso("unir_cambios_entrantes")?;
    let (tarea, todas) = tarea_y_todas(estado, tarea_id)?;
    let rama = rama_de(&tarea)?;
    let workspace = estado.workspace_abierto()?;
    let antes = calcular(estado, &workspace, &rama, &todas)?;
    if !antes.en_rama {
        return Err(Error::Validacion(format!(
            "Tu workspace está en la rama {}, no en {rama}: cambia a esa rama antes de unir sus cambios entrantes",
            antes.rama_actual
        )));
    }
    if antes.changesets.is_empty() {
        return Ok(antes);
    }
    let pendientes = ramas::cambios_pendientes(estado, &workspace)?;
    if !pendientes.is_empty() {
        return Err(Error::Validacion(format!(
            "Tienes {} cambio(s) sin check-in en esta rama. Guárdalos o deshazlos antes de unir los cambios entrantes",
            pendientes.len()
        )));
    }
    Comando::cm()
        .args(["update", ".", "--noinput"])
        .en(&workspace)
        .exigir_exito(estado)?;
    calcular(estado, &workspace, &rama, &todas)
}

fn tarea_y_todas(estado: &AppState, tarea_id: &str) -> Resultado<(Tarea, Vec<Tarea>)> {
    estado.con_db(|db| {
        let todas = db_tareas::listar(db.conexion())?;
        let tarea = todas
            .iter()
            .find(|t| t.id == tarea_id && !t.eliminada)
            .cloned()
            .ok_or_else(|| Error::NoEncontrado(format!("La tarea {tarea_id} no existe")))?;
        Ok((tarea, todas))
    })
}

fn rama_de(tarea: &Tarea) -> Resultado<String> {
    let rama = tarea
        .rama_asociada
        .as_deref()
        .map(|r| r.trim().trim_end_matches('/'))
        .filter(|r| !r.is_empty())
        .ok_or_else(|| Error::Validacion(format!("{} todavía no tiene rama", tarea.id)))?;
    if rama.contains(['\'', '"']) || !rama.starts_with('/') {
        return Err(Error::Validacion(format!("Rama no válida: {rama}")));
    }
    Ok(rama.to_string())
}

fn calcular(
    estado: &AppState,
    workspace: &Path,
    rama: &str,
    todas: &[Tarea],
) -> Resultado<CambiosEntrantes> {
    let actual = rama_actual_workspace(estado, workspace)?;
    let en_rama = actual.rama.eq_ignore_ascii_case(rama);
    let mut res = CambiosEntrantes {
        rama: rama.to_string(),
        rama_actual: actual.rama.clone(),
        en_rama,
        changeset_workspace: actual.changeset,
        cabeza: None,
        changesets: Vec::new(),
    };
    if !en_rama {
        return Ok(res);
    }
    let cabecera = Comando::cm()
        .args(["status", "--header"])
        .en(workspace)
        .exigir_exito(estado)?;
    let (Some(desde), Some(cabeza)) = (actual.changeset, interpretar_cabeza(&cabecera)) else {
        return Ok(res);
    };
    if cabeza <= desde {
        return Ok(res);
    }
    res.cabeza = Some(cabeza);
    let repo = estado.repo_abierto()?;
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);

    let texto_cs = Comando::cm()
        .args(["find", "changeset"])
        .arg(format!(
            "where branch='br:{rama}' and changesetid > {desde} on repository '{repo_cm}'"
        ))
        .args([
            &format!(
                "--format={SEP_REGISTRO}{{changesetid}}{SEP}{{owner}}{SEP}{{date}}{SEP}{{comment}}"
            ),
            "--nototal",
        ])
        .exigir_exito(estado)?;
    let texto_merges = Comando::cm()
        .args(["find", "merge"])
        .arg(format!(
            "where dstbranch='br:{rama}' on repository '{repo_cm}'"
        ))
        .args(["--format={type}|{srcbranch}|{dstchangeset}", "--nototal"])
        .exigir_exito(estado)?;

    let merges = interpretar_merges(&texto_merges);
    res.changesets = interpretar_changesets(&texto_cs)
        .into_iter()
        .filter(|c| c.changeset > desde && c.changeset <= cabeza)
        .map(|mut c| {
            if let Some((_, origen)) = merges.iter().find(|(cs, _)| *cs == c.changeset) {
                c.tarea_origen = todas
                    .iter()
                    .find(|t| {
                        !t.eliminada
                            && t.rama_asociada.as_deref().is_some_and(|r| {
                                r.trim_end_matches('/').eq_ignore_ascii_case(origen)
                            })
                    })
                    .map(|t| t.id.clone());
                c.rama_origen = Some(origen.clone());
            }
            c
        })
        .collect();
    Ok(res)
}

const SEP: &str = "\u{1f}";
const SEP_REGISTRO: &str = "\u{1e}";

/// `(cs:27 - head:28)` → `Some(28)`; `(cs:28 - head)` → `Some(28)` (al día).
fn interpretar_cabeza(texto: &str) -> Option<i64> {
    let linea = texto
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with('/') || l.starts_with("cs:"))?;
    let parentesis = linea.rsplit_once(" (")?.1;
    let numero = |s: &str| -> Option<i64> {
        let d: String = s
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        d.parse().ok()
    };
    if let Some((_, despues)) = parentesis.split_once("head:") {
        return numero(despues);
    }
    parentesis
        .contains("head")
        .then(|| parentesis.split_once("cs:").and_then(|(_, d)| numero(d)))
        .flatten()
}

/// Filas de `cm find changeset` separadas por `SEP_REGISTRO` (el comentario puede tener saltos de línea).
fn interpretar_changesets(texto: &str) -> Vec<ChangesetEntrante> {
    let mut lista: Vec<ChangesetEntrante> = texto
        .split(SEP_REGISTRO)
        .filter_map(|registro| {
            let mut partes = registro.splitn(4, SEP);
            let changeset = partes.next()?.trim().parse().ok()?;
            Some(ChangesetEntrante {
                changeset,
                autor: partes.next().unwrap_or_default().trim().to_string(),
                fecha: partes.next().unwrap_or_default().trim().to_string(),
                comentario: partes.next().unwrap_or_default().trim().to_string(),
                rama_origen: None,
                tarea_origen: None,
            })
        })
        .collect();
    lista.sort_by_key(|c| c.changeset);
    lista
}

/// Merges (`type == merge`) que llegaron a la rama: `(changeset destino, rama origen)`.
/// Los préstamos (`cherrypick…`) no cuentan como subtarea.
fn interpretar_merges(texto: &str) -> Vec<(i64, String)> {
    texto
        .lines()
        .filter_map(|l| {
            let mut p = l.trim().splitn(3, '|');
            let (tipo, origen, destino) = (p.next()?, p.next()?, p.next()?);
            if !tipo.trim().eq_ignore_ascii_case("merge") {
                return None;
            }
            let origen = origen
                .trim()
                .trim_start_matches("br:")
                .split('@')
                .next()?
                .trim_end_matches('/')
                .to_string();
            Some((destino.trim().parse().ok()?, origen))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cabeza_atrasada_y_al_dia() {
        let r = "/main/dev/T-1-padre@Proy/Repo@org@unity";
        assert_eq!(
            interpretar_cabeza(&format!("{r} (cs:288 - head:289)\n")),
            Some(289)
        );
        assert_eq!(
            interpretar_cabeza(&format!("{r} (cs:289 - head)")),
            Some(289)
        );
        assert_eq!(interpretar_cabeza("Buscando cambios...\n"), None);
        assert_eq!(interpretar_cabeza(&format!("{r} (cs:7)")), None);
    }

    #[test]
    fn changesets_con_comentario_multilinea_y_separadores() {
        let texto = format!(
            "{SEP_REGISTRO}289{SEP}b@x.com{SEP}2026-11-16{SEP}merge B\nsegunda | línea\n\
             {SEP_REGISTRO}288{SEP}a@x.com{SEP}2026-11-15{SEP}merge A\n"
        );
        let lista = interpretar_changesets(&texto);
        assert_eq!(
            lista.iter().map(|c| c.changeset).collect::<Vec<_>>(),
            [288, 289]
        );
        assert_eq!(lista[1].comentario, "merge B\nsegunda | línea");
        assert_eq!(lista[0].autor, "a@x.com");
    }

    #[test]
    fn merges_ignoran_prestamos() {
        let texto = "merge|br:/main/dev/T-1/T-1-1-a@286|288\n\
                     merge|br:/main/dev/T-1/T-1-2-b|289\n\
                     cherrypick|br:/main/dev/T-9@12|291\n";
        assert_eq!(
            interpretar_merges(texto),
            vec![
                (288, "/main/dev/T-1/T-1-1-a".to_string()),
                (289, "/main/dev/T-1/T-1-2-b".to_string())
            ]
        );
    }
}
