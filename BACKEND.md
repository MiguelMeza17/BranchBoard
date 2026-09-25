# BranchBoard — Backend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que corre del lado de Tauri (Rust) / lógica de datos.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Cada tarea puede ser de tipo:
  - Código → asociada 1 a 1 a una rama de Git (una tarea = una rama)
  - Modelado 3D
  - Arte 2D
  - Música
  - Video
- **Stack elegido**: Tauri + Vue. Fase 1 = app de escritorio (full desktop). Fase 2 (futuro) = versión web, con el mismo código gracias a Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — otro tablero distinto al Scrum (su diferencia exacta con el Scrum aún no está definida — pendiente de aclarar con el usuario).
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (backend) y otra en [FRONTEND.md](./FRONTEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

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
