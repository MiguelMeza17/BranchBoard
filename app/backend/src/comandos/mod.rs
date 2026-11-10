//! Comandos de Tauri que el frontend llama con `invoke`, agrupados por área.
//! Cada comando devuelve `Resultado<T>`; el error llega como `{ codigo, mensaje }`.

pub mod actualizaciones;
pub mod datos;
pub mod identidad;
pub mod sistema;
