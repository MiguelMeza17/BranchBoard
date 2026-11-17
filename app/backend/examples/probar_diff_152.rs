//! Comprobación de IPC + WebView real sin workspace ni mutaciones remotas.
//! Ejecutar con BB_PRUEBA_ORG, BB_PRUEBA_REPO y servidor frontend en
//! http://127.0.0.2:4300. Ventana oculta, base temporal, solo comandos de diff.
use branchboard_lib::{
    cm::lectura,
    comandos::diff as comandos_diff,
    db::tareas::{self, Tarea},
    estado::AppState,
    identidad::acceso::{self, RepoUnityVc},
    modelo::{ahora_iso, hoy_local, normalizar_email, TipoTarea},
    servidor,
};

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    let estado = AppState::default();
    let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
    let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
    assert!(repo.starts_with("BranchBoard-Pruebas/"));
    let email = lectura::whoami(&estado).unwrap();
    let cliente = servidor::conectar(&estado, &servidor::LlaveroSistema, &email, &org).unwrap();
    let es_dueno = cliente
        .listar_repositorios()
        .unwrap()
        .iter()
        .any(|r| r.name == repo && normalizar_email(&r.owner) == email);
    let repo = RepoUnityVc::desde_especificacion(&format!(
        "{repo}@{}@unity",
        servidor::nombre_organizacion(&org)
    ))
    .unwrap();
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
    let db = db.expect("la cuenta debe ser dueña del repositorio de pruebas");
    let mut tarea = Tarea::nueva(
        "T-152",
        "Visor real",
        TipoTarea::Scripting,
        &email,
        &hoy_local(),
    );
    tarea.rama_asociada = Some("/main/dev/T-101".into());
    tareas::insertar(db.conexion(), &tarea).unwrap();
    estado.abrir_proyecto(db);
    estado.fijar_usuario(Some(&email));
    estado.fijar_workspace(None, Some(repo));

    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.2:4300".parse().unwrap());
    for ventana in &mut contexto.config_mut().app.windows {
        ventana.visible = false;
        ventana.title = "BranchBoard prueba aislada #152".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .invoke_handler(tauri::generate_handler![
            comandos_diff::diff_rama,
            comandos_diff::diff_changeset,
            comandos_diff::diff_archivo,
            terminar_prueba
        ])
        .run(contexto)
        .expect("no se pudo abrir la WebView de prueba");
}
