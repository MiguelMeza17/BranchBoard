//! Servidor de BranchBoard en Supabase (#231; hoja de ruta #248). Ver "Servidor (Supabase)" en BACKEND.md.
//!
//! - `entorno` — a qué proyecto de Supabase habla la app (pruebas en desarrollo, producción en el instalador).

pub mod entorno;

pub use entorno::{Entorno, ProyectoSupabase};
