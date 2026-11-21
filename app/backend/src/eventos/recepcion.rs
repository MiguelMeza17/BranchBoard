//! Recibir y aplicar los eventos del servidor de Supabase (#239). Reemplaza la lectura de `ops/`
//! ([`super::lectura`]) cuando el proyecto está vinculado al servidor.
//!
//! - **Consulta periódica** (decisión del 2026-11-20 en #239): la app pide lo nuevo cada ~5 s mientras
//!   está abierta, al entrar al proyecto y al volver a la ventana:
//!   `GET /rest/v1/eventos?proyecto_id=eq.<id>&orden=gt.<último>&order=orden`.
//! - **Copia local**: lo que baja se guarda en `eventos_servidor` (SQLite). El último `orden` guardado es
//!   desde dónde se pide la próxima vez; la **primera vez en una PC** es 0, así que baja todo el historial
//!   (por páginas de [`TAMANO_PAGINA`]).
//! - **Orden y desempate por el servidor**: se aplica por `orden` del servidor, y la hora del evento es
//!   `recibido_en` (la del servidor), no la del reloj de cada PC.
//! - **Los propios no se aplican dos veces**: el id del evento es el mismo en `ops/`, la bandeja y el
//!   servidor, y lo que ya está en `eventos_aplicados` se salta.
//! - **El autor**: el servidor guarda la cuenta (`autor`, uuid); la app trabaja con correos, así que se
//!   pide el correo a `perfiles` (se ven los de quienes comparten proyecto). Si no se puede ver (alguien
//!   que ya salió de todos los proyectos que compartían), queda `<uuid>@sin-perfil`.
//! - **Red y base separadas**: la base solo se presta para leer o escribir, nunca durante la red.
//! - Lo que no se puede aplicar todavía (tipo desconocido) queda pendiente y se reintenta en la próxima
//!   vuelta, igual que con `ops/`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value;

use super::lectura::{self, Aplicacion, Pendiente, ResumenSincronizacion};
use super::Evento;
use crate::error::{Error, Resultado};
use crate::modelo::normalizar_email;
use crate::tareas::InfoDobleToma;

/// Un trabajo sobre la base que [`bajar`] le pide a quien la presta.
pub type TrabajoDb<'f> = &'f mut dyn FnMut(&Connection) -> Resultado<()>;

/// Cuántos eventos se piden por petición.
pub const TAMANO_PAGINA: usize = 500;

/// Columnas que se piden de `eventos`.
pub const COLUMNAS: &str = "orden,id,tipo,tarea_id,autor,payload,recibido_en";

/// Una fila de la tabla `eventos` del servidor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FilaServidor {
    pub orden: i64,
    pub id: String,
    pub tipo: String,
    #[serde(default)]
    pub tarea_id: Option<String>,
    pub autor: String,
    #[serde(default)]
    pub payload: Value,
    pub recibido_en: String,
}

/// Ruta de la API para pedir los eventos de `proyecto` posteriores a `desde`.
pub fn ruta_eventos(proyecto: &str, desde: i64) -> String {
    format!(
        "/rest/v1/eventos?select={COLUMNAS}&proyecto_id=eq.{proyecto}&orden=gt.{desde}&order=orden.asc&limit={TAMANO_PAGINA}"
    )
}

/// Ruta de la API para pedir el correo de las cuentas `ids`.
pub fn ruta_perfiles(ids: &[String]) -> String {
    format!(
        "/rest/v1/perfiles?select=id,correo&id=in.({})",
        ids.join(",")
    )
}

/// Convierte la respuesta de `GET /rest/v1/eventos` en filas.
pub fn filas_desde_json(json: Value) -> Resultado<Vec<FilaServidor>> {
    serde_json::from_value(json)
        .map_err(|e| Error::Interno(format!("respuesta de eventos del servidor no válida: {e}")))
}

/// Convierte la respuesta de `GET /rest/v1/perfiles` en `(id, correo)`.
pub fn correos_desde_json(json: Value) -> Resultado<Vec<(String, String)>> {
    #[derive(Deserialize)]
    struct Perfil {
        id: String,
        correo: String,
    }
    let perfiles: Vec<Perfil> = serde_json::from_value(json).map_err(|e| {
        Error::Interno(format!("respuesta de perfiles del servidor no válida: {e}"))
    })?;
    Ok(perfiles.into_iter().map(|p| (p.id, p.correo)).collect())
}

/// El último `orden` del servidor que ya está en la copia local de `proyecto` (0 si ninguno).
pub fn ultimo_orden(c: &Connection, proyecto: &str) -> Resultado<i64> {
    Ok(c.query_row(
        "SELECT COALESCE(MAX(orden), 0) FROM eventos_servidor WHERE proyecto_servidor = ?1",
        [proyecto],
        |f| f.get(0),
    )?)
}

fn guardar(
    c: &Connection,
    proyecto: &str,
    filas: &[FilaServidor],
    correos: &HashMap<String, String>,
) -> Resultado<()> {
    let tx = c.unchecked_transaction()?;
    for f in filas {
        let usuario = correos
            .get(&f.autor)
            .map(|c| normalizar_email(c))
            .unwrap_or_else(|| format!("{}@sin-perfil", f.autor));
        let payload =
            serde_json::to_string(&f.payload).map_err(|e| Error::Interno(e.to_string()))?;
        tx.execute(
            "INSERT OR IGNORE INTO eventos_servidor
             (orden, id, proyecto_servidor, tipo, tarea_id, usuario, payload, recibido_en)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                f.orden,
                f.id,
                proyecto,
                f.tipo,
                f.tarea_id,
                usuario,
                payload,
                f.recibido_en
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Baja del servidor todo lo nuevo de `proyecto` y lo guarda en la copia local. Devuelve cuántos
/// eventos nuevos bajaron.
///
/// `pedir_eventos(desde)` devuelve hasta [`TAMANO_PAGINA`] filas con `orden > desde`, en orden;
/// `pedir_correos(ids)` devuelve `(id, correo)` de las cuentas que se puedan ver.
pub fn bajar(
    con_db: impl Fn(TrabajoDb<'_>) -> Resultado<()>,
    proyecto: &str,
    pedir_eventos: impl Fn(i64) -> Resultado<Vec<FilaServidor>>,
    pedir_correos: impl Fn(&[String]) -> Resultado<Vec<(String, String)>>,
) -> Resultado<usize> {
    let mut correos: HashMap<String, String> = HashMap::new();
    let mut nuevos = 0;
    loop {
        let mut desde = 0;
        con_db(&mut |c| {
            desde = ultimo_orden(c, proyecto)?;
            Ok(())
        })?;
        let filas = pedir_eventos(desde)?;
        if filas.is_empty() {
            break;
        }
        let mut faltan: Vec<String> = filas
            .iter()
            .map(|f| f.autor.clone())
            .filter(|a| !correos.contains_key(a))
            .collect();
        faltan.sort();
        faltan.dedup();
        if !faltan.is_empty() {
            correos.extend(pedir_correos(&faltan)?);
        }
        let completa = filas.len() >= TAMANO_PAGINA;
        con_db(&mut |c| guardar(c, proyecto, &filas, &correos))?;
        nuevos += filas.len();
        if !completa {
            break;
        }
    }
    Ok(nuevos)
}

/// Los eventos de la copia local de `proyecto` que esta base todavía no aplicó, en el orden del servidor.
fn pendientes(c: &Connection, proyecto: &str) -> Resultado<Vec<Pendiente>> {
    let mut consulta = c.prepare(
        "SELECT s.orden, s.id, s.tipo, s.tarea_id, s.usuario, s.payload, s.recibido_en
         FROM eventos_servidor s
         WHERE s.proyecto_servidor = ?1
           AND NOT EXISTS (SELECT 1 FROM eventos_aplicados a WHERE a.id = s.id)
         ORDER BY s.orden",
    )?;
    let filas = consulta.query_map([proyecto], |f| {
        Ok((
            f.get::<_, i64>(0)?,
            f.get::<_, String>(1)?,
            f.get::<_, String>(2)?,
            f.get::<_, Option<String>>(3)?,
            f.get::<_, String>(4)?,
            f.get::<_, String>(5)?,
            f.get::<_, String>(6)?,
        ))
    })?;
    let mut lista = Vec::new();
    for fila in filas {
        let (orden_servidor, id, tipo, tarea_id, usuario, payload, recibido_en) = fila?;
        let momento = DateTime::parse_from_rfc3339(&recibido_en)
            .map(|m| m.with_timezone(&Utc))
            .map_err(|_| Error::Interno(format!("hora del servidor no válida: {recibido_en}")))?;
        let mut evento = Evento::nuevo(
            &tipo,
            tarea_id.as_deref(),
            &usuario,
            serde_json::from_str(&payload).unwrap_or(Value::Null),
        );
        // La hora que manda es la del servidor (#239), con el mismo formato que las de la app.
        evento.timestamp = momento.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        lista.push(Pendiente {
            id,
            evento,
            orden: momento,
            orden_servidor: Some(orden_servidor),
        });
    }
    Ok(lista)
}

/// Aplica, en el orden del servidor, lo que bajó de `proyecto` y esta base todavía no tiene.
/// Los avisos (descartados) van a `carpeta_log/eventos.log` si se da una carpeta.
pub fn aplicar(
    c: &Connection,
    proyecto: &str,
    carpeta_log: Option<&std::path::Path>,
    ahora: DateTime<Utc>,
    aplicar_evento: impl Fn(&Connection, &Evento) -> Resultado<Aplicacion>,
    revisar_rama_perdedor: impl Fn(&InfoDobleToma) -> Resultado<bool>,
) -> Resultado<ResumenSincronizacion> {
    let mut resumen = ResumenSincronizacion::default();
    let mut avisos = Vec::new();
    let lista = pendientes(c, proyecto)?;
    lectura::aplicar_pendientes(
        c,
        None,
        ahora,
        lista,
        &mut resumen,
        &mut avisos,
        aplicar_evento,
        revisar_rama_perdedor,
    )?;
    if let (Some(carpeta), false) = (carpeta_log, avisos.is_empty()) {
        lectura::registrar(carpeta, ahora, &avisos);
    }
    Ok(resumen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::configuracion::{self, CLAVE_PROYECTO_SERVIDOR};
    use std::cell::RefCell;

    const P: &str = "11111111-2222-4333-8444-555555555555";
    const ANA: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const BETO: &str = "bbbbbbbb-0000-4000-8000-000000000002";

    fn base() -> crate::db::Db {
        crate::db::Db::en_memoria().unwrap()
    }

    fn en<'a>(c: &'a Connection) -> impl Fn(TrabajoDb<'_>) -> Resultado<()> + 'a {
        move |f| f(c)
    }

    fn fila(orden: i64, id: &str, tipo: &str, autor: &str) -> FilaServidor {
        FilaServidor {
            orden,
            id: id.into(),
            tipo: tipo.into(),
            tarea_id: Some("T-1".into()),
            autor: autor.into(),
            payload: serde_json::json!({ "n": orden }),
            recibido_en: format!("2026-11-21T10:00:{:02}.5+00:00", orden % 60),
        }
    }

    fn correos(ids: &[String]) -> Resultado<Vec<(String, String)>> {
        Ok(ids
            .iter()
            .filter_map(|id| match id.as_str() {
                ANA => Some((id.clone(), "Ana@VirusTF.dev".to_string())),
                BETO => Some((id.clone(), "beto@virustf.dev".to_string())),
                _ => None,
            })
            .collect())
    }

    /// Un "servidor" en memoria que responde como `GET /rest/v1/eventos`.
    fn servidor(todas: Vec<FilaServidor>) -> impl Fn(i64) -> Resultado<Vec<FilaServidor>> {
        move |desde| {
            Ok(todas
                .iter()
                .filter(|f| f.orden > desde)
                .take(TAMANO_PAGINA)
                .cloned()
                .collect())
        }
    }

    #[test]
    fn la_primera_vez_baja_todo_el_historial_por_paginas() {
        let db = base();
        let c = db.conexion();
        let todas: Vec<_> = (1..=(TAMANO_PAGINA as i64 + 7))
            .map(|i| fila(i, &format!("e{i:04}"), "crear_tarea", ANA))
            .collect();
        let pedidos = RefCell::new(Vec::new());
        let srv = servidor(todas);
        let n = bajar(
            en(c),
            P,
            |desde| {
                pedidos.borrow_mut().push(desde);
                srv(desde)
            },
            correos,
        )
        .unwrap();
        assert_eq!(n, TAMANO_PAGINA + 7);
        assert_eq!(pedidos.into_inner(), vec![0, TAMANO_PAGINA as i64]);
        assert_eq!(ultimo_orden(c, P).unwrap(), TAMANO_PAGINA as i64 + 7);
        // La siguiente vez pide desde el último y no baja nada nuevo.
        assert_eq!(bajar(en(c), P, servidor(vec![]), correos).unwrap(), 0);
    }

    #[test]
    fn el_autor_se_guarda_como_correo_y_sin_perfil_queda_marcado() {
        let db = base();
        let c = db.conexion();
        let filas = vec![
            fila(1, "e1", "crear_tarea", ANA),
            fila(
                2,
                "e2",
                "crear_tarea",
                "cccccccc-0000-4000-8000-000000000003",
            ),
        ];
        bajar(en(c), P, servidor(filas), correos).unwrap();
        let usuarios: Vec<String> = c
            .prepare("SELECT usuario FROM eventos_servidor ORDER BY orden")
            .unwrap()
            .query_map([], |f| f.get(0))
            .unwrap()
            .map(|u| u.unwrap())
            .collect();
        assert_eq!(
            usuarios,
            vec![
                "ana@virustf.dev".to_string(),
                "cccccccc-0000-4000-8000-000000000003@sin-perfil".to_string()
            ]
        );
    }

    #[test]
    fn aplica_en_el_orden_del_servidor_con_su_hora_y_salta_los_propios() {
        let db = base();
        let c = db.conexion();
        // "e2" lo publicó esta máquina: ya está aplicado.
        super::super::marcar_aplicado(c, "e2", "2026-11-21T09:00:00.000Z").unwrap();
        // El servidor los numeró así aunque las horas de cada PC dirían otra cosa.
        let filas = vec![
            fila(3, "e3", "crear_tarea", BETO),
            fila(1, "e1", "crear_tarea", ANA),
            fila(2, "e2", "crear_tarea", ANA),
        ];
        let mut ordenadas = filas.clone();
        ordenadas.sort_by_key(|f| f.orden);
        bajar(en(c), P, servidor(ordenadas), correos).unwrap();

        let vistos = RefCell::new(Vec::new());
        let r = aplicar(
            c,
            P,
            None,
            Utc::now(),
            |_, e| {
                vistos.borrow_mut().push((
                    e.usuario.clone(),
                    e.timestamp.clone(),
                    e.payload["n"].clone(),
                ));
                Ok(Aplicacion::Aplicado)
            },
            |_| Ok(false),
        )
        .unwrap();
        assert_eq!(r.aplicados, 2);
        assert_eq!(
            vistos.into_inner(),
            vec![
                (
                    "ana@virustf.dev".to_string(),
                    "2026-11-21T10:00:01.500Z".to_string(),
                    serde_json::json!(1)
                ),
                (
                    "beto@virustf.dev".to_string(),
                    "2026-11-21T10:00:03.500Z".to_string(),
                    serde_json::json!(3)
                ),
            ]
        );
        // Una segunda vuelta no aplica nada otra vez.
        let r = aplicar(
            c,
            P,
            None,
            Utc::now(),
            |_, _| panic!("ya aplicado"),
            |_| Ok(false),
        )
        .unwrap();
        assert_eq!(r.aplicados, 0);
    }

    #[test]
    fn un_tipo_desconocido_queda_pendiente_y_no_frena_a_los_demas() {
        let db = base();
        let c = db.conexion();
        bajar(
            en(c),
            P,
            servidor(vec![
                fila(1, "e1", "algo_de_una_version_nueva", ANA),
                fila(2, "e2", "crear_tarea", ANA),
            ]),
            correos,
        )
        .unwrap();
        let aplicar_una = |_: &Connection, e: &Evento| {
            Ok(if e.tipo == "crear_tarea" {
                Aplicacion::Aplicado
            } else {
                Aplicacion::Desconocido
            })
        };
        let r = aplicar(c, P, None, Utc::now(), aplicar_una, |_| Ok(false)).unwrap();
        assert_eq!((r.aplicados, r.desconocidos), (1, 1));
        let r = aplicar(c, P, None, Utc::now(), aplicar_una, |_| Ok(false)).unwrap();
        assert_eq!((r.aplicados, r.desconocidos), (0, 1));
    }

    #[test]
    fn sin_conexion_no_se_guarda_nada() {
        let db = base();
        let c = db.conexion();
        let e = bajar(en(c), P, |_| Err(Error::ServidorSinConexion), correos).unwrap_err();
        assert!(matches!(e, Error::ServidorSinConexion));
        assert_eq!(ultimo_orden(c, P).unwrap(), 0);
    }

    #[test]
    fn las_copias_de_cada_proyecto_van_aparte() {
        let db = base();
        let c = db.conexion();
        configuracion::guardar(
            c,
            CLAVE_PROYECTO_SERVIDOR,
            P,
            "ana@virustf.dev",
            "2026-11-21T00:00:00.000Z",
        )
        .unwrap();
        bajar(
            en(c),
            P,
            servidor(vec![fila(9, "e9", "crear_tarea", ANA)]),
            correos,
        )
        .unwrap();
        assert_eq!(ultimo_orden(c, P).unwrap(), 9);
        assert_eq!(ultimo_orden(c, "otro").unwrap(), 0);
    }

    #[test]
    fn rutas_de_la_api() {
        assert_eq!(
            ruta_eventos(P, 42),
            format!("/rest/v1/eventos?select={COLUMNAS}&proyecto_id=eq.{P}&orden=gt.42&order=orden.asc&limit={TAMANO_PAGINA}")
        );
        assert_eq!(
            ruta_perfiles(&[ANA.to_string(), BETO.to_string()]),
            format!("/rest/v1/perfiles?select=id,correo&id=in.({ANA},{BETO})")
        );
        let filas = filas_desde_json(serde_json::json!([{
            "orden": 5, "id": "e5", "tipo": "crear_tarea", "tarea_id": null, "autor": ANA,
            "payload": {"a": 1}, "recibido_en": "2026-11-21T10:00:00.123456+00:00"
        }]))
        .unwrap();
        assert_eq!(filas[0].orden, 5);
        assert_eq!(filas[0].tarea_id, None);
    }

    #[derive(Clone, Default)]
    struct LlaveroCompartido(std::rc::Rc<RefCell<HashMap<String, String>>>);
    impl crate::servidor::pat::Llavero for LlaveroCompartido {
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

    /// Contra branchboard-pruebas, con la sesión real de Unity VC de este equipo (#237) y **dos bases
    /// locales** como si fueran dos PCs:
    /// 1. crea un proyecto en el servidor y vincula las dos bases a él;
    /// 2. la PC A crea dos tareas de verdad (`tareas::crear`) y las sube (bandeja, #238);
    /// 3. la PC B, nueva, baja todo el historial y las aplica: quedan con el creador (correo, no uuid) y
    ///    la hora del servidor;
    /// 4. la PC A crea una tercera; B baja solo esa (pide desde el último `orden`);
    /// 5. la PC A, al bajar, no aplica dos veces lo suyo.
    ///
    /// Deja en pruebas el proyecto "Prueba recepción #239": borrarlo después (ver BACKEND.md).
    #[test]
    #[ignore = "toca branchboard-pruebas y la sesión real de Unity VC: correr a mano con --ignored"]
    fn entorno_real_dos_pcs_reciben_lo_del_otro() {
        use crate::eventos::bandeja;
        use crate::modelo::{Rol, TipoTarea};
        use crate::supabase::{entorno::PRUEBAS, login_unity, ClienteSupabase};
        use crate::tareas::{self, AutorTarea, DatosNuevaTarea};

        let estado = crate::estado::AppState::default();
        let correo = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let carpeta = crate::cm::sesion::carpeta_config_plastic().unwrap();
        let servidor = ClienteSupabase::new(&PRUEBAS, LlaveroCompartido::default());
        assert!(login_unity::asegurar_sesion_servidor(
            &servidor,
            &carpeta,
            &correo,
            crate::servidor::region::webserver,
            |org| login_unity::renovar_token_cm(&estado, org)
        )
        .unwrap());

        // 1. Proyecto en el servidor y dos PCs vinculadas.
        let proyecto = servidor
            .llamar(
                "POST",
                "/rest/v1/rpc/crear_proyecto",
                Some(&serde_json::json!({ "p_nombre": "Prueba recepción #239" })),
            )
            .unwrap();
        let proyecto = proyecto.as_str().unwrap().to_string();
        let (pc_a, pc_b) = (base(), base());
        for db in [&pc_a, &pc_b] {
            configuracion::guardar(
                db.conexion(),
                CLAVE_PROYECTO_SERVIDOR,
                &proyecto,
                &correo,
                "2026-11-21T00:00:00.000Z",
            )
            .unwrap();
        }
        let ops_a = tempfile::tempdir().unwrap();
        let autor = AutorTarea {
            email: &correo,
            rol: Rol::Administrador,
        };
        let crear = |titulo: &str| {
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
            tareas::crear(
                pc_a.conexion(),
                ops_a.path(),
                &autor,
                &datos,
                &crate::modelo::ahora_iso(),
                "2026-11-21",
            )
            .unwrap()
        };
        let subir_a = || {
            let r = bandeja::subir(
                |f| f(pc_a.conexion()),
                |filas| servidor.insertar_eventos(filas),
            )
            .unwrap();
            assert_eq!(r.error, None);
            r.subidos
        };
        let recibir = |db: &crate::db::Db| {
            let nuevos = bajar(
                |f| f(db.conexion()),
                &proyecto,
                |desde| {
                    servidor
                        .llamar("GET", &ruta_eventos(&proyecto, desde), None)
                        .and_then(filas_desde_json)
                },
                |ids| {
                    servidor
                        .llamar("GET", &ruta_perfiles(ids), None)
                        .and_then(correos_desde_json)
                },
            )
            .unwrap();
            let r = aplicar(
                db.conexion(),
                &proyecto,
                None,
                Utc::now(),
                lectura::aplicar_evento,
                |_| Ok(false),
            )
            .unwrap();
            (nuevos, r)
        };

        // 2. A crea dos tareas y las sube.
        let t1 = crear("Recepción 1");
        let t2 = crear("Recepción 2");
        assert_eq!(subir_a(), 2);

        // 3. B, nueva, baja todo y las aplica.
        let (nuevos, r) = recibir(&pc_b);
        assert_eq!((nuevos, r.aplicados, r.descartados), (2, 2, 0));
        for t in [&t1, &t2] {
            let en_b = crate::db::tareas::obtener(pc_b.conexion(), &t.id).unwrap();
            assert_eq!(en_b.titulo, t.titulo);
            assert_eq!(en_b.creado_por, crate::modelo::normalizar_email(&correo));
        }
        let usuario: String = pc_b
            .conexion()
            .query_row(
                "SELECT usuario FROM eventos_servidor ORDER BY orden LIMIT 1",
                [],
                |f| f.get(0),
            )
            .unwrap();
        assert_eq!(usuario, crate::modelo::normalizar_email(&correo));

        // 4. A crea una tercera; B baja solo esa.
        let t3 = crear("Recepción 3");
        assert_eq!(subir_a(), 1);
        let (nuevos, r) = recibir(&pc_b);
        assert_eq!((nuevos, r.aplicados), (1, 1));
        assert!(crate::db::tareas::obtener(pc_b.conexion(), &t3.id).is_ok());

        // 5. A baja lo suyo y no lo aplica otra vez.
        let (nuevos, r) = recibir(&pc_a);
        assert_eq!((nuevos, r.aplicados), (3, 0));

        servidor.cerrar_sesion().unwrap();
        println!("recepción verificada en el proyecto {proyecto} de branchboard-pruebas");
    }
}
