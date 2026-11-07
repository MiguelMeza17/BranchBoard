# BranchBoard — Seguridad

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre los aspectos de seguridad de la aplicación.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Cada tarea puede ser de tipo (ver [BACKEND.md](./BACKEND.md#contexto-completo-del-proyecto)):
  - **Scripting** → asociada a una rama (una tarea = una rama)
  - **Modelado 3D**
  - **Modelado 2D**
  - **Animación**
  - **Imagen** (arte 2D)
  - **UI/Interfaz**
  - **Audio**
  - **Música**
  - **Video**
- **Control de versiones: Unity Version Control** (no Git genérico), integrado desde el backend en **Rust directo** (sin Python) invocando el CLI `cm` como subproceso.
- **Persistencia**: SQLite local por máquina (no compartido directamente) — lo que se sincroniza a la nube es un log de eventos, ver [Sincronización entre máquinas en BACKEND.md](./BACKEND.md#sincronización-entre-máquinas-arquitectura-de-eventos).
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri).
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP**: ver lista completa y autoritativa en [Pantallas / tableros del MVP en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp) — no se duplica aquí para evitar que este documento quede desactualizado cuando se agreguen pantallas nuevas.
- **Flujo de trabajo actual**: varias sesiones de Claude trabajando en paralelo (una en [BACKEND.md](./BACKEND.md), otra en [FRONTEND.md](./FRONTEND.md), esta en seguridad), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que otra sesión.

## Roles y permisos

5 roles: Administrador, Líder, Desarrollador, **Tester** y **Colaborador externo** (nuevo 2026-11-07).

### Administrador
- Gestiona y administra todo tipo de usuarios, incluyendo **eliminar cualquier usuario** (Líder o Desarrollador) — ej. cuando alguien deja el equipo
- Es el único que puede asignar el rol de **Líder** a otro usuario, o **degradarlo**/reasignar entre Desarrollador, Tester y Colaborador externo (`cambiar_rol`, que reemplaza a `ascender_a_lider`) — ver [BACKEND.md](./BACKEND.md#comandos-de-usuarios-y-ajustes)
- Acceso total al proyecto
- Único rol que puede **eliminar el proyecto completo** — **definido**: un escalón más estricto que eliminar una tarea (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), dado que borra absolutamente todo, no solo un elemento. En vez de escribir una palabra genérica como "eliminar", el Administrador debe **escribir el nombre exacto del proyecto** para confirmar (mismo patrón que usa GitHub al borrar un repositorio) — así se evita una confirmación "en automático" sin medir la gravedad.
- Único rol que puede **restaurar un backup de la base de datos** (ver [Copias de seguridad en BACKEND.md](./BACKEND.md#copias-de-seguridad-y-restauración-de-la-bd)) — Líder y Desarrollador solo pueden pedírselo vía notificación, no hacerlo ellos mismos.
- Si toma y trabaja una tarea general él mismo, puede **mergearla directo a `dev`** al terminar, sin pasar por "en revisión" — ya tiene permiso de autorizar, ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)

### Líder (por debajo del Administrador)
- Puede crear usuarios y cambiar roles entre Desarrollador, Tester y Colaborador externo (no crea ni asciende Líderes ni Administradores). Lo que hace queda "Pendiente de confirmar" hasta que el Administrador lo firme
- Puede **eliminar usuarios** (ej. cuando alguien deja el equipo), pero **solo Desarrolladores y Colaboradores externos** — **definido**: no puede eliminar a otro Líder, esa acción es exclusiva del Administrador
- Puede crear tareas, poner permisos en tareas, eliminar tareas de otros usuarios
- Puede ver todas las tareas del proyecto
- Si toma y trabaja una tarea general él mismo, puede **mergearla directo a `dev`** al terminar, sin pasar por "en revisión" — mismo caso que el Administrador, ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)
- **No puede**: crear/gestionar Líderes, eliminar el proyecto

### Desarrollador
- Puede ver sus propias tareas y el avance general del proyecto
- **Puede crear nuevas tareas y subtareas, pero quedan pendientes de aprobación** (`pendiente_aprobacion = true`, **corregido** — ya no existe un estado "bloqueada" aparte): la tarea sigue su curso normal desde ya (aparece en "por hacer", se puede tomar y crear su rama sin esperar nada) — lo único que no puede hacer mientras tanto es **mandarla a revisión**. La notificación de "hay una tarea por aprobar" llega **solo a Líderes y Administradores**, nunca a otros Desarrolladores, y también queda listada en el apartado [Pendientes](./PLANNING.md#pendientes).
- La aprobación solo quita esa bandera — no toca el `estado` de la tarea, que ya venía avanzando por su cuenta (por hacer o en producción, según si alguien ya la había tomado)
- **Corregido, flujo invertido**: puede **modificar** una tarea propia ya aprobada en cualquier momento, sin pedir permiso antes — el cambio se aplica de inmediato, pero queda marcado como **edición pendiente** (sombra amarilla en la tarjeta, también listada en [Pendientes](./PLANNING.md#pendientes)) hasta que un Líder/Administrador la confirme o la revierta (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 4). Si el Líder/Admin la rechaza, el campo vuelve a su valor anterior. **Excepción**: mientras la tarea sigue `pendiente_aprobacion = true` (recién creada, sin aprobar todavía), la edita libremente sin que quede marcada como pendiente — es su propia tarea todavía sin confirmar por nadie.
- **No puede modificar** tareas que no son suyas, y en los tableros principales (Scrum, Lista de tareas, Resumen, Avance) **solo ve las suyas** — **corregido/acotado**: esta regla no aplica a las pantallas que ya están explícitamente documentadas como visibles para todos los roles, donde sí ve tareas y actividad de otras personas en modo lectura: [Revisión](./PLANNING.md#revisión) ("Tareas grandes", solo lectura para Desarrollador), [Papelera](./PLANNING.md#alcance) (solo lectura), [Branches](./PLANNING.md#pantallas--tableros-del-mvp) (árbol completo de ramas del proyecto) y [Reportes](./PLANNING.md#reportes) (desglose de actividad de todo el equipo). En ninguna de esas cuatro puede modificar nada ajeno, solo verlo.
- No puede crear usuarios
- Al terminar de trabajar una tarea, solo puede **mandarla a revisión** — nunca puede mergearla/conectarla él mismo a `dev`, eso es exclusivo de Líder/Administrador (ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev))

### Tester (4to rol)
- Puede ver **todas** las tareas del proyecto (a diferencia de Desarrollador)
- **Corregido**: sí puede **crear tareas de producción** (Scripting/Modelado/Animación/Imagen/Video/Música), con la misma restricción que el Desarrollador — quedan `pendiente_aprobacion = true` hasta que un Líder/Administrador las apruebe desde [Pendientes](./PLANNING.md#pendientes), ver arriba. Mismo caso que el Desarrollador: puede editarlas libremente mientras siguen pendientes de aprobación, sin pedir autorización
- Sí puede **crear reportes de Testing**: al revisar una tarea, documenta lo que encontró en el apartado [Testing](./PLANNING.md#testing), vinculado a la tarea revisada
- No puede modificar tareas de otros (solo reportar sobre ellas vía Testing)
- No puede crear usuarios ni gestionar el proyecto

### Colaborador externo

**Nuevo (2026-11-07)**: reemplaza la lista aparte `colaboradores_externos`. Es alguien que sube archivos puntuales (imagen, audio, video, animación, `.fbx`) en subtareas simples, con workspace parcial (Gluon); no descarga el proyecto completo ni necesita Unity Editor.
- Tiene las mismas restricciones que Desarrollador/Tester en lo que comparten (sus tareas quedan pendientes de aprobación, no termina tareas directo).
- Lo crea o le cambia el rol un Líder o el Administrador desde [Usuarios](./FRONTEND.md#usuarios); también puede pasar a otro rol si el Líder/Administrador lo decide.
- **Pendiente** (lo revisa la auditoría de backend): qué pantallas ve exactamente; hoy en el mockup ve lo mismo que un Desarrollador.

### Matriz resumen

| Acción | Administrador | Líder | Desarrollador | Tester |
|---|---|---|---|---|
| Eliminar el proyecto | ✅ | ❌ | ❌ | ❌ |
| Merge de `dev` a `main` (si algún día se implementa en la app) | ✅ (exclusivo) | ❌ | ❌ | ❌ |
| Checkin directo sobre `main` o `dev` | ❌ (nadie, ni el Admin) | ❌ | ❌ | ❌ |
| Restaurar backup de la BD | ✅ | ❌ | ❌ | ❌ |
| Solicitar acción sobre la BD (notificación al Admin) | — | ✅ | ✅ | ❌ |
| Asignar rol de Líder | ✅ | ❌ | ❌ | ❌ |
| Crear usuarios | ✅ | ✅ (no Líderes ni Administradores) | ❌ | ❌ |
| Eliminar usuarios | ✅ (cualquiera) | ✅ (Desarrolladores y Colaboradores externos) | ❌ | ❌ |
| Cambiar rol (entre Desarrollador, Tester y Colaborador externo) | ✅ | ✅ (queda pendiente de confirmar) | ❌ | ❌ |
| Ver la lista de usuarios (nombre y rol) | ✅ | ✅ | ✅ | ✅ |
| Crear tareas (producción) | ✅ (directo) | ✅ (directo) | ⚠️ (queda pendiente de aprobación, ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)) | ⚠️ (mismo caso que Desarrollador) |
| Aprobar tarea pendiente (creada por Desarrollador o Tester) | ✅ | ✅ | ❌ | ❌ |
| Editar y aprobar una tarea pendiente en un solo paso | ✅ | ✅ | ❌ | ❌ |
| Advertir sobre una tarea pendiente (mensaje, sin modificarla) | ✅ | ✅ | ❌ | ❌ |
| Editar tarea propia mientras sigue pendiente de aprobación | — | — | ✅ | ✅ |
| Rechazar tarea en revisión o por conflicto de merge (con motivo, vuelve al Desarrollador) | ✅ | ✅ | ❌ | ❌ |
| Asignar tarea directamente a un Desarrollador | ✅ | ✅ | ❌ | ❌ |
| Crear reporte de Testing | — | — | — | ✅ (exclusivo) |
| Poner permisos en tareas | ✅ | ✅ | ❌ | ❌ |
| Eliminar tareas de otros | ✅ | ✅ | ❌ | ❌ |
| Ver "Revisión" — carpeta "Tareas grandes" | ✅ | ✅ | 👁️ solo lectura | 👁️ solo lectura |
| Autorizar/rechazar — carpeta "Tareas grandes" (hacia `dev`) | ✅ | ✅ | ❌ | ❌ |
| Mergear su propia tarea directo a `dev`, sin pasar por "en revisión" (solo si él mismo la trabajó) | ✅ | ✅ | ❌ (siempre debe mandar a revisión) | ❌ (**corregido**: sí puede trabajar tareas grandes — ej. su propia tarea de corrección de Testing — siempre debe mandar a revisión, mismo trato que Desarrollador) |
| Ver "Revisión" — carpeta "Tareas pequeñas" | ✅ | ✅ | ✅ | ✅ |
| Autorizar/rechazar — carpeta "Tareas pequeñas" (hacia la tarea padre) | ✅ | ✅ | ✅ (solo si es dueño de la tarea padre) | ❌ |
| Asignar la revisión de una tarea a otro Líder específico | ✅ (exclusivo) | ❌ | ❌ | ❌ |
| Auto-asignarse la revisión de una tarea ("Asignármela") | ✅ | ✅ | ❌ | ❌ |
| Ver todas las tareas del proyecto (tableros principales: Scrum, Lista de tareas, Resumen, Avance) | ✅ | ✅ | ❌ (solo las suyas) | ✅ |
| Ver Revisión / Papelera / Branches / Reportes (con tareas y actividad de todo el equipo, solo lectura para quien no autoriza) | ✅ | ✅ | ✅ (solo lectura) | ✅ |
| Ver avance del proyecto | ✅ | ✅ | ✅ | ✅ |
| Modificar tareas propias ya aprobadas | ✅ | ✅ | ⚠️ se aplica de inmediato, pero queda como edición pendiente hasta que Líder/Admin la confirme | ⚠️ mismo caso que Desarrollador |
| Aprobar/rechazar una edición pendiente de otra persona | ✅ | ✅ | ❌ | ❌ |
| Modificar tareas de otros (comando `editar_tarea_ajena`, ver [BACKEND.md](./BACKEND.md#notificaciones)) | ✅ | ✅ | ❌ | ❌ |
| Ver "Papelera" | ✅ | ✅ | 👁️ solo lectura | 👁️ solo lectura |
| Eliminar definitivamente desde "Papelera" | ✅ | ✅ | ❌ | ❌ |
| Mover cambios entre ramas propias | ✅ | ✅ | ✅ | ✅ |
| Mover cambios entre ramas de dos personas distintas | ✅ (exclusivo de Líder/Admin) | ✅ (exclusivo de Líder/Admin) | ❌ | ❌ |

### Usuarios semilla (equipo VirusTF)

Basado en los miembros actuales del proyecto en Unity Cloud (identidad = email de `cm whoami`, ver [BACKEND.md](./BACKEND.md#identidad-de-usuario)):

| Nombre | Email | Rol en Unity Cloud | Rol en BranchBoard |
|---|---|---|---|
| Miguel Meza | miguellmezza123@gmail.com | Propietario | **Administrador** |
| jo-an | arteagacristobal957@gmail.com | Usuario | **Líder** |
| Sebas | johns.telagt@uqvirtual.edu.co | Usuario | **Desarrollador** |

Nota: el rol de Unity Cloud (Propietario/Usuario) no distingue Líder de Desarrollador — esa asignación es decisión del equipo, no se deriva automáticamente de Unity Cloud.

## Alcance

- Permisos de Tauri: qué comandos/API del sistema (filesystem, shell/git) se exponen al frontend y con qué restricciones — **resuelto**, ver [Allowlist de capabilities de Tauri](#allowlist-de-capabilities-de-tauri-resuelto)
- Ejecución de comandos `cm` (Unity VC) desde la app: **resuelto**, ver [Seguridad al invocar cm en BACKEND.md](./BACKEND.md#seguridad-al-invocar-cm-evitar-inyección-de-comandos) — siempre pasar argumentos por separado a `Command`, nunca construir un string de shell con datos del usuario
- Alcance del acceso a disco: limitar al workspace de Unity VC configurado, no acceso libre al filesystem
- Manejo de datos del equipo: **resuelto**, no hay login propio, ni siquiera para el colaborador de subtareas simples — identidad vía `cm whoami` para **todos** los roles (ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario) y [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples)). El diseño anterior de Service Account/API REST se descartó por una limitación real de la plataforma (las Service Accounts no pueden recibir permisos de Version Control), así que no hay ninguna llave/secreto compartido que proteger.
- Control de acceso por rol (autorización): cada comando Tauri sensible (eliminar proyecto, crear líder, eliminar tarea de otro, crear usuario o cambiar rol) debe validar el rol del usuario que lo invoca, no solo ocultar el botón en la UI — la UI oculta, pero el backend es quien debe negar la acción aunque alguien intente saltarse la interfaz
- **Resuelto (hallazgo C1 de auditoría)**: el `rol` en la tabla `usuarios` no bastaba como ancla de confianza porque el `.db` se sincroniza por Drive/OneDrive con lectura/escritura para todo el equipo — cualquiera podía editarlo a mano. Ver [Firma criptográfica del rol en BACKEND.md](./BACKEND.md#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac): cada fila de rol se firma con Ed25519 (llave privada solo en la máquina del Administrador, nunca sincronizada; llave pública embebida en la app) y se verifica al leer. Adicionalmente, **eliminar el proyecto** exige también permiso real `rmrepository` en Unity Version Control (ver [Verificación adicional sobre Unity Version Control en BACKEND.md](./BACKEND.md#verificación-adicional-sobre-unity-version-control-para-eliminar-el-proyecto)) — doble candado, no solo la firma local.

## Allowlist de capabilities de Tauri (resuelto)

Enumerado a partir de todo lo ya documentado en [BACKEND.md](./BACKEND.md) — nada de acceso "abierto" a shell o filesystem, solo lo que cada funcionalidad ya descrita necesita:

**Subprocesos (shell)** — solo estos binarios, siempre invocados con argumentos en array separado, nunca con un string armado a mano (ver [Seguridad al invocar cm en BACKEND.md](./BACKEND.md#seguridad-al-invocar-cm-evitar-inyección-de-comandos)):
- `cm` (CLI de Unity Version Control) — todo lo de ramas/changesets/merge.
- `warp-cli` (Cloudflare WARP) — solo los subcomandos fijos de conectar/desconectar/estado, nunca argumentos libres del usuario.
- El ejecutable del Editor de Unity, solo para [Abrir en Unity](./BACKEND.md#comando-abrir-en-unity) — se lanza apuntando a la carpeta del workspace, ruta descubierta/validada, no un comando arbitrario.
- Ningún otro binario ni shell genérica (`sh`, `cmd.exe`) expuesta.

**Filesystem** — acceso restringido a rutas específicas resueltas una vez al configurar el proyecto, nunca a una ruta arbitraria que llegue del frontend sin validar (ver también [Validación de rutas de archivo/adjuntos en BACKEND.md](./BACKEND.md#validación-de-rutas-de-archivoadjuntos-resuelto-previene-path-traversal)):
- La carpeta del workspace local de Unity VC (lectura para [Pre-revisión](./PLANNING.md#pre-revisión), escritura para subir el archivo de una subtarea simple).
- La subcarpeta `ops/` dentro de la carpeta del proyecto sincronizada por Drive/OneDrive (leer/crear eventos y snapshots, ver [Sincronización entre máquinas en BACKEND.md](./BACKEND.md#sincronización-entre-máquinas-arquitectura-de-eventos)) — nunca editar/borrar un evento ya escrito por otra máquina.
- La carpeta de datos locales de la app (ej. `%LocalAppData%\BranchBoard\`): `.db` local, backups, y `admin.key` — esta última en particular **nunca** debe quedar alcanzable desde la webview/frontend, solo el proceso Rust del backend la lee.
- Ninguna otra carpeta del sistema; no se solicita el permiso amplio de "acceso a disco" que trae Tauri por default.

**Dialog** (selector nativo de archivos/carpetas): para elegir dónde clonar el proyecto y para elegir el archivo al subir una subtarea simple/adjunto — el usuario elige explícitamente, la app no navega el filesystem por su cuenta más allá de eso.

**Notificaciones nativas del SO**: capability de `notification`, condicionada a que se confirme implementar el aviso de escritorio (ver pendiente en [Notificaciones de BACKEND.md](./BACKEND.md#notificaciones)).

**Sin capability de red/HTTP**: no hace falta — ni la sincronización de datos (es solo lectura/escritura de archivos en una carpeta que Drive/OneDrive sincroniza por su cuenta) ni `cm` pasan por el `fetch`/HTTP del webview; toda la red real la maneja el propio proceso `cm` por fuera del control de Tauri.

Todo lo demás (clipboard, global-shortcut, updater, etc.) queda **desactivado** hasta que un caso de uso concreto lo necesite.

## Pendiente por definir

- Si la autorización de modificación es por tarea puntual o un permiso general (quién la envía ya quedó definido: Líder o Administrador)
- Consideraciones de seguridad para la futura versión web (autenticación, permisos por usuario/equipo)
