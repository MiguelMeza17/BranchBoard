//! Comandos Tauri para gestionar documentos del proyecto (#120). Los archivos van a Supabase Storage si
//! el proyecto ya está en el servidor (#242); si no, pide vincularlo primero (#244).

use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::configuracion::{self, CLAVE_PROYECTO_SERVIDOR};
use crate::db::documentos::Documento;
use crate::documentos::{self, AlmacenServidor};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::modelo::{ahora_iso, hoy_local};
use crate::servidor::pat::LlaveroSistema;
use crate::supabase::storage::Storage;
use crate::supabase::{ClienteSupabase, Entorno};

/// Carpeta local (de este equipo) donde se guardan los documentos bajados del servidor para abrirlos.
use crate::exportacion::CACHE_DOCUMENTOS;

/// Id del proyecto en el servidor, si ya está vinculado (#240). Sin él no se pueden guardar ni abrir archivos.
pub(crate) fn proyecto_en_servidor(estado: &AppState) -> Resultado<Option<String>> {
    let id = estado.con_db(|db| configuracion::leer(db.conexion(), CLAVE_PROYECTO_SERVIDOR))?;
    Ok(id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

/// Hace `con` con el almacén del proyecto abierto: Supabase Storage (con la sesión del servidor). Desde
/// #244 no hay carpeta compartida: sin servidor, los documentos piden pasar el proyecto al servidor (#245).
fn con_almacen<T>(
    estado: &AppState,
    con: impl FnOnce(&dyn documentos::Almacen) -> Resultado<T>,
) -> Resultado<T> {
    let carpeta = carpeta_abierta(estado)?;
    match proyecto_en_servidor(estado)? {
        Some(proyecto_id) => {
            let supabase = Entorno::actual().proyecto();
            let cliente = ClienteSupabase::new(supabase, LlaveroSistema);
            let storage = Storage::new(&cliente, supabase);
            let cache = carpeta.join(CACHE_DOCUMENTOS);
            con(&AlmacenServidor {
                servidor: &storage,
                proyecto_id: &proyecto_id,
                cache: &cache,
            })
        }
        None => Err(Error::Validacion(
            "Para guardar o abrir documentos, primero pasa el proyecto al servidor de BranchBoard"
                .into(),
        )),
    }
}

/// Lista los documentos del proyecto (#120). Nunca incluye la ruta interna en disco.
#[tauri::command]
pub fn listar_documentos(estado: State<'_, AppState>) -> Resultado<Vec<Documento>> {
    listar_documentos_en(&estado)
}

/// Versión sin envoltorio Tauri de [`listar_documentos`].
pub fn listar_documentos_en(estado: &AppState) -> Resultado<Vec<Documento>> {
    estado.exigir_permiso("listar_documentos")?;
    estado.con_db(|db| documentos::listar(db.conexion()))
}

/// Sube archivos locales al proyecto y publica el evento (#120, #242).
#[tauri::command]
pub fn subir_documentos(
    estado: State<'_, AppState>,
    archivos: Vec<String>,
) -> Resultado<Vec<Documento>> {
    subir_documentos_en(&estado, &archivos)
}

/// Versión sin envoltorio Tauri de [`subir_documentos`]. Los archivos se suben sin bloquear la base
/// (con el servidor puede tardar); después se publica el evento.
pub fn subir_documentos_en(estado: &AppState, archivos: &[String]) -> Resultado<Vec<Documento>> {
    let yo = estado.exigir_permiso("subir_documentos")?;
    con_almacen(estado, |almacen| {
        let creados =
            documentos::guardar_archivos(almacen, &yo.email, yo.rol, archivos, &hoy_local())?;
        let publicado = estado.con_db(|db| {
            documentos::publicar_subida(
                db.conexion(),
                almacen,
                &yo.email,
                yo.rol,
                creados.clone(),
                &ahora_iso(),
            )
        });
        if publicado.is_err() {
            // Si ni siquiera se pudo abrir la base, `publicar_subida` no llegó a borrarlos.
            for d in &creados {
                almacen.borrar(&d.ruta);
            }
        }
        publicado
    })
}

/// Aprueba un documento pendiente (solo Líder o Administrador, #120).
#[tauri::command]
pub fn aprobar_documento(estado: State<'_, AppState>, id: String) -> Resultado<Documento> {
    aprobar_documento_en(&estado, &id)
}

/// Versión sin envoltorio Tauri de [`aprobar_documento`].
pub fn aprobar_documento_en(estado: &AppState, id: &str) -> Resultado<Documento> {
    let yo = estado.exigir_permiso("aprobar_documento")?;
    estado.con_db(|db| documentos::aprobar(db.conexion(), &yo.email, yo.rol, id, &ahora_iso()))
}

/// Elimina un documento del proyecto y su archivo (Líder o Administrador, #120, #242).
#[tauri::command]
pub fn eliminar_documento(estado: State<'_, AppState>, id: String) -> Resultado<Documento> {
    eliminar_documento_en(&estado, &id)
}

/// Versión sin envoltorio Tauri de [`eliminar_documento`]. El archivo se borra después de publicar,
/// sin bloquear la base.
pub fn eliminar_documento_en(estado: &AppState, id: &str) -> Resultado<Documento> {
    let yo = estado.exigir_permiso("eliminar_documento")?;
    con_almacen(estado, |almacen| {
        let doc = estado.con_db(|db| {
            documentos::eliminar_registro(db.conexion(), &yo.email, yo.rol, id, &ahora_iso())
        })?;
        almacen.borrar(&doc.ruta);
        Ok(doc)
    })
}

/// Ruta local de un documento para abrirlo (#120); si está en el servidor, lo baja la primera vez (#242).
pub fn resolver_documento_en(estado: &AppState, id: &str) -> Resultado<PathBuf> {
    estado.exigir_permiso("abrir_documento")?;
    let doc = estado.con_db(|db| crate::db::documentos::obtener(db.conexion(), id.trim()))?;
    let carpeta = carpeta_abierta(estado)?;
    if proyecto_en_servidor(estado)?.is_none() && crate::exportacion::es_recuperado(&carpeta) {
        return documentos::Almacen::archivo_local(
            &documentos::CarpetaCompartida {
                carpeta_docs: &carpeta.join(CACHE_DOCUMENTOS),
            },
            &doc.ruta,
            &doc.nombre,
        );
    }
    con_almacen(estado, |almacen| {
        almacen.archivo_local(&doc.ruta, &doc.nombre)
    })
}

/// Abre el documento con la aplicación predeterminada del sistema (local, #120).
#[tauri::command]
pub fn abrir_documento(estado: State<'_, AppState>, id: String) -> Resultado<()> {
    let ruta = resolver_documento_en(&estado, &id)?;
    abrir_en_sistema(&ruta)
}

fn abrir_en_sistema(ruta: &Path) -> Resultado<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(ruta)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(ruta).spawn()?;
        Ok(())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(ruta).spawn()?;
        Ok(())
    }
}
