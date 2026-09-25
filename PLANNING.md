# BranchBoard

Herramienta de gestión de tareas y producción para equipos pequeños de desarrollo de videojuegos en Unity.

## Visión

Todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Una tarea puede ser:
- Código → asociada a una rama de Git (una tarea = una rama)
- Modelado 3D
- Arte 2D
- Música
- Video

Cada tarea, sin importar su tipo, pasa por los mismos estados y se ve reflejada en los mismos tableros.

## Stack

- **Framework**: Tauri + Vue
- **Fase 1**: aplicación de escritorio (full desktop)
- **Fase 2** (futuro): versión web

## Usuarios

Equipo pequeño de desarrollo de un proyecto Unity.

## Pantallas / tableros del MVP

1. **Tablero Scrum** — vista tipo kanban de las tareas
2. **Resumen** — gráfica circular (dona) con el conteo de tareas: por hacer, en producción, en revisión, terminadas
3. **Lista de tareas** — todas las tareas existentes, lugar donde se crean
4. **Tablero "Avance"** — (pendiente de definir en qué se diferencia del tablero Scrum)

## Pendiente por definir

- Campos de una tarea (título, tipo, rama asociada, estado, asignado, etc.)
- Diferencia entre tablero Scrum y tablero Avance
- Cómo se gestionan los assets (3D/2D/música/video) que no son código — ¿tienen rama asociada o solo archivo/link?
- Estados exactos del flujo (por hacer → en producción → en revisión → terminada, ¿alguno más?)
- Integración con Git: ¿lee un repo local? ¿crea la rama automáticamente al crear la tarea?
