//! Eventos: cada cambio compartido del proyecto (crear una tarea, mandarla a revisión, un mensaje…).
//! Ver "Sincronización entre máquinas" y "Servidor (Supabase)" en BACKEND.md.
//!
//! - La PC que hace el cambio lo aplica en su base y lo deja en la **bandeja de salida** (#238), todo en
//!   la misma transacción ([`publicar`]); la bandeja lo sube al servidor cuando hay conexión.
//! - Las demás PCs lo reciben del servidor y lo aplican con las mismas reglas (#239, [`recepcion`],
//!   [`lectura::aplicar_evento`]).
//! - Hasta #244 los eventos viajaban como archivos en `ops/` de una carpeta compartida (Drive/OneDrive).
//!   Esa carpeta solo se **lee** ya para subir al servidor los proyectos de antes (#245): ver
//!   [`carpeta_compartida`], [`carpeta_documentos`] y [`carpeta_imagen_proyecto`].
//! - Desde #243 no hay eventos firmados ni compactación con snapshots.

pub mod bandeja;
pub mod importacion;
pub mod lectura;
pub mod recepcion;

pub use lectura::{Aplicacion, ResumenSincronizacion};

use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Resultado};
use crate::modelo::normalizar_email;

/// Carpeta de documentos dentro de la carpeta compartida de antes (#120). Solo se lee (#245).
pub const CARPETA_DOCUMENTOS: &str = "documentos";
/// Carpeta de la imagen del proyecto dentro de la carpeta compartida de antes (#127). Solo se lee (#245).
pub const CARPETA_PROYECTO: &str = "proyecto";
/// Tipos de evento que ya no se aceptan (#243): los cambios de equipo y la compactación iban firmados
/// por el Administrador. Sin firmas, nadie podría probar que los mandó él, así que se descartan siempre;
/// los roles los cambian las funciones del servidor (#241).
pub const TIPOS_RETIRADOS: &[&str] = &[
    "crear_usuario",
    "cambiar_rol",
    "confirmar_usuario",
    "eliminar_usuario",
    "compactar_eventos",
];
/// Ajustes de esta máquina para el proyecto (dentro de su carpeta local). Hasta #244 guardaba la
/// carpeta compartida; se sigue leyendo para subir los proyectos de antes (#245).
pub const ARCHIVO_LOCAL: &str = "local.json";

/// Un evento: un cambio compartido del proyecto. Sube al servidor por la bandeja de salida (#238) y
/// las demás PCs lo reciben de ahí (#239).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evento {
    /// Nombre del comando que lo generó, ej. `crear_tarea`.
    pub tipo: String,
    #[serde(default)]
    pub tarea_id: Option<String>,
    /// Correo de quien lo generó.
    pub usuario: String,
    /// ISO 8601 en UTC con milisegundos, ej. `2026-11-14T18:30:05.123Z`.
    pub timestamp: String,
    #[serde(default)]
    pub payload: Value,
}

impl Evento {
    pub fn nuevo(tipo: &str, tarea_id: Option<&str>, usuario: &str, payload: Value) -> Self {
        Self::en(Utc::now(), tipo, tarea_id, usuario, payload)
    }

    fn en(
        momento: DateTime<Utc>,
        tipo: &str,
        tarea_id: Option<&str>,
        usuario: &str,
        payload: Value,
    ) -> Self {
        Evento {
            tipo: tipo.to_string(),
            tarea_id: tarea_id.map(String::from),
            usuario: normalizar_email(usuario),
            timestamp: momento.to_rfc3339_opts(SecondsFormat::Millis, true),
            payload,
        }
    }
}

/// Aplica un cambio compartido y lo publica como evento, todo o nada:
/// 1. `aplicar` escribe en la base, dentro de una transacción;
/// 2. se anota en `eventos_aplicados` (así, cuando vuelva del servidor, no se aplica otra vez);
/// 3. si el proyecto está vinculado al servidor, se guarda en la bandeja de salida (#238);
/// 4. se confirma la transacción.
///
/// Si `aplicar` falla, la base queda como estaba. Devuelve el id del evento. Sin servidor (proyecto
/// sin vincular) el cambio queda solo en esta PC hasta que se suba (#245).
pub fn publicar(
    c: &Connection,
    evento: &Evento,
    aplicar: impl FnOnce(&Connection) -> Resultado<()>,
) -> Resultado<String> {
    let tx = c.unchecked_transaction()?;
    aplicar(&tx)?;
    let id = nombre_archivo(&evento.timestamp, &evento.usuario, &uuid())?;
    marcar_aplicado(&tx, &id, &evento.timestamp)?;
    bandeja::encolar(&tx, &id, evento)?;
    tx.commit()?;
    #[cfg(test)]
    servidor_de_prueba::registrar(&id, evento);
    Ok(id)
}

/// Id de un evento: `20261114T183005123Z_ana@virustf.dev_<32 hex>.json` (hasta #244 era el nombre de
/// su archivo en `ops/`; se conserva la forma para que los ids de antes y los nuevos no choquen). El timestamp va sin `:`
/// (Windows no los permite en nombres) y el correo solo con `a-z0-9@.-`.
fn nombre_archivo(timestamp: &str, usuario: &str, uuid: &str) -> Resultado<String> {
    let momento = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| Error::Validacion(format!("Fecha de evento no válida: {timestamp}")))?
        .with_timezone(&Utc);
    let usuario: String = normalizar_email(usuario)
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '@' | '.' | '-' => c,
            _ => '_',
        })
        .collect();
    if usuario.is_empty() {
        return Err(Error::Validacion("El evento no tiene usuario".into()));
    }
    Ok(format!(
        "{}_{usuario}_{uuid}.json",
        momento.format("%Y%m%dT%H%M%S%3fZ")
    ))
}

/// 128 bits al azar en hexadecimal.
pub(crate) fn uuid() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Anota que el evento `id` ya está aplicado en esta base.
pub fn marcar_aplicado(c: &Connection, id: &str, aplicado_en: &str) -> Resultado<()> {
    c.execute(
        "INSERT OR IGNORE INTO eventos_aplicados (id, aplicado_en) VALUES (?1, ?2)",
        params![id, aplicado_en],
    )?;
    Ok(())
}

pub fn ya_aplicado(c: &Connection, id: &str) -> Resultado<bool> {
    Ok(c.query_row(
        "SELECT 1 FROM eventos_aplicados WHERE id = ?1",
        [id],
        |_| Ok(()),
    )
    .optional()?
    .is_some())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AjustesLocales {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    carpeta_compartida: Option<PathBuf>,
}

fn leer_locales(carpeta_proyecto: &Path) -> Resultado<AjustesLocales> {
    match std::fs::read_to_string(carpeta_proyecto.join(ARCHIVO_LOCAL)) {
        Ok(texto) => serde_json::from_str(&texto)
            .map_err(|_| Error::Archivos(format!("{ARCHIVO_LOCAL} del proyecto está dañado"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AjustesLocales::default()),
        Err(e) => Err(e.into()),
    }
}

/// Carpeta compartida (Drive/OneDrive) que esta PC usaba para el proyecto hasta #244, si la tenía.
/// Solo se lee para subir el proyecto al servidor (#245).
pub fn carpeta_compartida(carpeta_proyecto: &Path) -> Resultado<Option<PathBuf>> {
    Ok(leer_locales(carpeta_proyecto)?.carpeta_compartida)
}

/// `documentos/` de la carpeta compartida de antes (#120). Solo se lee (#245).
pub fn carpeta_documentos(carpeta_proyecto: &Path) -> Resultado<PathBuf> {
    let compartida = carpeta_compartida(carpeta_proyecto)?
        .ok_or_else(|| Error::Validacion("Este proyecto no tenía carpeta compartida".into()))?;
    Ok(compartida.join(CARPETA_DOCUMENTOS))
}

/// `proyecto/` de la carpeta compartida de antes, con la imagen del proyecto (#127). Solo se lee (#245).
pub fn carpeta_imagen_proyecto(carpeta_proyecto: &Path) -> Resultado<PathBuf> {
    let compartida = carpeta_compartida(carpeta_proyecto)?
        .ok_or_else(|| Error::Validacion("Este proyecto no tenía carpeta compartida".into()))?;
    Ok(compartida.join(CARPETA_PROYECTO))
}

/// Solo para pruebas: hace de **servidor** entre dos bases que simulan dos PCs. Guarda cada evento que
/// se publica en el hilo de la prueba (con su autor, que en la app pone el servidor) y se los entrega a
/// otra base por la misma recepción que usa la app con Supabase ([`recepcion::aplicar`], #239).
#[cfg(test)]
pub mod servidor_de_prueba {
    use std::cell::RefCell;

    use chrono::{DateTime, Utc};
    use rusqlite::{params, Connection};

    use super::{lectura, recepcion, Evento, ResumenSincronizacion};
    use crate::error::Resultado;
    use crate::tareas::InfoDobleToma;

    /// Id del proyecto de prueba en el "servidor".
    pub const PROYECTO: &str = "00000000-0000-4000-8000-000000000244";

    thread_local! {
        static EVENTOS: RefCell<Vec<(String, Evento)>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn registrar(id: &str, evento: &Evento) {
        EVENTOS.with(|e| e.borrow_mut().push((id.to_string(), evento.clone())));
    }

    /// Agrega un evento como si lo hubiera mandado otra PC (por ejemplo, uno falso).
    pub fn agregar(evento: &Evento) -> String {
        let id = super::nombre_archivo(&evento.timestamp, &evento.usuario, &super::uuid()).unwrap();
        registrar(&id, evento);
        id
    }

    /// Cuántos eventos tiene el "servidor".
    pub fn cuantos() -> usize {
        EVENTOS.with(|e| e.borrow().len())
    }

    /// Le entrega a `destino` todo lo que tiene el "servidor" y aplica lo que le falte.
    pub fn entregar(
        destino: &Connection,
        ahora: DateTime<Utc>,
    ) -> Resultado<ResumenSincronizacion> {
        entregar_con(destino, ahora, |_| Ok(false))
    }

    /// Como [`entregar`], con la revisión de la rama del perdedor en una doble toma (#97).
    pub fn entregar_con(
        destino: &Connection,
        ahora: DateTime<Utc>,
        revisar_rama_perdedor: impl Fn(&InfoDobleToma) -> Resultado<bool>,
    ) -> Resultado<ResumenSincronizacion> {
        let eventos = EVENTOS.with(|e| e.borrow().clone());
        for (orden, (id, ev)) in eventos.iter().enumerate() {
            destino.execute(
                "INSERT OR IGNORE INTO eventos_servidor
                 (orden, id, proyecto_servidor, tipo, tarea_id, usuario, payload, recibido_en)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    orden as i64 + 1,
                    id,
                    PROYECTO,
                    ev.tipo,
                    ev.tarea_id,
                    ev.usuario,
                    ev.payload.to_string(),
                    ev.timestamp
                ],
            )?;
        }
        recepcion::aplicar(
            destino,
            PROYECTO,
            None,
            ahora,
            lectura::aplicar_evento,
            revisar_rama_perdedor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::configuracion::{self, CLAVE_PROYECTO_SERVIDOR};
    use crate::db::Db;
    use serde_json::json;

    fn evento_de_prueba() -> Evento {
        let momento = DateTime::parse_from_rfc3339("2026-11-14T18:30:05.123Z")
            .unwrap()
            .with_timezone(&Utc);
        Evento::en(
            momento,
            "crear_tarea",
            Some("T-101"),
            "Ana@VirusTF.dev",
            json!({ "titulo": "Menú" }),
        )
    }

    fn contar(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |f| f.get(0)).unwrap()
    }

    #[test]
    fn el_id_lleva_la_hora_en_utc_y_el_autor() {
        let n = nombre_archivo("2026-11-14T18:30:05.123Z", "Ana@VirusTF.dev", "abc").unwrap();
        assert_eq!(n, "20261114T183005123Z_ana@virustf.dev_abc.json");
        let raro = nombre_archivo("2026-11-14T13:30:05.123-05:00", "a/b\\c:d@x.dev", "u").unwrap();
        assert_eq!(raro, "20261114T183005123Z_a_b_c_d@x.dev_u.json");
        assert!(nombre_archivo("ayer", "a@x.dev", "u").is_err());
        assert!(nombre_archivo("2026-11-14T18:30:05Z", "", "u").is_err());
    }

    #[test]
    fn el_evento_tiene_los_campos_del_diseno() {
        let v = serde_json::to_value(evento_de_prueba()).unwrap();
        assert_eq!(
            v,
            json!({
                "tipo": "crear_tarea",
                "tarea_id": "T-101",
                "usuario": "ana@virustf.dev",
                "timestamp": "2026-11-14T18:30:05.123Z",
                "payload": { "titulo": "Menú" }
            })
        );
        // Un evento de antes con `firma_admin` (#243) se sigue leyendo.
        let viejo: Evento = serde_json::from_value(json!({
            "tipo": "crear_tarea", "usuario": "a@x.dev", "timestamp": "2026-11-14T18:30:05.123Z",
            "firma_admin": "abc"
        }))
        .unwrap();
        assert_eq!(viejo.tipo, "crear_tarea");
        assert_eq!(uuid().len(), 32);
        assert_ne!(uuid(), uuid());
    }

    #[test]
    fn publicar_aplica_anota_y_deja_en_la_bandeja_sin_escribir_archivos() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        // Sin vincular: se aplica y se anota, pero no hay a dónde subirlo.
        let id = publicar(c, &evento_de_prueba(), |tx| {
            tx.execute(
                "INSERT INTO meta (clave, valor) VALUES ('prueba', 'aplicado')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert!(ya_aplicado(c, &id).unwrap());
        assert!(!ya_aplicado(c, "otro.json").unwrap());
        assert_eq!(
            contar(c, "SELECT count(*) FROM meta WHERE clave = 'prueba'"),
            1
        );
        assert_eq!(bandeja::pendientes(c).unwrap(), 0);
        // Vinculado al servidor: queda en la bandeja de salida.
        configuracion::guardar(
            c,
            CLAVE_PROYECTO_SERVIDOR,
            servidor_de_prueba::PROYECTO,
            "a@x.dev",
            "2026-11-14",
        )
        .unwrap();
        publicar(c, &evento_de_prueba(), |_| Ok(())).unwrap();
        assert_eq!(bandeja::pendientes(c).unwrap(), 1);
    }

    #[test]
    fn si_aplicar_falla_no_queda_nada() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        configuracion::guardar(
            c,
            CLAVE_PROYECTO_SERVIDOR,
            servidor_de_prueba::PROYECTO,
            "a@x.dev",
            "2026-11-14",
        )
        .unwrap();
        let antes = servidor_de_prueba::cuantos();
        let e = publicar(c, &evento_de_prueba(), |tx| {
            tx.execute("INSERT INTO meta (clave, valor) VALUES ('x', '1')", [])?;
            Err(Error::Validacion("regla rota".into()))
        })
        .unwrap_err();
        assert_eq!(e.codigo(), "validacion");
        assert_eq!(contar(c, "SELECT count(*) FROM meta WHERE clave = 'x'"), 0);
        assert_eq!(bandeja::pendientes(c).unwrap(), 0, "nada en la bandeja");
        assert_eq!(servidor_de_prueba::cuantos(), antes, "nada publicado");
    }

    #[test]
    fn el_servidor_de_prueba_entrega_a_la_otra_base_lo_que_le_falta() {
        let a = Db::en_memoria().unwrap();
        let b = Db::en_memoria().unwrap();
        let mut ev = evento_de_prueba();
        ev.tipo = "tipo_que_nadie_conoce".into();
        publicar(a.conexion(), &ev, |_| Ok(())).unwrap();
        let ahora = Utc::now();
        let r = servidor_de_prueba::entregar(b.conexion(), ahora).unwrap();
        assert_eq!(r.desconocidos, 1, "B lo recibe; su tipo no lo conoce nadie");
        // A no se lo aplica a sí misma: ya lo tiene anotado.
        let r = servidor_de_prueba::entregar(a.conexion(), ahora).unwrap();
        assert_eq!(r, ResumenSincronizacion::default());
    }

    #[test]
    fn la_carpeta_compartida_de_antes_se_lee_de_local_json() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(carpeta_compartida(dir.path()).unwrap(), None);
        assert!(carpeta_documentos(dir.path()).is_err());
        let vieja = dir.path().join("OneDrive").join("Juego");
        std::fs::write(
            dir.path().join(ARCHIVO_LOCAL),
            json!({ "carpeta_compartida": vieja }).to_string(),
        )
        .unwrap();
        assert_eq!(carpeta_compartida(dir.path()).unwrap(), Some(vieja.clone()));
        assert_eq!(
            carpeta_documentos(dir.path()).unwrap(),
            vieja.join(CARPETA_DOCUMENTOS)
        );
        assert_eq!(
            carpeta_imagen_proyecto(dir.path()).unwrap(),
            vieja.join(CARPETA_PROYECTO)
        );
    }
}
