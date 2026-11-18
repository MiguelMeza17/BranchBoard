//! Compactar `ops/` con un snapshot firmado por el Administrador (issue #94).
//! Ver "Compactación/purga de `ops/`" en BACKEND.md.
//!
//! - Solo el Administrador puede compactar (`compactar_eventos`). No corre solo.
//! - Calcula el estado resultante de todos los eventos con más de
//!   [`DIAS_PARA_COMPACTAR`] (30) días de antigüedad (partiendo del último
//!   snapshot válido si ya había uno) y lo escribe en `ops/snapshot_<fecha>.json`
//!   como un [`Evento`] de tipo `"compactar_eventos"` firmado con la llave
//!   privada del Administrador (`Evento::firmar`).
//! - Borra de `ops/` los eventos ya incluidos en ese snapshot (y cualquier
//!   snapshot anterior que haya quedado reemplazado).
//! - Al reconstruir el `.db` local (o al entrar desde una máquina nueva),
//!   [`sincronizar`](super::sincronizar) verifica la firma del snapshot con
//!   [`equipo::firmante_del_evento`], parte del **último snapshot válido** y
//!   aplica únicamente los eventos posteriores a su corte (`hasta`).
//! - Una máquina que ya había aplicado todos los eventos incluidos en el
//!   snapshot no necesita pisar sus tablas locales: al ver el snapshot solo lo
//!   anota en `eventos_aplicados` y guarda su corte.

use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::lectura::{self, Aplicacion};
use super::{marcar_aplicado, ya_aplicado, Evento};
use crate::db::chat::{self, GrupoChat, Mensaje};
use crate::db::commits::{self, CambioPrestado, Commit};
use crate::db::configuracion::{self, Ajuste};
use crate::db::documentos::{self, Documento};
use crate::db::ediciones::{self, EdicionPendiente};
use crate::db::notificaciones::{self, Notificacion};
use crate::db::sprints::{self, ReporteSprint, Sprint};
use crate::db::tareas::{self, Tarea};
use crate::db::usuarios::{self, Usuario};
use crate::db::Db;
use crate::error::{Error, Resultado};
use crate::identidad::equipo::{self, Firmante};
use crate::modelo::Rol;

/// Antigüedad mínima que debe tener un evento para entrar en el snapshot.
pub const DIAS_PARA_COMPACTAR: i64 = 30;
/// Prefijo de los archivos de snapshot dentro de `ops/`.
pub const PREFIJO_SNAPSHOT: &str = "snapshot_";
/// Clave en `meta` donde cada base anota hasta qué instante cubre el último snapshot aplicado.
const META_ULTIMO_SNAPSHOT_HASTA: &str = "ultimo_snapshot_hasta";

/// `true` si el nombre de archivo de `ops/` corresponde a un snapshot (`snapshot_<fecha>.json`).
pub fn es_snapshot(nombre: &str) -> bool {
    nombre.starts_with(PREFIJO_SNAPSHOT) && nombre.ends_with(".json")
}

/// Nombre del archivo de snapshot para una fecha `AAAA-MM-DD`.
pub fn nombre_snapshot(fecha: &str) -> String {
    format!("{PREFIJO_SNAPSHOT}{fecha}.json")
}

/// Resultado de `compactar_eventos()`, para el frontend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumenCompactacion {
    /// Cuántos eventos de más de 30 días se juntaron y borraron de `ops/`.
    pub compactados: usize,
    /// Nombre del snapshot escrito en `ops/` (`snapshot_<fecha>.json`), o `None`
    /// si no había eventos de más de 30 días.
    pub archivo: Option<String>,
}

/// `Documento` con su `ruta` interna incluida para que sobreviva al snapshot
/// (en [`Documento`] el campo `ruta` tiene `#[serde(skip)]` hacia el frontend).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentoSnapshot {
    pub id: String,
    pub nombre: String,
    pub tamano: u64,
    pub subido_por: String,
    pub fecha: String,
    pub pendiente: bool,
    pub ruta: String,
}

impl From<Documento> for DocumentoSnapshot {
    fn from(d: Documento) -> Self {
        Self {
            id: d.id,
            nombre: d.nombre,
            tamano: d.tamano,
            subido_por: d.subido_por,
            fecha: d.fecha,
            pendiente: d.pendiente,
            ruta: d.ruta,
        }
    }
}

impl From<DocumentoSnapshot> for Documento {
    fn from(d: DocumentoSnapshot) -> Self {
        Self {
            id: d.id,
            nombre: d.nombre,
            tamano: d.tamano,
            subido_por: d.subido_por,
            fecha: d.fecha,
            pendiente: d.pendiente,
            ruta: d.ruta,
        }
    }
}

/// Foto consolidada de todas las tablas compartidas del proyecto.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EstadoSnapshot {
    #[serde(default)]
    pub usuarios: Vec<Usuario>,
    #[serde(default)]
    pub configuracion_proyecto: Vec<Ajuste>,
    #[serde(default)]
    pub tareas: Vec<Tarea>,
    #[serde(default)]
    pub ediciones_pendientes: Vec<EdicionPendiente>,
    #[serde(default)]
    pub sprints: Vec<Sprint>,
    #[serde(default)]
    pub reportes_sprint: Vec<ReporteSprint>,
    #[serde(default)]
    pub commits: Vec<Commit>,
    #[serde(default)]
    pub cambios_prestados: Vec<CambioPrestado>,
    #[serde(default)]
    pub notificaciones: Vec<Notificacion>,
    #[serde(default)]
    pub documentos: Vec<DocumentoSnapshot>,
    #[serde(default)]
    pub grupos_chat: Vec<GrupoChat>,
    #[serde(default)]
    pub mensajes_chat: Vec<Mensaje>,
}

/// Contenido (`payload`) de `ops/snapshot_<fecha>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Fecha (`AAAA-MM-DD`) en que se generó.
    pub fecha: String,
    /// Instante de corte (ISO 8601 UTC): incluye todos los eventos con `timestamp <= hasta`.
    pub hasta: String,
    /// Nombres de los archivos de eventos que quedaron consolidados en este snapshot.
    #[serde(default)]
    pub incluidos: Vec<String>,
    pub estado: EstadoSnapshot,
}

/// Lee todas las tablas compartidas de `c` para guardarlas en un snapshot.
pub fn exportar_estado(c: &Connection) -> Resultado<EstadoSnapshot> {
    let usuarios = usuarios::listar(c)?;
    let configuracion_proyecto = configuracion::listar(c)?;
    let mut tareas = tareas::listar(c)?;
    for t in &mut tareas {
        t.edicion_pendiente = false;
        t.modificada = false;
        t.ultimo_changeset_visto = None;
    }
    let ediciones_pendientes = ediciones::listar(c)?;
    let mut sprints = sprints::listar(c)?;
    let mut reportes_sprint = Vec::new();
    for s in &mut sprints {
        s.reporte = None;
        reportes_sprint.extend(sprints::reportes(c, &s.id)?);
    }
    let mut commits = Vec::new();
    let mut cambios_prestados = Vec::new();
    for t in &tareas {
        commits.extend(commits::de_tarea(c, &t.id)?);
        cambios_prestados.extend(commits::prestamos_de_tarea(c, &t.id)?);
    }
    let notificaciones = listar_todas_notificaciones(c)?;
    let documentos = documentos::listar(c)?
        .into_iter()
        .map(DocumentoSnapshot::from)
        .collect();
    let grupos_chat = listar_todos_grupos_chat(c)?;
    let mensajes_chat = listar_todos_mensajes_chat(c)?;

    Ok(EstadoSnapshot {
        usuarios,
        configuracion_proyecto,
        tareas,
        ediciones_pendientes,
        sprints,
        reportes_sprint,
        commits,
        cambios_prestados,
        notificaciones,
        documentos,
        grupos_chat,
        mensajes_chat,
    })
}

/// Reemplaza las tablas compartidas de `c` por el contenido de `estado`.
pub fn importar_estado(c: &Connection, estado: &EstadoSnapshot) -> Resultado<()> {
    c.execute_batch(
        "DELETE FROM ediciones_pendientes;
         DELETE FROM reportes_sprint;
         DELETE FROM commits;
         DELETE FROM cambios_prestados;
         DELETE FROM notificaciones;
         DELETE FROM documentos;
         DELETE FROM grupos_chat;
         DELETE FROM mensajes_chat;
         DELETE FROM tareas;
         DELETE FROM sprints;
         DELETE FROM configuracion_proyecto;
         DELETE FROM usuarios;",
    )?;
    for u in &estado.usuarios {
        usuarios::insertar(c, u)?;
    }
    for a in &estado.configuracion_proyecto {
        configuracion::guardar(
            c,
            &a.clave,
            &a.valor,
            &a.actualizado_por,
            &a.fecha_actualizacion,
        )?;
    }
    for s in &estado.sprints {
        let mut limpio = s.clone();
        limpio.reporte = None;
        sprints::insertar(c, &limpio)?;
    }
    for r in &estado.reportes_sprint {
        sprints::guardar_reporte(c, r)?;
    }
    for t in &estado.tareas {
        tareas::insertar(c, t)?;
        // La tarea roja (#225) viaja dentro de la tarea; su fila va aparte.
        if let Some(ref cd) = t.cambios_directos {
            tareas::guardar_cambios_directos(c, &t.id, cd)?;
        }
    }
    for e in &estado.ediciones_pendientes {
        ediciones::guardar_o_sumar(c, e)?;
    }
    for cm in &estado.commits {
        commits::guardar(c, cm)?;
    }
    for p in &estado.cambios_prestados {
        commits::insertar_prestamo(c, p)?;
    }
    for n in &estado.notificaciones {
        let mut limpia = n.clone();
        limpia.leida = false;
        notificaciones::insertar(c, &limpia)?;
    }
    for d in &estado.documentos {
        documentos::insertar(c, &Documento::from(d.clone()))?;
    }
    for g in &estado.grupos_chat {
        chat::crear_grupo(c, g)?;
    }
    for m in &estado.mensajes_chat {
        let mut limpio = m.clone();
        limpio.leido_por.clear();
        chat::guardar_mensaje(c, &limpio)?;
    }
    Ok(())
}

fn listar_todas_notificaciones(c: &Connection) -> Resultado<Vec<Notificacion>> {
    let mut consulta = c.prepare(
        "SELECT id, tipo, para, titulo, cuerpo, subtitle_placeholder, fecha, tarea_id, accionable
         FROM (
             SELECT id, tipo, para, titulo, cuerpo, 0 AS subtitle_placeholder, fecha, tarea_id, accionable, rowid
             FROM notificaciones
         )
         ORDER BY rowid",
    )?;
    let filas = consulta.query_map([], |f| {
        Ok(Notificacion {
            id: f.get(0)?,
            tipo: f.get(1)?,
            para: f.get(2)?,
            titulo: f.get(3)?,
            cuerpo: f.get(4)?,
            leida: false,
            fecha: f.get(6)?,
            tarea_id: f.get(7)?,
            accionable: f.get(8)?,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

fn listar_todos_grupos_chat(c: &Connection) -> Resultado<Vec<GrupoChat>> {
    let mut consulta = c.prepare(
        "SELECT id, nombre, miembros, creado_por, fecha FROM grupos_chat ORDER BY rowid",
    )?;
    let filas = consulta.query_map([], |f| {
        let texto: String = f.get(2)?;
        let miembros: Vec<String> = serde_json::from_str(&texto).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?;
        Ok(GrupoChat {
            id: f.get(0)?,
            nombre: f.get(1)?,
            miembros,
            creado_por: f.get(3)?,
            fecha: f.get(4)?,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

fn listar_todos_mensajes_chat(c: &Connection) -> Resultado<Vec<Mensaje>> {
    let mut consulta =
        c.prepare("SELECT id, de, para, texto, fecha FROM mensajes_chat ORDER BY rowid")?;
    let filas = consulta.query_map([], |f| {
        Ok(Mensaje {
            id: f.get(0)?,
            de: f.get(1)?,
            para: f.get(2)?,
            texto: f.get(3)?,
            fecha: f.get(4)?,
            leido_por: Vec::new(),
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<_>>()?)
}

/// Instante de corte (`hasta`) del último snapshot ya aplicado en esta base, si hay uno.
pub fn ultimo_snapshot_hasta(c: &Connection) -> Resultado<Option<DateTime<Utc>>> {
    let texto: Option<String> = c
        .query_row(
            "SELECT valor FROM meta WHERE clave = ?1",
            [META_ULTIMO_SNAPSHOT_HASTA],
            |f| f.get(0),
        )
        .optional()?;
    Ok(texto
        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .map(|d| d.with_timezone(&Utc)))
}

fn guardar_ultimo_snapshot_hasta(c: &Connection, hasta: &str) -> Resultado<()> {
    c.execute(
        "INSERT INTO meta (clave, valor) VALUES (?1, ?2)
         ON CONFLICT (clave) DO UPDATE SET valor = excluded.valor",
        [META_ULTIMO_SNAPSHOT_HASTA, hasta],
    )?;
    Ok(())
}

/// `true` si esta base ya tiene un snapshot más reciente o ya había aplicado
/// todos los eventos que entraron en `snapshot`.
fn ya_cubierto(c: &Connection, snapshot: &Snapshot, hasta: DateTime<Utc>) -> Resultado<bool> {
    if let Some(previo) = ultimo_snapshot_hasta(c)? {
        if previo >= hasta {
            return Ok(true);
        }
    }
    if snapshot.incluidos.is_empty() {
        return Ok(false);
    }
    for id in &snapshot.incluidos {
        if !ya_aplicado(c, id)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Aplica un evento `"compactar_eventos"` (`ops/snapshot_<fecha>.json`) cuya
/// firma del Administrador ya fue verificada por `lectura::aplicar_evento`.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    let snapshot: Snapshot = serde_json::from_value(evento.payload.clone())
        .map_err(|_| Error::Validacion("El snapshot no trae los datos esperados".into()))?;
    let hasta = DateTime::parse_from_rfc3339(&snapshot.hasta)
        .map_err(|_| {
            Error::Validacion(format!(
                "Fecha de corte de snapshot no válida: {}",
                snapshot.hasta
            ))
        })?
        .with_timezone(&Utc);

    if !ya_cubierto(c, &snapshot, hasta)? {
        importar_estado(c, &snapshot.estado)?;
        for id in &snapshot.incluidos {
            marcar_aplicado(c, id, &snapshot.hasta)?;
        }
    }
    guardar_ultimo_snapshot_hasta(c, &snapshot.hasta)?;
    Ok(())
}

/// Compacta en `ops/snapshot_<fecha>.json` todos los eventos de `ops/` con más
/// de 30 días de antigüedad respecto a `ahora` y los borra de `ops/`.
/// Solo el Administrador puede ejecutarla (`firmante.rol == Rol::Administrador`).
pub fn compactar(
    c: &Connection,
    ops: &Path,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    ahora: DateTime<Utc>,
) -> Resultado<ResumenCompactacion> {
    if firmante.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }
    if firmante.llave.publica() != publica_admin {
        return Err(Error::Validacion(
            "Tu llave de firma no es la del Administrador de este proyecto".into(),
        ));
    }
    let entradas = match std::fs::read_dir(ops) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ResumenCompactacion::default())
        }
        Err(e) => return Err(e.into()),
    };

    let hasta = ahora - Duration::days(DIAS_PARA_COMPACTAR);
    let hasta_iso = hasta.to_rfc3339_opts(SecondsFormat::Millis, true);
    let fecha = ahora.format("%Y-%m-%d").to_string();

    let mut snapshots_previos: Vec<(DateTime<Utc>, String, Snapshot)> = Vec::new();
    let mut candidatos: Vec<(DateTime<Utc>, String, Evento)> = Vec::new();

    for entrada in entradas {
        let entrada = entrada?;
        let id = entrada.file_name().to_string_lossy().to_string();
        if id.starts_with('.') || !id.ends_with(".json") {
            continue;
        }
        let Some(evento) = std::fs::read_to_string(entrada.path())
            .ok()
            .and_then(|t| serde_json::from_str::<Evento>(&t).ok())
        else {
            continue;
        };
        if es_snapshot(&id) {
            if evento.tipo == "compactar_eventos"
                && equipo::firmante_del_evento(c, publica_admin, &evento)?
                    == Some(Rol::Administrador)
            {
                if let Ok(snap) = serde_json::from_value::<Snapshot>(evento.payload) {
                    if let Ok(corte) = DateTime::parse_from_rfc3339(&snap.hasta) {
                        snapshots_previos.push((corte.with_timezone(&Utc), id, snap));
                    }
                }
            }
            continue;
        }
        let Ok(declarado) = DateTime::parse_from_rfc3339(&evento.timestamp) else {
            continue;
        };
        let declarado = declarado.with_timezone(&Utc);
        if declarado <= hasta {
            candidatos.push((declarado, id, evento));
        }
    }

    if candidatos.is_empty() {
        return Ok(ResumenCompactacion::default());
    }

    snapshots_previos.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let ultimo_previo = snapshots_previos.last();
    let corte_previo = ultimo_previo.map(|(corte, _, _)| *corte);

    candidatos.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.2.usuario.cmp(&b.2.usuario))
            .then_with(|| a.1.cmp(&b.1))
    });

    // Base en memoria para calcular el estado exacto hasta `hasta` (sin mezclar
    // eventos de los últimos 30 días que siguen vivos en `ops/`).
    let base = Db::en_memoria()?;
    let mut incluidos: Vec<String> = Vec::new();
    if let Some((_, id_prev, snap_prev)) = ultimo_previo {
        importar_estado(base.conexion(), &snap_prev.estado)?;
        incluidos.extend(snap_prev.incluidos.iter().cloned());
        if !incluidos.contains(id_prev) {
            incluidos.push(id_prev.clone());
        }
    }
    sembrar_base_inicial(c, base.conexion())?;

    let mut a_borrar: Vec<String> = Vec::new();
    for (declarado, id, evento) in candidatos {
        if incluidos.contains(&id) || corte_previo.is_some_and(|prev| declarado <= prev) {
            a_borrar.push(id);
            continue;
        }
        let tx = base.conexion().unchecked_transaction()?;
        match lectura::aplicar_evento(&tx, publica_admin, &evento)? {
            Aplicacion::Aplicado => {
                tx.commit()?;
                a_borrar.push(id.clone());
                incluidos.push(id);
            }
            Aplicacion::Descartado(_) => {
                drop(tx);
                a_borrar.push(id.clone());
                incluidos.push(id);
            }
            Aplicacion::Desconocido => {
                // Escrito por una versión más nueva: no se compacta ni se borra.
                drop(tx);
            }
        }
    }

    if a_borrar.is_empty() {
        return Ok(ResumenCompactacion::default());
    }

    let estado = exportar_estado(base.conexion())?;
    let snapshot = Snapshot {
        fecha: fecha.clone(),
        hasta: hasta_iso.clone(),
        incluidos,
        estado,
    };
    let payload = serde_json::to_value(&snapshot).map_err(|e| Error::Interno(e.to_string()))?;
    let mut evento_snapshot = Evento::nuevo("compactar_eventos", None, firmante.email, payload);
    evento_snapshot.timestamp = hasta_iso.clone();
    let evento_snapshot = evento_snapshot.firmar(firmante.llave);

    let archivo = nombre_snapshot(&fecha);
    escribir_snapshot(ops, &archivo, &evento_snapshot)?;

    let ahora_texto = ahora.to_rfc3339_opts(SecondsFormat::Millis, true);
    let tx = c.unchecked_transaction()?;
    marcar_aplicado(&tx, &archivo, &ahora_texto)?;
    for id in &a_borrar {
        marcar_aplicado(&tx, id, &ahora_texto)?;
    }
    guardar_ultimo_snapshot_hasta(&tx, &hasta_iso)?;
    tx.commit()?;

    for id in &a_borrar {
        let _ = std::fs::remove_file(ops.join(id));
    }
    for (_, id_prev, _) in &snapshots_previos {
        if id_prev != &archivo {
            let _ = std::fs::remove_file(ops.join(id_prev));
        }
    }

    Ok(ResumenCompactacion {
        compactados: a_borrar.len(),
        archivo: Some(archivo),
    })
}

/// Si la base temporal donde se calcula el snapshot todavía no tiene la fila
/// semilla del Administrador ni el nombre del proyecto (que se escriben al
/// configurar el proyecto antes del primer evento), los copia de `origen`.
fn sembrar_base_inicial(origen: &Connection, destino: &Connection) -> Resultado<()> {
    for u in usuarios::listar(origen)? {
        if u.rol == Rol::Administrador && usuarios::buscar(destino, &u.email)?.is_none() {
            usuarios::insertar(destino, &u)?;
        }
    }
    if configuracion::listar(destino)?.is_empty() {
        for a in configuracion::listar(origen)? {
            configuracion::guardar(
                destino,
                &a.clave,
                &a.valor,
                &a.actualizado_por,
                &a.fecha_actualizacion,
            )?;
        }
    }
    Ok(())
}

/// Escribe `ops/snapshot_<fecha>.json` pasando por un temporal `.snapshot_<fecha>.json.tmp`.
fn escribir_snapshot(ops: &Path, nombre: &str, evento: &Evento) -> Resultado<()> {
    std::fs::create_dir_all(ops)?;
    let destino = ops.join(nombre);
    let json = serde_json::to_string_pretty(evento).map_err(|e| Error::Interno(e.to_string()))?;
    let temporal = ops.join(format!(".{nombre}.tmp"));
    let _ = std::fs::remove_file(&temporal);
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
    std::fs::rename(&temporal, &destino).map_err(|e| {
        let _ = std::fs::remove_file(&temporal);
        Error::from(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventos::lectura::ARCHIVO_LOG;
    use crate::eventos::{escribir_evento, sincronizar, ResumenSincronizacion};
    use crate::identidad::firmas::{self, Verificacion};
    use crate::identidad::llaves::{self, LlavePrivada};
    use crate::modelo::{EstadoTarea, TipoNotificacion, TipoTarea};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    const ADMIN: &str = "admin@virustf.dev";
    const LIDER: &str = "lider@virustf.dev";
    const SEMILLA: &str = "2026-09-01T08:00:00Z";
    const HOY: &str = "2026-11-15";

    fn momento(texto: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(texto)
            .unwrap()
            .with_timezone(&Utc)
    }

    struct Escenario {
        _dirs: (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir),
        db: Db,
        ops: PathBuf,
        local: PathBuf,
        publica: String,
        admin: LlavePrivada,
        lider: LlavePrivada,
    }

    impl Escenario {
        fn nuevo() -> Self {
            let dir_admin = tempfile::tempdir().unwrap();
            let dir_lider = tempfile::tempdir().unwrap();
            let dir_nube = tempfile::tempdir().unwrap();
            let publica = llaves::generar_llaves_admin(dir_admin.path(), None).unwrap();
            let admin = llaves::cargar_llave_admin(dir_admin.path(), None).unwrap();
            let publica_lider = llaves::generar_llaves_lider(dir_lider.path(), None).unwrap();
            let lider = llaves::cargar_llave_lider(dir_lider.path(), None).unwrap();

            let db = Db::en_memoria().unwrap();
            let fila_admin = Usuario {
                email: ADMIN.into(),
                nombre: "Admin".into(),
                apellido: "Uno".into(),
                rol: Rol::Administrador,
                fecha_alta: "2026-09-01".into(),
                firma: Some(
                    firmas::firmar_rol(&admin, ADMIN, Rol::Administrador, SEMILLA).unwrap(),
                ),
                firmado_en: Some(SEMILLA.into()),
                firmado_por: Some(ADMIN.into()),
                llave_publica: None,
                certificado: None,
            };
            usuarios::insertar(db.conexion(), &fila_admin).unwrap();
            configuracion::guardar(
                db.conexion(),
                configuracion::CLAVE_NOMBRE_PROYECTO,
                "VirusTF",
                ADMIN,
                "2026-09-01",
            )
            .unwrap();
            let _ = publica_lider;

            let ops = dir_nube.path().join("ops");
            let local = dir_nube.path().join("local");
            Self {
                _dirs: (dir_admin, dir_lider, dir_nube),
                db,
                ops,
                local,
                publica,
                admin,
                lider,
            }
        }

        fn como_admin(&self) -> Firmante<'_> {
            Firmante {
                email: ADMIN,
                rol: Rol::Administrador,
                llave: &self.admin,
            }
        }

        /// Publica un alta de usuario fechada en `cuando` (para simular eventos de hace >30 días).
        fn alta_en(&self, email: &str, rol: Rol, cuando: &str) {
            let (nombre, apellido) = usuarios::nombre_desde_email(email);
            let nuevo = Usuario {
                email: email.into(),
                nombre,
                apellido,
                rol,
                fecha_alta: cuando[..10].to_string(),
                firma: Some(firmas::firmar_rol(&self.admin, email, rol, cuando).unwrap()),
                firmado_en: Some(cuando.into()),
                firmado_por: Some(ADMIN.into()),
                llave_publica: None,
                certificado: None,
            };
            let mut ev = Evento::nuevo(
                "crear_usuario",
                None,
                ADMIN,
                serde_json::to_value(&nuevo).unwrap(),
            );
            ev.timestamp = format!("{}.000Z", &cuando[..19]);
            let ev = ev.firmar(&self.admin);
            crate::eventos::publicar(self.db.conexion(), &self.ops, &ev, |tx| {
                equipo::aplicar_evento(tx, &self.publica, Rol::Administrador, &ev)
            })
            .unwrap();
        }

        /// Publica un cambio de rol fechado en `cuando`.
        fn cambiar_rol_en(&self, email: &str, rol: Rol, cuando: &str) {
            let fila = equipo::FilaFirmada {
                email: email.into(),
                rol,
                firma: firmas::firmar_rol(&self.admin, email, rol, cuando).unwrap(),
                firmado_en: cuando.into(),
                firmado_por: ADMIN.into(),
            };
            let mut ev = Evento::nuevo(
                "cambiar_rol",
                None,
                ADMIN,
                serde_json::to_value(&fila).unwrap(),
            );
            ev.timestamp = format!("{}.000Z", &cuando[..19]);
            let ev = ev.firmar(&self.admin);
            crate::eventos::publicar(self.db.conexion(), &self.ops, &ev, |tx| {
                equipo::aplicar_evento(tx, &self.publica, Rol::Administrador, &ev)
            })
            .unwrap();
        }

        fn archivos_ops(&self) -> Vec<String> {
            let mut v: Vec<String> = match std::fs::read_dir(&self.ops) {
                Ok(d) => d
                    .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                    .collect(),
                Err(_) => Vec::new(),
            };
            v.sort();
            v
        }

        fn sincronizar_en(&self, db: &Db, ahora: DateTime<Utc>) -> ResumenSincronizacion {
            sincronizar(db.conexion(), &self.ops, &self.local, ahora, |c, e| {
                lectura::aplicar_evento(c, &self.publica, e)
            })
            .unwrap()
        }
    }

    #[test]
    fn solo_el_administrador_puede_compactar() {
        let e = Escenario::nuevo();
        e.alta_en("ana@virustf.dev", Rol::Tester, "2026-09-10T10:00:00Z");
        let ahora = momento("2026-11-15T12:00:00Z");

        for rol in [
            Rol::Lider,
            Rol::Desarrollador,
            Rol::Tester,
            Rol::ColaboradorExterno,
        ] {
            let intento = Firmante {
                email: LIDER,
                rol,
                llave: &e.lider,
            };
            let err = compactar(e.db.conexion(), &e.ops, &e.publica, &intento, ahora).unwrap_err();
            assert_eq!(err.codigo(), "sin_permiso");
        }
        // Y aunque declare ser Administrador, si la llave no es la del proyecto se rechaza.
        let impostor = Firmante {
            email: ADMIN,
            rol: Rol::Administrador,
            llave: &e.lider,
        };
        assert_eq!(
            compactar(e.db.conexion(), &e.ops, &e.publica, &impostor, ahora)
                .unwrap_err()
                .codigo(),
            "validacion"
        );
    }

    #[test]
    fn junta_los_eventos_de_mas_de_30_dias_en_snapshot_y_los_borra() {
        let e = Escenario::nuevo();
        // Hace 60 y 40 días (> 30 días respecto al 2026-11-15): entran al snapshot y se borran.
        e.alta_en("ana@virustf.dev", Rol::Tester, "2026-09-15T10:00:00Z");
        e.cambiar_rol_en(
            "ana@virustf.dev",
            Rol::Desarrollador,
            "2026-10-05T10:00:00Z",
        );
        // Hace 10 días (< 30 días): se queda en `ops/` sin tocar.
        e.alta_en("beto@virustf.dev", Rol::Tester, "2026-11-05T10:00:00Z");
        assert_eq!(e.archivos_ops().len(), 3);

        let ahora = momento("2026-11-15T12:00:00Z");
        let r = compactar(e.db.conexion(), &e.ops, &e.publica, &e.como_admin(), ahora).unwrap();
        assert_eq!(
            r,
            ResumenCompactacion {
                compactados: 2,
                archivo: Some("snapshot_2026-11-15.json".into()),
            }
        );

        // En `ops/` quedan únicamente el evento reciente y `snapshot_2026-11-15.json`.
        let en_ops = e.archivos_ops();
        assert_eq!(en_ops.len(), 2, "{en_ops:?}");
        assert!(en_ops.contains(&"snapshot_2026-11-15.json".to_string()));
        assert!(en_ops.iter().any(|n| n.starts_with("20261105T")));

        // Si se vuelve a correr enseguida, no hay más eventos de >30 días: no hace nada.
        let r2 = compactar(e.db.conexion(), &e.ops, &e.publica, &e.como_admin(), ahora).unwrap();
        assert_eq!(r2, ResumenCompactacion::default());

        // Al reconstruir el `.db` desde cero se parte del snapshot y se aplica el reciente.
        let reconstruida = Db::en_memoria().unwrap();
        let sync = e.sincronizar_en(&reconstruida, ahora);
        assert_eq!(
            (sync.aplicados, sync.descartados, sync.desconocidos),
            (2, 0, 0)
        );
        assert_eq!(
            usuarios::listar(reconstruida.conexion()).unwrap(),
            usuarios::listar(e.db.conexion()).unwrap()
        );
        let verificados =
            equipo::listar(reconstruida.conexion(), &e.publica, Rol::Administrador).unwrap();
        assert!(verificados
            .iter()
            .all(|u| u.verificacion == Verificacion::Verificado));
    }

    #[test]
    fn al_reconstruir_el_db_se_parte_del_ultimo_snapshot() {
        let e = Escenario::nuevo();
        // Primera tanda (septiembre) → primer snapshot el 2026-10-20.
        e.alta_en("ana@virustf.dev", Rol::Tester, "2026-09-10T10:00:00Z");
        let r1 = compactar(
            e.db.conexion(),
            &e.ops,
            &e.publica,
            &e.como_admin(),
            momento("2026-10-20T12:00:00Z"),
        )
        .unwrap();
        assert_eq!(r1.archivo.as_deref(), Some("snapshot_2026-10-20.json"));
        let primer_snapshot =
            std::fs::read_to_string(e.ops.join("snapshot_2026-10-20.json")).unwrap();

        // Segunda tanda (octubre, más un evento de noviembre) → segundo snapshot el 2026-11-15.
        e.cambiar_rol_en("ana@virustf.dev", Rol::Lider, "2026-10-10T10:00:00Z");
        e.alta_en(
            "beto@virustf.dev",
            Rol::Desarrollador,
            "2026-11-10T10:00:00Z",
        );
        let r2 = compactar(
            e.db.conexion(),
            &e.ops,
            &e.publica,
            &e.como_admin(),
            momento("2026-11-15T12:00:00Z"),
        )
        .unwrap();
        assert_eq!(r2.archivo.as_deref(), Some("snapshot_2026-11-15.json"));
        assert!(
            !e.ops.join("snapshot_2026-10-20.json").exists(),
            "borró el snapshot viejo"
        );

        // Incluso si el snapshot viejo sigue en `ops/` (ej. OneDrive aún no lo borra),
        // al reconstruir el `.db` se parte del ÚLTIMO (`snapshot_2026-11-15.json`) y
        // el viejo no pisa el estado más nuevo.
        std::fs::write(e.ops.join("snapshot_2026-10-20.json"), primer_snapshot).unwrap();

        let reconstruida = Db::en_memoria().unwrap();
        let sync = e.sincronizar_en(&reconstruida, momento("2026-11-15T12:00:00Z"));
        assert_eq!(sync.descartados, 0);
        assert_eq!(
            usuarios::listar(reconstruida.conexion()).unwrap(),
            usuarios::listar(e.db.conexion()).unwrap()
        );
        let ana = usuarios::buscar(reconstruida.conexion(), "ana@virustf.dev")
            .unwrap()
            .unwrap();
        assert_eq!(
            ana.rol,
            Rol::Lider,
            "quedó el rol del último snapshot, no el del primero"
        );
    }

    #[test]
    fn un_snapshot_falso_se_descarta_y_no_pisa_el_legitimo() {
        let e = Escenario::nuevo();
        e.alta_en("ana@virustf.dev", Rol::Tester, "2026-09-15T10:00:00Z");
        let ahora = momento("2026-11-15T12:00:00Z");
        compactar(e.db.conexion(), &e.ops, &e.publica, &e.como_admin(), ahora).unwrap();

        // Alguien deja en `ops/` dos snapshots falsos con fecha más nueva: uno sin
        // firma y otro firmado con la llave del Líder.
        let mut falso_sin_firma = Evento::nuevo(
            "compactar_eventos",
            None,
            ADMIN,
            json!({
                "fecha": "2026-12-01",
                "hasta": "2026-11-01T00:00:00.000Z",
                "incluidos": [],
                "estado": EstadoSnapshot::default(),
            }),
        );
        falso_sin_firma.timestamp = "2026-11-01T00:00:00.000Z".into();
        escribir_snapshot(&e.ops, "snapshot_2026-12-01.json", &falso_sin_firma).unwrap();

        let falso_lider = falso_sin_firma.clone().firmar(&e.lider);
        escribir_snapshot(&e.ops, "snapshot_2026-12-02.json", &falso_lider).unwrap();

        let reconstruida = Db::en_memoria().unwrap();
        let sync = e.sincronizar_en(&reconstruida, ahora);
        assert_eq!((sync.aplicados, sync.descartados), (1, 2));
        assert_eq!(
            usuarios::listar(reconstruida.conexion()).unwrap(),
            usuarios::listar(e.db.conexion()).unwrap(),
            "se partió del snapshot legítimo y se descartaron los dos falsos"
        );
        let log = std::fs::read_to_string(e.local.join(ARCHIVO_LOG)).unwrap();
        assert!(log.contains("snapshot_2026-12-01.json"), "{log}");
        assert!(log.contains("snapshot_2026-12-02.json"), "{log}");
    }

    #[test]
    fn exportar_e_importar_conserva_todas_las_tablas_compartidas() {
        let origen = Db::en_memoria().unwrap();
        let c = origen.conexion();
        usuarios::insertar(
            c,
            &Usuario {
                email: ADMIN.into(),
                nombre: "Admin".into(),
                apellido: "Uno".into(),
                rol: Rol::Administrador,
                fecha_alta: HOY.into(),
                firma: Some("f".into()),
                firmado_en: Some(SEMILLA.into()),
                firmado_por: Some(ADMIN.into()),
                llave_publica: None,
                certificado: None,
            },
        )
        .unwrap();
        configuracion::guardar(c, "nombre_proyecto", "Juego", ADMIN, HOY).unwrap();
        let mut sp = Sprint {
            id: "S-1".into(),
            nombre: "Sprint 1".into(),
            descripcion: "Desc".into(),
            con_fecha: true,
            fecha_inicio: "2026-10-01".into(),
            duracion_dias: Some(14),
            fecha_fin: Some("2026-10-15".into()),
            cerrado: false,
            fecha_cierre_real: None,
            creado_por: ADMIN.into(),
            reporte: None,
        };
        sprints::insertar(c, &sp).unwrap();
        let mut tarea = Tarea::nueva("T-101", "Combate", TipoTarea::Scripting, ADMIN, HOY);
        tarea.sprint_id = Some("S-1".into());
        tarea.estado = EstadoTarea::EnProduccion;
        tareas::insertar(c, &tarea).unwrap();
        sprints::guardar_reporte(
            c,
            &ReporteSprint {
                sprint_id: "S-1".into(),
                fecha: "2026-10-15".into(),
                de_cierre: false,
                tareas: vec![tarea.clone()],
            },
        )
        .unwrap();
        let mut nuevos = BTreeMap::new();
        nuevos.insert("titulo".into(), "Combate 2".into());
        let mut anteriores = BTreeMap::new();
        anteriores.insert("titulo".into(), "Combate".into());
        ediciones::guardar_o_sumar(
            c,
            &EdicionPendiente {
                id: "E-1".into(),
                tarea_id: "T-101".into(),
                campos_nuevos: nuevos,
                campos_anteriores: anteriores,
                motivo: Some("Ajuste".into()),
                creado_por: ADMIN.into(),
                fecha: HOY.into(),
            },
        )
        .unwrap();
        commits::guardar(
            c,
            &Commit {
                id: "C-1".into(),
                tarea_id: "T-101".into(),
                hash: "42".into(),
                mensaje: " Avance".into(),
                descripcion: None,
                tipo: Some(TipoTarea::Scripting),
                autor: ADMIN.into(),
                fecha: SEMILLA.into(),
            },
        )
        .unwrap();
        commits::insertar_prestamo(
            c,
            &CambioPrestado {
                id: "P-1".into(),
                tarea_id: "T-101".into(),
                origen_id: "T-102".into(),
                hash: Some("10".into()),
                autor: ADMIN.into(),
                fecha: HOY.into(),
                archivos: vec!["Assets/A.cs".into()],
                archivos_tocados: Vec::new(),
                quitado_en: None,
                quitado_por: None,
                changeset_merge: None,
                changeset_limpio: None,
            },
        )
        .unwrap();
        notificaciones::insertar(
            c,
            &Notificacion {
                id: "N-1".into(),
                tipo: TipoNotificacion::Mensaje,
                para: ADMIN.into(),
                titulo: "Hola".into(),
                cuerpo: "Texto".into(),
                leida: true,
                fecha: SEMILLA.into(),
                tarea_id: Some("T-101".into()),
                accionable: false,
            },
        )
        .unwrap();
        documentos::insertar(
            c,
            &Documento {
                id: "D-1".into(),
                nombre: "GDD.pdf".into(),
                tamano: 1024,
                subido_por: ADMIN.into(),
                fecha: HOY.into(),
                pendiente: false,
                ruta: "D-1.pdf".into(),
            },
        )
        .unwrap();
        chat::crear_grupo(
            c,
            &GrupoChat {
                id: "G-1".into(),
                nombre: "Arte".into(),
                miembros: vec![ADMIN.into()],
                creado_por: ADMIN.into(),
                fecha: HOY.into(),
            },
        )
        .unwrap();
        chat::guardar_mensaje(
            c,
            &Mensaje {
                id: "M-1".into(),
                de: ADMIN.into(),
                para: "grupo".into(),
                texto: "Listo".into(),
                fecha: SEMILLA.into(),
                leido_por: vec![ADMIN.into()],
            },
        )
        .unwrap();

        let estado = exportar_estado(c).unwrap();
        let destino = Db::en_memoria().unwrap();
        importar_estado(destino.conexion(), &estado).unwrap();
        assert_eq!(exportar_estado(destino.conexion()).unwrap(), estado);
        assert_eq!(
            documentos::obtener(destino.conexion(), "D-1").unwrap().ruta,
            "D-1.pdf",
            "la ruta interna del documento sobrevive al snapshot"
        );
        sp.reporte = Some(vec![tarea]);
        assert_eq!(sprints::listar(destino.conexion()).unwrap(), vec![sp]);
    }

    /// **Prueba en el entorno real** (CLAUDE.md): sobre la carpeta de OneDrive/Drive
    /// de verdad, A (Administrador) publica eventos de más de 30 días y un evento
    /// reciente, ejecuta `compactar`, comprueba que en OneDrive quedaron solo
    /// `snapshot_<fecha>.json` y el evento reciente (los antiguos se borraron), y
    /// una segunda máquina B reconstruye su `.db` desde cero partiendo del
    /// snapshot más el evento reciente y llega al mismo estado que A. También
    /// comprueba que un snapshot falso sin firma se descarta. Borra la subcarpeta
    /// al terminar.
    ///
    /// ```text
    /// BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive cargo test --lib entorno_real_compactar -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa la carpeta de OneDrive/Drive de verdad: correr a mano con BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_compactar_eventos_y_reconstruir() {
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida};

        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let compartida = nube.join(format!(
            "BranchBoard-prueba-94-{}",
            chrono::Local::now().format("%H%M%S")
        ));
        std::fs::create_dir_all(&compartida).unwrap();
        let maquina_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let local_a = maquina_a.path().join("proyectos").join("prueba-94");
        let local_b = maquina_b.path().join("proyectos").join("prueba-94");

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let publica = llaves::generar_llaves_admin(&local_a, None).unwrap();
            let admin = llaves::cargar_llave_admin(&local_a, None).unwrap();
            let db_a = Db::en_memoria().unwrap();
            let fila_admin = Usuario {
                email: ADMIN.into(),
                nombre: "Admin".into(),
                apellido: "Uno".into(),
                rol: Rol::Administrador,
                fecha_alta: "2026-09-01".into(),
                firma: Some(
                    firmas::firmar_rol(&admin, ADMIN, Rol::Administrador, SEMILLA).unwrap(),
                ),
                firmado_en: Some(SEMILLA.into()),
                firmado_por: Some(ADMIN.into()),
                llave_publica: None,
                certificado: None,
            };
            usuarios::insertar(db_a.conexion(), &fila_admin).unwrap();

            let ahora = Utc::now();
            let hace_45_dias =
                (ahora - Duration::days(45)).to_rfc3339_opts(SecondsFormat::Secs, true);
            let hace_35_dias =
                (ahora - Duration::days(35)).to_rfc3339_opts(SecondsFormat::Secs, true);
            let hace_5_dias =
                (ahora - Duration::days(5)).to_rfc3339_opts(SecondsFormat::Secs, true);

            // 2 eventos de > 30 días y 1 de hace 5 días.
            for (email, rol, cuando) in [
                ("ana@virustf.dev", Rol::Tester, hace_45_dias.as_str()),
                (
                    "beto@virustf.dev",
                    Rol::Desarrollador,
                    hace_35_dias.as_str(),
                ),
                ("caro@virustf.dev", Rol::Tester, hace_5_dias.as_str()),
            ] {
                let (nombre, apellido) = usuarios::nombre_desde_email(email);
                let nuevo = Usuario {
                    email: email.into(),
                    nombre,
                    apellido,
                    rol,
                    fecha_alta: cuando[..10].to_string(),
                    firma: Some(firmas::firmar_rol(&admin, email, rol, cuando).unwrap()),
                    firmado_en: Some(cuando.into()),
                    firmado_por: Some(ADMIN.into()),
                    llave_publica: None,
                    certificado: None,
                };
                let mut ev = Evento::nuevo(
                    "crear_usuario",
                    None,
                    ADMIN,
                    serde_json::to_value(&nuevo).unwrap(),
                );
                ev.timestamp = format!("{}.000Z", &cuando[..19]);
                let ev = ev.firmar(&admin);
                super::super::publicar(db_a.conexion(), &ops_a, &ev, |tx| {
                    equipo::aplicar_evento(tx, &publica, Rol::Administrador, &ev)
                })
                .unwrap();
            }

            let como_admin = Firmante {
                email: ADMIN,
                rol: Rol::Administrador,
                llave: &admin,
            };
            let resumen = compactar(db_a.conexion(), &ops_a, &publica, &como_admin, ahora).unwrap();
            println!("compactación en OneDrive: {resumen:?}");
            assert_eq!(resumen.compactados, 2);
            let nombre_snap = resumen.archivo.unwrap();

            // Un snapshot falso sin firma dejado en `ops/` para comprobar que se descarta.
            let mut falso = Evento::nuevo(
                "compactar_eventos",
                None,
                ADMIN,
                json!({
                    "fecha": "2099-01-01",
                    "hasta": "2099-01-01T00:00:00.000Z",
                    "incluidos": [],
                    "estado": EstadoSnapshot::default(),
                }),
            );
            falso.timestamp = "2099-01-01T00:00:00.000Z".into();
            let _ = escribir_evento(&ops_b, &falso).unwrap();

            std::thread::sleep(std::time::Duration::from_secs(5));

            // B reconstruye su `.db` desde cero partiendo del snapshot y aplicando el evento de hace 5 días.
            let db_b = Db::en_memoria().unwrap();
            let sync = sincronizar(db_b.conexion(), &ops_b, &local_b, Utc::now(), |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            println!("B reconstruye desde snapshot + evento reciente: {sync:?}");
            assert_eq!((sync.aplicados, sync.descartados), (2, 1));
            assert!(ops_b.join(&nombre_snap).is_file());
            assert_eq!(
                usuarios::listar(db_b.conexion()).unwrap(),
                usuarios::listar(db_a.conexion()).unwrap(),
                "la base reconstruida en B coincide con la de A"
            );

            // También ejercita `compactar_eventos_en` sobre un proyecto real entrado contra
            // `BranchBoard-Pruebas` + carpeta compartida en OneDrive (auditoría #94).
            if let (Ok(org), Ok(repo)) = (
                std::env::var("BB_PRUEBA_ORG"),
                std::env::var("BB_PRUEBA_REPO"),
            ) {
                let estado_real = crate::estado::AppState::default();
                let ws_real = tempfile::tempdir().unwrap();
                let datos_real = tempfile::tempdir().unwrap();
                let comp_real = nube.join(format!(
                    "BranchBoard-prueba-94-real-{}",
                    chrono::Local::now().format("%H%M%S")
                ));
                std::fs::create_dir_all(&comp_real).unwrap();
                let ruta_ws = ws_real.path().to_string_lossy().to_string();
                let spec_real = format!("{repo}@{org}@unity");
                crate::cm::Comando::cm()
                    .args(["workspace", "create"])
                    .arg(format!(
                        "bb-prueba-94-{}",
                        chrono::Local::now().format("%H%M%S")
                    ))
                    .arg(&ruta_ws)
                    .arg(format!("rep:{spec_real}"))
                    .exigir_exito(&estado_real)
                    .unwrap();

                let res_real = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::comandos::acceso::entrar_en(
                        &estado_real,
                        datos_real.path(),
                        ws_real.path(),
                    )
                    .unwrap();
                    let local_real = crate::comandos::datos::carpeta_abierta(&estado_real).unwrap();
                    configurar_carpeta_compartida(&local_real, &comp_real.to_string_lossy(), true)
                        .unwrap();
                    let ops_real = carpeta_ops(&local_real).unwrap();
                    let pub_real = estado_real
                        .con_db(|db| crate::estado::llave_publica_del_proyecto(db))
                        .unwrap();
                    let adm_real = llaves::cargar_llave_admin(&local_real, None).unwrap();
                    let admin_email = estado_real
                        .con_db(|db| usuarios::listar(db.conexion()))
                        .unwrap()
                        .into_iter()
                        .next()
                        .unwrap()
                        .email;
                    let hace_40_dias = (Utc::now() - Duration::days(40))
                        .to_rfc3339_opts(SecondsFormat::Secs, true);
                    let (nom, ape) = usuarios::nombre_desde_email("aud94@virustf.dev");
                    let u94 = Usuario {
                        email: "aud94@virustf.dev".into(),
                        nombre: nom,
                        apellido: ape,
                        rol: Rol::Tester,
                        fecha_alta: hace_40_dias[..10].to_string(),
                        firma: Some(
                            firmas::firmar_rol(
                                &adm_real,
                                "aud94@virustf.dev",
                                Rol::Tester,
                                &hace_40_dias,
                            )
                            .unwrap(),
                        ),
                        firmado_en: Some(hace_40_dias.clone()),
                        firmado_por: Some(admin_email.clone()),
                        llave_publica: None,
                        certificado: None,
                    };
                    let mut ev94 = Evento::nuevo(
                        "crear_usuario",
                        None,
                        &admin_email,
                        serde_json::to_value(&u94).unwrap(),
                    );
                    ev94.timestamp = format!("{}.000Z", &hace_40_dias[..19]);
                    let ev94 = ev94.firmar(&adm_real);
                    estado_real
                        .con_db(|db| {
                            super::super::publicar(db.conexion(), &ops_real, &ev94, |tx| {
                                equipo::aplicar_evento(tx, &pub_real, Rol::Administrador, &ev94)
                            })
                        })
                        .unwrap();

                    let res_compactar =
                        crate::comandos::eventos::compactar_eventos_en(&estado_real, None).unwrap();
                    assert_eq!(res_compactar.compactados, 1);
                    assert!(res_compactar.archivo.is_some());
                }));
                estado_real.cerrar_proyecto();
                let _ = crate::cm::Comando::cm()
                    .args(["workspace", "delete", &ruta_ws])
                    .ejecutar(&estado_real);
                let _ = std::fs::remove_dir_all(&comp_real);
                if let Err(p) = res_real {
                    std::panic::resume_unwind(p);
                }
            }
        }));
        let borrada = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", borrada.is_ok());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
