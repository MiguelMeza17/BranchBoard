//! Dos cuentas reales en branchboard-pruebas y una WebView con la App real (#239).
//! Sesiones solo en memoria. Conserva el proyecto/cuentas de prueba y los datos locales.
use branchboard_lib::{
    comandos::{eventos::EVENTO_EVENTOS_RECIBIDOS, proyecto_servidor, tareas as comandos_tareas},
    db::usuarios,
    error::{Error, Resultado},
    estado::AppState,
    eventos::{self, bandeja, lectura, recepcion},
    modelo::{ahora_iso, hoy_local, Rol, TipoTarea},
    servidor::pat::Llavero,
    supabase::{entorno::PRUEBAS, miembros, proyectos, ClienteSupabase},
    tareas::{self, AutorTarea, CambiosTarea, DatosNuevaTarea},
};
use rand::{distributions::Alphanumeric, Rng};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager, State};

#[derive(Default)]
struct Memoria(Mutex<HashMap<String, String>>);
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
    autor: AppState,
    a: ClienteSupabase<Memoria>,
    b: ClienteSupabase<Memoria>,
    proyecto: String,
    tarea: Mutex<Option<String>>,
    detener: Arc<AtomicBool>,
}

#[tauri::command]
fn datos_prueba(estado: State<'_, AppState>, ensayo: State<'_, Ensayo>) -> Resultado<Value> {
    let s = ensayo
        .b
        .sesion_guardada()?
        .ok_or(Error::SinSesionServidor)?;
    Ok(
        json!({"correo":s.correo,"proyecto":ensayo.proyecto,"usuarios":estado.con_db(|db| usuarios::listar(db.conexion()))?}),
    )
}

#[tauri::command]
async fn cambio_remoto(ensayo: State<'_, Ensayo>, editar: bool) -> Resultado<()> {
    let yo = ensayo.autor.exigir_permiso("crear_tarea")?;
    let carpeta = ensayo.autor.con_db(|db| {
        db.carpeta()
            .map(Path::to_path_buf)
            .ok_or_else(|| Error::Validacion("falta carpeta de la PC emisora".into()))
    })?;
    let ops = eventos::carpeta_ops(&carpeta)?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    if editar {
        let id = ensayo
            .tarea
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Validacion("crea primero la tarea".into()))?;
        ensayo.autor.con_db(|db| {
            tareas::editar(
                db.conexion(),
                &ops,
                &autor,
                &id,
                &CambiosTarea {
                    titulo: Some("Cambio remoto 239 editado".into()),
                    ..Default::default()
                },
                &ahora_iso(),
            )
        })?;
    } else {
        let datos = DatosNuevaTarea {
            titulo: "Cambio remoto 239 nuevo".into(),
            detalles: "Creado por otra cuenta real en branchboard-pruebas".into(),
            tipo: TipoTarea::Scripting,
            tarea_padre_id: None,
            sprint_id: None,
            backlog_id: None,
            con_fecha: false,
            fecha_limite: None,
        };
        let nueva = ensayo.autor.con_db(|db| {
            tareas::crear(
                db.conexion(),
                &ops,
                &autor,
                &datos,
                &ahora_iso(),
                &hoy_local(),
            )
        })?;
        *ensayo.tarea.lock().unwrap() = Some(nueva.id);
    }
    let r = bandeja::subir(
        |f| ensayo.autor.con_db(|db| f(db.conexion())),
        |filas| ensayo.a.insertar_eventos(filas),
    )?;
    if r.error.is_some() || r.rechazados > 0 {
        return Err(Error::Interno(
            "no se pudo subir el cambio de prueba".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
fn terminar_prueba(app: tauri::AppHandle) {
    app.exit(0);
}

fn preparar(datos: &Path) -> Resultado<(AppState, Ensayo)> {
    let hora = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let clave: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let a = ClienteSupabase::new(&PRUEBAS, Memoria::default());
    let b = ClienteSupabase::new(&PRUEBAS, Memoria::default());
    let correo_a = format!("ui239-a-{hora}@prueba.branchboard");
    let correo_b = format!("ui239-b-{hora}@prueba.branchboard");
    std::fs::create_dir_all(datos)?;
    std::fs::write(
        datos.join("cuentas-de-prueba.json"),
        serde_json::to_vec_pretty(
            &json!({"entorno":"branchboard-pruebas","cuentas":[correo_a,correo_b]}),
        )
        .unwrap(),
    )?;
    a.registrar("Cuenta A", "Prueba", &correo_a, &clave)?;
    b.registrar("Cuenta B", "Prueba", &correo_b, &clave)?;
    let autor = AppState::default();
    proyecto_servidor::crear_en(
        &autor,
        &datos.join("a"),
        &a,
        &format!("bb-prueba-239-ui-{hora}"),
    )?;
    let proyecto = autor
        .con_db(|db| proyectos::proyecto_guardado(db.conexion()))?
        .ok_or_else(|| Error::Interno("proyecto sin vincular".into()))?;
    branchboard_lib::proyecto_servidor::pendientes::subir(
        |f| autor.con_db(|db| f(db.conexion())),
        &a,
    )?;
    miembros::agregar(&a, &proyecto, &correo_b, Rol::Administrador)?;
    let receptor = AppState::default();
    proyecto_servidor::abrir_en(&receptor, &datos.join("b"), &b, &proyecto)?;
    // Solo metadatos de recursos de prueba, nunca contraseñas ni tokens.
    std::fs::create_dir_all(datos)?;
    std::fs::write(
        datos.join("recursos.json"),
        serde_json::to_vec_pretty(&json!({"proyecto":proyecto,"entorno":"branchboard-pruebas"}))
            .unwrap(),
    )?;
    Ok((
        receptor,
        Ensayo {
            autor,
            a,
            b,
            proyecto,
            tarea: Mutex::new(None),
            detener: Arc::new(AtomicBool::new(false)),
        },
    ))
}

fn recibir(app: &tauri::AppHandle) -> Resultado<usize> {
    let estado = app.state::<AppState>();
    let ensayo = app.state::<Ensayo>();
    recepcion::bajar(
        |f| estado.con_db(|db| f(db.conexion())),
        &ensayo.proyecto,
        |desde| {
            ensayo
                .b
                .llamar(
                    "GET",
                    &recepcion::ruta_eventos(&ensayo.proyecto, desde),
                    None,
                )
                .and_then(recepcion::filas_desde_json)
        },
        |ids| {
            ensayo
                .b
                .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                .and_then(recepcion::correos_desde_json)
        },
    )?;
    let resumen = estado.con_db(|db| {
        recepcion::aplicar(
            db.conexion(),
            &ensayo.proyecto,
            None,
            chrono::Utc::now(),
            lectura::aplicar_evento,
            |_| Ok(false),
        )
    })?;
    let n = resumen.aplicados;
    if resumen.hubo_cambios() {
        app.emit(EVENTO_EVENTOS_RECIBIDOS, &resumen)
            .map_err(|e| Error::Interno(e.to_string()))?;
    }
    Ok(n)
}

fn main() {
    let datos =
        PathBuf::from(std::env::var("BB239_DATOS").expect("falta carpeta propia BB239_DATOS"));
    let (estado, ensayo) =
        preparar(&datos).expect("preparar las dos cuentas en branchboard-pruebas");
    let mut contexto = tauri::generate_context!();
    contexto.config_mut().build.dev_url = Some(
        "http://127.0.0.7:4300/scripts/fixtures/refresco239.html"
            .parse()
            .unwrap(),
    );
    for w in &mut contexto.config_mut().app.windows {
        w.visible = false;
        w.title = "Prueba aislada #239".into();
    }
    tauri::Builder::default()
        .manage(estado)
        .manage(ensayo)
        .setup(|app| {
            let handle = app.handle().clone();
            let detener = app.state::<Ensayo>().detener.clone();
            std::thread::spawn(move || {
                while !detener.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    if detener.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Err(e) = recibir(&handle) {
                        eprintln!("fallo de recepción de prueba: {e}");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            datos_prueba,
            cambio_remoto,
            terminar_prueba,
            comandos_tareas::listar_tareas
        ])
        .build(contexto)
        .unwrap()
        .run(|app, evento| {
            if matches!(evento, tauri::RunEvent::Exit) {
                let e = app.state::<Ensayo>();
                e.detener.store(true, Ordering::SeqCst);
                let _ = e.a.cerrar_sesion();
                let _ = e.b.cerrar_sesion();
            }
        });
}
