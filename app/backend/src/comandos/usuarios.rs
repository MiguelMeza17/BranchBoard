//! Pantalla Usuarios (issue #90): listar, crear, cambiar de rol, confirmar y
//! eliminar. Las reglas viven en `identidad::equipo`; aquí se revisa el rol
//! (`identidad::permisos`), se abre la llave de firma y se llama a esas reglas.
//!
//! Los comandos que firman reciben `passphrase` opcional: si la llave de quien
//! firma está cifrada, la primera vez en la sesión responden `llave_cifrada` y
//! la interfaz la pide.
//!
//! Cada cambio sale como evento administrativo firmado a `ops/` (issue #93), así
//! que hace falta la carpeta compartida del proyecto en esta máquina; sin ella
//! el comando responde `validacion` pidiéndola.

use std::path::Path;

use tauri::State;

use crate::comandos::datos::carpeta_abierta;
use crate::db::usuarios::Usuario;
use crate::error::Resultado;
use crate::estado::{llave_publica_del_proyecto, AppState};
use crate::eventos;
use crate::identidad::equipo::{self, Firmante, UsuarioVisible};
use crate::modelo::{ahora_iso, hoy_local, Rol};

/// Corre `f` con la base, `ops/`, la llave pública del Administrador y quien firma.
fn firmando<T>(
    estado: &AppState,
    comando: &str,
    passphrase: Option<String>,
    f: impl FnOnce(&rusqlite::Connection, &Path, &str, &Firmante<'_>) -> Resultado<T>,
) -> Resultado<T> {
    let yo = estado.exigir_permiso(comando)?;
    // Antes de tomar la base: `carpeta_abierta` también la toma.
    let ops = eventos::carpeta_ops(&carpeta_abierta(estado)?)?;
    estado.con_db(|db| {
        let publica = llave_publica_del_proyecto(db)?;
        let llave = estado.llave_firma(db, &yo, passphrase.as_deref())?;
        let firmante = Firmante {
            email: &yo.email,
            rol: yo.rol,
            llave: &llave,
        };
        f(db.conexion(), &ops, &publica, &firmante)
    })
}

/// Todos los del proyecto. Sin correos para quien no es Líder ni Administrador.
#[tauri::command]
pub fn listar_usuarios(estado: State<'_, AppState>) -> Resultado<Vec<UsuarioVisible>> {
    let yo = estado.exigir_permiso("listar_usuarios")?;
    estado.con_db(|db| equipo::listar(db.conexion(), &llave_publica_del_proyecto(db)?, yo.rol))
}

#[tauri::command]
pub fn crear_usuario(
    estado: State<'_, AppState>,
    email: String,
    rol: Rol,
    passphrase: Option<String>,
) -> Resultado<Usuario> {
    firmando(
        &estado,
        "crear_usuario",
        passphrase,
        |c, ops, publica, firmante| {
            equipo::crear(
                c,
                ops,
                publica,
                firmante,
                &email,
                rol,
                &ahora_iso(),
                &hoy_local(),
            )
        },
    )
}

#[tauri::command]
pub fn cambiar_rol(
    estado: State<'_, AppState>,
    email: String,
    rol: Rol,
    passphrase: Option<String>,
) -> Resultado<()> {
    firmando(
        &estado,
        "cambiar_rol",
        passphrase,
        |c, ops, publica, firmante| {
            equipo::cambiar_rol(c, ops, publica, firmante, &email, rol, &ahora_iso())
        },
    )
}

#[tauri::command]
pub fn confirmar_usuario(
    estado: State<'_, AppState>,
    email: String,
    passphrase: Option<String>,
) -> Resultado<()> {
    firmando(
        &estado,
        "confirmar_usuario",
        passphrase,
        |c, ops, publica, firmante| {
            equipo::confirmar(c, ops, publica, firmante, &email, &ahora_iso())
        },
    )
}

/// Devuelve cuántas filas volvió a firmar el Administrador (al eliminar a un Líder).
#[tauri::command]
pub fn eliminar_usuario(
    estado: State<'_, AppState>,
    email: String,
    passphrase: Option<String>,
) -> Resultado<usize> {
    firmando(
        &estado,
        "eliminar_usuario",
        passphrase,
        |c, ops, publica, firmante| {
            equipo::eliminar(c, ops, publica, firmante, &email, &ahora_iso())
        },
    )
}
