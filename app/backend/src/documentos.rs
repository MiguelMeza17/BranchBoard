//! Documentos del proyecto en la carpeta compartida (no versionados en Unity VC, issue #120).
//!
//! - El backend genera siempre su propio nombre de archivo interno (`D-<uuid>[.ext]`)
//!   y valida de forma canónica que toda lectura, escritura o borrado permanezca
//!   estrictamente dentro de `<carpeta_compartida>/documentos/` (previene path traversal).
//! - Subido por Líder/Administrador -> publicado (`pendiente = false`); otro rol -> `pendiente = true`.
//! - `subir_documentos`, `aprobar_documento` y `eliminar_documento` viajan por `ops/`;
//!   `abrir_documento` resuelve y abre la copia en la carpeta compartida de forma local.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::documentos::{self as db_docs, Documento};
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::{normalizar_email, Rol};
use crate::tareas::rol_verificado_de;

/// Tipos de evento de documentos manejados desde `ops/`.
pub const TIPOS: &[&str] = &[
    "subir_documentos",
    "aprobar_documento",
    "eliminar_documento",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PayloadDocumentoEvento {
    id: String,
    nombre: String,
    tamano: u64,
    subido_por: String,
    fecha: String,
    pendiente: bool,
    ruta: String,
}

impl From<&Documento> for PayloadDocumentoEvento {
    fn from(d: &Documento) -> Self {
        Self {
            id: d.id.clone(),
            nombre: d.nombre.clone(),
            tamano: d.tamano,
            subido_por: d.subido_por.clone(),
            fecha: d.fecha.clone(),
            pendiente: d.pendiente,
            ruta: d.ruta.clone(),
        }
    }
}

impl From<PayloadDocumentoEvento> for Documento {
    fn from(p: PayloadDocumentoEvento) -> Self {
        Self {
            id: p.id,
            nombre: p.nombre,
            tamano: p.tamano,
            subido_por: p.subido_por,
            fecha: p.fecha,
            pendiente: p.pendiente,
            ruta: p.ruta,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PayloadSubirDocumentos {
    documentos: Vec<PayloadDocumentoEvento>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PayloadIdDocumento {
    id: String,
}

fn validar_ruta_interna(ruta: &str) -> Resultado<String> {
    let limpia = ruta.trim();
    if limpia.is_empty() || limpia.contains('/') || limpia.contains('\\') {
        return Err(Error::Validacion(
            "Ruta interna de documento no válida".into(),
        ));
    }
    let mut comps = Path::new(limpia).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(limpia.to_string()),
        _ => Err(Error::Validacion(
            "Ruta interna de documento fuera de la carpeta permitida".into(),
        )),
    }
}

fn extension_segura(origen: &Path) -> String {
    let raw = origen
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let limpia: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(10)
        .collect();
    limpia.to_ascii_lowercase()
}

fn nombre_visible_seguro(origen: &Path) -> Resultado<String> {
    let nombre = origen
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .ok_or_else(|| {
            Error::Validacion("El archivo seleccionado no tiene un nombre válido".into())
        })?;
    if nombre.contains('/') || nombre.contains('\\') {
        return Err(Error::Validacion(
            "El nombre del archivo contiene caracteres de ruta no permitidos".into(),
        ));
    }
    Ok(nombre.to_string())
}

fn base_canonica_documentos(carpeta_docs: &Path) -> Resultado<PathBuf> {
    fs::create_dir_all(carpeta_docs)?;
    Ok(fs::canonicalize(carpeta_docs)?)
}

fn destino_seguro_en(base_canonica: &Path, ruta_interna: &str) -> Resultado<PathBuf> {
    let limpia = validar_ruta_interna(ruta_interna)?;
    let destino = base_canonica.join(&limpia);
    if destino.parent() != Some(base_canonica) {
        return Err(Error::Validacion(
            "La ruta de destino queda fuera de documentos/".into(),
        ));
    }
    Ok(destino)
}

fn copiar_a_carpeta_segura(
    origen_raw: &str,
    base_canonica: &Path,
) -> Resultado<(String, String, u64, PathBuf)> {
    let ruta_txt = origen_raw.trim();
    if ruta_txt.is_empty() {
        return Err(Error::Validacion("Selecciona al menos un archivo".into()));
    }
    let origen_canon = fs::canonicalize(Path::new(ruta_txt))
        .map_err(|e| Error::NoEncontrado(format!("el archivo {ruta_txt} ({e})")))?;
    let meta = fs::metadata(&origen_canon)?;
    if !meta.is_file() {
        return Err(Error::Validacion(format!(
            "{ruta_txt} no es un archivo regular"
        )));
    }
    let nombre = nombre_visible_seguro(&origen_canon)?;
    let id = format!("D-{}", eventos::uuid());
    let ext = extension_segura(&origen_canon);
    let ruta_interna = if ext.is_empty() {
        id.clone()
    } else {
        format!("{id}.{ext}")
    };
    let destino = destino_seguro_en(base_canonica, &ruta_interna)?;

    let mut lector = fs::File::open(&origen_canon)?;
    let mut escritor = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destino)?;
    let escrito = io::copy(&mut lector, &mut escritor)?;
    escritor.sync_all()?;

    // Revalidamos de forma canónica que el archivo creado sigue dentro de `base_canonica`.
    let destino_canon = fs::canonicalize(&destino)?;
    if !destino_canon.starts_with(base_canonica) {
        let _ = fs::remove_file(&destino);
        return Err(Error::Validacion(
            "El archivo resuelto quedó fuera de la carpeta de documentos".into(),
        ));
    }
    Ok((id, nombre, escrito, destino))
}

/// Lista todos los documentos del proyecto (#120). `ruta` interna nunca sale serializada.
pub fn listar(c: &Connection) -> Resultado<Vec<Documento>> {
    db_docs::listar(c)
}

/// Sube uno o varios archivos a `<compartida>/documentos/` con nombre interno único
/// y publica el evento `subir_documentos` en `ops/` (#120).
/// Líder o Administrador publica directo (`pendiente = false`); otros roles quedan `pendiente = true`.
#[allow(clippy::too_many_arguments)]
pub fn subir(
    c: &Connection,
    ops: &Path,
    carpeta_docs: &Path,
    emisor: &str,
    rol_emisor: Rol,
    archivos: &[String],
    ahora: &str,
    hoy: &str,
) -> Resultado<Vec<Documento>> {
    if archivos.is_empty() {
        return Err(Error::Validacion(
            "Selecciona al menos un archivo para subir".into(),
        ));
    }
    let emisor_norm = normalizar_email(emisor);
    let base = base_canonica_documentos(carpeta_docs)?;
    let pendiente = !rol_emisor.es_lider_o_admin();

    let mut creados = Vec::with_capacity(archivos.len());
    let mut destinos_creados: Vec<PathBuf> = Vec::with_capacity(archivos.len());

    for ruta_origen in archivos {
        match copiar_a_carpeta_segura(ruta_origen, &base) {
            Ok((id, nombre, tamano, destino)) => {
                let ruta_rel = destino
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                destinos_creados.push(destino);
                creados.push(Documento {
                    id,
                    nombre,
                    tamano,
                    subido_por: emisor_norm.clone(),
                    fecha: hoy.to_string(),
                    pendiente,
                    ruta: ruta_rel,
                });
            }
            Err(e) => {
                for p in destinos_creados {
                    let _ = fs::remove_file(p);
                }
                return Err(e);
            }
        }
    }

    let payload = PayloadSubirDocumentos {
        documentos: creados.iter().map(PayloadDocumentoEvento::from).collect(),
    };
    let mut ev = Evento::nuevo(
        "subir_documentos",
        None,
        &emisor_norm,
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar subir_documentos: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    if let Err(e) = eventos::publicar(c, ops, &ev, |tx| {
        aplicar_subir_documentos(tx, &emisor_norm, Some(rol_emisor), &payload)
    }) {
        for p in destinos_creados {
            let _ = fs::remove_file(p);
        }
        return Err(e);
    }

    Ok(creados)
}

/// Aprueba un documento pendiente (Líder o Administrador) y publica en `ops/` (#120).
pub fn aprobar(
    c: &Connection,
    ops: &Path,
    actor: &str,
    rol_actor: Rol,
    id: &str,
    ahora: &str,
) -> Resultado<Documento> {
    if !rol_actor.es_lider_o_admin() {
        return Err(Error::SinPermiso);
    }
    let id_limpio = id.trim();
    let actor_norm = normalizar_email(actor);
    let payload = PayloadIdDocumento {
        id: id_limpio.to_string(),
    };
    let mut ev = Evento::nuevo(
        "aprobar_documento",
        None,
        &actor_norm,
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar aprobar_documento: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_aprobar_documento(tx, Some(rol_actor), id_limpio)
    })?;
    db_docs::obtener(c, id_limpio)
}

/// Elimina un documento del proyecto (Líder o Administrador), publica en `ops/`
/// y borra su copia segura de `<compartida>/documentos/` (#120).
pub fn eliminar(
    c: &Connection,
    ops: &Path,
    carpeta_docs: &Path,
    actor: &str,
    rol_actor: Rol,
    id: &str,
    ahora: &str,
) -> Resultado<Documento> {
    if !rol_actor.es_lider_o_admin() {
        return Err(Error::SinPermiso);
    }
    let id_limpio = id.trim();
    let actor_norm = normalizar_email(actor);
    let payload = PayloadIdDocumento {
        id: id_limpio.to_string(),
    };
    let mut ev = Evento::nuevo(
        "eliminar_documento",
        None,
        &actor_norm,
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar eliminar_documento: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();

    let mut eliminado = None;
    eventos::publicar(c, ops, &ev, |tx| {
        let doc = db_docs::borrar(tx, id_limpio)?;
        eliminado = Some(doc);
        Ok(())
    })?;
    let doc = eliminado.expect("documento borrado en transacción");
    borrar_copia_fisica(carpeta_docs, &doc.ruta);
    Ok(doc)
}

fn borrar_copia_fisica(carpeta_docs: &Path, ruta_interna: &str) {
    if let Ok(base) = base_canonica_documentos(carpeta_docs) {
        if let Ok(destino) = destino_seguro_en(&base, ruta_interna) {
            if let Ok(canon) = fs::canonicalize(&destino) {
                if canon.starts_with(&base) && canon.is_file() {
                    let _ = fs::remove_file(canon);
                }
            }
        }
    }
}

/// Resuelve y valida de forma canónica la ruta en disco de un documento para abrirlo localmente (#120).
pub fn resolver_archivo(c: &Connection, carpeta_docs: &Path, id: &str) -> Resultado<PathBuf> {
    let doc = db_docs::obtener(c, id.trim())?;
    let base = base_canonica_documentos(carpeta_docs)?;
    let esperado = destino_seguro_en(&base, &doc.ruta)?;
    let canon = fs::canonicalize(&esperado).map_err(|_| {
        Error::NoEncontrado(format!(
            "El archivo de \"{}\" aún no está sincronizado en esta máquina",
            doc.nombre
        ))
    })?;
    if !canon.starts_with(&base) || !canon.is_file() {
        return Err(Error::Validacion(
            "La ruta del documento quedó fuera de la carpeta compartida".into(),
        ));
    }
    Ok(canon)
}

fn aplicar_subir_documentos(
    c: &Connection,
    emisor: &str,
    rol_emisor: Option<Rol>,
    payload: &PayloadSubirDocumentos,
) -> Resultado<()> {
    let emisor_norm = normalizar_email(emisor);
    let espera_pendiente = match rol_emisor {
        Some(rol) => !rol.es_lider_o_admin(),
        None => return Err(Error::SinPermiso),
    };
    for raw in &payload.documentos {
        if normalizar_email(&raw.subido_por) != emisor_norm {
            return Err(Error::SinPermiso);
        }
        if !raw.pendiente && espera_pendiente {
            return Err(Error::SinPermiso);
        }
        validar_ruta_interna(&raw.ruta)?;
        if db_docs::buscar(c, &raw.id)?.is_some() {
            continue;
        }
        let doc = Documento {
            id: raw.id.clone(),
            nombre: raw.nombre.trim().to_string(),
            tamano: raw.tamano,
            subido_por: emisor_norm.clone(),
            fecha: raw.fecha.clone(),
            pendiente: raw.pendiente,
            ruta: raw.ruta.clone(),
        };
        db_docs::insertar(c, &doc)?;
    }
    Ok(())
}

fn aplicar_aprobar_documento(c: &Connection, rol_actor: Option<Rol>, id: &str) -> Resultado<()> {
    if !rol_actor.is_some_and(|r| r.es_lider_o_admin()) {
        return Err(Error::SinPermiso);
    }
    db_docs::aprobar(c, id.trim())
}

/// Reproduce un evento de documentos desde `ops/` (opcionalmente borrando copias físicas al eliminar).
pub fn aplicar_evento(
    c: &Connection,
    publica_admin: &str,
    carpeta_docs: Option<&Path>,
    evento: &Evento,
) -> Resultado<()> {
    let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
    if rol.is_none() {
        return Err(Error::SinPermiso);
    }
    match evento.tipo.as_str() {
        "subir_documentos" => {
            let payload: PayloadSubirDocumentos = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("subir_documentos inválido: {e}")))?;
            aplicar_subir_documentos(c, &evento.usuario, rol, &payload)
        }
        "aprobar_documento" => {
            let payload: PayloadIdDocumento = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("aprobar_documento inválido: {e}")))?;
            aplicar_aprobar_documento(c, rol, &payload.id)
        }
        "eliminar_documento" => {
            if !rol.is_some_and(|r| r.es_lider_o_admin()) {
                return Err(Error::SinPermiso);
            }
            let payload: PayloadIdDocumento = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("eliminar_documento inválido: {e}")))?;
            if let Some(doc) = db_docs::buscar(c, payload.id.trim())? {
                db_docs::borrar(c, &doc.id)?;
                if let Some(carpeta) = carpeta_docs {
                    borrar_copia_fisica(carpeta, &doc.ruta);
                }
            }
            Ok(())
        }
        otro => Err(Error::Validacion(format!(
            "tipo de evento de documento no soportado: {otro}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use crate::db::{usuarios, Db};
    use crate::eventos::lectura::{self, sincronizar};
    use crate::eventos::{carpeta_documentos, carpeta_ops, configurar_carpeta_compartida};
    use crate::identidad::firmas;
    use crate::identidad::llaves::{
        cargar_llave_admin, fijar_llave_publica_admin, generar_llaves_admin,
    };

    const ADMIN: &str = "admin@estudio.com";
    const LIDER: &str = "lider@estudio.com";
    const DEV: &str = "dev@estudio.com";
    const INTRUSO: &str = "intruso@fuera.com";
    const AHORA: &str = "2026-11-16T10:00:00.000Z";
    const HOY: &str = "2026-11-16";

    struct Escenario {
        tmp: TempDir,
        db_a: Db,
        ops: PathBuf,
        docs: PathBuf,
        db_b: Db,
        local_b: PathBuf,
        publica: String,
    }

    fn setup() -> Escenario {
        let tmp = TempDir::new().unwrap();
        let compartida = tmp.path().join("compartida");
        fs::create_dir_all(&compartida).unwrap();
        let local_a = tmp
            .path()
            .join("datos_a")
            .join("proyectos")
            .join("estudio-docs");
        let local_b = tmp
            .path()
            .join("datos_b")
            .join("proyectos")
            .join("estudio-docs");
        configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
        configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
        let ops = carpeta_ops(&local_a).unwrap();
        let docs = carpeta_documentos(&local_a).unwrap();

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

        Escenario {
            tmp,
            db_a,
            ops,
            docs,
            db_b,
            local_b,
            publica,
        }
    }

    #[test]
    fn subir_aprobar_eliminar_y_prevenir_path_traversal() {
        let e = setup();
        let origen_dir = e.tmp.path().join("usuario_origen");
        fs::create_dir_all(&origen_dir).unwrap();
        let f_dev = origen_dir.join("GDD v1.txt");
        let f_lider = origen_dir.join("Arte.PDF");
        fs::write(&f_dev, b"contenido gdd").unwrap();
        fs::write(&f_lider, b"%PDF-1.4 demo").unwrap();

        // Rechaza lista vacía, ruta inexistente y directorios.
        assert!(subir(
            e.db_a.conexion(),
            &e.ops,
            &e.docs,
            DEV,
            Rol::Desarrollador,
            &[],
            AHORA,
            HOY,
        )
        .is_err());
        assert!(subir(
            e.db_a.conexion(),
            &e.ops,
            &e.docs,
            DEV,
            Rol::Desarrollador,
            &[origen_dir.join("no_existe.txt").to_string_lossy().into()],
            AHORA,
            HOY,
        )
        .is_err());
        assert!(subir(
            e.db_a.conexion(),
            &e.ops,
            &e.docs,
            DEV,
            Rol::Desarrollador,
            &[origen_dir.to_string_lossy().into()],
            AHORA,
            HOY,
        )
        .is_err());

        // Desarrollador sube archivo -> `pendiente = true` y archivo renombrado a `D-<uuid>.txt`.
        let subidos_dev = subir(
            e.db_a.conexion(),
            &e.ops,
            &e.docs,
            DEV,
            Rol::Desarrollador,
            &[f_dev.to_string_lossy().into()],
            "2026-11-16T10:01:00.000Z",
            HOY,
        )
        .unwrap();
        assert_eq!(subidos_dev.len(), 1);
        let d_dev = &subidos_dev[0];
        assert!(d_dev.pendiente);
        assert_eq!(d_dev.nombre, "GDD v1.txt");
        assert!(d_dev.ruta.starts_with("D-") && d_dev.ruta.ends_with(".txt"));
        assert!(e.docs.join(&d_dev.ruta).is_file());
        assert!(!e.docs.join("GDD v1.txt").exists());
        assert!(serde_json::to_value(d_dev).unwrap().get("ruta").is_none());

        // Líder sube archivo -> publicado (`pendiente = false`).
        let subidos_lider = subir(
            e.db_a.conexion(),
            &e.ops,
            &e.docs,
            LIDER,
            Rol::Lider,
            &[f_lider.to_string_lossy().into()],
            "2026-11-16T10:02:00.000Z",
            HOY,
        )
        .unwrap();
        assert!(!subidos_lider[0].pendiente);
        assert!(subidos_lider[0].ruta.ends_with(".pdf"));

        // Permisos aprobar / eliminar.
        assert_eq!(
            aprobar(
                e.db_a.conexion(),
                &e.ops,
                DEV,
                Rol::Desarrollador,
                &d_dev.id,
                "2026-11-16T10:03:00.000Z",
            )
            .unwrap_err()
            .codigo(),
            "sin_permiso"
        );
        let aprobado = aprobar(
            e.db_a.conexion(),
            &e.ops,
            ADMIN,
            Rol::Administrador,
            &d_dev.id,
            "2026-11-16T10:03:10.000Z",
        )
        .unwrap();
        assert!(!aprobado.pendiente);

        // `resolver_archivo` resuelve dentro de `e.docs` y rechaza cualquier fila alterada con traversal.
        let resuelto = resolver_archivo(e.db_a.conexion(), &e.docs, &d_dev.id).unwrap();
        assert!(resuelto.starts_with(fs::canonicalize(&e.docs).unwrap()));

        let mut malicioso = Documento {
            id: "D-evil".into(),
            nombre: "evil.txt".into(),
            tamano: 10,
            subido_por: DEV.into(),
            fecha: HOY.into(),
            pendiente: true,
            ruta: "../fuera.txt".into(),
        };
        db_docs::insertar(e.db_a.conexion(), &malicioso).unwrap();
        assert!(resolver_archivo(e.db_a.conexion(), &e.docs, "D-evil").is_err());
        db_docs::borrar(e.db_a.conexion(), "D-evil").unwrap();

        // Rechaza eventos de ops/ con path traversal o bypass de aprobación.
        malicioso.ruta = "sub/archivo.txt".into();
        let ev_trav = Evento::nuevo(
            "subir_documentos",
            None,
            DEV,
            serde_json::to_value(PayloadSubirDocumentos {
                documentos: vec![PayloadDocumentoEvento::from(&malicioso)],
            })
            .unwrap(),
        );
        assert!(aplicar_evento(e.db_a.conexion(), &e.publica, Some(&e.docs), &ev_trav).is_err());

        malicioso.ruta = "D-fake.txt".into();
        malicioso.pendiente = false;
        let ev_bypass = Evento::nuevo(
            "subir_documentos",
            None,
            DEV,
            serde_json::to_value(PayloadSubirDocumentos {
                documentos: vec![PayloadDocumentoEvento::from(&malicioso)],
            })
            .unwrap(),
        );
        assert_eq!(
            aplicar_evento(e.db_a.conexion(), &e.publica, Some(&e.docs), &ev_bypass)
                .unwrap_err()
                .codigo(),
            "sin_permiso"
        );

        // Eliminar borra fila SQLite y archivo en `e.docs`.
        let ruta_lider = e.docs.join(&subidos_lider[0].ruta);
        assert!(ruta_lider.is_file());
        eliminar(
            e.db_a.conexion(),
            &e.ops,
            &e.docs,
            LIDER,
            Rol::Lider,
            &subidos_lider[0].id,
            "2026-11-16T10:04:00.000Z",
        )
        .unwrap();
        assert!(!ruta_lider.exists());

        // B sincroniza por `ops/`: termina solo con `d_dev` aprobado (`pendiente = false`).
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:10:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync = sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, &e.publica, ev)
        })
        .unwrap();
        assert_eq!(sync.aplicados, 4);
        let en_b = listar(e.db_b.conexion()).unwrap();
        assert_eq!(en_b.len(), 1);
        assert_eq!(en_b[0].id, d_dev.id);
        assert!(!en_b[0].pendiente);
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #120): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// ejerce los comandos Tauri `subir_documentos_en`, `aprobar_documento_en`,
    /// `eliminar_documento_en`, `listar_documentos_en` y `resolver_documento_en`:
    /// 1. Desarrollador sube archivo -> queda `pendiente = true` dentro de `<OneDrive>/documentos/`
    ///    con nombre interno `D-<uuid>.<ext>` (oculto al serializar).
    /// 2. Desarrollador no puede aprobar ni eliminar (`sin_permiso`).
    /// 3. Líder sube otro archivo -> queda publicado (`pendiente = false`).
    /// 4. Administrador aprueba el del Desarrollador, Líder elimina el suyo (borra copia física)
    ///    y `resolver_documento_en` verifica la ruta canónica en OneDrive.
    /// 5. Eventos forjados o con path traversal en OneDrive se descartan; máquina B
    ///    sincroniza exactamente el estado esperado.
    ///
    /// Al terminar borra workspace y subcarpeta en OneDrive.
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_documentos() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::documentos::{
            aprobar_documento_en, eliminar_documento_en, listar_documentos_en,
            resolver_documento_en, subir_documentos_en,
        };
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
        let compartida = nube.join(format!("BranchBoard-prueba-120-{sufijo}"));
        fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let origen_tmp = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-120-{sufijo}"))
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
            let docs_a = carpeta_documentos(&local_a).unwrap();

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

            estado_a
                .con_db(|db_a| {
                    let firmante_admin = Firmante {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                        llave: &llave_admin,
                    };
                    for (correo, rol) in [(DEV, Rol::Desarrollador), (LIDER, Rol::Lider)] {
                        equipo::crear(
                            db_a.conexion(),
                            &ops_a,
                            &publica,
                            &firmante_admin,
                            correo,
                            rol,
                            &ahora_pausa(),
                            &hoy_real,
                        )?;
                    }
                    Ok(())
                })
                .unwrap();

            let f_dev = origen_tmp.path().join("Diseño Combate.txt");
            let f_lider = origen_tmp.path().join("Cronograma.md");
            fs::write(&f_dev, b"documento dev real").unwrap();
            fs::write(&f_lider, b"# Cronograma").unwrap();

            // 1. DEV sube documento -> `pendiente = true`, ruta oculta y nombre generado en OneDrive/documentos.
            estado_a.fijar_usuario(Some(DEV));
            pausa();
            let sub_dev =
                subir_documentos_en(&estado_a, &[f_dev.to_string_lossy().into()]).unwrap();
            assert_eq!(sub_dev.len(), 1);
            assert!(sub_dev[0].pendiente);
            assert!(serde_json::to_value(&sub_dev[0])
                .unwrap()
                .get("ruta")
                .is_none());

            // DEV rechazado al aprobar o eliminar.
            assert_eq!(
                aprobar_documento_en(&estado_a, &sub_dev[0].id)
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );
            assert_eq!(
                eliminar_documento_en(&estado_a, &sub_dev[0].id)
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );

            // 2. Líder sube otro -> publicado enseguida (`pendiente = false`), luego lo elimina.
            estado_a.fijar_usuario(Some(LIDER));
            pausa();
            let sub_lider =
                subir_documentos_en(&estado_a, &[f_lider.to_string_lossy().into()]).unwrap();
            assert!(!sub_lider[0].pendiente);

            // 3. Admin aprueba el de DEV y Líder elimina el suyo.
            estado_a.fijar_usuario(Some(&admin_fila.email));
            pausa();
            let aprobado = aprobar_documento_en(&estado_a, &sub_dev[0].id).unwrap();
            assert!(!aprobado.pendiente);
            let ruta_abierta = resolver_documento_en(&estado_a, &sub_dev[0].id).unwrap();
            assert!(ruta_abierta.starts_with(fs::canonicalize(&docs_a).unwrap()));
            assert!(ruta_abierta.is_file());

            estado_a.fijar_usuario(Some(LIDER));
            pausa();
            eliminar_documento_en(&estado_a, &sub_lider[0].id).unwrap();
            assert_eq!(listar_documentos_en(&estado_a).unwrap().len(), 1);

            // Inyectamos evento de traversal y de intruso para comprobar descarte.
            let mut ev_trav = Evento::nuevo(
                "subir_documentos",
                None,
                DEV,
                serde_json::to_value(PayloadSubirDocumentos {
                    documentos: vec![PayloadDocumentoEvento {
                        id: "D-trav".into(),
                        nombre: "hack.txt".into(),
                        tamano: 4,
                        subido_por: DEV.into(),
                        fecha: hoy_real.clone(),
                        pendiente: true,
                        ruta: "../../hack.txt".into(),
                    }],
                })
                .unwrap(),
            );
            ev_trav.timestamp = crate::modelo::ahora_iso();
            eventos::escribir_evento(&ops_a, &ev_trav).unwrap();

            let mut ev_intruso = Evento::nuevo(
                "subir_documentos",
                None,
                INTRUSO,
                serde_json::to_value(PayloadSubirDocumentos {
                    documentos: vec![PayloadDocumentoEvento {
                        id: "D-intruso".into(),
                        nombre: "spam.txt".into(),
                        tamano: 4,
                        subido_por: INTRUSO.into(),
                        fecha: hoy_real,
                        pendiente: true,
                        ruta: "D-intruso.txt".into(),
                    }],
                })
                .unwrap(),
            );
            ev_intruso.timestamp = crate::modelo::ahora_iso();
            eventos::escribir_evento(&ops_a, &ev_intruso).unwrap();

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 4. Máquina B sincroniza vía OneDrive.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            println!("B sincroniza documentos desde OneDrive: {sync_b:?}");
            assert_eq!(sync_b.descartados, 2);

            let docs_b = listar(db_b.conexion()).unwrap();
            assert_eq!(docs_b.len(), 1);
            assert_eq!(docs_b[0].id, sub_dev[0].id);
            assert!(!docs_b[0].pendiente);
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
