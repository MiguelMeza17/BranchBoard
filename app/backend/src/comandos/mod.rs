//! Comandos de Tauri que el frontend llama con `invoke`, agrupados por área.
//! Cada comando devuelve `Resultado<T>`; el error llega como `{ codigo, mensaje }`.

pub mod acceso;
pub mod actualizaciones;
pub mod chat;
pub mod cm;
pub mod cuenta;
pub mod datos;
pub mod diff;
pub mod documentos;
pub mod eventos;
pub mod identidad;
pub mod logs_unity;
pub mod notificaciones;
pub mod proyecto;
pub mod proyecto_local;
pub mod proyecto_servidor;
pub mod sistema;
pub mod sprints;
pub mod subir_proyecto;
pub mod tareas;
pub mod usuarios;
pub mod vistos;
pub mod warp;
