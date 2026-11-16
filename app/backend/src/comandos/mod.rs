//! Comandos de Tauri que el frontend llama con `invoke`, agrupados por área.
//! Cada comando devuelve `Resultado<T>`; el error llega como `{ codigo, mensaje }`.

pub mod acceso;
pub mod actualizaciones;
pub mod chat;
pub mod datos;
pub mod documentos;
pub mod eventos;
pub mod identidad;
pub mod notificaciones;
pub mod sistema;
pub mod sprints;
pub mod tareas;
pub mod usuarios;
