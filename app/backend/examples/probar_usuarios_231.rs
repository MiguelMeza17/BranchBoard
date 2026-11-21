//! Prueba de la pantalla Usuarios en Tauri con dos cuentas reales de branchboard-pruebas (#231).
//! A (Administrador) maneja la pantalla real; B se consulta con su propia sesión y su propia base.
//! BB231_DATOS debe ser una carpeta nueva fuera del repositorio. Vite: 127.0.0.9:4300.
//! WebView oculta con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9467`.
use branchboard_lib::{
    comandos::usuarios,
    db::{configuracion, usuarios as db_usuarios, Db},
    error::Resultado,
    estado::AppState,
    identidad::equipo::UsuarioVisible,
    modelo::Rol,
    servidor::pat::Llavero,
    supabase::{entorno::PRUEBAS, ClienteSupabase},
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
    a: ClienteSupabase<Memoria>,
    b: ClienteSupabase<Memoria>,
    estado_b: AppState,
    correo_a: String,
    correo_b: String,
    invitado: String,
    proyecto: String,
}

/// Una base local nueva vinculada al proyecto del servidor, con solo la fila de quien la abre.
fn base(proyecto: &str, correo: &str, rol: Rol) -> AppState {
    let db = Db::en_memoria().unwrap();
    let c = db.conexion();
    configuracion::guardar(
        c,
        configuracion::CLAVE_PROYECTO_SERVIDOR,
        proyecto,
        correo,
        "2026-11-21",
    )
    .unwrap();
    configuracion::guardar(
        c,
        configuracion::CLAVE_NOMBRE_PROYECTO,
        "Usuarios UI 231",
        correo,
        "2026-11-21",
    )
    .unwrap();
    db_usuarios::insertar(
        c,
        &db_usuarios::Usuario {
            email: correo.into(),
            nombre: "Prueba".into(),
            apellido: "Usuarios".into(),
            rol,
            fecha_alta: "2026-11-21".into(),
        },
    )
    .unwrap();
    let estado = AppState::default();
    estado.fijar_usuario(Some(correo));
    estado.abrir_proyecto(db);
    estado
}

#[tauri::command]
fn datos_prueba(ensayo: State<'_, Ensayo>) -> Value {
    json!({"a":ensayo.correo_a,"b":ensayo.correo_b,"invitado":ensayo.invitado,"proyecto":ensayo.proyecto})
}
#[tauri::command]
fn listar_usuarios(
    estado: State<'_, AppState>,
    ensayo: State<'_, Ensayo>,
) -> Resultado<Vec<UsuarioVisible>> {
    usuarios::listar_en(&estado, &ensayo.a)
}
#[tauri::command]
fn crear_usuario(
    estado: State<'_, AppState>,
    ensayo: State<'_, Ensayo>,
    email: String,
    rol: Rol,
) -> Resultado<Value> {
    let u = usuarios::crear_en(&estado, &ensayo.a, &email, rol)?;
    Ok(json!({"email":u.email,"rol":u.rol}))
}
#[tauri::command]
fn cambiar_rol(
    estado: State<'_, AppState>,
    ensayo: State<'_, Ensayo>,
    email: String,
    rol: Rol,
) -> Resultado<()> {
    usuarios::cambiar_rol_en(&estado, &ensayo.a, &email, rol)
}
#[tauri::command]
fn eliminar_usuario(
    estado: State<'_, AppState>,
    ensayo: State<'_, Ensayo>,
    email: String,
) -> Resultado<()> {
    usuarios::eliminar_en(&estado, &ensayo.a, &email)
}
/// Lo que ve B desde su propia PC (su sesión y su base), por el servidor.
#[tauri::command]
fn vista_de_b(ensayo: State<'_, Ensayo>) -> Result<Vec<UsuarioVisible>, String> {
    usuarios::listar_en(&ensayo.estado_b, &ensayo.b).map_err(|e| e.to_string())
}
/// B intenta cambiar el rol de A con su sesión: lo tiene que rechazar el servidor.
#[tauri::command]
fn b_intenta_degradar_a_a(ensayo: State<'_, Ensayo>) -> Result<(), String> {
    usuarios::cambiar_rol_en(&ensayo.estado_b, &ensayo.b, &ensayo.correo_a, Rol::Tester)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle, ensayo: State<'_, Ensayo>) {
    let _ = ensayo.a.cerrar_sesion();
    let _ = ensayo.b.cerrar_sesion();
    app.exit(0);
}

fn main() {
    let raiz = PathBuf::from(std::env::var("BB231_DATOS").expect("falta BB231_DATOS"));
    assert!(
        raiz.is_absolute() && !raiz.exists(),
        "usar una carpeta nueva"
    );
    std::fs::create_dir_all(&raiz).unwrap();
    let sufijo = chrono::Utc::now().timestamp_millis();
    let clave = format!("Aa7!{sufijo}-231-UI");
    let registrar = |letra: &str, nombre: &str| {
        let c = ClienteSupabase::new(&PRUEBAS, Memoria::default());
        let correo = format!("usuarios-231-ui-{letra}-{sufijo}@prueba.branchboard");
        let sesion = c.registrar(nombre, "Prueba", &correo, &clave).unwrap();
        (c, correo, sesion.usuario_id)
    };
    let (a, correo_a, id_a) = registrar("a", "Ana");
    let (b, correo_b, id_b) = registrar("b", "Beto");
    let invitado = format!("usuarios-231-ui-invitado-{sufijo}@prueba.branchboard");
    let proyecto = a
        .llamar(
            "POST",
            "/rest/v1/rpc/crear_proyecto",
            Some(&json!({"p_nombre":format!("bb-prueba-231-ui-{sufijo}")})),
        )
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    std::fs::write(
        raiz.join("recursos.json"),
        serde_json::to_vec_pretty(&json!({"usuarios":[id_a,id_b],"proyectos":[proyecto]})).unwrap(),
    )
    .unwrap();
    let estado = base(&proyecto, &correo_a, Rol::Administrador);
    let estado_b = base(&proyecto, &correo_b, Rol::Desarrollador);
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some("http://127.0.0.9:4300".parse().unwrap());
    for ventana in &mut contexto.config_mut().app.windows {
        ventana.visible = false;
        ventana.title = "BranchBoard usuarios #231".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .manage(Ensayo {
            a,
            b,
            estado_b,
            correo_a,
            correo_b,
            invitado,
            proyecto,
        })
        .invoke_handler(tauri::generate_handler![
            datos_prueba,
            listar_usuarios,
            crear_usuario,
            cambiar_rol,
            eliminar_usuario,
            vista_de_b,
            b_intenta_degradar_a_a,
            terminar_prueba
        ])
        .run(contexto)
        .expect("WebView de usuarios");
}
