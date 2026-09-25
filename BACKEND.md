# BranchBoard — Backend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que corre del lado de Tauri (Rust) / lógica de datos.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Cada tarea puede ser de tipo:
  - **Scripting** → asociada a una rama (una tarea = una rama)
  - **Modelado** (3D)
  - **Animación**
  - **Imagen** (arte 2D)
  - **Video**
  - **Música**
- **Control de versiones: Unity Version Control** (antes Plastic SCM), no Git genérico. Se usa su CLI (`cm`). La integración desde el backend se hace mediante **scripts Python** que invocan `cm`.
- **Flujo de vida de una tarea**: se crea visual/sin rama → al "tomarla" alguien, el backend crea la rama en Unity VC vía script Python → el sprint es el tiempo que esa rama vive sobre `dev` antes de mergear a `main`.
- **Stack elegido**: Tauri + Vue. Fase 1 = app de escritorio (full desktop). Fase 2 (futuro) = versión web, con el mismo código gracias a Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — otro tablero distinto al Scrum (su diferencia exacta con el Scrum aún no está definida — pendiente de aclarar con el usuario).
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (backend) y otra en [FRONTEND.md](./FRONTEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite** (un solo archivo `.db`, sin servidor). Fase 1 = archivo local; el archivo `.db` se sincroniza a la nube (ej. Drive/OneDrive) para compartirlo entre el equipo. Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de varios usuarios.
- Integración con Unity Version Control vía scripts Python que invocan el CLI `cm`: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios)
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`)

## Detección de cambios en Unity Version Control (polling)

Problema a resolver: un compañero sube un asset (ej. un modelo 3D) desde Unity directamente a una rama, y esa tarea debe reflejarse como "modificada" en BranchBoard.

**Conceptos**: cada checkin en Unity VC genera un **changeset** (número secuencial) en la rama donde se hizo. El CLI `cm` permite consultar el historial de una rama.

**Flujo propuesto (opción simple, sin triggers de servidor)**:

1. Al crear la rama de una tarea (cuando alguien la "toma"), el backend guarda `ultimo_changeset_visto` para esa tarea (el changeset que existía en ese momento).
2. El usuario dispara una revisión manual desde el frontend (botón "Refrescar", comando Tauri `revisar_actualizaciones(tarea_id)`) — no hace falta un proceso en segundo plano para el MVP.
3. El script Python corre algo como `cm log br:/<rama> --xml` y obtiene el changeset más reciente de esa rama.
4. Si el changeset más reciente es mayor al `ultimo_changeset_visto` guardado → hay cambios nuevos:
   - Marca la tarea como "modificada" (para que el frontend le ponga un badge/indicador)
   - Actualiza `ultimo_changeset_visto`
   - Opcional: extrae metadata del changeset (autor, comentario, archivos afectados) para mostrarla en el detalle de la tarea
5. (Fase futura) Reemplazar el polling manual por **triggers del servidor de Unity VC** (ej. `after-checkin`) para detección casi en tiempo real, sin depender de que alguien le dé clic a "Refrescar".

## Roles y permisos

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 3 roles confirmados (Administrador, Líder, Desarrollador; falta un 4to por confirmar). El modelo de datos de usuario necesita un campo de rol, y cada comando Tauri sensible debe validar permisos en el backend (no solo ocultar en la UI).

## Pendiente por definir

- Estructura exacta de la tarea (campos)
- Estructura exacta de usuario (rol, y cómo se valida el permiso en cada comando Tauri)
- Cómo se detecta/vincula el workspace de Unity VC del proyecto Unity (ruta local, nombre del repo)
- Estados del flujo de una tarea
- Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)
- Dónde vive `ultimo_changeset_visto` por tarea (misma base de datos del modelo de tarea)
