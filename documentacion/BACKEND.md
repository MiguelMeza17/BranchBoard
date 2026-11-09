# BranchBoard — Backend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que corre del lado de Tauri (Rust) / lógica de datos.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. **Toda tarea se trabaja en su propia rama** (se crea al tomarla), sin importar su tipo. **Actualizado (2026-11-07, según el frontend)**: hay 12 tipos que se eligen al crear una tarea (son también las carpetas de [Categorías](./FRONTEND.md#alcance)):
  - **Scripting**, **Modelado 3D**, **Modelado 2D**, **Animación**, **Imagen** (arte 2D), **UI/Interfaz**, **Audio**, **Música**, **Video** (uno por cada visor de [Pre-revisión](./PLANNING.md#pre-revisión))
  - **Testing**, **Corrección**, **Ajustes**
  - Más un valor interno **"Sin definir"**, solo para la subtarea comodín "En espera" (ver [Check-ins de la rama](#check-ins-de-la-rama-subtarea-en-espera-y-cambios-entrantes)); no se elige ni sale en Categorías.
- **Control de versiones: Unity Version Control** (antes Plastic SCM), no Git genérico. Se usa su CLI (`cm`). La integración desde el backend se hace en **Rust directo** (sin Python), invocando `cm` como subproceso.
- **Flujo de vida de una tarea**: se crea visual/sin rama → al "tomarla" alguien, el backend (Rust) crea la rama en Unity VC invocando `cm` → el sprint es el tiempo que esa rama vive antes de mergear a `dev` (ver [Flujo de revisión y merge a dev](#flujo-de-revisión-y-merge-a-dev)). El merge de `dev` hacia `main` (release) queda **fuera del alcance de BranchBoard** — es una acción manual que hace alguien técnico directamente en Unity VC, no algo que la app gestione.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, SQLite, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri) que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP**: ver lista completa y autoritativa en [Pantallas / tableros del MVP en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp) — no se duplica aquí para evitar que este documento quede desactualizado cuando se agreguen pantallas nuevas.
- **Referencia: el frontend manda** (actualizado 2026-11-07). El mockup de `app/frontend/` (React, datos en memoria) y [FRONTEND.md](./FRONTEND.md) son la referencia de lo que el producto hace: este documento describe lo que el backend real tiene que hacer para que ese frontend funcione. Cuando el frontend y este documento no coinciden, se corrige este documento. Los modelos de datos del frontend (`src/types.ts`) y sus acciones (`src/store/useStore.ts`) son el contrato: cada acción del store corresponde a un comando Tauri descrito aquí (ver [Contrato con el frontend](#contrato-con-el-frontend-acciones-y-comandos)). Lo que está diseñado aquí pero el frontend todavía no tiene se marca como **"sin pantalla en el mockup"**.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite local por máquina** (sin servidor). Cada máquina tiene su propio `.db`, que **no se sincroniza directamente** — lo que se comparte por la nube (ej. Drive/OneDrive) es un log de eventos (carpeta `ops/`), y cada máquina reconstruye/actualiza su `.db` reproduciendo esos eventos (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)). Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de verdad, y este mecanismo de eventos deja de ser necesario.
- Integración con Unity Version Control desde Rust invocando el CLI `cm` como subproceso: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios)
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`). **Actualizado (2026-11-07)**: tomar una tarea se confirma escribiendo la palabra **"asignar"** (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)); ya no se escribe una descripción, así que el comentario del changeset inicial de la rama lo arma el backend (ej. `Rama de T-123: <título>`). Tomar una tarea pone `asignado_a`, `modo_trabajo = 'rama'` y `rama_asociada` (formato `task/<id>-<nombre>`)
- **Condición de carrera al tomar/asignar una tarea (resuelto, ver arquitectura de eventos)**: un `UPDATE ... WHERE estado = 'por hacer'` solo evita colisiones dentro de una única BD compartida en vivo — no sirve aquí porque cada máquina tiene su propio `.db` con retraso de sincronización. La resolución real vive en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos): `tomar_tarea`/`asignar_tarea` generan un evento, y el desempate entre dos tomas simultáneas ocurre de forma determinista al reproducir los eventos en cada máquina, no con un `UPDATE` condicionado local.
- **Toda rama de tarea general nace desde la cabeza actual de `dev`, nunca desde un punto viejo**: al crear la rama (`cm branch create` o equivalente) no se fija un changeset específico guardado de antes — se apunta siempre a la última versión de `dev` en el momento exacto de "tomar" la tarea. Esto importa especialmente para tareas que se crean y se toman en momentos distintos (ej. una tarea de corrección de Testing que nadie toma hasta días después, y mientras tanto `dev` siguió recibiendo otros merges) — la rama nueva debe partir de lo último, no de cómo estaba `dev` cuando se creó la tarea.
- **Auto-switch al tomar una tarea (importante, medida de seguridad)**: justo después de `cm branch create`, `tomar_tarea` ejecuta **`cm switch <rama-nueva>`** sobre el workspace local de quien la tomó — su Unity Editor queda apuntando automáticamente a esa rama. Sin esto, la persona podría seguir escribiendo código/cambios sobre la rama en la que estaba antes (una tarea anterior, o `dev`) sin darse cuenta, mezclando trabajo de dos tareas distintas en la misma rama. Ver también [Guardia de rama incorrecta al sincronizar o crear subtarea](#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea) y [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) para los casos en que igual pase.
- **Colaborador externo solo en tareas individuales** (2026-11-07, ver [SEGURIDAD.md](./SEGURIDAD.md#colaborador-externo)): `tomar_tarea` y `asignar_tarea` rechazan la acción si quien queda a cargo es Colaborador externo y la tarea tiene subtareas (no eliminadas) o es subtarea de otra (`tarea_padre_id` no nulo). Al reproducir el evento se valida igual y se descarta si no cumple.
- **`asignar_tarea` NO hace auto-switch (corregido)**: el diseño original hacía que `asignar_tarea` ejecutara `cm switch` igual que `tomar_tarea`, pero eso corre en el proceso de Tauri de **quien invoca el comando** (el Líder/Administrador que asigna) — un `cm switch` ahí cambiaría la rama del workspace de quien asigna, no la del Desarrollador destino, que ni siquiera tiene la app abierta en ese momento. `asignar_tarea` solo crea la rama (`cm branch create`) y genera la notificación #3; el cambio de rama en la máquina del Desarrollador asignado ocurre después, como acción explícita de **su propio lado**. **Actualizado (2026-11-07, según el frontend)**: no hay un botón aparte de "Cambiar mi workspace a esta rama"; se hace desde el panel **"Abrir en Unity"**, que lista sus tareas abiertas y, al elegir una, cambia su workspace a esa rama (`cm switch`) y abre Unity ahí (ver [Comando "Abrir en Unity"](#comando-abrir-en-unity)). Nunca automático ni silencioso, para no interrumpirle sin avisar un cambio de rama mientras podría estar trabajando en otra cosa.
- **Quién asigna** (según el frontend): Líder o Administrador, desde la Lista de tareas, solo en tareas **sin subtareas** que no estén terminadas; si la tarea ya tiene rama, solo cambia el responsable. Se confirma escribiendo **"asignar"**.
- **Soltar una tarea** (`desasignar_tarea`, arrastrarla a "Sin asignar"): quita el responsable y la deja en "por hacer" junto con sus subtareas. Se rechaza si la tarea o alguna subtarea ya tiene commits (hay progreso, ver [Estados, progreso y reaperturas](#estados-progreso-y-reaperturas)).

## Sincronización entre máquinas (arquitectura de eventos)

Reemplaza el diseño anterior de "lock de escritura sobre el `.db` compartido": sincronizar directamente un archivo SQLite entre varias máquinas por Drive/OneDrive no funciona, porque el sync es **por archivo completo, con retraso, y sin fusión de filas** — dos máquinas escribiendo casi al mismo tiempo pueden terminar con la copia de una pisando por completo la de la otra, sin importar si tocaron la misma tarea o no.

**Diseño**: lo que se sincroniza deja de ser el `.db`. Cada máquina tiene su propio `.db` **local, no sincronizado, desechable y reconstruible**. Lo que sí viaja por la carpeta de nube es una carpeta `ops/` de **eventos inmutables**, uno por archivo:

- Cada comando Tauri que muta estado compartido (`tomar_tarea`, `asignar_tarea`, `cambiar_estado`, `aprobar_tarea`, `rechazar_revision`, crear/editar/eliminar tarea, etc.; lista completa más abajo) — en vez de hacer un `UPDATE`/`INSERT` directo que luego se sincroniza como parte del `.db` — escribe un archivo nuevo en `ops/`, nombrado `ops/<timestamp_iso>_<usuario>_<uuid>.json`, con `{ tipo, tarea_id, usuario, timestamp, payload }`. Nunca se edita ni se borra un archivo de `ops/` ya creado — solo se crean nuevos. Esto es lo que evita el problema de sincronización de archivo completo: como nadie más edita ese archivo, Drive/OneDrive nunca tiene que decidir "qué versión gana".
- Cada máquina mantiene en su `.db` local una tabla `eventos_aplicados` (id de archivo ya procesado) para no reaplicar el mismo evento dos veces.
- Al abrir la app, y luego en el mismo intervalo que [el polling de Unity VC](#detección-de-cambios-en-unity-version-control-polling), el backend escanea `ops/`, toma los archivos que no estén en `eventos_aplicados`, los ordena por `timestamp` ascendente (desempate determinista: alfabético por `usuario`) y los aplica uno por uno a su `.db` local, con la **misma regla en todas las máquinas** — así todas convergen al mismo estado sin coordinarse en vivo.

**Caso concreto — doble toma de la misma tarea**: dos personas le dan clic a "tomar tarea" sobre la misma tarea casi al mismo tiempo, cada una en su propia máquina, sin saber todavía que la otra lo hizo. Al aplicar el evento `tomar_tarea` de cada una, el backend solo lo efectúa si, según el estado ya calculado hasta ese punto, la tarea sigue en `'por hacer'`; si no, el evento se descarta. Así queda un **"ganador"** (a quien se le queda asignada la tarea) y un **"perdedor"** (a quien se le descarta el evento). Si el evento descartado es el que generó esta misma máquina (la persona pensó que la había tomado), se dispara una notificación tipo "X se adelantó — tu toma de la tarea Y fue revertida", y el frontend corrige la UI optimista que había mostrado. El resultado es el mismo que el `UPDATE ... WHERE estado='por hacer'` original, pero aplicado de forma diferida y determinista en vez de en vivo contra una BD compartida que no existe.

**Corregido — revertir el `.db` no deshace lo que ya pasó en Unity VC, y un evento "tardío" no siempre debería ganar solo por tener timestamp más viejo**: el diseño de arriba, aplicado tal cual, tiene dos huecos:
1. `tomar_tarea` ya ejecutó `cm branch create` + `cm switch` **en el momento de invocar el comando**, no cuando se reproduce el evento — las dos máquinas ya crearon su rama antes de que el desempate ocurra. Revertir solo el `.db` deja una **rama huérfana** en el repo y el workspace del perdedor apuntando a una rama que ya no corresponde a ninguna tarea.
2. Una máquina que estuvo offline puede sincronizar horas después con un evento de timestamp **anterior** al de quien "ganó" en vivo — si ese desempate se aplica ciegamente por timestamp, revierte retroactivamente a alguien que ya lleva horas trabajando y con changesets reales en su rama.

**Resuelto**: antes de aplicar la reversión, el backend revisa si la rama del "perdedor" tiene **changesets reales más allá del inicial** (mismo dato que ya usa [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling), comparar contra `ultimo_changeset_visto` en el momento de crear la rama):
- **Sin changesets reales todavía** (rama recién creada, nadie hizo checkin) → seguro revertir automático: además de corregir el `.db`, el backend cierra/borra esa rama huérfana vía `cm`, y la notificación al perdedor incluye un botón explícito **"Volver mi workspace a `dev`"** (mismo patrón de acción explícita ya usado para el auto-switch de `asignar_tarea` — nunca un `cm switch` silencioso en la máquina de otra persona).
- **Con changesets reales** (alguien ya trabajó de verdad en esa rama, sin importar si su evento llegó "tarde") → **no se revierte automático**: se trata igual que [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) — alerta a ambas personas y a un Líder/Administrador para que decidan a mano cómo resolverlo (ej. una de las dos ramas se convierte en subtarea, o se descarta la que menos avanzó), en vez de que el sistema decida solo y arriesgarse a tirar trabajo real.

**Riesgo aceptado — manipulación del reloj local**: el desempate usa el timestamp que cada máquina le pone a su propio evento; alguien podría adelantar el reloj de su PC para "ganar" siempre una carrera. Se documenta como **riesgo aceptado de baja prioridad** dado el tamaño y nivel de confianza del equipo. Se descartó explícitamente usar el timestamp "server-side" de la API de Drive/OneDrive como desempate más confiable: requeriría credenciales OAuth por usuario y por proveedor (contradice [Sin backend propio de autenticación y Sin servicios en la nube propios](./ARQUITECTURA.md#decisiones-de-arquitectura-ya-tomadas)) y dependencia de estar online en el momento de resolver el orden, rompiendo el funcionamiento sin conexión que tiene hoy escribir un evento local.

- **Mitigación barata añadida (no requiere red ni credenciales)**: al aplicar un evento de `ops/`, cada máquina compara el `timestamp` del evento contra **su propio reloj local** (el de quien está leyendo, no el de quien lo escribió). Si el evento viene marcado más de **5 minutos en el futuro** respecto al reloj de la máquina que lo procesa, se trata como sospechoso: se aplica igual (para no bloquear el flujo), pero se reordena usando **la hora de llegada a `ops/` en vez del timestamp declarado**, y se registra un aviso interno (no notificación al equipo, solo log). Esto no elimina el riesgo — todas las máquinas confían en su propio reloj, que también podría estar mal por error genuino — pero acota cuánta ventaja puede sacar alguien adelantando su reloj: ya no es ilimitada, queda topada a la tolerancia de 5 minutos frente al reloj de las demás máquinas del equipo.

**Integridad de los eventos — qué está firmado y qué no (resuelto)**: un evento de `ops/` es un JSON plano con `usuario` **autodeclarado** — nada impide, en principio, que alguien con acceso de escritura a la carpeta (todo el equipo, por diseño) suelte un evento suplantando a otro miembro. Distinguir dos niveles de gravedad en vez de tratar esto como un solo problema:
- **Eventos que solo un Administrador (o un Líder certificado, dentro de lo que puede asignar) puede emitir legítimamente** (`crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`, `compactar_eventos`, ver [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac); **actualizado 2026-11-07**: `asignar_rol` ya no existe, lo reemplaza `cambiar_rol`, y los Líderes también firman, ver [Firma de los Líderes](#firma-de-los-líderes-nuevo-2026-11-07)): **se extiende la misma firma Ed25519** que ya protege el campo `rol` de `usuarios` a estos eventos completos — el evento incluye un campo `firma_admin` (firma del JSON del evento con la llave privada del Administrador), y cada máquina la verifica contra la llave pública embebida antes de aplicarlo; si no coincide o falta, el evento se descarta sin aplicar (no se degrada, se ignora — a diferencia de una fila de `usuarios` inconsistente, aquí no hay "rol mínimo" al que degradar un evento). Esto cierra el caso más grave: nadie puede forjar una elevación de rol ni una acción administrativa aunque tenga acceso de escritura a `ops/`.
- **Eventos del día a día de un miembro regular** (`tomar_tarea`, `cambiar_estado`, `eliminar_tarea` sobre su propio trabajo, etc.): **quedan sin firmar, riesgo aceptado**. Firmar esto requeriría un par de llaves por cada persona del equipo (no solo el Administrador) — una PKI completa, desproporcionada para 3 personas de confianza. Además, quien tiene acceso de escritura a la carpeta compartida por diseño ya puede, siendo honesto, borrar o alterar tareas de cualquiera desde la propia app sin necesitar forjar nada — la firma no cambia esa exposición de fondo, que ya depende por completo de la confianza del equipo (mismo criterio que ya aceptan otros riesgos residuales de esta arquitectura, ver el de rollback de `.db` más abajo).

**Intervalo de escaneo de `ops/` (resuelto)**: tres disparadores, no uno solo —
1. **Al abrir la app**: escaneo inmediato antes de mostrar cualquier tablero, para arrancar con el estado más reciente posible.
2. **Al volver la app a primer plano** (el mismo momento en que se repite la [escucha de Unity VC](#detección-de-cambios-en-unity-version-control-polling)). **Actualizado (2026-11-07)**: el frontend no tiene un botón "Refrescar"; todo llega solo.
3. **Temporizador de fondo cada 2 minutos** mientras la app está abierta: más frecuente que el polling de Unity VC (que sí cuesta red/proceso por invocar `cm`), porque escanear `ops/` es solo leer archivos de una carpeta ya sincronizada localmente por Drive/OneDrive — barato, no justifica esperar al intervalo largo de `cm`.

**Compactación/purga de `ops/` (resuelto)**: la carpeta crecería sin límite si nunca se limpia. Diseño: comando Tauri `compactar_eventos()`, **exclusivo Administrador**, disponible desde [Ajustes](./FRONTEND.md#ajustes) —
- Calcula el estado resultante de todos los eventos con más de **30 días** de antigüedad y lo guarda como `ops/snapshot_<fecha>.json` (una sola foto consolidada, no un evento más).
- Borra de `ops/` todos los eventos ya incluidos en ese snapshot.
- Al reconstruir su `.db` local, cualquier máquina parte del snapshot más reciente en vez de reproducir todo el historial desde cero, y solo aplica los eventos posteriores a él.
- **Riesgo aceptado**: una máquina que estuvo offline por más de 30 días y no alcanzó a aplicar eventos ya purgados los pierde (su estado diverge del snapshot en esa franja). Mitigación: la UI de `compactar_eventos()` exige confirmar que todo el equipo sincronizó recientemente antes de habilitar el botón (mismo patrón de doble candado que otras acciones destructivas del Administrador), y no corre automáticamente.

**Qué comandos Tauri pasan por `ops/` y cuáles quedan fuera (resuelto)**:
- **Sí migran a eventos** (mutan estado compartido entre el equipo). **Actualizado (2026-11-07)** con todas las acciones del frontend:
  - Tareas: `crear_tarea` (y `crear_tarea_con_subtareas`), `editar_tarea`, `proponer_edicion`, `tomar_tarea`, `asignar_tarea`, `desasignar_tarea`, `cambiar_estado` (incluye mandar a revisión), `borrar_progreso`, `tarea_cerrada_por_deteccion`, `eliminar_tarea`, `eliminar_definitivamente`.
  - Pendientes: `aprobar_tarea` / `aprobar_tareas`, `editar_y_aprobar_tarea`, `advertir_tarea_pendiente`, `aprobar_modificacion` / `aprobar_modificaciones`, `rechazar_modificacion`.
  - Revisión: `autorizar_merge`, `rechazar_revision`, `reabrir_tarea`, `asignar_revision`.
  - Ramas: `hacer_checkin`, `crear_rama_desde_espera`, `traer_cambios`, `devolver_cambios_prestados`, `eliminar_cambios_ajenos`, `mover_cambios_entre_ramas` (sin pantalla en el mockup).
  - Planificación: `crear_sprint`, `cerrar_sprint`, `asignar_tarea_a_sprint`, `asignar_tarea_a_backlog`, `asignar_backlog_a_sprint`.
  - Equipo y proyecto: `crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`, `renombrar_proyecto`, `actualizar_configuracion_proyecto`, `subir_imagen_proyecto`, `solicitar_accion_bd`, `enviar_mensaje`.
  - Documentos y chat: `subir_documentos`, `aprobar_documento`, `eliminar_documento`, `enviar_chat`, `crear_grupo_chat`.
  - Sin pantalla en el mockup: `crear_reporte_testing`.
  - En general: cualquier comando que escriba en `tareas`, `usuarios`, `ediciones_pendientes`, `reportes_testing`, `notificaciones` (la fila de notificación misma), `sprints`, `reportes_sprint`, `cambios_prestados`, `documentos`, `mensajes_chat`, `grupos_chat` o `configuracion_proyecto`.
- **Quedan fuera, son locales por máquina/usuario** (no tienen por qué ser iguales entre todo el equipo):
  - `marcar_leida(notificacion_id)`, `desmarcar_leida(notificacion_id)` y `marcar_todas_leidas()` — el estado de "leída" es personal de quien la lee en su propia máquina; si la misma persona usa dos máquinas, no es grave que difiera.
  - `marcar_chat_leido(conversacion)` y `eliminar_chat(conversacion)` — "leído" y "eliminar chat" son de cada persona (eliminar un chat solo lo oculta para ti, ver [Chat](#chat)).
  - `marcar_visto(panel, huella)` — los puntos de aviso del menú (ver [Puntos de aviso del menú](#puntos-de-aviso-del-menú)).
  - `abrir_en_unity`, `unir_cambios_entrantes`, `abrir_documento` — actúan solo sobre el workspace o la máquina de quien los usa.
  - `restaurar_backup(nombre_archivo)` y `listar_backups()` — actúan sobre backups locales de esa máquina (ver [Copias de seguridad](#copias-de-seguridad-y-restauración-de-la-bd)); restaurar un backup ya vuelve a dejar esa máquina desincronizada del resto a propósito (caso de emergencia), no algo que deba replicarse como evento normal.
  - `compactar_eventos()` — escribe directamente el snapshot en `ops/`, no es en sí mismo un evento que otras máquinas deban "aplicar" (ya leen el snapshot como parte del escaneo normal).
  - Comandos de solo lectura (`listar_*`, `obtener_notificaciones`, `revisar_actualizaciones`, `cm whoami`, etc.) — no mutan nada.

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

## Guardia de rama incorrecta al sincronizar o crear subtarea

El [auto-switch al tomar una tarea](#alcance) reduce el riesgo, pero no lo elimina del todo — alguien podría cambiar de rama manualmente en su cliente de Unity VC después, sin darse cuenta de que sigue ahí al volver a BranchBoard. Antes de **sincronizar** o **crear una subtarea** (acciones que escriben directamente sobre una rama específica), el backend corre `cm status` (o equivalente) para leer la rama activa del workspace y la compara contra `rama_asociada` de la tarea sobre la que se está actuando:

- **Coincide** → la acción sigue normal, sin interrupciones.
- **No coincide** → se bloquea la acción y se muestra un aviso claro: "Tu workspace está en la rama `X`, pero esta tarea es de la rama `Y` — cambia de rama antes de continuar" — con un botón para que BranchBoard mismo ejecute el `cm switch` correcto ahí, en vez de que la persona tenga que hacerlo a mano.

Esto evita el caso real que motivó el auto-switch: mezclar cambios de dos tareas distintas en la misma rama sin querer.

**Aclarado: esta guardia no aplica a "mandar a revisión"**. Mandar una tarea a revisión es solo un cambio de estado (con su propio doble candado de escribir "mandar a revisar", ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)) — no requiere que el workspace esté físicamente parado en la rama de esa tarea en ese momento, a diferencia de sincronizar o crear una subtarea.

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

**Actualizado (2026-10-31) — la app escucha siempre, no solo con "Refrescar"**: lo normal es que los check-ins se hagan **desde Unity** (plugin de Unity Version Control); hacerlos desde BranchBoard es una forma extra, sin tener que abrir la interfaz de UVCS. Por eso BranchBoard **tiene que estar escuchando Unity VC todo el tiempo** mientras está abierta, y no depender de que alguien le dé a "Refrescar" (que se mantiene como acción manual inmediata). Esto reemplaza el "no hace falta un proceso en segundo plano para el MVP" del paso 2:

- **Proceso de fondo** mientras la app está abierta: repite el chequeo de los pasos 3–5 cada pocos segundos para las ramas activas (las de tareas abiertas y sus padres) y además al volver la app a primer plano. El intervalo exacto se ajusta al implementar (equilibrio entre inmediatez y el costo de invocar `cm`); los triggers del servidor (paso 6) siguen siendo la mejora futura para tiempo real. **Según el frontend**: no hay botón "Refrescar" en la interfaz, así que este proceso de fondo es el único camino (reemplaza al paso 2).
- **Check-ins hechos desde Unity**: un changeset nuevo en la rama de una tarea aparece solo en BranchBoard (commits de la tarea, Historial, Revisión) sin que nadie haga nada. Si son **cambios sin check-in en la rama de una tarea con subtareas**, aparecen como su subtarea **"En espera"** (ver [Check-ins de la rama](#check-ins-de-la-rama-subtarea-en-espera-y-cambios-entrantes)).
- **Workspace**: también se escucha en qué rama está parado el workspace local (por si alguien cambió de rama desde Unity o desde el cliente de UVCS) y BranchBoard actualiza su "rama actual del workspace" (la que muestra el panel "Abrir en Unity"); si queda en `dev` o en una rama que no es de sus tareas, aplica el aviso de [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app).
- Lo detectado que es solo indicador local (`modificada`, `ultimo_changeset_visto`, rama del workspace) no pasa por `ops/`; lo que muta estado compartido sigue emitiendo su evento (ver abajo).

**Corregido — el paso 5 debe generar un evento (`revisar_actualizaciones` por sí solo no basta)**: `revisar_actualizaciones` está listado entre los comandos **locales/solo lectura** que no pasan por `ops/` (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)), lo cual es correcto para marcar `modificada`/`ultimo_changeset_visto` (son solo indicadores locales de esa máquina, sin problema si difieren). Pero **el paso 5 sí muta estado compartido** (`estado = 'terminada'`, `fecha_completada`) — si eso se queda solo en el `.db` local de quien hizo el refresco, las demás máquinas nunca se enteran, y se pierde al reconstruir el `.db` desde `ops/` (que no tiene registro de este cambio). Cuando el paso 5 detecta el cierre de rama, además de actualizar localmente, emite un evento nuevo `tarea_cerrada_por_deteccion(tarea_id, fecha_completada)` a `ops/` — ese evento sí migra (agregarlo a la lista de comandos que migran a eventos), y se aplica igual en cualquier máquina que lo reproduzca, sin depender de que esa máquina específica haya hecho el refresco.

## Detección de ediciones en rama ajena (fuera de la app)

**Problema real señalado**: Unity Version Control tiene su propio sistema nativo de permisos por rama, pero **actualmente no está funcionando de forma confiable** — no se puede depender de que Unity VC por sí solo impida que alguien haga cambios en una rama que no le corresponde. BranchBoard necesita su propia red de seguridad encima.

**Dentro de BranchBoard esto ya no puede pasar por diseño**: cualquiera puede **abrir** la tarea de otra persona y **ver qué cambios tiene** (vía [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling)), pero **no puede asignársela** si ya la tomó alguien más (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos): el evento `tomar_tarea` se descarta si ya fue tomada) ni **mandarla a revisión** si no es la persona a la que está asignada — la app misma no expone o rechaza esas acciones sobre una tarea que no es tuya.

El caso real que sí puede pasar es cuando alguien **sale de BranchBoard y usa el cliente de Unity Version Control directamente**: ahí sí puede cambiarse a mano a la rama de un compañero y hacer checkins, sin pasar por ninguna validación de la app. BranchBoard no puede impedir esto (ocurre completamente fuera de su control), solo detectarlo después:

- Cada vez que el backend hace polling (ver [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling)) o el temporizador de [recordatorio de inactividad](#notificaciones) corre, además revisa: **¿la rama actual del workspace de esta persona (`cm status --header`) coincide con alguna tarea que tiene asignada?**
- Si el workspace está parado en una rama que **no es una de sus tareas asignadas** (ej. la rama de la tarea de otro compañero, o `dev`) **y** hay cambios pendientes o checkins recientes ahí (`cm status` con archivos modificados, o un changeset nuevo del autor equivocado) → se dispara una **alerta inmediata** en pantalla (no solo una notificación de campana, dado lo grave del error): *"Estás haciendo cambios en la rama de la tarea [X], que no te pertenece. Cambia a tu rama asignada [Y] antes de continuar."*
- La alerta no deshace nada automáticamente — los cambios ya hechos siguen ahí (podrían perderse si se descartan sin cuidado); solo advierte para que la persona decida cómo corregirlo. **Definido**: ya no hace falta corregirlo a mano fuera de la app — la persona (o, si son ramas de otra persona, un Líder/Administrador) puede usar [Mover cambios entre ramas](#mover-cambios-entre-ramas) para llevar ese trabajo a la rama correcta directamente desde BranchBoard.
- Esto es una **detección después del hecho**, no una prevención — no reemplaza al auto-switch de [tomar/asignar tarea](#alcance), que es la primera línea de defensa, ni a la [Guardia de rama incorrecta al sincronizar o crear subtarea](#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea); esta detección es para cuando, aun así, alguien termina escribiendo en la rama equivocada desde fuera de BranchBoard.

**Pendiente por definir**: cada cuánto corre esta revisión (¿mismo intervalo que el recordatorio de inactividad, o más frecuente dado que es un caso de seguridad?), y si además debe avisarle a un Líder/Administrador cuando pasa, no solo a la persona que se equivocó.

**Pendiente por definir (señalado por el usuario)**: la alerta debe dispararse por **cambios bruscos/significativos**, no por cualquier cambio mínimo — Unity Version Control genera por su cuenta archivos y cambios automáticos (ej. `.meta`, u otros que no tocó la persona a propósito) que no deberían contar como "editando la rama de otro". Cómo distinguir exactamente un cambio real de uno automático de Unity queda para después, no resuelto todavía.

## Comando "Guardar cambios y sincronizar"

**Actualizado (2026-11-07, según el frontend)**: el frontend no tiene un botón "Guardar cambios y sincronizar". Lo que existe es **hacer check-in desde BranchBoard** en los detalles de una tarea sin subtareas (sección "Pending changes" → **"Hacer check-in"**, con título, descripción y tipo) y **"Crear rama"** en una tarea con subtareas — ver [Check-ins de la rama](#check-ins-de-la-rama-subtarea-en-espera-y-cambios-entrantes). Los comandos son `hacer_checkin(rama, titulo, descripcion, tipo)` y `crear_rama_desde_espera(espera_id, titulo, detalles, tipo)`; usan el mismo cable React → comando Tauri → Rust → `cm` que "tomar tarea". Al terminar, la escucha de Unity VC refleja el changeset nuevo sin pasos extra.

## Bloquear el cierre de la app durante una operación

Igual que hace Unity Editor mientras guarda/compila: mientras hay una operación de `cm` en curso (crear rama, sincronizar, merge, etc.), no se debe poder cerrar la app a la mitad.

- El backend mantiene una bandera en memoria `operacion_en_curso: bool` (o un contador, si puede haber más de una en paralelo), que se activa justo antes de invocar `cm` y se desactiva cuando el subproceso termina (éxito o error).
- Comando Tauri `hay_operacion_en_curso()` para que el frontend lo consulte antes de permitir cerrar la ventana.
- Tauri permite interceptar el evento de cierre de ventana (`onCloseRequested` o equivalente) para cancelarlo y mostrar un aviso en vez de cerrar de inmediato — esa parte de UI queda para [FRONTEND.md](./FRONTEND.md#navegación-general).

## Flujo de revisión y merge a `dev`

**Dos caminos, según quién trabajó la tarea** (definido):

- **Si quien trabajó la tarea es un Desarrollador**: no tiene permiso de mergear él mismo — su único camino es mandar la tarea a "en revisión" para que un Líder/Administrador la autorice (pasos 1-5 más abajo).
- **Si quien trabajó la tarea es un Líder o Administrador** (porque él mismo la tomó y la desarrolló): se salta por completo la carpeta "en revisión" y **mergea/conecta su propia rama a `dev` directamente** — ya tiene el permiso de autorizar, así que no tiene sentido pedirse permiso a sí mismo. Usa el mismo mecanismo del paso 3 (misma validación de orden de merge de abajo hacia arriba, mismo `cm merge` + `cm checkin`), disparado con un botón "Autorizar" directo sobre su propia tarjeta en el Tablero Scrum (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), sin pasar por "Tareas grandes". **Diferencia con el paso 4** si el merge falla por conflicto: como esta tarea nunca pasó por "en revisión", no puede "quedarse" ahí — se queda en **"en producción"** en su lugar; y la opción de "enviarla al Desarrollador para que la corrija" no aplica (no hay ningún Desarrollador en este camino, el propio Líder/Admin hizo el trabajo) — solo le queda resolverlo él mismo en el cliente de Unity VC y reintentar.

El resto de esta sección (pasos 1-5) describe el camino del Desarrollador:

1. El Desarrollador arrastra su tarea a **"En revisión"** en el Tablero Scrum, confirmando con la palabra **"mandar a revisar"** (ver [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)); **actualizado (2026-11-07)**: ya no se escribe una descripción. Se rechaza si la tarea sigue `pendiente_aprobacion` o tiene subtareas en "por hacer"/"en producción" (incluida una "En espera"). Si lleva [cambios prestados](#cambios-prestados-traer-cambios) sin devolver, se deja pasar con un aviso. Genera la notificación #5.
2. La tarea aparece automáticamente en la carpeta **"Tareas grandes"** del apartado **[Revisión](./PLANNING.md#revisión)** (página de estado de PR) — visible para **todos los roles**, pero Desarrollador y Tester la ven en **solo lectura** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).
   - **Actualizado (2026-11-07, según el frontend) — elegir revisor**: comando `asignar_revision(tarea_id, email_lider)`, con la palabra **"asignar"**. Mientras la tarea **no tiene revisor**, lo pueden elegir un **Líder o el Administrador** (puede ser cualquier Líder o Administrador, también uno mismo). Una vez asignado, **solo el Administrador** puede cambiarlo. Genera la notificación #6 al revisor elegido.
   - `autoasignar_revision` y la opción "Asignármela" **ya no existen**: un Líder se asigna a sí mismo eligiéndose en el mismo selector.
   - **Quién autoriza o rechaza**: en el frontend, cualquier Líder o Administrador, haya o no revisor asignado. El revisor asignado se muestra en la fila como responsable. (La regla anterior, "con revisor asignado solo él puede actuar, salvo el Administrador", no está en el frontend; queda **pendiente de decidir** si se implementa.)
3. Cuando el Líder/Admin marca la tarea como **"autorizado"**, el backend valida primero el **orden de merge de abajo hacia arriba** (ver [Subtareas en PLANNING.md](./PLANNING.md#subtareas-tareas-anidadas)): consulta `SELECT COUNT(*) FROM tareas WHERE tarea_padre_id = ? AND estado != 'terminada' AND eliminada = false`. Si el resultado es mayor a 0, **rechaza la acción antes de tocar `cm`** y muestra un error listando qué subtareas siguen sin terminar — no se puede mergear una rama padre mientras tenga hijas abiertas. Si pasa la validación, ejecuta el **merge real de la rama hacia `dev`** invocando `cm` como subproceso (comando exacto pendiente de verificar al implementar, ej. `cm merge` + `cm checkin`) — no es solo un cambio de estado en SQLite, sí toca Unity VC.
4. **Si el merge falla** (conflicto de contenido entre archivos): **decisión tomada** — BranchBoard no implementa resolución de conflictos propia, Unity Version Control ya tiene su propia herramienta para eso. El backend detecta que `cm merge` devolvió error y la tarea **no se marca como terminada** (se queda en "en revisión"). Como quien dispara el "autorizado" es el propio Líder/Admin (acción síncrona, no en segundo plano), la reacción es una **alerta/pantalla inmediata** en ese momento (no una notificación de campana) mostrando que el merge falló, con dos opciones para el Líder/Admin:
   - **Resolverlo él mismo**: cierra la alerta, va al cliente de Unity VC a resolver el conflicto manualmente, y luego reintenta el "autorizado" desde BranchBoard. No se notifica a nadie más.
   - **Enviarla al Desarrollador para que la corrija**: reutiliza el mismo mecanismo de **`rechazar_revision(tarea_id, motivo)`** (antes `rechazar_tarea`) — el motivo queda como "falló el merge por conflicto", la tarea vuelve al Desarrollador (con notificación normal de tarea rechazada), y él la reimplementa y la vuelve a mandar a revisión como cualquier rechazo.
5. Si se **rechaza** (`rechazar_revision(tarea_id, motivo)`, motivo obligatorio y palabra **"rechazar"**), la tarea vuelve a **"en producción"** (también sus subtareas), se guarda `motivo_rechazo` y se notifica al responsable (`asignado_a`, notificación #2). Sigue en la misma rama, sin perder historial.
6. Tras un merge exitoso, la tarea pasa a "terminada" con `fecha_completada` y se dispara la misma revisión de "Detección de cambios" (rama cerrada → tarea "terminada").

**Resuelto (2026-11-07, según el frontend)**: autorizar sí lleva confirmación escribiendo **"autorizar"** (sin descripción), tanto en Revisión como al arrastrar la tarjeta de "En revisión" a "Terminada" o con el botón "Autorizar (merge a dev)" del Líder/Administrador sobre su propia tarea. Antes de mergear se exige: todas las subtareas terminadas y **ningún cambio prestado sin devolver** (si quedan, se bloquea y se pide devolverlos).

**Nota**: este flujo completo es específico de la carpeta **"Tareas grandes"** de [Revisión](./PLANNING.md#revisión) (tarea general → `dev`). Para el flujo de **"Tareas pequeñas"** (subtarea → su tarea padre), ver la sección siguiente — es más simple y con otro dueño de la decisión.

## Flujo de merge de subtareas pequeñas (hacia su tarea padre)

A diferencia del flujo anterior, este **no** pasa por Líder/Administrador exclusivamente — quien autoriza es **quien esté a cargo de la tarea padre** (puede ser Desarrollador, Líder o Administrador, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).

**Actualizado (2026-11-07, según el frontend)**: "Tareas pequeñas" son **todas las subtareas** (tareas con `tarea_padre_id`) que están en "en revisión"; "Tareas grandes" son las que no tienen padre. Una subtarea llega a revisión igual que cualquier tarea (arrastrándola, con la palabra "mandar a revisar"). Ya no depende de un colaborador externo: el Colaborador externo **no trabaja subtareas** (ver [SEGURIDAD.md](./SEGURIDAD.md#colaborador-externo)). Autorizar y rechazar usan las mismas confirmaciones ("autorizar"; "rechazar" con motivo), y el merge va a la rama del padre, no a `dev`.

1. La subtarea aparece en la carpeta **"Tareas pequeñas"** de [Revisión](./PLANNING.md#revisión) cuando pasa a "en revisión".
2. Quien esté a cargo de la tarea padre abre la fila y ve el visor de [Pre-revisión](./PLANNING.md#pre-revisión) correspondiente al tipo (malla 3D, imagen, audio, video, etc.) — **sin cambiar de rama en su propio workspace** (ver [Guardia de rama incorrecta al sincronizar o crear subtarea](#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea), este flujo es precisamente el que motivó no depender de cambios de rama para revisar algo tan pequeño).
3. Comando Tauri `autorizar_merge(tarea_id)` (el mismo de "Tareas grandes"; **según el frontend** ya no hay un `autorizar_merge_subtarea` aparte): si la tarea tiene padre, ejecuta `cm merge` de su rama hacia la rama de su tarea padre (no hacia `dev`) — mismo patrón de invocar `cm` como subproceso. Valida que quien autoriza sea quien está a cargo de la tarea padre.
4. **Si falla por conflicto**: mismo patrón que el flujo de "Tareas grandes" — alerta síncrona inmediata para quien intentó autorizar, con las mismas dos opciones (resolverlo él mismo, o rechazar con `rechazar_revision`).
5. Si se **rechaza**: la subtarea vuelve a "en producción" con su responsable, sobre la misma rama hija (ver [Subtareas en PLANNING.md](./PLANNING.md#subtareas-tareas-anidadas)).
6. Tras un merge exitoso, la subtarea pasa a "terminada" — no dispara el flujo completo de "Detección de cambios" del punto anterior (no aplica el chequeo de changesets por polling para algo tan simple), solo el cambio de estado.

**Resuelto (según el frontend)**: lleva la misma confirmación que mergear a `dev` (escribir "autorizar").

## Estados, progreso y reaperturas

**Nuevo (2026-11-07, según el frontend)**. Reglas que el frontend ya aplica y que el backend tiene que validar igual (al ejecutar el comando y al reproducir el evento):

- **Estados**: por hacer / en producción / en revisión / terminada. "Sin asignar" no es un estado: es "por hacer" sin `asignado_a`.
- **El estado de una tarea con subtareas se calcula a partir de ellas** (se recalcula de abajo hacia arriba por toda la cadena de padres):
  - todas sus subtareas en "por hacer" → "por hacer", **salvo que la tarea padre ya tenga commits** (entonces "en producción");
  - alguna en "por hacer" o "en producción" → "en producción";
  - ninguna en "por hacer"/"en producción" → "en revisión".
  - "Terminada" es final: una tarea padre ya mergeada no se vuelve a calcular.
  - Si una tarea padre avanza por sus subtareas y no tiene responsable, hereda el de alguna de ellas (ninguna tarea fuera de "por hacer" queda sin responsable).
- **Progreso = commits**: una tarea (o cualquiera de sus subtareas) con commits no puede volver a "por hacer" ni soltarse; se avisa y se rechaza.
- **Mover hacia atrás** (`cambiar_estado` a "por hacer" o "en producción") mueve también todo su subárbol; hacia "en revisión" o "terminada" solo la tarea (sus subtareas ya tuvieron que llegar).
- **Borrar progreso** (`borrar_progreso(tarea_id)`, todos los roles, palabra **"eliminar"**): pasar de "en producción" a "por hacer" una tarea con progreso. Borra los commits de la tarea y sus subtareas, sus cambios prestados y sus cambios sin check-in, y la deja en "por hacer" (sigue tomada y con su rama). En Unity VC: revertir la rama a su changeset inicial (comando exacto pendiente de verificar).
- **Reabrir** (`reabrir_tarea(tarea_id, motivo)`, palabra **"reabrir"** y motivo obligatorio): una tarea "terminada" vuelve a "en revisión" (sus subtareas se quedan terminadas). Cada tarea guarda `reaperturas`; se puede reabrir **hasta 3 veces** y desde ahí **solo el Administrador** puede seguir reabriéndola. Notifica a todo Líder/Administrador (notificación `tarea_reabierta`, con el motivo).
- **De "En revisión" a "Terminada" arrastrando la tarjeta**: solo Líder/Administrador; es lo mismo que autorizar el merge (palabra "autorizar").
- **Saltar estados desde "Sin asignar"** (arrastrar directo a otra columna): primero se toma la tarea (se crea la rama) y luego sigue el flujo normal de la columna destino. Un Desarrollador o Tester no puede mandarla directo a "Terminada".
- **Colaborador externo**: solo toma o recibe tareas **individuales** (sin subtareas y que no sean subtarea de otra), ver [Alcance](#alcance).

## Eliminar una tarea

Comando Tauri `eliminar_tarea(tarea_id, justificacion)`, validado en backend para que solo Administrador o Líder puedan invocarlo (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)). Requiere el doble candado descrito en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop) (justificación + escribir "eliminar"). Al eliminarla también se limpian `pendiente_aprobacion` y la edición pendiente que tuviera.

- La `justificacion` se guarda en SQLite como parte del registro de auditoría (no se borra junto con la tarea, queda como historial de por qué se eliminó).
- **Confirmado**: si la tarea ya tenía una rama asociada en Unity VC, `eliminar_tarea` también **cierra esa rama** en Unity VC (invocando `cm` — comando exacto pendiente de verificar al implementar), en vez de dejarla huérfana. La `justificacion` queda como el motivo tanto del borrado en BranchBoard como del cierre de la rama.
- **Definido**: esto **no borra la tarea de inmediato** — la manda a la [Papelera](#papelera), desde donde se elimina definitivamente en un segundo paso. Rechazar una tarea de [Pendientes](./PLANNING.md#pendientes) usa este mismo comando (el Líder/Admin decide que no debe existir, así que la "elimina" igual que cualquier otra tarea).
- **Señalado por el usuario**: `eliminar_tarea` también genera una notificación al **creador** de la tarea (ver [Notificaciones](#notificaciones), punto 13) — la `justificacion` escrita es lo que arma el mensaje.
- **Definido — borrado en cascada de subtareas** (el mockup todavía no lo hace: hoy solo manda a la Papelera la tarea elegida; el backend sí lo debe hacer): si la tarea tiene subtareas colgando (`tarea_padre_id` apuntando a ella, sin importar cuántos niveles de anidación), `eliminar_tarea` **arrastra en cascada solo las subtareas que siguen activas** (`estado` distinto de `terminada` y `eliminada = false`) — se mandan a Papelera junto con la tarea padre, en la misma operación, con la misma `justificacion`, y sus ramas también se cierran en Unity VC. Razón: una vez se cierra la rama padre, dejar una subtarea activa colgando de ella la deja huérfana (su flujo de merge apunta justo a esa rama, ver [Flujo de merge de subtareas pequeñas](#flujo-de-merge-de-subtareas-pequeñas-hacia-su-tarea-padre)).
  - **Las subtareas ya `terminada` (mergeadas) no se tocan** — su trabajo ya está integrado en el historial de la rama padre antes de cerrarla; borrar la tarea padre no debe deshacer trabajo ya completado y mergeado.
  - Cada creador de una subtarea arrastrada recibe la misma notificación #13 que el creador de la tarea padre, con una `justificacion` que aclara que fue un borrado en cascada (ej. "eliminada junto con su tarea padre: <motivo>").

## Papelera

**Nuevo**, va en el menú justo después de "Pendientes". **Visible para todos los roles** (Desarrollador y Tester la ven en **solo lectura**, igual que la carpeta "Tareas grandes" de Revisión) — lista todas las tareas con `eliminada = true`, sin importar si llegaron ahí por `eliminar_tarea` directo o por el rechazo de una tarea de Pendientes.

- Comando Tauri `eliminar_definitivamente(tarea_id)`, **exclusivo Líder/Administrador**, mismo doble candado que el resto de acciones destructivas (escribir "eliminar" para habilitar el botón).
- A diferencia de `eliminar_tarea` (que ya cerró la rama pero conserva la fila como soft-delete para historial), esto **borra la fila por completo** de la tabla `tareas` y purga la rama en Unity VC en vez de solo cerrarla (comando `cm` exacto pendiente de verificar al implementar — depende de si Unity VC permite borrar una rama cerrada o solo cerrarla para siempre).
- **Definido — misma cascada que `eliminar_tarea`**: como el borrado en cascada ya ocurrió al mandar la tarea a Papelera (ver [Eliminar una tarea](#eliminar-una-tarea)), toda subtarea activa que llegó junto a su padre ya está en Papelera también — `eliminar_definitivamente(tarea_id)` sobre la tarea padre purga en la misma operación esas subtareas que llegaron con ella. No aplica a subtareas `terminada` (nunca llegaron a Papelera, siguen existiendo normalmente).
- No hay acción de "restaurar" definida — una vez en Papelera, el único destino es la eliminación definitiva.
- **Según el frontend**: la lista va de la eliminación más reciente a la más antigua (a igual fecha, por id) y muestra cuántas hay. Abrir una tarea muestra primero **"Por qué se eliminó"** (`justificacion_eliminacion` y `fecha_eliminacion`) y después sus datos.

## Mover cambios entre ramas

**Sin pantalla en el mockup (2026-11-07)**: el frontend no tiene la opción "Mover cambios" (solo existe el tipo de notificación `mover_cambios`). Lo que sí tiene es [Traer cambios](#cambios-prestados-traer-cambios) (préstamos) y "Crear rama" desde una "En espera". Queda **pendiente de decidir** si se agrega al frontend o se descarta; mientras tanto, el diseño de abajo se conserva como referencia.

Resuelve en la práctica el caso que [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) antes dejaba como "corrígelo manualmente" — BranchBoard ofrecería una acción real para mover el trabajo a la rama correcta.

- **UI**: menú de tres puntos sobre la fila de cualquier tarea/rama, opción **"Mover cambios"** — disponible en **cualquier rama que exista**, incluidas las que están "en revisión" y las que están en [Papelera](#papelera) (antes de que se eliminen definitivamente).
- Al elegir la opción, se selecciona la **rama destino** (cualquier otra tarea existente) y el **alcance** de lo que se mueve:
  - **Un solo commit** específico (se elige de la lista de changesets de la rama origen)
  - **Todos los cambios** (todo el historial de la rama origen hacia la destino)
  - **Cambios sin commitear** (trabajo en el workspace que todavía no se guardó con un checkin)
- Comando Tauri `mover_cambios_entre_ramas(rama_origen_id, rama_destino_id, modo, commit_id?)` (**corregido**: nombre unificado — el resto del documento ya lo refería como `mover_cambios_entre_ramas`) — `modo` ∈ `commit_unico` / `todos_los_cambios` / `sin_commitear`. Comando `cm` exacto pendiente de verificar al implementar para cada modo (ej. cherry-pick para un commit, merge/rebase para todos los cambios, shelve/unshelve para lo no commiteado).
- **Permisos** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): cualquier usuario puede mover cambios **entre ramas que le pertenecen a él mismo** (ej. entre una tarea y su subtarea). Mover cambios **entre ramas de dos personas distintas es exclusivo de Líder/Administrador**.
- **Definido**: cuando un Líder/Administrador mueve cambios hacia la rama de otra persona, se le manda una notificación al dueño de esa rama avisándole — a diferencia del resto de notificaciones del sistema, **esta no es automática**: quien mueve los cambios debe escribir un **nombre** (título corto) y una **descripción** explicando qué se movió y por qué, y eso es lo que se le muestra al dueño de la rama en la notificación.

## Cambios prestados ("Traer cambios")

**Nuevo (2026-11-01)**. "Traer cambios" (en los detalles de una tarea) sirve para usar trabajo de otra tarea: se trae a la rama de esta tarea para poder trabajar con él, pero **no le pertenece**. **Según el frontend (2026-11-07)**, se puede traer de tareas **terminadas**, **en revisión** (con el aviso "sin aprobar") o **en producción con commits** (con el aviso "en curso"); de una tarea completa (`traer_cambios(destino_id, origen_id)`) o de un solo commit (`hash`). Se confirma escribiendo **"traer"**. Por eso se trata como un **préstamo**: se usa sin modificarlo y se devuelve antes del merge. No es lo mismo que [Mover cambios entre ramas](#mover-cambios-entre-ramas), que traslada trabajo de forma definitiva.

**Cuándo se puede traer** (todos los roles, salvo lo indicado):

- En **"Por hacer"** (ya tomada, con rama) y en **"En producción"**.
- En **"En revisión"**, solo **Líder/Administrador**, que son quienes la mandan a terminada.
- **"En espera"** nunca: sus cambios todavía están sin guardar.
- **Check-in primero**: en "En producción", si la rama tiene cambios sin check-in (`cm status`), no se trae nada hasta hacer el check-in. Así el trabajo propio queda guardado aparte y no se mezcla con lo prestado.

**Registro del préstamo**: al traer, el backend guarda (tabla `cambios_prestados`, evento `traer_cambios` en `ops/`):

- tarea destino, tarea origen, commit puntual o "todos sus cambios", quién y cuándo;
- el **changeset del merge** y la **lista de archivos que llegaron** (`cm diff` entre el changeset anterior al merge y el del merge).

Se muestra en los detalles de la tarea, en la sección **"Cambios prestados"**.

**Devolver** (`devolver_cambios_prestados(prestamo_id)`): botón **"Devolver"** en cada préstamo. En Unity VC es un **merge sustractivo** del changeset que se trajo (`cm merge --subtractive`; comando exacto pendiente de verificar al implementar). El registro no se borra: queda como historial ("Devuelto el … por …").

- **A revisión** se puede mandar con préstamos sin devolver (a veces hacen falta para revisarla): solo se avisa.
- **A terminada (merge)** no: se bloquea hasta devolverlos todos. Si no los devolvió quien trabajó la tarea, los devuelve el revisor antes de autorizar.
- Cuenta también los préstamos de sus subtareas: un check-in trabaja sobre la rama del padre.

### Detección automática de cambios ajenos tocados

**Problema**: quien trajo los cambios puede modificar, sin darse cuenta, archivos que llegaron en el préstamo. Esas modificaciones no le corresponden a su tarea y, al devolver el préstamo, quedarían mezcladas o darían conflicto.

**Detección (automática, sin intervención)**: la [escucha de Unity VC](#detección-de-cambios-en-unity-version-control-polling) ya ve cada check-in nuevo y los cambios sin check-in de la rama. Para cada préstamo activo, el backend **cruza esos archivos con la lista de archivos prestados**:

- **Cambios sin check-in** sobre un archivo prestado: se detectan antes de que se guarden.
- **Check-in** que modifica un archivo prestado: se detecta en cuanto aparece el changeset (`cm diff` del changeset contra su anterior, intersectado con la lista del préstamo).
- Los archivos detectados se guardan en el préstamo (`archivos_tocados`).

**Alerta**: al abrir la tarea (detalles en el Tablero Scrum, en el tablero de categoría o en Revisión) sale sola una ventana **"Se tocaron cambios ajenos en esta tarea"** con la lista de archivos y de qué tarea vienen. Tiene **dos botones**:

- **"Aceptar y eliminar"**: elimina esas modificaciones (ver abajo).
- **"Continuar sin eliminar"**: solo cierra el aviso, no borra nada. La **X** hace lo mismo. En los detalles queda un recuadro rojo con los archivos y un botón **"Eliminar"** para hacerlo después. Mientras no se eliminen, la tarea **no puede pasar a terminada**, y el aviso vuelve a salir cada vez que se abre la tarea.

**Con "Aceptar y eliminar" (o con "Eliminar") se eliminan** (`eliminar_cambios_ajenos(prestamo_id)`): cada archivo tocado **vuelve a como llegó en el préstamo** (se restaura su revisión del changeset del merge). El préstamo sigue activo hasta que se devuelva, como cualquier otro.

- En archivos de texto (scripts `.cs`) se restaura el archivo completo a la revisión prestada. Si en el mismo archivo hubo cambios propios legítimos, también se pierden: el archivo era ajeno.
- En escenas, prefabs y binarios (texturas, modelos, audio) no se puede separar por dentro: se restaura el archivo entero.
- Lo restaurado se guarda con un check-in automático en la rama de la tarea, con comentario "Se eliminaron cambios ajenos (prestados de T-xxx)", para que quede en el historial (evento `eliminar_cambios_ajenos` en `ops/`).

**Pendiente por definir**: excluir los archivos que Unity modifica solo (ej. `.meta`), igual que en [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app), para que no cuenten como "tocados".

## Datos para la gráfica de Avance

**Actualizado (2026-11-07, según el frontend)**. Ver el diseño en [FRONTEND.md → Alcance](./FRONTEND.md#alcance) ("Tablero Avance", "Backlogs en Avance" y "Reportes"). Todo se calcula en SQLite con dos fechas por tarea, sin `cm`:

- `fecha_creacion` (AAAA-MM-DD, fecha **local** del equipo, se pone sola al crear la tarea) y `fecha_completada` (al pasar a "terminada"; se borra si se reabre).
- **Sprint** (`con_fecha = true`): tareas restantes por día entre `fecha_inicio` y `fecha_fin` = total − terminadas hasta ese día; línea ideal recta de total a 0; proyección por **regresión lineal** de los días ya transcurridos (el cierre estimado es el primer día en que la recta llega a 0). Un sprint por empezar solo muestra la línea ideal. Un sprint cerrado se grafica con la **copia de sus tareas al cierre** (ver `reportes_sprint` en [Modelo de datos: tabla `sprints`](#modelo-de-datos-tabla-sprints)).
- **Backlog** (`con_fecha = false`): restantes por día desde su primera tarea hasta hoy = creadas − terminadas (sube cuando entran tareas, baja cuando se cierran). Ritmo = tareas terminadas en las **últimas 4 semanas**; también se muestran las que entran por semana. Cierre estimado = restantes ÷ ritmo (sin ritmo: "Sin avance todavía"; sin restantes: "Terminado" con la fecha del último cierre).
- **General**: no es un backlog guardado; es el avance de todo el proyecto = todas las tareas que están en algún sprint o backlog (cada una una vez), calculado como un backlog.
- **Descartado**: la línea secundaria de "changesets hechos por día" de todo el repositorio no está en el frontend.

## Identidad de usuario

BranchBoard **no tiene sistema propio de login/usuarios**. La identidad se obtiene directamente de la sesión de Unity Version Control ya activa en la máquina:

- Al arrancar, el backend corre `cm whoami` (subproceso) y obtiene el email de la cuenta logueada (confirmado en pruebas reales: devuelve algo como `nombre@dominio.com`).
- Ese email es el identificador del usuario dentro de BranchBoard — no hay contraseña, sesión propia, ni token que gestionar.
- La tabla de usuarios en SQLite mapea **email (de `cm whoami`) → rol** (ver Roles y permisos). Un Líder o el Administrador da de alta cada email del equipo con su rol desde [Usuarios](./FRONTEND.md#usuarios) (`crear_usuario`); después de eso, cada persona que abre la app en su máquina queda identificada automáticamente.
- Requisito implícito: cada miembro del equipo ya debe tener el cliente de Unity Version Control instalado y con sesión iniciada (login por SSO/navegador, una sola vez) en su propia máquina — eso ya lo resuelve `cm`, BranchBoard no reimplementa nada de eso.
- **Confirmado**: aunque todo el equipo comparte el mismo workspace de Unity Version Control, cada persona inicia sesión con su propia cuenta/correo — `cm whoami` sí distingue a cada usuario individualmente, no hay una cuenta compartida.
- **Eliminar a alguien del equipo** (rol: Administrador o Líder, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)) es tan simple como borrar su fila en la tabla de usuarios: como la identidad se valida contra esa tabla cada vez que se abre la app (no hay sesión persistente que revocar), la persona pierde el acceso **dentro de BranchBoard** de inmediato la próxima vez que intente entrar — su correo ya no aparecería registrado.
  - **Corregido — esto NO revoca su acceso a la carpeta de Drive/OneDrive del proyecto**, que es donde vive todo el dato real (`ops/` con el historial completo de eventos, y todos los `.db` locales de cada quien parten de ahí) — ese acceso lo administra Drive/OneDrive directamente, fuera del control de BranchBoard, y la app no tiene (ni debería tener, ver [Sin servicios en la nube propios](./ARQUITECTURA.md#decisiones-de-arquitectura-ya-tomadas)) forma de tocarlo por API. Alguien eliminado sigue pudiendo leer todo el historial y, en el peor caso, seguir escribiendo eventos si conserva el acceso. `eliminar_usuario` por sí solo **no es suficiente** para sacar a alguien del proyecto.
  - Mitigación: la confirmación de `eliminar_usuario` en la UI incluye un recordatorio explícito, no evitable — "recuerda quitarle a [persona] el acceso a la carpeta de Drive/OneDrive del proyecto; BranchBoard no puede hacerlo por ti" — para que quede como paso manual explícito en vez de una falsa sensación de que ya quedó resuelto.
- **Actualizado (2026-11-02) — configuración automática, Administrador = dueño del repositorio**: BranchBoard detecta solo un proyecto de Unity VC sin `.db` y lo configura la primera vez que lo abre **el dueño del repositorio** (BranchBoard lo lee de Unity VC; comando `cm` exacto pendiente de verificar), que queda como **Administrador** y genera ahí las llaves de firma. Si lo abre otra persona antes, no se configura: se le avisa que espere a que lo abra su dueño. Así el Administrador nunca depende de quién llegó primero. Lo que sigue describe el diseño anterior, en que el dueño lo inicializaba a mano.
- **Bootstrapping (primer usuario de un proyecto nuevo) — corregido**: quien inicializa un `.db` de BranchBoard por primera vez para un workspace de Unity VC (ver flujo en [FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)) queda registrado con rol **Administrador**, no Líder. Es un cambio respecto al diseño anterior ("Líder, y el Admin se asigna después a mano"): con la [firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac), **solo un Administrador puede ejecutar los comandos que asignan/firman roles** (hoy `cambiar_rol` y `crear_usuario`; los Líderes firman solo dentro de lo que pueden asignar) — si el primer usuario quedara como Líder, nunca existiría nadie que pudiera convertirlo (o a cualquier otro) en Administrador, y el proyecto quedaría sin ancla de confianza de forma permanente. En el mismo momento de la inicialización, el backend genera el par de llaves Ed25519 y firma esa primera fila como Administrador. Consistente con que en Unity Cloud el "Propietario" ya es la persona identificada aparte, ver [Usuarios semilla en SEGURIDAD.md](./SEGURIDAD.md#usuarios-semilla-equipo-virustf) — normalmente coincide con quien inicializa el proyecto.
- **Quién necesita Unity Version Control instalado** — **corregido de nuevo, versión final**: en la práctica, **todos** los roles (incluido el Colaborador externo) necesitan tener `cm` instalado en su máquina. La diferencia real no es "instalado sí/no", sino **cuánto del proyecto se descarga**:
  - **Administrador, Líder, Desarrollador y Tester**: workspace **completo** — lo necesitan porque trabajan en Unity Editor (programar, modelar, etc.) o, en el caso del Tester, porque necesita **jugar/ejecutar el juego real** para encontrar errores, algo que ninguna interfaz de BranchBoard reemplaza.
  - **Colaborador externo** (rol desde 2026-11-07; alguien que sube archivos puntuales — imagen, audio, video, animación, `.fbx` — y nunca necesita jugar ni programar el juego): workspace **parcial**, ver [Workspace parcial (Gluon) para subtareas simples](#workspace-parcial-gluon-para-subtareas-simples) — nunca descarga el proyecto completo, solo la carpeta donde va su archivo. Solo trabaja **tareas individuales** (ni grandes ni subtareas).
  - Para **todos** los roles sin excepción: identidad vía `cm whoami`, workspace vía `cm workspace list` (ver arriba, sin pedir carpeta a mano). Si `cm` no está instalado, ver [Instalación de Unity Version Control](#instalación-de-unity-version-control) más abajo — el botón de instalar aplica a todos los roles.

### Sesión de Unity VC desde la app, proyectos y salir (según el frontend)

**Nuevo (2026-11-07)**. Lo que el frontend ya muestra en la [Pantalla inicial](./FRONTEND.md#autenticación--acceso-al-proyecto) y en el menú del proyecto (clic en el logo):

- **"Iniciar sesión con Unity VC"**: si no hay sesión (`cm whoami` falla), la app abre el inicio de sesión **oficial de Unity en el navegador** (SSO); BranchBoard nunca ve ni guarda la contraseña. Al volver, se vuelve a correr `cm whoami`. Comando `iniciar_sesion_unity_vc()` (invoca el login de `cm`; comando exacto pendiente de verificar).
- **"Cerrar sesión"** (menú del proyecto): cierra la sesión de Unity VC en esa máquina (`cerrar_sesion_unity_vc()`); la app queda en "No se detectó una sesión de Unity VC" y al volver a entrar pasa otra vez por el acceso al proyecto.
- **"Tus proyectos"**: los workspaces locales que ya usan BranchBoard (`cm workspace list` + si tienen datos de BranchBoard); elegir uno entra a ese proyecto (`entrar_a_proyecto`).
- **"Cargar proyecto"**: lista los repositorios del servidor de Unity VC a los que la persona tiene acceso y que todavía no están en ese equipo (`listar_proyectos_servidor()`, vía `cm repo list`), con la etiqueta "Con tareas" si ya usan BranchBoard. Elegir uno lleva a [Clonar proyecto](#clonar-proyecto-in-app).
- **"Nuevo proyecto"**: (a) **traer** un proyecto de Unity VC que ya está en el equipo pero nunca se configuró con BranchBoard (se configura como dice [Identidad](#identidad-de-usuario)), o (b) **crear** uno nuevo: se crea su repositorio y su workspace en Unity VC (`crear_proyecto(nombre, carpeta)`, comandos `cm repo create` + `cm workspace create`) y quien lo crea queda como Administrador.
- **"Salir de la app"**: cierra la ventana (respetando [Bloquear el cierre durante una operación](#bloquear-el-cierre-de-la-app-durante-una-operación)).
- **Proyectos sin Unity VC ("Continuar sin Unity VC")**: el frontend permite **crear un proyecto local** (nombre del proyecto, tu nombre, tu correo y una contraseña de mínimo 8 caracteres; quedas como Administrador y único usuario, sin repositorio) o **abrir la carpeta de un proyecto local** existente. En el mockup la contraseña no se guarda. **Pendiente de definir para el backend** (choca con "sin login propio"): cómo se guarda y verifica esa contraseña (solo un hash, nunca el texto), cómo se trabajan las tareas sin ramas de Unity VC, si se puede sumar más gente a un proyecto local y si se puede conectar a Unity VC después.

## Instalación de Unity Version Control

Mismo patrón ya usado para [Conexión VPN (Cloudflare WARP)](#conexión-vpn-cloudflare-warp): si `cm` no está instalado (sin importar el rol — **corregido**, tampoco se excluye al Colaborador externo, ver arriba), aparece un botón **"Instalar Unity Version Control"**.

- **Confirmado disponible en winget**: `winget install --id Codice.PlasticSCM.CloudEdition -e --accept-package-agreements --accept-source-agreements` — instala el cliente completo, que trae `cm` incluido (y con él, el modo Gluon para workspace parcial).
- Mismo fallback que WARP: si `winget` falla o no está disponible, el botón abre la página oficial de descarga de Unity Version Control en el navegador.
- Misma nota de UX: puede disparar un prompt de UAC, mostrar estado "instalando..." mientras corre.

## Workspace parcial (Gluon) para subtareas simples

**Decisión final** (reemplaza el intento anterior de API REST + Service Account, descartado — ver nota abajo): Unity Version Control tiene un modo llamado **Gluon**, diseñado oficialmente para artistas que necesitan trabajar con archivos grandes sin descargar ni gestionar el proyecto completo. Trae su propio soporte de CLI: **`cm partial`** (`cm partial add`, `cm partial checkin`, etc.), así que el backend lo puede manejar por subproceso exactamente igual que el resto de comandos `cm` — el colaborador nunca necesita abrir una interfaz aparte.

- El Colaborador externo instala Unity Version Control (mismo botón de winget que todos, ver arriba) — es liviano, no implica tener Unity Editor. **Según el frontend**, trabaja solo **tareas individuales** (sin subtareas y que no sean subtarea de otra); el nombre de esta sección ("subtareas simples") viene del diseño anterior.
- **Definido**: la carpeta a descargar no la calcula el backend en automático — **el propio colaborador la elige**, desde un selector con las mismas categorías de [Categorías en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp): Audio, Modelado 3D, Modelado 2D, Imagen, Animación, Música, Video, UI/Interfaz (Scripting y Testing no aplican a este perfil, no son subtareas simples de archivo). El backend crea/usa un **workspace parcial** con `cm partial` scopeado solo a esa carpeta elegida — nunca descarga el proyecto completo, y solo trae la categoría en la que la persona va a trabajar, ni siquiera las demás carpetas de assets.
- **Identidad: vuelve a ser `cm whoami`, igual que todos los demás roles.** No hace falta ninguna Service Account, ni API REST, ni llave cifrada compartida, ni contraseña de equipo — todo ese diseño queda descartado (ver nota abajo).
- Esto también evita por completo el problema que motivó la idea de la Service Account: como cada colaborador usa su propia cuenta real (no una cuenta de servicio), no choca con ninguna limitación de permisos de Unity Cloud.

**Por qué se descartó la API REST + Service Account** (para que quede el porqué, no solo el qué): se comprobó en la práctica, directamente en el Dashboard de Unity Cloud, que **las Service Accounts no pueden asignarse a ningún rol/permiso de Unity Version Control** (ni los roles genéricos de proyecto, ni los Custom Roles de Version Control — se probó buscándola en el selector de usuarios de un Custom Role y no aparece como opción). Es una limitación real de la plataforma en este momento, no algo que dependiera de elegir mejor el rol. Gluon evita el problema de raíz porque no depende de ninguna cuenta de servicio.

**Pendiente por definir**:
- Sintaxis exacta de `cm partial add` / `cm partial checkin` para el caso de uso (verificar al implementar).
- Mapeo exacto categoría→carpeta real del proyecto (ej. Audio → `Assets/MP3/`) — la categoría la elige el colaborador, pero la ruta destino la resuelve el backend. **Corregido**: la referencia anterior a un "ignore.conf del proyecto" no apuntaba a ningún archivo real (este repo es solo documentación, no contiene el proyecto Unity de VirusTF) — hay que confirmar el mapeo directamente contra la estructura de carpetas real del proyecto Unity al implementar, no contra un archivo de config que no existe.

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

### Validación de rutas de archivo/adjuntos (resuelto, previene path traversal)

Aplica a **todo** comando que escribe un archivo que llegó del frontend: `subir_subtarea_simple` (workspace parcial de Unity VC) y el adjunto opcional de `crear_reporte_testing` (ver [Modelo de datos: tabla `reportes_testing`](#modelo-de-datos-tabla-reportes_testing)).

- **El backend nunca usa el nombre de archivo que manda el frontend para construir la ruta final.** Genera su propio nombre (ej. `uuid` + la extensión ya validada por la tabla de arriba) — así un nombre malicioso tipo `../../otra_carpeta/archivo` nunca llega a formar parte de una ruta real, ni hace falta parsear/sanear el string a mano (frágil y fácil de dejar un caso sin cubrir).
- **Antes de escribir**, resuelve la ruta final a su forma canónica (`std::fs::canonicalize` o equivalente, sigue symlinks y colapsa `..`) y confirma con un chequeo de prefijo que sigue siendo descendiente de la carpeta permitida para ese caso — la carpeta del tipo dentro del workspace de Unity VC (subtareas simples) o la carpeta de adjuntos de reportes de Testing. Si no lo es, rechaza con error; nunca se intenta "corregir" la ruta, solo aceptar o rechazar.
- Mismo criterio aplica a cualquier comando futuro que reciba una ruta o nombre de archivo desde el frontend — ninguno pasa ese valor directo a una llamada de filesystem sin este chequeo (ver también [Allowlist de capabilities de Tauri en SEGURIDAD.md](./SEGURIDAD.md#allowlist-de-capabilities-de-tauri-resuelto), que ya limita a qué carpetas puede escribir el proceso en primer lugar — esta validación es la segunda capa, específica de cada comando).

## Clonar proyecto (in-app)

Para roles técnicos sin workspace todavía (ver [Instalación de Unity Version Control](#instalación-de-unity-version-control)), en vez de mandarlos a clonar por su cuenta en Unity Hub:

1. El frontend abre el selector de carpetas nativo de Tauri (diálogo del sistema operativo) para elegir dónde clonar
2. Comando Tauri `clonar_proyecto(ruta_destino)`: corre `cm workspace create` + el checkout correspondiente como subproceso, mostrando progreso
3. Al terminar, dispara el mismo flujo de `cm workspace list` para que ese workspace nuevo aparezca disponible en la Pantalla inicial
4. **Según el frontend**: se llega aquí desde la Pantalla inicial (si no hay ningún workspace) o desde "Cargar proyecto". El frontend muestra una barra de progreso y, si el proyecto ya usa BranchBoard, llega con sus tareas.

## Comando "Abrir en Unity"

Botón visible dentro de la app una vez hay un proyecto cargado (ver [Panel "Abrir en Unity" en FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), solo para roles con workspace local.

- Comando Tauri `abrir_en_unity(tarea_id, rama, changeset?)`: cambia el workspace a esa rama (`cm switch`, o a un changeset concreto si se pide) y lanza el Editor de Unity (o Unity Hub) apuntando a la ruta del workspace, vía su propio CLI — mismo patrón de invocar un ejecutable externo ya usado con `cm` y `warp-cli`. Es local a la máquina, no pasa por `ops/`.
- **Según el frontend (2026-11-07)**: el botón de la barra superior abre el panel **"¿Qué tarea vas a trabajar en Unity?"** con el nombre y rol de quien lo usa, la rama en la que está el workspace (**nunca `dev`**; si no está en ninguna tarea, "No estás en ninguna tarea") y sus **tareas abiertas** (asignadas a él, no terminadas). También hay "Abrir en Unity" en los detalles de una tarea, en cada check-in (abre ese changeset) y en la pantalla de éxito al tomar una tarea. El backend tiene que exponer la rama actual del workspace (`rama_actual_workspace()`, de `cm status --header`).
- Pendiente definir: comando exacto del CLI de Unity Hub para abrir un proyecto por ruta (verificar al implementar), y qué pasa si Unity Editor no está instalado (¿mismo patrón de "Instalar" con winget, o simplemente un mensaje de error?).

## Conexión VPN (Cloudflare WARP)

Problema real observado: en la red de la universidad, `cm` no logra conectar con el servidor de Unity Version Control (bloqueo de puerto). La solución probada en la práctica es activar **Cloudflare WARP** (cliente gratuito, `warp-cli`) — con la VPN activa, `cm` conecta sin problema.

Se agrega un **switch de encender/apagar WARP desde la pantalla inicial** de BranchBoard (ver [FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), para no depender de que el usuario sepa buscar el ícono de WARP en la bandeja del sistema.

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

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 5 roles (Administrador, Líder, Desarrollador, Tester y Colaborador externo — nuevo 2026-11-07, ver [SEGURIDAD.md](./SEGURIDAD.md#colaborador-externo)). El modelo de datos de usuario necesita un campo de rol asociado al email de `cm whoami` (ver Identidad de usuario), y cada comando Tauri sensible debe validar permisos en el backend (no solo ocultar en la UI).

**Comando faltante, ahora definido**: la fila "Modificar tareas de otros" de la matriz no tenía comando Tauri asociado — ver `editar_tarea_ajena` en [Notificaciones, punto 4](#notificaciones).

## Notificaciones

Tabla `notificaciones` en SQLite, con los campos del frontend: `id`, `tipo`, `para` (email destinatario), `titulo`, `cuerpo`, `leida`, `fecha`, `tarea_id` (opcional) y `accionable` (si ofrece un botón de acción directa, ej. aprobar). Tipos (los del frontend): `aprobacion_tarea` (#1), `tarea_rechazada` (#2), `tarea_asignada` (#3), `edicion_pendiente` (#4), `tarea_a_revision` (#5), `revision_asignada` (#6), `mensaje` (#7), `inactividad` (#8), `accion_bd` (#9), `mover_cambios` (#10), `editada_aprobada` (#11), `advertencia` (#12), `tarea_eliminada` (#13) y `tarea_reabierta` (#15). Casi todos estos casos disparan mensaje **automático**, sin acción manual de quien los envía más allá de la acción que los origina (aprobar, rechazar, asignar, etc.) — las excepciones son el punto 10 (mover cambios entre ramas de distintas personas) y el punto 12 (advertir sobre una tarea pendiente), que sí piden contenido escrito a mano.

**Aclarado**: todas las notificaciones de esta lista son **100% internas a la app** (campana + tabla SQLite) — no se manda ningún correo real por revisión, tareas o permisos. **Actualizado (2026-11-07)**: ya no hay ningún caso que use correo (solicitar ingreso por `mailto:` pasó a la versión 2, issue #57).

1. **Tarea creada por un Desarrollador o Tester esperando aprobación** — destinatarios: todos los usuarios con rol Líder o Administrador (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)). También queda listada en el apartado Pendientes, no solo como notificación de campana. Se resuelve con un comando Tauri `aprobar_tarea(tarea_id)` que solo Líder/Admin puede invocar (validación de rol en backend), y que solo pone `pendiente_aprobacion = false` — **no toca el campo `estado`**, que ya venía avanzando por su cuenta desde que se creó la tarea (la tarea nunca estuvo bloqueada: se podía tomar y trabajar desde el primer momento, solo no se podía mandar a revisión). **Nuevo — 4 acciones posibles** sobre esta tarea, todas exclusivas de Líder/Admin (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)):
   - `aprobar_tarea(tarea_id)` — tal como está (ya descrito arriba).
   - `editar_y_aprobar_tarea(tarea_id, campos_editados)` — modifica los campos de la tarea y pone `pendiente_aprobacion = false` en la misma operación (un solo comando, no dos). Genera la notificación #11.
   - `advertir_tarea_pendiente(tarea_id, mensaje)` — no toca ningún campo de la tarea (`pendiente_aprobacion` sigue en `true`), solo genera la notificación #12 con el mensaje escrito a mano por quien la envía.
   - `eliminar_tarea(tarea_id, justificacion)` — mismo comando ya descrito en [Eliminar una tarea](#eliminar-una-tarea), manda la tarea a Papelera.
   - **Definido**: mientras `pendiente_aprobacion = true`, el creador (Desarrollador/Tester) puede editar su propia tarea libremente desde Pendientes ("Editar" → "Guardar") con un `editar_tarea(tarea_id, campos_editados)` normal (sin aprobación de por medio, ya que la aprobación la da otra persona).
   - **Según el frontend**: "Aprobar" pide escribir **"aprobar"**; `aprobar_tareas(tarea_ids)` aprueba varias a la vez ("Marcar todo como revisado"). "Rechazar" es `eliminar_tarea` con justificación (palabra "rechazar"). Lo que crea un Colaborador externo también queda pendiente de aprobación, igual que lo de un Desarrollador o Tester.
2. **Tarea rechazada en revisión** — destinatario: el responsable de la tarea (`asignado_a`). Comando `rechazar_revision(tarea_id, motivo)` (Líder/Admin; en "Tareas pequeñas", quien está a cargo de la tarea padre). La tarea vuelve a "en producción", el motivo queda en `motivo_rechazo` y se muestra en la notificación. (Antes se llamaba `rechazar_tarea`.)
3. **Tarea asignada directamente por Administrador/Líder a un Desarrollador** — destinatario: ese Desarrollador. Comando Tauri nuevo `asignar_tarea(tarea_id, email_destinatario)` (solo Administrador/Líder) — a diferencia de "tomar tarea" (autoasignación), aquí alguien más asigna la tarea. Crea la rama igual que "tomar tarea" lo hace hoy.
4. **Edición pendiente de una tarea** — **actualizado (2026-11-07, según el frontend)**: desde los detalles de una tarea, **cualquier usuario** puede editar su **título y detalles**, pero **solo mientras está en "Sin asignar" o "Por hacer"** (desde que alguien la toma ya tiene rama en Unity VC). Comando `proponer_edicion(tarea_id, titulo, detalles)` (reemplaza a `editar_tarea_con_autorizacion`). Los valores nuevos se aplican **de inmediato**, pero la tarea queda marcada con **"edición pendiente"** (sombra amarilla bajo la tarjeta, ver [FRONTEND.md](./FRONTEND.md#alcance)) hasta que un Líder/Administrador la resuelva, y se notifica a todo Líder/Administrador (`edicion_pendiente`, accionable). Aparece en [Pendientes](./PLANNING.md#pendientes) con la etiqueta "Edición", mostrando lo anterior y lo nuevo campo por campo.
   - Antes de aplicar los cambios, el comando guarda una fila en la nueva tabla `ediciones_pendientes` (ver [Modelo de datos](#modelo-de-datos-tabla-ediciones_pendientes)) con los **valores anteriores** de los campos tocados — necesarios para poder revertir si se rechaza.
   - **`aprobar_modificacion(tarea_id)`** (Líder/Admin, palabra **"aprobar"**): borra la fila de `ediciones_pendientes` — los valores nuevos quedan como definitivos, desaparece la sombra amarilla y sale de Pendientes. `aprobar_modificaciones(tarea_ids)` hace lo mismo con varias a la vez ("Marcar todo como revisado").
   - **`rechazar_modificacion(tarea_id)`** (Líder/Admin, palabra **"rechazar"**): **restaura los campos de la tarea a los valores guardados en `campos_anteriores`** (también unos detalles anteriores vacíos), borra la fila de `ediciones_pendientes`. Notificar al autor del rechazo queda pendiente (el frontend hoy no lo hace).
   - **Una sola edición pendiente por tarea**: si se vuelve a editar una tarea que ya tiene una pendiente, los cambios se **suman a la misma fila**, conservando los valores anteriores **originales** (los que se restauran si se rechaza).
   - **`editar_tarea_ajena` (sin pantalla en el mockup)**: el frontend no tiene una edición directa sin aprobación para Líder/Administrador; hoy toda edición desde los detalles pasa por edición pendiente, y la notificación #14 no existe en el frontend. Queda **pendiente de decidir** si se agrega.
5. **Tarea llega a "en revisión"** — destinatarios: todos los usuarios con rol Líder o Administrador (no solo que aparezca en el apartado [Revisión](./PLANNING.md#revisión) — también genera notificación en la campana).
6. **Revisión asignada a un Líder específico** — destinatario: ese Líder. Generada por `asignar_revision(tarea_id, email_lider)` (Líder o Administrador si la tarea no tenía revisor; solo el Administrador si ya tenía). Mensaje tipo "se te asignó revisar [tarea]".
7. **Mensaje de un compañero** (nuevo 2026-11-07) — destinatario: la persona elegida en `enviar_mensaje`. Tipo `mensaje`, título "Mensaje de [nombre]", cuerpo = el texto. No es un chat; viaja como cualquier otra notificación.
8. **Recordatorio de inactividad en Unity Version Control**: si una tarea con rama (Scripting u otra tarea general) no tiene **changesets nuevos** en un tiempo determinado, se le manda al Desarrollador dueño un recordatorio del tipo "llevas X días sin actividad en [tarea], recuerda [qué le toca hacer]". Se apoya en el mismo mecanismo de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) (`cm log br:/<rama> --xml`), pero a diferencia del polling manual por "Refrescar", **este caso sí necesita revisarse solo, sin que nadie le dé clic a nada** — implica correr esta revisión con un temporizador mientras la app está abierta (ej. cada N horas), no solo bajo demanda. Pendiente definir: el umbral de tiempo exacto (¿3 días? ¿configurable por Líder/Admin?), y si esto corre también para subtareas simples.
9. **Solicitud de acción sobre la base de datos** — un Líder o Desarrollador que detecta un problema (ej. datos raros, algo se ve corrupto) no puede actuar directamente sobre la BD, así que le manda una notificación a **todos los Administradores** pidiendo que revisen/restauren. Comando Tauri `solicitar_accion_bd(motivo)`, disponible para Líder y Desarrollador (no para Tester, que no tiene relación con el ciclo de tareas/BD del equipo). El `motivo` escrito queda en el mensaje de la notificación para que el Admin sepa qué está pasando antes de actuar.
10. **Te movieron cambios a tu rama** — destinatario: el dueño de la rama destino, cuando un Líder/Administrador mueve cambios hacia una rama que no es la suya (ver [Mover cambios entre ramas](#mover-cambios-entre-ramas)). **No es automática**: quien mueve los cambios debe escribir un **nombre** (título corto) y una **descripción** al confirmar la acción — eso es lo que arma el mensaje de la notificación, ya que el sistema no puede inferir por su cuenta qué se movió ni por qué.
11. **Tu tarea pendiente fue editada y aprobada** — destinatario: el Desarrollador/Tester que creó la tarea. Generada por `editar_y_aprobar_tarea(tarea_id, campos_editados)` (ver punto 1). Automática — el mensaje puede armarse solo listando qué campos cambiaron.
12. **Advertencia sobre tu tarea pendiente** — destinatario: el Desarrollador/Tester que creó la tarea. Generada por `advertir_tarea_pendiente(tarea_id, mensaje)` (ver punto 1). **No es automática**, igual que el punto 10: el Líder/Admin escribe el mensaje a mano explicando qué está mal — la tarea sigue pendiente, sin cambios, para que el creador la corrija él mismo.
13. **Tu tarea fue eliminada** — destinatario: el **creador** de la tarea (aplica a cualquier `eliminar_tarea`, no solo a un rechazo desde Pendientes — ver [Eliminar una tarea](#eliminar-una-tarea)). Automática: usa la misma `justificacion` que ya se escribe para el doble candado como contenido del mensaje, no pide texto aparte.
14. **Un Administrador/Líder editó tu tarea directamente** — **sin pantalla en el mockup**: depende de `editar_tarea_ajena`, que el frontend no tiene (ver punto 4).
15. **Una tarea terminada se reabrió** (`tarea_reabierta`, nuevo 2026-11-07) — destinatarios: todos los Líderes y Administradores. Generada por `reabrir_tarea(tarea_id, motivo)`; el cuerpo incluye la reapertura (n/3) y el motivo (ver [Estados, progreso y reaperturas](#estados-progreso-y-reaperturas)).

Nota: el **merge fallido por conflicto** no genera un tipo de notificación nuevo — se maneja con una alerta síncrona en pantalla para el Líder/Admin en el momento, y si decide enviarla a corregir, reutiliza el caso 2 (`rechazar_revision`). Ver detalle completo en [Flujo de revisión y merge a `dev`](#flujo-de-revisión-y-merge-a-dev).

Comandos Tauri expuestos al frontend: `obtener_notificaciones()` (para el ícono de campana con contador), `marcar_leida(notificacion_id)`, `desmarcar_leida(notificacion_id)` y `marcar_todas_leidas()` (las tres son locales, ver [Sincronización](#sincronización-entre-máquinas-arquitectura-de-eventos)).

Pendiente definir: si además de la campana in-app, estas notificaciones también disparan una notificación nativa de escritorio del sistema operativo (Tauri lo soporta) para que lleguen aunque la app esté minimizada.

## Comandos de usuarios y ajustes

Ver diseño en [FRONTEND.md → Usuarios](./FRONTEND.md#usuarios) y [FRONTEND.md → Ajustes](./FRONTEND.md#ajustes). **Actualizado (2026-11-07)**: se quitan `listar_solicitudes_pendientes`, `aceptar_solicitud`, `rechazar_solicitud` y `crear_solicitud_manual` (solicitar ingreso pasa a la versión 2, issue #57).

- `listar_usuarios()` — **todos los roles**; a quien no es Líder/Administrador solo se le devuelven nombre y rol (sin correo).
- `crear_usuario(email, rol)` — Líder/Administrador. **Sin contraseña** (decidido 2026-11-07): la identidad es `cm whoami`, igual que siempre. Valida el rol según quién invoca (`rolesAsignablesA` en el frontend): el Líder solo Desarrollador/Tester/Colaborador externo. La fila nace **firmada por quien la crea** (Administrador o Líder, ver [Firma de los Líderes](#firma-de-los-líderes-nuevo-2026-11-07)).
- `cambiar_rol(email, rol_nuevo)` — **actualizado**: reemplaza a `ascender_a_lider`. Administrador: cualquier rol menos Administrador, sobre cualquiera menos otro Administrador. Líder: solo entre Desarrollador/Tester/Colaborador externo, y lo firma él mismo con su llave — la persona sigue entrando y trabajando sin interrupción.
- `eliminar_usuario(email)` — Administrador: cualquiera; Líder: Desarrolladores y Colaboradores externos.
- `confirmar_usuario(email)` — exclusivo Administrador; ahora solo hace falta para filas **sin firma** (escritas a mano o anteriores a este cambio), ver [Confirmación de usuarios nuevos](#confirmación-de-usuarios-nuevos-resuelto).
- `renombrar_proyecto(nombre)` — exclusivo Administrador; cambia el nombre en BranchBoard (clave `nombre_proyecto` en [`configuracion_proyecto`](#modelo-de-datos-tabla-configuracion_proyecto)), no el repositorio de Unity VC.
- `enviar_mensaje(para, texto)` — todos los roles; crea una notificación de tipo `mensaje` para `para` (máx. 500 caracteres). Es un aviso suelto desde el perfil, distinto del [Chat](#chat).
- `solicitar_accion_bd(motivo)` — Líder y Desarrollador (botón "Reportar problema con la base de datos" en Ajustes); notifica a todos los Administradores (notificación #9).
- `eliminar_proyecto()` — exclusivo Administrador, palabra = **el nombre exacto del proyecto**; ver [Verificación adicional sobre Unity Version Control](#verificación-adicional-sobre-unity-version-control-para-eliminar-el-proyecto).
- **Según el frontend**: crear usuario devuelve error si el correo ya existe (sin distinguir mayúsculas); el nombre y apellido salen del correo (`ana.lopez@…` → "Ana" "Lopez"). Eliminar usuario pide escribir **"eliminar"**. `renombrar_proyecto` rechaza un nombre igual al de otro proyecto del mismo equipo.

## Documentos del proyecto

**Nuevo (2026-11-07, según el frontend)** — ver [FRONTEND.md → Apartado del proyecto](./FRONTEND.md#apartado-del-proyecto-documentos). Archivos del proyecto (no versionados en Unity VC) que se guardan en la carpeta compartida del proyecto (ej. `proyecto/documentos/`) y viajan por Drive/OneDrive como el resto.

- `subir_documentos(archivos)` — todos los roles. Lo que sube un Líder o Administrador queda publicado; lo que sube cualquier otro rol queda **pendiente** (sale en [Pendientes](./PLANNING.md#pendientes) y con un punto en el menú). Mismas reglas de [validación de rutas](#validación-de-rutas-de-archivoadjuntos-resuelto-previene-path-traversal): el backend genera su propio nombre de archivo y guarda el original solo como dato.
- `aprobar_documento(id)` — Líder/Administrador (visto bueno a un documento pendiente).
- `eliminar_documento(id)` — Líder/Administrador.
- `abrir_documento(id)` — todos los roles; abre el archivo con la aplicación del sistema (local, no pasa por `ops/`).
- Tabla `documentos`: `id`, `nombre` (el original), `tamano` (bytes), `subido_por` (email), `fecha`, `pendiente` (booleano) y la ruta interna del archivo.

## Chat

**Nuevo (2026-11-07, según el frontend)** — ver [FRONTEND.md → Chat](./FRONTEND.md#chat). Sin servidor: cada mensaje viaja como un evento por `ops/` (llega cuando sincroniza Drive/OneDrive, con la app abierta).

- **Conversaciones**: el **grupo general** (id fijo `grupo`, se llama como el proyecto, están todos; siempre existe), los **grupos** que cree el equipo y los **privados** entre dos personas.
- `enviar_chat(para, texto)` — `para` es un email (privado), `grupo` (general) o el id de un grupo. Quien lo envía ya lo cuenta como leído.
- `crear_grupo_chat(nombre, miembros)` — todos los roles; quien lo crea queda dentro.
- `marcar_chat_leido(conversacion)` — local: marca como leídos los mensajes de esa conversación para quien lo invoca.
- `eliminar_chat(conversacion)` — local y solo para quien lo invoca: guarda desde cuándo la eliminó y deja de ver los mensajes anteriores; si llega uno nuevo, la conversación vuelve a aparecer solo con ese.
- Tablas: `mensajes_chat` (`id`, `de`, `para`, `texto`, `fecha`), `grupos_chat` (`id`, `nombre`, `miembros`, `creado_por`, `fecha`); locales por usuario: los leídos de cada mensaje y `chats_eliminados` (conversación → desde cuándo). En el menú, el número del Chat cuenta **conversaciones** con algo sin leer, no mensajes.

## Puntos de aviso del menú

**Nuevo (2026-11-07, según el frontend)** — ver [FRONTEND.md → Navegación general](./FRONTEND.md#navegación-general). Cada pantalla tiene una **huella** de sus datos (ej. Usuarios: correo y rol de cada uno; Scrum/Lista: id, estado, responsable, título y aprobación de cada tarea). Si la huella cambió desde la última vez que la persona entró, se enciende el punto; al entrar se guarda la huella vista (`marcar_visto(panel, huella)`). Es **local por usuario** (no pasa por `ops/`); el backend solo tiene que darle al frontend los datos para calcularla y guardar la huella vista.

## Imagen del proyecto

**Nuevo (2026-11-07, pendiente de implementar)**: `subir_imagen_proyecto(ruta_archivo)` — Administrador (por definir si también el Líder). Copia la imagen elegida a la carpeta compartida del proyecto (ej. `proyecto/icono.png`, redimensionada a un tamaño fijo, p. ej. 256×256) y guarda la ruta en [`configuracion_proyecto`](#modelo-de-datos-tabla-configuracion_proyecto) con la clave `icono_proyecto`. Migra a eventos para que todas las máquinas la vean. Validar tipo (png/jpg/webp) y tamaño máximo, con la misma [validación de rutas](#validación-de-rutas-de-archivoadjuntos-resuelto-previene-path-traversal) que los adjuntos. Ver [FRONTEND.md → Apartado del proyecto](./FRONTEND.md#apartado-del-proyecto-documentos).

## Actualizaciones de la app

**Nuevo (2026-11-09)**. Quien ya tiene BranchBoard instalado recibe las versiones nuevas **sin desinstalar nada**, desde [Ajustes → "Actualizar versión"](./FRONTEND.md#ajustes). Usa el plugin oficial de actualización de Tauri.

- **Cómo se publica una versión**: se sube el número de versión en `app/backend/tauri.conf.json` y `Cargo.toml`, y se sube una etiqueta `vX.Y.Z`. GitHub Actions (`.github/workflows/publicar-version.yml`) compila el instalador de Windows, lo **firma** con la llave privada de actualizaciones y crea la Release con el instalador, su firma (`.sig`) y `latest.json` (versión, notas, enlace y firma).
- **Cómo se actualiza la app**: consulta `https://github.com/MiguelMeza17/BranchBoard/releases/latest/download/latest.json`. Si la versión es mayor que la instalada, descarga el instalador, **verifica la firma** con la llave pública que va en `tauri.conf.json` y lo instala encima. Si la firma no coincide, no instala nada.
- **No se pierden datos**: el `.db`, los backups y las llaves viven en la carpeta de datos local, y `ops/` en Drive/OneDrive; nada de eso está en la carpeta de instalación.
- **Comandos** (locales, todos los roles): `buscar_actualizacion()` devuelve `{ version_actual, version_nueva, notas }` (`version_nueva` vacío = al día) y no descarga nada; `instalar_actualizacion()` descarga (evento `actualizacion-progreso` con `{ descargado, total }`), instala y reinicia la app. Se rechaza si hay una [operación en curso](#bloquear-el-cierre-de-la-app-durante-una-operación).
- **Nunca automático**: la app no busca ni instala sola; siempre lo pide la persona.
- **Llave de actualizaciones**: es **otra llave**, distinta de la de firma de roles. La privada está solo en la máquina de quien publica (`~/.tauri/branchboard-actualizaciones.key`) y como secreto `TAURI_SIGNING_PRIVATE_KEY` del repositorio; nunca se sube al repo. Si se pierde, las apps ya instaladas no pueden actualizarse solas: hay que reinstalar una vez con un instalador firmado con la llave nueva.
- **Un solo instalador para todos los proyectos**: ver el cambio en [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac).

## Copias de seguridad y restauración de la BD

**Backup automático**: cada vez que se abre BranchBoard, antes de tocar el `.db`, se copia a una carpeta local de backups (ej. `%LocalAppData%\BranchBoard\backups\branchboard_2026-09-25.db`). Se conservan solo las **últimas 10 copias** — al llegar a la 11, se borra la más vieja. No requiere acción de nadie, corre solo.

**Restaurar backup** — **exclusivo del Administrador** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)):
- Comando Tauri `listar_backups()` (lista las copias disponibles con fecha) y `restaurar_backup(nombre_archivo)`, ambos validados para que solo un Administrador los invoque.
- Dado lo delicado de la acción (descarta todo lo escrito después del backup elegido), usa el mismo patrón de doble candado que [eliminar una tarea](#eliminar-una-tarea): confirmación + escribir la palabra **"restaurar"** (definida en el frontend) antes de habilitar el botón. Cada copia muestra fecha, tamaño y, si es la que se guardó al restaurar otra, su nota.
- **Actualizado (2026-11-07) — "Actual" y guardar antes de restaurar**: la lista siempre muestra arriba la base en uso como **"Actual"** (fecha de hoy, no restaurable). `restaurar_backup` primero copia el `.db` actual como un backup más (con nota "Guardada antes de restaurar la copia del …") y luego aplica el elegido, así siempre se puede volver al estado anterior. Esa copia previa no cuenta para el límite de 10 hasta que se restaure otra.
- Un Líder o Desarrollador que detecte un problema no puede restaurar directamente — le llega a los Administradores vía la notificación **"Solicitud de acción sobre la base de datos"** (ver [Notificaciones](#notificaciones), punto 9).

## Modelo de datos: tabla `usuarios`

| Campo | Tipo | Notas |
|---|---|---|
| `email` | texto (PK) | viene de `cm whoami`, ver [Identidad de usuario](#identidad-de-usuario) — no hay contraseña que guardar |
| `nombre` | texto | al crear el usuario se toma del correo (`ana.lopez@…` → Ana), editable después |
| `apellido` | texto | ídem |
| `rol` | enum | Administrador / Líder / Desarrollador / Tester / Colaborador externo |
| `fecha_alta` | fecha | cuándo quedó registrado en el proyecto |
| `firma` | texto (base64) | **nuevo** — firma Ed25519 de `email\|rol\|firmado_en`, ver [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac) |
| `firmado_en` | fecha/hora | **nuevo** — timestamp incluido en el mensaje firmado, evita reutilizar una firma vieja para otro campo |

Borrar una fila de esta tabla = [eliminar a esa persona del equipo](#identidad-de-usuario) (revoca acceso de inmediato, sin sesión que cerrar).

## Firma criptográfica del rol (ancla de confianza para el RBAC)

**Resuelve**: el `.db` de SQLite se sincroniza por Drive/OneDrive con lectura/escritura para todo el equipo — cualquiera podía abrir el archivo y poner su propio `rol = 'Administrador'` a mano, y el backend no tenía forma de distinguir ese cambio de uno hecho legítimamente desde la app (hallazgo C1 de la auditoría de seguridad).

**Actualizado (2026-11-09) — un solo instalador, la llave pública ya no va dentro del binario**: con las [actualizaciones de la app](#actualizaciones-de-la-app) todos los proyectos usan **el mismo instalador**, así que la llave pública del Administrador no puede ir embebida en la build. Reemplaza los pasos 1 y 2 de abajo y la "consecuencia asumida" de una build por proyecto:
- El par de llaves se genera **dentro de la app**, cuando el dueño del repositorio configura el proyecto por primera vez (ver [Identidad de usuario](#identidad-de-usuario)). La privada sigue igual: solo en su máquina, fuera de Drive/OneDrive.
- La **llave pública** viaja en el primer evento del proyecto (`configurar_proyecto`, firmado con esa misma llave).
- Cada máquina **guarda esa llave pública la primera vez que entra al proyecto**, en su carpeta de datos local (fuera de `ops/` y de Drive/OneDrive), y desde ahí verifica todo contra ella. Si más adelante aparece en `ops/` otra llave para el mismo proyecto, se rechaza y se avisa: nadie puede cambiar el ancla de confianza escribiendo en la carpeta compartida.
- **Riesgo aceptado**: la confianza se fija en la primera entrada (si alguien alterara `ops/` antes de que una persona nueva entre por primera vez, esa persona fijaría una llave falsa). Mitigación: al configurar solo se acepta la llave si quien firma el evento es el **dueño del repositorio en Unity VC**, dato que no se puede cambiar desde `ops/`.
- Regenerar las llaves ya no exige recompilar: el Administrador genera un par nuevo y cada persona confirma el cambio de llave en su máquina.

**Mecanismo**:
- **Corregido — orden real de los pasos** (la versión anterior decía a la vez "se genera al configurar el proyecto" y "queda embebida en el binario que ya se compiló", lo cual es contradictorio: una llave generada en tiempo de ejecución no puede estar ya horneada en un binario compilado antes): la generación de llaves es un **paso de preparación del proyecto, no un botón dentro de la app ya instalada por el equipo**.
  1. Quien va a ser Administrador corre `generar_llaves_admin()` **una sola vez**, en un modo de configuración/build previo a distribuir la app (la misma persona técnica que arma el build para el equipo — coherente con que ya es quien inicializa el proyecto, ver [Bootstrapping](#identidad-de-usuario)).
  2. La **llave pública** resultante se escribe en un archivo de configuración del proyecto que el proceso de build de Tauri incluye como constante embebida (`include_str!`/`env!` en tiempo de compilación) en el binario que se empaqueta y distribuye a **todo el equipo de ese proyecto específico** — de ahí que "todos los clientes la traen igual": es la misma build, no que cada instalación genere o descubra la llave por su cuenta.
  3. La **llave privada** nunca entra a ese paso de build — se guarda **fuera de la carpeta sincronizada por Drive/OneDrive** (ej. `%LocalAppData%\BranchBoard\admin.key`, solo en la máquina del Administrador) y nunca viaja por la nube.
     - **Protección en reposo (resuelto)**: por defecto, `admin.key` queda protegida solo por los permisos del sistema operativo (en Windows, el archivo hereda los permisos del perfil de usuario — otros usuarios de esa máquina no pueden leerlo). Es la misma base de confianza que ya usan llaves SSH/GPG típicas en una máquina de desarrollo — comprometer el perfil del Administrador implica comprometer la llave de todas formas. Se ofrece además un **modo opcional recomendado**: cifrar `admin.key` en reposo con la misma passphrase que ya usa `exportar_llave_firma(passphrase)` — al activarlo, cada operación que firma (`crear_usuario`, `cambiar_rol`, `confirmar_usuario`) pide la passphrase la primera vez por sesión de la app y la mantiene solo en memoria (nunca en disco) mientras esa sesión sigue abierta. Queda como opción, no obligatorio, para no forzarle a un equipo chico una fricción diaria que quizás no quiere.
  - **Consecuencia asumida**: cada proyecto nuevo de BranchBoard tiene su propio par de llaves y, en sentido estricto, su propia build del binario (aunque el código fuente sea el mismo). Regenerar las llaves (ej. llave privada perdida sin respaldo) implica recompilar y redistribuir el instalador al equipo, no solo un cambio de datos.
- Cada vez que se asigna o cambia un rol — `crear_usuario(email, rol)`, `cambiar_rol(email, rol)`, `confirmar_usuario(email)` (ver abajo; `asignar_rol` ya no existe) — el backend, corriendo en la máquina del Administrador, firma el mensaje `email|rol|firmado_en` con la llave privada y guarda `rol`, `firma` y `firmado_en` en la fila correspondiente de `usuarios`.
- **Al leer la tabla `usuarios`** (arranque de la app, cualquier chequeo de permisos), el backend recalcula la firma esperada con la llave pública embebida y la compara contra `firma`. Hay tres estados posibles, no dos:
  - **Firma presente y coincide** → rol de confianza, se aplica tal cual.
  - **Firma presente pero NO coincide** (fila editada a mano fuera de la app, o campos `rol`/`firmado_en` alterados) → **no confiable**: el usuario se degrada al rol mínimo (Desarrollador) y la UI muestra un aviso de "rol no verificado — posible modificación manual del archivo".
  - **`firma` es `NULL`** (fila recién creada o con rol cambiado por un Líder vía `crear_usuario`/`cambiar_rol`, ver [Confirmación de usuarios nuevos](#confirmación-de-usuarios-nuevos-resuelto), no por una manipulación) → **bloqueado, no degradado**: la persona no puede entrar a la app todavía, ni siquiera como Desarrollador, hasta que el Administrador la confirme. Es un estado legítimo de "esperando firma", distinto del caso de arriba (que sí es sospechoso).
- **Actualizado (2026-11-07)**: firman el Administrador **y los Líderes** (ver abajo). Cada firma solo es válida dentro de lo que ese rol puede asignar.

### Firma de los Líderes (nuevo, 2026-11-07)

**Decisión del usuario**: quien es creado o cambiado de rol por un Líder entra y trabaja como siempre, sin esperar al Administrador. Para eso **los Líderes también firman**:
- Cada Líder tiene su **propio par de llaves**. La privada vive solo en su máquina, igual que la del Administrador (fuera de Drive/OneDrive).
- Al nombrar a alguien Líder (`cambiar_rol(email, 'Líder')`, exclusivo Administrador), el Administrador **firma la llave pública del Líder** (un "certificado": `email_lider|llave_publica|fecha`). Ese certificado se guarda junto al usuario y viaja por `ops/` como cualquier evento.
- Al verificar una fila de `usuarios`, el backend acepta la firma si es del Administrador (llave embebida en el build) **o** de un Líder con certificado válido del Administrador, **y** el rol firmado está dentro de lo que un Líder puede asignar (Desarrollador, Tester, Colaborador externo). Una firma de Líder sobre `rol = Líder` o `Administrador` se rechaza.
- **Quitarle el rol de Líder a alguien** (o eliminarlo) revoca su certificado: sus firmas futuras dejan de valer. Las filas que ya firmó siguen siendo válidas (fueron legítimas cuando las hizo); si se quiere, el Administrador puede volver a firmarlas.
- Esto sigue cerrando el hueco de C1: escribir a mano una fila con un rol cualquiera no sirve, porque no tendrá una firma válida del Administrador ni de un Líder certificado.
- **Pendiente para la auditoría de backend**: dónde guarda cada Líder su llave privada y cómo la respalda (mismo esquema que `exportar_llave_firma`), y qué pasa si un Líder la pierde.

### Confirmación de usuarios nuevos (resuelto)

**Problema**: `crear_usuario`/`cambiar_rol` (antes `aceptar_solicitud`) los puede invocar un **Líder**, pero solo el **Administrador** tiene la llave privada para firmar — un Líder no puede firmar el rol de la persona que acaba de dar de alta. La versión anterior de este documento listaba `crear_usuario` como uno de los comandos que firman sin notar esta incompatibilidad.

**Reemplazado (2026-11-07)** por la [Firma de los Líderes](#firma-de-los-líderes-nuevo-2026-11-07): lo que crea o cambia un Líder ya queda firmado y la persona entra de una vez. Lo de abajo solo aplica ahora a filas **sin firma** (escritas a mano o anteriores al cambio). Texto anterior:

**Resuelto** (decisión del usuario: mantener la capacidad de alta del Líder tal como está, sin restringirla al Administrador):
- `crear_usuario` (o `cambiar_rol` hecho por un Líder) escribe la fila en `usuarios` **de inmediato**, con el rol elegido pero `firma = NULL`, `firmado_en = NULL` — sin pedirle nada extra a quien crea el usuario.
- Esa persona **no puede entrar a BranchBoard todavía**: la [Pantalla inicial](./FRONTEND.md#autenticación--acceso-al-proyecto) reutiliza la misma pantalla de "tu cuenta no está registrada en este proyecto todavía", con el mensaje ajustado a "tu cuenta fue agregada por [quien la dio de alta], está pendiente de que el Administrador la confirme".
- Nuevo comando `confirmar_usuario(email)`, **exclusivo Administrador**: firma la fila (mismo mecanismo que `cambiar_rol`) — a partir de ahí la persona ya puede entrar con el rol que le asignaron.
- Aparece listado en [Usuarios](./FRONTEND.md#usuarios) con una etiqueta **"Pendiente de confirmar"**, visible para el Administrador con el botón `confirmar_usuario`, y visible (solo lectura) para el Líder que la dio de alta, para que sepa que todavía no tiene efecto real.
- **Trade-off asumido**: alguien dado de alta por un Líder no puede empezar a trabajar hasta que el Administrador confirme, aunque sea de forma asíncrona (no tiene que ser inmediato/en vivo). Para un equipo de 3 personas donde el Administrador está normalmente disponible, es un costo bajo a cambio de mantener el punto único de firma que es el eje de todo C1.

**Aclarado — dar de alta en `usuarios` NO le da acceso a la carpeta de Drive/OneDrive, y sin esa carpeta la persona no puede usar la app en absoluto** (simétrico a lo ya documentado para [eliminar a alguien del equipo](#identidad-de-usuario)): `crear_usuario`/`cambiar_rol`/`confirmar_usuario` solo tocan la tabla `usuarios` — todos comandos de BranchBoard, nada más. Pero como el `.db` local de esa persona se reconstruye leyendo `ops/` (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)), necesita **antes que nada** tener acceso real a esa carpeta compartida. Eso pasa 100% fuera de BranchBoard y de forma manual:
  1. La persona necesita su propia cuenta de Drive/OneDrive (la que ya use).
  2. Alguien del equipo con acceso a la carpeta del proyecto la comparte con el correo de Drive/OneDrive de esa persona — desde la interfaz normal de Drive/OneDrive, no desde BranchBoard.
  3. La persona instala el cliente de sincronización de escritorio de Drive/OneDrive e inicia sesión — recién ahí esa carpeta aparece en su máquina como una carpeta local normal, y BranchBoard puede apuntar a esa ruta.
  - Este paso normalmente pasa **antes** de que la persona intente abrir BranchBoard por primera vez (si no, la pantalla inicial no encuentra ningún `.db`/`ops/` con qué trabajar). BranchBoard no tiene forma de detectar ni recordar que falta este paso — es responsabilidad del equipo coordinarlo, igual que la revocación al salir.

**Respaldo de la llave privada**: como el Administrador es ahora punto único de firma, debe exportar la llave privada cifrada con contraseña a un lugar seguro fuera de la app (gestor de contraseñas, USB, etc.) — comando `exportar_llave_firma(passphrase)`. Sin esa llave (perdida y sin respaldo), nadie puede asignar/cambiar roles nuevos válidos hasta regenerar el par **y volver a distribuir un build nuevo con la llave pública actualizada** (ver mecanismo arriba) — no es solo re-firmar filas, es un nuevo binario para todo el equipo.

**Aclarado — qué pasa si el Administrador deja el equipo**: es el mismo caso que "llave perdida sin respaldo", solo que en vez de un accidente es planeado — antes de que esa persona se vaya, alguien del equipo (normalmente quien va a tomar el rol de Administrador después) debe recibir el export cifrado de `exportar_llave_firma` y la passphrase por un canal fuera de la app (no viaja por Drive/OneDrive, igual que la llave misma). Si eso no se hizo a tiempo, el equipo queda en el mismo escenario de "regenerar llaves y redistribuir build" — no hay ningún mecanismo de recuperación adicional dentro de BranchBoard, es 100% responsabilidad del equipo coordinarlo por fuera.

**Riesgo residual aceptado**: la firma evita *forjar* un rol nunca otorgado, pero no impide por sí sola un *rollback*: alguien podría restaurar una versión antigua del `.db` (vía el historial de versiones de Drive/OneDrive) que contenga una firma válida de un rol que ya le fue revocado. Dado el tamaño del equipo (3 personas conocidas) se acepta este riesgo por ahora en vez de añadir un contador monotónico adicional — si el equipo crece, revisar.

## Verificación adicional sobre Unity Version Control para eliminar el proyecto

**Contexto**: se evaluó anclar también el rol de Administrador al **Owner** real de la organización en Unity Cloud (dato que Unity controla, no editable desde el `.db`). Se investigó a fondo (documentación pública + inspección del propio Unity Dashboard) y **no es viable como fuente de autorización en tiempo real**:
- No existe una API pública/documentada (ni en `services.docs.unity.com` ni en el CLI `cm`) para preguntar "¿qué user type tiene este email en la organización?".
- El dato sí existe, pero lo sirve un endpoint interno del dashboard (`services.unity.com/api/plastic/v1/organizations/{id}/profiles`) atado a la sesión del navegador — no pensado para consumo de terceros, sin SLA, y no aplicable a un backend de escritorio sin esa sesión.

**Alternativa real y sí soportada**: en vez de leer el "Owner" abstracto, apoyarse en el sistema de permisos de **Unity Version Control** que la app ya usa vía `cm` — concretamente el permiso `rmrepository` ("controla si el repositorio puede eliminarse"), visible y configurable hoy mismo en el Unity Dashboard (`Version Control > Roles personalizados` / `Permisos de usuario`).

- Antes de ejecutar `eliminar_proyecto()`, el backend corre `cm showacl` sobre el repositorio del proyecto y confirma que el email de `cm whoami` (quien invoca la acción) tiene `rmrepository` concedido (no denegado) en Unity Version Control.
- Queda como **doble candado** sobre la acción más destructiva de la app: hace falta (a) que la firma local diga `rol = Administrador` **y** (b) que Unity Version Control conceda `rmrepository` sobre el repo real — alguien que solo comprometa el `.db` local no puede borrar el proyecto si Unity no se lo permite también.
- **Pendiente por probar al implementar**: confirmar que `cm showacl` puede ejecutarse con la sesión normal del usuario (sin privilegios de administrador de Unity) para consultar su propio permiso efectivo; si `cm showacl` requiere privilegios elevados, usar como respaldo el resultado de un intento de operación real de bajo impacto y capturar el error de "permission denied" de `cm` como señal equivalente.

## Modelo de datos: tabla `colaboradores_externos` (eliminada)

**Actualizado (2026-11-07)**: ya no existe. Los colaboradores externos son un **rol** más en `usuarios` (`rol = 'Colaborador externo'`), creado desde [Usuarios](./FRONTEND.md#usuarios) con `crear_usuario`. Se quitan `agregar_colaborador_externo` y `revocar_colaborador_externo`. Sigue igual que usan workspace parcial (Gluon), ver [Workspace parcial (Gluon) para subtareas simples](#workspace-parcial-gluon-para-subtareas-simples).

## Modelo de datos: tabla `solicitudes_ingreso` (versión 2, no se implementa)

**Actualizado (2026-11-07)**: solicitar ingreso queda para la versión 2 (issue #57, con las decisiones pendientes y cómo lo hacen otras plataformas). Se quitan la tabla `solicitudes_ingreso`, el flujo `mailto:` (ida y vuelta), `crear_solicitud_manual` y la clave `correo_contacto_solicitudes`. El alta la hace un Líder o Administrador con `crear_usuario`. La variante de invitar por correo con un código que vence en 3 días está en el issue #58.

## Modelo de datos: tabla `tareas`

Consolidando todo lo definido a lo largo de la conversación en un esquema concreto:

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | identificador único |
| `titulo` | texto | |
| `detalles` | texto | **reemplaza a `descripcion`** (2026-10-31): texto opcional, puede ser largo — qué hay que hacer, los pasos, dónde está cada cosa. Vacío si no hay (la interfaz muestra "Sin detalles."). Se escribe al crear la tarea (general o subtarea) y se puede editar |
| `tipo` | enum | **Actualizado (2026-11-07, según el frontend)**: Scripting / Modelado 3D / Modelado 2D / Animación / Imagen / UI/Interfaz / Audio / Música / Video / Testing / Corrección / Ajustes, más el valor interno **Sin definir** (solo la subtarea "En espera") |
| `tarea_padre_id` | nullable | si es subtarea (rama hija); `null` si es tarea general |
| `archivo_ruta` | nullable | solo tareas de archivo puntual (Colaborador externo): ruta del archivo dentro del workspace de Unity VC |
| `estado` | enum | por hacer / en producción / en revisión / terminada (**corregido**: ya no incluye "bloqueada", ver `pendiente_aprobacion`) |
| `pendiente_aprobacion` | booleano | `true` si la creó un Desarrollador o Tester y todavía no la aprueba un Líder/Admin (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)) — `false` de entrada si la crea Líder/Admin. Es independiente de `estado`: mientras es `true`, el único efecto es que `mandar_a_revision` la rechaza; no impide tomarla ni crearle rama. |
| `rama_asociada` | nullable | `null` hasta que alguien "toma" la tarea; formato `task/<id>-<nombre>` |
| `modo_trabajo` | `'rama'`, nullable | `'rama'` desde que se tomó (ya tiene su rama). Ya no existen tareas de check-in directo |
| `fecha_creacion` | fecha (AAAA-MM-DD) | **nuevo**: se pone sola al crear la tarea, con la fecha **local** del equipo (no UTC); no se edita. La usan las gráficas de backlog |
| `ultimo_changeset_visto` | nullable | para el polling de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) |
| `modificada` | booleano | badge de "hay cambios nuevos" (ver polling) |
| `creado_por` | email | quien creó la tarea |
| `asignado_a` | email, nullable | quien la tomó/le fue asignada |
| `lider_revision_asignado` | email, nullable | ver [asignar_revision en Flujo de revisión](#flujo-de-revisión-y-merge-a-dev) |
| ~~`autorizada_modificacion`~~ | — | **eliminado** (flujo invertido, ya no es un permiso que se otorga por adelantado) — el estado de "edición pendiente" ahora se deriva de si existe una fila para esta tarea en [`ediciones_pendientes`](#modelo-de-datos-tabla-ediciones_pendientes), ver [Notificaciones](#notificaciones), punto 4 |
| `sprint_id` | nullable | **sprint** (`con_fecha = true`) que la está trabajando ahora; uno a la vez (ver [Modelo de datos: tabla `sprints`](#modelo-de-datos-tabla-sprints)) |
| `backlog_id` | nullable | **nuevo (2026-11-07)**: **backlog** (`con_fecha = false`) al que pertenece, es decir, cómo se organiza. Independiente de `sprint_id`: una tarea puede estar suelta, solo en un backlog, solo en un sprint o en los dos (el sprint la "ocupa" sin sacarla de su backlog) |
| `con_fecha` / `fecha_limite` | booleano / nullable | tareas con y sin tiempo (ver [Sprints y tareas en FRONTEND.md](./FRONTEND.md#alcance)) |
| `fecha_completada` | nullable | usada para la [gráfica de Avance](#datos-para-la-gráfica-de-avance) |
| `eliminada` | booleano | **soft-delete** (ver [Eliminar una tarea](#eliminar-una-tarea)) — mientras es `true` la tarea vive en [Papelera](#papelera); `eliminar_definitivamente` sí borra la fila por completo después |
| `justificacion_eliminacion` / `fecha_eliminacion` | nullable | solo si `eliminada = true` |
| `motivo_rechazo` | nullable | último motivo de rechazo en revisión, si aplica |
| `reaperturas` | entero | **nuevo**: veces que volvió de "terminada" a "en revisión" (tope 3 salvo el Administrador, ver [Estados, progreso y reaperturas](#estados-progreso-y-reaperturas)) |
| `edicion_pendiente` | — | **no se guarda**: el frontend lo recibe calculado (existe una fila en `ediciones_pendientes` para la tarea) |
| `reporte_testing_origen_id` | nullable | si esta tarea nació de un reporte de Testing (ver abajo), referencia a ese reporte |
| `tarea_origen_id` | nullable | tarea ya terminada (mergeada a `dev`) que esta corrige o a la que da seguimiento. **Solo referencia, no jerarquía**: `tarea_padre_id` queda en `null`, la rama sale de `dev` y la tarea lleva su propio número (`T-211`, no `T-101-3`), porque la rama de la original ya está cerrada |

## Modelo de datos: tabla `sprints`

**Resuelto** (confirmado por el usuario: se implementan ambas variantes — con tiempo y sin tiempo — mismo patrón que `con_fecha`/`fecha_limite` en `tareas`, ver [Sprints y tareas en FRONTEND.md](./FRONTEND.md#alcance)):

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | |
| `nombre` | texto | |
| `descripcion` | texto | **nuevo** (según el frontend), puede ir vacía |
| `con_fecha` | booleano | `true` = **sprint**, `false` = **backlog** (un grupo de tareas con nombre, sin fecha de fin, que puede crecer) |
| `fecha_inicio` | fecha | se fija al crear el sprint, siempre tiene valor (con o sin `con_fecha`) |
| `duracion_dias` | entero, nullable | solo si `con_fecha = true` — cuántos días dura, lo pregunta el modal de creación |
| `fecha_fin` | fecha, nullable | calculada = `fecha_inicio + duracion_dias` al crear el sprint (no se recalcula después); solo si `con_fecha = true`. Es lo que dispara el [Reporte automático de cierre](./PLANNING.md#reportes) al llegar esa fecha |
| `cerrado` | booleano | `true` cuando el sprint ya terminó. Sprint con fecha: automático al llegar `fecha_fin` **solo si todas sus tareas están terminadas** (si no, sigue abierto, ver [Cierre de sprints](#cierre-de-sprints)); o manual por el Administrador, también antes de tiempo. Backlog (sin fecha): manual |
| `fecha_cierre_real` | fecha, nullable | cuándo se cerró de verdad: `fecha_fin` si se cerró solo, o la fecha del cierre manual (antes de tiempo, después de `fecha_fin` si se había pasado, o la de un backlog) |
| `creado_por` | email | |

**Tabla `reportes_sprint`** (nuevo, 2026-11-07, según el frontend): la **copia de las tareas de un sprint** tal como estaban al cerrarse o al llegar su `fecha_fin` (`sprint_id`, `fecha`, y por cada tarea: id, título, tipo, estado, responsable, `fecha_creacion` y `fecha_completada`). La pantalla [Reportes](./FRONTEND.md#alcance) y la gráfica de un sprint cerrado se arman con esta copia, no con las tareas vivas (que cambian después). Reportes solo lista sprints cerrados o con `fecha_fin` pasada.

**Comandos Tauri**:
- `crear_sprint(nombre, descripcion, con_fecha, duracion_dias?)` — exclusivo Líder/Administrador, mismo criterio que crear/organizar el resto de la estructura del proyecto. Si `con_fecha = true`, `duracion_dias` es obligatorio y el backend calcula `fecha_fin` de una vez.
- `cerrar_sprint(sprint_id)` — **actualizado (2026-11-02): exclusivo del Administrador** (antes Líder/Administrador; crear sprints y asignarles tareas sigue siendo de Líder/Administrador). Puede cerrar un sprint con fecha antes de su fin y también un backlog: ver [Cierre de sprints](#cierre-de-sprints). Texto anterior (reemplazado): exclusivo Líder/Administrador, **solo aplica a sprints sin fecha** (`con_fecha = false`): como no tienen un punto de cierre automático, alguien tiene que cerrarlo a mano para que dispare el mismo Reporte que un sprint con fecha genera solo. Un sprint con fecha no se puede cerrar manualmente antes de tiempo con este comando (**pendiente por definir**: si hace falta un `cerrar_sprint_anticipado` aparte para ese caso, o si simplemente no se soporta en el MVP).

- `listar_sprints()` — cualquier rol, para poblar la vista de sprints y el selector al crear/editar una tarea.
- `asignar_tarea_a_sprint(tarea_id, sprint_id?)` y `asignar_tarea_a_backlog(tarea_id, backlog_id?)` — Líder/Administrador, desde "Asignar tareas" en Planificación (`null` = quitarla). Se rechaza si la tarea ya está en otro sprint (o en otro backlog).
- **Según el frontend**: un sprint cuya `fecha_inicio` todavía no llega se muestra "Por empezar"; uno abierto, "En curso"; uno cerrado, "Cerrado". Un sprint cerrado cuenta (y grafica) las tareas de su reporte.

Asignar una tarea a un sprint es escribir su `sprint_id`, y a un backlog su `backlog_id` — se hace como parte de `crear_tarea`/`editar_tarea` (o desde "Asignar tareas" en Planificación). **Actualizado (2026-11-07)**:
- `asignar_backlog_a_sprint(backlog_id, sprint_id)` — Líder/Administrador: el sprint toma de una vez todas las tareas del backlog que están **libres** (`sprint_id` nulo) y no eliminadas, terminadas o no. Con `backlog_id = null` toma las tareas **sin backlog**. Las que ya tiene otro sprint no se tocan.
- Una tarea solo puede estar en **un sprint y un backlog a la vez**: `editar_tarea` rechaza cambiar `sprint_id` (o `backlog_id`) de una tarea que ya tiene otro sprint (u otro backlog); primero hay que quitarla de ese.
- Al cerrar un sprint (`cerrar_sprint` o cierre automático), **primero se guarda la copia de sus tareas tal como están** (id, título, tipo, estado, responsable, `fecha_completada`) como el reporte del sprint — el reporte se arma siempre con esa copia, no con las tareas vivas. El reporte que se genera al llegar `fecha_fin` sin cerrar el sprint guarda también su copia de ese día. Después, sus tareas **sin terminar** quedan con `sprint_id = null`: vuelven a estar libres en su backlog (si tienen) para el siguiente sprint. Las terminadas conservan su `sprint_id` (para el reporte).
- Un backlog se puede trabajar sin sprint: sus tareas se toman y terminan igual que cualquier otra.

### Cierre de sprints

**Definido (2026-11-02):**

- **Solo el Administrador** cierra sprints y backlogs a mano (`cerrar_sprint`), y puede hacerlo **antes de la fecha de fin**. Al cerrarlo se genera su reporte.
- **Al llegar `fecha_fin`** (lo revisa el proceso de fondo / al abrir la app):
  - **si todas sus tareas están terminadas**: el sprint **se cierra solo** (`cerrado = true`, `fecha_cierre_real = fecha_fin`) y genera su reporte;
  - **si no**: se genera **un reporte en esa fecha** (lo hecho y lo que faltó) pero el sprint **sigue abierto** y se continúa trabajando en él. Desde ahí se cierra solo en cuanto se terminen todas sus tareas, o cuando el Administrador lo cierre a mano (con un reporte final).
- Un backlog (sin fecha) nunca se cierra solo.

## Modelo de datos: tabla `ediciones_pendientes`

Una fila = una edición de título y/o detalles (hecha por cualquier usuario sobre una tarea en "Sin asignar" o "Por hacer") esperando que un Líder/Admin la confirme o la revierta (ver [Notificaciones](#notificaciones), punto 4).

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | |
| `tarea_id` | — | la tarea editada |
| `campos_nuevos` | JSON | los valores que ya están aplicados en `tareas` (la edición se ve de inmediato) |
| `campos_anteriores` | JSON | los valores previos a la edición — se usan para revertir si `rechazar_modificacion` |
| `motivo` | texto, nullable | por qué se quiere el cambio, escrito por quien edita (opcional) |
| `creado_por` | email | quien editó |
| `fecha` | fecha | |

Borrar esta fila (al aprobar o rechazar) = la tarea deja de mostrar la sombra amarilla y sale de [Pendientes](./PLANNING.md#pendientes). Solo puede existir **una** fila activa por tarea a la vez. **Resuelto (según el frontend)**: una segunda edición se **suma** a la fila existente (los campos nuevos se combinan y se conservan los valores anteriores originales).

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

**Sin pantalla en el mockup (2026-11-07)**: el frontend tiene el tipo de tarea "Testing" y el campo `reporte_testing_origen_id`, pero no la pantalla para crear un reporte. Comando Tauri `crear_reporte_testing(tarea_revisada_id, descripcion, adjunto?)`, exclusivo Tester: inserta el reporte y, en la misma operación, crea la fila en `tareas` de corrección (mismo `tipo` que la tarea revisada, `tarea_padre_id = null` porque es una tarea general nueva, no una subtarea). **Resuelto**: nace con `estado = 'por hacer'` y `pendiente_aprobacion = false` — no pasa por aprobación, ver [Testing en PLANNING.md](./PLANNING.md#testing).

## Modelo de datos: tablas `commits` y `cambios_prestados`

**Nuevo (2026-11-07, según el frontend)**. Los check-ins viven en Unity VC; el backend guarda una **copia de lo que lee con `cm log`** para mostrarlos rápido (se puede reconstruir siempre):

| Campo | Notas |
|---|---|
| `id`, `tarea_id`, `hash` | el hash es el changeset de Unity VC |
| `mensaje` | título del check-in (primera línea del comentario) |
| `descripcion` | resto del comentario (opcional) |
| `tipo` | tipo del check-in (Scripting, Modelado 3D…), elegido al hacer check-in desde BranchBoard (opcional) |
| `autor`, `fecha` | |

`cambios_prestados` (los préstamos de [Traer cambios](#cambios-prestados-traer-cambios)): `id`, `tarea_id` (destino), `origen_id`, `hash` (`null` = todos los cambios), `autor`, `fecha`, `archivos` (los que llegaron), `archivos_tocados` (los prestados que se modificaron después), `quitado_en` y `quitado_por` (cuándo y quién los devolvió; `null` = siguen en la rama).

Locales por máquina (no pasan por `ops/`): los **cambios sin check-in** de cada rama (de `cm status`) y los **cambios entrantes ya unidos** al workspace.

## Modelo de datos: tabla `configuracion_proyecto`

Tabla clave/valor para ajustes del proyecto entero. **Actualizado (2026-11-07)**: `correo_contacto_solicitudes` ya no se usa (solicitar ingreso pasa a la versión 2); la primera clave real es `nombre_proyecto` (ver `renombrar_proyecto`). Originalmente no había ninguna tabla de configuración a nivel proyecto en el modelo de datos. Clave/valor simple, pensada para que cualquier ajuste futuro de este tipo (no por tarea, no por usuario, sino del proyecto entero) tenga dónde ir sin agregar una tabla nueva cada vez:

| Campo | Tipo | Notas |
|---|---|---|
| `clave` | texto (PK) | ej. `nombre_proyecto` |
| `valor` | texto | |
| `actualizado_por` | email | |
| `fecha_actualizacion` | fecha | |

Comando Tauri `actualizar_configuracion_proyecto(clave, valor)` — exclusivo Administrador. **Migra a eventos** (agregarlo a la lista en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)) — de lo contrario cambiar un ajuste del proyecto en una máquina no se reflejaría en las demás.

## Pendiente por definir
- Cómo se valida el permiso de rol en cada comando Tauri (mecanismo exacto de autorización, ej. middleware/decorador vs. chequeo manual al inicio de cada comando)
- Estados del flujo de una tarea (ya definidos en la práctica: por hacer / en producción / en revisión / terminada, más la bandera aparte `pendiente_aprobacion` — falta solo confirmarlos como lista cerrada)
- Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)
- Comando `cm` exacto para leer si una rama está cerrada/mergeada
- ~~Intervalo exacto de escaneo de la carpeta `ops/`~~, ~~estrategia de compactación/purga de `ops/`~~ y ~~qué comandos migran al mecanismo de eventos~~ — **corregido**: los tres quedaron resueltos más arriba en este mismo documento (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)); esta sección no se había actualizado después de resolverlos.
- **Diferencias con el frontend que quedan por decidir** (2026-11-07): si se agrega o se descarta "Mover cambios" y `editar_tarea_ajena`; si en Revisión solo el revisor asignado puede autorizar; si se notifica el rechazo de una edición; el borrado en cascada de subtareas (diseñado aquí, el mockup no lo hace); cómo funcionan los proyectos sin Unity VC (contraseña, tareas sin ramas).
- Librería/mecanismo exacto de conversión `.fbx` → glTF para el visor de [Pre-revisión de Modelado 3D](./PLANNING.md#pre-revisión) — la restricción de subir solo `.fbx` ya está decidida, falta elegir qué convierte ese archivo al formato que consume el visor (`model-viewer`/Three.js).

## Check-ins: aprobación, rechazo y corrección

**Descartado (2026-11-02).** Ya no existen tareas de check-in directo (subtareas sin rama propia que hacían check-in sobre la rama del padre): **toda tarea se trabaja en su propia rama** y los check-ins son guardados dentro de ella. Lo que se revisa, aprueba o rechaza es la **rama** completa, con el flujo normal de [revisión y merge](#flujo-de-revisión-y-merge-a-dev) o de [subtareas hacia su padre](#flujo-de-merge-de-subtareas-pequeñas-hacia-su-tarea-padre). También se eliminan la regla de "un solo check-in en curso por tarea padre" y la revisión de check-ins en conjunto.

## Check-ins de la rama, subtarea "En espera" y cambios entrantes

**Redefinido (2026-11-02).** Toda tarea tiene **su propia rama** (se crea al tomarla). Hacer check-in a cada rato es lo normal en Unity VC (respaldo en el servidor, puntos de retorno, menos conflictos), y **un check-in nunca se convierte en tarea**: es un guardado dentro de la rama.

| Rama | Cambios sin check-in | Qué se hace |
|---|---|---|
| **Sin subtareas** | Se ven en sus detalles, en **"Pending changes"** | **"Hacer check-in"** (título, descripción y tipo). Queda en **"Check-ins de la rama"** |
| **Con subtareas** (rama padre) | Aparece su subtarea **"En espera"** con esos cambios | En la rama padre **no se hace check-in**: se **crea una rama** (nombre, detalles y tipo) y la "En espera" pasa a ser esa subtarea, con los cambios |

### Ramas sin subtareas: check-ins en los detalles

- Cada check-in queda en **"Check-ins de la rama"** (título, descripción, tipo, changeset, autor y fecha; a futuro también los archivos). La tarea sigue siendo una sola y avanza normal: producción → revisión → terminada.
- Los detecta la [escucha de Unity VC](#detección-de-cambios-en-unity-version-control-polling) (`cm log` de la rama): se hagan desde Unity o desde BranchBoard, aparecen solos.
- Si a esa tarea se le crea una subtarea, pasa a ser una rama con subtareas.

### Ramas con subtareas: "En espera" y "Crear rama"

La rama de una tarea con subtareas solo debería recibir los merges de sus subtareas. Si alguien hace cambios directamente en ella:

1. La [escucha de Unity VC](#detección-de-cambios-en-unity-version-control-polling) ve **cambios sin check-in** en esa rama (`cm status` del workspace) y aparece la subtarea comodín **"En espera"**, de tipo **"Sin definir"** (valor especial: no es una categoría ni se puede elegir al crear tareas). Su tarjeta lleva el indicador **"N sin check-in"** y sus detalles, en "Pending changes", muestran quién trabaja y cuántos cambios hay.
2. Esos cambios **no se guardan con check-in en la rama padre**. Desde "Pending changes" (en la "En espera" o en la tarea padre) se usa **"Crear rama"**, que pide **nombre, detalles y tipo**:
   - la "En espera" **se convierte en una subtarea normal** con ese título, detalles y tipo, en "en producción", asignada a quien trabajaba;
   - se crea **su rama** (`task/<id>-<nombre>`) y los cambios sin check-in **se mueven a ella** (en Unity VC: crear la rama hija y cambiar el workspace a ella llevando los cambios pendientes; comando exacto pendiente de verificar);
   - desde ahí se trabaja como cualquier rama: check-ins, revisión y merge hacia el padre (que llegará como incoming change).
3. "En espera" **no puede traer cambios** ([Cambios prestados](#cambios-prestados-traer-cambios)): sus cambios todavía están sin guardar.
4. Mientras haya "En espera", la tarea padre no puede pasar a revisión (cuenta como subtarea pendiente).

### Click en cada elemento: detalle y acciones

Cada fila de las tres secciones abre su detalle al hacer click, con **"Abrir en Unity"** y su acción propia:

| Sección | Detalle | Acción propia |
|---|---|---|
| **Pending changes** | Rama, quién trabaja, cuántos cambios y los archivos (`cm status`), y qué pasará según la rama | Rama sin subtareas: **"Hacer check-in"** (`cm checkin`), con **título, descripción y tipo** (título y descripción van en el comentario del changeset). Rama con subtareas: **"Crear rama"** (nombre, detalles y tipo) |
| **Cambios entrantes** (incoming changes) | Subtarea, rama de origen, fecha, quién la trabajó y los check-ins que trae | **"Unir cambios"** (`unir_cambios_entrantes`, `cm update` con esos changesets). Opcional |
| **Check-ins de la rama** | Mensaje, autor, fecha, changeset | Solo "Abrir en Unity", en ese changeset |

### Cambios entrantes (incoming changes)

- Cuando una **subtarea pasa a "terminada"**, su rama se mergea a la rama del padre en el servidor. Quien tiene el workspace en la rama padre todavía no tiene esos cambios: en Unity VC son **incoming changes**.
- **Solo cuenta lo terminado**: una subtarea en revisión todavía no se ha mergeado, así que no sale. Si alguien necesita su trabajo antes, lo trae como [préstamo](#cambios-prestados-traer-cambios).
- BranchBoard lo muestra en los detalles del padre, en la sección **"Cambios entrantes"** (aparece en **toda rama con subtareas**, en 0 si no hay ninguno; las ramas sin subtareas no la tienen, y una tarea terminada nunca la muestra porque ya se mergeó y su rama está cerrada): qué subtarea, cuándo terminó, cuántos check-ins trae y desde qué rama.
- **Es solo un aviso y recibirlos no es obligatorio**: no llegan solos al workspace (ni al abrir la rama en Unity). Se reciben desde **"Traer cambios"**, donde salen **primero y en azul**, en su propio apartado "Incoming changes — llegaron a esta rama", con botón **"Unir cambios"** (misma confirmación escribiendo "traer"), o desde el detalle del incoming change. En Unity VC es actualizar el workspace con esos changesets (`cm update`). **No es un préstamo**: ya son de esta rama, no se registran en `cambios_prestados` ni hay que devolverlos.
- Se calcula **por máquina**: depende del workspace de cada uno (changeset en el que está el workspace contra el último de la rama). Es un indicador local, no pasa por `ops/`.

**Pendiente de definir al implementar**: comando `cm` exacto para crear la rama hija llevando los cambios pendientes; cómo se atribuye la "En espera" si quien hizo los cambios no es el responsable de la tarea padre; comando `cm` exacto para saber qué changesets de la rama le faltan al workspace (incoming); y excluir de la cuenta los archivos que Unity modifica solo (ej. `.meta`).

## Contrato con el frontend: acciones y comandos

**Nuevo (2026-11-07)**. Cada acción del store del frontend (`app/frontend/src/store/useStore.ts`) y el comando Tauri que la respalda. "Local" = no pasa por `ops/`.

| Acción del frontend | Comando Tauri | Quién |
|---|---|---|
| `crearTarea` / `crearTareaConSubtareas` | `crear_tarea` / `crear_tarea_con_subtareas` | todos (Desarrollador, Tester y Colaborador externo: queda pendiente de aprobación) |
| `editarTarea` | `editar_tarea` | creador sobre su tarea pendiente; Líder/Admin |
| `proponerEdicion` | `proponer_edicion` | todos, solo en "Sin asignar"/"Por hacer" |
| `editarYAprobarTarea` | `editar_y_aprobar_tarea` | Líder/Admin |
| `tomarTarea` / `asignarTarea` / `desasignarTarea` | `tomar_tarea` / `asignar_tarea` / `desasignar_tarea` | todos / Líder/Admin / todos (sin progreso) |
| `moverTarea` / `puedeAvanzar` | `cambiar_estado` (valida subtareas y préstamos) | según [Estados](#estados-progreso-y-reaperturas) |
| `borrarProgreso` | `borrar_progreso` | todos |
| `eliminarTarea` / `eliminarDefinitivo` | `eliminar_tarea` / `eliminar_definitivamente` | Líder/Admin |
| `aprobarTarea` / `aprobarTareas` / `advertirTarea` | `aprobar_tarea` / `aprobar_tareas` / `advertir_tarea_pendiente` | Líder/Admin |
| `aprobarEdicion` / `aprobarEdiciones` / `rechazarEdicion` | `aprobar_modificacion` / `aprobar_modificaciones` / `rechazar_modificacion` | Líder/Admin |
| `autorizarMerge` / `rechazarRevision` | `autorizar_merge` / `rechazar_revision` | Líder/Admin; en subtareas, quien tiene la tarea padre |
| `reabrirTarea` | `reabrir_tarea` | hasta 3 veces; después solo el Administrador |
| `asignarRevision` | `asignar_revision` | Líder/Admin sin revisor; solo Admin si ya tiene |
| `hacerCheckin` / `crearRamaDesdeEspera` | `hacer_checkin` / `crear_rama_desde_espera` | responsable de la rama |
| `traerCambios` / `quitarCambiosTraidos` / `eliminarCambiosAjenos` | `traer_cambios` / `devolver_cambios_prestados` / `eliminar_cambios_ajenos` | ver [Cambios prestados](#cambios-prestados-traer-cambios) |
| `recibirEntrante` | `unir_cambios_entrantes` (local) | quien tiene el workspace |
| `abrirEnUnity` | `abrir_en_unity` (local) | roles con workspace |
| `crearSprint` / `cerrarSprint` | `crear_sprint` / `cerrar_sprint` | Líder/Admin / solo Administrador |
| `asignarTareaASprint` / `asignarTareaABacklog` / `asignarBacklogASprint` | `asignar_tarea_a_sprint` / `asignar_tarea_a_backlog` / `asignar_backlog_a_sprint` | Líder/Admin |
| `crearUsuario` / `cambiarRol` / `eliminarUsuario` / `confirmarUsuario` | `crear_usuario` / `cambiar_rol` / `eliminar_usuario` / `confirmar_usuario` | ver [Comandos de usuarios](#comandos-de-usuarios-y-ajustes) |
| `renombrarProyecto` / `restaurarBackup` | `renombrar_proyecto` / `restaurar_backup` (local) | Administrador |
| `enviarMensaje` | `enviar_mensaje` | todos |
| `subirDocumentos` / `aprobarDocumento` / `eliminarDocumento` / `abrirDocumento` | `subir_documentos` / `aprobar_documento` / `eliminar_documento` / `abrir_documento` (local) | todos / Líder/Admin / Líder/Admin / todos |
| `enviarChat` / `crearGrupoChat` / `marcarChatLeido` / `eliminarChat` | `enviar_chat` / `crear_grupo_chat` / `marcar_chat_leido` (local) / `eliminar_chat` (local) | todos |
| `marcarLeida` / `desmarcarLeida` / `marcarTodasLeidas` | igual (local) | todos |
| `marcarVisto` | `marcar_visto` (local) | todos |
| `cerrarSesion` / `setSesionUnityVC` | `cerrar_sesion_unity_vc` / `iniciar_sesion_unity_vc` | todos |
| `entrarAProyecto` / `irAClonar` / `cambiarDeProyecto` | `entrar_a_proyecto` / `clonar_proyecto` / (vuelve al acceso) | todos |
| `crearProyectoNuevo` | `crear_proyecto_local` (sin Unity VC, pendiente de definir) | quien lo crea queda Administrador |
| `toggleVpn` | `alternar_warp` | todos |
| Ajustes → "Actualizar versión" (`lib/actualizaciones.ts`) | `buscar_actualizacion` / `instalar_actualizacion` (locales) | todos |

Las acciones `setRolActual` ("Ver como"), `setEmail` y `toggleCmSesion` son **solo del mockup** (demo) y no tienen comando: en la app real el rol viene de la tabla `usuarios` firmada y la identidad de `cm whoami`.
