//! Ventana Tauri aislada para #203: CLI real en lectura y caducidad reproducida.
//! El SSO se sustituye por una respuesta controlada: nunca abre el navegador ni
//! modifica credenciales. Frontend en http://127.0.0.3:4300 y CDP en el puerto elegido.
use std::sync::atomic::{AtomicUsize, Ordering};

use branchboard_lib::{
    cm::{lectura, sesion, Comando},
    comandos::cm as comandos_cm,
    error::{Error, Resultado},
    estado::AppState,
};
use serde::Serialize;
use tauri::{Emitter, State};

struct Prueba {
    org: String,
    repo: String,
    inicios: AtomicUsize,
    reconexiones: AtomicUsize,
}

fn verificar_cli(estado: &AppState, prueba: &Prueba) -> Resultado<()> {
    let consulta = format!("on repository '{}@{}@unity'", prueba.repo, prueba.org);
    let ramas = Comando::cm()
        .args(["find", "branch", &consulta, "--format={name}", "--nototal"])
        .exigir_exito(estado)?;
    if !ramas.lines().any(|r| r.trim() == "/main/dev") {
        return Err(Error::Validacion(
            "Falta dev en el repositorio de pruebas".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
fn simular_caducidad(
    app: tauri::AppHandle,
    estado: State<'_, AppState>,
    segundo_plano: bool,
) -> Resultado<()> {
    estado.fijar_usuario(None);
    if segundo_plano {
        app.emit(sesion::EVENTO_SESION_CADUCADA, ()).unwrap();
        Ok(())
    } else {
        Err(Error::SesionUnityVcCaducada)
    }
}

#[tauri::command]
fn iniciar_sesion_unity_vc(
    estado: State<'_, AppState>,
    prueba: State<'_, Prueba>,
    forzar: bool,
) -> Resultado<sesion::EstadoSesionUnityVc> {
    if !forzar {
        return Err(Error::Validacion(
            "La sesión caducada debe pedir un nuevo SSO".into(),
        ));
    }
    prueba.inicios.fetch_add(1, Ordering::SeqCst);
    let email = lectura::whoami(&estado)?;
    estado.fijar_usuario(Some(&email));
    Ok(sesion::EstadoSesionUnityVc {
        activa: true,
        email: Some(email),
        abrio_asistente: false,
    })
}

#[tauri::command]
fn reconectar_unity_vc(
    estado: State<'_, AppState>,
    prueba: State<'_, Prueba>,
    org: Option<String>,
) -> Resultado<sesion::ResultadoReconexionUnityVc> {
    if org.as_ref().is_some_and(|o| o != &prueba.org) {
        return Err(Error::Validacion(
            "Organización distinta de la prueba".into(),
        ));
    }
    verificar_cli(&estado, &prueba)?;
    prueba.reconexiones.fetch_add(1, Ordering::SeqCst);
    let email = lectura::whoami(&estado)?;
    estado.fijar_usuario(Some(&email));
    Ok(sesion::ResultadoReconexionUnityVc {
        email,
        reconectado: true,
        abrio_asistente: false,
        mensaje: "Conexión de prueba verificada".into(),
    })
}

#[derive(Serialize)]
struct Contadores {
    inicios: usize,
    reconexiones: usize,
    identidad: bool,
}

#[tauri::command]
fn contadores(estado: State<'_, AppState>, prueba: State<'_, Prueba>) -> Contadores {
    Contadores {
        inicios: prueba.inicios.load(Ordering::SeqCst),
        reconexiones: prueba.reconexiones.load(Ordering::SeqCst),
        identidad: estado.usuario_actual().is_some(),
    }
}

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    let prueba = Prueba {
        org: std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG"),
        repo: std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO"),
        inicios: AtomicUsize::new(0),
        reconexiones: AtomicUsize::new(0),
    };
    assert!(prueba.repo.starts_with("BranchBoard-Pruebas/"));
    let estado = AppState::default();
    verificar_cli(&estado, &prueba).expect("la CLI real debe conectar con BranchBoard-Pruebas");
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.3:4300".parse().unwrap());
    for ventana in &mut contexto.config_mut().app.windows {
        ventana.visible = false;
        ventana.title = "BranchBoard prueba aislada #203".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .manage(prueba)
        .invoke_handler(tauri::generate_handler![
            comandos_cm::estado_cm,
            comandos_cm::sesion_unity_vc,
            simular_caducidad,
            iniciar_sesion_unity_vc,
            reconectar_unity_vc,
            contadores,
            terminar_prueba,
        ])
        .run(contexto)
        .expect("no se pudo abrir la WebView de prueba");
}
