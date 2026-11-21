//! Logs locales del revisor, consultados al pulsar Terminar (#153).
//! No ejecuta Unity, no modifica tareas y no transmite los logs al servidor.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    error::Resultado,
    estado::AppState,
    tareas::{self, AutorTarea},
};

const MAXIMO_BYTES: u64 = 64 * 1024;

#[derive(Debug, Default, Serialize)]
pub struct LogsUnity {
    pub archivo: Option<String>,
    pub modificado: Option<String>,
    pub contenido: String,
    pub recortado: bool,
    pub aviso: Option<String>,
}

/// Solo se recibe el id de la tarea; el frontend nunca elige qué archivo leer.
#[tauri::command]
pub async fn logs_unity_para_terminar(
    app: AppHandle,
    estado: State<'_, AppState>,
    tarea_id: String,
) -> Resultado<LogsUnity> {
    let yo = estado.exigir_permiso("logs_unity_para_terminar")?;
    estado.con_db(|db| {
        tareas::plan_de_autorizacion(
            db.conexion(),
            &AutorTarea {
                email: &yo.email,
                rol: yo.rol,
            },
            &tarea_id,
        )?;
        Ok(())
    })?;
    let ruta = ruta_editor_log(&app);
    Ok(ruta.map(|p| leer_log(&p)).unwrap_or_default())
}

fn ruta_editor_log(app: &AppHandle) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        app.path()
            .local_data_dir()
            .ok()
            .map(|p| p.join("Unity/Editor/Editor.log"))
    }
    #[cfg(target_os = "macos")]
    {
        app.path()
            .home_dir()
            .ok()
            .map(|p| p.join("Library/Logs/Unity/Editor.log"))
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        app.path()
            .home_dir()
            .ok()
            .map(|p| p.join(".config/unity3d/Editor.log"))
    }
}

/// Lee un fragmento acotado, incluso si Unity tiene el archivo abierto.
/// Un log ausente no es una compilación aprobada; tampoco impide terminar.
pub fn leer_log(ruta: &Path) -> LogsUnity {
    match leer(ruta) {
        Ok(log) => log,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LogsUnity::default(),
        Err(_) => LogsUnity {
            aviso: Some("No se pudieron leer los logs locales de Unity.".into()),
            ..Default::default()
        },
    }
}

fn leer(ruta: &Path) -> std::io::Result<LogsUnity> {
    let mut archivo = File::open(ruta)?;
    let datos = archivo.metadata()?;
    if !datos.is_file() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    if datos.len() == 0 {
        return Ok(LogsUnity::default());
    }
    let inicio = datos.len().saturating_sub(MAXIMO_BYTES);
    archivo.seek(SeekFrom::Start(inicio))?;
    let mut bytes = Vec::new();
    archivo.take(MAXIMO_BYTES).read_to_end(&mut bytes)?;
    let mut contenido = String::from_utf8_lossy(&bytes).into_owned();
    // No mostrar una línea cortada por el límite del fragmento.
    if inicio > 0 {
        if let Some(fin) = contenido.find('\n') {
            contenido.drain(..=fin);
        }
    }
    Ok(LogsUnity {
        archivo: Some("Editor.log".into()),
        modificado: datos
            .modified()
            .ok()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
        contenido,
        recortado: inicio > 0,
        aviso: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ausente_o_vacio_no_inventa_un_resultado() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("Editor.log");
        assert!(leer_log(&ruta).archivo.is_none());
        std::fs::write(&ruta, "").unwrap();
        assert!(leer_log(&ruta).archivo.is_none());
    }

    #[test]
    fn conserva_errores_y_fecha_sin_marcar_la_tarea_como_aprobada() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("Editor.log");
        std::fs::write(&ruta, "Assets/Salto.cs(3,1): error CS1002: ; expected\n").unwrap();
        let log = leer_log(&ruta);
        assert!(log.contenido.contains("error CS1002"));
        assert!(log.modificado.is_some());
        assert!(!log.recortado);
    }

    #[test]
    fn acota_el_final_del_log_y_no_sigue_el_crecimiento_del_archivo() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("Editor.log");
        std::fs::write(
            &ruta,
            format!(
                "{}\nultimo error\n",
                "x".repeat(MAXIMO_BYTES as usize + 100)
            ),
        )
        .unwrap();
        let log = leer_log(&ruta);
        assert!(log.recortado);
        assert!(log.contenido.len() <= MAXIMO_BYTES as usize);
        assert_eq!(log.contenido, "ultimo error\n");
    }

    #[test]
    fn un_directorio_no_se_presenta_como_sin_logs() {
        let dir = tempfile::tempdir().unwrap();
        assert!(leer_log(dir.path()).aviso.is_some());
    }
}
