//! Comprobación en pantalla de #123 en la app de escritorio real, con la ventana oculta.
//! El frontend de prueba va en 127.0.0.4:4300 y se maneja por CDP
//! (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9438`).
//! No instala nada: exige que `cm` ya esté instalado, así `instalar_cm` lo
//! reconoce y responde sin winget ni navegador.
use branchboard_lib::{
    cm::instalacion::resolver_cm, comandos::cm as comandos_cm,
    comandos::sistema as comandos_sistema, estado::AppState,
};

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    assert!(
        resolver_cm().is_some(),
        "esta prueba necesita `cm` instalado: no debe instalar nada"
    );
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.4:4300".parse().unwrap());
    for w in &mut contexto.config_mut().app.windows {
        w.visible = false;
        w.title = "Prueba aislada #123".into();
    }
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            comandos_sistema::ping,
            comandos_cm::estado_cm,
            comandos_cm::instalar_cm,
            terminar_prueba
        ])
        .run(contexto)
        .unwrap();
}
