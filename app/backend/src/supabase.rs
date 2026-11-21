//! Servidor de BranchBoard en Supabase (#231; hoja de ruta #248). Ver "Servidor (Supabase)" en BACKEND.md.
//!
//! - `entorno` — a qué proyecto de Supabase habla la app (pruebas en desarrollo, producción en el instalador).
//! - `cliente` — iniciar, renovar y cerrar sesión (guardada en el llavero) y llamadas autenticadas (#235).
//! - `login_unity` — al iniciar sesión con Unity VC, abrir también la del servidor (#237).
//! - `storage` — documentos e imagen del proyecto en Supabase Storage (#242).
//! - `miembros` — usuarios, roles e invitaciones del proyecto en el servidor (#241).
//! - `proyectos` — vincular el proyecto de Unity VC abierto con el servidor y listar los de la cuenta (#240).

pub mod cliente;
pub mod entorno;
pub mod login_unity;
pub mod miembros;
pub mod proyectos;
pub mod storage;

pub use cliente::{ClienteSupabase, Sesion};
pub use entorno::{Entorno, ProyectoSupabase};
