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
- **Control de versiones: Unity Version Control** (antes Plastic SCM), no Git genérico. Se usa su CLI (`cm`). La integración desde el backend se hace en **Rust directo** (sin Python), invocando `cm` como subproceso.
- **Flujo de vida de una tarea**: se crea visual/sin rama → al "tomarla" alguien, el backend (Rust) crea la rama en Unity VC invocando `cm` → el sprint es el tiempo que esa rama vive antes de mergear a `dev` (ver [Flujo de revisión y merge a dev](#flujo-de-revisión-y-merge-a-dev)). El merge de `dev` hacia `main` (release) queda **fuera del alcance de BranchBoard** — es una acción manual que hace alguien técnico directamente en Unity VC, no algo que la app gestione.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, SQLite, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri) que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — gráfica de curva en el tiempo (tareas resueltas por día + changesets por día).
  5. **"Branches"** — árbol de linaje de ramas (`dev` → tareas → subtareas/mini-tareas), coloreado por estado.
  6. **"Merging"** — página tipo estado de PR, lista las tareas en "en revisión" pendientes de autorizar/rechazar.
  7. **"Ajustes"** — agrupa gestión de usuarios, solicitudes de ingreso, backups y eliminar proyecto.
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (backend) y otra en [FRONTEND.md](./FRONTEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite** (un solo archivo `.db`, sin servidor). Fase 1 = archivo local; el archivo `.db` se sincroniza a la nube (ej. Drive/OneDrive) para compartirlo entre el equipo. Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de varios usuarios.
- Integración con Unity Version Control desde Rust invocando el CLI `cm` como subproceso: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios)
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`) — `tomar_tarea` recibe además la descripción escrita en el modal de confirmación del frontend (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), que se usa como comentario del changeset inicial de la rama

## Seguridad al invocar `cm` (evitar inyección de comandos)

Regla de implementación, aplica a **todo** lugar donde el backend invoque `cm` con datos que vienen del usuario (nombre de rama, justificación, descripción, comentarios de checkin/merge, etc.):

- **Nunca** construir un string de comando y pasarlo a una shell (`Command::new("sh").arg("-c").arg(comando_armado)`). Si un dato del usuario trae caracteres como `;` o `&&`, una shell los interpretaría como comandos adicionales — ej. un título de tarea con `algo; rm -rf carpeta` terminaría ejecutando ese segundo comando.
- **Siempre** pasar cada argumento por separado al proceso, como hace `std::process::Command` de forma nativa:
  ```rust
  Command::new("cm")
      .arg("log")
      .arg(format!("br:/{}", nombre_rama))
      .output();
  ```
  Así, aunque `nombre_rama` traiga caracteres raros, viajan como texto literal hacia `cm` (en el peor caso, `cm` responde con un error de "rama no encontrada"), nunca se interpretan como comandos.
- No requiere librerías ni validación extra — es la forma normal de invocar subprocesos en Rust, solo hay que ser deliberado y no armar comandos como texto plano en ningún punto.

## Detección de cambios en Unity Version Control (polling)

Problema a resolver: un compañero sube un asset (ej. un modelo 3D) desde Unity directamente a una rama, y esa tarea debe reflejarse como "modificada" en BranchBoard.

**Conceptos**: cada checkin en Unity VC genera un **changeset** (número secuencial) en la rama donde se hizo. El CLI `cm` permite consultar el historial de una rama.

**Flujo propuesto (opción simple, sin triggers de servidor)**:

1. Al crear la rama de una tarea (cuando alguien la "toma"), el backend guarda `ultimo_changeset_visto` para esa tarea (el changeset que existía en ese momento).
2. El usuario dispara una revisión manual desde el frontend (botón "Refrescar", comando Tauri `revisar_actualizaciones(tarea_id)`) — no hace falta un proceso en segundo plano para el MVP.
3. El backend en Rust corre algo como `cm log br:/<rama> --xml` (como subproceso) y obtiene el changeset más reciente de esa rama.
4. Si el changeset más reciente es mayor al `ultimo_changeset_visto` guardado → hay cambios nuevos:
   - Marca la tarea como "modificada" (para que el frontend le ponga un badge/indicador)
   - Actualiza `ultimo_changeset_visto`
   - Opcional: extrae metadata del changeset (autor, comentario, archivos afectados) para mostrarla en el detalle de la tarea
5. **El mismo refresco también revisa si la rama se cerró** (mergeada hacia su rama padre). Unity VC guarda el estado de cierre de una rama al mergearla. Si la rama aparece cerrada → la tarea se marca automáticamente como **"terminada"**, sin intervención manual. (Comando `cm` exacto para leer el estado de cierre: pendiente de verificar al implementar.)
6. (Fase futura) Reemplazar el polling manual por **triggers del servidor de Unity VC** (ej. `after-checkin`) para detección casi en tiempo real, sin depender de que alguien le dé clic a "Refrescar".

## Comando "Guardar cambios y sincronizar"

Mismo patrón que "tomar tarea", reusando la arquitectura de comandos Tauri ya establecida:

1. El usuario le da clic al botón en la interfaz (React)
2. Se llama un comando Tauri, ej. `sincronizar_tarea(tarea_id)`
3. El backend en Rust corre el `cm` correspondiente (checkin/update) sobre la rama de esa tarea
4. El frontend muestra estado de carga mientras corre, y éxito/error al terminar
5. Al terminar con éxito, dispara de una vez la revisión del punto anterior (changesets nuevos + estado de cierre) para que la tarea quede actualizada sin pasos extra

No es una pieza nueva de arquitectura — es el mismo cable React → comando Tauri → Rust → `cm`, ya usado en "tomar tarea" y "revisar_actualizaciones".

## Flujo de revisión y merge a `dev`

1. El Desarrollador arrastra su tarea a **"En revisión"** en el Tablero Scrum (con la confirmación de doble candado descrita en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)). La descripción escrita se guarda junto a la tarea.
2. La tarea aparece automáticamente en el apartado **[Merging](./PLANNING.md#merging)** (página de estado de PR) — visible para **todos los roles**, pero de **solo lectura para Desarrollador y Tester**; solo Líder y Administrador pueden actuar ahí (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).
   - **Nuevo**: el Administrador puede **asignarle la revisión de esa tarea a un Líder específico** — comando Tauri `asignar_revision(tarea_id, email_lider)`, exclusivo de Administrador. Genera notificación al Líder asignado ("se te asignó revisar [tarea]"). Sin asignación explícita, cualquier Líder/Administrador puede actuar sobre cualquier tarea en Merging.
3. Cuando el Líder/Admin marca la tarea como **"autorizado"**, el backend en Rust ejecuta el **merge real de la rama hacia `dev`** invocando `cm` como subproceso (comando exacto pendiente de verificar al implementar, ej. `cm merge` + `cm checkin`) — no es solo un cambio de estado en SQLite, sí toca Unity VC.
4. **Si el merge falla** (conflicto de contenido entre archivos): **decisión tomada** — BranchBoard no implementa resolución de conflictos propia, Unity Version Control ya tiene su propia herramienta para eso. El backend detecta que `cm merge` devolvió error y la tarea **no se marca como terminada** (se queda en "en revisión"). Como quien dispara el "autorizado" es el propio Líder/Admin (acción síncrona, no en segundo plano), la reacción es una **alerta/pantalla inmediata** en ese momento (no una notificación de campana) mostrando que el merge falló, con dos opciones para el Líder/Admin:
   - **Resolverlo él mismo**: cierra la alerta, va al cliente de Unity VC a resolver el conflicto manualmente, y luego reintenta el "autorizado" desde BranchBoard. No se notifica a nadie más.
   - **Enviarla al Desarrollador para que la corrija**: reutiliza el mismo mecanismo de **`rechazar_tarea(tarea_id, motivo)`** ya definido arriba — el motivo queda como "falló el merge por conflicto", la tarea vuelve al Desarrollador (con notificación normal de tarea rechazada), y él la reimplementa y la vuelve a mandar a revisión como cualquier rechazo.
5. Si se **rechaza**, la tarea vuelve al Desarrollador para reimplementar (mismos changesets sobre la misma rama, sin perder historial) — igual que el flujo ya descrito para mini-tareas rechazadas.
6. Tras un merge exitoso, se dispara la misma revisión de "Detección de cambios" (rama cerrada → tarea "terminada").

Pendiente confirmar: si la acción de "autorizado" también requiere el doble candado (descripción + palabra literal) del punto 1, dado que mergear a `dev` es igual o más delicado que mandar a revisión.

## Eliminar una tarea

Comando Tauri `eliminar_tarea(tarea_id, justificacion)`, validado en backend para que solo Administrador o Líder puedan invocarlo (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)). Requiere el doble candado descrito en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop) (justificación + escribir "eliminar").

- La `justificacion` se guarda en SQLite como parte del registro de auditoría (no se borra junto con la tarea, queda como historial de por qué se eliminó).
- **Confirmado**: si la tarea ya tenía una rama asociada en Unity VC, `eliminar_tarea` también **cierra esa rama** en Unity VC (invocando `cm` — comando exacto pendiente de verificar al implementar), en vez de dejarla huérfana. La `justificacion` queda como el motivo tanto del borrado en BranchBoard como del cierre de la rama.

## Datos para la gráfica de Avance

Ver definición de la gráfica en [PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp). Dos fuentes de datos distintas:

1. **Tareas resueltas por día** (línea principal): cada tarea guarda un campo `fecha_completada`, escrito cuando pasa a "terminada" (ya sea manual o automático al detectar la rama cerrada, ver "Detección de cambios"). Para armar la curva, se agrupan las tareas por ese campo y se cuenta cuántas hay por día — es una consulta normal a SQLite, no requiere `cm`.
2. **Changesets hechos por día** (línea secundaria, todo el equipo): a diferencia del polling por tarea (que revisa una rama a la vez), esto necesita el historial de **todo el repositorio**, filtrado por fecha — comando `cm` exacto pendiente de verificar al implementar.

## Identidad de usuario

BranchBoard **no tiene sistema propio de login/usuarios**. La identidad se obtiene directamente de la sesión de Unity Version Control ya activa en la máquina:

- Al arrancar, el backend corre `cm whoami` (subproceso) y obtiene el email de la cuenta logueada (confirmado en pruebas reales: devuelve algo como `nombre@dominio.com`).
- Ese email es el identificador del usuario dentro de BranchBoard — no hay contraseña, sesión propia, ni token que gestionar.
- La tabla de usuarios en SQLite mapea **email (de `cm whoami`) → rol** (ver Roles y permisos). El admin asigna el rol a cada email conocido del equipo la primera vez; después de eso, cada persona que abre la app en su máquina queda identificada automáticamente.
- Requisito implícito: cada miembro del equipo ya debe tener el cliente de Unity Version Control instalado y con sesión iniciada (login por SSO/navegador, una sola vez) en su propia máquina — eso ya lo resuelve `cm`, BranchBoard no reimplementa nada de eso.
- **Confirmado**: aunque todo el equipo comparte el mismo workspace de Unity Version Control, cada persona inicia sesión con su propia cuenta/correo — `cm whoami` sí distingue a cada usuario individualmente, no hay una cuenta compartida.
- **Eliminar a alguien del equipo** (rol: Administrador o Líder, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)) es tan simple como borrar su fila en la tabla de usuarios: como la identidad se valida contra esa tabla cada vez que se abre la app (no hay sesión persistente que revocar), la persona pierde el acceso de inmediato la próxima vez que intente entrar — su correo ya no aparecería registrado.
- **Bootstrapping (primer usuario de un proyecto nuevo)**: resuelto. Cuando alguien inicializa un `.db` de BranchBoard por primera vez para un workspace de Unity VC (ver flujo en [FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), su email (de `cm whoami`) se inserta automáticamente en la tabla de usuarios con rol **Líder** — no Administrador. El rol de Administrador se asigna después, a mano, por decisión del equipo (consistente con que en Unity Cloud el "Propietario" ya es una persona identificada aparte, ver [Usuarios semilla en SEGURIDAD.md](./SEGURIDAD.md#usuarios-semilla-equipo-virustf)).
- **Quién necesita Unity Version Control instalado, y quién no** — **corregido**, la regla es por lo que la persona necesita hacer, no por conveniencia:
  - **Administrador, Líder, Desarrollador y Tester**: **todos** necesitan Unity Version Control instalado y el proyecto clonado en su máquina. Los primeros tres porque trabajan directamente en Unity Editor (programar, modelar, etc.). El **Tester también** — para encontrar errores necesita **jugar/ejecutar el juego real**, algo que ninguna interfaz de BranchBoard puede reemplazar, así que sí o sí necesita el proyecto corriendo en su máquina.
  - Para estos 4 roles: identidad vía `cm whoami`, workspace vía `cm workspace list` (ver arriba, sin pedir carpeta a mano). Si `cm` no está instalado, ver [Instalación de Unity Version Control](#instalación-de-unity-version-control-roles-técnicos) más abajo.
  - **Únicamente el colaborador de Minitareas** (alguien que solo sube un archivo puntual — imagen, audio, video, animación, `.fbx` — y nunca necesita jugar ni programar el juego) **no necesita nada instalado**: ni Unity, ni el cliente de Unity VC, ni el proyecto clonado. Para este caso se usa la [API REST de Unity Version Control](#api-rest-de-unity-version-control-minitareas-sin-proyecto-instalado) en vez de `cm`.

## Instalación de Unity Version Control (roles técnicos)

Mismo patrón ya usado para [Conexión VPN (Cloudflare WARP)](#conexión-vpn-cloudflare-warp): si `cm` no está instalado y el rol de la persona lo necesita (Administrador/Líder/Desarrollador/Tester — nunca para un colaborador de Minitareas, a ellos no se les ofrece esto), aparece un botón **"Instalar Unity Version Control"**.

- **Confirmado disponible en winget**: `winget install --id Codice.PlasticSCM.CloudEdition -e --accept-package-agreements --accept-source-agreements` — instala el cliente completo, que trae `cm` incluido.
- Mismo fallback que WARP: si `winget` falla o no está disponible, el botón abre la página oficial de descarga de Unity Version Control en el navegador.
- Misma nota de UX: puede disparar un prompt de UAC, mostrar estado "instalando..." mientras corre.

## API REST de Unity Version Control (Minitareas sin proyecto instalado)

Para el colaborador de Minitareas, en vez de invocar `cm` como subproceso, el backend llama directamente al servidor de Unity Version Control por HTTPS — **confirmado en la documentación oficial de Unity** que existe una API REST del lado del servidor que no requiere cliente ni workspace local, incluyendo un endpoint de checkin:

```
POST https://<servidor-regional>.plasticscm.com:7178/api/v1/organizations/{org}/repos/{repo}/branch/{branch}/checkin
```
(recibe el contenido del archivo directo en el body — `path`, `content`, `hash`, etc.)

**Autenticación — Service Account**:
- El Administrador crea, **una sola vez**, un *Service Account* desde el Dashboard de Unity Cloud (Administración → Service Accounts → Create service account), y le asigna el rol/permiso necesario para acceder a Unity Version Control.
- Eso genera un **Key ID + Secret Key** — se guardan cifrados (ej. Windows Credential Manager, nunca en texto plano en el `.db`).
- Ese Key ID + Secret se intercambia (Token Exchange API de Unity) por un **Bearer token de corta duración**, que es el que se usa en cada llamada real. El backend lo renueva automáticamente cuando expira — nadie del equipo vuelve a tocar esto tras la configuración inicial.
- El Bearer token vive solo en memoria mientras la app corre, nunca se persiste a disco.
- Esto es un mecanismo de autenticación **completamente separado** de `cm whoami` — el Service Account identifica a la *app* ante Unity, no a la persona.

**Pendiente por definir**:
- Identidad del colaborador de Minitareas en BranchBoard: como no tiene `cm` instalado, `cm whoami` no le sirve para identificarse. ¿Se identifica con el correo que usó al "solicitar ingreso"? ¿Un selector simple de "quién eres" dentro de la app (equipo pequeño, de confianza)?
- Qué rol/permiso exacto de Unity Cloud hay que asignarle al Service Account para que pueda usar los endpoints de Version Control (la documentación de Unity no lo especifica, hay que probarlo al implementar).
- Endpoint regional exacto a usar (Unity Cloud tiene servidores por región — US East, Europa, Asia — hay que confirmar cuál usa la organización del equipo).

## Conexión VPN (Cloudflare WARP)

Problema real observado: en la red de la universidad, `cm` no logra conectar con el servidor de Unity Version Control (bloqueo de puerto). La solución probada en la práctica es activar **Cloudflare WARP** (cliente gratuito, `warp-cli`) — con la VPN activa, `cm` conecta sin problema.

Se agrega un **switch de encender/apagar WARP desde la pantalla inicial** de BranchBoard (ver [FRONTEND.md](./FRONTEND.md#pantalla-inicial)), para no depender de que el usuario sepa buscar el ícono de WARP en la bandeja del sistema.

- **Comandos Tauri nuevos**: `estado_warp()` y `alternar_warp()` (o `activar_warp()` / `desactivar_warp()`).
- Internamente invoca `warp-cli status`, `warp-cli connect`, `warp-cli disconnect` como subproceso — mismo patrón que la integración con `cm`.
- **Detección de instalación**: si `warp-cli` no existe en la ruta esperada (Windows: `C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe`), el switch se muestra deshabilitado, y junto a él aparece un texto pequeño **"Instalar Cloudflare WARP"**.
  - Comando Tauri nuevo: `instalar_warp()` → corre `winget install --id Cloudflare.Warp -e --accept-package-agreements --accept-source-agreements` como subproceso (probado en la práctica, funciona). Al terminar, se vuelve a llamar `estado_warp()` para refrescar el switch.
  - **Fallback si falla o `winget` no está disponible**: el mismo texto abre en el navegador la página oficial de descarga (`https://1.1.1.1/download`) usando el plugin `shell.open` de Tauri, para que el usuario lo instale manualmente.
  - Nota de UX: `winget install` puede disparar un prompt de UAC de Windows (permiso de administrador) porque instala un servicio a nivel de sistema — el frontend debe mostrar un estado de "instalando..." mientras esto ocurre, ya que puede tardar y depende de que el usuario acepte el prompt.
  - Es una acción explícita del usuario (clic en "Instalar"), nunca automática al abrir la app.
- El estado (conectado/desconectado) se puede consultar cuando se quiera, no requiere que el servicio `CloudflareWARP` esté haciendo nada especial — confirmado en pruebas reales que el túnel sigue activo aunque se cierre la ventana de la app de WARP, porque lo mantiene un servicio de Windows en segundo plano.
- **Solo Windows por ahora** (consistente con que Fase 1 del stack apunta a desktop en Windows) — la ruta del ejecutable y el comportamiento del servicio en Mac/Linux quedan pendientes si se porta la app a esos sistemas.

## Roles y permisos

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 4 roles confirmados (Administrador, Líder, Desarrollador, Tester). El modelo de datos de usuario necesita un campo de rol asociado al email de `cm whoami` (ver Identidad de usuario), y cada comando Tauri sensible debe validar permisos en el backend (no solo ocultar en la UI).

## Notificaciones

Tabla `notificaciones` en SQLite: destinatario (email), tipo, mensaje, tarea relacionada (opcional), leída/no leída, fecha. Todos estos casos disparan mensaje **automático**, sin acción manual de quien los envía más allá de la acción que los origina (aprobar, rechazar, asignar, etc.).

**Aclarado**: todas las notificaciones de esta lista son **100% internas a la app** (campana + tabla SQLite) — no se manda ningún correo real por revisión, tareas o permisos. El **único** caso que usa correo de verdad es la solicitud de ingreso a la organización (ver [Autenticación / acceso al proyecto en FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), porque ahí la persona todavía no tiene acceso a BranchBoard para ver una notificación in-app — envía su nombre/correo de Unity Version Control (`cm whoami`) desde la Pantalla inicial para que un Admin/Líder lo agregue, y solo ese primer contacto necesita salir por correo.

1. **Tarea creada por un Desarrollador esperando aprobación** — destinatarios: todos los usuarios con rol Líder o Administrador (ver [PLANNING.md](./PLANNING.md#visión), estado "bloqueada"). Se resuelve con un comando Tauri `aprobar_tarea(tarea_id)` que solo Líder/Admin puede invocar (validación de rol en backend), y que solo cambia el estado de la tarea a "por hacer" — no crea rama ni la mueve de tablero, eso lo sigue haciendo "tomar tarea" por separado.
2. **Tarea rechazada** — destinatario: el Desarrollador que la creó/tomó. Comando Tauri nuevo `rechazar_tarea(tarea_id, motivo)` (Líder/Admin), simétrico a `aprobar_tarea`. El motivo queda guardado y se muestra en la notificación.
3. **Tarea asignada directamente por Administrador/Líder a un Desarrollador** — destinatario: ese Desarrollador. Comando Tauri nuevo `asignar_tarea(tarea_id, email_destinatario)` (solo Administrador/Líder) — a diferencia de "tomar tarea" (autoasignación), aquí alguien más asigna la tarea. Crea la rama igual que "tomar tarea" lo hace hoy.
4. **Autorización para modificar una tarea** — destinatario: el Desarrollador dueño de la tarea. Puede enviarla tanto un Líder como un Administrador (comando Tauri validado para ambos roles). Pendiente definir si es por tarea puntual o un permiso general.
5. **Tarea llega a "en revisión"** — destinatarios: todos los usuarios con rol Líder o Administrador (no solo que aparezca en el apartado [Merging](./PLANNING.md#merging) — también genera notificación en la campana).
6. **Revisión asignada a un Líder específico** — destinatario: ese Líder. Generada por `asignar_revision(tarea_id, email_lider)` (solo Administrador). Mensaje tipo "se te asignó revisar [tarea]".
7. **Solicitud de ingreso a la organización aprobada** — destinatario: quien la solicitó (además del correo externo que ya se manda, ver [FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)).
8. **Recordatorio de inactividad en Unity Version Control**: si una tarea con rama (Scripting u otra tarea general) no tiene **changesets nuevos** en un tiempo determinado, se le manda al Desarrollador dueño un recordatorio del tipo "llevas X días sin actividad en [tarea], recuerda [qué le toca hacer]". Se apoya en el mismo mecanismo de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) (`cm log br:/<rama> --xml`), pero a diferencia del polling manual por "Refrescar", **este caso sí necesita revisarse solo, sin que nadie le dé clic a nada** — implica correr esta revisión con un temporizador mientras la app está abierta (ej. cada N horas), no solo bajo demanda. Pendiente definir: el umbral de tiempo exacto (¿3 días? ¿configurable por Líder/Admin?), y si esto corre también para minitareas.
9. **Solicitud de acción sobre la base de datos** — un Líder o Desarrollador que detecta un problema (ej. datos raros, algo se ve corrupto) no puede actuar directamente sobre la BD, así que le manda una notificación a **todos los Administradores** pidiendo que revisen/restauren. Comando Tauri `solicitar_accion_bd(motivo)`, disponible para Líder y Desarrollador (no para Tester, que no tiene relación con el ciclo de tareas/BD del equipo). El `motivo` escrito queda en el mensaje de la notificación para que el Admin sepa qué está pasando antes de actuar.

Nota: el **merge fallido por conflicto** no genera un tipo de notificación nuevo — se maneja con una alerta síncrona en pantalla para el Líder/Admin en el momento, y si decide enviarla a corregir, reutiliza el caso 2 (`rechazar_tarea`). Ver detalle completo en [Flujo de revisión y merge a `dev`](#flujo-de-revisión-y-merge-a-dev).

Comando Tauri expuesto al frontend: `obtener_notificaciones()` (para el ícono de campana con contador) y `marcar_leida(notificacion_id)`.

Pendiente definir: si además de la campana in-app, estas notificaciones también disparan una notificación nativa de escritorio del sistema operativo (Tauri lo soporta) para que lleguen aunque la app esté minimizada.

## Comandos de la pantalla "Ajustes"

Ver diseño de la pantalla en [FRONTEND.md](./FRONTEND.md#ajustes).

- `listar_solicitudes_pendientes()` — Líder/Administrador
- `aceptar_solicitud(solicitud_id, rol)` — Líder/Administrador; valida que el rol elegido esté permitido para quien invoca (ej. un Líder no puede asignar `rol = Líder`, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos))
- `rechazar_solicitud(solicitud_id)` — Líder/Administrador
- `listar_usuarios()` — Líder/Administrador
- `eliminar_usuario(email)` — validado según quién invoca (Administrador: cualquiera; Líder: solo Desarrolladores)
- `ascender_a_lider(email)` — exclusivo Administrador

## Copias de seguridad y restauración de la BD

**Backup automático**: cada vez que se abre BranchBoard, antes de tocar el `.db`, se copia a una carpeta local de backups (ej. `%LocalAppData%\BranchBoard\backups\branchboard_2026-09-25.db`). Se conservan solo las **últimas 10 copias** — al llegar a la 11, se borra la más vieja. No requiere acción de nadie, corre solo.

**Restaurar backup** — **exclusivo del Administrador** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)):
- Comando Tauri `listar_backups()` (lista las copias disponibles con fecha) y `restaurar_backup(nombre_archivo)`, ambos validados para que solo un Administrador los invoque.
- Dado lo delicado de la acción (descarta todo lo escrito después del backup elegido), usa el mismo patrón de doble candado que [eliminar una tarea](#eliminar-una-tarea): confirmación + escribir una palabra literal antes de habilitar el botón (palabra exacta pendiente de definir en frontend).
- Un Líder o Desarrollador que detecte un problema no puede restaurar directamente — le llega a los Administradores vía la notificación **"Solicitud de acción sobre la base de datos"** (ver [Notificaciones](#notificaciones), punto 9).

## Modelo de datos: tabla `usuarios`

| Campo | Tipo | Notas |
|---|---|---|
| `email` | texto (PK) | viene de `cm whoami`, ver [Identidad de usuario](#identidad-de-usuario) — no hay contraseña que guardar |
| `nombre` | texto | viene del formulario de [solicitud de ingreso](./FRONTEND.md#autenticación--acceso-al-proyecto) |
| `apellido` | texto | ídem |
| `rol` | enum | Administrador / Líder / Desarrollador / Tester |
| `fecha_alta` | fecha | cuándo quedó registrado en el proyecto |

Borrar una fila de esta tabla = [eliminar a esa persona del equipo](#identidad-de-usuario) (revoca acceso de inmediato, sin sesión que cerrar).

## Modelo de datos: tabla `solicitudes_ingreso`

Separada de `usuarios` — una solicitud no es todavía un usuario del proyecto.

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | |
| `nombre` / `apellido` | texto | |
| `correo` | texto | debe coincidir con el email de `cm whoami` de quien la aprueba después, ver [Pendiente por definir en FRONTEND.md](./FRONTEND.md#pendiente-por-definir) |
| `id_organizacion` | texto | |
| `estado` | enum | pendiente / aceptada / rechazada |
| `fecha_solicitud` / `fecha_resolucion` | fecha, nullable | |

Al aceptarse, se inserta la fila correspondiente en `usuarios` con el rol que le asigne quien aprueba — la solicitud no desaparece, queda como historial con `estado = aceptada`.

## Modelo de datos: tabla `tareas`

Consolidando todo lo definido a lo largo de la conversación en un esquema concreto:

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | identificador único |
| `titulo` | texto | |
| `descripcion` | texto | viene del modal de confirmación al crear/tomar/mandar a revisar (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)) |
| `tipo` | enum | Scripting / Modelado / Animación / Imagen / Video / Música |
| `tarea_padre_id` | nullable | si es subtarea o mini-tarea (rama hija); `null` si es tarea general |
| `archivo_ruta` | nullable | solo mini-tareas: ruta del archivo dentro del workspace de Unity VC |
| `estado` | enum | bloqueada / por hacer / en producción / en revisión / terminada |
| `rama_asociada` | nullable | `null` hasta que alguien "toma" la tarea |
| `ultimo_changeset_visto` | nullable | para el polling de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) |
| `modificada` | booleano | badge de "hay cambios nuevos" (ver polling) |
| `creado_por` | email | quien creó la tarea |
| `asignado_a` | email, nullable | quien la tomó/le fue asignada |
| `lider_revision_asignado` | email, nullable | ver [asignar_revision en Flujo de revisión](#flujo-de-revisión-y-merge-a-dev) |
| `autorizada_modificacion` | booleano | ver [Notificaciones](#notificaciones), punto 4 |
| `sprint_id` | nullable | una tarea puede no pertenecer a ningún sprint |
| `con_fecha` / `fecha_limite` | booleano / nullable | tareas con y sin tiempo (ver [Sprints y tareas en FRONTEND.md](./FRONTEND.md#alcance)) |
| `fecha_completada` | nullable | usada para la [gráfica de Avance](#datos-para-la-gráfica-de-avance) |
| `eliminada` | booleano | **soft-delete**, no se borra la fila (ver [Eliminar una tarea](#eliminar-una-tarea)) |
| `justificacion_eliminacion` / `fecha_eliminacion` | nullable | solo si `eliminada = true` |
| `motivo_rechazo` | nullable | último motivo de rechazo, si aplica |

Los reportes de Testing (ver [Testing en PLANNING.md](./PLANNING.md#testing)) viven en una tabla aparte (`reportes_testing`, con `tarea_id` de referencia), no como campos de `tareas` — no son tareas ni afectan su estado.

## Pendiente por definir
- Cómo se valida el permiso de rol en cada comando Tauri (mecanismo exacto de autorización, ej. middleware/decorador vs. chequeo manual al inicio de cada comando)
- Estados del flujo de una tarea (ya definidos en la práctica: bloqueada / por hacer / en producción / en revisión / terminada — falta solo confirmarlos como lista cerrada)
- Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)
- Comando `cm` exacto para leer si una rama está cerrada/mergeada
- Diseño final del lock de escritura en SQLite: corto y por operación (no por sesión completa) + detección de sobrescritura como red de seguridad, dado el retraso de sincronización de la carpeta en la nube. **Aclarado**: el lock solo restringe **escrituras** concurrentes mientras alguien más está modificando el `.db` — nunca bloquea abrir la app ni leer datos, eso siempre debe estar disponible.
