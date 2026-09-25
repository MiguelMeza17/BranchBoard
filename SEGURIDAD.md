# BranchBoard — Seguridad

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre los aspectos de seguridad de la aplicación.

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
- **Flujo de trabajo actual**: varias sesiones de Claude trabajando en paralelo (una en [BACKEND.md](./BACKEND.md), otra en [FRONTEND.md](./FRONTEND.md), esta en seguridad), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que otra sesión.

## Alcance

- Permisos de Tauri: qué comandos/API del sistema (filesystem, shell/git) se exponen al frontend y con qué restricciones (allowlist de Tauri)
- Ejecución de comandos Git desde la app: evitar inyección de comandos si algún dato (nombre de rama, nombre de tarea) viene de input del usuario
- Alcance del acceso a disco: limitar a la carpeta del proyecto Unity/repo configurado, no acceso libre al filesystem
- Manejo de datos del equipo: si en el futuro hay sync/versión web, cómo se autentican los usuarios y se protegen los datos en tránsito

## Pendiente por definir

- Si la app v1 (solo escritorio, un solo usuario local) necesita autenticación o no
- Validación de inputs que terminan en comandos de shell/git
- Configuración del allowlist de Tauri (capabilities) una vez se defina qué comandos del backend se necesitan
- Consideraciones de seguridad para la futura versión web (autenticación, permisos por usuario/equipo)
