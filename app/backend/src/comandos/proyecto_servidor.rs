//! Comandos de los proyectos sin Unity VC en el servidor (#240). Las reglas están en
//! `crate::proyecto_servidor`; aquí se juntan la sesión de BranchBoard, el servidor y el estado de la app.

use std::path::Path;

use tauri::{AppHandle, State};

use crate::comandos::acceso::{carpeta_datos, refrescar_equipo};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::Entrada;
use crate::modelo::hoy_local;
use crate::proyecto_servidor;
use crate::servidor::pat::Llavero;
use crate::servidor::LlaveroSistema;
use crate::supabase::{proyectos, ClienteSupabase, Entorno};

/// `crear_proyecto_servidor(nombre)`: crea localmente con UUID definitivo y cuenta guardada como
/// Administrador; el temporizador registra el proyecto antes de enviar eventos al recuperar la red.
#[tauri::command]
pub async fn crear_proyecto_servidor(
    app: AppHandle,
    estado: State<'_, AppState>,
    nombre: String,
) -> Resultado<Entrada> {
    let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
    crear_en(&estado, &carpeta_datos(&app)?, &cliente, &nombre)
}

/// `abrir_proyecto_servidor(id)`: abre un proyecto del servidor de los de la cuenta (de
/// `listar_proyectos_del_servidor`). En una PC sin sus datos los crea; sin red abre con la última copia.
#[tauri::command]
pub async fn abrir_proyecto_servidor(
    app: AppHandle,
    estado: State<'_, AppState>,
    id: String,
) -> Resultado<Entrada> {
    let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
    abrir_en(&estado, &carpeta_datos(&app)?, &cliente, &id)
}

/// Lo mismo que [`crear_proyecto_servidor`], sin Tauri (para la prueba en el entorno real).
pub fn crear_en<L: Llavero>(
    estado: &AppState,
    datos_app: &Path,
    cliente: &ClienteSupabase<L>,
    nombre: &str,
) -> Resultado<Entrada> {
    let nombre = nombre.trim();
    if nombre.is_empty() {
        return Err(Error::Validacion("Escribe el nombre del proyecto.".into()));
    }
    let sesion = cliente.sesion_guardada()?.ok_or(Error::SinSesionServidor)?;
    let db = proyecto_servidor::pendientes::crear(datos_app, nombre, &sesion)?;
    let entrada = proyecto_servidor::decidir(&db, &sesion.correo, true)?;
    estado.cerrar_proyecto();
    estado.fijar_usuario(Some(&sesion.correo));
    estado.abrir_proyecto(db);
    estado.fijar_workspace(None, None);
    Ok(entrada)
}

/// Lo mismo que [`abrir_proyecto_servidor`], sin Tauri.
pub fn abrir_en<L: Llavero>(
    estado: &AppState,
    datos_app: &Path,
    cliente: &ClienteSupabase<L>,
    id: &str,
) -> Resultado<Entrada> {
    // La sesión guardada alcanza para saber quién es, también sin red.
    let sesion = cliente.sesion_guardada()?.ok_or(Error::SinSesionServidor)?;
    if proyecto_servidor::pendientes::listar(datos_app, &sesion.usuario_id)?
        .iter()
        .any(|p| p.id == id)
    {
        let db = crate::db::Db::abrir(&proyecto_servidor::carpeta_datos(datos_app, id)?)?;
        if proyecto_servidor::pendientes::leer(db.conexion())?.is_some() {
            let entrada = proyecto_servidor::decidir(&db, &sesion.correo, false)?;
            estado.cerrar_proyecto();
            estado.fijar_usuario(Some(&sesion.correo));
            estado.abrir_proyecto(db);
            estado.fijar_workspace(None, None);
            return Ok(entrada);
        }
    }
    // El nombre, del servidor si hay red; si no, el que ya tenía la base.
    let nombre = proyectos::listar_mios(cliente)
        .ok()
        .and_then(|l| l.into_iter().find(|p| p.id == id))
        .map(|p| p.nombre)
        .unwrap_or_default();
    abrir_con(
        estado,
        datos_app,
        cliente,
        id,
        &nombre,
        &sesion.correo,
        false,
    )
}

fn abrir_con<L: Llavero>(
    estado: &AppState,
    datos_app: &Path,
    cliente: &ClienteSupabase<L>,
    id: &str,
    nombre: &str,
    email: &str,
    configurado_ahora: bool,
) -> Resultado<Entrada> {
    let db = proyecto_servidor::preparar(datos_app, id, nombre, email, &hoy_local(), |db| {
        refrescar_equipo(cliente, db, id)
    })?;
    let entrada = proyecto_servidor::decidir(&db, email, configurado_ahora)?;
    if matches!(entrada, Entrada::Dentro { .. }) {
        estado.cerrar_proyecto();
        estado.fijar_usuario(Some(email));
        estado.abrir_proyecto(db);
        estado.fijar_workspace(None, None);
        let _ = crate::comandos::sprints::evaluar_cierres_en(estado, email);
    }
    Ok(entrada)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventos::{bandeja, lectura, recepcion};
    use crate::modelo::{Rol, TipoTarea};
    use crate::tareas::{self, AutorTarea, DatosNuevaTarea};
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[test]
    fn alta_offline_no_necesita_token_vigente_y_no_cambia_de_cuenta() {
        use crate::proyecto_servidor::pendientes;
        use crate::supabase::{entorno::PRUEBAS, Sesion};
        let sesion = Sesion {
            access_token: "solo-prueba".into(),
            refresh_token: "solo-prueba".into(),
            expira_en: 0,
            usuario_id: "cuenta-a".into(),
            correo: "ana@virustf.dev".into(),
            nombre: "Ana".into(),
            apellido: String::new(),
        };
        let llavero = LlaveroMemoria::default();
        llavero
            .guardar(
                "supabase-branchboard-pruebas",
                &serde_json::to_string(&sesion).unwrap(),
            )
            .unwrap();
        let mut cliente = ClienteSupabase::new(&PRUEBAS, llavero);
        cliente.probar_sin_red(true);
        let datos = tempfile::tempdir().unwrap();
        let estado = AppState::default();
        crear_en(&estado, datos.path(), &cliente, "  Juego  ").unwrap();
        let listado = pendientes::listar(datos.path(), &sesion.usuario_id).unwrap();
        assert_eq!(listado.len(), 1);
        assert_eq!(listado[0].nombre, "Juego");
        let id = &listado[0].id;
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        let carpeta = proyecto_servidor::carpeta_datos(datos.path(), id).unwrap();
        assert!(!carpeta.join(crate::eventos::ARCHIVO_LOCAL).exists());
        assert!(!carpeta.join("eventos-locales").exists());
        estado.cerrar_proyecto();
        abrir_en(&estado, datos.path(), &cliente, id).unwrap();
        let otro = LlaveroMemoria::default();
        let otra_sesion = Sesion {
            usuario_id: "cuenta-b".into(),
            ..sesion
        };
        otro.guardar(
            "supabase-branchboard-pruebas",
            &serde_json::to_string(&otra_sesion).unwrap(),
        )
        .unwrap();
        let cliente_otro = ClienteSupabase::new(&PRUEBAS, otro);
        assert!(matches!(
            pendientes::subir(|f| estado.con_db(|db| f(db.conexion())), &cliente_otro),
            Err(Error::Validacion(_))
        ));
        assert!(estado
            .con_db(|db| pendientes::leer(db.conexion()))
            .unwrap()
            .is_some());
        assert!(crear_en(&estado, datos.path(), &cliente, "  ").is_err());
    }

    /// Cuentas nuevas, llavero en memoria y caída de red solo en este cliente.
    /// Conserva recursos remotos para inspección; no toca credenciales de la máquina.
    #[test]
    #[ignore = "requiere branchboard-pruebas"]
    fn entorno_real_crear_sin_red_269() {
        use crate::proyecto_servidor::pendientes;
        use crate::supabase::entorno::PRUEBAS;
        let hora = chrono::Utc::now().timestamp_millis();
        let mut cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        let sesion = cliente
            .registrar(
                "Prueba",
                "Offline",
                &format!("offline-{hora}@prueba.branchboard"),
                &format!("Prueba-269-{hora}-Aa!"),
            )
            .unwrap();
        let estado = AppState::default();
        let datos = tempfile::tempdir().unwrap();
        cliente.probar_sin_red(true);
        let entrada = crear_en(&estado, datos.path(), &cliente, "Proyecto sin red").unwrap();
        assert!(matches!(
            entrada,
            Entrada::Dentro {
                rol: Rol::Administrador,
                ..
            }
        ));
        let id = estado
            .con_db(|db| proyectos::proyecto_guardado(db.conexion()))
            .unwrap()
            .unwrap();
        let autor = AutorTarea {
            email: &sesion.correo,
            rol: Rol::Administrador,
        };
        let tarea = estado
            .con_db(|db| {
                tareas::crear(
                    db.conexion(),
                    &autor,
                    &DatosNuevaTarea {
                        titulo: "Creada sin conexión".into(),
                        detalles: "Historia conservada".into(),
                        tipo: TipoTarea::Scripting,
                        tarea_padre_id: None,
                        sprint_id: None,
                        backlog_id: None,
                        con_fecha: false,
                        fecha_limite: None,
                    },
                    &crate::modelo::ahora_iso(),
                    &hoy_local(),
                )
            })
            .unwrap();
        let cantidad = estado
            .con_db(|db| bandeja::pendientes(db.conexion()))
            .unwrap();
        assert!(cantidad > 0);
        let sin_red = crate::comandos::eventos::subir_bandeja_con(&estado, &cliente).unwrap();
        assert!(sin_red.error.is_some());
        assert_eq!(sin_red.pendientes, cantidad + 1);
        assert_eq!(sin_red.rechazados, 0);
        assert_eq!(
            estado
                .con_db(|db| bandeja::pendientes(db.conexion()))
                .unwrap(),
            cantidad
        );
        estado.cerrar_proyecto();
        assert_eq!(
            pendientes::listar(datos.path(), &sesion.usuario_id).unwrap()[0].id,
            id
        );
        assert!(pendientes::listar(datos.path(), "otra-cuenta")
            .unwrap()
            .is_empty());
        abrir_en(&estado, datos.path(), &cliente, &id).unwrap();
        assert_eq!(
            estado
                .con_db(|db| crate::db::tareas::listar(db.conexion()))
                .unwrap()[0]
                .id,
            tarea.id
        );
        cliente.probar_sin_red(false);
        // Simula una respuesta perdida: el servidor ya creó, pero SQLite sigue pendiente.
        let respuesta = cliente
            .llamar(
                "POST",
                "/rest/v1/rpc/crear_proyecto",
                Some(&serde_json::json!({"p_nombre":"Proyecto sin red", "p_id":id})),
            )
            .unwrap();
        assert_eq!(respuesta.as_str(), Some(id.as_str()));
        let subida = crate::comandos::eventos::subir_bandeja_con(&estado, &cliente).unwrap();
        assert_eq!(subida.error, None);
        assert_eq!(subida.rechazados, 0);
        assert_eq!(subida.pendientes, 0);
        assert_eq!(subida.subidos, cantidad);
        assert_eq!(
            pendientes::listar(datos.path(), &sesion.usuario_id).unwrap()[0].id,
            id
        );
        assert!(estado
            .con_db(|db| pendientes::leer(db.conexion()))
            .unwrap()
            .is_none());
        let otra_pc = AppState::default();
        let otros_datos = tempfile::tempdir().unwrap();
        abrir_en(&otra_pc, otros_datos.path(), &cliente, &id).unwrap();
        let filas = cliente
            .llamar("GET", &recepcion::ruta_eventos(&id, 0), None)
            .and_then(recepcion::filas_desde_json)
            .unwrap();
        assert_eq!(filas.len() as u64, cantidad);
        recepcion::bajar(
            |f| otra_pc.con_db(|db| f(db.conexion())),
            &id,
            |desde| {
                cliente
                    .llamar("GET", &recepcion::ruta_eventos(&id, desde), None)
                    .and_then(recepcion::filas_desde_json)
            },
            |ids| {
                cliente
                    .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                    .and_then(recepcion::correos_desde_json)
            },
        )
        .unwrap();
        otra_pc
            .con_db(|db| {
                recepcion::aplicar(
                    db.conexion(),
                    &id,
                    None,
                    chrono::Utc::now(),
                    lectura::aplicar_evento,
                    |_| Ok(false),
                )
            })
            .unwrap();
        let recibidas = otra_pc
            .con_db(|db| crate::db::tareas::listar(db.conexion()))
            .unwrap();
        assert_eq!(recibidas.len(), 1);
        assert_eq!(recibidas[0].id, tarea.id);
        assert_eq!(recibidas[0].titulo, "Creada sin conexión");
        assert!(proyectos::listar_mios(&cliente)
            .unwrap()
            .iter()
            .any(|p| p.id == id));
        cliente.cerrar_sesion().unwrap();
        println!("#269 OK: crear offline, tarea durable, reabrir offline, reintentar alta y subir sin duplicados; proyecto {id} en branchboard-pruebas");
    }

    #[derive(Default)]
    struct LlaveroMemoria(RefCell<HashMap<String, String>>);
    impl Llavero for LlaveroMemoria {
        fn leer(&self, cuenta: &str) -> Resultado<Option<String>> {
            Ok(self.0.borrow().get(cuenta).cloned())
        }
        fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()> {
            self.0.borrow_mut().insert(cuenta.into(), valor.into());
            Ok(())
        }
        fn borrar(&self, cuenta: &str) -> Resultado<()> {
            self.0.borrow_mut().remove(cuenta);
            Ok(())
        }
    }

    /// Una "PC": su estado, su carpeta de datos y la sesión de su cuenta.
    struct Pc {
        estado: AppState,
        datos: tempfile::TempDir,
        cliente: ClienteSupabase<LlaveroMemoria>,
        correo: String,
    }

    impl Pc {
        fn crear_tarea(&self, titulo: &str) -> String {
            let yo = self.estado.exigir_permiso("crear_tarea").unwrap();
            let autor = AutorTarea {
                email: &yo.email,
                rol: yo.rol,
            };
            let datos = DatosNuevaTarea {
                titulo: titulo.into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
            };
            self.estado
                .con_db(|db| {
                    tareas::crear(
                        db.conexion(),
                        &autor,
                        &datos,
                        &crate::modelo::ahora_iso(),
                        &hoy_local(),
                    )
                })
                .unwrap()
                .id
        }

        /// Sube la bandeja y baja y aplica lo nuevo, como una vuelta de la app (#238, #239).
        fn sincronizar(&self, proyecto: &str) -> usize {
            let r = bandeja::subir(
                |f| self.estado.con_db(|db| f(db.conexion())),
                |filas| self.cliente.insertar_eventos(filas),
            )
            .unwrap();
            assert_eq!(r.error, None);
            recepcion::bajar(
                |f| self.estado.con_db(|db| f(db.conexion())),
                proyecto,
                |desde| {
                    self.cliente
                        .llamar("GET", &recepcion::ruta_eventos(proyecto, desde), None)
                        .and_then(recepcion::filas_desde_json)
                },
                |ids| {
                    self.cliente
                        .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                        .and_then(recepcion::correos_desde_json)
                },
            )
            .unwrap();
            self.estado
                .con_db(|db| {
                    recepcion::aplicar(
                        db.conexion(),
                        proyecto,
                        None,
                        chrono::Utc::now(),
                        lectura::aplicar_evento,
                        |_| Ok(false),
                    )
                })
                .unwrap()
                .aplicados
        }

        fn titulos(&self) -> Vec<String> {
            let mut t: Vec<String> = self
                .estado
                .con_db(|db| crate::db::tareas::listar(db.conexion()))
                .unwrap()
                .into_iter()
                .map(|t| t.titulo)
                .collect();
            t.sort();
            t
        }
    }

    /// **Prueba en el entorno real** (#240): un proyecto **sin Unity VC** sincronizado entre dos PCs con
    /// dos cuentas, contra branchboard-pruebas. Registra `sin-unity-a|b-<hora>@prueba.branchboard` (clave
    /// aleatoria, no se imprime):
    /// 1. A crea el proyecto en su PC (queda Administradora) y agrega a B como Desarrollador;
    /// 2. A crea una tarea y la sube;
    /// 3. B, en una PC **sin datos**, abre el proyecto desde el servidor (Desarrollador), baja el historial
    ///    y ve la tarea;
    /// 4. B crea otra tarea y la sube; A la baja y la ve.
    ///
    /// ```text
    /// cargo test --lib entorno_real_sin_unity -- --ignored --nocapture
    /// ```
    /// Deja las cuentas y el proyecto: borrarlos después (ver BACKEND.md → Proyectos en el servidor).
    #[test]
    #[ignore = "toca branchboard-pruebas: correr a mano con --ignored"]
    fn entorno_real_sin_unity_entre_dos_pcs() {
        use crate::supabase::entorno::PRUEBAS;
        use rand::{distributions::Alphanumeric, Rng};

        let hora = chrono::Utc::now().format("%H%M%S%3f").to_string();
        let clave: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let pc = |quien: &str, nombre: &str| {
            let correo = format!("sin-unity-{quien}-{hora}@prueba.branchboard");
            let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
            cliente
                .registrar(nombre, "Prueba", &correo, &clave)
                .unwrap();
            Pc {
                estado: AppState::default(),
                datos: tempfile::tempdir().unwrap(),
                cliente,
                correo,
            }
        };
        let (a, b) = (pc("a", "Ana"), pc("b", "Beto"));

        // 1. A crea el proyecto y agrega a B.
        let entrada = crear_en(
            &a.estado,
            a.datos.path(),
            &a.cliente,
            &format!("bb-prueba-240-{hora}"),
        )
        .unwrap();
        assert!(
            matches!(
                entrada,
                Entrada::Dentro {
                    rol: Rol::Administrador,
                    configurado_ahora: true,
                    ..
                }
            ),
            "{entrada:?}"
        );
        let proyecto = a
            .estado
            .con_db(|db| proyectos::proyecto_guardado(db.conexion()))
            .unwrap()
            .expect("vinculado");
        proyecto_servidor::pendientes::subir(
            |f| a.estado.con_db(|db| f(db.conexion())),
            &a.cliente,
        )
        .unwrap();
        crate::supabase::miembros::agregar(&a.cliente, &proyecto, &b.correo, Rol::Desarrollador)
            .unwrap();

        // 2. A crea una tarea y la sube.
        a.crear_tarea("De Ana");
        a.sincronizar(&proyecto);

        // 3. B abre desde el servidor en una PC sin datos y ve la tarea de A.
        let entrada = abrir_en(&b.estado, b.datos.path(), &b.cliente, &proyecto).unwrap();
        assert!(
            matches!(
                entrada,
                Entrada::Dentro {
                    rol: Rol::Desarrollador,
                    configurado_ahora: false,
                    ..
                }
            ),
            "{entrada:?}"
        );
        assert_eq!(b.sincronizar(&proyecto), 1);
        assert_eq!(b.titulos(), vec!["De Ana".to_string()]);

        // 4. B crea otra tarea; A la recibe.
        b.crear_tarea("De Beto");
        b.sincronizar(&proyecto);
        assert_eq!(a.sincronizar(&proyecto), 1);
        assert_eq!(
            a.titulos(),
            vec!["De Ana".to_string(), "De Beto".to_string()]
        );

        a.cliente.cerrar_sesion().unwrap();
        b.cliente.cerrar_sesion().unwrap();
        println!(
            "proyecto sin Unity VC sincronizado entre dos PCs: {proyecto} de branchboard-pruebas"
        );
    }
}
