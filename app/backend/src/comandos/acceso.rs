//! Entrar a un proyecto (issue #89). Las reglas viven en `identidad::acceso`;
//! aquí se juntan `cm whoami`, el repositorio del workspace, el dueño del repo
//! (por la API de Unity VC) y la carpeta de datos de la app.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::cm::lectura::{rama_actual_workspace, whoami};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::{self, Entrada, RepoUnityVc};
use crate::modelo::{ahora_iso, hoy_local, normalizar_email};
use crate::servidor::{conectar, LlaveroSistema};

/// `entrar_a_proyecto(carpeta)`: libre (todavía no hay rol). Si la persona
/// entra, deja abierta la base del proyecto y queda identificada; si no, no
/// queda ningún proyecto abierto.
#[tauri::command]
pub async fn entrar_a_proyecto(
    app: AppHandle,
    estado: State<'_, AppState>,
    carpeta: String,
) -> Resultado<Entrada> {
    let datos = carpeta_datos(&app)?;
    entrar_en(&estado, &datos, Path::new(&carpeta))
}

/// `%LocalAppData%\BranchBoard` (ver "Base de datos local" en BACKEND.md).
pub fn carpeta_datos(app: &AppHandle) -> Resultado<PathBuf> {
    let base = app
        .path()
        .local_data_dir()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    Ok(base.join("BranchBoard"))
}

/// Lo mismo que el comando, sin Tauri (para la prueba en el entorno real).
pub fn entrar_en(estado: &AppState, datos: &Path, workspace: &Path) -> Resultado<Entrada> {
    estado.cerrar_proyecto();
    let email = whoami(estado)?;
    estado.fijar_usuario(Some(&email));
    let especificacion = rama_actual_workspace(estado, workspace)?.repositorio;
    let repo = RepoUnityVc::desde_especificacion(&especificacion)
        .ok_or_else(|| acceso::error_repo_no_cloud(&especificacion))?;
    let (entrada, db) = acceso::entrar(
        datos,
        &repo,
        &email,
        || es_dueno_del_repo(estado, &repo, &email),
        &ahora_iso(),
        &hoy_local(),
    )?;
    if let Some(db) = db {
        estado.abrir_proyecto(db);
        estado.fijar_workspace(Some(workspace.to_path_buf()), Some(repo));
        let _ = crate::comandos::sprints::evaluar_cierres_en(estado, &email)?;
    }
    Ok(entrada)
}

/// `true` si `email` es el dueño (`owner`) del repositorio según la API.
fn es_dueno_del_repo(estado: &AppState, repo: &RepoUnityVc, email: &str) -> Resultado<bool> {
    let cliente = conectar(estado, &LlaveroSistema, email, &repo.org)?;
    let repos = cliente.listar_repositorios()?;
    let encontrado = repos
        .iter()
        .find(|r| r.name == repo.repo)
        .ok_or_else(|| Error::NoEncontrado(format!("el repositorio {} en Unity VC", repo.repo)))?;
    Ok(normalizar_email(&encontrado.owner) == normalizar_email(email))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cm::Comando;
    use crate::modelo::Rol;

    /// **Prueba en el entorno real** (CLAUDE.md): con un workspace temporal del
    /// repo de pruebas y una carpeta de datos temporal, entra como el dueño (se
    /// configura y queda Administrador) y vuelve a entrar (ya no configura).
    /// Necesita que la cuenta de `cm` sea la dueña del repo.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_entrar -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_entrar_a_proyecto() {
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let estado = AppState::default();
        let ws = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let ruta_ws = ws.path().to_string_lossy().to_string();
        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!(
                "bb-prueba-89-{}",
                chrono::Local::now().format("%H%M%S")
            ))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let primera = entrar_en(&estado, datos.path(), ws.path()).unwrap();
            println!(
                "primera vez: {}",
                serde_json::to_string(&primera)
                    .unwrap()
                    .replace(&whoami(&estado).unwrap(), "<correo>")
            );
            assert!(matches!(
                primera,
                Entrada::Dentro {
                    rol: Rol::Administrador,
                    configurado_ahora: true,
                    ..
                }
            ));
            // Con el proyecto abierto, los comandos del Administrador ya pasan el chequeo de rol.
            assert!(estado.exigir_permiso("listar_usuarios").is_ok());

            let segunda = entrar_en(&estado, datos.path(), ws.path()).unwrap();
            println!(
                "segunda vez: {}",
                serde_json::to_string(&segunda)
                    .unwrap()
                    .replace(&whoami(&estado).unwrap(), "<correo>")
            );
            assert!(matches!(
                segunda,
                Entrada::Dentro {
                    rol: Rol::Administrador,
                    rol_verificado: true,
                    configurado_ahora: false,
                    ..
                }
            ));
        }));
        estado.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado);
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
