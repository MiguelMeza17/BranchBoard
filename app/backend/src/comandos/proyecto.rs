//! Comandos Tauri de los ajustes del proyecto entero (#127): nombre, ajustes
//! clave/valor e imagen. Cambiarlos es exclusivo del Administrador; leerlos, de todos.

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::configuracion::Ajuste;
use crate::error::Resultado;
use crate::estado::AppState;
use crate::eventos;
use crate::modelo::ahora_iso;
use crate::proyecto::{self, ConfiguracionProyecto};

/// El nombre y la imagen del proyecto abierto.
#[tauri::command]
pub fn obtener_configuracion_proyecto(
    estado: State<'_, AppState>,
) -> Resultado<ConfiguracionProyecto> {
    obtener_configuracion_proyecto_en(&estado)
}

/// Versión sin envoltorio Tauri de [`obtener_configuracion_proyecto`].
pub fn obtener_configuracion_proyecto_en(estado: &AppState) -> Resultado<ConfiguracionProyecto> {
    estado.exigir_permiso("obtener_configuracion_proyecto")?;
    // Sin carpeta compartida configurada todavía no hay imagen que leer.
    let carpeta = eventos::carpeta_imagen_proyecto(&carpeta_abierta(estado)?).ok();
    estado.con_db(|db| proyecto::leer(db.conexion(), carpeta.as_deref()))
}

/// Cambia el nombre del proyecto en BranchBoard (no el repositorio de Unity VC).
#[tauri::command]
pub fn renombrar_proyecto(estado: State<'_, AppState>, nombre: String) -> Resultado<Ajuste> {
    renombrar_proyecto_en(&estado, &nombre)
}

/// Versión sin envoltorio Tauri de [`renombrar_proyecto`].
pub fn renombrar_proyecto_en(estado: &AppState, nombre: &str) -> Resultado<Ajuste> {
    let yo = estado.exigir_permiso("renombrar_proyecto")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    estado.con_db(|db| {
        proyecto::renombrar(db.conexion(), &ops, &yo.email, yo.rol, nombre, &ahora_iso())
    })
}

/// Crea o cambia un ajuste del proyecto entero (`configuracion_proyecto`).
#[tauri::command]
pub fn actualizar_configuracion_proyecto(
    estado: State<'_, AppState>,
    clave: String,
    valor: String,
) -> Resultado<Ajuste> {
    actualizar_configuracion_proyecto_en(&estado, &clave, &valor)
}

/// Versión sin envoltorio Tauri de [`actualizar_configuracion_proyecto`].
pub fn actualizar_configuracion_proyecto_en(
    estado: &AppState,
    clave: &str,
    valor: &str,
) -> Resultado<Ajuste> {
    let yo = estado.exigir_permiso("actualizar_configuracion_proyecto")?;
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    estado.con_db(|db| {
        proyecto::actualizar(
            db.conexion(),
            &ops,
            &yo.email,
            yo.rol,
            clave,
            valor,
            &ahora_iso(),
        )
    })
}

/// Pone la imagen elegida (PNG, JPG o WebP) como imagen del proyecto, en 256×256.
/// Devuelve la imagen nueva como `data:image/png;base64,…`.
#[tauri::command]
pub fn subir_imagen_proyecto(estado: State<'_, AppState>, ruta: String) -> Resultado<String> {
    subir_imagen_proyecto_en(&estado, &ruta)
}

/// Versión sin envoltorio Tauri de [`subir_imagen_proyecto`].
pub fn subir_imagen_proyecto_en(estado: &AppState, ruta: &str) -> Resultado<String> {
    let yo = estado.exigir_permiso("subir_imagen_proyecto")?;
    let carpeta = carpeta_abierta(estado)?;
    let ops = eventos::carpeta_ops(&carpeta)?;
    let imagen = eventos::carpeta_imagen_proyecto(&carpeta)?;
    estado.con_db(|db| {
        proyecto::subir_imagen(
            db.conexion(),
            &ops,
            &imagen,
            &yo.email,
            yo.rol,
            ruta,
            &ahora_iso(),
        )
    })
}
