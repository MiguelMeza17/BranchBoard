//! Copias de seguridad de la base de datos (apartado "Restaurar backup" de Ajustes).
//! Ambos comandos son **exclusivos del Administrador** (ver `identidad::permisos`).

use std::path::PathBuf;

use tauri::State;

use crate::db::{backups, Db};
use crate::error::{Error, Resultado};
use crate::estado::AppState;

pub use crate::db::backups::{Backup, ListaBackups};

pub(crate) fn carpeta_abierta(estado: &AppState) -> Resultado<PathBuf> {
    estado.con_db(|db| {
        db.carpeta().map(|c| c.to_path_buf()).ok_or_else(|| {
            Error::Validacion("El proyecto abierto no tiene carpeta de datos".into())
        })
    })
}

/// La base en uso ("Actual") y las copias disponibles.
#[tauri::command]
pub fn listar_backups(estado: State<'_, AppState>) -> Resultado<ListaBackups> {
    estado.exigir_permiso("listar_backups")?;
    backups::lista(&carpeta_abierta(&estado)?)
}

/// Guarda la base actual como una copia más y pone en su lugar la copia `nombre`.
/// Cierra la base mientras cambia el archivo y la vuelve a abrir, pase lo que pase.
#[tauri::command]
pub fn restaurar_backup(estado: State<'_, AppState>, nombre: String) -> Resultado<Backup> {
    estado.exigir_permiso("restaurar_backup")?;
    if estado.hay_operacion_en_curso() {
        return Err(Error::Validacion(
            "Hay una operación en curso. Espera a que termine para restaurar.".into(),
        ));
    }
    let carpeta = carpeta_abierta(&estado)?;
    let _operacion = estado.iniciar_operacion();
    estado.cerrar_proyecto();
    let resultado = backups::restaurar(&carpeta, &nombre);
    estado.abrir_proyecto(Db::abrir(&carpeta)?);
    resultado
}
