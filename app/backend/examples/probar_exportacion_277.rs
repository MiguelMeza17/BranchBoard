//! Prueba de Ajustes en Tauri, con Supabase real y selector de destino controlado.
//! BB277_DATOS debe ser una carpeta nueva fuera del repositorio. Vite: 127.0.0.9:4300.
use branchboard_lib::{
    db::{configuracion, usuarios, Db},
    error::Resultado,
    estado::AppState,
    exportacion,
    modelo::Rol,
    proyecto_local,
    servidor::pat::Llavero,
    supabase::{entorno::PRUEBAS, ClienteSupabase},
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tauri::State;

#[derive(Clone, Default)]
struct Memoria(Arc<Mutex<HashMap<String, String>>>);
impl Llavero for Memoria {
    fn leer(&self, cuenta: &str) -> Resultado<Option<String>> {
        Ok(self.0.lock().unwrap().get(cuenta).cloned())
    }
    fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()> {
        self.0.lock().unwrap().insert(cuenta.into(), valor.into());
        Ok(())
    }
    fn borrar(&self, cuenta: &str) -> Resultado<()> {
        self.0.lock().unwrap().remove(cuenta);
        Ok(())
    }
}
struct Ensayo {
    cliente: ClienteSupabase<Memoria>,
    raiz: PathBuf,
    correo: String,
    proyecto: String,
    exportaciones: AtomicUsize,
}
#[tauri::command]
fn datos_prueba(ensayo: State<'_, Ensayo>) -> Value {
    json!({"correo":ensayo.correo,"proyecto":ensayo.proyecto})
}
#[tauri::command]
async fn exportar_proyecto_local(
    estado: State<'_, AppState>,
    ensayo: State<'_, Ensayo>,
) -> Resultado<Option<String>> {
    let _operacion = estado.iniciar_operacion();
    let archivo = ensayo.raiz.join("respaldo.bbproyecto");
    exportacion::exportar(&estado, &ensayo.cliente, &PRUEBAS, &archivo)?;
    ensayo.exportaciones.fetch_add(1, Ordering::SeqCst);
    Ok(Some(archivo.to_string_lossy().into_owned()))
}
#[tauri::command]
fn comprobar_respaldo(ensayo: State<'_, Ensayo>) -> Resultado<Value> {
    let archivo = ensayo.raiz.join("respaldo.bbproyecto");
    let importado = proyecto_local::importar(&archivo, ensayo.raiz.to_str().unwrap())?;
    let estado = AppState::default();
    proyecto_local::abrir(
        &estado,
        &ensayo.raiz.join("app-recuperada"),
        std::path::Path::new(&importado.ruta),
        false,
    )?;
    let desvinculado = estado.con_db(|db| {
        Ok(configuracion::leer(db.conexion(), configuracion::CLAVE_PROYECTO_SERVIDOR)?.is_none())
    })?;
    Ok(
        json!({"exportaciones":ensayo.exportaciones.load(Ordering::SeqCst),"archivo":archivo.is_file(),
        "desvinculado":desvinculado,"administrador_correcto":estado.usuario_actual().as_deref()==Some(ensayo.correo.as_str())}),
    )
}
#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle, ensayo: State<'_, Ensayo>) {
    let _ = ensayo.cliente.cerrar_sesion();
    app.exit(0);
}
fn main() {
    let raiz = PathBuf::from(std::env::var("BB277_DATOS").expect("falta BB277_DATOS"));
    assert!(
        raiz.is_absolute() && !raiz.exists(),
        "usar una carpeta nueva"
    );
    std::fs::create_dir_all(&raiz).unwrap();
    let cliente = ClienteSupabase::new(&PRUEBAS, Memoria::default());
    let sufijo = chrono::Utc::now().timestamp_millis();
    let correo = format!("exportar-277-ui-{sufijo}@prueba.branchboard");
    let sesion = cliente
        .registrar(
            "Prueba",
            "Exportar",
            &correo,
            &format!("Aa7!{sufijo}-277-UI"),
        )
        .unwrap();
    let proyecto = cliente
        .llamar(
            "POST",
            "/rest/v1/rpc/crear_proyecto",
            Some(&json!({"p_nombre":"Exportacion UI 277"})),
        )
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    std::fs::write(
        raiz.join("recursos.json"),
        serde_json::to_vec_pretty(&json!({"usuarios":[sesion.usuario_id],"proyectos":[proyecto]}))
            .unwrap(),
    )
    .unwrap();
    let db = Db::en_memoria().unwrap();
    configuracion::guardar(
        db.conexion(),
        configuracion::CLAVE_PROYECTO_SERVIDOR,
        &proyecto,
        &correo,
        "2026-11-21",
    )
    .unwrap();
    configuracion::guardar(
        db.conexion(),
        configuracion::CLAVE_NOMBRE_PROYECTO,
        "Exportacion UI 277",
        &correo,
        "2026-11-21",
    )
    .unwrap();
    usuarios::insertar(
        db.conexion(),
        &usuarios::Usuario {
            email: correo.clone(),
            nombre: "Prueba".into(),
            apellido: "Exportar".into(),
            rol: Rol::Administrador,
            fecha_alta: "2026-11-21".into(),
        },
    )
    .unwrap();
    let estado = AppState::default();
    estado.fijar_usuario(Some(&correo));
    estado.abrir_proyecto(db);
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.9:4300".parse().unwrap());
    for ventana in &mut contexto.config_mut().app.windows {
        ventana.visible = false;
        ventana.title = "BranchBoard exportacion #277".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .manage(Ensayo {
            cliente,
            raiz,
            correo,
            proyecto,
            exportaciones: AtomicUsize::new(0),
        })
        .invoke_handler(tauri::generate_handler![
            datos_prueba,
            exportar_proyecto_local,
            comprobar_respaldo,
            terminar_prueba
        ])
        .run(contexto)
        .expect("WebView de exportacion");
}
