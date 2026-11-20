//! Integración con Unity Version Control: todo lo que invoca `cm` como subproceso. Issues #80 a #84.

pub mod ejecutor;
pub mod instalacion;
pub mod lectura;
pub mod proyectos;
pub mod sesion;
pub mod sso_navegador;

pub use ejecutor::{Comando, Salida};
