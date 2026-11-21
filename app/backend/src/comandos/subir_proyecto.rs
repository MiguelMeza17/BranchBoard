//! "Subir este proyecto al servidor" (#245): pasa un proyecto que ya existía (de Unity VC o por carpeta)
//! a Supabase sin perder sus datos. Solo un Administrador del proyecto.
//!
//! 1. Crea el proyecto en el servidor: de Unity VC con `vincular-repo-unity` (#237, revisa que sea el dueño
//!    del repo); por carpeta con `rpc/crear_proyecto`. **Gana el primero** (decisión del 2026-11-20 en
//!    #245): si el repo ya está en el servidor con datos, o lo trajo otra persona, no se mezcla nada y se
//!    pide acceso a un Administrador.
//! 2. Sube el equipo con sus roles (`rpc/agregar_miembro`, #241): quien no tiene cuenta queda invitado.
//! 3. Copia los documentos y la imagen del proyecto de la carpeta compartida a Storage (#242).
//! 4. Manda la **foto del estado** (`importar_proyecto`, ver `eventos::importacion`; decisión del
//!    2026-11-21 en #245) y vincula la base local (`proyecto_servidor_id`). Desde ahí sigue con la bandeja
//!    (#238) y la recepción (#239).
//!
//! Si algo falla a medias se puede repetir: el id del proyecto creado queda en `subida_pendiente_id` y se
//! reutiliza (no se crea otro), agregar a alguien que ya está no es error y los archivos que ya están no
//! se vuelven a subir.

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::comandos::acceso::traer_repo;
use crate::comandos::datos::carpeta_abierta;
use crate::db::configuracion::{self, CLAVE_ICONO_PROYECTO, CLAVE_NOMBRE_PROYECTO};
use crate::db::{documentos as db_documentos, usuarios};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos::{self, importacion};
use crate::identidad::acceso::RepoUnityVc;
use crate::modelo::{ahora_iso, hoy_local, normalizar_email};
use crate::servidor::pat::Llavero;
use crate::servidor::LlaveroSistema;
use crate::supabase::storage::{self, Carpeta, Storage};
use crate::supabase::{miembros, proyectos, ClienteSupabase, Entorno, ProyectoSupabase};

/// Id del proyecto del servidor creado por una subida que no terminó (para no crear otro al repetir).
pub const CLAVE_SUBIDA_PENDIENTE: &str = "subida_pendiente_id";

/// Cómo quedó la subida.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ResumenSubida {
    pub proyecto_id: String,
    /// Agregados con su rol (ya tenían cuenta).
    pub miembros: usize,
    /// Invitados (todavía no tienen cuenta; entran al registrarse con ese correo).
    pub invitados: usize,
    /// Filas de la foto del estado (tareas, sprints, mensajes…).
    pub filas: usize,
    /// Documentos copiados a Storage.
    pub documentos: usize,
    /// Documentos que no estaban en la carpeta compartida de esta PC (quedan en la lista sin archivo).
    pub documentos_sin_archivo: usize,
}

/// `proyecto_en_el_servidor()`: el id del proyecto abierto en el servidor, o `null` si todavía no está
/// (para mostrar "Subir este proyecto al servidor").
#[tauri::command]
pub fn proyecto_en_el_servidor(estado: State<'_, AppState>) -> Resultado<Option<String>> {
    estado.exigir_permiso("proyecto_en_el_servidor")?;
    estado.con_db(|db| proyectos::proyecto_guardado(db.conexion()))
}

/// `subir_proyecto_al_servidor()`: solo Administrador. Ver el inicio del módulo.
#[tauri::command]
pub async fn subir_proyecto_al_servidor(estado: State<'_, AppState>) -> Resultado<ResumenSubida> {
    let supabase = Entorno::actual().proyecto();
    let cliente = ClienteSupabase::new(supabase, LlaveroSistema);
    subir_en(&estado, &cliente, supabase, |repo, email| {
        traer_repo(&estado, &cliente, repo, email)
    })
}

/// Lo mismo que el comando, con el cliente que se pase. `traer_unity(repo, email)` trae un repo de Unity
/// VC al servidor (en la app, `vincular-repo-unity`).
pub fn subir_en<L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    supabase: &'static ProyectoSupabase,
    traer_unity: impl FnOnce(&RepoUnityVc, &str) -> Resultado<String>,
) -> Resultado<ResumenSubida> {
    let yo = estado.exigir_permiso("subir_proyecto_al_servidor")?;
    let carpeta = carpeta_abierta(estado)?;
    if estado
        .con_db(|db| proyectos::proyecto_guardado(db.conexion()))?
        .is_some()
    {
        return Err(Error::Validacion(
            "Este proyecto ya está en el servidor.".into(),
        ));
    }
    let sesion = cliente.sesion_vigente()?;
    if normalizar_email(&sesion.correo) != normalizar_email(&yo.email) {
        return Err(Error::Validacion(format!(
            "Para subir el proyecto, inicia sesión en BranchBoard con {} (su Administrador).",
            yo.email
        )));
    }

    // 1. El proyecto en el servidor (o el de una subida que no terminó).
    let pendiente =
        estado.con_db(|db| configuracion::leer(db.conexion(), CLAVE_SUBIDA_PENDIENTE))?;
    let proyecto_id = match pendiente.filter(|p| !p.trim().is_empty()) {
        Some(id) => id,
        None => {
            let id = match estado.repo_abierto() {
                Ok(repo) => traer_unity(&repo, &yo.email).map_err(|e| match e {
                    Error::Validacion(m) if m.contains("ya está en BranchBoard") => ya_esta(),
                    otro => otro,
                })?,
                Err(_) => {
                    let nombre = estado
                        .con_db(|db| configuracion::leer(db.conexion(), CLAVE_NOMBRE_PROYECTO))?
                        .unwrap_or_else(|| "Proyecto".into());
                    cliente
                        .llamar(
                            "POST",
                            "/rest/v1/rpc/crear_proyecto",
                            Some(&serde_json::json!({ "p_nombre": nombre })),
                        )?
                        .as_str()
                        .map(String::from)
                        .ok_or_else(|| {
                            Error::Interno("crear_proyecto no devolvió el proyecto".into())
                        })?
                }
            };
            // Gana el primero: si el servidor ya tiene datos de ese proyecto, no se mezclan.
            let con_datos = cliente.llamar(
                "GET",
                &format!("/rest/v1/eventos?select=orden&proyecto_id=eq.{id}&limit=1"),
                None,
            )?;
            if con_datos.as_array().is_some_and(|a| !a.is_empty()) {
                return Err(ya_esta());
            }
            estado.con_db(|db| {
                configuracion::guardar(
                    db.conexion(),
                    CLAVE_SUBIDA_PENDIENTE,
                    &id,
                    &yo.email,
                    &hoy_local(),
                )
            })?;
            id
        }
    };
    let mut resumen = ResumenSubida {
        proyecto_id: proyecto_id.clone(),
        ..Default::default()
    };

    // 2. El equipo, con los roles de esta base (desde #243 sin firmas que verificar).
    let equipo = estado.con_db(|db| usuarios::listar(db.conexion()))?;
    let ya_en_el_servidor = miembros::leer_equipo(cliente, &proyecto_id)?;
    for u in equipo {
        let rol = u.rol;
        let correo = normalizar_email(&u.email);
        if correo == normalizar_email(&yo.email)
            || ya_en_el_servidor.id_de(&correo).is_some()
            || ya_en_el_servidor.esta_invitado(&correo)
        {
            continue;
        }
        match miembros::agregar(cliente, &proyecto_id, &correo, rol)? {
            Some(_) => resumen.miembros += 1,
            None => resumen.invitados += 1,
        }
    }

    // 3. Documentos e imagen, de la carpeta compartida a Storage.
    let servidor = Storage::new(cliente, supabase);
    if let Ok(docs) = eventos::carpeta_documentos(&carpeta) {
        let lista = estado.con_db(|db| db_documentos::listar(db.conexion()))?;
        for d in lista {
            let origen = docs.join(&d.ruta);
            let Ok(bytes) = std::fs::read(&origen) else {
                resumen.documentos_sin_archivo += 1;
                continue;
            };
            let ruta = storage::ruta_objeto(&proyecto_id, Carpeta::Documentos, &d.ruta)?;
            subir_si_falta(&servidor, &ruta, &bytes)?;
            resumen.documentos += 1;
        }
    }
    let icono = estado.con_db(|db| configuracion::leer(db.conexion(), CLAVE_ICONO_PROYECTO))?;
    if let (Some(icono), Ok(dir)) = (
        icono.filter(|i| !i.is_empty()),
        eventos::carpeta_imagen_proyecto(&carpeta),
    ) {
        if let Ok(bytes) = std::fs::read(dir.join(&icono)) {
            let ruta = storage::ruta_objeto(&proyecto_id, Carpeta::Proyecto, &icono)?;
            subir_si_falta(&servidor, &ruta, &bytes)?;
        }
    }

    // 4. La foto del estado y, en la misma vuelta, la base queda vinculada.
    let foto = estado.con_db(|db| importacion::foto(db.conexion()))?;
    resumen.filas = importacion::filas_en(&foto);
    let id_evento = format!("importar-{}", eventos::uuid());
    cliente.insertar_eventos(&serde_json::json!([{
        "id": id_evento,
        "proyecto_id": proyecto_id,
        "tipo": importacion::TIPO,
        "tarea_id": null,
        "payload": foto,
        "creado_en_app": ahora_iso(),
    }]))?;
    estado.con_db(|db| vincular(db.conexion(), &proyecto_id, &id_evento, &yo.email))?;
    Ok(resumen)
}

fn ya_esta() -> Error {
    Error::Validacion(
        "Este proyecto ya está en el servidor (lo subió otra persona). Pídele acceso a uno de sus \
         Administradores."
            .into(),
    )
}

/// Sube `bytes` a `ruta` si todavía no está (una subida repetida no falla por los que ya subieron).
fn subir_si_falta<L: Llavero>(
    servidor: &Storage<'_, L>,
    ruta: &str,
    bytes: &[u8],
) -> Resultado<()> {
    match servidor.subir(ruta, bytes) {
        Err(Error::Validacion(m)) if m.to_lowercase().contains("ya existe") => Ok(()),
        otro => otro,
    }
}

fn vincular(c: &Connection, proyecto_id: &str, id_evento: &str, email: &str) -> Resultado<()> {
    let tx = c.unchecked_transaction()?;
    configuracion::guardar(
        &tx,
        configuracion::CLAVE_PROYECTO_SERVIDOR,
        proyecto_id,
        email,
        &hoy_local(),
    )?;
    tx.execute(
        "DELETE FROM configuracion_proyecto WHERE clave = ?1",
        [CLAVE_SUBIDA_PENDIENTE],
    )?;
    // La foto salió de esta base: aquí no se aplica otra vez.
    eventos::marcar_aplicado(&tx, id_evento, &ahora_iso())?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::usuarios::Usuario;
    use crate::eventos::{lectura, recepcion};
    use crate::modelo::{Rol, TipoTarea};
    use crate::supabase::entorno::PRUEBAS;
    use crate::tareas::{self, AutorTarea, DatosNuevaTarea};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::Path;

    #[derive(Clone, Default)]
    struct LlaveroMemoria(std::rc::Rc<RefCell<HashMap<String, String>>>);
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

    fn usuario(email: &str, rol: Rol) -> Usuario {
        Usuario {
            email: email.into(),
            nombre: "N".into(),
            apellido: String::new(),
            rol,
            fecha_alta: "2026-11-01".into(),
        }
    }

    /// Un proyecto "de antes" en `carpeta`: su equipo, base sin vincular y su carpeta local de `ops/` y
    /// documentos (desde #243 sin llaves ni firmas). Lo deja abierto en `estado` con `admin` como usuario.
    fn proyecto_viejo(
        estado: &AppState,
        carpeta: &Path,
        nombre: &str,
        admin: &str,
        equipo: &[(&str, Rol)],
    ) {
        let db = crate::db::Db::abrir(carpeta).unwrap();
        let c = db.conexion();
        configuracion::guardar(c, CLAVE_NOMBRE_PROYECTO, nombre, admin, "2026-11-01").unwrap();
        usuarios::insertar(c, &usuario(admin, Rol::Administrador)).unwrap();
        for (email, rol) in equipo {
            usuarios::insertar(c, &usuario(email, *rol)).unwrap();
        }
        estado.cerrar_proyecto();
        estado.fijar_usuario(Some(admin));
        estado.abrir_proyecto(db);
    }

    fn crear_tarea(estado: &AppState, titulo: &str) {
        let carpeta = carpeta_abierta(estado).unwrap();
        let yo = estado.exigir_permiso("crear_tarea").unwrap();
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
        estado
            .con_db(|db| {
                tareas::crear(
                    db.conexion(),
                    &autor,
                    &datos,
                    &ahora_iso(),
                    &hoy_local(),
                )
            })
            .unwrap();
    }

    fn registrar(correo: &str, clave: &str) -> ClienteSupabase<LlaveroMemoria> {
        let c = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        c.registrar("Prueba", "245", correo, clave).unwrap();
        c
    }

    fn clave() -> String {
        use rand::{distributions::Alphanumeric, Rng};
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect()
    }

    /// **Prueba en el entorno real** (#245): subir un proyecto **de prueba** "por carpeta" con equipo,
    /// tareas y un documento, contra branchboard-pruebas, con dos cuentas
    /// (`subir-245-a|b|c-<hora>@prueba.branchboard`; C no se registra):
    /// 1. A (Administradora) lo sube: B queda miembro, C invitada, el documento en Storage;
    /// 2. B, en una PC sin datos, lo abre desde el servidor y recibe la foto: ve las tareas con su autor
    ///    original (no A) y el documento, que se baja de Storage;
    /// 3. subirlo otra vez desde A dice que ya está en el servidor.
    ///
    /// ```text
    /// cargo test --lib entorno_real_subir_proyecto_por_carpeta -- --ignored --nocapture
    /// ```
    /// Deja las cuentas, el proyecto y sus archivos: borrarlos después (ver BACKEND.md).
    #[test]
    #[ignore = "toca branchboard-pruebas: correr a mano con --ignored"]
    fn entorno_real_subir_proyecto_por_carpeta() {
        let hora = chrono::Utc::now().format("%H%M%S%3f").to_string();
        let correo = |q: &str| format!("subir-245-{q}-{hora}@prueba.branchboard");
        let (ca, cb, cc) = (correo("a"), correo("b"), correo("c"));
        let clave = clave();
        let cliente_a = registrar(&ca, &clave);
        let cliente_b = registrar(&cb, &clave);

        // El proyecto de antes, en la PC de A: B crea una tarea (cuando estaba en el equipo) y A otra.
        let estado_a = AppState::default();
        let pc_a = tempfile::tempdir().unwrap();
        proyecto_viejo(
            &estado_a,
            pc_a.path(),
            &format!("bb-prueba-245-{hora}"),
            &ca,
            &[(&cb, Rol::Desarrollador), (&cc, Rol::Tester)],
        );
        estado_a.fijar_usuario(Some(&cb));
        crear_tarea(&estado_a, "De Beto");
        estado_a.fijar_usuario(Some(&ca));
        crear_tarea(&estado_a, "De Ana");
        let docs = eventos::carpeta_documentos(pc_a.path()).unwrap();
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("D-245.txt"), b"documento de prueba 245").unwrap();
        estado_a
            .con_db(|db| {
                db.conexion().execute(
                    "INSERT INTO documentos (id, nombre, tamano, subido_por, fecha, pendiente, ruta)
                     VALUES ('D-1', 'guia.txt', 23, ?1, '2026-11-02', 0, 'D-245.txt')",
                    [&cb],
                )?;
                Ok(())
            })
            .unwrap();

        // 1. A lo sube.
        let r = subir_en(&estado_a, &cliente_a, &PRUEBAS, |_, _| {
            panic!("no es de Unity VC")
        })
        .unwrap();
        assert_eq!(
            (
                r.miembros,
                r.invitados,
                r.documentos,
                r.documentos_sin_archivo
            ),
            (1, 1, 1, 0),
            "{r:?}"
        );
        assert!(r.filas >= 3, "{r:?}");
        let proyecto = r.proyecto_id.clone();

        // 2. B, en una PC sin datos.
        let estado_b = AppState::default();
        let pc_b = tempfile::tempdir().unwrap();
        let entrada = crate::comandos::proyecto_servidor::abrir_en(
            &estado_b,
            pc_b.path(),
            &cliente_b,
            &proyecto,
        )
        .unwrap();
        assert!(
            matches!(
                entrada,
                crate::identidad::acceso::Entrada::Dentro {
                    rol: Rol::Desarrollador,
                    ..
                }
            ),
            "{entrada:?}"
        );
        recepcion::bajar(
            |f| estado_b.con_db(|db| f(db.conexion())),
            &proyecto,
            |desde| {
                cliente_b
                    .llamar("GET", &recepcion::ruta_eventos(&proyecto, desde), None)
                    .and_then(recepcion::filas_desde_json)
            },
            |ids| {
                cliente_b
                    .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                    .and_then(recepcion::correos_desde_json)
            },
        )
        .unwrap();
        let r_b = estado_b
            .con_db(|db| {
                recepcion::aplicar(
                    db.conexion(),
                    &proyecto,
                    None,
                    chrono::Utc::now(),
                    lectura::aplicar_evento,
                    |_| Ok(false),
                )
            })
            .unwrap();
        assert_eq!(r_b.aplicados, 1, "la foto");
        let mut tareas_b: Vec<(String, String)> = estado_b
            .con_db(|db| crate::db::tareas::listar(db.conexion()))
            .unwrap()
            .into_iter()
            .map(|t| (t.titulo, t.creado_por))
            .collect();
        tareas_b.sort();
        assert_eq!(
            tareas_b,
            vec![
                ("De Ana".to_string(), ca.clone()),
                ("De Beto".to_string(), cb.clone())
            ],
            "con sus autores originales"
        );
        assert_eq!(
            estado_b
                .con_db(|db| db_documentos::listar(db.conexion()))
                .unwrap()
                .len(),
            1
        );
        let bajado = Storage::new(&cliente_b, &PRUEBAS)
            .bajar(&storage::ruta_objeto(&proyecto, Carpeta::Documentos, "D-245.txt").unwrap())
            .unwrap();
        assert_eq!(bajado, b"documento de prueba 245");

        // 3. Ya está: no se sube otra vez.
        let e = subir_en(&estado_a, &cliente_a, &PRUEBAS, |_, _| {
            panic!("no llega aquí")
        })
        .unwrap_err();
        assert!(e.to_string().contains("ya está en el servidor"), "{e}");

        cliente_a.cerrar_sesion().unwrap();
        cliente_b.cerrar_sesion().unwrap();
        println!("proyecto subido: {proyecto} de branchboard-pruebas");
    }

    /// **Prueba en el entorno real** (#245): **gana el primero**. El mismo repo de Unity VC, en dos PCs de
    /// su dueña, cada una con datos distintos y sin vincular: la primera lo sube; la segunda recibe "ya
    /// está en el servidor" y en el servidor queda solo la foto de la primera. Con la sesión de Unity VC
    /// de la dueña del repo de pruebas.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_gana_el_primero -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "toca branchboard-pruebas y Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_gana_el_primero() {
        use crate::supabase::login_unity;
        let repo = RepoUnityVc {
            repo: std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO"),
            org: std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG"),
        };
        let cm = AppState::default();
        let duena = crate::cm::lectura::whoami(&cm).expect("sin sesión de cm");
        let plastic = crate::cm::sesion::carpeta_config_plastic().unwrap();
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        login_unity::asegurar_sesion_servidor(
            &cliente,
            &plastic,
            &duena,
            crate::servidor::region::webserver,
            |o| login_unity::renovar_token_cm(&cm, o),
        )
        .unwrap();

        let pc = |tarea: &str| {
            let estado = AppState::default();
            let dir = tempfile::tempdir().unwrap();
            proyecto_viejo(&estado, dir.path(), "BranchBoard-Pruebas", &duena, &[]);
            estado.fijar_workspace(Some(dir.path().to_path_buf()), Some(repo.clone()));
            crear_tarea(&estado, tarea);
            (estado, dir)
        };
        let (pc1, _d1) = pc("De la PC 1");
        let (pc2, _d2) = pc("De la PC 2");

        let r1 = subir_en(&pc1, &cliente, &PRUEBAS, |r, e| {
            traer_repo(&cm, &cliente, r, e)
        })
        .unwrap();
        let e2 = subir_en(&pc2, &cliente, &PRUEBAS, |r, e| {
            traer_repo(&cm, &cliente, r, e)
        })
        .unwrap_err();
        assert!(e2.to_string().contains("ya está en el servidor"), "{e2}");
        assert_eq!(
            pc2.con_db(|db| proyectos::proyecto_guardado(db.conexion()))
                .unwrap(),
            None,
            "la segunda no queda vinculada"
        );
        let eventos = cliente
            .llamar(
                "GET",
                &format!(
                    "/rest/v1/eventos?select=tipo,payload&proyecto_id=eq.{}",
                    r1.proyecto_id
                ),
                None,
            )
            .unwrap();
        let eventos = eventos.as_array().unwrap();
        assert_eq!(eventos.len(), 1, "solo la foto de la primera");
        assert_eq!(eventos[0]["tipo"], importacion::TIPO);
        let tareas = eventos[0]["payload"]["tablas"]["tareas"]
            .as_array()
            .unwrap();
        assert_eq!(tareas.len(), 1);
        assert_eq!(tareas[0]["titulo"], "De la PC 1");

        cliente.cerrar_sesion().unwrap();
        println!("gana el primero: {} de branchboard-pruebas", r1.proyecto_id);
    }
}
