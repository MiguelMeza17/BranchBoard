//! Acceso real sin carpeta compartida. Solo usa BranchBoard-Pruebas, datos y WebView propios.
//! Vite: 127.0.0.8:4300; BB244_DATOS: carpeta nueva fuera del repositorio.
use std::path::{Path, PathBuf};

use branchboard_lib::{
    cm::Comando,
    comandos::acceso::entrar_en,
    error::{Error, Resultado},
    estado::AppState,
    identidad::acceso::Entrada,
};
use serde::Serialize;
use tauri::State;

struct Prueba {
    datos: PathBuf,
    workspace: PathBuf,
}

#[derive(Serialize)]
struct DatosPrueba {
    nombre: &'static str,
    repo: &'static str,
    ruta: String,
    tiene_db_branchboard: bool,
}

#[tauri::command]
fn datos_prueba(prueba: State<'_, Prueba>) -> DatosPrueba {
    DatosPrueba {
        nombre: "Acceso de prueba 244",
        repo: "BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity",
        ruta: prueba.workspace.to_string_lossy().into_owned(),
        tiene_db_branchboard: true,
    }
}

#[tauri::command]
fn entrar_a_proyecto(
    estado: State<'_, AppState>,
    prueba: State<'_, Prueba>,
    carpeta: String,
) -> Resultado<Entrada> {
    if Path::new(&carpeta) != prueba.workspace {
        return Err(Error::Validacion("Solo el workspace de esta prueba".into()));
    }
    entrar_en(&estado, &prueba.datos, &prueba.workspace)
}

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle, estado: State<'_, AppState>, prueba: State<'_, Prueba>) {
    estado.cerrar_proyecto();
    Comando::cm()
        .args(["workspace", "delete"])
        .arg(prueba.workspace.to_string_lossy().as_ref())
        .exigir_exito(&estado)
        .expect("limpiar el workspace propio");
    app.exit(0);
}

fn main() {
    let raiz = PathBuf::from(std::env::var("BB244_DATOS").expect("falta BB244_DATOS"));
    assert!(
        raiz.is_absolute() && !raiz.exists(),
        "usar una carpeta nueva"
    );
    let prueba = Prueba {
        datos: raiz.join("datos"),
        workspace: raiz.join("workspace"),
    };
    std::fs::create_dir_all(&prueba.workspace).unwrap();
    let estado = AppState::default();
    Comando::cm()
        .args(["workspace", "create"])
        .arg(format!(
            "bb-acceso-244-{}",
            chrono::Utc::now().format("%H%M%S")
        ))
        .arg(prueba.workspace.to_string_lossy().as_ref())
        .arg("rep:BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity")
        .exigir_exito(&estado)
        .unwrap();
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.8:4300".parse().unwrap());
    for ventana in &mut contexto.config_mut().app.windows {
        ventana.visible = false;
        ventana.title = "BranchBoard prueba de acceso #244".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .manage(prueba)
        .invoke_handler(tauri::generate_handler![
            datos_prueba,
            entrar_a_proyecto,
            terminar_prueba
        ])
        .run(contexto)
        .expect("WebView de prueba");
}
