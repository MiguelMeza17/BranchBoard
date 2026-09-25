# BranchBoard — Seguridad

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre los aspectos de seguridad de la aplicación.

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
- **Control de versiones: Unity Version Control** (no Git genérico), integrado vía scripts Python que invocan el CLI `cm`.
- **Persistencia**: SQLite (archivo `.db` local, sincronizado a la nube para compartir entre el equipo).
- **Stack elegido**: Tauri + Vue. Fase 1 = app de escritorio (full desktop). Fase 2 (futuro) = versión web, con el mismo código gracias a Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — otro tablero distinto al Scrum (su diferencia exacta con el Scrum aún no está definida — pendiente de aclarar con el usuario).
- **Flujo de trabajo actual**: varias sesiones de Claude trabajando en paralelo (una en [BACKEND.md](./BACKEND.md), otra en [FRONTEND.md](./FRONTEND.md), esta en seguridad), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que otra sesión.

## Roles y permisos

El usuario mencionó 4 roles pero solo describió 3 — **falta confirmar cuál es el cuarto**. Los 3 definidos hasta ahora:

### Administrador
- Gestiona y administra todo tipo de usuarios
- Es el único que puede asignar el rol de **Líder** a otro usuario
- Acceso total al proyecto
- Único rol que puede **eliminar el proyecto completo** — acción crítica, tratar con cuidado (requiere confirmación fuerte en la UI, ver "Pendiente por definir")

### Líder (por debajo del Administrador)
- Puede crear usuarios (pero no puede crear ni ascender a otros Líderes)
- Administra las solicitudes de ingreso a la organización (aprobar/rechazar)
- Puede crear tareas, poner permisos en tareas, eliminar tareas de otros usuarios
- Puede ver todas las tareas del proyecto
- **No puede**: crear/gestionar Líderes, eliminar el proyecto

### Desarrollador
- Puede ver sus propias tareas y el avance general del proyecto
- Puede crear nuevas tareas
- Puede **modificar** una tarea propia solo si recibe una **autorización explícita** enviada como mensaje dentro de la app (pendiente definir quién la envía: ¿Líder, Administrador, o ambos?)
- No puede ver/modificar tareas que no son suyas
- No puede crear usuarios

### Matriz resumen

| Acción | Administrador | Líder | Desarrollador |
|---|---|---|---|
| Eliminar el proyecto | ✅ | ❌ | ❌ |
| Asignar rol de Líder | ✅ | ❌ | ❌ |
| Crear usuarios | ✅ | ✅ (no Líderes) | ❌ |
| Gestionar solicitudes de ingreso | ✅ | ✅ | ❌ |
| Crear tareas | ✅ | ✅ | ✅ |
| Poner permisos en tareas | ✅ | ✅ | ❌ |
| Eliminar tareas de otros | ✅ | ✅ | ❌ |
| Ver todas las tareas del proyecto | ✅ | ✅ | ❌ (solo las suyas) |
| Ver avance del proyecto | ✅ | ✅ | ✅ |
| Modificar tareas propias | ✅ | ✅ | ⚠️ solo con autorización explícita por mensaje |
| Modificar tareas de otros | ✅ | ✅ | ❌ |

## Alcance

- Permisos de Tauri: qué comandos/API del sistema (filesystem, shell/git) se exponen al frontend y con qué restricciones (allowlist de Tauri)
- Ejecución de comandos `cm` (Unity VC) desde la app: evitar inyección de comandos si algún dato (nombre de rama, nombre de tarea) viene de input del usuario
- Alcance del acceso a disco: limitar al workspace de Unity VC configurado, no acceso libre al filesystem
- Manejo de datos del equipo: cómo se autentican los usuarios (login) y se protegen los datos en tránsito (correo con clave de acceso, etc.)
- Control de acceso por rol (autorización): cada comando Tauri sensible (eliminar proyecto, crear líder, eliminar tarea de otro, aprobar solicitud) debe validar el rol del usuario que lo invoca, no solo ocultar el botón en la UI — la UI oculta, pero el backend es quien debe negar la acción aunque alguien intente saltarse la interfaz

## Pendiente por definir

- Confirmar el cuarto rol de usuario (el usuario mencionó 4, solo describió 3)
- Quién envía la autorización de modificación al Desarrollador (¿Líder, Administrador, o ambos?) y si es por tarea puntual o un permiso general
- Confirmación reforzada para "eliminar el proyecto" (ej. escribir el nombre del proyecto, doble confirmación) dado que es irreversible
- Validación de inputs que terminan en comandos `cm`/shell
- Configuración del allowlist de Tauri (capabilities) una vez se defina qué comandos del backend se necesitan
- Consideraciones de seguridad para la futura versión web (autenticación, permisos por usuario/equipo)
