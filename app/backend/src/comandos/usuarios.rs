//! Pantalla Usuarios (issue #90): listar, crear, cambiar de rol, confirmar y
//! eliminar. Con el proyecto en el servidor (#241) todo va por `identidad::equipo_servidor`
//! (funciones del servidor, sin firmas); lo de abajo es el camino de antes, por `ops/`.
//! Las reglas viven en `identidad::equipo`; aquí se revisa el rol
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
use crate::comandos::documentos::proyecto_en_servidor;
use crate::db::usuarios::{self, Usuario};
use crate::error::Resultado;
use crate::estado::{llave_publica_del_proyecto, AppState};
use crate::eventos;
use crate::identidad::equipo::{self, Firmante, UsuarioVisible};
use crate::identidad::equipo_servidor::{self as servidor, EnSupabase};
use crate::modelo::{ahora_iso, hoy_local, Rol};
use crate::servidor::pat::LlaveroSistema;
use crate::supabase::{ClienteSupabase, Entorno};

/// Con el proyecto en el servidor (#241), corre `f` con el equipo remoto y la función que guarda la copia
/// local; si no, `None` (sigue el camino de las firmas por `ops/`). La red va fuera del bloqueo de la base.
fn en_servidor<T>(
    estado: &AppState,
    comando: &str,
    f: impl FnOnce(
        &EnSupabase<'_, LlaveroSistema>,
        &dyn Fn(&[Usuario]) -> Resultado<()>,
        Rol,
    ) -> Resultado<T>,
) -> Option<Resultado<T>> {
    let proyecto = match proyecto_en_servidor(estado) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => return Some(Err(e)),
    };
    Some((|| {
        let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
        let remoto = EnSupabase {
            cliente: &cliente,
            proyecto: &proyecto,
        };
        let guardar = |filas: &[Usuario]| {
            estado.con_db(|db| usuarios::reemplazar_todos(db.conexion(), filas))
        };
        // Sin la copia local al día (primera vez en esta PC, o el rol cambió en el servidor) se rehace y
        // se vuelve a revisar una vez. El servidor igual revisa todo lo que se le pide.
        let yo = match estado.exigir_permiso(comando) {
            Err(crate::error::Error::SinPermiso) => {
                servidor::refrescar(&remoto, guardar)?;
                estado.exigir_permiso(comando)?
            }
            otro => otro?,
        };
        f(&remoto, &guardar, yo.rol)
    })())
}

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
    if let Some(r) = en_servidor(&estado, "listar_usuarios", |remoto, guardar, rol| {
        servidor::listar(
            remoto,
            guardar,
            || estado.con_db(|db| equipo::listar(db.conexion(), "", rol)),
            rol,
        )
    }) {
        return r;
    }
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
    if let Some(r) = en_servidor(&estado, "crear_usuario", |remoto, guardar, _| {
        servidor::crear(remoto, guardar, &email, rol, &hoy_local())
    }) {
        return r;
    }
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
    if let Some(r) = en_servidor(&estado, "cambiar_rol", |remoto, guardar, _| {
        servidor::cambiar_rol(remoto, guardar, &email, rol)
    }) {
        return r;
    }
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
    if let Some(r) = en_servidor(&estado, "confirmar_usuario", |_, _, _| {
        servidor::confirmar()
    }) {
        return r;
    }
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
    if let Some(r) = en_servidor(&estado, "eliminar_usuario", |remoto, guardar, _| {
        servidor::eliminar(remoto, guardar, &email)
    }) {
        return r;
    }
    firmando(
        &estado,
        "eliminar_usuario",
        passphrase,
        |c, ops, publica, firmante| {
            equipo::eliminar(c, ops, publica, firmante, &email, &ahora_iso())
        },
    )
}
