//! Prueba del actualizador real, sin instalar ni abrir proyectos del usuario.
//! Ejecutar con `cargo run --example probar_actualizaciones_133 --locked`.
//! Descarga el instalador público de v0.1.0, verifica su firma y comprueba que
//! un instalador alterado se rechaza antes de ejecutarlo. No cambia el llavero.
use branchboard_lib::{comandos::actualizaciones, estado::AppState};
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

async fn comprobar(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let estado = app.state::<AppState>();
    let operacion = estado.iniciar_operacion();
    let bloqueo = actualizaciones::instalar_actualizacion(app.clone(), app.state())
        .await
        .expect_err("no debe instalar con una operación en curso");
    if bloqueo.codigo() != "validacion" || !bloqueo.to_string().contains("operación en curso") {
        return Err(format!("bloqueo inesperado: {bloqueo}").into());
    }
    drop(operacion);
    println!("OK: una operación en curso impide instalar antes de consultar la red");

    // Se fija una versión publicada: esta prueba funciona también en versiones posteriores.
    let actualizador = app
        .updater_builder()
        .endpoints(vec![
            "https://github.com/MiguelMeza17/BranchBoard/releases/download/v0.1.0/latest.json"
                .parse()?,
        ])?
        .version_comparator(|_, _| true)
        .build()?;
    let nueva = actualizador
        .check()
        .await?
        .ok_or("falta la versión publicada")?;
    if nueva.version != "0.1.0" {
        return Err("el manifiesto no corresponde a v0.1.0".into());
    }
    let mut recibido = 0;
    let mut bytes = nueva
        .download(|cantidad, _| recibido += cantidad, || {})
        .await?;
    if bytes.is_empty() || recibido != bytes.len() {
        return Err("descarga vacía o progreso incorrecto".into());
    }
    println!("OK: instalador descargado y firma verificada ({recibido} bytes)");

    let ultimo = bytes.last_mut().ok_or("descarga vacía")?;
    *ultimo ^= 1;
    match nueva.install(&bytes) {
        Err(tauri_plugin_updater::Error::Minisign(_)) => {
            println!("OK: instalador alterado rechazado por su firma, sin ejecutarlo");
        }
        Err(error) => return Err(format!("rechazo inesperado: {error}").into()),
        Ok(()) => return Err("se aceptó un instalador alterado".into()),
    }
    Ok(())
}

fn main() {
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().app.windows.clear();
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match comprobar(&handle).await {
                    Ok(()) => handle.exit(0),
                    Err(error) => {
                        eprintln!("FALLO: {error}");
                        handle.exit(1);
                    }
                }
            });
            Ok(())
        })
        .run(contexto)
        .expect("iniciar Tauri para probar el actualizador");
}
