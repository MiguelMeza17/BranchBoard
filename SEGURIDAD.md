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
- **Control de versiones: Unity Version Control** (no Git genérico), integrado desde el backend en **Rust directo** (sin Python) invocando el CLI `cm` como subproceso.
- **Persistencia**: SQLite (archivo `.db` local, sincronizado a la nube para compartir entre el equipo).
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri).
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — otro tablero distinto al Scrum (su diferencia exacta con el Scrum aún no está definida — pendiente de aclarar con el usuario).
- **Flujo de trabajo actual**: varias sesiones de Claude trabajando en paralelo (una en [BACKEND.md](./BACKEND.md), otra en [FRONTEND.md](./FRONTEND.md), esta en seguridad), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que otra sesión.

## Roles y permisos

4 roles confirmados: Administrador, Líder, Desarrollador y **Tester** (el 4to rol, antes pendiente de confirmar).

### Administrador
- Gestiona y administra todo tipo de usuarios, incluyendo **eliminar cualquier usuario** (Líder o Desarrollador) — ej. cuando alguien deja el equipo
- Es el único que puede asignar el rol de **Líder** a otro usuario
- Acceso total al proyecto
- Único rol que puede **eliminar el proyecto completo** — acción crítica, tratar con cuidado (requiere confirmación fuerte en la UI, ver "Pendiente por definir")

### Líder (por debajo del Administrador)
- Puede crear usuarios (pero no puede crear ni ascender a otros Líderes)
- Puede **eliminar usuarios** (ej. cuando alguien deja el equipo) — pendiente confirmar si puede eliminar a otro Líder o solo a Desarrolladores (por consistencia con "no puede gestionar Líderes", se asume que no)
- Administra las solicitudes de ingreso a la organización (aprobar/rechazar)
- Puede crear tareas, poner permisos en tareas, eliminar tareas de otros usuarios
- Puede ver todas las tareas del proyecto
- **No puede**: crear/gestionar Líderes, eliminar el proyecto

### Desarrollador
- Puede ver sus propias tareas y el avance general del proyecto
- **Puede crear nuevas tareas, pero no libremente**: la tarea nace en estado **"bloqueada"** (pendiente de aprobación) — no puede tomarse ni crear rama hasta que un Líder o Administrador la apruebe. La notificación de "hay una tarea por aprobar" llega **solo a Líderes y Administradores**, nunca a otros Desarrolladores.
- La aprobación **desbloquea** la tarea (pasa al backlog normal, ej. "por hacer") — no la mueve ni le crea la rama automáticamente; alguien todavía tiene que "tomarla" como cualquier otra tarea para que ahí sí se cree la rama.
- Puede **modificar** una tarea propia solo si recibe una **autorización explícita** enviada como mensaje dentro de la app (pendiente definir quién la envía: ¿Líder, Administrador, o ambos?)
- No puede ver/modificar tareas que no son suyas
- No puede crear usuarios

### Tester (4to rol)
- Puede ver **todas** las tareas del proyecto (a diferencia de Desarrollador)
- No crea tareas de producción (Scripting/Modelado/Animación/Imagen/Video/Música)
- Sí puede **crear reportes de Testing**: al revisar una tarea, documenta lo que encontró en el apartado [Testing](./PLANNING.md#testing), vinculado a la tarea revisada
- No puede modificar tareas de otros (solo reportar sobre ellas vía Testing)
- No puede crear usuarios ni gestionar el proyecto

### Matriz resumen

| Acción | Administrador | Líder | Desarrollador | Tester |
|---|---|---|---|---|
| Eliminar el proyecto | ✅ | ❌ | ❌ | ❌ |
| Asignar rol de Líder | ✅ | ❌ | ❌ | ❌ |
| Crear usuarios | ✅ | ✅ (no Líderes) | ❌ | ❌ |
| Eliminar usuarios | ✅ (cualquiera) | ✅ (no Líderes, a confirmar) | ❌ | ❌ |
| Gestionar solicitudes de ingreso | ✅ | ✅ | ❌ | ❌ |
| Crear tareas (producción) | ✅ (directo) | ✅ (directo) | ⚠️ (nace "bloqueada", requiere aprobación) | ❌ |
| Aprobar/desbloquear tarea creada por Desarrollador | ✅ | ✅ | ❌ | ❌ |
| Crear reporte de Testing | — | — | — | ✅ (exclusivo) |
| Poner permisos en tareas | ✅ | ✅ | ❌ | ❌ |
| Eliminar tareas de otros | ✅ | ✅ | ❌ | ❌ |
| Ver todas las tareas del proyecto | ✅ | ✅ | ❌ (solo las suyas) | ✅ |
| Ver avance del proyecto | ✅ | ✅ | ✅ | ✅ |
| Modificar tareas propias | ✅ | ✅ | ⚠️ solo con autorización explícita por mensaje | — (no crea tareas propias) |
| Modificar tareas de otros | ✅ | ✅ | ❌ | ❌ |

### Usuarios semilla (equipo VirusTF)

Basado en los miembros actuales del proyecto en Unity Cloud (identidad = email de `cm whoami`, ver [BACKEND.md](./BACKEND.md#identidad-de-usuario)):

| Nombre | Email | Rol en Unity Cloud | Rol en BranchBoard |
|---|---|---|---|
| Miguel Meza | miguellmezza123@gmail.com | Propietario | **Administrador** |
| jo-an | arteagacristobal957@gmail.com | Usuario | **Líder** |
| Sebas | johns.telagt@uqvirtual.edu.co | Usuario | **Desarrollador** |

Nota: el rol de Unity Cloud (Propietario/Usuario) no distingue Líder de Desarrollador — esa asignación es decisión del equipo, no se deriva automáticamente de Unity Cloud.

## Alcance

- Permisos de Tauri: qué comandos/API del sistema (filesystem, shell/git) se exponen al frontend y con qué restricciones (allowlist de Tauri)
- Ejecución de comandos `cm` (Unity VC) desde la app: evitar inyección de comandos si algún dato (nombre de rama, nombre de tarea) viene de input del usuario
- Alcance del acceso a disco: limitar al workspace de Unity VC configurado, no acceso libre al filesystem
- Manejo de datos del equipo: cómo se autentican los usuarios (login) y se protegen los datos en tránsito (correo con clave de acceso, etc.)
- Control de acceso por rol (autorización): cada comando Tauri sensible (eliminar proyecto, crear líder, eliminar tarea de otro, aprobar solicitud) debe validar el rol del usuario que lo invoca, no solo ocultar el botón en la UI — la UI oculta, pero el backend es quien debe negar la acción aunque alguien intente saltarse la interfaz

## Pendiente por definir

- Quién envía la autorización de modificación al Desarrollador (¿Líder, Administrador, o ambos?) y si es por tarea puntual o un permiso general
- Confirmación reforzada para "eliminar el proyecto" (ej. escribir el nombre del proyecto, doble confirmación) dado que es irreversible
- Validación de inputs que terminan en comandos `cm`/shell
- Configuración del allowlist de Tauri (capabilities) una vez se defina qué comandos del backend se necesitan
- Consideraciones de seguridad para la futura versión web (autenticación, permisos por usuario/equipo)
