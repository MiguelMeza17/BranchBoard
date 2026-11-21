//! Servidor de BranchBoard en Supabase (#231; hoja de ruta #248). Ver "Servidor (Supabase)" en BACKEND.md.
//!
//! - `entorno` — a qué proyecto de Supabase habla la app (pruebas en desarrollo, producción en el instalador).
//! - `cliente` — iniciar, renovar y cerrar sesión (guardada en el llavero) y llamadas autenticadas (#235).

pub mod cliente;
pub mod entorno;

pub use cliente::{ClienteSupabase, Sesion};
pub use entorno::{Entorno, ProyectoSupabase};
