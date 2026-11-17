//! Comandos Tauri para gestionar documentos del proyecto en la carpeta compartida (#120).

use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::documentos::Documento;
use crate::documentos;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::{ahora_iso, hoy_local};

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

/// Sube archivos locales al proyecto en `<compartida>/documentos/` y publica por `ops/` (#120).
#[tauri::command]
pub fn subir_documentos(
    estado: State<'_, AppState>,
    archivos: Vec<String>,
) -> Resultado<Vec<Documento>> {
    subir_documentos_en(&estado, &archivos)
}

/// Versión sin envoltorio Tauri de [`subir_documentos`].
pub fn subir_documentos_en(estado: &AppState, archivos: &[String]) -> Resultado<Vec<Documento>> {
    let yo = estado.exigir_permiso("subir_documentos")?;
    let carpeta = carpeta_abierta(estado)?;
    let ops = eventos::carpeta_ops(&carpeta)?;
    let docs = eventos::carpeta_documentos(&carpeta)?;
    estado.con_db(|db| {
        documentos::subir(
            db.conexion(),
            &ops,
            &docs,
            &yo.email,
            yo.rol,
            archivos,
            &ahora_iso(),
            &hoy_local(),
        )
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
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    estado
        .con_db(|db| documentos::aprobar(db.conexion(), &ops, &yo.email, yo.rol, id, &ahora_iso()))
}

/// Elimina un documento del proyecto y su archivo compartido (Líder o Administrador, #120).
#[tauri::command]
pub fn eliminar_documento(estado: State<'_, AppState>, id: String) -> Resultado<Documento> {
    eliminar_documento_en(&estado, &id)
}

/// Versión sin envoltorio Tauri de [`eliminar_documento`].
pub fn eliminar_documento_en(estado: &AppState, id: &str) -> Resultado<Documento> {
    let yo = estado.exigir_permiso("eliminar_documento")?;
    let carpeta = carpeta_abierta(estado)?;
    let ops = eventos::carpeta_ops(&carpeta)?;
    let docs = eventos::carpeta_documentos(&carpeta)?;
    estado.con_db(|db| {
        documentos::eliminar(
            db.conexion(),
            &ops,
            &docs,
            &yo.email,
            yo.rol,
            id,
            &ahora_iso(),
        )
    })
}

/// Resuelve y valida canónicamente la ruta local de un documento antes de abrirlo (#120).
pub fn resolver_documento_en(estado: &AppState, id: &str) -> Resultado<PathBuf> {
    estado.exigir_permiso("abrir_documento")?;
    let docs = eventos::carpeta_documentos(&carpeta_abierta(estado)?)?;
    estado.con_db(|db| documentos::resolver_archivo(db.conexion(), &docs, id))
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
