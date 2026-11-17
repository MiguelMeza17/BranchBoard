//! Prueba destructiva aislada de #128, solo con autorización explícita.
//! Exige BB_PRUEBA_REPO_ELIMINAR = BB_AUTORIZAR_BORRADO = spec completo y
//! un nombre nuevo BranchBoard-Pruebas/bb-prueba-128-* en MiguelM17@unity.
//! La ventana está oculta; el frontend de prueba va en 127.0.0.2:4300.
use branchboard_lib::servidor::pat::{cuenta_llavero, Llavero, Pat};
use branchboard_lib::{
    cm::{lectura, Comando},
    comandos::proyecto as comandos_proyecto,
    estado::AppState,
    identidad::acceso::{self, RepoUnityVc},
    modelo::{ahora_iso, hoy_local, normalizar_email},
    servidor,
};
use tauri::Manager;

struct RepoTemporal(String);
impl Drop for RepoTemporal {
    fn drop(&mut self) {
        // Solo se construye tras crear con éxito el repo nuevo y autorizado.
        let _ = Comando::cm()
            .args(["repo", "rm", &format!("rep:{}", self.0)])
            .ejecutar(&AppState::default());
    }
}

struct PatTemporal {
    cuenta: String,
    org: String,
    previo: Option<String>,
}
impl Drop for PatTemporal {
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

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    let spec = std::env::var("BB_PRUEBA_REPO_ELIMINAR").expect("falta el repo nuevo autorizado");
    assert_eq!(
        std::env::var("BB_AUTORIZAR_BORRADO").expect("falta autorización exacta"),
        spec
    );
    let repo = RepoUnityVc::desde_especificacion(&spec).expect("spec Cloud inválido");
    assert_eq!(repo.org, "MiguelM17");
    let sufijo = repo
        .repo
        .strip_prefix("BranchBoard-Pruebas/bb-prueba-128-")
        .expect("solo repo descartable #128");
    assert!(
        !sufijo.is_empty()
            && sufijo
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
    );
    assert_eq!(format!("{}@{}@unity", repo.repo, repo.org), spec);
    let estado = AppState::default();
    let email = lectura::whoami(&estado).unwrap();
    let cuenta = cuenta_llavero(&email, &repo.org);
    let pat = PatTemporal {
        previo: servidor::LlaveroSistema
            .leer(&cuenta)
            .unwrap()
            .map(|v| serde_json::from_str::<Pat>(&v).unwrap().id),
        cuenta,
        org: format!("{}@unity", repo.org),
    };
    let cliente =
        servidor::conectar(&estado, &servidor::LlaveroSistema, &email, &repo.org).unwrap();
    assert!(
        !cliente
            .listar_repositorios()
            .unwrap()
            .iter()
            .any(|r| r.name == repo.repo),
        "no se toca un repositorio preexistente"
    );
    Comando::cm()
        .args(["repo", "create", &spec])
        .exigir_exito(&estado)
        .unwrap();
    let temporal = RepoTemporal(spec);
    let es_dueno = cliente
        .listar_repositorios()
        .unwrap()
        .iter()
        .any(|r| r.name == repo.repo && normalizar_email(&r.owner) == email);
    let datos = tempfile::tempdir().unwrap();
    let (_, db) = acceso::entrar(
        datos.path(),
        &repo,
        &email,
        || Ok(es_dueno),
        &ahora_iso(),
        &hoy_local(),
    )
    .unwrap();
    estado.abrir_proyecto(db.expect("se requiere ser el dueño del repo de prueba"));
    estado.fijar_usuario(Some(&email));
    estado.fijar_workspace(None, Some(repo));

    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.2:4300".parse().unwrap());
    for w in &mut contexto.config_mut().app.windows {
        w.visible = false;
        w.title = "Prueba aislada #128".into();
    }
    let app = tauri::Builder::default()
        .manage(estado)
        .invoke_handler(tauri::generate_handler![
            comandos_proyecto::eliminar_proyecto,
            terminar_prueba
        ])
        .build(contexto)
        .unwrap();
    let mut limpieza = Some((temporal, pat, datos));
    app.run(move |app, evento| {
        if matches!(evento, tauri::RunEvent::Exit) {
            app.state::<AppState>().cerrar_proyecto();
            drop(limpieza.take());
        }
    });
}
