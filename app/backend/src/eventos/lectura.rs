//! Leer `ops/` y aplicar lo que le falta a esta máquina (issue #92).
//! Ver "Sincronización entre máquinas" en BACKEND.md.
//!
//! - Solo se leen los `.json` que no están en `eventos_aplicados` (los temporales
//!   `.…tmp` de quien está escribiendo se ignoran). Un archivo que no se puede leer
//!   o que no es un evento se salta y se reintenta en el próximo escaneo: puede ser
//!   que Drive/OneDrive todavía lo esté bajando.
//! - Todas las máquinas los aplican **en el mismo orden**: por `timestamp`, luego
//!   por `usuario` y al final por nombre de archivo, así todas llegan al mismo estado.
//! - Un evento marcado más de **5 minutos en el futuro** respecto al reloj de esta
//!   máquina se ordena por su **hora de llegada** a `ops/` (fecha del archivo) y se
//!   anota en `eventos.log` de la carpeta local del proyecto (no se avisa al equipo).
//! - Cada evento se aplica en su propia transacción con el aplicador de su tipo:
//!   aplicado o descartado (ej. una toma doble que perdió) → se anota y no se repite;
//!   tipo desconocido (lo escribió una versión más nueva de la app) → queda
//!   pendiente para cuando esta máquina se actualice.
//! - Los eventos administrativos (`eventos::TIPOS_FIRMADOS`) tienen que traer una
//!   `firma_admin` válida; si falta o no coincide se descartan sin aplicar (#93).

use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::Connection;
use serde::Serialize;

use super::snapshot::{self, es_snapshot};
use super::{marcar_aplicado, ya_aplicado, Evento};
use crate::chat;
use crate::documentos;
use crate::error::{Error, Resultado};
use crate::identidad::equipo;
use crate::modelo::Rol;
use crate::notificaciones;
use crate::sprints;
use crate::tareas::{self, InfoDobleToma};

/// Cuánto puede adelantarse el reloj de otra máquina antes de desconfiar.
const TOLERANCIA_FUTURO_MIN: i64 = 5;
/// Avisos internos de la sincronización, en la carpeta local del proyecto.
pub const ARCHIVO_LOG: &str = "eventos.log";

/// Qué pasó al aplicar un evento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aplicacion {
    /// Se aplicó a la base.
    Aplicado,
    /// No se aplicó porque ya no cumple las reglas (ej. la tarea ya la tomó
    /// otro). Igual se anota como procesado: el resultado es el mismo en todas
    /// las máquinas.
    Descartado(String),
    /// Esta versión de la app no conoce el tipo: queda pendiente.
    Desconocido,
}

/// Resumen de un escaneo, para el frontend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ResumenSincronizacion {
    pub aplicados: usize,
    pub descartados: usize,
    /// De un tipo que esta versión no conoce (quedan pendientes).
    pub desconocidos: usize,
    /// Archivos que no se pudieron leer (se reintentan en el próximo escaneo).
    pub ilegibles: usize,
}

impl ResumenSincronizacion {
    /// `true` si cambió algo en la base y el frontend debe refrescarse.
    pub fn hubo_cambios(&self) -> bool {
        self.aplicados > 0
    }
}

/// Un evento pendiente, con lo que hace falta para ordenarlo.
#[derive(Debug, Clone)]
struct Pendiente {
    id: String,
    evento: Evento,
    /// Momento por el que se ordena (el declarado, o la llegada si es sospechoso).
    orden: DateTime<Utc>,
}

/// Escanea `ops/` y aplica, en orden, todo lo que esta base todavía no tiene.
/// `aplicar` decide qué hace cada tipo de evento. `ahora` es el reloj de esta
/// máquina. Los avisos van a `carpeta_proyecto/eventos.log`.
pub fn sincronizar(
    c: &Connection,
    ops: &Path,
    carpeta_proyecto: &Path,
    ahora: DateTime<Utc>,
    aplicar: impl Fn(&Connection, &Evento) -> Resultado<Aplicacion>,
) -> Resultado<ResumenSincronizacion> {
    sincronizar_con_doble_toma(c, ops, carpeta_proyecto, ahora, "", aplicar, |_| Ok(false))
}

/// Igual que [`sincronizar`], pero al detectar una doble toma (`tomar_tarea`
/// sobre una tarea ya asignada) consulta `revisar_rama_perdedor` para saber si
/// la rama del perdedor ya tiene changesets reales (y cerrarla si no tiene)
/// antes de aplicar la resolución en SQLite (#97).
pub fn sincronizar_con_doble_toma(
    c: &Connection,
    ops: &Path,
    carpeta_proyecto: &Path,
    ahora: DateTime<Utc>,
    publica_admin: &str,
    aplicar: impl Fn(&Connection, &Evento) -> Resultado<Aplicacion>,
    revisar_rama_perdedor: impl Fn(&InfoDobleToma) -> Resultado<bool>,
) -> Resultado<ResumenSincronizacion> {
    let mut resumen = ResumenSincronizacion::default();
    let mut avisos = Vec::new();
    let pendientes = leer_pendientes(c, ops, ahora, &mut resumen, &mut avisos)?;
    for p in pendientes {
        if !es_snapshot(&p.id)
            && snapshot::ultimo_snapshot_hasta(c)?.is_some_and(|corte| p.orden <= corte)
        {
            marcar_aplicado(c, &p.id, &ahora_iso(ahora))?;
            continue;
        }
        let tx = c.unchecked_transaction()?;
        match aplicar(&tx, &p.evento)? {
            Aplicacion::Aplicado => {
                marcar_aplicado(&tx, &p.id, &ahora_iso(ahora))?;
                tx.commit()?;
                resumen.aplicados += 1;
            }
            Aplicacion::Descartado(motivo) => {
                let doble = if p.evento.tipo == "tomar_tarea" {
                    tareas::detectar_doble_toma(&tx, ops, &p.id, &p.evento)?
                } else {
                    None
                };
                // Lo que haya escrito el aplicador antes de descartar no se guarda.
                drop(tx);
                if let Some(info) = doble {
                    let tiene_changesets = revisar_rama_perdedor(&info)?;
                    let tx2 = c.unchecked_transaction()?;
                    let aplicado = tareas::aplicar_resolucion_doble_toma(
                        &tx2,
                        publica_admin,
                        &info,
                        tiene_changesets,
                        &ahora_iso(ahora),
                    )?;
                    marcar_aplicado(&tx2, &p.id, &ahora_iso(ahora))?;
                    tx2.commit()?;
                    if aplicado {
                        resumen.aplicados += 1;
                    } else {
                        avisos.push(format!("descartado {} (doble toma): {motivo}", p.id));
                        resumen.descartados += 1;
                    }
                } else {
                    marcar_aplicado(c, &p.id, &ahora_iso(ahora))?;
                    avisos.push(format!("descartado {}: {motivo}", p.id));
                    resumen.descartados += 1;
                }
            }
            Aplicacion::Desconocido => {
                drop(tx);
                resumen.desconocidos += 1;
            }
        }
    }
    if !avisos.is_empty() {
        registrar(carpeta_proyecto, ahora, &avisos);
    }
    Ok(resumen)
}

fn leer_pendientes(
    c: &Connection,
    ops: &Path,
    ahora: DateTime<Utc>,
    resumen: &mut ResumenSincronizacion,
    avisos: &mut Vec<String>,
) -> Resultado<Vec<Pendiente>> {
    let entradas = match std::fs::read_dir(ops) {
        Ok(e) => e,
        // Todavía nadie escribió nada.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let limite = ahora + Duration::minutes(TOLERANCIA_FUTURO_MIN);
    let mut pendientes = Vec::new();
    for entrada in entradas {
        let entrada = entrada?;
        let id = entrada.file_name().to_string_lossy().to_string();
        if id.starts_with('.') || !id.ends_with(".json") || ya_aplicado(c, &id)? {
            continue;
        }
        let Some(evento) = std::fs::read_to_string(entrada.path())
            .ok()
            .and_then(|t| serde_json::from_str::<Evento>(&t).ok())
        else {
            resumen.ilegibles += 1;
            continue;
        };
        let Ok(declarado) = DateTime::parse_from_rfc3339(&evento.timestamp) else {
            resumen.ilegibles += 1;
            continue;
        };
        let declarado = declarado.with_timezone(&Utc);
        let orden = if declarado > limite {
            let llegada = entrada
                .metadata()
                .and_then(|m| m.modified())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| DateTime::<Utc>::from(SystemTime::now()));
            avisos.push(format!(
                "{id} viene {} min en el futuro (declara {}); se ordena por su llegada ({})",
                (declarado - ahora).num_minutes(),
                evento.timestamp,
                ahora_iso(llegada)
            ));
            llegada
        } else {
            declarado
        };
        pendientes.push(Pendiente { id, evento, orden });
    }
    pendientes.sort_by(|a, b| match (es_snapshot(&a.id), es_snapshot(&b.id)) {
        // Los snapshots van antes que los eventos sueltos, del más reciente al
        // más antiguo: al reconstruir el `.db` se parte del último snapshot
        // válido (#94) y los anteriores quedan ya cubiertos.
        (true, true) => b.orden.cmp(&a.orden).then_with(|| b.id.cmp(&a.id)),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => a
            .orden
            .cmp(&b.orden)
            .then_with(|| a.evento.usuario.cmp(&b.evento.usuario))
            .then_with(|| a.id.cmp(&b.id)),
    });
    Ok(pendientes)
}

fn ahora_iso(momento: DateTime<Utc>) -> String {
    momento.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Agrega los avisos a `eventos.log`. Si no se puede escribir, no frena la sincronización.
fn registrar(carpeta_proyecto: &Path, ahora: DateTime<Utc>, avisos: &[String]) {
    let _ = std::fs::create_dir_all(carpeta_proyecto);
    if let Ok(mut log) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(carpeta_proyecto.join(ARCHIVO_LOG))
    {
        for aviso in avisos {
            let _ = writeln!(log, "{} {aviso}", ahora_iso(ahora));
        }
    }
}

/// Aplicador de la app: cada tipo de evento compartido se registra aquí cuando su
/// comando pasa a eventos (#93 usuarios, #95 tareas, …).
///
/// 1. Si el tipo exige firma (`eventos::TIPOS_FIRMADOS`), primero se comprueba
///    `firma_admin` contra la llave del Administrador fijada en esta máquina
///    (`publica_admin`) o la de un Líder con certificado vigente. Sin firma
///    válida el evento se **descarta** sin mirar su contenido, sea del tipo que
///    sea: no hay "rol mínimo" al que degradar un evento.
/// 2. Eventos de usuarios → `identidad::equipo::aplicar_evento`, con las mismas
///    reglas que en la máquina que lo emitió. Si ya no las cumple (ej. el correo
///    ya existe), se descarta; un error de la base detiene el escaneo.
/// 3. Lo demás todavía es desconocido (#95 tareas…).
pub fn aplicar_evento(
    c: &Connection,
    publica_admin: &str,
    evento: &Evento,
) -> Resultado<Aplicacion> {
    let rol_firmante = if evento.exige_firma() {
        match equipo::firmante_del_evento(c, publica_admin, evento)? {
            Some(rol) => Some(rol),
            None => {
                return Ok(Aplicacion::Descartado(
                    "firma_admin falta o no coincide".into(),
                ))
            }
        }
    } else {
        None
    };
    match rol_firmante {
        Some(rol) if equipo::TIPOS.contains(&evento.tipo.as_str()) => {
            match equipo::aplicar_evento(c, publica_admin, rol, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        Some(rol) if evento.tipo == "compactar_eventos" => {
            if rol != Rol::Administrador {
                return Ok(Aplicacion::Descartado(Error::SinPermiso.to_string()));
            }
            match snapshot::aplicar_evento(c, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        None if tareas::TIPOS.contains(&evento.tipo.as_str()) => {
            match tareas::aplicar_evento(c, publica_admin, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        None if sprints::TIPOS.contains(&evento.tipo.as_str()) => {
            match sprints::aplicar_evento(c, publica_admin, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        None if notificaciones::TIPOS.contains(&evento.tipo.as_str()) => {
            match notificaciones::aplicar_evento(c, publica_admin, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        None if chat::TIPOS.contains(&evento.tipo.as_str()) => {
            match chat::aplicar_evento(c, publica_admin, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        None if documentos::TIPOS.contains(&evento.tipo.as_str()) => {
            match documentos::aplicar_evento(c, publica_admin, None, evento) {
                Ok(()) => Ok(Aplicacion::Aplicado),
                Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
                Err(e) => Err(e),
            }
        }
        _ => Ok(Aplicacion::Desconocido),
    }
}

/// Errores que significan "el evento no cumple las reglas con el estado actual"
/// (se descarta), a diferencia de un fallo de la base o del disco.
fn es_regla_rota(e: &Error) -> bool {
    matches!(
        e,
        Error::Validacion(_) | Error::SinPermiso | Error::NoEncontrado(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::error::Error;
    use crate::eventos::{escribir_evento, publicar};
    use serde_json::json;

    fn momento(texto: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(texto)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn evento(cuando: &str, usuario: &str, n: i64) -> Evento {
        let mut e = Evento::nuevo("prueba", None, usuario, json!({ "n": n }));
        e.timestamp = cuando.to_string();
        e
    }

    /// Aplicador de prueba: guarda el orden en `meta` (`orden` = lista de `n`).
    /// `n` negativo se descarta; tipo `nuevo` es desconocido.
    fn aplicador(c: &Connection, e: &Evento) -> Resultado<Aplicacion> {
        if e.tipo == "nuevo" {
            return Ok(Aplicacion::Desconocido);
        }
        let n = e.payload["n"].as_i64().unwrap();
        c.execute(
            "INSERT INTO meta (clave, valor) VALUES ('orden', ?1)
             ON CONFLICT (clave) DO UPDATE SET valor = valor || ',' || excluded.valor",
            [n.to_string()],
        )?;
        if n < 0 {
            return Ok(Aplicacion::Descartado("regla rota".into()));
        }
        Ok(Aplicacion::Aplicado)
    }

    fn orden(c: &Connection) -> String {
        c.query_row("SELECT valor FROM meta WHERE clave = 'orden'", [], |f| {
            f.get(0)
        })
        .unwrap_or_default()
    }

    struct Escenario {
        _dir: tempfile::TempDir,
        ops: std::path::PathBuf,
        local: std::path::PathBuf,
        db: Db,
    }

    fn escenario() -> Escenario {
        let dir = tempfile::tempdir().unwrap();
        let ops = dir.path().join("ops");
        let local = dir.path().join("local");
        Escenario {
            _dir: dir,
            ops,
            local,
            db: Db::en_memoria().unwrap(),
        }
    }

    const AHORA: &str = "2026-11-14T19:00:00.000Z";

    #[test]
    fn sin_carpeta_ops_no_hay_nada_que_hacer() {
        let e = escenario();
        let r = sincronizar(e.db.conexion(), &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(r, ResumenSincronizacion::default());
        assert!(!r.hubo_cambios());
    }

    #[test]
    fn aplica_en_orden_por_hora_y_desempata_por_usuario() {
        let e = escenario();
        let c = e.db.conexion();
        // Se escriben desordenados; dos con la misma hora.
        escribir_evento(&e.ops, &evento("2026-11-14T18:00:03.000Z", "ana@x.dev", 3)).unwrap();
        escribir_evento(&e.ops, &evento("2026-11-14T18:00:01.000Z", "luis@x.dev", 2)).unwrap();
        escribir_evento(&e.ops, &evento("2026-11-14T18:00:01.000Z", "ana@x.dev", 1)).unwrap();
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(r.aplicados, 3);
        assert_eq!(
            orden(c),
            "1,2,3",
            "hora, y con la misma hora, ana antes que luis"
        );

        // Otra vez: no repite nada.
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(r, ResumenSincronizacion::default());
        assert_eq!(orden(c), "1,2,3");
    }

    #[test]
    fn dos_maquinas_llegan_al_mismo_orden() {
        let e = escenario();
        for (i, (cuando, quien)) in [
            ("2026-11-14T18:00:05.000Z", "b@x.dev"),
            ("2026-11-14T18:00:05.000Z", "a@x.dev"),
            ("2026-11-14T17:59:00.000Z", "c@x.dev"),
        ]
        .iter()
        .enumerate()
        {
            escribir_evento(&e.ops, &evento(cuando, quien, i as i64)).unwrap();
        }
        let otra = Db::en_memoria().unwrap();
        sincronizar(e.db.conexion(), &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        sincronizar(otra.conexion(), &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(orden(e.db.conexion()), "2,1,0");
        assert_eq!(orden(otra.conexion()), orden(e.db.conexion()));
    }

    #[test]
    fn lo_que_publico_esta_maquina_no_se_aplica_otra_vez() {
        let e = escenario();
        let c = e.db.conexion();
        publicar(
            c,
            &e.ops,
            &evento("2026-11-14T18:00:00.000Z", "ana@x.dev", 7),
            |tx| aplicador(tx, &evento("2026-11-14T18:00:00.000Z", "ana@x.dev", 7)).map(|_| ()),
        )
        .unwrap();
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(r.aplicados, 0);
        assert_eq!(orden(c), "7", "una sola vez");
    }

    #[test]
    fn descartado_se_anota_sin_aplicar_y_desconocido_queda_pendiente() {
        let e = escenario();
        let c = e.db.conexion();
        escribir_evento(&e.ops, &evento("2026-11-14T18:00:00.000Z", "ana@x.dev", -1)).unwrap();
        let mut nuevo = evento("2026-11-14T18:00:01.000Z", "ana@x.dev", 9);
        nuevo.tipo = "nuevo".into();
        escribir_evento(&e.ops, &nuevo).unwrap();

        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!((r.aplicados, r.descartados, r.desconocidos), (0, 1, 1));
        assert_eq!(orden(c), "", "lo que escribió antes de descartar no quedó");
        let log = std::fs::read_to_string(e.local.join(ARCHIVO_LOG)).unwrap();
        assert!(
            log.contains("descartado") && log.contains("regla rota"),
            "{log}"
        );

        // El descartado no se repite; el desconocido sí se vuelve a intentar.
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!((r.descartados, r.desconocidos), (0, 1));
    }

    #[test]
    fn ignora_temporales_y_reintenta_los_ilegibles() {
        let e = escenario();
        let c = e.db.conexion();
        std::fs::create_dir_all(&e.ops).unwrap();
        std::fs::write(e.ops.join(".algo.json.tmp"), "a medias").unwrap();
        std::fs::write(e.ops.join("notas.txt"), "otra cosa").unwrap();
        std::fs::write(
            e.ops.join("20261114T180000000Z_ana@x.dev_roto.json"),
            "{ a medias",
        )
        .unwrap();
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(r.ilegibles, 1);
        assert_eq!(r.aplicados, 0);
        // Cuando termina de bajar, se aplica.
        std::fs::write(
            e.ops.join("20261114T180000000Z_ana@x.dev_roto.json"),
            serde_json::to_string(&evento("2026-11-14T18:00:00.000Z", "ana@x.dev", 4)).unwrap(),
        )
        .unwrap();
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!((r.aplicados, r.ilegibles), (1, 0));
    }

    #[test]
    fn evento_del_futuro_se_ordena_por_su_llegada_y_se_registra() {
        let e = escenario();
        let c = e.db.conexion();
        // Son las 19:00. "tramposo" declara las 23:00 (4 h en el futuro) para quedar
        // último, pero su archivo llegó a las 18:30: se ordena por la llegada y va
        // antes que el de ana (18:59). Por lo declarado habría ido después.
        let id = escribir_evento(
            &e.ops,
            &evento("2026-11-14T23:00:00.000Z", "tramposo@x.dev", 1),
        )
        .unwrap();
        let llegada: SystemTime = momento("2026-11-14T18:30:00.000Z").into();
        std::fs::File::options()
            .write(true)
            .open(e.ops.join(&id))
            .unwrap()
            .set_modified(llegada)
            .unwrap();
        let ahora = momento(AHORA);
        escribir_evento(&e.ops, &evento("2026-11-14T18:59:00.000Z", "ana@x.dev", 2)).unwrap();
        let r = sincronizar(c, &e.ops, &e.local, ahora, aplicador).unwrap();
        assert_eq!(r.aplicados, 2);
        assert_eq!(
            orden(c),
            "1,2",
            "ordenado por su llegada, no por lo que declara"
        );
        let log = std::fs::read_to_string(e.local.join(ARCHIVO_LOG)).unwrap();
        assert!(
            log.contains("tramposo@x.dev") && log.contains("en el futuro"),
            "{log}"
        );
        // Dentro de la tolerancia (4 min) no se registra nada.
        let e2 = escenario();
        escribir_evento(&e2.ops, &evento("2026-11-14T19:04:00.000Z", "ana@x.dev", 5)).unwrap();
        sincronizar(e2.db.conexion(), &e2.ops, &e2.local, ahora, aplicador).unwrap();
        assert!(!e2.local.join(ARCHIVO_LOG).exists());
    }

    #[test]
    fn un_error_de_la_base_detiene_y_no_marca() {
        let e = escenario();
        let c = e.db.conexion();
        escribir_evento(&e.ops, &evento("2026-11-14T18:00:00.000Z", "ana@x.dev", 1)).unwrap();
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), |_, _| {
            Err(Error::BaseDatos("disco lleno".into()))
        });
        assert!(r.is_err());
        let r = sincronizar(c, &e.ops, &e.local, momento(AHORA), aplicador).unwrap();
        assert_eq!(r.aplicados, 1, "se reintenta en el próximo escaneo");
    }

    #[test]
    fn los_tipos_que_exigen_firma_se_descartan_sin_ella_y_los_demas_siguen_desconocidos() {
        use crate::identidad::llaves;

        let db = Db::en_memoria().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let publica = llaves::generar_llaves_admin(dir.path(), None).unwrap();
        let admin = llaves::cargar_llave_admin(dir.path(), None).unwrap();

        // Un tipo cualquiera sin firma: todavía no lo conoce la app.
        assert_eq!(
            aplicar_evento(db.conexion(), &publica, &evento(AHORA, "a@x.dev", 1)).unwrap(),
            Aplicacion::Desconocido
        );
        // `compactar_eventos` (#94) exige firma del Administrador y un snapshot válido.
        let mut compactar = evento(AHORA, "a@x.dev", 1);
        compactar.tipo = "compactar_eventos".into();
        assert!(matches!(
            aplicar_evento(db.conexion(), &publica, &compactar).unwrap(),
            Aplicacion::Descartado(motivo) if motivo.contains("firma_admin")
        ));
        let firmado = compactar.firmar(&admin);
        assert!(matches!(
            aplicar_evento(db.conexion(), &publica, &firmado).unwrap(),
            Aplicacion::Descartado(_)
        ));
        // Un evento de usuarios bien firmado pero sin tabla `usuarios` detrás que lo
        // respalde: regla rota, se descarta (no detiene el escaneo).
        let mut usuario = evento(AHORA, "a@x.dev", 1);
        usuario.tipo = "eliminar_usuario".into();
        let usuario = usuario.firmar(&admin);
        assert!(matches!(
            aplicar_evento(db.conexion(), &publica, &usuario).unwrap(),
            Aplicacion::Descartado(_)
        ));
    }

    /// **Prueba en el entorno real** (CLAUDE.md): dos "máquinas" (cada una con su
    /// base y su carpeta local) comparten una subcarpeta de la carpeta de
    /// OneDrive/Drive de verdad. A (Administrador) configura la carpeta y publica;
    /// B (sin ser Administrador) elige la misma carpeta, sincroniza y llega al mismo
    /// estado; después B publica y A recibe solo lo de B. Borra la subcarpeta al final.
    ///
    /// ```text
    /// BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive cargo test --lib entorno_real_dos_maquinas -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa la carpeta de OneDrive/Drive de verdad: correr a mano con BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_dos_maquinas_por_la_carpeta_compartida() {
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida};

        let nube = std::path::PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let compartida = nube.join(format!(
            "BranchBoard-prueba-92-{}",
            chrono::Local::now().format("%H%M%S")
        ));
        std::fs::create_dir_all(&compartida).unwrap();
        let maquina_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let local_a = maquina_a.path().join("proyectos").join("prueba-92");
        let local_b = maquina_b.path().join("proyectos").join("prueba-92");

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();
            let db_a = Db::en_memoria().unwrap();
            let db_b = Db::en_memoria().unwrap();

            // A publica 3 cambios; el tercero con el reloj adelantado 2 h.
            let ahora = Utc::now();
            for (n, adelanto) in [(1, 0), (2, 0), (3, 120)] {
                let mut e = Evento::nuevo("prueba_92", None, "admin@x.dev", json!({ "n": n }));
                e.timestamp =
                    ahora_iso(ahora + Duration::minutes(adelanto) + Duration::milliseconds(n));
                publicar(db_a.conexion(), &ops_a, &e, |tx| {
                    aplicador(tx, &e).map(|_| ())
                })
                .unwrap();
            }
            std::thread::sleep(std::time::Duration::from_secs(5));

            // B trae los cambios de A.
            let r = sincronizar(db_b.conexion(), &ops_b, &local_b, Utc::now(), aplicador).unwrap();
            println!("B sincroniza: {r:?}, orden {}", orden(db_b.conexion()));
            assert_eq!(r.aplicados, 3);
            assert_eq!(
                orden(db_b.conexion()),
                orden(db_a.conexion()),
                "mismo estado que A"
            );
            let log = std::fs::read_to_string(local_b.join(ARCHIVO_LOG)).unwrap();
            println!("eventos.log de B: {}", log.trim());
            assert!(log.contains("en el futuro"));

            // B publica; A recibe solo eso, no lo suyo.
            let e = Evento::nuevo("prueba_92", None, "dev@x.dev", json!({ "n": 4 }));
            publicar(db_b.conexion(), &ops_b, &e, |tx| {
                aplicador(tx, &e).map(|_| ())
            })
            .unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
            let r = sincronizar(db_a.conexion(), &ops_a, &local_a, Utc::now(), aplicador).unwrap();
            println!("A sincroniza: {r:?}, orden {}", orden(db_a.conexion()));
            assert_eq!(r.aplicados, 1);
            assert_eq!(orden(db_a.conexion()), "1,2,3,4");
            assert_eq!(orden(db_b.conexion()), "1,2,3,4");

            // Otra vuelta en las dos: nada nuevo.
            assert!(
                !sincronizar(db_a.conexion(), &ops_a, &local_a, Utc::now(), aplicador)
                    .unwrap()
                    .hubo_cambios()
            );
            assert!(
                !sincronizar(db_b.conexion(), &ops_b, &local_b, Utc::now(), aplicador)
                    .unwrap()
                    .hubo_cambios()
            );
        }));
        let borrada = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", borrada.is_ok());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
