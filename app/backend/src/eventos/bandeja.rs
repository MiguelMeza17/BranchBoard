//! Bandeja de salida hacia el servidor de Supabase (#238).
//!
//! `publicar` (ver [`super::publicar`]) guarda cada evento en la tabla local `bandeja_salida`, en la misma
//! transacción que el cambio: si el cambio se guarda, el evento queda en la bandeja; si no, tampoco.
//! [`subir`] (lo llama el temporizador de 15 s) los manda en orden a la tabla `eventos` del servidor y los
//! borra de la bandeja cuando el servidor los confirma. Sin internet o sin sesión esperan, aunque se
//! cierre la app.
//!
//! - **Sin duplicar**: el id del evento es la clave en el servidor; si una subida llegó pero se perdió la
//!   respuesta, al reintentar el servidor ignora el repetido.
//! - **En orden**: se suben por orden de llegada y, si un lote falla, se para ahí (no se saltan eventos).
//! - **Qué no sube**: los eventos de roles ([`super::TIPOS_FIRMADOS`]): en el servidor los roles los cambian
//!   solo sus funciones (`agregar_miembro`, `cambiar_rol`…, #234/#241), no los eventos.
//! - **Solo proyectos vinculados**: si el proyecto no tiene `proyecto_servidor_id` en su configuración
//!   (lo fija #240), no se encola nada.

use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

use super::{Evento, TIPOS_FIRMADOS};
use crate::db::configuracion::{self, CLAVE_PROYECTO_SERVIDOR};
use crate::error::{Error, Resultado};

/// Un trabajo sobre la base que [`subir`] le pide a quien la presta.
pub type TrabajoDb<'f> = &'f mut dyn FnMut(&Connection) -> Resultado<()>;

/// Cuántos eventos se mandan por petición.
pub const TAMANO_LOTE: usize = 50;

/// Cómo quedó la bandeja después de intentar subir.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EstadoBandeja {
    /// Eventos que siguen esperando.
    pub pendientes: u64,
    /// Eventos subidos en esta vuelta.
    pub subidos: u64,
    /// Por qué se paró, si quedaron pendientes por un error (sin internet, sin sesión…).
    pub error: Option<String>,
}

/// Guarda `evento` (con id `id`) en la bandeja si el proyecto está vinculado al servidor. Devuelve si lo
/// encoló. Se llama dentro de la transacción de `publicar`.
pub fn encolar(c: &Connection, id: &str, evento: &Evento) -> Resultado<bool> {
    if TIPOS_FIRMADOS.contains(&evento.tipo.as_str()) {
        return Ok(false);
    }
    let Some(proyecto) = configuracion::leer(c, CLAVE_PROYECTO_SERVIDOR)?
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    else {
        return Ok(false);
    };
    let payload =
        serde_json::to_string(&evento.payload).map_err(|e| Error::Interno(e.to_string()))?;
    c.execute(
        "INSERT OR IGNORE INTO bandeja_salida (id, proyecto_servidor, tipo, tarea_id, payload, creado_en)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            proyecto,
            evento.tipo,
            evento.tarea_id,
            payload,
            evento.timestamp
        ],
    )?;
    Ok(true)
}

/// Cuántos eventos esperan subir.
pub fn pendientes(c: &Connection) -> Resultado<u64> {
    Ok(c.query_row("SELECT COUNT(*) FROM bandeja_salida", [], |f| {
        f.get::<_, i64>(0)
    })? as u64)
}

/// Los primeros `max` pendientes, en orden, como filas para `POST /rest/v1/eventos`.
fn siguiente_lote(c: &Connection, max: usize) -> Resultado<(Vec<String>, Value)> {
    let mut consulta = c.prepare(
        "SELECT id, proyecto_servidor, tipo, tarea_id, payload, creado_en
         FROM bandeja_salida ORDER BY rowid LIMIT ?1",
    )?;
    let mut ids = Vec::new();
    let mut filas = Vec::new();
    let mut resultado = consulta.query([max as i64])?;
    while let Some(f) = resultado.next()? {
        let id: String = f.get(0)?;
        let payload: String = f.get(4)?;
        filas.push(serde_json::json!({
            "id": id,
            "proyecto_id": f.get::<_, String>(1)?,
            "tipo": f.get::<_, String>(2)?,
            "tarea_id": f.get::<_, Option<String>>(3)?,
            "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null),
            "creado_en_app": f.get::<_, String>(5)?,
        }));
        ids.push(id);
    }
    Ok((ids, Value::Array(filas)))
}

fn borrar(c: &Connection, ids: &[String]) -> Resultado<()> {
    let tx = c.unchecked_transaction()?;
    for id in ids {
        tx.execute("DELETE FROM bandeja_salida WHERE id = ?1", [id])?;
    }
    tx.commit()?;
    Ok(())
}

fn anotar_fallo(c: &Connection, ids: &[String], error: &str) -> Resultado<()> {
    let tx = c.unchecked_transaction()?;
    for id in ids {
        tx.execute(
            "UPDATE bandeja_salida SET intentos = intentos + 1, ultimo_error = ?2 WHERE id = ?1",
            params![id, error],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Sube lo pendiente por lotes hasta vaciar la bandeja o encontrar un error.
///
/// `con_db` presta la base solo mientras se lee o se borra (nunca durante la red, para no bloquear la
/// app); `enviar` manda un lote al servidor. Un error de `enviar` no es un error de esta función: queda
/// en [`EstadoBandeja::error`] y los eventos siguen en la bandeja.
pub fn subir(
    con_db: impl Fn(TrabajoDb<'_>) -> Resultado<()>,
    enviar: impl Fn(&Value) -> Resultado<()>,
) -> Resultado<EstadoBandeja> {
    let mut estado = EstadoBandeja::default();
    loop {
        let mut lote = (Vec::new(), Value::Null);
        con_db(&mut |c| {
            lote = siguiente_lote(c, TAMANO_LOTE)?;
            Ok(())
        })?;
        let (ids, filas) = lote;
        if ids.is_empty() {
            break;
        }
        match enviar(&filas) {
            Ok(()) => {
                con_db(&mut |c| borrar(c, &ids))?;
                estado.subidos += ids.len() as u64;
            }
            Err(e) => {
                let mensaje = e.to_string();
                con_db(&mut |c| anotar_fallo(c, &ids, &mensaje))?;
                estado.error = Some(mensaje);
                break;
            }
        }
    }
    con_db(&mut |c| {
        estado.pendientes = pendientes(c)?;
        Ok(())
    })?;
    Ok(estado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn base() -> crate::db::Db {
        crate::db::Db::en_memoria().unwrap()
    }

    fn vincular(c: &Connection) {
        configuracion::guardar(
            c,
            CLAVE_PROYECTO_SERVIDOR,
            "11111111-2222-4333-8444-555555555555",
            "ana@virustf.dev",
            "2026-11-21T00:00:00.000Z",
        )
        .unwrap();
    }

    fn evento(tipo: &str) -> Evento {
        Evento::nuevo(
            tipo,
            Some("T-1"),
            "ana@virustf.dev",
            serde_json::json!({ "titulo": "Hola" }),
        )
    }

    fn subir_en<'a>(c: &'a Connection) -> impl Fn(TrabajoDb<'_>) -> Resultado<()> + 'a {
        move |f| f(c)
    }

    #[test]
    fn sin_proyecto_del_servidor_no_encola() {
        let db = base();
        let c = db.conexion();
        assert!(!encolar(c, "e1", &evento("crear_tarea")).unwrap());
        assert_eq!(pendientes(c).unwrap(), 0);
    }

    #[test]
    fn encola_con_proyecto_vinculado_pero_no_los_eventos_de_roles() {
        let db = base();
        let c = db.conexion();
        vincular(c);
        assert!(encolar(c, "e1", &evento("crear_tarea")).unwrap());
        // El mismo id dos veces no duplica.
        encolar(c, "e1", &evento("crear_tarea")).unwrap();
        for tipo in TIPOS_FIRMADOS {
            assert!(!encolar(c, &format!("rol-{tipo}"), &evento(tipo)).unwrap());
        }
        assert_eq!(pendientes(c).unwrap(), 1);
    }

    #[test]
    fn sube_en_orden_por_lotes_y_vacia_la_bandeja() {
        let db = base();
        let c = db.conexion();
        vincular(c);
        for i in 0..(TAMANO_LOTE + 3) {
            encolar(c, &format!("e{i:03}"), &evento("crear_tarea")).unwrap();
        }
        let enviados = RefCell::new(Vec::<Value>::new());
        let estado = subir(subir_en(c), |filas| {
            enviados.borrow_mut().push(filas.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            estado,
            EstadoBandeja {
                pendientes: 0,
                subidos: (TAMANO_LOTE + 3) as u64,
                error: None
            }
        );
        let lotes = enviados.into_inner();
        assert_eq!(lotes.len(), 2);
        assert_eq!(lotes[0].as_array().unwrap().len(), TAMANO_LOTE);
        let primera = &lotes[0][0];
        assert_eq!(primera["id"], "e000");
        assert_eq!(
            primera["proyecto_id"],
            "11111111-2222-4333-8444-555555555555"
        );
        assert_eq!(primera["tipo"], "crear_tarea");
        assert_eq!(primera["tarea_id"], "T-1");
        assert_eq!(primera["payload"], serde_json::json!({ "titulo": "Hola" }));
        assert!(primera["creado_en_app"].as_str().unwrap().ends_with('Z'));
        // Las filas no llevan autor ni hora del servidor: los pone el servidor.
        assert!(primera.get("autor").is_none() && primera.get("usuario").is_none());
        assert_eq!(lotes[1][0]["id"], format!("e{TAMANO_LOTE:03}"));
    }

    #[test]
    fn sin_conexion_los_eventos_esperan_y_se_anota_el_intento() {
        let db = base();
        let c = db.conexion();
        vincular(c);
        encolar(c, "e1", &evento("crear_tarea")).unwrap();
        encolar(c, "e2", &evento("mover_tarea")).unwrap();
        let estado = subir(subir_en(c), |_| Err(Error::ServidorSinConexion)).unwrap();
        assert_eq!(estado.pendientes, 2);
        assert_eq!(estado.subidos, 0);
        assert_eq!(
            estado.error.as_deref(),
            Some(Error::ServidorSinConexion.to_string().as_str())
        );
        let intentos: i64 = c
            .query_row(
                "SELECT intentos FROM bandeja_salida WHERE id = 'e1'",
                [],
                |f| f.get(0),
            )
            .unwrap();
        assert_eq!(intentos, 1);

        // Al volver la red se suben y la bandeja queda vacía.
        let estado = subir(subir_en(c), |_| Ok(())).unwrap();
        assert_eq!((estado.pendientes, estado.subidos), (0, 2));
    }

    #[test]
    fn un_lote_que_falla_frena_los_siguientes() {
        let db = base();
        let c = db.conexion();
        vincular(c);
        for i in 0..(TAMANO_LOTE * 2) {
            encolar(c, &format!("e{i:03}"), &evento("crear_tarea")).unwrap();
        }
        let llamadas = RefCell::new(0);
        let estado = subir(subir_en(c), |_| {
            *llamadas.borrow_mut() += 1;
            if *llamadas.borrow() == 1 {
                Ok(())
            } else {
                Err(Error::SinSesionServidor)
            }
        })
        .unwrap();
        assert_eq!(*llamadas.borrow(), 2);
        assert_eq!(
            (estado.subidos, estado.pendientes),
            (TAMANO_LOTE as u64, TAMANO_LOTE as u64)
        );
    }

    #[test]
    fn la_bandeja_sobrevive_a_cerrar_la_app() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = crate::db::Db::abrir(dir.path()).unwrap();
            vincular(db.conexion());
            encolar(db.conexion(), "e1", &evento("crear_tarea")).unwrap();
        }
        let db = crate::db::Db::abrir(dir.path()).unwrap();
        assert_eq!(pendientes(db.conexion()).unwrap(), 1);
    }

    /// Llavero en memoria que comparten varios clientes (el "sin red" y el real usan la misma sesión).
    #[derive(Clone, Default)]
    struct LlaveroCompartido(std::rc::Rc<RefCell<std::collections::HashMap<String, String>>>);
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

    /// Mismo nombre que pruebas (misma sesión en el llavero) pero una dirección que no resuelve: sin red.
    static PRUEBAS_SIN_RED: crate::supabase::ProyectoSupabase = crate::supabase::ProyectoSupabase {
        nombre: "branchboard-pruebas",
        url: "https://sin-red.branchboard.invalid",
        clave_publica: "sb_publishable_sin_red",
    };

    /// Contra branchboard-pruebas, entrando con la sesión real de Unity VC de este equipo (#237):
    /// 1. crea un proyecto en el servidor y vincula una base local a él;
    /// 2. publica dos cambios con `publicar` (van a la bandeja en la misma transacción);
    /// 3. sin red no se suben y esperan; al volver la red se suben y la bandeja queda vacía;
    /// 4. el servidor los tiene con su id, el autor es la cuenta de la sesión, y reenviarlos no duplica.
    ///
    /// Deja en pruebas la cuenta y el proyecto: borrarlos después (ver BACKEND.md → Bandeja de salida).
    #[test]
    #[ignore = "toca branchboard-pruebas y la sesión real de Unity VC: correr a mano con --ignored"]
    fn entorno_real_sin_red_esperan_y_al_volver_se_suben() {
        use crate::supabase::{entorno::PRUEBAS, login_unity, ClienteSupabase};

        let estado = crate::estado::AppState::default();
        let correo = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let carpeta = crate::cm::sesion::carpeta_config_plastic().unwrap();
        let llavero = LlaveroCompartido::default();
        let servidor = ClienteSupabase::new(&PRUEBAS, llavero.clone());
        assert!(login_unity::asegurar_sesion_servidor(
            &servidor,
            &carpeta,
            &correo,
            crate::servidor::region::webserver,
            |org| login_unity::renovar_token_cm(&estado, org)
        )
        .unwrap());
        let sesion = servidor.sesion_guardada().unwrap().unwrap();

        // 1. Proyecto en el servidor y base local vinculada.
        let proyecto = servidor
            .llamar(
                "POST",
                "/rest/v1/rpc/crear_proyecto",
                Some(&serde_json::json!({ "p_nombre": "Prueba bandeja #238" })),
            )
            .unwrap();
        let proyecto = proyecto.as_str().unwrap().to_string();
        let db = base();
        let c = db.conexion();
        configuracion::guardar(
            c,
            CLAVE_PROYECTO_SERVIDOR,
            &proyecto,
            &correo,
            "2026-11-21T00:00:00.000Z",
        )
        .unwrap();

        // 2. Dos cambios publicados como los publica la app.
        let ops = tempfile::tempdir().unwrap();
        let mut ids = Vec::new();
        for tipo in ["crear_tarea", "mover_tarea"] {
            let e = Evento::nuevo(
                tipo,
                Some("T-238"),
                &correo,
                serde_json::json!({ "prueba": 238 }),
            );
            ids.push(super::super::publicar(c, ops.path(), &e, |_| Ok(())).unwrap());
        }
        assert_eq!(pendientes(c).unwrap(), 2);

        // 3. Sin red esperan; con red se suben.
        let sin_red = ClienteSupabase::new(&PRUEBAS_SIN_RED, llavero.clone());
        let r = subir(subir_en(c), |f| sin_red.insertar_eventos(f)).unwrap();
        assert_eq!((r.subidos, r.pendientes), (0, 2));
        assert_eq!(
            r.error.as_deref(),
            Some(Error::ServidorSinConexion.to_string().as_str())
        );
        let r = subir(subir_en(c), |f| servidor.insertar_eventos(f)).unwrap();
        assert_eq!((r.subidos, r.pendientes, r.error), (2, 0, None));

        // 4. En el servidor, en orden, con el autor de la sesión; reenviar no duplica.
        let ruta = format!("/rest/v1/eventos?select=id,tipo,tarea_id,autor,payload&proyecto_id=eq.{proyecto}&order=orden");
        let filas = servidor.llamar("GET", &ruta, None).unwrap();
        let filas = filas.as_array().unwrap();
        assert_eq!(filas.len(), 2);
        for (fila, (id, tipo)) in filas
            .iter()
            .zip(ids.iter().zip(["crear_tarea", "mover_tarea"]))
        {
            assert_eq!(fila["id"], id.as_str());
            assert_eq!(fila["tipo"], tipo);
            assert_eq!(fila["tarea_id"], "T-238");
            assert_eq!(fila["autor"], sesion.usuario_id.as_str());
            assert_eq!(fila["payload"], serde_json::json!({ "prueba": 238 }));
        }
        let repetido = serde_json::json!([{
            "id": ids[0], "proyecto_id": proyecto, "tipo": "crear_tarea", "tarea_id": "T-238",
            "payload": {}, "creado_en_app": "2026-11-21T00:00:00.000Z"
        }]);
        servidor.insertar_eventos(&repetido).unwrap();
        let filas = servidor.llamar("GET", &ruta, None).unwrap();
        assert_eq!(filas.as_array().unwrap().len(), 2);

        servidor.cerrar_sesion().unwrap();
        println!("bandeja verificada en el proyecto {proyecto} de branchboard-pruebas ({correo})");
    }
}
