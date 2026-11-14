//! Sincronización entre máquinas por eventos en `ops/`. Issues #91 a #94.
//! Ver "Sincronización entre máquinas" en BACKEND.md.
//!
//! Escribir (#91):
//! - Cada cambio compartido es un archivo **nuevo** en `<carpeta compartida>/ops/`,
//!   `<timestamp>_<usuario>_<uuid>.json`, con `{ tipo, tarea_id, usuario, timestamp, payload }`.
//! - Nunca se edita ni se borra un evento escrito: aquí solo existe escribir, y si
//!   el nombre ya existe no se pisa. Se escribe a un temporal y se renombra, para
//!   que Drive/OneDrive nunca suba un archivo a medias.
//! - La máquina que lo escribe lo aplica de inmediato a su `.db` y lo anota en
//!   `eventos_aplicados`, para que al leer `ops/` (#92) no lo aplique otra vez.
//! - La carpeta compartida (Drive/OneDrive) es de cada máquina: se guarda en
//!   `local.json` de la carpeta local del proyecto, no en la base (que se puede
//!   reconstruir o restaurar sin perder la ruta).
//!
//! Leer y aplicar los de otras máquinas (#92): ver [`lectura`].

pub mod lectura;

pub use lectura::{sincronizar, Aplicacion, ResumenSincronizacion};

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Resultado};
use crate::modelo::normalizar_email;

/// Carpeta de eventos dentro de la carpeta compartida del proyecto.
pub const CARPETA_OPS: &str = "ops";
/// Ajustes de esta máquina para el proyecto (dentro de su carpeta local).
const ARCHIVO_LOCAL: &str = "local.json";
/// Marca, dentro de la carpeta compartida, de qué proyecto es.
pub const ARCHIVO_MARCA: &str = "branchboard-proyecto.json";

/// Un evento, tal como queda en el archivo de `ops/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evento {
    /// Nombre del comando que lo generó, ej. `crear_tarea`.
    pub tipo: String,
    #[serde(default)]
    pub tarea_id: Option<String>,
    /// Correo de quien lo generó (el de `cm whoami`).
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

/// `ops/` del proyecto: la carpeta compartida configurada en esta máquina + `ops`.
pub fn carpeta_ops(carpeta_proyecto: &Path) -> Resultado<PathBuf> {
    let compartida = carpeta_compartida(carpeta_proyecto)?.ok_or_else(|| {
        Error::Validacion(
            "Configura la carpeta compartida del proyecto (Drive/OneDrive) antes de guardar cambios".into(),
        )
    })?;
    Ok(compartida.join(CARPETA_OPS))
}

/// Aplica un cambio compartido y lo publica como evento, todo o nada:
/// 1. `aplicar` escribe en la base, dentro de una transacción;
/// 2. se escribe el archivo del evento en `ops/`;
/// 3. se anota en `eventos_aplicados` y se confirma la transacción.
///
/// Si `aplicar` o la escritura fallan, la base queda como estaba y no hay archivo.
/// Devuelve el id del evento (el nombre de su archivo).
pub fn publicar(
    c: &Connection,
    ops: &Path,
    evento: &Evento,
    aplicar: impl FnOnce(&Connection) -> Resultado<()>,
) -> Resultado<String> {
    let tx = c.unchecked_transaction()?;
    aplicar(&tx)?;
    let id = escribir_evento(ops, evento)?;
    marcar_aplicado(&tx, &id, &evento.timestamp)?;
    tx.commit()?;
    Ok(id)
}

/// Escribe el evento como archivo nuevo en `ops/` y devuelve su nombre.
pub fn escribir_evento(ops: &Path, evento: &Evento) -> Resultado<String> {
    let nombre = nombre_archivo(&evento.timestamp, &evento.usuario, &uuid())?;
    escribir_con_nombre(ops, &nombre, evento)?;
    Ok(nombre)
}

fn ya_existe(nombre: &str) -> Error {
    Error::Interno(format!(
        "Ya existe el evento {nombre}: los eventos no se sobrescriben"
    ))
}

fn escribir_con_nombre(ops: &Path, nombre: &str, evento: &Evento) -> Resultado<()> {
    std::fs::create_dir_all(ops)?;
    let destino = ops.join(nombre);
    if destino.exists() {
        return Err(ya_existe(nombre));
    }
    let json = serde_json::to_string_pretty(evento).map_err(|e| Error::Interno(e.to_string()))?;
    // Temporal con punto al inicio: quien lee `ops/` (#92) ignora lo que no termina en `.json`.
    let temporal = ops.join(format!(".{nombre}.tmp"));
    let escrito = (|| -> std::io::Result<()> {
        let mut archivo = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporal)?;
        archivo.write_all(json.as_bytes())?;
        archivo.sync_all()
    })();
    if let Err(e) = escrito {
        let _ = std::fs::remove_file(&temporal);
        return Err(e.into());
    }
    if destino.exists() {
        let _ = std::fs::remove_file(&temporal);
        return Err(ya_existe(nombre));
    }
    std::fs::rename(&temporal, &destino).map_err(|e| {
        let _ = std::fs::remove_file(&temporal);
        Error::from(e)
    })
}

/// `20261114T183005123Z_ana@virustf.dev_<32 hex>.json`. El timestamp va sin `:`
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
fn uuid() -> String {
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

/// Carpeta compartida (Drive/OneDrive) del proyecto en esta máquina, si ya se configuró.
pub fn carpeta_compartida(carpeta_proyecto: &Path) -> Resultado<Option<PathBuf>> {
    Ok(leer_locales(carpeta_proyecto)?.carpeta_compartida)
}

/// Marca que dice de qué proyecto es una carpeta compartida.
#[derive(Debug, Serialize, Deserialize)]
struct MarcaProyecto {
    /// El mismo identificador que la carpeta local del proyecto (`<org>-<repo>`).
    proyecto: String,
}

/// Lo que hay en la carpeta que se eligió como compartida.
fn leer_marca(compartida: &Path) -> Resultado<Option<MarcaProyecto>> {
    match std::fs::read_to_string(compartida.join(ARCHIVO_MARCA)) {
        Ok(texto) => serde_json::from_str(&texto).map(Some).map_err(|_| {
            Error::Validacion(format!(
                "El archivo {ARCHIVO_MARCA} de esa carpeta está dañado"
            ))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Guarda la carpeta compartida del proyecto en esta máquina. Tiene que ser una
/// carpeta que ya existe (la que sincroniza Drive/OneDrive), no la carpeta local
/// de datos de la app, y **la de este proyecto**:
/// - si tiene la marca `branchboard-proyecto.json`, debe ser de este proyecto;
/// - si no la tiene, solo quien `puede_inicializar` (el Administrador) la deja
///   como carpeta del proyecto y le escribe la marca. A los demás se les pide la
///   carpeta que les compartió el Administrador, así nadie publica sus cambios en
///   una carpeta que el resto del equipo nunca lee.
pub fn configurar_carpeta_compartida(
    carpeta_proyecto: &Path,
    ruta: &str,
    puede_inicializar: bool,
) -> Resultado<PathBuf> {
    let ruta = PathBuf::from(ruta.trim());
    if !ruta.is_absolute() {
        return Err(Error::Validacion(
            "La carpeta compartida tiene que ser una ruta completa (ej. C:\\Users\\ana\\OneDrive\\MiJuego)".into(),
        ));
    }
    if !ruta.is_dir() {
        return Err(Error::Validacion(format!(
            "No existe la carpeta {}",
            ruta.display()
        )));
    }
    let ruta = sin_prefijo_extendido(ruta.canonicalize()?);
    std::fs::create_dir_all(carpeta_proyecto)?;
    let local = sin_prefijo_extendido(carpeta_proyecto.canonicalize()?);
    let datos = local
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&local)
        .to_path_buf();
    if ruta.starts_with(&datos) || datos.starts_with(&ruta) {
        return Err(Error::Validacion(
            "La carpeta compartida no puede ser la carpeta local de datos de BranchBoard".into(),
        ));
    }
    let id = local
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| Error::Interno("La carpeta local del proyecto no tiene nombre".into()))?;
    match leer_marca(&ruta)? {
        Some(marca) if marca.proyecto == id => {}
        Some(marca) => {
            return Err(Error::Validacion(format!(
                "Esa carpeta es de otro proyecto ({}). Elige la carpeta compartida de este proyecto",
                marca.proyecto
            )))
        }
        None if puede_inicializar => {
            let json = serde_json::to_string_pretty(&MarcaProyecto { proyecto: id })
                .map_err(|e| Error::Interno(e.to_string()))?;
            let mut archivo = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(ruta.join(ARCHIVO_MARCA))?;
            archivo.write_all(json.as_bytes())?;
            archivo.sync_all()?;
        }
        None => {
            return Err(Error::Validacion(format!(
                "Esa carpeta todavía no es la de ningún proyecto de BranchBoard. Elige la carpeta que te compartió el Administrador (tiene el archivo {ARCHIVO_MARCA}); si acabas de recibirla, espera a que Drive/OneDrive termine de sincronizarla"
            )))
        }
    }
    let mut ajustes = leer_locales(carpeta_proyecto)?;
    ajustes.carpeta_compartida = Some(ruta.clone());
    let json = serde_json::to_string_pretty(&ajustes).map_err(|e| Error::Interno(e.to_string()))?;
    std::fs::write(carpeta_proyecto.join(ARCHIVO_LOCAL), json)?;
    Ok(ruta)
}

/// En Windows `canonicalize` devuelve `\\?\C:\…`; para guardar y mostrar se deja
/// como `C:\…` (solo si es una ruta normal con letra de unidad).
fn sin_prefijo_extendido(ruta: PathBuf) -> PathBuf {
    let texto = ruta.to_string_lossy();
    match texto.strip_prefix(r"\\?\") {
        Some(resto) if resto.as_bytes().get(1) == Some(&b':') => PathBuf::from(resto),
        _ => ruta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn archivos(ops: &Path) -> Vec<String> {
        let mut lista: Vec<String> = std::fs::read_dir(ops)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        lista.sort();
        lista
    }

    fn contar(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |f| f.get(0)).unwrap()
    }

    #[test]
    fn nombre_del_archivo_sin_dos_puntos() {
        let n = nombre_archivo("2026-11-14T18:30:05.123Z", "Ana@VirusTF.dev", "abc").unwrap();
        assert_eq!(n, "20261114T183005123Z_ana@virustf.dev_abc.json");
        let raro = nombre_archivo("2026-11-14T13:30:05.123-05:00", "a/b\\c:d@x.dev", "u").unwrap();
        assert_eq!(
            raro, "20261114T183005123Z_a_b_c_d@x.dev_u.json",
            "en UTC y sin caracteres de ruta"
        );
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
        assert_eq!(uuid().len(), 32);
        assert_ne!(uuid(), uuid());
    }

    #[test]
    fn escribe_un_archivo_nuevo_y_no_deja_temporales() {
        let dir = tempfile::tempdir().unwrap();
        let ops = dir.path().join(CARPETA_OPS);
        let id = escribir_evento(&ops, &evento_de_prueba()).unwrap();
        assert!(id.starts_with("20261114T183005123Z_ana@virustf.dev_") && id.ends_with(".json"));
        assert_eq!(archivos(&ops), std::slice::from_ref(&id));
        let leido: Evento =
            serde_json::from_str(&std::fs::read_to_string(ops.join(&id)).unwrap()).unwrap();
        assert_eq!(leido, evento_de_prueba());
    }

    #[test]
    fn nunca_sobrescribe_un_evento() {
        let dir = tempfile::tempdir().unwrap();
        let ops = dir.path().join(CARPETA_OPS);
        let nombre = "20261114T183005123Z_ana@virustf.dev_fijo.json";
        escribir_con_nombre(&ops, nombre, &evento_de_prueba()).unwrap();
        let antes = std::fs::read_to_string(ops.join(nombre)).unwrap();
        let mut otro = evento_de_prueba();
        otro.tipo = "eliminar_tarea".into();
        assert!(escribir_con_nombre(&ops, nombre, &otro).is_err());
        assert_eq!(std::fs::read_to_string(ops.join(nombre)).unwrap(), antes);
        assert_eq!(archivos(&ops), [nombre.to_string()], "sin temporales");
    }

    #[test]
    fn publicar_aplica_escribe_y_anota() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        let dir = tempfile::tempdir().unwrap();
        let ops = dir.path().join(CARPETA_OPS);
        let id = publicar(c, &ops, &evento_de_prueba(), |tx| {
            tx.execute(
                "INSERT INTO meta (clave, valor) VALUES ('prueba', 'aplicado')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert!(ya_aplicado(c, &id).unwrap());
        assert!(!ya_aplicado(c, "otro.json").unwrap());
        assert!(ops.join(&id).is_file());
        assert_eq!(
            contar(c, "SELECT count(*) FROM meta WHERE clave = 'prueba'"),
            1
        );
    }

    #[test]
    fn si_aplicar_falla_no_queda_nada() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        let dir = tempfile::tempdir().unwrap();
        let ops = dir.path().join(CARPETA_OPS);
        let e = publicar(c, &ops, &evento_de_prueba(), |tx| {
            tx.execute("INSERT INTO meta (clave, valor) VALUES ('x', '1')", [])?;
            Err(Error::Validacion("regla rota".into()))
        })
        .unwrap_err();
        assert_eq!(e.codigo(), "validacion");
        assert!(!ops.exists() || archivos(&ops).is_empty(), "sin archivo");
        assert_eq!(contar(c, "SELECT count(*) FROM meta WHERE clave = 'x'"), 0);
    }

    #[test]
    fn si_escribir_falla_la_base_queda_como_estaba() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        let dir = tempfile::tempdir().unwrap();
        // `ops` es un archivo, no una carpeta: escribir falla.
        let ops = dir.path().join(CARPETA_OPS);
        std::fs::write(&ops, "no soy carpeta").unwrap();
        assert!(publicar(c, &ops, &evento_de_prueba(), |tx| {
            tx.execute("INSERT INTO meta (clave, valor) VALUES ('y', '1')", [])?;
            Ok(())
        })
        .is_err());
        assert_eq!(contar(c, "SELECT count(*) FROM meta WHERE clave = 'y'"), 0);
        assert_eq!(contar(c, "SELECT count(*) FROM eventos_aplicados"), 0);
    }

    #[test]
    fn la_carpeta_tiene_que_ser_la_de_este_proyecto() {
        let datos = tempfile::tempdir().unwrap();
        let proyecto = datos.path().join("proyectos").join("MiguelM17-Juego");
        let nube = tempfile::tempdir().unwrap();
        let ruta = nube.path().to_string_lossy().to_string();

        // Sin marca, quien no es Administrador no puede dejarla como la del proyecto.
        let e = configurar_carpeta_compartida(&proyecto, &ruta, false).unwrap_err();
        assert!(
            e.to_string().contains("te compartió el Administrador"),
            "{e}"
        );
        assert!(!nube.path().join(ARCHIVO_MARCA).exists());
        assert_eq!(carpeta_compartida(&proyecto).unwrap(), None);

        // El Administrador sí, y le deja la marca.
        configurar_carpeta_compartida(&proyecto, &ruta, true).unwrap();
        let marca: MarcaProyecto = serde_json::from_str(
            &std::fs::read_to_string(nube.path().join(ARCHIVO_MARCA)).unwrap(),
        )
        .unwrap();
        assert_eq!(marca.proyecto, "MiguelM17-Juego");

        // Con la marca, cualquiera del equipo la elige (otra máquina, mismo proyecto).
        let otra_maquina = tempfile::tempdir().unwrap();
        let proyecto_otra = otra_maquina
            .path()
            .join("proyectos")
            .join("MiguelM17-Juego");
        configurar_carpeta_compartida(&proyecto_otra, &ruta, false).unwrap();

        // Pero no desde otro proyecto.
        let ajeno = otra_maquina.path().join("proyectos").join("MiguelM17-Otro");
        let e = configurar_carpeta_compartida(&ajeno, &ruta, true).unwrap_err();
        assert!(
            e.to_string().contains("otro proyecto (MiguelM17-Juego)"),
            "{e}"
        );
        assert_eq!(carpeta_compartida(&ajeno).unwrap(), None);
    }

    #[test]
    fn la_ruta_guardada_no_lleva_el_prefijo_extendido() {
        assert_eq!(
            sin_prefijo_extendido(PathBuf::from(r"\\?\C:\Users\ana\OneDrive")),
            PathBuf::from(r"C:\Users\ana\OneDrive")
        );
        let red = PathBuf::from(r"\\?\UNC\servidor\carpeta");
        assert_eq!(
            sin_prefijo_extendido(red.clone()),
            red,
            "las de red se dejan igual"
        );
        let nube = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let proyecto = datos.path().join("proyectos").join("p");
        let guardada =
            configurar_carpeta_compartida(&proyecto, &nube.path().to_string_lossy(), true).unwrap();
        assert!(
            !guardada.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            guardada.display()
        );
    }

    #[test]
    fn carpeta_compartida_configurable_por_maquina() {
        let datos = tempfile::tempdir().unwrap();
        let proyecto = datos.path().join("proyectos").join("org-repo");
        let nube = tempfile::tempdir().unwrap();
        assert_eq!(carpeta_compartida(&proyecto).unwrap(), None);
        assert_eq!(carpeta_ops(&proyecto).unwrap_err().codigo(), "validacion");

        let ruta =
            configurar_carpeta_compartida(&proyecto, &nube.path().to_string_lossy(), true).unwrap();
        assert_eq!(carpeta_compartida(&proyecto).unwrap(), Some(ruta.clone()));
        assert_eq!(carpeta_ops(&proyecto).unwrap(), ruta.join(CARPETA_OPS));

        let no_existe = nube.path().join("no-existe");
        for mala in ["relativa/carpeta", "", &no_existe.to_string_lossy()] {
            assert_eq!(
                configurar_carpeta_compartida(&proyecto, mala, true)
                    .unwrap_err()
                    .codigo(),
                "validacion",
                "{mala}"
            );
        }
        let local = configurar_carpeta_compartida(&proyecto, &datos.path().to_string_lossy(), true);
        assert_eq!(
            local.unwrap_err().codigo(),
            "validacion",
            "no la carpeta local de datos"
        );
        assert_eq!(
            carpeta_compartida(&proyecto).unwrap(),
            Some(ruta),
            "la buena sigue"
        );
    }

    /// **Prueba en el entorno real** (CLAUDE.md): la carpeta compartida de verdad
    /// (una carpeta que sincroniza OneDrive/Drive en esta máquina). Publica eventos
    /// en una subcarpeta temporal, espera a que el cliente de sincronización la
    /// procese y comprueba que los archivos siguen intactos (sin copias en conflicto
    /// ni renombres). Al final borra la subcarpeta.
    ///
    /// ```text
    /// BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive cargo test --lib entorno_real_eventos -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa la carpeta de OneDrive/Drive de verdad: correr a mano con BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_eventos_en_la_carpeta_compartida() {
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let compartida = nube.join(format!(
            "BranchBoard-prueba-91-{}",
            chrono::Local::now().format("%H%M%S")
        ));
        std::fs::create_dir_all(&compartida).unwrap();
        let datos = tempfile::tempdir().unwrap();
        let proyecto = datos.path().join("proyectos").join("prueba-91");

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            configurar_carpeta_compartida(&proyecto, &compartida.to_string_lossy(), true).unwrap();
            let ops = carpeta_ops(&proyecto).unwrap();
            let db = Db::en_memoria().unwrap();
            let c = db.conexion();
            let mut ids = Vec::new();
            for i in 0..3 {
                let evento = Evento::nuevo(
                    "prueba_91",
                    Some(&format!("T-{i}")),
                    "ana@virustf.dev",
                    json!({ "n": i, "texto": "ñandú y acentos" }),
                );
                ids.push(
                    publicar(c, &ops, &evento, |tx| {
                        tx.execute(
                            "INSERT INTO meta (clave, valor) VALUES (?1, 'ok')",
                            [format!("prueba-{i}")],
                        )?;
                        Ok(())
                    })
                    .unwrap(),
                );
            }
            println!("escritos en {}: {ids:?}", ops.display());

            // Dar tiempo al cliente de OneDrive/Drive a ver y subir los archivos.
            std::thread::sleep(std::time::Duration::from_secs(20));
            let en_carpeta = archivos(&ops);
            println!("en la carpeta tras 20 s: {en_carpeta:?}");
            let mut esperados = ids.clone();
            esperados.sort();
            assert_eq!(
                en_carpeta, esperados,
                "ni temporales, ni copias en conflicto, ni renombres"
            );
            for (i, id) in ids.iter().enumerate() {
                let leido: Evento =
                    serde_json::from_str(&std::fs::read_to_string(ops.join(id)).unwrap()).unwrap();
                assert_eq!(leido.payload["n"], i);
                assert_eq!(leido.payload["texto"], "ñandú y acentos");
                assert!(ya_aplicado(c, id).unwrap());
            }

            // La marca del proyecto quedó en la carpeta de OneDrive y la reconoce otra
            // "máquina" del mismo proyecto aunque no sea Administrador.
            assert!(compartida.join(ARCHIVO_MARCA).is_file());
            let otra = tempfile::tempdir().unwrap();
            let mismo = otra.path().join("proyectos").join("prueba-91");
            configurar_carpeta_compartida(&mismo, &compartida.to_string_lossy(), false).unwrap();
            // Una carpeta de OneDrive sin marca no se le acepta a quien no es Administrador.
            let vacia = compartida.join("otra-carpeta");
            std::fs::create_dir_all(&vacia).unwrap();
            let e =
                configurar_carpeta_compartida(&mismo, &vacia.to_string_lossy(), false).unwrap_err();
            println!("carpeta sin marca, sin ser Administrador: {e}");
            assert_eq!(e.codigo(), "validacion");
        }));
        let borrada = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", borrada.is_ok());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
