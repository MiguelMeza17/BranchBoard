//! WebView oculta para #153: solo lectura de logs; no ejecuta merges ni cambia proyectos reales.
//! Frontend propio: http://127.0.0.6:4300/scripts/fixtures/logs153.html
use branchboard_lib::{
    comandos::logs_unity,
    db::{
        configuracion,
        tareas::{self, Tarea},
        usuarios, Db,
    },
    estado::AppState,
    modelo::{EstadoTarea, Rol, TipoTarea},
};

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    let db = Db::en_memoria().unwrap();
    let email = "revision153@example.invalid";
    // Copia local sintética de un proyecto de servidor: no hace ninguna llamada de red.
    configuracion::guardar(
        db.conexion(),
        configuracion::CLAVE_PROYECTO_SERVIDOR,
        "prueba-local-153",
        email,
        "2026-11-21",
    )
    .unwrap();
    usuarios::insertar(
        db.conexion(),
        &usuarios::Usuario {
            email: email.into(),
            nombre: "Prueba".into(),
            apellido: "Logs".into(),
            rol: Rol::Administrador,
            fecha_alta: "2026-11-21".into(),
        },
    )
    .unwrap();
    let mut tarea = Tarea::nueva(
        "T-153",
        "Revisar salto",
        TipoTarea::Scripting,
        email,
        "2026-11-21",
    );
    tarea.estado = EstadoTarea::EnRevision;
    tarea.rama_asociada = Some("/main/dev/T-153".into());
    tareas::insertar(db.conexion(), &tarea).unwrap();
    let estado = AppState::default();
    estado.abrir_proyecto(db);
    estado.fijar_usuario(Some(email));
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some(
        "http://127.0.0.6:4300/scripts/fixtures/logs153.html"
            .parse()
            .unwrap(),
    );
    for ventana in &mut contexto.config_mut().app.windows {
        ventana.visible = false;
        ventana.title = "Prueba aislada de logs #153".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .invoke_handler(tauri::generate_handler![
            logs_unity::logs_unity_para_terminar,
            terminar_prueba
        ])
        .run(contexto)
        .expect("iniciar WebView de prueba");
}
