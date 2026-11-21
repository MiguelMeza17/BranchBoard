//! Pantalla Usuarios (issues #90 y #241): listar, agregar (o invitar), cambiar de rol y quitar.
//!
//! Con el proyecto en el servidor todo va por `identidad::equipo_servidor`: los roles los cambian las
//! funciones del servidor y la tabla `usuarios` es su copia. Desde #243 no hay firmas, así que un
//! proyecto que todavía va por Drive/OneDrive (sin `proyecto_servidor_id`) solo **lista** su equipo:
//! para cambiarlo hay que pasarlo al servidor (#245). Ya no existe "confirmar usuario".

use tauri::State;

use crate::comandos::documentos::proyecto_en_servidor;
use crate::db::usuarios::{self, Usuario};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::equipo::{self, UsuarioVisible};
use crate::identidad::equipo_servidor::{self as servidor, EnSupabase};
use crate::modelo::{hoy_local, Rol};
use crate::servidor::pat::{Llavero, LlaveroSistema};
use crate::supabase::{ClienteSupabase, Entorno};

/// Con el proyecto en el servidor (#241), corre `f` con el equipo remoto y la función que guarda la copia
/// local; si no, `None`. La red va fuera del bloqueo de la base.
fn en_servidor<T, L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    comando: &str,
    f: impl FnOnce(&EnSupabase<'_, L>, &dyn Fn(&[Usuario]) -> Resultado<()>, Rol) -> Resultado<T>,
) -> Option<Resultado<T>> {
    let proyecto = match proyecto_en_servidor(estado) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => return Some(Err(e)),
    };
    Some((|| {
        let remoto = EnSupabase {
            cliente,
            proyecto: &proyecto,
        };
        let guardar = |filas: &[Usuario]| {
            estado.con_db(|db| {
                if crate::supabase::proyectos::proyecto_guardado(db.conexion())?.as_deref()
                    != Some(proyecto.as_str())
                {
                    return Err(Error::Validacion(
                        "El proyecto abierto cambió durante la operación".into(),
                    ));
                }
                usuarios::reemplazar_todos(db.conexion(), filas)
            })
        };
        // Sin la copia local al día (primera vez en esta PC, o el rol cambió en el servidor) se rehace y
        // se vuelve a revisar una vez. El servidor igual revisa todo lo que se le pide.
        let yo = match estado.exigir_permiso(comando) {
            Err(Error::SinPermiso) => {
                servidor::refrescar(&remoto, guardar)?;
                estado.exigir_permiso(comando)?
            }
            otro => otro?,
        };
        f(&remoto, &guardar, yo.rol)
    })())
}

/// Sin servidor (proyecto por Drive/OneDrive) el equipo ya no se cambia (#243).
fn solo_con_servidor() -> Error {
    Error::Validacion(
        "Para cambiar el equipo de este proyecto, primero pásalo al servidor de BranchBoard".into(),
    )
}

/// Todos los del proyecto. Sin correos para quien no es Líder ni Administrador.
#[tauri::command]
pub fn listar_usuarios(estado: State<'_, AppState>) -> Resultado<Vec<UsuarioVisible>> {
    listar_en(
        &estado,
        &ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema),
    )
}

pub fn listar_en<L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
) -> Resultado<Vec<UsuarioVisible>> {
    if let Some(r) = en_servidor(
        estado,
        cliente,
        "listar_usuarios",
        |remoto, guardar, rol| {
            servidor::listar(
                remoto,
                guardar,
                || {
                    // El refresco puede haber cambiado el rol propio o quitado nuestra membresía.
                    let yo = estado.exigir_permiso("listar_usuarios")?;
                    estado.con_db(|db| equipo::listar_para(db.conexion(), yo.rol, Some(&yo.email)))
                },
                rol,
            )
        },
    ) {
        return r;
    }
    let yo = estado.exigir_permiso("listar_usuarios")?;
    estado.con_db(|db| equipo::listar_para(db.conexion(), yo.rol, Some(&yo.email)))
}

#[tauri::command]
pub fn crear_usuario(estado: State<'_, AppState>, email: String, rol: Rol) -> Resultado<Usuario> {
    crear_en(
        &estado,
        &ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema),
        &email,
        rol,
    )
}

pub fn crear_en<L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    email: &str,
    rol: Rol,
) -> Resultado<Usuario> {
    en_servidor(estado, cliente, "crear_usuario", |remoto, guardar, _| {
        servidor::crear(remoto, guardar, email, rol, &hoy_local())
    })
    .unwrap_or_else(|| Err(solo_con_servidor()))
}

#[tauri::command]
pub fn cambiar_rol(estado: State<'_, AppState>, email: String, rol: Rol) -> Resultado<()> {
    cambiar_rol_en(
        &estado,
        &ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema),
        &email,
        rol,
    )
}

pub fn cambiar_rol_en<L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    email: &str,
    rol: Rol,
) -> Resultado<()> {
    en_servidor(estado, cliente, "cambiar_rol", |remoto, guardar, _| {
        servidor::cambiar_rol(remoto, guardar, email, rol)
    })
    .unwrap_or_else(|| Err(solo_con_servidor()))
}

/// Lo quita del proyecto (pierde el acceso de inmediato) o cancela su invitación.
#[tauri::command]
pub fn eliminar_usuario(estado: State<'_, AppState>, email: String) -> Resultado<()> {
    eliminar_en(
        &estado,
        &ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema),
        &email,
    )
}

pub fn eliminar_en<L: Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    email: &str,
) -> Resultado<()> {
    en_servidor(estado, cliente, "eliminar_usuario", |remoto, guardar, _| {
        servidor::eliminar(remoto, guardar, email)
    })
    .unwrap_or_else(|| Err(solo_con_servidor()))
}
