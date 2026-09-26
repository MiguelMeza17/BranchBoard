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
  4. **Tablero "Avance"** — gráfica de curva en el tiempo (tareas resueltas por día + changesets por día).
  5. **"Branches"** — árbol de linaje de ramas (`dev` → tareas → subtareas anidadas), coloreado por estado.
  6. **"Revisión"** (antes "Merging") — página tipo estado de PR, lista las tareas en "en revisión" pendientes de autorizar/rechazar.
  7. **"Ajustes"** — agrupa gestión de usuarios, solicitudes de ingreso, backups y eliminar proyecto.
- **Flujo de trabajo actual**: varias sesiones de Claude trabajando en paralelo (una en [BACKEND.md](./BACKEND.md), otra en [FRONTEND.md](./FRONTEND.md), esta en seguridad), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que otra sesión.

## Roles y permisos

4 roles confirmados: Administrador, Líder, Desarrollador y **Tester** (el 4to rol, antes pendiente de confirmar).

### Administrador
- Gestiona y administra todo tipo de usuarios, incluyendo **eliminar cualquier usuario** (Líder o Desarrollador) — ej. cuando alguien deja el equipo
- Es el único que puede asignar el rol de **Líder** a otro usuario
- Acceso total al proyecto
- Único rol que puede **eliminar el proyecto completo** — **definido**: un escalón más estricto que eliminar una tarea (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), dado que borra absolutamente todo, no solo un elemento. En vez de escribir una palabra genérica como "eliminar", el Administrador debe **escribir el nombre exacto del proyecto** para confirmar (mismo patrón que usa GitHub al borrar un repositorio) — así se evita una confirmación "en automático" sin medir la gravedad.
- Único rol que puede **restaurar un backup de la base de datos** (ver [Copias de seguridad en BACKEND.md](./BACKEND.md#copias-de-seguridad-y-restauración-de-la-bd)) — Líder y Desarrollador solo pueden pedírselo vía notificación, no hacerlo ellos mismos.

### Líder (por debajo del Administrador)
- Puede crear usuarios (pero no puede crear ni ascender a otros Líderes)
- Puede **eliminar usuarios** (ej. cuando alguien deja el equipo), pero **solo Desarrolladores** — **definido**: no puede eliminar a otro Líder, esa acción es exclusiva del Administrador
- Administra las solicitudes de ingreso a la organización (aprobar/rechazar)
- Puede crear tareas, poner permisos en tareas, eliminar tareas de otros usuarios
- Puede ver todas las tareas del proyecto
- **No puede**: crear/gestionar Líderes, eliminar el proyecto

### Desarrollador
- Puede ver sus propias tareas y el avance general del proyecto
- **Puede crear nuevas tareas, pero no libremente**: la tarea nace en estado **"bloqueada"** (pendiente de aprobación) — no puede tomarse ni crear rama hasta que un Líder o Administrador la apruebe. La notificación de "hay una tarea por aprobar" llega **solo a Líderes y Administradores**, nunca a otros Desarrolladores.
- La aprobación **desbloquea** la tarea (pasa al backlog normal, ej. "por hacer") — no la mueve ni le crea la rama automáticamente; alguien todavía tiene que "tomarla" como cualquier otra tarea para que ahí sí se cree la rama.
- Puede **modificar** una tarea propia solo si recibe una **autorización explícita** enviada como mensaje dentro de la app — **definido**: la puede enviar tanto un **Líder** como un **Administrador**, cualquiera de los dos
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
| Merge de `dev` a `main` (si algún día se implementa en la app) | ✅ (exclusivo) | ❌ | ❌ | ❌ |
| Checkin directo sobre `main` o `dev` | ❌ (nadie, ni el Admin) | ❌ | ❌ | ❌ |
| Restaurar backup de la BD | ✅ | ❌ | ❌ | ❌ |
| Solicitar acción sobre la BD (notificación al Admin) | — | ✅ | ✅ | ❌ |
| Asignar rol de Líder | ✅ | ❌ | ❌ | ❌ |
| Crear usuarios | ✅ | ✅ (no Líderes) | ❌ | ❌ |
| Eliminar usuarios | ✅ (cualquiera) | ✅ (solo Desarrolladores) | ❌ | ❌ |
| Gestionar solicitudes de ingreso | ✅ | ✅ | ❌ | ❌ |
| Otorgar/revocar permiso de subtareas simples a colaborador externo | ✅ | ✅ | ❌ | ❌ |
| Crear tareas (producción) | ✅ (directo) | ✅ (directo) | ⚠️ (nace "bloqueada", requiere aprobación) | ❌ |
| Aprobar/desbloquear tarea creada por Desarrollador | ✅ | ✅ | ❌ | ❌ |
| Rechazar tarea (con motivo) | ✅ | ✅ | ❌ | ❌ |
| Asignar tarea directamente a un Desarrollador | ✅ | ✅ | ❌ | ❌ |
| Crear reporte de Testing | — | — | — | ✅ (exclusivo) |
| Poner permisos en tareas | ✅ | ✅ | ❌ | ❌ |
| Eliminar tareas de otros | ✅ | ✅ | ❌ | ❌ |
| Ver "Revisión" — carpeta "Tareas grandes" | ✅ | ✅ | ❌ (no la ve) | ❌ (no la ve) |
| Autorizar/rechazar — carpeta "Tareas grandes" (hacia `dev`) | ✅ | ✅ | ❌ | ❌ |
| Ver "Revisión" — carpeta "Tareas pequeñas" | ✅ | ✅ | ✅ | ✅ |
| Autorizar/rechazar — carpeta "Tareas pequeñas" (hacia la tarea padre) | ✅ | ✅ | ✅ (solo si es dueño de la tarea padre) | ❌ |
| Asignar la revisión de una tarea a un Líder específico | ✅ (exclusivo) | ❌ | ❌ | ❌ |
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
- Ejecución de comandos `cm` (Unity VC) desde la app: **resuelto**, ver [Seguridad al invocar cm en BACKEND.md](./BACKEND.md#seguridad-al-invocar-cm-evitar-inyección-de-comandos) — siempre pasar argumentos por separado a `Command`, nunca construir un string de shell con datos del usuario
- Alcance del acceso a disco: limitar al workspace de Unity VC configurado, no acceso libre al filesystem
- Manejo de datos del equipo: **resuelto**, no hay login propio, ni siquiera para el colaborador de subtareas simples — identidad vía `cm whoami` para **todos** los roles (ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario) y [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples)). El diseño anterior de Service Account/API REST se descartó por una limitación real de la plataforma (las Service Accounts no pueden recibir permisos de Version Control), así que no hay ninguna llave/secreto compartido que proteger.
- Control de acceso por rol (autorización): cada comando Tauri sensible (eliminar proyecto, crear líder, eliminar tarea de otro, aprobar solicitud) debe validar el rol del usuario que lo invoca, no solo ocultar el botón en la UI — la UI oculta, pero el backend es quien debe negar la acción aunque alguien intente saltarse la interfaz

## Pendiente por definir

- Si la autorización de modificación es por tarea puntual o un permiso general (quién la envía ya quedó definido: Líder o Administrador)
- Configuración del allowlist de Tauri (capabilities) una vez se defina qué comandos del backend se necesitan
- Consideraciones de seguridad para la futura versión web (autenticación, permisos por usuario/equipo)
