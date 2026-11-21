//! Comandos de la cuenta de BranchBoard (#236): registrarse, iniciar y cerrar sesión con Supabase Auth.
//!
//! La sesión queda en el llavero del sistema (`supabase::ClienteSupabase`); al frontend solo le llega
//! nombre, apellido y correo, nunca los tokens.

use serde::Serialize;

use crate::error::Resultado;
use crate::servidor::LlaveroSistema;
use crate::supabase::{ClienteSupabase, Entorno, Sesion};

/// Lo que el frontend sabe de la cuenta (`Cuenta` en `lib/cuentas.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cuenta {
    pub nombre: String,
    pub apellido: String,
    pub correo: String,
}

impl From<Sesion> for Cuenta {
    fn from(s: Sesion) -> Self {
        Cuenta {
            nombre: s.nombre,
            apellido: s.apellido,
            correo: s.correo,
        }
    }
}

fn cliente() -> ClienteSupabase<LlaveroSistema> {
    ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema)
}

#[tauri::command]
pub async fn registrar_cuenta(
    nombre: String,
    apellido: String,
    correo: String,
    clave: String,
) -> Resultado<Cuenta> {
    Ok(cliente()
        .registrar(&nombre, &apellido, &correo, &clave)?
        .into())
}

#[tauri::command]
pub async fn iniciar_sesion_branchboard(correo: String, clave: String) -> Resultado<Cuenta> {
    Ok(cliente().iniciar_sesion(&correo, &clave)?.into())
}

/// La cuenta con sesión guardada en este equipo, sin tocar la red: al abrir la app se entra directo.
#[tauri::command]
pub async fn sesion_branchboard() -> Resultado<Option<Cuenta>> {
    Ok(cliente().sesion_guardada()?.map(Cuenta::from))
}

#[tauri::command]
pub async fn cerrar_sesion_branchboard() -> Resultado<()> {
    cliente().cerrar_sesion()
}
