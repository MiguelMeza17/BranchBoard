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
  5. **"Branches"** — árbol de linaje de ramas (`dev` → tareas → subtareas anidadas), coloreado por estado.
  6. **"Merging"** — página tipo estado de PR, lista las tareas en "en revisión" pendientes de autorizar/rechazar.
  7. **"Ajustes"** — agrupa gestión de usuarios, solicitudes de ingreso, backups y eliminar proyecto.
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (backend) y otra en [FRONTEND.md](./FRONTEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite** (un solo archivo `.db`, sin servidor). Fase 1 = archivo local; el archivo `.db` se sincroniza a la nube (ej. Drive/OneDrive) para compartirlo entre el equipo. Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de varios usuarios.
- Integración con Unity Version Control desde Rust invocando el CLI `cm` como subproceso: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios)
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`) — `tomar_tarea` recibe además la descripción escrita en el modal de confirmación del frontend (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), que se usa como comentario del changeset inicial de la rama
- **Condición de carrera al tomar/asignar una tarea (resuelto)**: caso poco común, pero cubierto por el mismo lock de escritura ya definido (ver "Pendiente por definir" más abajo) — `tomar_tarea`/`asignar_tarea` hacen un `UPDATE` condicionado a que la tarea siga en estado "por hacer" (`WHERE estado = 'por hacer'`). Si dos personas lo disparan casi al mismo tiempo, el `UPDATE` de quien llega primero cambia el estado y afecta 1 fila; el segundo `UPDATE` no afecta ninguna fila (la condición ya no se cumple) y el backend responde "esta tarea ya fue tomada por [otra persona]" en vez de crear una segunda rama. No hace falta ningún mecanismo nuevo, es el patrón normal de actualización condicionada en SQL.
- **Toda rama de tarea general nace desde la cabeza actual de `dev`, nunca desde un punto viejo**: al crear la rama (`cm branch create` o equivalente) no se fija un changeset específico guardado de antes — se apunta siempre a la última versión de `dev` en el momento exacto de "tomar" la tarea. Esto importa especialmente para tareas que se crean y se toman en momentos distintos (ej. una tarea de corrección de Testing que nadie toma hasta días después, y mientras tanto `dev` siguió recibiendo otros merges) — la rama nueva debe partir de lo último, no de cómo estaba `dev` cuando se creó la tarea.
- **Auto-switch al tomar/asignar una tarea (importante, medida de seguridad)**: justo después de `cm branch create`, `tomar_tarea`/`asignar_tarea` ejecutan **`cm switch <rama-nueva>`** sobre el workspace local de quien la tomó — su Unity Editor queda apuntando automáticamente a esa rama. Sin esto, la persona podría seguir escribiendo código/cambios sobre la rama en la que estaba antes (una tarea anterior, o `dev`) sin darse cuenta, mezclando trabajo de dos tareas distintas en la misma rama. Ver también [Guardia de rama incorrecta](#guardia-de-rama-incorrecta) para el caso en que igual pase.

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

## Protección de ramas (`main` y `dev`)

Esta regla es **solo una capa de código en BranchBoard** — no la puede hacer cumplir Unity Version Control por sí solo (no existe un permiso nativo de VCS para "límite de niveles de anidación" ni reglas de negocio como estas). Si alguien usa el cliente de Unity VC directamente, sin pasar por la app, estas protecciones no aplican — es un acuerdo de equipo además de una validación técnica.

- **Límite de profundidad, contado como cadena de ancestros**: antes de crear cualquier rama, el backend camina la cadena de ramas padre (`cm branch` / `cm log` para reconstruirla) contando desde `main`. Cadena máxima permitida: **5** — `main → dev → tarea general → subtarea → subtarea de subtarea`. Si crear la nueva rama haría que la cadena llegue a 6, la creación **se rechaza antes de llamar a `cm`**, con un mensaje explicando el límite.
- **Ningún comando de checkin puede apuntar a `main`**: cualquier función que ejecute `cm checkin` valida primero que la rama destino no sea literalmente `main` — aplica para **todos los roles, incluido el Administrador**, sin excepción. Si se detecta, se rechaza antes de invocar `cm`.
- **Ningún comando de checkin puede apuntar a `dev` directamente**: mismo tipo de validación — todo cambio a `dev` debe entrar por el flujo de merge ya definido en [Flujo de revisión y merge a dev](#flujo-de-revisión-y-merge-a-dev), nunca por un checkin directo, sin importar el rol.
- **El merge de `dev` hacia `main` es exclusivo del Administrador**: hoy este merge queda fuera del alcance de BranchBoard (es manual, directo en Unity VC — ver [Flujo de vida de una tarea](#contexto-completo-del-proyecto)) y "esto jamás debería pasar" a través de la app. Aun así, si en algún momento se decide incorporar esa acción a BranchBoard, queda anotado desde ya como **exclusiva del Administrador** — ningún otro rol podría invocarla, ni siquiera el Líder.

## Guardia de rama incorrecta

El [auto-switch al tomar una tarea](#alcance) reduce el riesgo, pero no lo elimina del todo — alguien podría cambiar de rama manualmente en su cliente de Unity VC después, sin darse cuenta de que sigue ahí al volver a BranchBoard. Antes de cualquier acción que dependa de estar en la rama correcta (sincronizar, mandar a revisión, crear una subtarea), el backend corre `cm status` (o equivalente) para leer la rama activa del workspace y la compara contra `rama_asociada` de la tarea sobre la que se está actuando:

- **Coincide** → la acción sigue normal, sin interrupciones.
- **No coincide** → se bloquea la acción y se muestra un aviso claro: "Tu workspace está en la rama `X`, pero esta tarea es de la rama `Y` — cambia de rama antes de continuar" — con un botón para que BranchBoard mismo ejecute el `cm switch` correcto ahí, en vez de que la persona tenga que hacerlo a mano.

Esto evita el caso real que motivó el auto-switch: mezclar cambios de dos tareas distintas en la misma rama sin querer.

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

## Guardia de rama incorrecta

**Problema real señalado**: Unity Version Control tiene su propio sistema nativo de permisos por rama, pero **actualmente no está funcionando de forma confiable** — no se puede depender de que Unity VC por sí solo impida que alguien haga cambios en una rama que no le corresponde. BranchBoard necesita su propia red de seguridad encima.

- Cada vez que el backend hace polling (ver [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling)) o el temporizador de [recordatorio de inactividad](#notificaciones) corre, además revisa: **¿la rama actual del workspace de esta persona (`cm status --header`) coincide con alguna tarea que tiene asignada?**
- Si el workspace está parado en una rama que **no es una de sus tareas asignadas** (ej. la rama de la tarea de otro compañero, o `dev`) **y** hay cambios pendientes o checkins recientes ahí (`cm status` con archivos modificados, o un changeset nuevo del autor equivocado) → se dispara una **alerta inmediata** en pantalla (no solo una notificación de campana, dado lo grave del error): *"Estás haciendo cambios en la rama de la tarea [X], que no te pertenece. Cambia a tu rama asignada [Y] antes de continuar."*
- La alerta no deshace nada automáticamente — los cambios ya hechos siguen ahí (podrían perderse si se descartan sin cuidado); solo advierte para que la persona decida cómo corregirlo (ej. mover esos cambios a la rama correcta manualmente, o descartar los que en realidad no debían estar ahí).
- Esto es una **detección después del hecho**, no una prevención — no reemplaza al auto-switch de [tomar/asignar tarea](#alcance), que es la primera línea de defensa; la guardia es para cuando, aun así, alguien termina escribiendo en la rama equivocada (ej. cambió de rama a mano desde el cliente de Unity VC directamente, fuera de BranchBoard).

**Pendiente por definir**: cada cuánto corre esta revisión (¿mismo intervalo que el recordatorio de inactividad, o más frecuente dado que es un caso de seguridad?), y si además debe avisarle a un Líder/Administrador cuando pasa, no solo a la persona que se equivocó.

## Comando "Guardar cambios y sincronizar"

Mismo patrón que "tomar tarea", reusando la arquitectura de comandos Tauri ya establecida:

1. El usuario le da clic al botón en la interfaz (React)
2. Se llama un comando Tauri, ej. `sincronizar_tarea(tarea_id)`
3. El backend en Rust corre el `cm` correspondiente (checkin/update) sobre la rama de esa tarea
4. El frontend muestra estado de carga mientras corre, y éxito/error al terminar
5. Al terminar con éxito, dispara de una vez la revisión del punto anterior (changesets nuevos + estado de cierre) para que la tarea quede actualizada sin pasos extra

No es una pieza nueva de arquitectura — es el mismo cable React → comando Tauri → Rust → `cm`, ya usado en "tomar tarea" y "revisar_actualizaciones".

## Bloquear el cierre de la app durante una operación

Igual que hace Unity Editor mientras guarda/compila: mientras hay una operación de `cm` en curso (crear rama, sincronizar, merge, etc.), no se debe poder cerrar la app a la mitad.

- El backend mantiene una bandera en memoria `operacion_en_curso: bool` (o un contador, si puede haber más de una en paralelo), que se activa justo antes de invocar `cm` y se desactiva cuando el subproceso termina (éxito o error).
- Comando Tauri `hay_operacion_en_curso()` para que el frontend lo consulte antes de permitir cerrar la ventana.
- Tauri permite interceptar el evento de cierre de ventana (`onCloseRequested` o equivalente) para cancelarlo y mostrar un aviso en vez de cerrar de inmediato — esa parte de UI queda para [FRONTEND.md](./FRONTEND.md#navegación-general).

## Flujo de revisión y merge a `dev`

1. El Desarrollador arrastra su tarea a **"En revisión"** en el Tablero Scrum (con la confirmación de doble candado descrita en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)). La descripción escrita se guarda junto a la tarea.
2. La tarea aparece automáticamente en el apartado **[Merging](./PLANNING.md#merging)** (página de estado de PR) — visible para **todos los roles**, pero de **solo lectura para Desarrollador y Tester**; solo Líder y Administrador pueden actuar ahí (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).
   - **Nuevo**: el Administrador puede **asignarle la revisión de esa tarea a un Líder específico** — comando Tauri `asignar_revision(tarea_id, email_lider)`, exclusivo de Administrador. Genera notificación al Líder asignado ("se te asignó revisar [tarea]"). Sin asignación explícita, cualquier Líder/Administrador puede actuar sobre cualquier tarea en Merging.
3. Cuando el Líder/Admin marca la tarea como **"autorizado"**, el backend valida primero el **orden de merge de abajo hacia arriba** (ver [Subtareas en PLANNING.md](./PLANNING.md#subtareas-tareas-anidadas)): consulta `SELECT COUNT(*) FROM tareas WHERE tarea_padre_id = ? AND estado != 'terminada' AND eliminada = false`. Si el resultado es mayor a 0, **rechaza la acción antes de tocar `cm`** y muestra un error listando qué subtareas siguen sin terminar — no se puede mergear una rama padre mientras tenga hijas abiertas. Si pasa la validación, ejecuta el **merge real de la rama hacia `dev`** invocando `cm` como subproceso (comando exacto pendiente de verificar al implementar, ej. `cm merge` + `cm checkin`) — no es solo un cambio de estado en SQLite, sí toca Unity VC.
4. **Si el merge falla** (conflicto de contenido entre archivos): **decisión tomada** — BranchBoard no implementa resolución de conflictos propia, Unity Version Control ya tiene su propia herramienta para eso. El backend detecta que `cm merge` devolvió error y la tarea **no se marca como terminada** (se queda en "en revisión"). Como quien dispara el "autorizado" es el propio Líder/Admin (acción síncrona, no en segundo plano), la reacción es una **alerta/pantalla inmediata** en ese momento (no una notificación de campana) mostrando que el merge falló, con dos opciones para el Líder/Admin:
   - **Resolverlo él mismo**: cierra la alerta, va al cliente de Unity VC a resolver el conflicto manualmente, y luego reintenta el "autorizado" desde BranchBoard. No se notifica a nadie más.
   - **Enviarla al Desarrollador para que la corrija**: reutiliza el mismo mecanismo de **`rechazar_tarea(tarea_id, motivo)`** ya definido arriba — el motivo queda como "falló el merge por conflicto", la tarea vuelve al Desarrollador (con notificación normal de tarea rechazada), y él la reimplementa y la vuelve a mandar a revisión como cualquier rechazo.
5. Si se **rechaza**, la tarea vuelve al Desarrollador para reimplementar (mismos changesets sobre la misma rama, sin perder historial) — igual que el flujo ya descrito para subtareas rechazadas.
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
- **Quién necesita Unity Version Control instalado** — **corregido de nuevo, versión final**: en la práctica, **todos** los roles (incluido el colaborador de subtareas simples) necesitan tener `cm` instalado en su máquina. La diferencia real no es "instalado sí/no", sino **cuánto del proyecto se descarga**:
  - **Administrador, Líder, Desarrollador y Tester**: workspace **completo** — lo necesitan porque trabajan en Unity Editor (programar, modelar, etc.) o, en el caso del Tester, porque necesita **jugar/ejecutar el juego real** para encontrar errores, algo que ninguna interfaz de BranchBoard reemplaza.
  - **Colaborador de subtareas simples** (alguien que solo sube un archivo puntual — imagen, audio, video, animación, `.fbx` — y nunca necesita jugar ni programar el juego): workspace **parcial**, ver [Workspace parcial (Gluon) para subtareas simples](#workspace-parcial-gluon-para-subtareas-simples) — nunca descarga el proyecto completo, solo la carpeta donde va su archivo.
  - Para **todos** los roles sin excepción: identidad vía `cm whoami`, workspace vía `cm workspace list` (ver arriba, sin pedir carpeta a mano). Si `cm` no está instalado, ver [Instalación de Unity Version Control](#instalación-de-unity-version-control) más abajo — el botón de instalar ahora aplica a todos los roles, incluido el colaborador de subtareas simples.

## Instalación de Unity Version Control

Mismo patrón ya usado para [Conexión VPN (Cloudflare WARP)](#conexión-vpn-cloudflare-warp): si `cm` no está instalado (sin importar el rol — **corregido**, ya no se excluye al colaborador de subtareas simples, ver arriba), aparece un botón **"Instalar Unity Version Control"**.

- **Confirmado disponible en winget**: `winget install --id Codice.PlasticSCM.CloudEdition -e --accept-package-agreements --accept-source-agreements` — instala el cliente completo, que trae `cm` incluido (y con él, el modo Gluon para workspace parcial).
- Mismo fallback que WARP: si `winget` falla o no está disponible, el botón abre la página oficial de descarga de Unity Version Control en el navegador.
- Misma nota de UX: puede disparar un prompt de UAC, mostrar estado "instalando..." mientras corre.

## Workspace parcial (Gluon) para subtareas simples

**Decisión final** (reemplaza el intento anterior de API REST + Service Account, descartado — ver nota abajo): Unity Version Control tiene un modo llamado **Gluon**, diseñado oficialmente para artistas que necesitan trabajar con archivos grandes sin descargar ni gestionar el proyecto completo. Trae su propio soporte de CLI: **`cm partial`** (`cm partial add`, `cm partial checkin`, etc.), así que el backend lo puede manejar por subproceso exactamente igual que el resto de comandos `cm` — el colaborador nunca necesita abrir una interfaz aparte.

- El colaborador de subtareas simples instala Unity Version Control (mismo botón de winget que todos, ver arriba) — es liviano, no implica tener Unity Editor.
- **Definido**: la carpeta a descargar no la calcula el backend en automático — **el propio colaborador la elige**, desde un selector con las mismas categorías de [Categorías en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp): Audio, Modelado 3D, Modelado 2D, Imagen, Animación, Música, Video, UI/Interfaz (Scripting y Testing no aplican a este perfil, no son subtareas simples de archivo). El backend crea/usa un **workspace parcial** con `cm partial` scopeado solo a esa carpeta elegida — nunca descarga el proyecto completo, y solo trae la categoría en la que la persona va a trabajar, ni siquiera las demás carpetas de assets.
- **Identidad: vuelve a ser `cm whoami`, igual que todos los demás roles.** No hace falta ninguna Service Account, ni API REST, ni llave cifrada compartida, ni contraseña de equipo — todo ese diseño queda descartado (ver nota abajo).
- Esto también evita por completo el problema que motivó la idea de la Service Account: como cada colaborador usa su propia cuenta real (no una cuenta de servicio), no choca con ninguna limitación de permisos de Unity Cloud.

**Por qué se descartó la API REST + Service Account** (para que quede el porqué, no solo el qué): se comprobó en la práctica, directamente en el Dashboard de Unity Cloud, que **las Service Accounts no pueden asignarse a ningún rol/permiso de Unity Version Control** (ni los roles genéricos de proyecto, ni los Custom Roles de Version Control — se probó buscándola en el selector de usuarios de un Custom Role y no aparece como opción). Es una limitación real de la plataforma en este momento, no algo que dependiera de elegir mejor el rol. Gluon evita el problema de raíz porque no depende de ninguna cuenta de servicio.

**Pendiente por definir**:
- Sintaxis exacta de `cm partial add` / `cm partial checkin` para el caso de uso (verificar al implementar).
- Mapeo exacto categoría→carpeta real del proyecto (ej. Audio → `Assets/MP3/` según la estructura real de VirusTF, ver [ignore.conf del proyecto]) — la categoría la elige el colaborador, pero la ruta destino la resuelve el backend.

## Validación de tipo de archivo al subir una subtarea simple

**Definido**: cada categoría restringe qué extensión de archivo acepta al subir, validando el contenido real del archivo (magic bytes / firma), no solo el nombre/extensión declarada — para que no se pueda, por ejemplo, subir una imagen renombrada como `.mp4` en la categoría Video.

| Categoría | Extensión(es) permitida(s) |
|---|---|
| Modelado 3D | `.fbx` únicamente |
| Modelado 2D | formatos de imagen (a definir lista exacta, ej. `.png`, `.jpg`) |
| Animación | `.fbx` (igual que Modelado 3D, ya que trae la malla animada) |
| Imagen | formatos de imagen |
| UI/Interfaz | formatos de imagen |
| Audio | formatos de audio (ej. `.mp3`, `.wav`) |
| Música | formatos de audio |
| Video | formatos de video (ej. `.mp4`) — explícitamente rechaza imágenes u otros tipos |

Si el archivo no coincide con lo permitido para esa categoría, el comando Tauri de subida (ej. `subir_subtarea_simple(categoria, archivo)`) rechaza la operación **antes** de tocar `cm partial` — nunca llega a intentar el checkin con un archivo inválido.

## Clonar proyecto (in-app)

Para roles técnicos sin workspace todavía (ver [Instalación de Unity Version Control](#instalación-de-unity-version-control-roles-técnicos)), en vez de mandarlos a clonar por su cuenta en Unity Hub:

1. El frontend abre el selector de carpetas nativo de Tauri (diálogo del sistema operativo) para elegir dónde clonar
2. Comando Tauri `clonar_proyecto(ruta_destino)`: corre `cm workspace create` + el checkout correspondiente como subproceso, mostrando progreso
3. Al terminar, dispara el mismo flujo de `cm workspace list` para que ese workspace nuevo aparezca disponible en la Pantalla inicial

## Comando "Abrir en Unity"

Botón visible dentro de la app una vez hay un proyecto cargado (ver [Panel "Abrir en Unity" en FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), solo para roles con workspace local.

- Comando Tauri `abrir_en_unity()`: lanza el Editor de Unity (o Unity Hub) apuntando a la ruta del workspace, vía su propio CLI — mismo patrón de invocar un ejecutable externo ya usado con `cm` y `warp-cli`.
- Pendiente definir: comando exacto del CLI de Unity Hub para abrir un proyecto por ruta (verificar al implementar), y qué pasa si Unity Editor no está instalado (¿mismo patrón de "Instalar" con winget, o simplemente un mensaje de error?).

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
8. **Recordatorio de inactividad en Unity Version Control**: si una tarea con rama (Scripting u otra tarea general) no tiene **changesets nuevos** en un tiempo determinado, se le manda al Desarrollador dueño un recordatorio del tipo "llevas X días sin actividad en [tarea], recuerda [qué le toca hacer]". Se apoya en el mismo mecanismo de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) (`cm log br:/<rama> --xml`), pero a diferencia del polling manual por "Refrescar", **este caso sí necesita revisarse solo, sin que nadie le dé clic a nada** — implica correr esta revisión con un temporizador mientras la app está abierta (ej. cada N horas), no solo bajo demanda. Pendiente definir: el umbral de tiempo exacto (¿3 días? ¿configurable por Líder/Admin?), y si esto corre también para subtareas simples.
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

## Modelo de datos: tabla `colaboradores_externos`

Separada de `usuarios` — estas personas **sí tienen `cm` instalado** (liviano, solo para el workspace parcial vía Gluon, ver [Workspace parcial (Gluon) para subtareas simples](#workspace-parcial-gluon-para-subtareas-simples)) pero no tienen uno de los 4 roles principales — solo permiso puntual para crear subtareas simples (ver [Permisos de subtareas simples en FRONTEND.md](./FRONTEND.md#ajustes)).

| Campo | Tipo | Notas |
|---|---|---|
| `email` | texto (PK) | **corregido**: viene de `cm whoami`, igual que en `usuarios` — no se escribe/selecciona a mano dentro de la app |
| `nombre` | texto | |
| `agregado_por` | email | qué Líder/Admin le dio el permiso |
| `fecha_alta` | fecha | |

Borrar una fila = revocar el permiso; la próxima vez que `cm whoami` identifique a esa persona, ya no aparece en la lista de permitidos.

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
| `tarea_padre_id` | nullable | si es subtarea (rama hija); `null` si es tarea general |
| `archivo_ruta` | nullable | solo subtareas simples: ruta del archivo dentro del workspace de Unity VC |
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
| `reporte_testing_origen_id` | nullable | si esta tarea nació de un reporte de Testing (ver abajo), referencia a ese reporte |

## Modelo de datos: tabla `reportes_testing`

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | |
| `tarea_revisada_id` | — | la tarea general (ya "terminada") sobre la que se reportó el bug |
| `tarea_correccion_id` | — | la tarea de corrección generada automáticamente (ver abajo) |
| `creado_por` | email | el Tester que lo hizo |
| `descripcion` | texto | qué encontró |
| `adjunto` | nullable | archivo opcional |
| `fecha` | fecha | |

Comando Tauri `crear_reporte_testing(tarea_revisada_id, descripcion, adjunto?)`, exclusivo Tester: inserta el reporte y, en la misma operación, crea la fila en `tareas` de corrección (mismo `tipo` que la tarea revisada, `tarea_padre_id = null` porque es una tarea general nueva, no una subtarea). Pendiente definir el `estado` inicial de esa tarea (bloqueada vs. por hacer directo, ver [Pendiente en PLANNING.md](./PLANNING.md#testing)).

## Pendiente por definir
- Cómo se valida el permiso de rol en cada comando Tauri (mecanismo exacto de autorización, ej. middleware/decorador vs. chequeo manual al inicio de cada comando)
- Estados del flujo de una tarea (ya definidos en la práctica: bloqueada / por hacer / en producción / en revisión / terminada — falta solo confirmarlos como lista cerrada)
- Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)
- Comando `cm` exacto para leer si una rama está cerrada/mergeada
- Diseño final del lock de escritura en SQLite: corto y por operación (no por sesión completa) + detección de sobrescritura como red de seguridad, dado el retraso de sincronización de la carpeta en la nube. **Aclarado**: el lock solo restringe **escrituras** concurrentes mientras alguien más está modificando el `.db` — nunca bloquea abrir la app ni leer datos, eso siempre debe estar disponible.
