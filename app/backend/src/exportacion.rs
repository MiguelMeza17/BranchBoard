//! Respaldo de un proyecto del servidor en el formato portable `.bbproyecto` (#277).
//! Actualiza una copia aislada de SQLite, descarga todos sus archivos y publica el zip al terminar.
//! No escribe en el servidor ni modifica el proyecto abierto. La recuperación queda desvinculada.
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::db::configuracion::{
    self, CLAVE_ICONO_PROYECTO, CLAVE_NOMBRE_PROYECTO, CLAVE_PROYECTO_SERVIDOR,
};
use crate::db::{documentos, usuarios, Db};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos::{self, importacion, lectura, recepcion, Evento};
use crate::modelo::{ahora_iso, normalizar_email, Rol};
use crate::proyecto_local::{self, MarcaLocal, ARCHIVO_MARCA, CARPETA_DATOS};
use crate::servidor::pat::Llavero;
use crate::supabase::storage::{self, Carpeta, Storage};
use crate::supabase::{miembros, ClienteSupabase, ProyectoSupabase};

pub const CACHE_DOCUMENTOS: &str = "documentos-servidor";
pub const CACHE_IMAGEN: &str = "proyecto-servidor";

struct Temporal(PathBuf);
impl Drop for Temporal {
    fn drop(&mut self) {
        // Siempre una carpeta nueva creada por tempdir_en, nunca una ruta del archivo importado.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// La carpeta de datos de una copia recuperada. No habilita escritura local de documentos.
pub fn es_recuperado(datos: &Path) -> bool {
    datos.file_name().is_some_and(|n| n == CARPETA_DATOS)
        && datos
            .parent()
            .and_then(|p| proyecto_local::leer_marca(p).ok())
            .is_some_and(|m| m.administrador_recuperacion.is_some())
}

fn exigir_admin<L: Llavero>(
    cliente: &ClienteSupabase<L>,
    proyecto: &str,
    correo: &str,
) -> Resultado<miembros::Equipo> {
    let sesion = cliente.sesion_vigente()?;
    if normalizar_email(&sesion.correo) != normalizar_email(correo) {
        return Err(Error::Validacion(
            "Inicia sesión con la cuenta que tiene abierto este proyecto.".into(),
        ));
    }
    let mut equipo = miembros::leer_equipo(cliente, proyecto)?;
    if !equipo
        .miembros
        .iter()
        .any(|m| m.usuario.id == sesion.usuario_id && m.rol == Rol::Administrador)
    {
        return Err(Error::SinPermiso);
    }
    equipo
        .miembros
        .sort_by(|a, b| a.usuario.id.cmp(&b.usuario.id));
    equipo.invitaciones.sort_by(|a, b| a.correo.cmp(&b.correo));
    Ok(equipo)
}

/// Exporta con la sesión del cliente; las pruebas usan cuentas y llaveros independientes.
pub fn exportar<L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    entorno: &'static ProyectoSupabase,
    destino: &Path,
) -> Resultado<()> {
    let yo = estado.exigir_permiso("exportar_proyecto_local")?;
    if !destino
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bbproyecto"))
    {
        return Err(Error::Validacion(
            "Guarda el respaldo con la extensión .bbproyecto.".into(),
        ));
    }
    let proyecto = estado
        .con_db(|db| configuracion::leer(db.conexion(), CLAVE_PROYECTO_SERVIDOR))?
        .ok_or_else(|| Error::Validacion("El proyecto no está vinculado al servidor.".into()))?;
    // Valida el UUID antes de interpolarlo en rutas de la API.
    storage::ruta_objeto(&proyecto, Carpeta::Documentos, "validar")?;
    let equipo = exigir_admin(cliente, &proyecto, &yo.email)?;
    let padre = destino
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| Error::Validacion("Elige una carpeta existente para el respaldo.".into()))?;
    let temporal = Temporal(proyecto_local::tempdir_en(padre)?);
    let origen = temporal.0.join("origen");
    std::fs::create_dir(&origen)?;
    let copia = origen.join(crate::db::NOMBRE_DB);
    estado.con_db(|db| {
        if configuracion::leer(db.conexion(), CLAVE_PROYECTO_SERVIDOR)?.as_deref()
            != Some(&proyecto)
        {
            return Err(Error::Validacion(
                "El proyecto abierto cambió; vuelve a exportarlo.".into(),
            ));
        }
        db.conexion()
            .execute("VACUUM INTO ?1", [copia.to_string_lossy().as_ref()])?;
        Ok(())
    })?;
    let copia = Db::abrir(&origen)?;
    // Conserva lo ya aplicado en esta PC y actualiza únicamente la copia, no la base abierta.
    for u in miembros::filas_locales(&equipo) {
        if usuarios::buscar(copia.conexion(), &u.email)?.is_none() {
            usuarios::insertar(copia.conexion(), &u)?;
        } else {
            usuarios::actualizar_rol(copia.conexion(), &u.email, u.rol)?;
        }
    }
    let ultima = cliente.llamar(
        "GET",
        &format!(
            "/rest/v1/eventos?select=orden&proyecto_id=eq.{proyecto}&order=orden.desc&limit=1"
        ),
        None,
    )?;
    let filas = ultima
        .as_array()
        .ok_or_else(|| Error::Servidor("Respuesta de eventos inválida".into()))?;
    let corte = match filas.first() {
        None => 0,
        Some(f) => f["orden"]
            .as_i64()
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::Servidor("Orden de eventos inválido".into()))?,
    };
    recepcion::bajar(
        |f| f(copia.conexion()),
        &proyecto,
        |desde| {
            cliente
                .llamar(
                    "GET",
                    &format!(
                        "{}&orden=lte.{corte}",
                        recepcion::ruta_eventos(&proyecto, desde)
                    ),
                    None,
                )
                .and_then(recepcion::filas_desde_json)
        },
        |ids| {
            cliente
                .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                .and_then(recepcion::correos_desde_json)
        },
    )?;
    let resumen = recepcion::aplicar(
        copia.conexion(),
        &proyecto,
        None,
        Utc::now(),
        lectura::aplicar_evento,
        |_| {
            Err(Error::Validacion(
                "Sincroniza la doble toma pendiente antes de exportar.".into(),
            ))
        },
    )?;
    if resumen.desconocidos > 0 {
        return Err(Error::Validacion(
            "Actualiza BranchBoard para incluir todos los eventos en el respaldo.".into(),
        ));
    }
    let mut foto = importacion::foto(copia.conexion())?;
    // Las colas, credenciales y datos de transporte nunca forman parte del proyecto recuperado.
    if let Some(filas) = foto["tablas"]["configuracion_proyecto"].as_array_mut() {
        filas.retain(|f| {
            !matches!(
                f["clave"].as_str(),
                Some("alta_proyecto_pendiente" | "alta_proyecto_local")
            )
        });
    }
    let nombre = configuracion::leer(copia.conexion(), CLAVE_NOMBRE_PROYECTO)?
        .unwrap_or_else(|| "Proyecto".into());
    let contenido = temporal.0.join("contenido");
    let datos = contenido.join(CARPETA_DATOS);
    let respaldo = Db::abrir(&datos)?;
    let evento = Evento::nuevo(importacion::TIPO, None, &yo.email, foto);
    let tx = respaldo.conexion().unchecked_transaction()?;
    if importacion::aplicar(&tx, &evento)? != lectura::Aplicacion::Aplicado {
        return Err(Error::Interno(
            "No se pudo preparar el estado del respaldo.".into(),
        ));
    }
    tx.commit()?;
    let mut filas = miembros::filas_locales(&equipo);
    for invitado in &equipo.invitaciones {
        let (nombre, apellido) = usuarios::nombre_desde_email(&invitado.correo);
        filas.push(usuarios::Usuario {
            email: invitado.correo.clone(),
            nombre,
            apellido,
            rol: invitado.rol,
            fecha_alta: invitado.invitado_en.chars().take(10).collect(),
        });
    }
    usuarios::reemplazar_todos(respaldo.conexion(), &filas)?;
    let marca = MarcaLocal {
        aplicacion: "BranchBoard".into(),
        tipo: "local".into(),
        formato: proyecto_local::FORMATO,
        id: eventos::uuid(),
        nombre,
        creado_en: ahora_iso(),
        administrador_recuperacion: Some(yo.email.clone()),
    };
    std::fs::write(
        contenido.join(ARCHIVO_MARCA),
        serde_json::to_vec_pretty(&marca).map_err(|e| Error::Interno(e.to_string()))?,
    )?;
    let servidor = Storage::new(cliente, entorno);
    let tamano_inicial = std::fs::metadata(datos.join(crate::db::NOMBRE_DB))?.len();
    let mut total = tamano_inicial;
    for doc in documentos::listar(respaldo.conexion())? {
        let ruta = storage::ruta_objeto(&proyecto, Carpeta::Documentos, &doc.ruta)?;
        let bytes = servidor.bajar(&ruta)?;
        if bytes.len() as u64 != doc.tamano {
            return Err(Error::Validacion(format!(
                "El archivo de \"{}\" no coincide con su tamaño registrado.",
                doc.nombre
            )));
        }
        guardar_archivo(&datos.join(CACHE_DOCUMENTOS), &doc.ruta, &bytes, &mut total)?;
    }
    if let Some(icono) =
        configuracion::leer(respaldo.conexion(), CLAVE_ICONO_PROYECTO)?.filter(|s| !s.is_empty())
    {
        let ruta = storage::ruta_objeto(&proyecto, Carpeta::Proyecto, &icono)?;
        guardar_archivo(
            &datos.join(CACHE_IMAGEN),
            &icono,
            &servidor.bajar(&ruta)?,
            &mut total,
        )?;
    }
    if exigir_admin(cliente, &proyecto, &yo.email)? != equipo {
        return Err(Error::Validacion(
            "El equipo cambió durante la exportación; vuelve a intentarlo.".into(),
        ));
    }
    let copia_final = temporal.0.join("estado.db");
    respaldo
        .conexion()
        .execute("VACUUM INTO ?1", [copia_final.to_string_lossy().as_ref()])?;
    if total
        .saturating_sub(tamano_inicial)
        .saturating_add(std::fs::metadata(&copia_final)?.len())
        .saturating_add(std::fs::metadata(contenido.join(ARCHIVO_MARCA))?.len())
        > proyecto_local::MAXIMO_IMPORTADO
    {
        return Err(Error::Validacion(
            "El respaldo supera el máximo de 4 GB que puede importarse.".into(),
        ));
    }
    let archivo = temporal.0.join("completo.bbproyecto");
    proyecto_local::escribir_zip(&contenido, &copia_final, &archivo)?;
    // No toca un respaldo anterior si falla la descarga o la preparación de cualquier archivo.
    std::fs::rename(archivo, destino)?;
    Ok(())
}

fn guardar_archivo(base: &Path, nombre: &str, bytes: &[u8], total: &mut u64) -> Resultado<()> {
    *total = total.saturating_add(bytes.len() as u64);
    if *total > proyecto_local::MAXIMO_IMPORTADO {
        return Err(Error::Validacion(
            "El respaldo supera el máximo de 4 GB que puede importarse.".into(),
        ));
    }
    std::fs::create_dir_all(base)?;
    std::fs::write(base.join(nombre), bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Clone, Default)]
    struct Memoria(std::rc::Rc<RefCell<HashMap<String, String>>>);
    impl Llavero for Memoria {
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

    #[test]
    fn no_escribe_un_archivo_que_haria_imposible_importar_el_respaldo() {
        let dir = tempfile::tempdir().unwrap();
        let mut total = proyecto_local::MAXIMO_IMPORTADO;
        assert!(guardar_archivo(dir.path(), "D-a.txt", b"x", &mut total).is_err());
        assert!(!dir.path().join("D-a.txt").exists());
    }

    fn base(proyecto: &str, correo: &str, rol: Rol) -> AppState {
        let db = Db::en_memoria().unwrap();
        configuracion::guardar(
            db.conexion(),
            CLAVE_PROYECTO_SERVIDOR,
            proyecto,
            correo,
            "2026-11-21",
        )
        .unwrap();
        usuarios::insertar(
            db.conexion(),
            &usuarios::Usuario {
                email: correo.into(),
                nombre: "Zeta".into(),
                apellido: "Prueba".into(),
                rol,
                fecha_alta: "2026-11-21".into(),
            },
        )
        .unwrap();
        let estado = AppState::default();
        estado.abrir_proyecto(db);
        estado.fijar_usuario(Some(correo));
        estado
    }

    fn recibir(estado: &AppState, cliente: &ClienteSupabase<Memoria>, proyecto: &str) {
        recepcion::bajar(
            |f| estado.con_db(|db| f(db.conexion())),
            proyecto,
            |desde| {
                cliente
                    .llamar("GET", &recepcion::ruta_eventos(proyecto, desde), None)
                    .and_then(recepcion::filas_desde_json)
            },
            |ids| {
                cliente
                    .llamar("GET", &recepcion::ruta_perfiles(ids), None)
                    .and_then(recepcion::correos_desde_json)
            },
        )
        .unwrap();
        estado
            .con_db(|db| {
                recepcion::aplicar(
                    db.conexion(),
                    proyecto,
                    None,
                    Utc::now(),
                    lectura::aplicar_evento,
                    |_| Ok(false),
                )
            })
            .unwrap();
    }

    /// Registra cuentas propias en branchboard-pruebas, con sesión solo en memoria.
    /// BB277_EVIDENCIA guarda únicamente UUID de recursos para su limpieza posterior por CLI.
    #[test]
    #[ignore = "crea recursos temporales en branchboard-pruebas; requiere permiso para su limpieza"]
    fn entorno_real_exportar_importar_y_recuperar_en_otro_proyecto() {
        use crate::modelo::{hoy_local, TipoTarea};
        use crate::supabase::entorno::PRUEBAS;
        use crate::tareas::{self, AutorTarea, DatosNuevaTarea};
        use std::io::Cursor;
        let sufijo = eventos::uuid();
        let clave = format!("Aa7!{}", eventos::uuid());
        let registrar = |letra: &str, nombre: &str| {
            let c = ClienteSupabase::new(&PRUEBAS, Memoria::default());
            let correo = format!("exportar-277-{letra}-{sufijo}@prueba.branchboard");
            let sesion = c.registrar(nombre, "Prueba", &correo, &clave).unwrap();
            (c, sesion)
        };
        let (a, sa) = registrar("a", "Ana");
        let (b, sb) = registrar("b", "Zeta");
        let (c, sc) = registrar("c", "Cora");
        let proyecto = a
            .llamar(
                "POST",
                "/rest/v1/rpc/crear_proyecto",
                Some(&serde_json::json!({"p_nombre":format!("Respaldo 277 {sufijo}")})),
            )
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let mut evidencia = serde_json::json!({"usuarios":[sa.usuario_id,sb.usuario_id,sc.usuario_id],"proyectos":[proyecto]});
        let anotar = |e: &serde_json::Value| {
            if let Ok(ruta) = std::env::var("BB277_EVIDENCIA") {
                std::fs::write(ruta, serde_json::to_vec_pretty(e).unwrap()).unwrap();
            }
        };
        anotar(&evidencia);
        miembros::agregar(&a, &proyecto, &sb.correo, Rol::Administrador).unwrap();
        miembros::agregar(&a, &proyecto, &sc.correo, Rol::Desarrollador).unwrap();
        let invitado = format!("exportar-277-invitado-{sufijo}@prueba.branchboard");
        miembros::agregar(&a, &proyecto, &invitado, Rol::Tester).unwrap();
        let original = Db::en_memoria().unwrap();
        let conn = original.conexion();
        configuracion::guardar(
            conn,
            CLAVE_NOMBRE_PROYECTO,
            "Juego recuperable",
            &sa.correo,
            "2026-11-21",
        )
        .unwrap();
        configuracion::guardar(
            conn,
            CLAVE_ICONO_PROYECTO,
            "icono-277.png",
            &sa.correo,
            "2026-11-21",
        )
        .unwrap();
        conn.execute("INSERT INTO sprints(id,nombre,con_fecha,fecha_inicio,creado_por) VALUES ('S-277','Sprint respaldo',0,'2026-11-21',?1)", [&sa.correo]).unwrap();
        conn.execute("INSERT INTO tareas(id,titulo,tipo,estado,creado_por,fecha_creacion,sprint_id) VALUES ('T-101','Original','Scripting','por hacer',?1,'2026-11-21','S-277')", [&sc.correo]).unwrap();
        conn.execute("INSERT INTO mensajes_chat(id,de,para,texto,fecha) VALUES ('M-277',?1,'grupo','Mensaje conservado','2026-11-21')", [&sa.correo]).unwrap();
        documentos::insertar(
            conn,
            &documentos::Documento {
                id: "D-277".into(),
                nombre: "Guía.txt".into(),
                tamano: 16,
                subido_por: sc.correo.clone(),
                fecha: "2026-11-21".into(),
                pendiente: false,
                ruta: "D-277.txt".into(),
            },
        )
        .unwrap();
        a.insertar_eventos(&serde_json::json!([{"id":format!("foto-{sufijo}"),"proyecto_id":proyecto,
            "tipo":importacion::TIPO,"payload":importacion::foto(conn).unwrap(),"creado_en_app":ahora_iso()}])).unwrap();
        configuracion::guardar(
            conn,
            CLAVE_PROYECTO_SERVIDOR,
            &proyecto,
            &sa.correo,
            "2026-11-21",
        )
        .unwrap();
        usuarios::insertar(
            conn,
            &usuarios::Usuario {
                email: sa.correo.clone(),
                nombre: "Ana".into(),
                apellido: "Prueba".into(),
                rol: Rol::Administrador,
                fecha_alta: "2026-11-21".into(),
            },
        )
        .unwrap();
        tareas::crear(
            conn,
            &AutorTarea {
                email: &sa.correo,
                rol: Rol::Administrador,
            },
            &DatosNuevaTarea {
                titulo: "Nueva desde otra PC".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
            },
            &ahora_iso(),
            &hoy_local(),
        )
        .unwrap();
        eventos::bandeja::subir(|f| f(conn), |filas| a.insertar_eventos(filas)).unwrap();
        let storage_a = Storage::new(&a, &PRUEBAS);
        let ruta_doc = storage::ruta_objeto(&proyecto, Carpeta::Documentos, "D-277.txt").unwrap();
        let ruta_icono =
            storage::ruta_objeto(&proyecto, Carpeta::Proyecto, "icono-277.png").unwrap();
        let bytes_doc = b"Guia de respaldo";
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        storage_a.subir(&ruta_doc, bytes_doc).unwrap();
        storage_a.subir(&ruta_icono, png.get_ref()).unwrap();
        let mut recuperado_id = None;
        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let fuente = base(&proyecto, &sb.correo, Rol::Administrador);
            let dir = tempfile::tempdir().unwrap();
            let archivo = dir.path().join("respaldo.bbproyecto");
            exportar(&fuente, &b, &PRUEBAS, &archivo).unwrap();
            // La descarga no altera la base abierta ni inserta eventos en el servidor.
            assert_eq!(
                fuente
                    .con_db(|db| crate::db::tareas::listar(db.conexion()).map(|l| l.len()))
                    .unwrap(),
                0
            );
            let filas = a
                .llamar(
                    "GET",
                    &format!("/rest/v1/eventos?select=id&proyecto_id=eq.{proyecto}"),
                    None,
                )
                .unwrap();
            assert_eq!(filas.as_array().unwrap().len(), 2);
            let importado =
                proyecto_local::importar(&archivo, dir.path().to_str().unwrap()).unwrap();
            let recuperado = AppState::default();
            let app = tempfile::tempdir().unwrap();
            let entrada =
                proyecto_local::abrir(&recuperado, app.path(), Path::new(&importado.ruta), false)
                    .unwrap();
            assert!(
                matches!(entrada,crate::identidad::acceso::Entrada::Dentro {email,..} if email==sb.correo)
            );
            recuperado
                .con_db(|db| {
                    assert_eq!(
                        crate::db::tareas::obtener(db.conexion(), "T-101")?.creado_por,
                        sc.correo
                    );
                    assert_eq!(
                        crate::db::tareas::obtener(db.conexion(), "T-102")?.titulo,
                        "Nueva desde otra PC"
                    );
                    assert!(configuracion::leer(db.conexion(), CLAVE_PROYECTO_SERVIDOR)?.is_none());
                    for tabla in ["bandeja_salida", "eventos_aplicados", "eventos_servidor"] {
                        assert_eq!(
                            db.conexion().query_row(
                                &format!("SELECT COUNT(*) FROM {tabla}"),
                                [],
                                |f| f.get::<_, i64>(0)
                            )?,
                            0
                        );
                    }
                    assert_eq!(usuarios::listar(db.conexion())?.len(), 4);
                    assert_eq!(
                        db.conexion().query_row(
                            "SELECT texto FROM mensajes_chat WHERE id='M-277'",
                            [],
                            |f| f.get::<_, String>(0)
                        )?,
                        "Mensaje conservado"
                    );
                    Ok(())
                })
                .unwrap();
            let doc =
                crate::comandos::documentos::resolver_documento_en(&recuperado, "D-277").unwrap();
            assert_eq!(std::fs::read(doc).unwrap(), bytes_doc);
            assert!(
                crate::comandos::proyecto::obtener_configuracion_proyecto_en(&recuperado)
                    .unwrap()
                    .imagen
                    .is_some()
            );
            let subida =
                crate::comandos::subir_proyecto::subir_en(&recuperado, &b, &PRUEBAS, |_, _| {
                    panic!("no Unity")
                })
                .unwrap();
            assert_ne!(subida.proyecto_id, proyecto);
            recuperado_id = Some(subida.proyecto_id.clone());
            evidencia["proyectos"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!(subida.proyecto_id));
            anotar(&evidencia);
            assert_eq!(
                Storage::new(&b, &PRUEBAS)
                    .bajar(
                        &storage::ruta_objeto(
                            &subida.proyecto_id,
                            Carpeta::Documentos,
                            "D-277.txt"
                        )
                        .unwrap()
                    )
                    .unwrap(),
                bytes_doc
            );
            let nueva_pc = base(&subida.proyecto_id, &sb.correo, Rol::Administrador);
            recibir(&nueva_pc, &b, &subida.proyecto_id);
            assert_eq!(
                nueva_pc
                    .con_db(|db| crate::db::tareas::obtener(db.conexion(), "T-101"))
                    .unwrap()
                    .creado_por,
                sc.correo
            );
            // Un rol local falsificado no permite exportar como Administrador del servidor.
            let falso = base(&proyecto, &sc.correo, Rol::Administrador);
            let anterior = dir.path().join("anterior.bbproyecto");
            std::fs::write(&anterior, b"conservar").unwrap();
            assert!(matches!(
                exportar(&falso, &c, &PRUEBAS, &anterior),
                Err(Error::SinPermiso)
            ));
            assert_eq!(std::fs::read(&anterior).unwrap(), b"conservar");
            // Un archivo ausente no puede producir un respaldo aparentemente completo.
            a.insertar_eventos(&serde_json::json!([{"id":format!("ausente-{sufijo}"),"proyecto_id":proyecto,"tipo":"subir_documentos",
                "payload":{"documentos":[{"id":"D-ausente","nombre":"Falta.txt","tamano":1,"subido_por":sa.correo,
                    "fecha":"2026-11-21","pendiente":false,"ruta":"D-ausente.txt"}]},"creado_en_app":ahora_iso()}])).unwrap();
            assert!(exportar(&fuente, &b, &PRUEBAS, &anterior).is_err());
            assert_eq!(std::fs::read(&anterior).unwrap(), b"conservar");
            assert!(!std::fs::read_dir(dir.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".branchboard-exportando")));
        }));
        let _ = storage_a.borrar(&ruta_doc);
        let _ = storage_a.borrar(&ruta_icono);
        if let Some(id) = recuperado_id {
            for (carpeta, nombre) in [
                (Carpeta::Documentos, "D-277.txt"),
                (Carpeta::Proyecto, "icono-277.png"),
            ] {
                let _ = Storage::new(&b, &PRUEBAS)
                    .borrar(&storage::ruta_objeto(&id, carpeta, nombre).unwrap());
            }
        }
        for cliente in [&a, &b, &c] {
            let _ = cliente.cerrar_sesion();
        }
        if let Err(e) = resultado {
            std::panic::resume_unwind(e);
        }
    }
}
