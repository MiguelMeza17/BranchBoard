//! Historial real de `/main/dev` en Unity VC, para la vista Branches.
//!
//! Antes el tronco de dev se dibujaba solo con las tareas terminadas de BranchBoard: en un repo con
//! historia previa (VirusTF: siete merges de ramas que ya no existen) se veía un solo merge y al
//! pasar por la cabeza no se sabía cuál fue la última entrada. Aquí se leen los check-ins de dev
//! por la API (fecha, autor, comentario) y, con `cm find merge … dstbranch`, de qué rama vino cada
//! merge. Probado el 2026-11-21 en VirusTF (solo lectura).

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;

use crate::cm::Comando;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::ramas::RAMA_DEV;
use crate::servidor::{conectar, Changeset, LlaveroSistema};

/// Un check-in de dev: un merge (con la rama de origen) o un check-in directo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntradaDev {
    pub changeset: i64,
    /// Fecha en UTC, como la da la API (`2026-11-09T21:52:15Z`).
    pub fecha: String,
    pub autor: String,
    pub comentario: Option<String>,
    /// Rama que se unió en este check-in, si fue un merge.
    pub rama_origen: Option<String>,
    /// Changeset de la rama de origen que se unió.
    pub changeset_origen: Option<i64>,
}

/// Salida de `cm find merge "where dstbranch='br:/main/dev' …"
/// --format={type}|{srcbranch}|{srcchangeset}|{dstchangeset}`: changeset de dev → (rama, cs de
/// origen). Solo merges de verdad: un cherrypick no es una entrada de una rama a dev.
pub fn interpretar_merges_a_dev(salida: &str) -> BTreeMap<i64, (String, i64)> {
    let mut mapa = BTreeMap::new();
    for linea in salida.lines() {
        let partes: Vec<&str> = linea.split('|').collect();
        let [tipo, origen, cs_origen, cs_destino] = partes[..] else {
            continue;
        };
        let (Ok(cs_origen), Ok(cs_destino)) = (
            cs_origen.trim().parse::<i64>(),
            cs_destino.trim().parse::<i64>(),
        ) else {
            continue;
        };
        if tipo.trim().eq_ignore_ascii_case("merge") {
            let rama = origen
                .trim()
                .trim_start_matches("br:")
                .trim_end_matches('/')
                .to_string();
            mapa.insert(cs_destino, (rama, cs_origen));
        }
    }
    mapa
}

/// Junta los check-ins de dev con sus merges, del más viejo al más nuevo. El changeset 0 (la
/// raíz del repositorio) no es una entrada.
pub fn armar(changesets: &[Changeset], merges: &BTreeMap<i64, (String, i64)>) -> Vec<EntradaDev> {
    let mut lista: Vec<EntradaDev> = changesets
        .iter()
        .filter(|c| c.changeset_id > 0)
        .map(|c| {
            let merge = merges.get(&c.changeset_id);
            EntradaDev {
                changeset: c.changeset_id,
                fecha: c.date.clone(),
                autor: c.owner.clone(),
                comentario: c.comment.clone().filter(|t| !t.trim().is_empty()),
                rama_origen: merge.map(|(r, _)| r.clone()),
                changeset_origen: merge.map(|(_, cs)| *cs),
            }
        })
        .collect();
    lista.sort_by_key(|e| e.changeset);
    lista
}

/// Historial de dev del repositorio abierto. Sin repositorio (proyecto sin Unity VC), vacío.
pub fn consultar_en(estado: &AppState) -> Resultado<Vec<EntradaDev>> {
    let yo = estado.exigir_permiso("historial_dev")?;
    let Ok(repo) = estado.repo_abierto() else {
        return Ok(Vec::new());
    };
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
    let changesets = cliente
        .changesets_de_rama(&repo.repo, RAMA_DEV)?
        .unwrap_or_default();
    let salida = Comando::cm()
        .args(["find", "merge"])
        .arg(format!(
            "where dstbranch='br:{RAMA_DEV}' on repository '{repo_cm}'"
        ))
        .args([
            "--format={type}|{srcbranch}|{srcchangeset}|{dstchangeset}",
            "--nototal",
        ])
        .exigir_exito(estado)?;
    Ok(armar(&changesets, &interpretar_merges_a_dev(&salida)))
}

#[tauri::command]
pub async fn historial_dev(estado: State<'_, AppState>) -> Resultado<Vec<EntradaDev>> {
    consultar_en(&estado)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cs(id: i64, fecha: &str, comentario: &str) -> Changeset {
        Changeset {
            changeset_id: id,
            owner: "miguel@virustf.dev".into(),
            date: fecha.into(),
            comment: Some(comentario.into()),
            parent_changeset_id: Some(id - 1),
        }
    }

    #[test]
    fn junta_check_ins_y_merges_como_en_virustf() {
        // Salida real de VirusTF (2026-11-21), recortada.
        let merges = interpretar_merges_a_dev(
            "merge|br:/main/dev/limpieza proyecto|28|42\n\
             merge|br:/main/dev/CorreccionesEtapasBasicas1-6|121|122\n\
             cherrypick|br:/main/dev/Otra|90|91\n",
        );
        assert_eq!(merges.len(), 2, "un cherrypick no cuenta");
        let lista = armar(
            &[
                cs(
                    122,
                    "2026-11-09T21:52:15Z",
                    "Merge CorreccionesEtapasBasicas1-6 a dev",
                ),
                cs(0, "2026-10-01T00:00:00Z", ""),
                cs(54, "2026-10-11T22:58:27Z", "ProfundidadesGuardadoEnDev"),
                cs(42, "2026-10-03T18:31:47Z", ""),
            ],
            &merges,
        );
        assert_eq!(
            lista.iter().map(|e| e.changeset).collect::<Vec<_>>(),
            [42, 54, 122]
        );
        assert_eq!(
            lista[0].rama_origen.as_deref(),
            Some("/main/dev/limpieza proyecto")
        );
        assert_eq!(lista[0].comentario, None, "comentario vacío");
        assert_eq!(lista[1].rama_origen, None, "check-in directo");
        assert_eq!(
            lista[2].rama_origen.as_deref(),
            Some("/main/dev/CorreccionesEtapasBasicas1-6")
        );
        assert_eq!(lista[2].changeset_origen, Some(121));
    }
}
