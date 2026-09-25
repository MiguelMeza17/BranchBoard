# BranchBoard — Backend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que corre del lado de Tauri (Rust) / lógica de datos.

## Alcance

- Modelo de datos de una tarea (código, 3D, 2D, música, video)
- Persistencia local (¿SQLite? ¿archivo JSON?)
- Integración con Git: leer ramas del repo del proyecto Unity, crear rama al crear tarea
- Comandos Tauri expuestos al frontend

## Pendiente por definir

- Motor de almacenamiento
- Estructura exacta de la tarea (campos)
- Cómo se detecta/vincula el repo Git del proyecto Unity
- Estados del flujo de una tarea
