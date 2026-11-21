//! Notificaciones internas del equipo (campana y tabla `notificaciones`, issue #116).
//!
//! - Las notificaciones nacen siempre al aplicar eventos de `ops/`: ya sea desde
//!   eventos propios (`"crear_notificacion"` y `"crear_notificaciones"`, usados por
//!   comandos de comunicación como mensajes o solicitud de acción sobre la BD) o como
//!   efecto determinista dentro de los manejadores de `tareas`.
//! - El estado `leida` es local de cada máquina (`obtener_notificaciones`,
//!   `marcar_leida`, `desmarcar_leida`, `marcar_todas_leidas`).

use std::collections::BTreeSet;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::notificaciones::{self as db_notis, Notificacion};
use crate::db::{commits, tareas as db_tareas, usuarios};
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::{normalizar_email, EstadoTarea, Rol, TipoNotificacion, TipoTarea};
use crate::tareas::rol_de;

/// Días sin check-ins ni actividad antes de enviar recordatorio de inactividad (#117).
pub const UMBRAL_INACTIVIDAD_DIAS: i64 = 3;

/// Longitud máxima permitida para un mensaje manual enviado a otro miembro (#118).
pub const MAX_CARACTERES_MENSAJE: usize = 500;

/// Tipos de evento propios manejados por este módulo.
pub const TIPOS: &[&str] = &["crear_notificacion", "crear_notificaciones"];

/// Datos para crear una notificación dirigida a una persona concreta vía `ops/`.
#[derive(Debug, Clone)]
pub struct DatosNotificacion<'a> {
    pub tipo: TipoNotificacion,
    pub para: &'a str,
    pub titulo: &'a str,
    pub cuerpo: &'a str,
    pub tarea_id: Option<&'a str>,
    pub accionable: bool,
}

/// Datos para crear una notificación dirigida a todos los usuarios de ciertos roles vía `ops/`.
#[derive(Debug, Clone)]
pub struct DatosNotificacionRoles<'a> {
    pub tipo: TipoNotificacion,
    pub roles: &'a [Rol],
    pub titulo: &'a str,
    pub cuerpo: &'a str,
    pub tarea_id: Option<&'a str>,
    pub accionable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PayloadNotificaciones {
    notificaciones: Vec<Notificacion>,
}

/// Inserta `notificacion` en SQLite si su `id` todavía no existe (idempotente para
/// reproducir eventos de `ops/`). Normaliza `para` a minúsculas.
pub fn emitir_si_falta(c: &Connection, notificacion: &Notificacion) -> Resultado<()> {
    let mut fila = notificacion.clone();
    fila.para = normalizar_email(&fila.para);
    if db_notis::buscar(c, &fila.id)?.is_none() {
        db_notis::insertar(c, &fila)?;
    }
    Ok(())
}

/// Construye e inserta una notificación para cada usuario único cuyo rol pertenece a
/// `roles`, invocando `construir(email_normalizado)`.
pub fn emitir_a_roles_si_falta(
    c: &Connection,
    roles: &[Rol],
    construir: impl Fn(&str) -> Notificacion,
) -> Resultado<()> {
    let mut vistos = BTreeSet::new();
    for u in usuarios::listar(c)? {
        if !roles.contains(&u.rol) {
            continue;
        }
        let email = normalizar_email(&u.email);
        if email.is_empty() || !vistos.insert(email.clone()) {
            continue;
        }
        emitir_si_falta(c, &construir(&email))?;
    }
    Ok(())
}

/// Función común para crear una notificación para un destinatario y replicarla por `ops/` (#116).
pub fn crear(
    c: &Connection,
    emisor: &str,
    datos: &DatosNotificacion<'_>,
    ahora: &str,
) -> Resultado<Notificacion> {
    let id = format!("noti_{}", eventos::uuid());
    crear_con_id(c, emisor, &id, datos, ahora)
}

/// Igual que [`crear`], pero usando un `id` determinista para evitar duplicados cuando varias
/// máquinas detectan el mismo suceso antes de sincronizar `ops/` (p. ej. inactividad #117).
pub fn crear_con_id(
    c: &Connection,
    emisor: &str,
    id: &str,
    datos: &DatosNotificacion<'_>,
    ahora: &str,
) -> Resultado<Notificacion> {
    let id = id.trim();
    if id.is_empty() {
        return Err(Error::Validacion("La notificación no tiene id".into()));
    }
    let dest = normalizar_email(datos.para);
    if dest.is_empty() || usuarios::buscar(c, &dest)?.is_none() {
        return Err(Error::NoEncontrado(format!("el usuario {dest}")));
    }
    let titulo = datos.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion("La notificación no tiene título".into()));
    }
    let noti = Notificacion {
        id: id.to_string(),
        tipo: datos.tipo,
        para: dest,
        titulo: titulo.to_string(),
        cuerpo: datos.cuerpo.trim().to_string(),
        leida: false,
        fecha: ahora.to_string(),
        tarea_id: datos
            .tarea_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        accionable: datos.accionable,
    };

    let mut ev = Evento::nuevo(
        "crear_notificacion",
        noti.tarea_id.as_deref(),
        &normalizar_email(emisor),
        serde_json::to_value(&noti).map_err(|e| {
            Error::Interno(format!("no se pudo serializar crear_notificacion: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| emitir_si_falta(tx, &noti))?;
    Ok(noti)
}

/// Función común para notificar a todos los usuarios con alguno de `datos.roles` en un solo
/// evento de `ops/` (#116).
pub fn crear_para_roles(
    c: &Connection,
    emisor: &str,
    datos: &DatosNotificacionRoles<'_>,
    ahora: &str,
) -> Resultado<Vec<Notificacion>> {
    let titulo = datos.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion("La notificación no tiene título".into()));
    }
    let tarea_id = datos
        .tarea_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let cuerpo = datos.cuerpo.trim().to_string();

    let mut vistos = BTreeSet::new();
    let mut lote = Vec::new();
    for u in usuarios::listar(c)? {
        if !datos.roles.contains(&u.rol) {
            continue;
        }
        let email = normalizar_email(&u.email);
        if email.is_empty() || !vistos.insert(email.clone()) {
            continue;
        }
        lote.push(Notificacion {
            id: format!("noti_{}", eventos::uuid()),
            tipo: datos.tipo,
            para: email,
            titulo: titulo.to_string(),
            cuerpo: cuerpo.clone(),
            leida: false,
            fecha: ahora.to_string(),
            tarea_id: tarea_id.clone(),
            accionable: datos.accionable,
        });
    }
    if lote.is_empty() {
        return Ok(Vec::new());
    }

    let payload = PayloadNotificaciones {
        notificaciones: lote.clone(),
    };
    let mut ev = Evento::nuevo(
        "crear_notificaciones",
        tarea_id.as_deref(),
        &normalizar_email(emisor),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar crear_notificaciones: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_lote(tx, &lote))?;
    Ok(lote)
}

fn aplicar_lote(c: &Connection, lote: &[Notificacion]) -> Resultado<()> {
    for n in lote {
        emitir_si_falta(c, n)?;
    }
    Ok(())
}

/// Reproduce un evento de notificación recibido de `ops/`. Exige que el emisor
/// esté registrado y verificado en `usuarios`.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    if rol_de(c, &evento.usuario)?.is_none() {
        return Err(Error::SinPermiso);
    }
    match evento.tipo.as_str() {
        "crear_notificacion" => {
            let noti: Notificacion = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_notificacion inválido: {e}")))?;
            emitir_si_falta(c, &noti)
        }
        "crear_notificaciones" => {
            let payload: PayloadNotificaciones = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_notificaciones inválido: {e}")))?;
            aplicar_lote(c, &payload.notificaciones)
        }
        otro => Err(Error::Validacion(format!(
            "tipo de evento de notificación no soportado: {otro}"
        ))),
    }
}

/// Lista las notificaciones de `email` (más reciente primero, local #116).
pub fn listar_de_usuario(c: &Connection, email: &str) -> Resultado<Vec<Notificacion>> {
    db_notis::de_usuario(c, &normalizar_email(email))
}

/// Cantidad de notificaciones sin leer de `email`.
pub fn contar_sin_leer(c: &Connection, email: &str) -> Resultado<u32> {
    db_notis::sin_leer(c, &normalizar_email(email))
}

/// Marca una notificación como leída (`true`) o no leída (`false`), verificando que
/// pertenezca al usuario `email` (#116).
pub fn marcar_leida(c: &Connection, email: &str, id: &str, leida: bool) -> Resultado<()> {
    let id = id.trim();
    let noti = db_notis::buscar(c, id)?
        .ok_or_else(|| Error::NoEncontrado(format!("la notificación {id}")))?;
    if normalizar_email(&noti.para) != normalizar_email(email) {
        return Err(Error::SinPermiso);
    }
    db_notis::marcar_leida(c, id, leida)
}

/// Marca todas las notificaciones pendientes de `email` como leídas (#116).
pub fn marcar_todas_leidas(c: &Connection, email: &str) -> Resultado<usize> {
    db_notis::marcar_todas_leidas(c, &normalizar_email(email))
}

fn parse_fecha_dia(fecha: &str) -> Option<NaiveDate> {
    let s = fecha.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt.date());
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Local).date_naive());
    }
    let prefijo = s.get(..10)?;
    NaiveDate::parse_from_str(prefijo, "%Y-%m-%d").ok()
}

fn id_notificacion_inactividad(
    tarea_id: &str,
    dueno: &str,
    ultima_actividad: NaiveDate,
    ciclo: i64,
) -> String {
    let slug: String = dueno
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    format!(
        "inact_{tarea_id}_{slug}_{}_{ciclo}",
        ultima_actividad.format("%Y%m%d")
    )
}

/// Revisa tareas activas con rama asociada y responsable que lleven al menos
/// [`UMBRAL_INACTIVIDAD_DIAS`] días sin changesets nuevos ni cambios de activación (#117).
/// Emite una notificación `inactividad` al dueño por `ops/` deduplicada por ciclo.
pub fn evaluar_inactividad(
    c: &Connection,
    emisor: &str,
    filtro_tarea: Option<&str>,
    ahora: &str,
    hoy: &str,
) -> Resultado<Vec<Notificacion>> {
    let hoy_dia = parse_fecha_dia(hoy).ok_or_else(|| {
        Error::Validacion(format!("Fecha inválida para evaluar inactividad: {hoy}"))
    })?;
    let filtro = filtro_tarea.map(str::trim).filter(|s| !s.is_empty());
    let todas = db_tareas::listar(c)?;

    let mut padres_con_subtareas = BTreeSet::new();
    for t in &todas {
        if !t.eliminada && t.tipo != TipoTarea::SinDefinir {
            if let Some(ref pid) = t.tarea_padre_id {
                padres_con_subtareas.insert(pid.clone());
            }
        }
    }

    let mut emitidas = Vec::new();
    for t in &todas {
        if t.eliminada
            || t.tipo == TipoTarea::SinDefinir
            || padres_con_subtareas.contains(&t.id)
            || !matches!(t.estado, EstadoTarea::PorHacer | EstadoTarea::EnProduccion)
        {
            continue;
        }
        if let Some(id_buscado) = filtro {
            if t.id != id_buscado {
                continue;
            }
        }
        if t.rama_asociada
            .as_deref()
            .map_or(true, |r| r.trim().is_empty())
        {
            continue;
        }
        let Some(dueno) = t
            .asignado_a
            .as_deref()
            .map(normalizar_email)
            .filter(|e| !e.is_empty())
        else {
            continue;
        };
        if usuarios::buscar(c, &dueno)?.is_none() {
            continue;
        }

        let commit_max = commits::de_tarea(c, &t.id)?
            .into_iter()
            .filter_map(|cm| parse_fecha_dia(&cm.fecha))
            .max();
        let inicio_max = t.actividad_desde.as_deref().and_then(parse_fecha_dia);
        let ultima_actividad = match (commit_max, inicio_max) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
        .or_else(|| parse_fecha_dia(&t.fecha_creacion));

        let Some(ref_dia) = ultima_actividad else {
            continue;
        };
        let dias = (hoy_dia - ref_dia).num_days();
        if dias < UMBRAL_INACTIVIDAD_DIAS {
            continue;
        }
        let ciclo = dias / UMBRAL_INACTIVIDAD_DIAS;
        let id = id_notificacion_inactividad(&t.id, &dueno, ref_dia, ciclo);
        if db_notis::buscar(c, &id)?.is_some() {
            continue;
        }

        let accion = if t.estado == EstadoTarea::PorHacer {
            "empezar a trabajarla y subir tu primer check-in"
        } else {
            "subir un check-in o actualizar tu avance"
        };
        let cuerpo = format!(
            "Llevas {dias} días sin actividad en \"{}\" ({}), recuerda {accion}.",
            t.titulo, t.id
        );
        let noti = crear_con_id(
            c,
            emisor,
            &id,
            &DatosNotificacion {
                tipo: TipoNotificacion::Inactividad,
                para: &dueno,
                titulo: "Recordatorio de inactividad",
                cuerpo: &cuerpo,
                tarea_id: Some(&t.id),
                accionable: false,
            },
            ahora,
        )?;
        emitidas.push(noti);
    }
    Ok(emitidas)
}

fn nombre_visible_usuario(c: &Connection, email: &str) -> Resultado<String> {
    let limpio = normalizar_email(email);
    let usuario = usuarios::buscar(c, &limpio)?
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {limpio}")))?;
    let completo = format!("{} {}", usuario.nombre.trim(), usuario.apellido.trim())
        .trim()
        .to_string();
    Ok(if completo.is_empty() {
        limpio
    } else {
        completo
    })
}

/// Envía un mensaje manual de `emisor` hacia `para` (máx. [`MAX_CARACTERES_MENSAJE`]) vía `ops/` (#118).
pub fn enviar_mensaje(
    c: &Connection,
    emisor: &str,
    para: &str,
    texto: &str,
    ahora: &str,
) -> Resultado<Notificacion> {
    let emisor_norm = normalizar_email(emisor);
    let dest = normalizar_email(para);
    if dest.is_empty() || usuarios::buscar(c, &dest)?.is_none() {
        return Err(Error::NoEncontrado(format!("el usuario {dest}")));
    }
    if dest == emisor_norm {
        return Err(Error::Validacion(
            "No puedes enviarte un mensaje a ti mismo".into(),
        ));
    }
    let texto_limpio = texto.trim();
    if texto_limpio.is_empty() {
        return Err(Error::Validacion(
            "Escribe un mensaje antes de enviarlo".into(),
        ));
    }
    if texto_limpio.chars().count() > MAX_CARACTERES_MENSAJE {
        return Err(Error::Validacion(format!(
            "El mensaje no puede superar los {MAX_CARACTERES_MENSAJE} caracteres"
        )));
    }
    let nombre = nombre_visible_usuario(c, &emisor_norm)?;
    let titulo = format!("Mensaje de {nombre}");
    crear(
        c,
        &emisor_norm,
        &DatosNotificacion {
            tipo: TipoNotificacion::Mensaje,
            para: &dest,
            titulo: &titulo,
            cuerpo: texto_limpio,
            tarea_id: None,
            accionable: false,
        },
        ahora,
    )
}

/// Envía a todos los Administradores un reporte de problema con la base de datos (`accion_bd`, #118).
/// Solo Líder o Desarrollador pueden emitirlo.
pub fn solicitar_accion_bd(
    c: &Connection,
    emisor: &str,
    rol_emisor: Rol,
    motivo: &str,
    ahora: &str,
) -> Resultado<Vec<Notificacion>> {
    if !matches!(rol_emisor, Rol::Lider | Rol::Desarrollador) {
        return Err(Error::SinPermiso);
    }
    let motivo_limpio = motivo.trim();
    if motivo_limpio.is_empty() {
        return Err(Error::Validacion(
            "Describe el problema con la base de datos".into(),
        ));
    }
    let emisor_norm = normalizar_email(emisor);
    let nombre = nombre_visible_usuario(c, &emisor_norm)?;
    let cuerpo = format!(
        "{nombre} ({}) solicita revisar la base de datos: {motivo_limpio}",
        rol_emisor.como_texto()
    );
    let emitidas = crear_para_roles(
        c,
        &emisor_norm,
        &DatosNotificacionRoles {
            tipo: TipoNotificacion::AccionBd,
            roles: &[Rol::Administrador],
            titulo: "Solicitud de acción sobre la base de datos",
            cuerpo: &cuerpo,
            tarea_id: None,
            accionable: false,
        },
        ahora,
    )?;
    if emitidas.is_empty() {
        return Err(Error::NoEncontrado(
            "No hay Administradores registrados para recibir la solicitud".into(),
        ));
    }
    Ok(emitidas)
}

#[cfg(test)]
mod tests {

    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use crate::db::{usuarios, Db};

    use crate::modelo::{Rol, TipoTarea};
    use crate::tareas::{
        asignar, cambiar_estado, crear as crear_tarea, crear_con_subtareas, eliminar, reabrir,
        AutorTarea, DatosNuevaTarea, DatosNuevaTareaConSubtareas, DatosSubtareaNueva,
    };

    const ADMIN: &str = "admin@estudio.com";
    const LIDER: &str = "lider@estudio.com";
    const DEV: &str = "dev@estudio.com";
    const INTRUSO: &str = "intruso@fuera.com";
    const AHORA: &str = "2026-11-16T10:00:00.000Z";
    const HOY: &str = "2026-11-16";

    struct Entorno {
        _tmp: TempDir,
        db_a: Db,
        db_b: Db,
    }

    fn setup() -> Entorno {
        let tmp = TempDir::new().unwrap();
        let compartida = tmp.path().join("compartida");
        std::fs::create_dir_all(&compartida).unwrap();
        let local_a = tmp
            .path()
            .join("datos_a")
            .join("proyectos")
            .join("estudio-notis");
        let local_b = tmp
            .path()
            .join("datos_b")
            .join("proyectos")
            .join("estudio-notis");

        let db_a = Db::abrir(&local_a).unwrap();
        let db_b = Db::abrir(&local_b).unwrap();

        for (correo, rol) in [
            (ADMIN, Rol::Administrador),
            (LIDER, Rol::Lider),
            (DEV, Rol::Desarrollador),
        ] {
            let fila = usuarios::Usuario {
                email: correo.into(),
                nombre: correo.split('@').next().unwrap().into(),
                apellido: "Test".into(),
                rol,
                fecha_alta: HOY.into(),
            };
            usuarios::insertar(db_a.conexion(), &fila).unwrap();
            usuarios::insertar(db_b.conexion(), &fila).unwrap();
        }

        Entorno {
            _tmp: tmp,
            db_a,
            db_b,
        }
    }

    #[test]
    fn crear_y_crear_para_roles_viajan_por_ops_y_descartan_no_registrado() {
        let e = setup();
        let n1 = crear(
            e.db_a.conexion(),
            DEV,
            &DatosNotificacion {
                tipo: TipoNotificacion::Mensaje,
                para: LIDER,
                titulo: "Hola líder",
                cuerpo: "¿Revisamos hoy?",
                tarea_id: None,
                accionable: false,
            },
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        let lote = crear_para_roles(
            e.db_a.conexion(),
            DEV,
            &DatosNotificacionRoles {
                tipo: TipoNotificacion::AccionBd,
                roles: &[Rol::Administrador],
                titulo: "Revisar BD",
                cuerpo: "Algo raro en local",
                tarea_id: None,
                accionable: false,
            },
            "2026-11-16T10:01:01.000Z",
        )
        .unwrap();
        assert_eq!(lote.len(), 1);
        assert_eq!(lote[0].para, ADMIN);

        // Un evento forjado de alguien fuera del equipo se descarta al sincronizar.
        let mut falso = Evento::nuevo(
            "crear_notificacion",
            None,
            INTRUSO,
            serde_json::to_value(&Notificacion {
                id: "noti_falsa".into(),
                tipo: TipoNotificacion::Mensaje,
                para: ADMIN.into(),
                titulo: "Spam".into(),
                cuerpo: "No debería entrar".into(),
                leida: false,
                fecha: "2026-11-16T10:01:02.000Z".into(),
                tarea_id: None,
                accionable: false,
            })
            .unwrap(),
        );
        falso.timestamp = "2026-11-16T10:01:02.000Z".into();
        crate::eventos::servidor_de_prueba::agregar(&falso);

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b =
            crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();
        assert_eq!(sync_b.aplicados, 2);
        assert_eq!(sync_b.descartados, 1);

        let de_lider_b = listar_de_usuario(e.db_b.conexion(), LIDER).unwrap();
        assert_eq!(de_lider_b, vec![n1.clone()]);
        assert_eq!(listar_de_usuario(e.db_b.conexion(), ADMIN).unwrap(), lote);

        // El estado `leida` es estrictamente local y valida dueño.
        assert!(matches!(
            marcar_leida(e.db_a.conexion(), ADMIN, &n1.id, true),
            Err(Error::SinPermiso)
        ));
        marcar_leida(e.db_a.conexion(), LIDER, &n1.id, true).unwrap();
        assert_eq!(contar_sin_leer(e.db_a.conexion(), LIDER).unwrap(), 0);
        assert_eq!(contar_sin_leer(e.db_b.conexion(), LIDER).unwrap(), 1);
        marcar_leida(e.db_a.conexion(), LIDER, &n1.id, false).unwrap();
        assert_eq!(contar_sin_leer(e.db_a.conexion(), LIDER).unwrap(), 1);
        assert_eq!(marcar_todas_leidas(e.db_a.conexion(), LIDER).unwrap(), 1);
        assert_eq!(contar_sin_leer(e.db_a.conexion(), LIDER).unwrap(), 0);
    }

    #[test]
    fn conecta_aprobacion_asignada_revision_reabierta_y_eliminada() {
        let e = setup();
        let dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };

        // 1. Tarea con subtareas creada por Desarrollador -> 1 sola notificación `aprobacion_tarea` para Líder/Admin.
        let creadas = crear_con_subtareas(
            e.db_a.conexion(),
            &dev,
            &DatosNuevaTareaConSubtareas {
                titulo: "Sistema de combate".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![DatosSubtareaNueva {
                    tipo: TipoTarea::Scripting,
                    titulo: "Hitboxes".into(),
                    detalles: String::new(),
                }],
            },
            "2026-11-16T10:01:00.000Z",
            HOY,
        )
        .unwrap();
        assert!(creadas[0].pendiente_aprobacion);

        let apr_lider: Vec<_> = listar_de_usuario(e.db_a.conexion(), LIDER)
            .unwrap()
            .into_iter()
            .filter(|n| n.tipo == TipoNotificacion::AprobacionTarea)
            .collect();
        assert_eq!(apr_lider.len(), 1);
        assert!(apr_lider[0].accionable);
        assert_eq!(apr_lider[0].tarea_id.as_deref(), Some("T-101"));

        // 2. Tarea creada por Líder -> NO emite `aprobacion_tarea`; asignar -> revisión -> terminada -> reabrir -> eliminar.
        let t_ok = crear_tarea(
            e.db_a.conexion(),
            &lider,
            &DatosNuevaTarea {
                titulo: "Hud".into(),
                detalles: String::new(),
                tipo: TipoTarea::UiInterfaz,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
            },
            "2026-11-16T10:01:01.000Z",
            HOY,
        )
        .unwrap();
        asignar(
            e.db_a.conexion(),
            &lider,
            &t_ok.id,
            DEV,
            |_| {
                Ok(vec![(
                    t_ok.id.clone(),
                    crate::servidor::Rama {
                        id: 1,
                        name: "/main/dev/T-102-hud".into(),
                        head_changeset: 1,
                    },
                )])
            },
            "2026-11-16T10:01:02.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &dev,
            &t_ok.id,
            crate::modelo::EstadoTarea::EnRevision,
            "2026-11-16T10:01:03.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &lider,
            &t_ok.id,
            crate::modelo::EstadoTarea::Terminada,
            "2026-11-16T10:01:04.000Z",
        )
        .unwrap();
        reabrir(
            e.db_a.conexion(),
            &lider,
            &t_ok.id,
            "Falta contraste",
            "2026-11-16T10:01:05.000Z",
        )
        .unwrap();
        eliminar(
            e.db_a.conexion(),
            &lider,
            &creadas[0].id,
            "Duplicada",
            "2026-11-16T10:01:06.000Z",
        )
        .unwrap();

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();

        let tipos_lider: Vec<_> = listar_de_usuario(e.db_b.conexion(), LIDER)
            .unwrap()
            .into_iter()
            .map(|n| n.tipo)
            .collect();
        assert!(tipos_lider.contains(&TipoNotificacion::AprobacionTarea));
        assert!(tipos_lider.contains(&TipoNotificacion::TareaARevision));
        assert!(tipos_lider.contains(&TipoNotificacion::TareaReabierta));

        let tipos_dev: Vec<_> = listar_de_usuario(e.db_b.conexion(), DEV)
            .unwrap()
            .into_iter()
            .map(|n| n.tipo)
            .collect();
        assert!(tipos_dev.contains(&TipoNotificacion::TareaAsignada));
        assert!(tipos_dev.contains(&TipoNotificacion::TareaEliminada));
    }

    #[test]
    fn recordatorio_inactividad_por_umbral_ciclos_y_reapertura() {
        let e = setup();
        let admin = AutorTarea {
            email: ADMIN,
            rol: Rol::Administrador,
        };
        let dev_autor = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };

        // 1. Tarea general creada el 1 nov, pero tomada recién el 10 nov:
        //    el reloj debe arrancar desde la toma (`actividad_desde = 2026-11-10`).
        let mut t = crear_tarea(
            e.db_a.conexion(),
            &admin,
            &DatosNuevaTarea {
                titulo: "Salto doble".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
            },
            "2026-11-01T10:00:00Z",
            "2026-11-01",
        )
        .unwrap();
        crate::tareas::tomar(
            e.db_a.conexion(),
            &dev_autor,
            &t.id,
            |cadena| {
                Ok(vec![(
                    cadena[0].id.clone(),
                    crate::servidor::Rama {
                        id: 1,
                        name: "/main/dev/T-101-salto-doble".into(),
                        head_changeset: 1,
                    },
                )])
            },
            "2026-11-10T09:00:00Z",
        )
        .unwrap();

        t = cambiar_estado(
            e.db_a.conexion(),
            &dev_autor,
            &t.id,
            EstadoTarea::EnProduccion,
            "2026-11-10T09:05:00Z",
        )
        .unwrap()
        .tareas
        .into_iter()
        .find(|x| x.id == t.id)
        .unwrap();
        assert!(t.actividad_desde.is_some());

        // Padre con subtarea activa (el padre se excluye; solo la subtarea con rama se vigila).
        let grupo = crear_con_subtareas(
            e.db_a.conexion(),
            &admin,
            &DatosNuevaTareaConSubtareas {
                titulo: "Padre con subs".into(),
                detalles: String::new(),
                tipo: TipoTarea::Audio,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![DatosSubtareaNueva {
                    tipo: TipoTarea::Audio,
                    titulo: "Sub activa".into(),
                    detalles: String::new(),
                }],
            },
            "2026-11-10T10:00:00Z",
            "2026-11-10",
        )
        .unwrap();
        let sub_id = grupo[1].id.clone();
        crate::tareas::tomar(
            e.db_a.conexion(),
            &dev_autor,
            &sub_id,
            |cadena| {
                Ok(cadena
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        (
                            p.id.clone(),
                            crate::servidor::Rama {
                                id: 10 + i as i64,
                                name: format!("/main/dev/{}", p.id),
                                head_changeset: 1,
                            },
                        )
                    })
                    .collect())
            },
            "2026-11-15T10:00:00Z",
        )
        .unwrap();

        // Subtarea En espera tampoco debe alertar.
        let mut t_espera = db_tareas::Tarea::nueva(
            "T-999",
            "En espera 1",
            TipoTarea::SinDefinir,
            DEV,
            "2026-11-01",
        );
        t_espera.asignado_a = Some(DEV.into());
        t_espera.rama_asociada = Some("/main/dev/T-999-espera".into());
        t_espera.estado = EstadoTarea::EnProduccion;
        db_tareas::insertar(e.db_a.conexion(), &t_espera).unwrap();

        // Día 12 (2 días): < 3 días -> 0 notificaciones (tampoco dispara por fecha_creacion 2026-11-01).
        assert!(evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            None,
            "2026-11-12T10:00:00Z",
            "2026-11-12",
        )
        .unwrap()
        .is_empty());

        // Día 13 (3 días): emite 1 recordatorio para `t.id` al Desarrollador dueño vía `ops/`.
        let dia_3 = evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            None,
            "2026-11-13T10:00:00Z",
            "2026-11-13",
        )
        .unwrap();
        assert_eq!(dia_3.len(), 1);
        assert_eq!(dia_3[0].tipo, TipoNotificacion::Inactividad);
        assert_eq!(dia_3[0].para, DEV);
        assert_eq!(dia_3[0].tarea_id.as_deref(), Some(t.id.as_str()));
        assert!(!dia_3[0].accionable);
        assert!(dia_3[0].cuerpo.contains("Llevas 3 días sin actividad"));

        // Misma ventana (día 13 y día 14): NO duplica el recordatorio.
        assert!(evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            None,
            "2026-11-13T12:00:00Z",
            "2026-11-13",
        )
        .unwrap()
        .is_empty());
        assert!(evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            None,
            "2026-11-14T10:00:00Z",
            "2026-11-14",
        )
        .unwrap()
        .is_empty());

        // Día 16 (6 días = ciclo 2): vuelve a recordar para `t.id`.
        let dia_6 = evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            Some(&t.id),
            "2026-11-16T10:00:00Z",
            "2026-11-16",
        )
        .unwrap();
        assert_eq!(dia_6.len(), 1);
        assert!(dia_6[0].cuerpo.contains("Llevas 6 días sin actividad"));

        // Un check-in el día 16 reinicia el reloj: el día 18 (2 días después) -> 0 avisos.
        crate::db::commits::guardar(
            e.db_a.conexion(),
            &crate::db::commits::Commit {
                id: format!("C-{}-10", t.id),
                tarea_id: t.id.clone(),
                hash: "10".into(),
                mensaje: "Primer avance".into(),
                descripcion: None,
                tipo: Some(TipoTarea::Scripting),
                autor: DEV.into(),
                fecha: "2026-11-16T15:00:00".into(),
            },
        )
        .unwrap();
        assert!(evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            Some(&t.id),
            "2026-11-18T10:00:00Z",
            "2026-11-18",
        )
        .unwrap()
        .is_empty());

        // Subtarea en revisión + padre con subtareas nunca disparan recordatorio de inactividad.
        cambiar_estado(
            e.db_a.conexion(),
            &admin,
            &sub_id,
            EstadoTarea::EnRevision,
            "2026-11-18T11:00:00Z",
        )
        .unwrap();
        assert!(evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            Some(&grupo[0].id),
            "2026-11-25T10:00:00Z",
            "2026-11-25",
        )
        .unwrap()
        .is_empty());
        assert!(evaluar_inactividad(
            e.db_a.conexion(),
            ADMIN,
            Some(&sub_id),
            "2026-11-25T10:00:00Z",
            "2026-11-25",
        )
        .unwrap()
        .is_empty());

        // B sincroniza por ops/ y recibe ambos avisos de inactividad (ciclo 1 y ciclo 2).
        let ahora = DateTime::parse_from_rfc3339("2026-11-30T00:00:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();
        let inactivas_b: Vec<_> = listar_de_usuario(e.db_b.conexion(), DEV)
            .unwrap()
            .into_iter()
            .filter(|n| n.tipo == TipoNotificacion::Inactividad)
            .collect();
        assert_eq!(inactivas_b.len(), 2);
    }

    #[test]
    fn enviar_mensaje_y_solicitar_accion_bd_reglas_y_viajan_por_ops() {
        let e = setup();
        // Agrega un Tester para probar restricción de `solicitar_accion_bd`.
        const TESTER: &str = "tester@estudio.com";
        let tester_fila = usuarios::Usuario {
            email: TESTER.into(),
            nombre: "Tess".into(),
            apellido: "Uno".into(),
            rol: Rol::Tester,
            fecha_alta: HOY.into(),
        };
        usuarios::insertar(e.db_a.conexion(), &tester_fila).unwrap();
        usuarios::insertar(e.db_b.conexion(), &tester_fila).unwrap();

        // Validaciones de `enviar_mensaje`: vacío, >500 caracteres Unicode, autoenvío y usuario inexistente.
        assert!(enviar_mensaje(e.db_a.conexion(), DEV, LIDER, "   ", AHORA).is_err());
        let largo_501 = "á".repeat(501);
        assert!(enviar_mensaje(e.db_a.conexion(), DEV, LIDER, &largo_501, AHORA).is_err());
        assert!(enviar_mensaje(e.db_a.conexion(), DEV, DEV, "hola", AHORA).is_err());
        assert!(enviar_mensaje(e.db_a.conexion(), DEV, INTRUSO, "hola", AHORA).is_err());

        let exacto_500 = "ñ".repeat(500);
        let msg = enviar_mensaje(
            e.db_a.conexion(),
            DEV,
            "LIDER@estudio.com",
            &format!("  {exacto_500}  "),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        assert_eq!(msg.tipo, TipoNotificacion::Mensaje);
        assert_eq!(msg.para, LIDER);
        assert!(msg.titulo.starts_with("Mensaje de "));
        assert_eq!(msg.cuerpo, exacto_500);
        assert!(!msg.accionable);

        // `solicitar_accion_bd`: vacío rechazado, Tester y Admin rechazados (`sin_permiso`).
        assert!(
            solicitar_accion_bd(e.db_a.conexion(), DEV, Rol::Desarrollador, "   ", AHORA,).is_err()
        );
        assert_eq!(
            solicitar_accion_bd(e.db_a.conexion(), TESTER, Rol::Tester, "Tabla rota", AHORA,)
                .unwrap_err()
                .codigo(),
            "sin_permiso"
        );
        assert_eq!(
            solicitar_accion_bd(
                e.db_a.conexion(),
                ADMIN,
                Rol::Administrador,
                "Soy admin",
                AHORA,
            )
            .unwrap_err()
            .codigo(),
            "sin_permiso"
        );

        let req_dev = solicitar_accion_bd(
            e.db_a.conexion(),
            DEV,
            Rol::Desarrollador,
            "Registros duplicados en T-104",
            "2026-11-16T10:02:00.000Z",
        )
        .unwrap();
        assert_eq!(req_dev.len(), 1);
        assert_eq!(req_dev[0].para, ADMIN);
        assert_eq!(req_dev[0].tipo, TipoNotificacion::AccionBd);

        let req_lider = solicitar_accion_bd(
            e.db_a.conexion(),
            LIDER,
            Rol::Lider,
            "Diferencia tras rollback",
            "2026-11-16T10:03:00.000Z",
        )
        .unwrap();
        assert_eq!(req_lider.len(), 1);

        // B sincroniza vía ops/ y comprueba destinatarios exactos.
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:10:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora).unwrap();

        let notis_admin_b = listar_de_usuario(e.db_b.conexion(), ADMIN).unwrap();
        assert_eq!(
            notis_admin_b
                .iter()
                .filter(|n| n.tipo == TipoNotificacion::AccionBd)
                .count(),
            2
        );
        let notis_lider_b = listar_de_usuario(e.db_b.conexion(), LIDER).unwrap();
        assert!(notis_lider_b
            .iter()
            .any(|n| n.tipo == TipoNotificacion::Mensaje && n.cuerpo == exacto_500));
        assert!(!notis_lider_b
            .iter()
            .any(|n| n.tipo == TipoNotificacion::AccionBd));
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #116): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// prueba:
    /// 1. La función común `notificaciones::crear` / `crear_para_roles` viajando
    ///    por `ops/` en OneDrive (y el descarte de eventos de un usuario externo).
    /// 2. Notificación `aprobacion_tarea` (`accionable = true`) al crear un
    ///    Desarrollador una tarea pendiente, más los tipos ya conectados en el flujo
    ///    real (`tarea_asignada`, `tarea_a_revision`, `tarea_reabierta`,
    ///    `tarea_eliminada`).
    /// 3. Sincronización hacia máquina B por OneDrive y comandos locales Tauri
    ///    (`obtener_notificaciones_en`, `marcar_leida_en`, `desmarcar_leida_en`,
    ///    `marcar_todas_leidas_en`) verificando que `leida` queda estrictamente local.
    ///
    /// Al terminar borra el workspace y la subcarpeta de OneDrive.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_notificaciones() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::notificaciones::{
            desmarcar_leida_en, marcar_leida_en, marcar_todas_leidas_en, obtener_notificaciones_en,
        };
        use crate::comandos::tareas::{cambiar_estado_en, eliminar_tarea_en, reabrir_en};
        use crate::estado::AppState;

        use crate::modelo::hoy_local;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube_tmp = tempfile::tempdir().unwrap();
        let nube = nube_tmp.path().to_path_buf();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-116-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-116-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let hoy_real = hoy_local();
            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));
            let ahora_pausa = || {
                pausa();
                crate::modelo::ahora_iso()
            };

            let dev_autor = AutorTarea {
                email: DEV,
                rol: Rol::Desarrollador,
            };

            let t_flujo = estado_a
                .con_db(|db_a| {
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            DEV,
                            Rol::Desarrollador,
                            &hoy_real,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })?;
                    // 1. Desarrollador crea tarea -> emite `aprobacion_tarea` para Admin.
                    crear_tarea(
                        db_a.conexion(),
                        &dev_autor,
                        &DatosNuevaTarea {
                            titulo: "Pendiente para campana".into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        &ahora_pausa(),
                        &hoy_real,
                    )?;
                    // 2. Función común vía ops/ hacia Admin.
                    crear_para_roles(
                        db_a.conexion(),
                        DEV,
                        &DatosNotificacionRoles {
                            tipo: TipoNotificacion::AccionBd,
                            roles: &[Rol::Administrador],
                            titulo: "Aviso BD de prueba",
                            cuerpo: "Sincronizar revisión",
                            tarea_id: None,
                            accionable: false,
                        },
                        &ahora_pausa(),
                    )?;
                    let admin_autor = AutorTarea {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                    };
                    let t_flujo = crear_tarea(
                        db_a.conexion(),
                        &admin_autor,
                        &DatosNuevaTarea {
                            titulo: "Flujo campana real".into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Audio,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        &ahora_pausa(),
                        &hoy_real,
                    )?;
                    Ok(t_flujo)
                })
                .unwrap();

            // 3. Flujo con comandos reales de Admin: revisión -> terminada -> reabrir -> eliminar la pendiente.
            pausa();
            cambiar_estado_en(
                &estado_a,
                &t_flujo.id,
                crate::modelo::EstadoTarea::EnRevision,
            )
            .unwrap();
            pausa();
            cambiar_estado_en(
                &estado_a,
                &t_flujo.id,
                crate::modelo::EstadoTarea::Terminada,
            )
            .unwrap();
            pausa();
            reabrir_en(&estado_a, &t_flujo.id, "Probar notificación").unwrap();
            pausa();
            eliminar_tarea_en(&estado_a, "T-101", "Rechazo de prueba").unwrap();
            pausa();

            // 4. Comandos locales en máquina A (`obtener_notificaciones_en`, `marcar_leida_en`, `desmarcar_leida_en`, `marcar_todas_leidas_en`).
            let notis_admin_a = obtener_notificaciones_en(&estado_a).unwrap();
            assert!(notis_admin_a
                .iter()
                .any(|n| n.tipo == TipoNotificacion::AprobacionTarea && n.accionable));
            assert!(notis_admin_a
                .iter()
                .any(|n| n.tipo == TipoNotificacion::AccionBd));
            assert!(notis_admin_a
                .iter()
                .any(|n| n.tipo == TipoNotificacion::TareaARevision));
            assert!(notis_admin_a
                .iter()
                .any(|n| n.tipo == TipoNotificacion::TareaReabierta));
            assert!(notis_admin_a.iter().all(|n| !n.leida));

            let primera_id = notis_admin_a[0].id.clone();
            marcar_leida_en(&estado_a, &primera_id).unwrap();
            assert!(
                obtener_notificaciones_en(&estado_a)
                    .unwrap()
                    .into_iter()
                    .find(|n| n.id == primera_id)
                    .unwrap()
                    .leida
            );
            desmarcar_leida_en(&estado_a, &primera_id).unwrap();
            assert!(
                !obtener_notificaciones_en(&estado_a)
                    .unwrap()
                    .into_iter()
                    .find(|n| n.id == primera_id)
                    .unwrap()
                    .leida
            );
            let marcadas = marcar_todas_leidas_en(&estado_a).unwrap();
            assert_eq!(marcadas, notis_admin_a.len());
            assert!(obtener_notificaciones_en(&estado_a)
                .unwrap()
                .iter()
                .all(|n| n.leida));

            // Inyectamos notificación forjada de externo en OneDrive para verificar descarte.
            let mut ev_falso = Evento::nuevo(
                "crear_notificacion",
                None,
                INTRUSO,
                serde_json::to_value(&Notificacion {
                    id: "noti_intruso_real".into(),
                    tipo: TipoNotificacion::Mensaje,
                    para: admin_fila.email.clone(),
                    titulo: "Falsa".into(),
                    cuerpo: "Spam".into(),
                    leida: false,
                    fecha: ahora_pausa(),
                    tarea_id: None,
                    accionable: false,
                })
                .unwrap(),
            );
            ev_falso.timestamp = crate::modelo::ahora_iso();
            crate::eventos::servidor_de_prueba::agregar(&ev_falso);

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 5. Máquina B sincroniza vía OneDrive: recibe las mismas notificaciones, descarta la falsa
            //    y conserva `leida = false` localmente (no se filtra lo marcado en A).
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b =
                crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora).unwrap();
            println!("B sincroniza notificaciones por el servidor de prueba: {sync_b:?}");
            assert_eq!(sync_b.descartados, 1);

            let notis_b = listar_de_usuario(db_b.conexion(), &admin_fila.email).unwrap();
            assert_eq!(notis_b.len(), notis_admin_a.len());
            assert!(notis_b.iter().all(|n| !n.leida));
            assert!(listar_de_usuario(db_b.conexion(), DEV)
                .unwrap()
                .iter()
                .any(|n| n.tipo == TipoNotificacion::TareaEliminada));
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #117): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// prueba el flujo completo de recordatorio de inactividad:
    /// 1. Crea y asigna/toma tareas con ramas reales en Unity VC (`BranchBoard-Pruebas`).
    /// 2. Ejecuta `revisar_actualizaciones_en` (la misma función del temporizador en
    ///    segundo plano de `lib.rs`): sin días vencidos no notifica; con >= 3 días sin
    ///    changesets emite `inactividad` por `ops/` hacia OneDrive (`ResumenPolling`),
    ///    sin duplicar en reevaluaciones posteriores y replicando en máquina B.
    /// 3. Hace un check-in real (`hacer_checkin_en`), vuelve a sincronizar con
    ///    `revisar_actualizaciones_en` desde Unity VC REST API y verifica que el
    ///    changeset real reinicia el contador de inactividad.
    ///
    /// Al terminar borra workspace, ramas con `--delete-changesets` y carpeta en OneDrive.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_recordatorio_inactividad() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::{asignar_en, hacer_checkin_en, tomar_en};
        use crate::estado::AppState;

        use crate::modelo::hoy_local;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube_tmp = tempfile::tempdir().unwrap();
        let nube = nube_tmp.path().to_path_buf();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-117-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();
        let repo_cm = format!("{repo}@{org}@unity");
        let mut ramas_creadas: Vec<String> = Vec::new();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-117-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo_cm}"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let hoy_real = hoy_local();
            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));
            let ahora_pausa = || {
                pausa();
                crate::modelo::ahora_iso()
            };

            let (t_inact, t_propia) = estado_a
                .con_db(|db_a| {
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            DEV,
                            Rol::Desarrollador,
                            &hoy_real,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })?;
                    let admin_autor = AutorTarea {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                    };
                    let t1 = crear_tarea(
                        db_a.conexion(),
                        &admin_autor,
                        &DatosNuevaTarea {
                            titulo: format!("Inactiva 117 {sufijo}"),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        &ahora_pausa(),
                        &hoy_real,
                    )?;
                    let t2 = crear_tarea(
                        db_a.conexion(),
                        &admin_autor,
                        &DatosNuevaTarea {
                            titulo: format!("Con checkin 117 {sufijo}"),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        &ahora_pausa(),
                        &hoy_real,
                    )?;
                    Ok((t1, t2))
                })
                .unwrap();

            // 1. Asignar t1 a DEV y tomar t2 como Admin -> ambas crean ramas reales en Unity VC.
            pausa();
            let asig_t1 = asignar_en(&estado_a, &t_inact.id, DEV).unwrap();
            let rama_t1 = asig_t1.last().unwrap().rama_asociada.clone().unwrap();
            ramas_creadas.push(rama_t1);

            pausa();
            let tom_t2 = tomar_en(&estado_a, &t_propia.id).unwrap();
            let rama_t2 = tom_t2.last().unwrap().rama_asociada.clone().unwrap();
            ramas_creadas.push(rama_t2.clone());

            // Recién creadas (0 días): el polling real no envía recordatorios.
            let poll0 = revisar_actualizaciones_en(&estado_a, None).unwrap();
            assert!(poll0.inactividad_notificada.is_empty());

            // 2. Simular 4 días sin cambios en t1 y ejecutar `revisar_actualizaciones_en` real.
            let hace_4_dias = (chrono::Local::now().date_naive() - chrono::Duration::days(4))
                .format("%Y-%m-%d")
                .to_string();
            estado_a
                .con_db(|db_a| {
                    let mut fila = db_tareas::obtener(db_a.conexion(), &t_inact.id)?;
                    fila.actividad_desde = Some(format!("{hace_4_dias}T10:00:00Z"));
                    db_tareas::actualizar(db_a.conexion(), &fila)
                })
                .unwrap();

            let poll1 = revisar_actualizaciones_en(&estado_a, Some(&t_inact.id)).unwrap();
            assert_eq!(poll1.inactividad_notificada, vec![t_inact.id.clone()]);
            assert!(poll1.hubo_cambios());

            // Reejecutar enseguida no duplica notificaciones.
            let poll2 = revisar_actualizaciones_en(&estado_a, Some(&t_inact.id)).unwrap();
            assert!(poll2.inactividad_notificada.is_empty());

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 3. Máquina B sincroniza vía OneDrive y recibe `inactividad` para DEV.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b =
                crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora).unwrap();
            println!(
                "B sincroniza recordatorio de inactividad por el servidor de prueba: {sync_b:?}"
            );
            let notis_dev_b: Vec<_> = listar_de_usuario(db_b.conexion(), DEV)
                .unwrap()
                .into_iter()
                .filter(|n| n.tipo == TipoNotificacion::Inactividad)
                .collect();
            assert_eq!(notis_dev_b.len(), 1);
            assert!(notis_dev_b[0]
                .cuerpo
                .contains("Llevas 4 días sin actividad"));

            // 4. Con un check-in real en t2 + polling desde Unity VC, el reloj se reinicia.
            let hace_4_dias_ts = format!("{hace_4_dias}T09:00:00Z");
            estado_a
                .con_db(|db_a| {
                    let mut fila = db_tareas::obtener(db_a.conexion(), &t_propia.id)?;
                    fila.actividad_desde = Some(hace_4_dias_ts);
                    db_tareas::actualizar(db_a.conexion(), &fila)
                })
                .unwrap();

            let archivo_ws = ws_a.path().join(format!("avance_117_{sufijo}.txt"));
            std::fs::write(&archivo_ws, "checkin real 117").unwrap();
            Comando::cm()
                .arg("add")
                .arg(archivo_ws.to_string_lossy().as_ref())
                .en(ws_a.path())
                .exigir_exito(&estado_a)
                .unwrap();
            hacer_checkin_en(
                &estado_a,
                &rama_t2,
                "Avance real 117",
                "",
                TipoTarea::Scripting,
            )
            .unwrap();

            let poll_post_ci = revisar_actualizaciones_en(&estado_a, Some(&t_propia.id)).unwrap();
            assert!(poll_post_ci.inactividad_notificada.is_empty());

            let dentro_2_dias = (chrono::Local::now().date_naive() + chrono::Duration::days(2))
                .format("%Y-%m-%d")
                .to_string();
            let sin_alerta_2_dias = estado_a
                .con_db(|db_a| {
                    evaluar_inactividad(
                        db_a.conexion(),
                        &admin_fila.email,
                        Some(&t_propia.id),
                        &crate::modelo::ahora_iso(),
                        &dentro_2_dias,
                    )
                })
                .unwrap();
            assert!(sin_alerta_2_dias.is_empty());
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        for rama in ramas_creadas.iter().rev() {
            let salida = Comando::cm()
                .args(["branch", "delete", "--delete-changesets"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .ejecutar(&estado_a)
                .unwrap();
            println!("limpieza rama {rama}: {}", salida.exito());
        }
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #118): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// ejerce los comandos Tauri `enviar_mensaje_en` y `solicitar_accion_bd_en`:
    /// 1. Crea Desarrollador, Líder y Tester en el proyecto conectado a `BranchBoard-Pruebas`.
    /// 2. Cambia de usuario en `estado_a` verificando roles con firma criptográfica:
    ///    - `enviar_mensaje_en`: todos los roles pueden enviar (rechaza >500 chars/autoenvío).
    ///    - `solicitar_accion_bd_en`: Tester y Administrador reciben `sin_permiso`,
    ///      mientras Desarrollador y Líder notifican solo a Administradores vía `ops/`.
    /// 3. Máquina B sincroniza desde OneDrive y confirma que cada rol ve solo lo suyo.
    ///
    /// Al terminar borra workspace y carpeta de OneDrive.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_mensajes_y_accion_bd() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::notificaciones::{enviar_mensaje_en, solicitar_accion_bd_en};
        use crate::estado::AppState;

        use crate::modelo::hoy_local;

        const TESTER: &str = "tester118@estudio.com";

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube_tmp = tempfile::tempdir().unwrap();
        let nube = nube_tmp.path().to_path_buf();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-118-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-118-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let hoy_real = hoy_local();
            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));

            estado_a
                .con_db(|db_a| {
                    for (correo, rol) in [
                        (DEV, Rol::Desarrollador),
                        (LIDER, Rol::Lider),
                        (TESTER, Rol::Tester),
                    ] {
                        Ok::<_, crate::error::Error>({
                            let u = crate::identidad::equipo::alta_de_prueba(
                                db_a.conexion(),
                                correo,
                                rol,
                                &hoy_real,
                            );
                            crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                            u
                        })?;
                    }
                    Ok(())
                })
                .unwrap();

            // 1. Tester: puede enviar mensaje (<= 500 chars), rechazado en >500 y en `solicitar_accion_bd_en`.
            estado_a.fijar_usuario(Some(TESTER));
            pausa();
            assert_eq!(
                solicitar_accion_bd_en(&estado_a, "No debería poder Tester")
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );
            assert!(enviar_mensaje_en(&estado_a, DEV, &"x".repeat(501)).is_err());
            let m_tester =
                enviar_mensaje_en(&estado_a, DEV, "Ya subí captura del bug en escena").unwrap();
            assert_eq!(m_tester.tipo, TipoNotificacion::Mensaje);
            assert_eq!(m_tester.para, DEV);

            // 2. Administrador: puede enviar mensajes, pero `solicitar_accion_bd_en` devuelve `sin_permiso`.
            estado_a.fijar_usuario(Some(&admin_fila.email));
            pausa();
            assert_eq!(
                solicitar_accion_bd_en(&estado_a, "Admin no solicita")
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );
            assert!(enviar_mensaje_en(&estado_a, &admin_fila.email, "Autoenvío").is_err());
            enviar_mensaje_en(&estado_a, LIDER, "Revisemos sprint antes de cierre").unwrap();

            // 3. Desarrollador y Líder solicitan acción sobre BD -> notifica solo a Administradores.
            estado_a.fijar_usuario(Some(DEV));
            pausa();
            let req_dev =
                solicitar_accion_bd_en(&estado_a, "La tarea T-102 aparece duplicada en mi copia")
                    .unwrap();
            assert_eq!(req_dev.len(), 1);
            assert_eq!(req_dev[0].para, admin_fila.email);

            estado_a.fijar_usuario(Some(LIDER));
            pausa();
            let req_lider =
                solicitar_accion_bd_en(&estado_a, "Necesitamos restaurar respaldo de ayer")
                    .unwrap();
            assert_eq!(req_lider.len(), 1);

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 4. Máquina B sincroniza vía OneDrive y valida cada destinatario.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b =
                crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora).unwrap();
            println!("B sincroniza mensajes y accion_bd por el servidor de prueba: {sync_b:?}");

            let notis_admin_b = listar_de_usuario(db_b.conexion(), &admin_fila.email).unwrap();
            let bd_b: Vec<_> = notis_admin_b
                .iter()
                .filter(|n| n.tipo == TipoNotificacion::AccionBd)
                .collect();
            assert_eq!(bd_b.len(), 2);
            assert!(bd_b
                .iter()
                .any(|n| n.cuerpo.contains("Desarrollador") && n.cuerpo.contains("duplicada")));
            assert!(bd_b
                .iter()
                .any(|n| n.cuerpo.contains("Líder") && n.cuerpo.contains("respaldo")));

            let notis_dev_b = listar_de_usuario(db_b.conexion(), DEV).unwrap();
            assert!(notis_dev_b
                .iter()
                .any(|n| n.tipo == TipoNotificacion::Mensaje && n.cuerpo.contains("captura")));
            assert!(notis_dev_b
                .iter()
                .all(|n| n.tipo != TipoNotificacion::AccionBd));

            let notis_lider_b = listar_de_usuario(db_b.conexion(), LIDER).unwrap();
            assert!(notis_lider_b
                .iter()
                .any(|n| n.tipo == TipoNotificacion::Mensaje && n.cuerpo.contains("cierre")));
            assert!(notis_lider_b
                .iter()
                .all(|n| n.tipo != TipoNotificacion::AccionBd));
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
