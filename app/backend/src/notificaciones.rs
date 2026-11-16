//! Notificaciones internas del equipo (campana y tabla `notificaciones`, issue #116).
//!
//! - Las notificaciones nacen siempre al aplicar eventos de `ops/`: ya sea desde
//!   eventos propios (`"crear_notificacion"` y `"crear_notificaciones"`, usados por
//!   comandos de comunicación como mensajes o solicitud de acción sobre la BD) o como
//!   efecto determinista dentro de los manejadores de `tareas`.
//! - El estado `leida` es local de cada máquina (`obtener_notificaciones`,
//!   `marcar_leida`, `desmarcar_leida`, `marcar_todas_leidas`).

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::notificaciones::{self as db_notis, Notificacion};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::{normalizar_email, Rol, TipoNotificacion};
use crate::tareas::rol_verificado_de;

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
    ops: &Path,
    emisor: &str,
    datos: &DatosNotificacion<'_>,
    ahora: &str,
) -> Resultado<Notificacion> {
    let dest = normalizar_email(datos.para);
    if dest.is_empty() || usuarios::buscar(c, &dest)?.is_none() {
        return Err(Error::NoEncontrado(format!("el usuario {dest}")));
    }
    let titulo = datos.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion("La notificación no tiene título".into()));
    }
    let noti = Notificacion {
        id: format!("noti_{}", eventos::uuid()),
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
    eventos::publicar(c, ops, &ev, |tx| emitir_si_falta(tx, &noti))?;
    Ok(noti)
}

/// Función común para notificar a todos los usuarios con alguno de `datos.roles` en un solo
/// evento de `ops/` (#116).
pub fn crear_para_roles(
    c: &Connection,
    ops: &Path,
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
    eventos::publicar(c, ops, &ev, |tx| aplicar_lote(tx, &lote))?;
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
pub fn aplicar_evento(c: &Connection, publica_admin: &str, evento: &Evento) -> Resultado<()> {
    if rol_verificado_de(c, publica_admin, &evento.usuario)?.is_none() {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use crate::db::{usuarios, Db};
    use crate::eventos::carpeta_ops;
    use crate::eventos::configurar_carpeta_compartida;
    use crate::eventos::lectura::{self, sincronizar};
    use crate::identidad::firmas;
    use crate::identidad::llaves::{
        cargar_llave_admin, fijar_llave_publica_admin, generar_llaves_admin,
    };
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
        ops: PathBuf,
        db_b: Db,
        local_b: PathBuf,
        publica: String,
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
        configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
        configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
        let ops = carpeta_ops(&local_a).unwrap();

        let publica = generar_llaves_admin(&local_a, None).unwrap();
        fijar_llave_publica_admin(&local_b, &publica).unwrap();
        let privada = cargar_llave_admin(&local_a, None).unwrap();

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
                firma: Some(firmas::firmar_rol(&privada, correo, rol, AHORA).unwrap()),
                firmado_en: Some(AHORA.into()),
                firmado_por: Some(ADMIN.into()),
                llave_publica: None,
                certificado: None,
            };
            usuarios::insertar(db_a.conexion(), &fila).unwrap();
            usuarios::insertar(db_b.conexion(), &fila).unwrap();
        }

        Entorno {
            _tmp: tmp,
            db_a,
            ops,
            db_b,
            local_b,
            publica,
        }
    }

    #[test]
    fn crear_y_crear_para_roles_viajan_por_ops_y_descartan_no_registrado() {
        let e = setup();
        let n1 = crear(
            e.db_a.conexion(),
            &e.ops,
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
            &e.ops,
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
        eventos::escribir_evento(&e.ops, &falso).unwrap();

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b = sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, &e.publica, ev)
        })
        .unwrap();
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
            &e.ops,
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
            &e.ops,
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
            &e.ops,
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
            &e.ops,
            &dev,
            &t_ok.id,
            crate::modelo::EstadoTarea::EnRevision,
            "2026-11-16T10:01:03.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &lider,
            &t_ok.id,
            crate::modelo::EstadoTarea::Terminada,
            "2026-11-16T10:01:04.000Z",
        )
        .unwrap();
        reabrir(
            e.db_a.conexion(),
            &e.ops,
            &lider,
            &t_ok.id,
            "Falta contraste",
            "2026-11-16T10:01:05.000Z",
        )
        .unwrap();
        eliminar(
            e.db_a.conexion(),
            &e.ops,
            &lider,
            &creadas[0].id,
            "Duplicada",
            "2026-11-16T10:01:06.000Z",
        )
        .unwrap();

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, &e.publica, ev)
        })
        .unwrap();

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
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_notificaciones() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::notificaciones::{
            desmarcar_leida_en, marcar_leida_en, marcar_todas_leidas_en, obtener_notificaciones_en,
        };
        use crate::comandos::tareas::{cambiar_estado_en, eliminar_tarea_en, reabrir_en};
        use crate::estado::{llave_publica_del_proyecto, AppState};
        use crate::identidad::equipo::{self, Firmante};
        use crate::modelo::hoy_local;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
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

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let (publica, admin_fila) = estado_a
                .con_db(|db| {
                    let pub_k = llave_publica_del_proyecto(db)?;
                    let todos = usuarios::listar(db.conexion())?;
                    Ok((pub_k, todos.into_iter().next().unwrap()))
                })
                .unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let llave_admin = cargar_llave_admin(&local_a, None).unwrap();

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
                    let firmante_admin = Firmante {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                        llave: &llave_admin,
                    };
                    equipo::crear(
                        db_a.conexion(),
                        &ops_a,
                        &publica,
                        &firmante_admin,
                        DEV,
                        Rol::Desarrollador,
                        &ahora_pausa(),
                        &hoy_real,
                    )?;
                    // 1. Desarrollador crea tarea -> emite `aprobacion_tarea` para Admin.
                    crear_tarea(
                        db_a.conexion(),
                        &ops_a,
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
                        &ops_a,
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
                        &ops_a,
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
            eventos::escribir_evento(&ops_a, &ev_falso).unwrap();

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 5. Máquina B sincroniza vía OneDrive: recibe las mismas notificaciones, descarta la falsa
            //    y conserva `leida = false` localmente (no se filtra lo marcado en A).
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            println!("B sincroniza notificaciones desde OneDrive: {sync_b:?}");
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
}
