//! Tarea roja por check-ins directos en `/main` o `/main/dev` (issue #225).
//!
//! Unity Cloud no deja proteger esas ramas (#204): desde Plastic o `cm`, cualquiera
//! con rol `-rw` puede hacer check-in ahí. BranchBoard no lo puede impedir, pero sí
//! verlo: el polling lee por la API los changesets de las dos ramas y, si alguno
//! no es destino de un merge (los que entran por la revisión de una tarea), crea
//! una **tarea roja**. Es una tarea normal (en producción, se puede mandar a
//! revisión) que lleva la alerta hasta que un Líder o el Administrador **traslada**
//! los cambios a la rama de la tarea y los revierte de `main`/`dev`.
//!
//! Lo que viaja por `ops/`: `detectar_cambios_directos` (la tarea nueva),
//! `agregar_cambios_directos` (más check-ins directos en la misma rama mientras la
//! tarea sigue en rojo) y `trasladar_cambios_directos`.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::comandos::datos::carpeta_abierta;
use crate::db::tareas::{self as db_tareas, CambiosDirectos, ChangesetDirecto, Tarea};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos::{self, Evento};
use crate::identidad::permisos::LIDER_O_ADMIN;
use crate::modelo::{
    ahora_iso, hoy_local, normalizar_email, EstadoTarea, Rol, TipoNotificacion, TipoTarea,
};
use crate::notificaciones::{self, DatosNotificacionRoles};
use crate::ramas::{self, RAMA_DEV, RAMA_MAIN};
use crate::servidor::{conectar, Changeset, LlaveroSistema};
use crate::tareas::{self, AutorTarea};

/// Tipos de evento que maneja este módulo.
pub const TIPOS: &[&str] = &[
    "detectar_cambios_directos",
    "agregar_cambios_directos",
    "trasladar_cambios_directos",
];

/// Ramas en las que nadie debería hacer check-in directo.
pub const RAMAS_VIGILADAS: [&str; 2] = [RAMA_DEV, RAMA_MAIN];

/// `payload` de `agregar_cambios_directos`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadAgregar {
    pub tarea_id: String,
    pub changesets: Vec<ChangesetDirecto>,
}

/// `payload` de `trasladar_cambios_directos`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadTrasladar {
    pub tarea_id: String,
    pub rama_traslado: String,
    pub changesets_reversion: Vec<i64>,
}

fn es_rama_vigilada(rama: &str) -> bool {
    let rama = rama.trim().trim_end_matches('/');
    RAMAS_VIGILADAS.iter().any(|r| r.eq_ignore_ascii_case(rama))
}

fn leer_fecha(texto: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(texto.trim())
        .ok()
        .map(|f| f.with_timezone(&Utc))
}

/// Desde cuándo cuentan los check-ins directos: desde que se configuró el proyecto en BranchBoard (el
/// alta más vieja de un Administrador, desde el inicio de ese día en UTC; hasta #243 era la hora de su
/// firma). Lo de antes es la historia del proyecto Unity, no un error. `None` sin Administrador.
pub fn desde_del_proyecto(c: &Connection) -> Resultado<Option<DateTime<Utc>>> {
    Ok(usuarios::listar(c)?
        .into_iter()
        .filter(|u| u.rol == Rol::Administrador)
        .filter_map(|u| {
            chrono::NaiveDate::parse_from_str(u.fecha_alta.trim(), "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc())
        })
        .min())
}

/// Changesets de `rama` que ya no hay que mirar: los que se vio que son destino
/// de un merge y los que ya están en alguna tarea roja (también en la Papelera).
pub fn ya_vistos(c: &Connection, rama: &str) -> Resultado<BTreeSet<i64>> {
    let mut vistos: BTreeSet<i64> = {
        let mut consulta =
            c.prepare("SELECT changeset FROM changesets_revisados WHERE rama = ?1")?;
        let filas = consulta.query_map([rama], |f| f.get::<_, i64>(0))?;
        filas.collect::<rusqlite::Result<_>>()?
    };
    for t in db_tareas::listar(c)? {
        if let Some(cd) = t.cambios_directos {
            if cd.rama.eq_ignore_ascii_case(rama) {
                vistos.extend(cd.changesets.iter().map(|x| x.changeset));
            }
        }
    }
    Ok(vistos)
}

/// Anota localmente changesets de `rama` que son destino de un merge.
pub fn marcar_revisados(c: &Connection, rama: &str, changesets: &[i64]) -> Resultado<()> {
    for cs in changesets {
        c.execute(
            "INSERT OR IGNORE INTO changesets_revisados (rama, changeset) VALUES (?1, ?2)",
            rusqlite::params![rama, cs],
        )?;
    }
    Ok(())
}

/// Changesets de la API que todavía hay que revisar: posteriores a `desde`, que
/// no sean `cs:0` ni estén en `vistos`, del más viejo al más nuevo.
pub fn candidatos(
    changesets: &[Changeset],
    desde: DateTime<Utc>,
    vistos: &BTreeSet<i64>,
) -> Vec<Changeset> {
    let mut lista: Vec<Changeset> = changesets
        .iter()
        .filter(|cs| cs.changeset_id > 0 && !vistos.contains(&cs.changeset_id))
        .filter(|cs| leer_fecha(&cs.date).is_some_and(|f| f >= desde))
        .cloned()
        .collect();
    lista.sort_by_key(|cs| cs.changeset_id);
    lista
}

/// Tarea roja que todavía acepta check-ins nuevos en `rama` (no trasladada ni en la Papelera).
fn abierta_en(c: &Connection, rama: &str) -> Resultado<Option<Tarea>> {
    Ok(db_tareas::listar(c)?.into_iter().find(|t| {
        !t.eliminada
            && t.cambios_directos
                .as_ref()
                .is_some_and(|cd| cd.pendiente() && cd.rama.eq_ignore_ascii_case(rama))
    }))
}

fn nombre_corto(rama: &str) -> &'static str {
    if rama.eq_ignore_ascii_case(RAMA_MAIN) {
        "main"
    } else {
        "dev"
    }
}

/// Publica lo detectado en `rama`: agrega los check-ins a la tarea roja abierta de
/// esa rama o crea una nueva (asignada al autor si es del equipo) y avisa a
/// Líderes y Administrador. Devuelve el id de la tarea, o `None` si no había nada.
pub fn registrar_deteccion(
    c: &Connection,
    ops: &Path,
    detector: &str,
    rama: &str,
    directos: Vec<ChangesetDirecto>,
    ahora: &str,
    hoy: &str,
) -> Resultado<Option<String>> {
    if directos.is_empty() {
        return Ok(None);
    }
    let detector = normalizar_email(detector);
    let rama = rama.trim().trim_end_matches('/').to_string();
    if !es_rama_vigilada(&rama) {
        return Err(Error::Validacion(format!("{rama} no es main ni dev")));
    }

    if let Some(abierta) = abierta_en(c, &rama)? {
        let payload = PayloadAgregar {
            tarea_id: abierta.id.clone(),
            changesets: directos,
        };
        let mut ev = Evento::nuevo(
            "agregar_cambios_directos",
            Some(&abierta.id),
            &detector,
            serde_json::to_value(&payload).map_err(|e| {
                Error::Interno(format!(
                    "no se pudo serializar agregar_cambios_directos: {e}"
                ))
            })?,
        );
        ev.timestamp = ahora.to_string();
        eventos::publicar(c, ops, &ev, |tx| aplicar_agregar(tx, &payload))?;
        return Ok(Some(abierta.id));
    }

    let id = db_tareas::siguiente_id(c, None)?;
    let corto = nombre_corto(&rama);
    let mut tarea = Tarea::nueva(
        &id,
        &format!("Cambios directos en {corto}"),
        TipoTarea::Correccion,
        &detector,
        hoy,
    );
    tarea.estado = EstadoTarea::EnProduccion;
    tarea.pendiente_aprobacion = false;
    let autor = normalizar_email(&directos[0].autor);
    if usuarios::buscar(c, &autor)?.is_some() {
        tarea.asignado_a = Some(autor);
    }
    tarea.cambios_directos = Some(CambiosDirectos {
        rama: rama.clone(),
        detectado: ahora.to_string(),
        changesets: directos,
        rama_traslado: None,
        changesets_reversion: Vec::new(),
        trasladado_en: None,
        trasladado_por: None,
    });
    let mut ev = Evento::nuevo(
        "detectar_cambios_directos",
        Some(&id),
        &detector,
        serde_json::to_value(&tarea).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar detectar_cambios_directos: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| aplicar_detectar(tx, &tarea, &detector))?;

    let cs_texto = tarea
        .cambios_directos
        .as_ref()
        .map(|cd| {
            cd.changesets
                .iter()
                .map(|x| format!("cs:{}", x.changeset))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    notificaciones::crear_para_roles(
        c,
        ops,
        &detector,
        &DatosNotificacionRoles {
            tipo: TipoNotificacion::Advertencia,
            roles: LIDER_O_ADMIN,
            titulo: &format!("Check-in directo en {corto}"),
            cuerpo: &format!("{id}: {cs_texto} en {rama}"),
            tarea_id: Some(&id),
            accionable: false,
        },
        ahora,
    )?;
    Ok(Some(id))
}

fn validar_changesets(cd: &CambiosDirectos) -> Resultado<()> {
    if !es_rama_vigilada(&cd.rama) {
        return Err(Error::Validacion(format!(
            "Los cambios directos solo pueden estar en main o dev, no en {}",
            cd.rama
        )));
    }
    if cd.changesets.is_empty() || cd.changesets.iter().any(|x| x.changeset <= 0) {
        return Err(Error::Validacion(
            "La tarea roja necesita al menos un changeset válido".into(),
        ));
    }
    Ok(())
}

/// Aplica `detectar_cambios_directos`. Si dos máquinas detectaron lo mismo a la
/// vez, queda la tarea con `detectado` más viejo (a igual hora, el id menor) en
/// todas: la otra se borra si nadie la trasladó todavía.
pub fn aplicar_detectar(c: &Connection, tarea: &Tarea, usuario: &str) -> Resultado<()> {
    let cd = tarea.cambios_directos.as_ref().ok_or_else(|| {
        Error::Validacion("detectar_cambios_directos sin cambios_directos".into())
    })?;
    validar_changesets(cd)?;
    if tarea.tarea_padre_id.is_some() {
        return Err(Error::Validacion(
            "Una tarea roja es siempre una tarea general".into(),
        ));
    }

    let mut a_borrar = Vec::new();
    for otra in db_tareas::listar(c)? {
        let Some(ref ocd) = otra.cambios_directos else {
            continue;
        };
        let comparten = ocd.rama.eq_ignore_ascii_case(&cd.rama)
            && cd.changesets.iter().any(|x| ocd.incluye(x.changeset));
        if !comparten {
            continue;
        }
        if otra.id == tarea.id {
            return Ok(()); // ya aplicado
        }
        let gana_la_otra = (ocd.detectado.as_str(), otra.id.as_str())
            <= (cd.detectado.as_str(), tarea.id.as_str());
        if gana_la_otra || !ocd.pendiente() {
            return Ok(());
        }
        a_borrar.push(otra.id.clone());
    }
    for id in a_borrar {
        db_tareas::borrar_fila(c, &id)?;
    }

    let mut nueva = tarea.clone();
    nueva.creado_por = normalizar_email(usuario);
    nueva.cambios_directos = None;
    db_tareas::insertar(c, &nueva)?;
    db_tareas::guardar_cambios_directos(c, &nueva.id, cd)
}

/// Aplica `agregar_cambios_directos`: suma los check-ins que ninguna tarea roja
/// de esa rama tenga todavía, solo si la tarea sigue en rojo.
pub fn aplicar_agregar(c: &Connection, payload: &PayloadAgregar) -> Resultado<()> {
    let tarea = db_tareas::obtener(c, payload.tarea_id.trim())?;
    let mut cd = tarea
        .cambios_directos
        .clone()
        .ok_or_else(|| Error::Validacion(format!("La tarea {} no es una tarea roja", tarea.id)))?;
    if tarea.eliminada || !cd.pendiente() {
        return Err(Error::Validacion(format!(
            "La tarea {} ya no acepta check-ins directos nuevos",
            tarea.id
        )));
    }
    let vistos = ya_vistos_en_tareas(c, &cd.rama)?;
    let mut agregados = false;
    for x in &payload.changesets {
        if x.changeset > 0 && !vistos.contains(&x.changeset) && !cd.incluye(x.changeset) {
            cd.changesets.push(x.clone());
            agregados = true;
        }
    }
    if !agregados {
        return Ok(());
    }
    cd.changesets.sort_by_key(|x| x.changeset);
    db_tareas::guardar_cambios_directos(c, &tarea.id, &cd)
}

fn ya_vistos_en_tareas(c: &Connection, rama: &str) -> Resultado<BTreeSet<i64>> {
    let mut vistos = BTreeSet::new();
    for t in db_tareas::listar(c)? {
        if let Some(cd) = t.cambios_directos {
            if cd.rama.eq_ignore_ascii_case(rama) {
                vistos.extend(cd.changesets.iter().map(|x| x.changeset));
            }
        }
    }
    Ok(vistos)
}

/// Valida que `autor` pueda trasladar los cambios de la tarea roja `id`.
pub fn preparar_traslado(
    c: &Connection,
    autor: &AutorTarea<'_>,
    id: &str,
) -> Resultado<(Tarea, CambiosDirectos)> {
    if !autor.rol.es_lider_o_admin() {
        return Err(Error::SinPermiso);
    }
    let tarea = db_tareas::obtener(c, id.trim())?;
    let cd = tarea
        .cambios_directos
        .clone()
        .ok_or_else(|| Error::Validacion(format!("La tarea {} no es una tarea roja", tarea.id)))?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if !cd.pendiente() {
        return Err(Error::Validacion(format!(
            "Los cambios de {} ya se trasladaron a {}",
            tarea.id,
            cd.rama_traslado.as_deref().unwrap_or("su rama")
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    Ok((tarea, cd))
}

/// Changeset del que nace la rama del traslado: en `dev`, el anterior al primer
/// check-in directo; en `main`, la cabeza de `dev` (esos cambios no están en
/// `dev`). Ver [`ramas::trasladar_cambios_workspace`].
pub fn changeset_base(cd: &CambiosDirectos, cabeza_dev: i64) -> Resultado<i64> {
    if cd.rama.eq_ignore_ascii_case(RAMA_MAIN) {
        return Ok(cabeza_dev);
    }
    let primero = cd
        .changesets
        .iter()
        .min_by_key(|x| x.changeset)
        .ok_or_else(|| Error::Validacion("La tarea roja no tiene changesets".into()))?;
    primero.padre.ok_or_else(|| {
        Error::Validacion(format!(
            "No se sabe de qué changeset sale cs:{}: vuelve a revisar Unity VC",
            primero.changeset
        ))
    })
}

/// Publica `trasladar_cambios_directos`: la tarea deja de estar en rojo y queda
/// trabajándose en `rama_traslado`.
pub fn registrar_traslado(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    payload: &PayloadTrasladar,
    ahora: &str,
) -> Resultado<Tarea> {
    let email = normalizar_email(autor.email);
    let mut ev = Evento::nuevo(
        "trasladar_cambios_directos",
        Some(&payload.tarea_id),
        &email,
        serde_json::to_value(payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar trasladar_cambios_directos: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_trasladar(tx, payload, &email, Some(autor.rol), ahora)
    })?;
    db_tareas::obtener(c, &payload.tarea_id)
}

fn aplicar_trasladar(
    c: &Connection,
    payload: &PayloadTrasladar,
    usuario: &str,
    rol: Option<Rol>,
    cuando: &str,
) -> Resultado<()> {
    if !rol.is_some_and(Rol::es_lider_o_admin) {
        return Err(Error::SinPermiso);
    }
    let rama = payload.rama_traslado.trim().trim_end_matches('/');
    if !rama.starts_with(&format!("{RAMA_DEV}/")) {
        return Err(Error::Validacion(format!(
            "La rama del traslado no es una rama de tarea: {rama}"
        )));
    }
    let mut tarea = db_tareas::obtener(c, payload.tarea_id.trim())?;
    let mut cd = tarea
        .cambios_directos
        .clone()
        .ok_or_else(|| Error::Validacion(format!("La tarea {} no es una tarea roja", tarea.id)))?;
    if !cd.pendiente() {
        return Ok(());
    }
    cd.rama_traslado = Some(rama.to_string());
    cd.changesets_reversion = payload.changesets_reversion.clone();
    cd.trasladado_en = Some(cuando.to_string());
    cd.trasladado_por = Some(normalizar_email(usuario));
    tarea.rama_asociada = Some(rama.to_string());
    tarea.modo_trabajo = Some("rama".into());
    tarea.cambios_directos = None;
    db_tareas::actualizar(c, &tarea)?;
    db_tareas::guardar_cambios_directos(c, &tarea.id, &cd)
}

/// Aplica un evento de este módulo recibido desde `ops/`.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    match evento.tipo.as_str() {
        "detectar_cambios_directos" => {
            let tarea: Tarea = serde_json::from_value(evento.payload.clone()).map_err(|e| {
                Error::Validacion(format!("detectar_cambios_directos inválido: {e}"))
            })?;
            aplicar_detectar(c, &tarea, &evento.usuario)
        }
        "agregar_cambios_directos" => {
            let payload: PayloadAgregar =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("agregar_cambios_directos inválido: {e}"))
                })?;
            aplicar_agregar(c, &payload)
        }
        "trasladar_cambios_directos" => {
            let payload: PayloadTrasladar = serde_json::from_value(evento.payload.clone())
                .map_err(|e| {
                    Error::Validacion(format!("trasladar_cambios_directos inválido: {e}"))
                })?;
            let rol = tareas::rol_de(c, &evento.usuario)?;
            aplicar_trasladar(c, &payload, &evento.usuario, rol, &evento.timestamp)
        }
        otro => Err(Error::Validacion(format!(
            "evento desconocido para cambios directos: {otro}"
        ))),
    }
}

/// Revisa `/main` y `/main/dev` en el servidor y publica las tareas rojas que
/// hagan falta (lo llama el polling, `revisar_actualizaciones_en`). Devuelve los
/// ids de las tareas rojas creadas o con check-ins nuevos.
pub fn revisar_en(estado: &AppState, email: &str) -> Resultado<Vec<String>> {
    let Ok(repo) = estado.repo_abierto() else {
        return Ok(Vec::new());
    };
    let Ok(ops) = carpeta_abierta(estado).and_then(|carpeta| eventos::carpeta_ops(&carpeta)) else {
        return Ok(Vec::new());
    };
    let (desde, vistos) = estado.con_db(|db| {
        let desde = desde_del_proyecto(db.conexion())?;
        let mut vistos = Vec::new();
        for rama in RAMAS_VIGILADAS {
            vistos.push(ya_vistos(db.conexion(), rama)?);
        }
        Ok((desde, vistos))
    })?;
    let Some(desde) = desde else {
        return Ok(Vec::new());
    };

    let cliente = conectar(estado, &LlaveroSistema, email, &repo.org)?;
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    let workspace = estado.workspace_abierto().ok();
    let mut tocadas = Vec::new();
    for (rama, vistos_rama) in RAMAS_VIGILADAS.iter().zip(vistos) {
        let changesets = cliente
            .changesets_de_rama(&repo.repo, rama)?
            .unwrap_or_default();
        let lista = candidatos(&changesets, desde, &vistos_rama);
        if lista.is_empty() {
            continue;
        }
        let destinos = ramas::destinos_de_merge(estado, &repo_cm, rama)?;
        let (merges, directos): (Vec<Changeset>, Vec<Changeset>) = lista
            .into_iter()
            .partition(|cs| destinos.contains(&cs.changeset_id));
        let directos: Vec<ChangesetDirecto> = directos
            .into_iter()
            .map(|cs| ChangesetDirecto {
                changeset: cs.changeset_id,
                padre: cs.parent_changeset_id,
                autor: normalizar_email(&cs.owner),
                fecha: cs.date,
                comentario: cs.comment.filter(|t| !t.trim().is_empty()),
                archivos: ramas::archivos_changeset(
                    estado,
                    workspace.as_deref(),
                    &repo_cm,
                    cs.changeset_id,
                )
                .unwrap_or_default(),
            })
            .collect();
        let merges: Vec<i64> = merges.iter().map(|cs| cs.changeset_id).collect();
        let tocada = estado.con_db(|db| {
            marcar_revisados(db.conexion(), rama, &merges)?;
            registrar_deteccion(
                db.conexion(),
                &ops,
                email,
                rama,
                directos,
                &ahora_iso(),
                &hoy_local(),
            )
        })?;
        tocadas.extend(tocada);
    }
    Ok(tocadas)
}

/// `trasladar_cambios_directos(id)`: solo Líder o Administrador. Crea la rama de
/// la tarea roja por la API, le pasa los check-ins directos con cherrypick y los
/// revierte en `main`/`dev` desde el workspace de quien lo pide (la única
/// excepción a "nunca check-in a main/dev"), y publica el traslado.
pub fn trasladar_en(estado: &AppState, id: &str) -> Resultado<Tarea> {
    let yo = estado.exigir_permiso("trasladar_cambios_directos")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    let workspace = estado.workspace_abierto()?;
    let repo = estado.repo_abierto()?;
    let autor = AutorTarea {
        email: &yo.email,
        rol: yo.rol,
    };
    let (tarea, cd) = estado.con_db(|db| preparar_traslado(db.conexion(), &autor, id))?;

    let _guarda = estado.iniciar_operacion();
    let cliente = conectar(estado, &LlaveroSistema, &yo.email, &repo.org)?;
    let nombre = ramas::nombre_rama(&tarea.id, &tarea.titulo)?;
    let ruta = ramas::ruta_rama_nueva(None, &nombre)?;
    let ya_en_la_rama = match cliente.rama(&repo.repo, &ruta)? {
        // Un traslado anterior se cortó: si la rama ya tiene check-ins, el
        // cherrypick ya se hizo y solo falta revertir.
        Some(_) => !cliente
            .changesets_de_rama(&repo.repo, &ruta)?
            .unwrap_or_default()
            .is_empty(),
        None => {
            let cabeza_dev = cliente
                .rama(&repo.repo, RAMA_DEV)?
                .ok_or_else(|| Error::Validacion(format!("No existe {RAMA_DEV}")))?
                .head_changeset;
            let base = changeset_base(&cd, cabeza_dev)?;
            cliente.crear_rama(
                &repo.repo,
                &ruta,
                base,
                &format!(
                    "{}: cambios directos trasladados desde {}",
                    tarea.id, cd.rama
                ),
            )?;
            false
        }
    };

    let changesets: Vec<i64> = cd.changesets.iter().map(|x| x.changeset).collect();
    let reversion = ramas::trasladar_cambios_workspace(
        estado,
        &workspace,
        &cd.rama,
        &ruta,
        &changesets,
        ya_en_la_rama,
        &format!("{}: traslado de cambios directos de {}", tarea.id, cd.rama),
        &format!(
            "{}: se quitan de {} los cambios directos trasladados a {ruta}",
            tarea.id, cd.rama
        ),
    )?;

    let payload = PayloadTrasladar {
        tarea_id: tarea.id.clone(),
        rama_traslado: ruta,
        changesets_reversion: reversion.clone(),
    };
    estado.con_db(|db| {
        marcar_revisados(db.conexion(), &cd.rama, &reversion)?;
        registrar_traslado(db.conexion(), &ops, &autor, &payload, &ahora_iso())
    })
}

/// Comando Tauri de [`trasladar_en`].
#[tauri::command]
pub async fn trasladar_cambios_directos(
    estado: tauri::State<'_, AppState>,
    id: String,
) -> Resultado<Tarea> {
    trasladar_en(&estado, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    const ANA: &str = "ana@virustf.dev";
    const LIDER: &str = "lider@virustf.dev";

    fn cs(n: i64, fecha: &str) -> Changeset {
        Changeset {
            changeset_id: n,
            owner: "Ana@VirusTF.dev".into(),
            date: fecha.into(),
            comment: Some(format!("cs {n}")),
            parent_changeset_id: Some(n - 1),
        }
    }

    fn directo(n: i64) -> ChangesetDirecto {
        ChangesetDirecto {
            changeset: n,
            padre: Some(n - 1),
            autor: ANA.into(),
            fecha: "2026-11-17T10:00:00Z".into(),
            comentario: None,
            archivos: vec!["Assets/a.cs".into()],
        }
    }

    fn roja(id: &str, rama: &str, detectado: &str, changesets: &[i64]) -> Tarea {
        let mut t = Tarea::nueva(
            id,
            "Cambios directos en dev",
            TipoTarea::Correccion,
            ANA,
            "2026-11-17",
        );
        t.estado = EstadoTarea::EnProduccion;
        t.cambios_directos = Some(CambiosDirectos {
            rama: rama.into(),
            detectado: detectado.into(),
            changesets: changesets.iter().map(|n| directo(*n)).collect(),
            rama_traslado: None,
            changesets_reversion: Vec::new(),
            trasladado_en: None,
            trasladado_por: None,
        });
        t
    }

    #[test]
    fn candidatos_desde_la_configuracion_sin_cs0_ni_vistos() {
        let desde = leer_fecha("2026-11-17T09:00:00.000Z").unwrap();
        let lista = [
            cs(0, "2026-11-12T11:49:37Z"),
            cs(6, "2026-11-12T12:38:15Z"), // antes de configurar el proyecto
            cs(12, "2026-11-17T10:00:00Z"),
            cs(11, "2026-11-17T09:30:00Z"),
            cs(13, "2026-11-17T11:00:00Z"),
        ];
        let vistos = BTreeSet::from([13]);
        let ids: Vec<i64> = candidatos(&lista, desde, &vistos)
            .iter()
            .map(|c| c.changeset_id)
            .collect();
        assert_eq!(ids, [11, 12]);
    }

    #[test]
    fn base_del_traslado_en_dev_y_en_main() {
        let mut cd = roja("T-101", RAMA_DEV, "x", &[9, 7])
            .cambios_directos
            .unwrap();
        assert_eq!(
            changeset_base(&cd, 40).unwrap(),
            6,
            "en dev: padre del primero"
        );
        cd.rama = RAMA_MAIN.into();
        assert_eq!(
            changeset_base(&cd, 40).unwrap(),
            40,
            "en main: cabeza de dev"
        );
        cd.rama = RAMA_DEV.into();
        cd.changesets[1].padre = None;
        assert!(changeset_base(&cd, 40).is_err());
    }

    #[test]
    fn la_tarea_roja_se_guarda_y_se_lee_con_sus_cambios() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        aplicar_detectar(
            c,
            &roja("T-101", RAMA_DEV, "2026-11-17T10:00:00.000Z", &[7]),
            ANA,
        )
        .unwrap();
        let t = db_tareas::obtener(c, "T-101").unwrap();
        let cd = t.cambios_directos.clone().unwrap();
        assert!(cd.pendiente());
        assert_eq!(cd.changesets[0].archivos, ["Assets/a.cs"]);
        assert_eq!(t.estado, EstadoTarea::EnProduccion);
        // Aplicar dos veces el mismo evento no duplica nada.
        aplicar_detectar(
            c,
            &roja("T-101", RAMA_DEV, "2026-11-17T10:00:00.000Z", &[7]),
            ANA,
        )
        .unwrap();
        assert_eq!(db_tareas::listar(c).unwrap().len(), 1);
        assert_eq!(ya_vistos(c, RAMA_DEV).unwrap(), BTreeSet::from([7]));
        assert!(ya_vistos(c, RAMA_MAIN).unwrap().is_empty());
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["cambios_directos"]["rama"], "/main/dev");
    }

    #[test]
    fn dos_detecciones_a_la_vez_dejan_la_mas_vieja_en_las_dos_maquinas() {
        let vieja = roja("T-105", RAMA_DEV, "2026-11-17T10:00:00.000Z", &[7]);
        let nueva = roja("T-106", RAMA_DEV, "2026-11-17T10:00:05.000Z", &[7, 8]);
        for orden in [[&vieja, &nueva], [&nueva, &vieja]] {
            let db = Db::en_memoria().unwrap();
            let c = db.conexion();
            for t in orden {
                aplicar_detectar(c, t, ANA).unwrap();
            }
            let ids: Vec<String> = db_tareas::listar(c)
                .unwrap()
                .into_iter()
                .map(|t| t.id)
                .collect();
            assert_eq!(ids, ["T-105"]);
        }
    }

    #[test]
    fn agregar_solo_suma_lo_nuevo_y_no_despues_del_traslado() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        aplicar_detectar(c, &roja("T-101", RAMA_DEV, "x", &[7]), ANA).unwrap();
        aplicar_agregar(
            c,
            &PayloadAgregar {
                tarea_id: "T-101".into(),
                changesets: vec![directo(9), directo(7)],
            },
        )
        .unwrap();
        let cd = db_tareas::obtener(c, "T-101")
            .unwrap()
            .cambios_directos
            .unwrap();
        assert_eq!(
            cd.changesets
                .iter()
                .map(|x| x.changeset)
                .collect::<Vec<_>>(),
            [7, 9]
        );

        aplicar_trasladar(
            c,
            &PayloadTrasladar {
                tarea_id: "T-101".into(),
                rama_traslado: "/main/dev/T-101-cambios-directos-en-dev".into(),
                changesets_reversion: vec![12, 11],
            },
            LIDER,
            Some(Rol::Lider),
            "2026-11-17T11:00:00.000Z",
        )
        .unwrap();
        let t = db_tareas::obtener(c, "T-101").unwrap();
        let cd = t.cambios_directos.unwrap();
        assert!(!cd.pendiente());
        assert_eq!(cd.trasladado_por.as_deref(), Some(LIDER));
        assert_eq!(cd.changesets_reversion, [12, 11]);
        assert_eq!(
            t.rama_asociada.as_deref(),
            Some("/main/dev/T-101-cambios-directos-en-dev")
        );
        assert!(aplicar_agregar(
            c,
            &PayloadAgregar {
                tarea_id: "T-101".into(),
                changesets: vec![directo(20)],
            },
        )
        .is_err());
    }

    #[test]
    fn trasladar_exige_lider_o_admin_y_rama_de_tarea() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        aplicar_detectar(c, &roja("T-101", RAMA_DEV, "x", &[7]), ANA).unwrap();
        let payload = PayloadTrasladar {
            tarea_id: "T-101".into(),
            rama_traslado: "/main/dev/T-101-x".into(),
            changesets_reversion: vec![8],
        };
        for rol in [None, Some(Rol::Desarrollador), Some(Rol::Tester)] {
            assert!(matches!(
                aplicar_trasladar(c, &payload, ANA, rol, "t"),
                Err(Error::SinPermiso)
            ));
        }
        let mut a_dev = payload.clone();
        a_dev.rama_traslado = RAMA_DEV.into();
        assert!(aplicar_trasladar(c, &a_dev, LIDER, Some(Rol::Lider), "t").is_err());
        let autor = AutorTarea {
            email: ANA,
            rol: Rol::Desarrollador,
        };
        assert!(matches!(
            preparar_traslado(c, &autor, "T-101"),
            Err(Error::SinPermiso)
        ));
        let lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };
        assert!(preparar_traslado(c, &lider, "T-101").is_ok());
    }

    #[test]
    fn solo_main_y_dev() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        let otra = roja("T-101", "/main/dev/T-5", "x", &[7]);
        assert!(aplicar_detectar(c, &otra, ANA).is_err());
        assert!(es_rama_vigilada("/MAIN/dev/"));
        assert!(!es_rama_vigilada("/main/dev/T-1"));
    }
}
