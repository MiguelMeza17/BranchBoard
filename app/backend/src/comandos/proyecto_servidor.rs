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

/// `crear_proyecto_servidor(nombre)`: crea un proyecto sin Unity VC en el servidor, con la cuenta de la
/// sesión como Administrador, y lo deja abierto. Libre: todavía no hay proyecto ni rol.
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
    let sesion = cliente.sesion_vigente()?;
    let id = cliente
        .llamar(
            "POST",
            "/rest/v1/rpc/crear_proyecto",
            Some(&serde_json::json!({ "p_nombre": nombre })),
        )?
        .as_str()
        .map(String::from)
        .ok_or_else(|| Error::Interno("crear_proyecto no devolvió el proyecto".into()))?;
    abrir_con(
        estado,
        datos_app,
        cliente,
        &id,
        nombre,
        &sesion.correo,
        true,
    )
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
