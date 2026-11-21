//! Aplicar los eventos que llegan de las demás PCs (issues #92 y #239).
//! Ver "Sincronización entre máquinas" en BACKEND.md.
//!
//! - Los eventos llegan del servidor (`recepcion`, #239), en el orden del servidor, y cada uno se
//!   aplica en su propia transacción con el aplicador de su tipo ([`aplicar_evento`]):
//!   aplicado o descartado (ej. una toma doble que perdió) → se anota y no se repite; tipo desconocido
//!   (lo escribió una versión más nueva de la app) → queda pendiente para cuando esta PC se actualice.
//! - Los cambios de equipo y la compactación (`eventos::TIPOS_RETIRADOS`) se descartan siempre: iban
//!   firmados y desde #243 no hay firmas (los roles los cambia el servidor, #241).
//! - Hasta #244 también se leían de `ops/` (Drive/OneDrive), ordenados por `timestamp`.

use std::io::Write;
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::Connection;
use serde::Serialize;

use super::{marcar_aplicado, Evento};
use crate::chat;
use crate::documentos;
use crate::error::{Error, Resultado};
use crate::eventos::TIPOS_RETIRADOS;
use crate::notificaciones;
use crate::proyecto;
use crate::sprints;
use crate::tareas::{self, InfoDobleToma};

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
    /// Eventos que no se pudieron leer (se reintentan en la próxima vuelta).
    pub ilegibles: usize,
}

impl ResumenSincronizacion {
    /// `true` si cambió algo en la base y el frontend debe refrescarse.
    pub fn hubo_cambios(&self) -> bool {
        self.aplicados > 0
    }

    /// Suma lo de `otro`.
    pub fn sumar(&mut self, otro: &ResumenSincronizacion) {
        self.aplicados += otro.aplicados;
        self.descartados += otro.descartados;
        self.desconocidos += otro.desconocidos;
        self.ilegibles += otro.ilegibles;
    }
}

/// Un evento que bajó del servidor y esta base todavía no aplicó.
#[derive(Debug, Clone)]
pub(super) struct Pendiente {
    pub id: String,
    pub evento: Evento,
    /// Su número de `orden` en el servidor (#239).
    pub orden_servidor: i64,
}

/// Aplica `pendientes` en el orden en que vienen, cada uno en su transacción (ver el inicio del módulo).
#[allow(clippy::too_many_arguments)]
pub(super) fn aplicar_pendientes(
    c: &Connection,
    ahora: DateTime<Utc>,
    pendientes: Vec<Pendiente>,
    resumen: &mut ResumenSincronizacion,
    avisos: &mut Vec<String>,
    aplicar: impl Fn(&Connection, &Evento) -> Resultado<Aplicacion>,
    revisar_rama_perdedor: impl Fn(&InfoDobleToma) -> Resultado<bool>,
) -> Resultado<()> {
    for p in pendientes {
        let tx = c.unchecked_transaction()?;
        match aplicar(&tx, &p.evento)? {
            Aplicacion::Aplicado => {
                marcar_aplicado(&tx, &p.id, &ahora_iso(ahora))?;
                tx.commit()?;
                resumen.aplicados += 1;
            }
            Aplicacion::Descartado(motivo) => {
                let doble = if p.evento.tipo == "tomar_tarea" {
                    tareas::detectar_doble_toma(&tx, p.orden_servidor, &p.id, &p.evento)?
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
    Ok(())
}

fn ahora_iso(momento: DateTime<Utc>) -> String {
    momento.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Agrega los avisos a `eventos.log`. Si no se puede escribir, no frena la sincronización.
pub(super) fn registrar(carpeta_proyecto: &Path, ahora: DateTime<Utc>, avisos: &[String]) {
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

/// Aplicador de la app: cada tipo de evento compartido se registra aquí.
///
/// 1. Los [`TIPOS_RETIRADOS`] (cambios de equipo y compactación, que iban firmados) se **descartan**
///    siempre (#243).
/// 2. Cada tipo va a su módulo, que revisa el rol del autor con la tabla `usuarios` y aplica las
///    mismas reglas que en la máquina que lo emitió. Si ya no las cumple, se descarta; un error de la
///    base detiene el escaneo.
/// 3. Un tipo que no está aquí lo escribió una versión más nueva: queda pendiente.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<Aplicacion> {
    // Foto del estado de un proyecto que se subió al servidor (#245). Solo la manda un Administrador
    // (lo revisa el servidor, `permisos_evento`) y solo se aplica en un proyecto vacío.
    if evento.tipo == super::importacion::TIPO {
        return match super::importacion::aplicar(c, evento) {
            Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
            otro => otro,
        };
    }
    if TIPOS_RETIRADOS.contains(&evento.tipo.as_str()) {
        return Ok(Aplicacion::Descartado(
            "los cambios de equipo y la compactación ya no viajan como eventos (#243)".into(),
        ));
    }
    let tipo = evento.tipo.as_str();
    let resultado = if crate::cambios_directos::TIPOS.contains(&tipo) {
        crate::cambios_directos::aplicar_evento(c, evento)
    } else if tareas::TIPOS.contains(&tipo) {
        tareas::aplicar_evento(c, evento)
    } else if sprints::TIPOS.contains(&tipo) {
        sprints::aplicar_evento(c, evento)
    } else if notificaciones::TIPOS.contains(&tipo) {
        notificaciones::aplicar_evento(c, evento)
    } else if chat::TIPOS.contains(&tipo) {
        chat::aplicar_evento(c, evento)
    } else if documentos::TIPOS.contains(&tipo) {
        documentos::aplicar_evento(c, None, evento)
    } else if proyecto::TIPOS.contains(&tipo) {
        proyecto::aplicar_evento(c, evento)
    } else {
        return Ok(Aplicacion::Desconocido);
    };
    match resultado {
        Ok(()) => Ok(Aplicacion::Aplicado),
        Err(e) if es_regla_rota(&e) => Ok(Aplicacion::Descartado(e.to_string())),
        Err(e) => Err(e),
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
    use crate::eventos::servidor_de_prueba;
    use serde_json::json;

    const AHORA: &str = "2026-11-14T18:00:00.000Z";

    fn evento(cuando: &str, usuario: &str, n: i64) -> Evento {
        let mut e = Evento::nuevo("prueba", None, usuario, json!({ "n": n }));
        e.timestamp = cuando.into();
        e
    }

    #[test]
    fn los_cambios_de_equipo_y_la_compactacion_se_descartan_y_lo_demas_sigue_desconocido() {
        let db = Db::en_memoria().unwrap();
        // Un tipo cualquiera: todavía no lo conoce la app.
        assert_eq!(
            aplicar_evento(db.conexion(), &evento(AHORA, "a@x.dev", 1)).unwrap(),
            Aplicacion::Desconocido
        );
        // Sin firmas (#243) nadie puede probar quién mandó un cambio de equipo o un snapshot: se
        // descartan siempre, sin mirar su contenido, y el equipo no cambia.
        for tipo in crate::eventos::TIPOS_RETIRADOS {
            let mut e = evento(AHORA, "a@x.dev", 1);
            e.tipo = (*tipo).into();
            e.payload = json!({ "email": "a@x.dev", "rol": "Administrador" });
            assert!(
                matches!(
                    aplicar_evento(db.conexion(), &e).unwrap(),
                    Aplicacion::Descartado(motivo) if motivo.contains("#243")
                ),
                "{tipo}"
            );
        }
        assert!(crate::db::usuarios::listar(db.conexion())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn por_el_servidor_lo_descartado_se_anota_y_lo_desconocido_queda_pendiente() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        let mut retirado = evento(AHORA, "a@x.dev", 1);
        retirado.tipo = "cambiar_rol".into();
        servidor_de_prueba::agregar(&retirado);
        servidor_de_prueba::agregar(&evento(AHORA, "a@x.dev", 2));
        let ahora = DateTime::parse_from_rfc3339(AHORA)
            .unwrap()
            .with_timezone(&Utc);
        let r = servidor_de_prueba::entregar(c, ahora).unwrap();
        assert_eq!((r.aplicados, r.descartados, r.desconocidos), (0, 1, 1));
        // La vuelta siguiente: el descartado no se repite y el desconocido sigue esperando.
        let r = servidor_de_prueba::entregar(c, ahora).unwrap();
        assert_eq!((r.descartados, r.desconocidos), (0, 1));
    }
}
