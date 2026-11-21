//! Servidor de BranchBoard en Supabase (#231; hoja de ruta #248). Ver "Servidor (Supabase)" en BACKEND.md.
//!
//! - `entorno` — a qué proyecto de Supabase habla la app (pruebas en desarrollo, producción en el instalador).
//! - `cliente` — iniciar, renovar y cerrar sesión (guardada en el llavero) y llamadas autenticadas (#235).
//! - `login_unity` — al iniciar sesión con Unity VC, abrir también la del servidor (#237).

pub mod cliente;
pub mod entorno;
pub mod login_unity;

pub use cliente::{ClienteSupabase, Sesion};
pub use entorno::{Entorno, ProyectoSupabase};
