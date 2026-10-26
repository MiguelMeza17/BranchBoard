# BranchBoard — Backend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que corre del lado de Tauri (Rust) / lógica de datos.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Cada tarea puede ser de tipo (9 valores, uno por cada visor de [Pre-revisión en PLANNING.md](./PLANNING.md#pre-revisión)):
  - **Scripting** → asociada a una rama (una tarea = una rama)
  - **Modelado 3D**
  - **Modelado 2D**
  - **Animación**
  - **Imagen** (arte 2D)
  - **UI/Interfaz**
  - **Audio**
  - **Música**
  - **Video**
- **Control de versiones: Unity Version Control** (antes Plastic SCM), no Git genérico. Se usa su CLI (`cm`). La integración desde el backend se hace en **Rust directo** (sin Python), invocando `cm` como subproceso.
- **Flujo de vida de una tarea**: se crea visual/sin rama → al "tomarla" alguien, el backend (Rust) crea la rama en Unity VC invocando `cm` → el sprint es el tiempo que esa rama vive antes de mergear a `dev` (ver [Flujo de revisión y merge a dev](#flujo-de-revisión-y-merge-a-dev)). El merge de `dev` hacia `main` (release) queda **fuera del alcance de BranchBoard** — es una acción manual que hace alguien técnico directamente en Unity VC, no algo que la app gestione.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, SQLite, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri) que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP**: ver lista completa y autoritativa en [Pantallas / tableros del MVP en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp) — no se duplica aquí para evitar que este documento quede desactualizado cuando se agreguen pantallas nuevas.
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (backend) y otra en [FRONTEND.md](./FRONTEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite local por máquina** (sin servidor). Cada máquina tiene su propio `.db`, que **no se sincroniza directamente** — lo que se comparte por la nube (ej. Drive/OneDrive) es un log de eventos (carpeta `ops/`), y cada máquina reconstruye/actualiza su `.db` reproduciendo esos eventos (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)). Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de verdad, y este mecanismo de eventos deja de ser necesario.
- Integración con Unity Version Control desde Rust invocando el CLI `cm` como subproceso: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios)
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`) — `tomar_tarea` recibe además la descripción escrita en el modal de confirmación del frontend (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), que se usa como comentario del changeset inicial de la rama
- **Condición de carrera al tomar/asignar una tarea (resuelto, ver arquitectura de eventos)**: un `UPDATE ... WHERE estado = 'por hacer'` solo evita colisiones dentro de una única BD compartida en vivo — no sirve aquí porque cada máquina tiene su propio `.db` con retraso de sincronización. La resolución real vive en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos): `tomar_tarea`/`asignar_tarea` generan un evento, y el desempate entre dos tomas simultáneas ocurre de forma determinista al reproducir los eventos en cada máquina, no con un `UPDATE` condicionado local.
- **Toda rama de tarea general nace desde la cabeza actual de `dev`, nunca desde un punto viejo**: al crear la rama (`cm branch create` o equivalente) no se fija un changeset específico guardado de antes — se apunta siempre a la última versión de `dev` en el momento exacto de "tomar" la tarea. Esto importa especialmente para tareas que se crean y se toman en momentos distintos (ej. una tarea de corrección de Testing que nadie toma hasta días después, y mientras tanto `dev` siguió recibiendo otros merges) — la rama nueva debe partir de lo último, no de cómo estaba `dev` cuando se creó la tarea.
- **Auto-switch al tomar una tarea (importante, medida de seguridad)**: justo después de `cm branch create`, `tomar_tarea` ejecuta **`cm switch <rama-nueva>`** sobre el workspace local de quien la tomó — su Unity Editor queda apuntando automáticamente a esa rama. Sin esto, la persona podría seguir escribiendo código/cambios sobre la rama en la que estaba antes (una tarea anterior, o `dev`) sin darse cuenta, mezclando trabajo de dos tareas distintas en la misma rama. Ver también [Guardia de rama incorrecta al sincronizar o crear subtarea](#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea) y [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) para los casos en que igual pase.
- **`asignar_tarea` NO hace auto-switch (corregido)**: el diseño original hacía que `asignar_tarea` ejecutara `cm switch` igual que `tomar_tarea`, pero eso corre en el proceso de Tauri de **quien invoca el comando** (el Líder/Administrador que asigna) — un `cm switch` ahí cambiaría la rama del workspace de quien asigna, no la del Desarrollador destino, que ni siquiera tiene la app abierta en ese momento. `asignar_tarea` solo crea la rama (`cm branch create`) y genera la notificación #3; el cambio de rama en la máquina del Desarrollador asignado ocurre después, como acción explícita de **su propio lado**: al abrir la tarea asignada (desde la notificación o el tablero), ve un botón **"Cambiar mi workspace a esta rama"** que ejecuta `cm switch <rama-nueva>` localmente, invocado por y en la máquina de esa persona — nunca automático/silencioso, para no interrumpirle sin avisar un cambio de rama mientras podría estar trabajando en otra cosa.

## Sincronización entre máquinas (arquitectura de eventos)

Reemplaza el diseño anterior de "lock de escritura sobre el `.db` compartido": sincronizar directamente un archivo SQLite entre varias máquinas por Drive/OneDrive no funciona, porque el sync es **por archivo completo, con retraso, y sin fusión de filas** — dos máquinas escribiendo casi al mismo tiempo pueden terminar con la copia de una pisando por completo la de la otra, sin importar si tocaron la misma tarea o no.

**Diseño**: lo que se sincroniza deja de ser el `.db`. Cada máquina tiene su propio `.db` **local, no sincronizado, desechable y reconstruible**. Lo que sí viaja por la carpeta de nube es una carpeta `ops/` de **eventos inmutables**, uno por archivo:

- Cada comando Tauri que muta estado compartido (`tomar_tarea`, `asignar_tarea`, `cambiar_estado`, `aprobar_tarea`, `rechazar_tarea`, `mandar_a_revision`, crear/editar/eliminar tarea, etc.) — en vez de hacer un `UPDATE`/`INSERT` directo que luego se sincroniza como parte del `.db` — escribe un archivo nuevo en `ops/`, nombrado `ops/<timestamp_iso>_<usuario>_<uuid>.json`, con `{ tipo, tarea_id, usuario, timestamp, payload }`. Nunca se edita ni se borra un archivo de `ops/` ya creado — solo se crean nuevos. Esto es lo que evita el problema de sincronización de archivo completo: como nadie más edita ese archivo, Drive/OneDrive nunca tiene que decidir "qué versión gana".
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
- **Eventos que solo un Administrador puede emitir legítimamente** (`asignar_rol`, `ascender_a_lider`, `cambiar_rol`, `confirmar_usuario`, `compactar_eventos`, ver [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)): **se extiende la misma firma Ed25519** que ya protege el campo `rol` de `usuarios` a estos eventos completos — el evento incluye un campo `firma_admin` (firma del JSON del evento con la llave privada del Administrador), y cada máquina la verifica contra la llave pública embebida antes de aplicarlo; si no coincide o falta, el evento se descarta sin aplicar (no se degrada, se ignora — a diferencia de una fila de `usuarios` inconsistente, aquí no hay "rol mínimo" al que degradar un evento). Esto cierra el caso más grave: nadie puede forjar una elevación de rol ni una acción administrativa aunque tenga acceso de escritura a `ops/`.
- **Eventos del día a día de un miembro regular** (`tomar_tarea`, `cambiar_estado`, `eliminar_tarea` sobre su propio trabajo, etc.): **quedan sin firmar, riesgo aceptado**. Firmar esto requeriría un par de llaves por cada persona del equipo (no solo el Administrador) — una PKI completa, desproporcionada para 3 personas de confianza. Además, quien tiene acceso de escritura a la carpeta compartida por diseño ya puede, siendo honesto, borrar o alterar tareas de cualquiera desde la propia app sin necesitar forjar nada — la firma no cambia esa exposición de fondo, que ya depende por completo de la confianza del equipo (mismo criterio que ya aceptan otros riesgos residuales de esta arquitectura, ver el de rollback de `.db` más abajo).

**Intervalo de escaneo de `ops/` (resuelto)**: tres disparadores, no uno solo —
1. **Al abrir la app**: escaneo inmediato antes de mostrar cualquier tablero, para arrancar con el estado más reciente posible.
2. **Mismo botón "Refrescar" que ya dispara el [polling de Unity VC](#detección-de-cambios-en-unity-version-control-polling)**: reusa la acción manual que el usuario ya conoce, un solo botón para "traer todo lo nuevo" (ramas y eventos).
3. **Temporizador de fondo cada 2 minutos** mientras la app está abierta: más frecuente que el polling de Unity VC (que sí cuesta red/proceso por invocar `cm`), porque escanear `ops/` es solo leer archivos de una carpeta ya sincronizada localmente por Drive/OneDrive — barato, no justifica esperar al intervalo largo de `cm`.

**Compactación/purga de `ops/` (resuelto)**: la carpeta crecería sin límite si nunca se limpia. Diseño: comando Tauri `compactar_eventos()`, **exclusivo Administrador**, disponible desde [Ajustes](./FRONTEND.md#ajustes) —
- Calcula el estado resultante de todos los eventos con más de **30 días** de antigüedad y lo guarda como `ops/snapshot_<fecha>.json` (una sola foto consolidada, no un evento más).
- Borra de `ops/` todos los eventos ya incluidos en ese snapshot.
- Al reconstruir su `.db` local, cualquier máquina parte del snapshot más reciente en vez de reproducir todo el historial desde cero, y solo aplica los eventos posteriores a él.
- **Riesgo aceptado**: una máquina que estuvo offline por más de 30 días y no alcanzó a aplicar eventos ya purgados los pierde (su estado diverge del snapshot en esa franja). Mitigación: la UI de `compactar_eventos()` exige confirmar que todo el equipo sincronizó recientemente antes de habilitar el botón (mismo patrón de doble candado que otras acciones destructivas del Administrador), y no corre automáticamente.

**Qué comandos Tauri pasan por `ops/` y cuáles quedan fuera (resuelto)**:
- **Sí migran a eventos** (mutan estado compartido entre el equipo): `crear_tarea`, `editar_tarea`, `editar_tarea_con_autorizacion`, `tomar_tarea`, `asignar_tarea`, `mandar_a_revision`, `tarea_cerrada_por_deteccion`, `aprobar_tarea`, `editar_y_aprobar_tarea`, `advertir_tarea_pendiente`, `rechazar_tarea`, `eliminar_tarea`, `eliminar_definitivamente`, `aprobar_modificacion`, `rechazar_modificacion`, `autorizar_merge` / `autorizar_merge_subtarea`, `asignar_revision`, `crear_reporte_testing`, `mover_cambios_entre_ramas`, `solicitar_accion_bd`, `crear_solicitud_manual`, `aceptar_solicitud`, `rechazar_solicitud`, `ascender_a_lider`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`, `asignar_rol`, `agregar_colaborador_externo`, `revocar_colaborador_externo`, `actualizar_configuracion_proyecto`, `crear_sprint`, `cerrar_sprint` — cualquier comando que escriba en `tareas`, `usuarios`, `solicitudes_ingreso`, `ediciones_pendientes`, `reportes_testing`, `notificaciones` (la fila de notificación misma), `sprints`, `colaboradores_externos` o `configuracion_proyecto`.
- **Quedan fuera, son locales por máquina/usuario** (no tienen por qué ser iguales entre todo el equipo):
  - `marcar_leida(notificacion_id)` — el estado de "leída" es personal de quien la lee en su propia máquina; si la misma persona usa dos máquinas, no es grave que difiera.
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

**Dos caminos, según quién trabajó la tarea** (definido):

- **Si quien trabajó la tarea es un Desarrollador**: no tiene permiso de mergear él mismo — su único camino es mandar la tarea a "en revisión" para que un Líder/Administrador la autorice (pasos 1-5 más abajo).
- **Si quien trabajó la tarea es un Líder o Administrador** (porque él mismo la tomó y la desarrolló): se salta por completo la carpeta "en revisión" y **mergea/conecta su propia rama a `dev` directamente** — ya tiene el permiso de autorizar, así que no tiene sentido pedirse permiso a sí mismo. Usa el mismo mecanismo del paso 3 (misma validación de orden de merge de abajo hacia arriba, mismo `cm merge` + `cm checkin`), disparado con un botón "Autorizar" directo sobre su propia tarjeta en el Tablero Scrum (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), sin pasar por "Tareas grandes". **Diferencia con el paso 4** si el merge falla por conflicto: como esta tarea nunca pasó por "en revisión", no puede "quedarse" ahí — se queda en **"en producción"** en su lugar; y la opción de "enviarla al Desarrollador para que la corrija" no aplica (no hay ningún Desarrollador en este camino, el propio Líder/Admin hizo el trabajo) — solo le queda resolverlo él mismo en el cliente de Unity VC y reintentar.

El resto de esta sección (pasos 1-5) describe el camino del Desarrollador:

1. El Desarrollador arrastra su tarea a **"En revisión"** en el Tablero Scrum (con la confirmación de doble candado descrita en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)). La descripción escrita se guarda junto a la tarea.
2. La tarea aparece automáticamente en la carpeta **"Tareas grandes"** del apartado **[Revisión](./PLANNING.md#revisión)** (página de estado de PR) — visible para **todos los roles**, pero Desarrollador y Tester la ven en **solo lectura** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).
   - El Administrador puede **asignarle la revisión de esa tarea a un Líder específico** — comando Tauri `asignar_revision(tarea_id, email_lider)`, exclusivo de Administrador. Genera notificación al Líder asignado ("se te asignó revisar [tarea]").
   - **Nuevo**: un Líder también puede **auto-asignarse** una tarea de "Tareas grandes" — comando Tauri `autoasignar_revision(tarea_id)`, invocable por cualquier Líder, mismo efecto que `asignar_revision` pero con `email_lider` = el propio correo de quien lo invoca (validado vía `cm whoami`). En la UI es la opción "Asignármela" del menú de tres puntos en la fila (ver [FRONTEND.md](./FRONTEND.md#alcance)).
   - **Sin asignación explícita** (ni por Admin ni auto-asignada), cualquier Líder/Administrador puede actuar sobre cualquier tarea en "Tareas grandes".
   - **Corregido — con asignación explícita, sí restringe**: una vez que `lider_revision_asignado` tiene un valor (por `asignar_revision` o `autoasignar_revision`), **solo ese Líder** puede autorizar/rechazar esa tarea — otros Líderes la ven en "Tareas grandes" pero sin los botones de acción habilitados, solo lectura, hasta que se reasigne o se libere. El **Administrador conserva override** sobre cualquier tarea sin importar a quién esté asignada (consistente con que es quien puede reasignarla). Sin esto, tener un campo de "asignado" no tenía ningún efecto real — cualquiera podía actuar igual.
3. Cuando el Líder/Admin marca la tarea como **"autorizado"**, el backend valida primero el **orden de merge de abajo hacia arriba** (ver [Subtareas en PLANNING.md](./PLANNING.md#subtareas-tareas-anidadas)): consulta `SELECT COUNT(*) FROM tareas WHERE tarea_padre_id = ? AND estado != 'terminada' AND eliminada = false`. Si el resultado es mayor a 0, **rechaza la acción antes de tocar `cm`** y muestra un error listando qué subtareas siguen sin terminar — no se puede mergear una rama padre mientras tenga hijas abiertas. Si pasa la validación, ejecuta el **merge real de la rama hacia `dev`** invocando `cm` como subproceso (comando exacto pendiente de verificar al implementar, ej. `cm merge` + `cm checkin`) — no es solo un cambio de estado en SQLite, sí toca Unity VC.
4. **Si el merge falla** (conflicto de contenido entre archivos): **decisión tomada** — BranchBoard no implementa resolución de conflictos propia, Unity Version Control ya tiene su propia herramienta para eso. El backend detecta que `cm merge` devolvió error y la tarea **no se marca como terminada** (se queda en "en revisión"). Como quien dispara el "autorizado" es el propio Líder/Admin (acción síncrona, no en segundo plano), la reacción es una **alerta/pantalla inmediata** en ese momento (no una notificación de campana) mostrando que el merge falló, con dos opciones para el Líder/Admin:
   - **Resolverlo él mismo**: cierra la alerta, va al cliente de Unity VC a resolver el conflicto manualmente, y luego reintenta el "autorizado" desde BranchBoard. No se notifica a nadie más.
   - **Enviarla al Desarrollador para que la corrija**: reutiliza el mismo mecanismo de **`rechazar_tarea(tarea_id, motivo)`** ya definido arriba — el motivo queda como "falló el merge por conflicto", la tarea vuelve al Desarrollador (con notificación normal de tarea rechazada), y él la reimplementa y la vuelve a mandar a revisión como cualquier rechazo.
5. Si se **rechaza**, la tarea vuelve al Desarrollador para reimplementar (mismos changesets sobre la misma rama, sin perder historial) — igual que el flujo ya descrito para subtareas rechazadas.
6. Tras un merge exitoso, se dispara la misma revisión de "Detección de cambios" (rama cerrada → tarea "terminada").

Pendiente confirmar: si la acción de "autorizado" también requiere el doble candado (descripción + palabra literal) del punto 1, dado que mergear a `dev` es igual o más delicado que mandar a revisión.

**Nota**: este flujo completo es específico de la carpeta **"Tareas grandes"** de [Revisión](./PLANNING.md#revisión) (tarea general → `dev`). Para el flujo de **"Tareas pequeñas"** (subtarea → su tarea padre), ver la sección siguiente — es más simple y con otro dueño de la decisión.

## Flujo de merge de subtareas pequeñas (hacia su tarea padre)

A diferencia del flujo anterior, este **no** pasa por Líder/Administrador exclusivamente — quien autoriza es **quien esté a cargo de la tarea padre** (puede ser Desarrollador, Líder o Administrador, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).

1. La subtarea simple aparece en la carpeta **"Tareas pequeñas"** de [Revisión](./PLANNING.md#revisión) en cuanto el colaborador hace su checkin (no hay un paso explícito de "mandar a revisar" — el checkin mismo la deja lista para revisión, ya que es un solo archivo, no un flujo de trabajo largo).
2. Quien esté a cargo de la tarea padre abre la fila y ve el visor de [Pre-revisión](./PLANNING.md#pre-revisión) correspondiente al tipo (malla 3D, imagen, audio, video, etc.) — **sin cambiar de rama en su propio workspace** (ver [Guardia de rama incorrecta al sincronizar o crear subtarea](#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea), este flujo es precisamente el que motivó no depender de cambios de rama para revisar algo tan pequeño).
3. Comando Tauri `autorizar_merge_subtarea(subtarea_id)`: ejecuta `cm merge` de la rama de la subtarea hacia la rama de su tarea padre (no hacia `dev`) — mismo patrón de invocar `cm` como subproceso.
4. **Si falla por conflicto**: mismo patrón que el flujo de "Tareas grandes" — alerta síncrona inmediata para quien intentó autorizar, con las mismas dos opciones (resolverlo él mismo, o rechazar con `rechazar_tarea` para que el colaborador vuelva a subir el archivo).
5. Si se **rechaza**: la subtarea vuelve al colaborador — no se borra, simplemente se le pide subir una nueva versión sobre la misma rama hija (ver [Subtareas en PLANNING.md](./PLANNING.md#subtareas-tareas-anidadas)).
6. Tras un merge exitoso, la subtarea pasa a "terminada" — no dispara el flujo completo de "Detección de cambios" del punto anterior (no aplica el chequeo de changesets por polling para algo tan simple), solo el cambio de estado.

Pendiente por definir: si este merge también requiere el doble candado de confirmación, o basta con un solo clic dado que el impacto es mucho menor que mergear a `dev`.

## Eliminar una tarea

Comando Tauri `eliminar_tarea(tarea_id, justificacion)`, validado en backend para que solo Administrador o Líder puedan invocarlo (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)). Requiere el doble candado descrito en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop) (justificación + escribir "eliminar").

- La `justificacion` se guarda en SQLite como parte del registro de auditoría (no se borra junto con la tarea, queda como historial de por qué se eliminó).
- **Confirmado**: si la tarea ya tenía una rama asociada en Unity VC, `eliminar_tarea` también **cierra esa rama** en Unity VC (invocando `cm` — comando exacto pendiente de verificar al implementar), en vez de dejarla huérfana. La `justificacion` queda como el motivo tanto del borrado en BranchBoard como del cierre de la rama.
- **Definido**: esto **no borra la tarea de inmediato** — la manda a la [Papelera](#papelera), desde donde se elimina definitivamente en un segundo paso. Rechazar una tarea de [Pendientes](./PLANNING.md#pendientes) usa este mismo comando (el Líder/Admin decide que no debe existir, así que la "elimina" igual que cualquier otra tarea).
- **Señalado por el usuario**: `eliminar_tarea` también genera una notificación al **creador** de la tarea (ver [Notificaciones](#notificaciones), punto 13) — la `justificacion` escrita es lo que arma el mensaje.
- **Definido — borrado en cascada de subtareas**: si la tarea tiene subtareas colgando (`tarea_padre_id` apuntando a ella, sin importar cuántos niveles de anidación), `eliminar_tarea` **arrastra en cascada solo las subtareas que siguen activas** (`estado` distinto de `terminada` y `eliminada = false`) — se mandan a Papelera junto con la tarea padre, en la misma operación, con la misma `justificacion`, y sus ramas también se cierran en Unity VC. Razón: una vez se cierra la rama padre, dejar una subtarea activa colgando de ella la deja huérfana (su flujo de merge apunta justo a esa rama, ver [Flujo de merge de subtareas pequeñas](#flujo-de-merge-de-subtareas-pequeñas-hacia-su-tarea-padre)).
  - **Las subtareas ya `terminada` (mergeadas) no se tocan** — su trabajo ya está integrado en el historial de la rama padre antes de cerrarla; borrar la tarea padre no debe deshacer trabajo ya completado y mergeado.
  - Cada creador de una subtarea arrastrada recibe la misma notificación #13 que el creador de la tarea padre, con una `justificacion` que aclara que fue un borrado en cascada (ej. "eliminada junto con su tarea padre: <motivo>").

## Papelera

**Nuevo**, va en el menú justo después de "Pendientes". **Visible para todos los roles** (Desarrollador y Tester la ven en **solo lectura**, igual que la carpeta "Tareas grandes" de Revisión) — lista todas las tareas con `eliminada = true`, sin importar si llegaron ahí por `eliminar_tarea` directo o por el rechazo de una tarea de Pendientes.

- Comando Tauri `eliminar_definitivamente(tarea_id)`, **exclusivo Líder/Administrador**, mismo doble candado que el resto de acciones destructivas (escribir "eliminar" para habilitar el botón).
- A diferencia de `eliminar_tarea` (que ya cerró la rama pero conserva la fila como soft-delete para historial), esto **borra la fila por completo** de la tabla `tareas` y purga la rama en Unity VC en vez de solo cerrarla (comando `cm` exacto pendiente de verificar al implementar — depende de si Unity VC permite borrar una rama cerrada o solo cerrarla para siempre).
- **Definido — misma cascada que `eliminar_tarea`**: como el borrado en cascada ya ocurrió al mandar la tarea a Papelera (ver [Eliminar una tarea](#eliminar-una-tarea)), toda subtarea activa que llegó junto a su padre ya está en Papelera también — `eliminar_definitivamente(tarea_id)` sobre la tarea padre purga en la misma operación esas subtareas que llegaron con ella. No aplica a subtareas `terminada` (nunca llegaron a Papelera, siguen existiendo normalmente).
- No hay acción de "restaurar" definida — una vez en Papelera, el único destino es la eliminación definitiva.

## Mover cambios entre ramas

**Nuevo**. Resuelve en la práctica el caso que [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) antes dejaba como "corrígelo manualmente" — ahora BranchBoard ofrece una acción real para mover el trabajo a la rama correcta.

- **UI**: menú de tres puntos sobre la fila de cualquier tarea/rama, opción **"Mover cambios"** — disponible en **cualquier rama que exista**, incluidas las que están "en revisión" y las que están en [Papelera](#papelera) (antes de que se eliminen definitivamente).
- Al elegir la opción, se selecciona la **rama destino** (cualquier otra tarea existente) y el **alcance** de lo que se mueve:
  - **Un solo commit** específico (se elige de la lista de changesets de la rama origen)
  - **Todos los cambios** (todo el historial de la rama origen hacia la destino)
  - **Cambios sin commitear** (trabajo en el workspace que todavía no se guardó con un checkin)
- Comando Tauri `mover_cambios_entre_ramas(rama_origen_id, rama_destino_id, modo, commit_id?)` (**corregido**: nombre unificado — el resto del documento ya lo refería como `mover_cambios_entre_ramas`) — `modo` ∈ `commit_unico` / `todos_los_cambios` / `sin_commitear`. Comando `cm` exacto pendiente de verificar al implementar para cada modo (ej. cherry-pick para un commit, merge/rebase para todos los cambios, shelve/unshelve para lo no commiteado).
- **Permisos** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): cualquier usuario puede mover cambios **entre ramas que le pertenecen a él mismo** (ej. entre una tarea y su subtarea). Mover cambios **entre ramas de dos personas distintas es exclusivo de Líder/Administrador**.
- **Definido**: cuando un Líder/Administrador mueve cambios hacia la rama de otra persona, se le manda una notificación al dueño de esa rama avisándole — a diferencia del resto de notificaciones del sistema, **esta no es automática**: quien mueve los cambios debe escribir un **nombre** (título corto) y una **descripción** explicando qué se movió y por qué, y eso es lo que se le muestra al dueño de la rama en la notificación.

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
- **Eliminar a alguien del equipo** (rol: Administrador o Líder, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)) es tan simple como borrar su fila en la tabla de usuarios: como la identidad se valida contra esa tabla cada vez que se abre la app (no hay sesión persistente que revocar), la persona pierde el acceso **dentro de BranchBoard** de inmediato la próxima vez que intente entrar — su correo ya no aparecería registrado.
  - **Corregido — esto NO revoca su acceso a la carpeta de Drive/OneDrive del proyecto**, que es donde vive todo el dato real (`ops/` con el historial completo de eventos, y todos los `.db` locales de cada quien parten de ahí) — ese acceso lo administra Drive/OneDrive directamente, fuera del control de BranchBoard, y la app no tiene (ni debería tener, ver [Sin servicios en la nube propios](./ARQUITECTURA.md#decisiones-de-arquitectura-ya-tomadas)) forma de tocarlo por API. Alguien eliminado sigue pudiendo leer todo el historial y, en el peor caso, seguir escribiendo eventos si conserva el acceso. `eliminar_usuario` por sí solo **no es suficiente** para sacar a alguien del proyecto.
  - Mitigación: la confirmación de `eliminar_usuario` en la UI incluye un recordatorio explícito, no evitable — "recuerda quitarle a [persona] el acceso a la carpeta de Drive/OneDrive del proyecto; BranchBoard no puede hacerlo por ti" — para que quede como paso manual explícito en vez de una falsa sensación de que ya quedó resuelto.
- **Bootstrapping (primer usuario de un proyecto nuevo) — corregido**: quien inicializa un `.db` de BranchBoard por primera vez para un workspace de Unity VC (ver flujo en [FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)) queda registrado con rol **Administrador**, no Líder. Es un cambio respecto al diseño anterior ("Líder, y el Admin se asigna después a mano"): con la [firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac), **solo un Administrador puede ejecutar los comandos que asignan/firman roles** (`asignar_rol`, `ascender_a_lider`) — si el primer usuario quedara como Líder, nunca existiría nadie que pudiera convertirlo (o a cualquier otro) en Administrador, y el proyecto quedaría sin ancla de confianza de forma permanente. En el mismo momento de la inicialización, el backend genera el par de llaves Ed25519 y firma esa primera fila como Administrador. Consistente con que en Unity Cloud el "Propietario" ya es la persona identificada aparte, ver [Usuarios semilla en SEGURIDAD.md](./SEGURIDAD.md#usuarios-semilla-equipo-virustf) — normalmente coincide con quien inicializa el proyecto.
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

## Comando "Abrir en Unity"

Botón visible dentro de la app una vez hay un proyecto cargado (ver [Panel "Abrir en Unity" en FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), solo para roles con workspace local.

- Comando Tauri `abrir_en_unity()`: lanza el Editor de Unity (o Unity Hub) apuntando a la ruta del workspace, vía su propio CLI — mismo patrón de invocar un ejecutable externo ya usado con `cm` y `warp-cli`.
- **Definido**: al tomar/asignar una tarea, justo después del [auto-switch](#alcance) (`cm switch` a la rama nueva), el modal de confirmación ofrece un botón **"Abrir en Unity"** ahí mismo — la persona no tiene que ir a buscar el ícono de Unity Hub/Editor por su cuenta, ya con el workspace apuntando a la rama correcta. Sigue siendo opcional (algunas tareas simples de un Desarrollador pueden no requerir abrir el Editor de inmediato), pero queda a un clic en el momento justo en que más se necesita.
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

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 4 roles confirmados (Administrador, Líder, Desarrollador, Tester). El modelo de datos de usuario necesita un campo de rol asociado al email de `cm whoami` (ver Identidad de usuario), y cada comando Tauri sensible debe validar permisos en el backend (no solo ocultar en la UI).

**Comando faltante, ahora definido**: la fila "Modificar tareas de otros" de la matriz no tenía comando Tauri asociado — ver `editar_tarea_ajena` en [Notificaciones, punto 4](#notificaciones).

## Notificaciones

Tabla `notificaciones` en SQLite: destinatario (email), tipo, mensaje, tarea relacionada (opcional), leída/no leída, fecha. Casi todos estos casos disparan mensaje **automático**, sin acción manual de quien los envía más allá de la acción que los origina (aprobar, rechazar, asignar, etc.) — las excepciones son el punto 10 (mover cambios entre ramas de distintas personas) y el punto 12 (advertir sobre una tarea pendiente), que sí piden contenido escrito a mano.

**Aclarado**: todas las notificaciones de esta lista son **100% internas a la app** (campana + tabla SQLite) — no se manda ningún correo real por revisión, tareas o permisos. El **único** caso que usa correo de verdad es la solicitud de ingreso a la organización (ver [Solicitud de ingreso vía mailto](#solicitud-de-ingreso-vía-mailto-resuelto)), porque ahí la persona todavía no tiene acceso a BranchBoard para ver una notificación in-app — y ese correo lo manda el cliente de correo de cada quien vía enlaces `mailto:`, nunca la app directamente, para no necesitar un servicio de envío propio.

1. **Tarea creada por un Desarrollador o Tester esperando aprobación** — destinatarios: todos los usuarios con rol Líder o Administrador (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)). También queda listada en el apartado Pendientes, no solo como notificación de campana. Se resuelve con un comando Tauri `aprobar_tarea(tarea_id)` que solo Líder/Admin puede invocar (validación de rol en backend), y que solo pone `pendiente_aprobacion = false` — **no toca el campo `estado`**, que ya venía avanzando por su cuenta desde que se creó la tarea (la tarea nunca estuvo bloqueada: se podía tomar y trabajar desde el primer momento, solo no se podía mandar a revisión). **Nuevo — 4 acciones posibles** sobre esta tarea, todas exclusivas de Líder/Admin (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)):
   - `aprobar_tarea(tarea_id)` — tal como está (ya descrito arriba).
   - `editar_y_aprobar_tarea(tarea_id, campos_editados)` — modifica los campos de la tarea y pone `pendiente_aprobacion = false` en la misma operación (un solo comando, no dos). Genera la notificación #11.
   - `advertir_tarea_pendiente(tarea_id, mensaje)` — no toca ningún campo de la tarea (`pendiente_aprobacion` sigue en `true`), solo genera la notificación #12 con el mensaje escrito a mano por quien la envía.
   - `eliminar_tarea(tarea_id, justificacion)` — mismo comando ya descrito en [Eliminar una tarea](#eliminar-una-tarea), manda la tarea a Papelera.
   - **Definido**: mientras `pendiente_aprobacion = true`, el creador (Desarrollador/Tester) puede editar su propia tarea libremente con un `editar_tarea(tarea_id, campos_editados)` normal (sin aprobación de por medio, ya que la aprobación la da otra persona) — la excepción a la regla del punto 4 de esta lista ("necesita autorización explícita para modificar"), que solo aplica una vez la tarea ya fue aprobada.
2. **Tarea rechazada** — destinatario: el Desarrollador que la creó/tomó. Comando Tauri nuevo `rechazar_tarea(tarea_id, motivo)` (Líder/Admin), simétrico a `aprobar_tarea`. El motivo queda guardado y se muestra en la notificación.
3. **Tarea asignada directamente por Administrador/Líder a un Desarrollador** — destinatario: ese Desarrollador. Comando Tauri nuevo `asignar_tarea(tarea_id, email_destinatario)` (solo Administrador/Líder) — a diferencia de "tomar tarea" (autoasignación), aquí alguien más asigna la tarea. Crea la rama igual que "tomar tarea" lo hace hoy.
4. **Solicitud de autorización para modificar una tarea** — **corregido, flujo invertido**: no es el Líder/Admin quien inicia esto — el **Desarrollador dueño de una tarea ya aprobada** (fuera de Pendientes, en su curso normal) la edita directamente con `editar_tarea_con_autorizacion(tarea_id, campos_nuevos, motivo?)`. Los valores nuevos se aplican **de inmediato** a la tarea (no espera nada para reflejarse), pero queda marcada con una **bandera visual de "edición pendiente"** (sombra amarilla bajo la tarjeta, ver [FRONTEND.md](./FRONTEND.md#alcance)) hasta que un Líder/Administrador la resuelva. También aparece listada en [Pendientes](./PLANNING.md#pendientes), junto a las tareas nuevas esperando aprobación (distinguidas por tipo: "nueva" vs. "edición").
   - Antes de aplicar los cambios, el comando guarda una fila en la nueva tabla `ediciones_pendientes` (ver [Modelo de datos](#modelo-de-datos-tabla-ediciones_pendientes)) con los **valores anteriores** de los campos tocados — necesarios para poder revertir si se rechaza.
   - **`aprobar_modificacion(tarea_id)`** (Líder/Admin): borra la fila de `ediciones_pendientes` — los valores nuevos quedan como definitivos, desaparece la sombra amarilla y sale de Pendientes.
   - **`rechazar_modificacion(tarea_id, motivo?)`** (Líder/Admin): **restaura los campos de la tarea a los valores guardados en `campos_anteriores`** (el cambio se pierde), borra la fila de `ediciones_pendientes`, y notifica al Desarrollador que su edición fue rechazada.
   - **Comando faltante, ahora definido**: la matriz de [Roles y permisos en SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos) ya listaba "Modificar tareas de otros ✅ Administrador/Líder", pero no existía un comando Tauri para eso — todo lo anterior en este punto 4 es para el **dueño** editando su propia tarea. Se agrega **`editar_tarea_ajena(tarea_id, campos_editados)`**, exclusivo Administrador/Líder, válido sobre **cualquier tarea sin importar quién sea el dueño**: aplica los cambios de inmediato y **sin pasar por `ediciones_pendientes`** (a diferencia de `editar_tarea_con_autorizacion`) — al ser quien normalmente aprobaría la edición, no tiene sentido que se autoconfirme a sí mismo. Genera la notificación #14 al dueño de la tarea.
5. **Tarea llega a "en revisión"** — destinatarios: todos los usuarios con rol Líder o Administrador (no solo que aparezca en el apartado [Revisión](./PLANNING.md#revisión) — también genera notificación en la campana).
6. **Revisión asignada a un Líder específico** — destinatario: ese Líder. Generada por `asignar_revision(tarea_id, email_lider)` (solo Administrador). Mensaje tipo "se te asignó revisar [tarea]".
7. **Solicitud de ingreso a la organización aprobada** — destinatario: quien la solicitó, pero solo aplica si ya tiene acceso in-app; el aviso real al aceptarse es el `mailto:` de "Avisar por correo" (ver [Solicitud de ingreso vía mailto](#solicitud-de-ingreso-vía-mailto-resuelto)), ya que hasta ese momento la persona no tiene forma de ver notificaciones dentro de BranchBoard.
8. **Recordatorio de inactividad en Unity Version Control**: si una tarea con rama (Scripting u otra tarea general) no tiene **changesets nuevos** en un tiempo determinado, se le manda al Desarrollador dueño un recordatorio del tipo "llevas X días sin actividad en [tarea], recuerda [qué le toca hacer]". Se apoya en el mismo mecanismo de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) (`cm log br:/<rama> --xml`), pero a diferencia del polling manual por "Refrescar", **este caso sí necesita revisarse solo, sin que nadie le dé clic a nada** — implica correr esta revisión con un temporizador mientras la app está abierta (ej. cada N horas), no solo bajo demanda. Pendiente definir: el umbral de tiempo exacto (¿3 días? ¿configurable por Líder/Admin?), y si esto corre también para subtareas simples.
9. **Solicitud de acción sobre la base de datos** — un Líder o Desarrollador que detecta un problema (ej. datos raros, algo se ve corrupto) no puede actuar directamente sobre la BD, así que le manda una notificación a **todos los Administradores** pidiendo que revisen/restauren. Comando Tauri `solicitar_accion_bd(motivo)`, disponible para Líder y Desarrollador (no para Tester, que no tiene relación con el ciclo de tareas/BD del equipo). El `motivo` escrito queda en el mensaje de la notificación para que el Admin sepa qué está pasando antes de actuar.
10. **Te movieron cambios a tu rama** — destinatario: el dueño de la rama destino, cuando un Líder/Administrador mueve cambios hacia una rama que no es la suya (ver [Mover cambios entre ramas](#mover-cambios-entre-ramas)). **No es automática**: quien mueve los cambios debe escribir un **nombre** (título corto) y una **descripción** al confirmar la acción — eso es lo que arma el mensaje de la notificación, ya que el sistema no puede inferir por su cuenta qué se movió ni por qué.
11. **Tu tarea pendiente fue editada y aprobada** — destinatario: el Desarrollador/Tester que creó la tarea. Generada por `editar_y_aprobar_tarea(tarea_id, campos_editados)` (ver punto 1). Automática — el mensaje puede armarse solo listando qué campos cambiaron.
12. **Advertencia sobre tu tarea pendiente** — destinatario: el Desarrollador/Tester que creó la tarea. Generada por `advertir_tarea_pendiente(tarea_id, mensaje)` (ver punto 1). **No es automática**, igual que el punto 10: el Líder/Admin escribe el mensaje a mano explicando qué está mal — la tarea sigue pendiente, sin cambios, para que el creador la corrija él mismo.
13. **Tu tarea fue eliminada** — destinatario: el **creador** de la tarea (aplica a cualquier `eliminar_tarea`, no solo a un rechazo desde Pendientes — ver [Eliminar una tarea](#eliminar-una-tarea)). Automática: usa la misma `justificacion` que ya se escribe para el doble candado como contenido del mensaje, no pide texto aparte.
14. **Un Administrador/Líder editó tu tarea directamente** — destinatario: el dueño de la tarea (Desarrollador/Tester). Generada por `editar_tarea_ajena(tarea_id, campos_editados)` (ver punto 4). Automática — a diferencia del punto 11/12, aquí no hay bandera de "edición pendiente" ni acción posterior que tomar: el cambio ya quedó definitivo, el mensaje solo informa qué campos cambiaron y quién lo hizo.

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
- `cambiar_rol(email, rol_nuevo)` — **nuevo, resuelto**: exclusivo Administrador, cubre lo que `ascender_a_lider` no cubre — degradar a un Líder, o reasignar entre Desarrollador/Tester. Firma la fila igual que los demás comandos de rol (ver [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)). No permite asignar `rol = Administrador` (eso requeriría transferir también la llave privada, fuera de alcance del MVP — solo se resuelve regenerando llaves y redistribuyendo el build, ver [Respaldo de la llave privada](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)).
- `confirmar_usuario(email)` — exclusivo Administrador, ver [Confirmación de usuarios nuevos](#confirmación-de-usuarios-nuevos-resuelto)

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
| `firma` | texto (base64) | **nuevo** — firma Ed25519 de `email\|rol\|firmado_en`, ver [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac) |
| `firmado_en` | fecha/hora | **nuevo** — timestamp incluido en el mensaje firmado, evita reutilizar una firma vieja para otro campo |

Borrar una fila de esta tabla = [eliminar a esa persona del equipo](#identidad-de-usuario) (revoca acceso de inmediato, sin sesión que cerrar).

## Firma criptográfica del rol (ancla de confianza para el RBAC)

**Resuelve**: el `.db` de SQLite se sincroniza por Drive/OneDrive con lectura/escritura para todo el equipo — cualquiera podía abrir el archivo y poner su propio `rol = 'Administrador'` a mano, y el backend no tenía forma de distinguir ese cambio de uno hecho legítimamente desde la app (hallazgo C1 de la auditoría de seguridad).

**Mecanismo**:
- **Corregido — orden real de los pasos** (la versión anterior decía a la vez "se genera al configurar el proyecto" y "queda embebida en el binario que ya se compiló", lo cual es contradictorio: una llave generada en tiempo de ejecución no puede estar ya horneada en un binario compilado antes): la generación de llaves es un **paso de preparación del proyecto, no un botón dentro de la app ya instalada por el equipo**.
  1. Quien va a ser Administrador corre `generar_llaves_admin()` **una sola vez**, en un modo de configuración/build previo a distribuir la app (la misma persona técnica que arma el build para el equipo — coherente con que ya es quien inicializa el proyecto, ver [Bootstrapping](#identidad-de-usuario)).
  2. La **llave pública** resultante se escribe en un archivo de configuración del proyecto que el proceso de build de Tauri incluye como constante embebida (`include_str!`/`env!` en tiempo de compilación) en el binario que se empaqueta y distribuye a **todo el equipo de ese proyecto específico** — de ahí que "todos los clientes la traen igual": es la misma build, no que cada instalación genere o descubra la llave por su cuenta.
  3. La **llave privada** nunca entra a ese paso de build — se guarda **fuera de la carpeta sincronizada por Drive/OneDrive** (ej. `%LocalAppData%\BranchBoard\admin.key`, solo en la máquina del Administrador) y nunca viaja por la nube.
     - **Protección en reposo (resuelto)**: por defecto, `admin.key` queda protegida solo por los permisos del sistema operativo (en Windows, el archivo hereda los permisos del perfil de usuario — otros usuarios de esa máquina no pueden leerlo). Es la misma base de confianza que ya usan llaves SSH/GPG típicas en una máquina de desarrollo — comprometer el perfil del Administrador implica comprometer la llave de todas formas. Se ofrece además un **modo opcional recomendado**: cifrar `admin.key` en reposo con la misma passphrase que ya usa `exportar_llave_firma(passphrase)` — al activarlo, cada operación que firma (`asignar_rol`, `cambiar_rol`, etc.) pide la passphrase la primera vez por sesión de la app y la mantiene solo en memoria (nunca en disco) mientras esa sesión sigue abierta. Queda como opción, no obligatorio, para no forzarle a un equipo chico una fricción diaria que quizás no quiere.
  - **Consecuencia asumida**: cada proyecto nuevo de BranchBoard tiene su propio par de llaves y, en sentido estricto, su propia build del binario (aunque el código fuente sea el mismo). Regenerar las llaves (ej. llave privada perdida sin respaldo) implica recompilar y redistribuir el instalador al equipo, no solo un cambio de datos.
- Cada vez que se asigna o cambia un rol — `asignar_rol(email, rol)`, `ascender_a_lider(email)`, `confirmar_usuario(email)` (ver abajo) — el backend, corriendo en la máquina del Administrador, firma el mensaje `email|rol|firmado_en` con la llave privada y guarda `rol`, `firma` y `firmado_en` en la fila correspondiente de `usuarios`.
- **Al leer la tabla `usuarios`** (arranque de la app, cualquier chequeo de permisos), el backend recalcula la firma esperada con la llave pública embebida y la compara contra `firma`. Hay tres estados posibles, no dos:
  - **Firma presente y coincide** → rol de confianza, se aplica tal cual.
  - **Firma presente pero NO coincide** (fila editada a mano fuera de la app, o campos `rol`/`firmado_en` alterados) → **no confiable**: el usuario se degrada al rol mínimo (Desarrollador) y la UI muestra un aviso de "rol no verificado — posible modificación manual del archivo".
  - **`firma` es `NULL`** (fila recién creada por `aceptar_solicitud`/`crear_solicitud_manual`, ver [Confirmación de usuarios nuevos](#confirmación-de-usuarios-nuevos-resuelto), no por una manipulación) → **bloqueado, no degradado**: la persona no puede entrar a la app todavía, ni siquiera como Desarrollador, hasta que el Administrador la confirme. Es un estado legítimo de "esperando firma", distinto del caso de arriba (que sí es sospechoso).
- Solo el Administrador puede ejecutar comandos que firman (`asignar_rol`, `ascender_a_lider`, `cambiar_rol`, `confirmar_usuario`) — validado igual que cualquier otro comando sensible.

### Confirmación de usuarios nuevos (resuelto)

**Problema**: `aceptar_solicitud`/`crear_solicitud_manual` (ver [Solicitud de ingreso vía mailto](#solicitud-de-ingreso-vía-mailto-resuelto)) los puede invocar un **Líder**, pero solo el **Administrador** tiene la llave privada para firmar — un Líder no puede firmar el rol de la persona que acaba de dar de alta. La versión anterior de este documento listaba `crear_usuario` como uno de los comandos que firman sin notar esta incompatibilidad.

**Resuelto** (decisión del usuario: mantener la capacidad de alta del Líder tal como está, sin restringirla al Administrador):
- `aceptar_solicitud`/`crear_solicitud_manual` insertan la fila en `usuarios` **de inmediato**, con el rol elegido pero `firma = NULL`, `firmado_en = NULL` — igual que hoy, sin pedirle nada extra a quien acepta la solicitud.
- Esa persona **no puede entrar a BranchBoard todavía**: la [Pantalla inicial](./FRONTEND.md#autenticación--acceso-al-proyecto) reutiliza la misma pantalla de "tu cuenta no está registrada en este proyecto todavía", con el mensaje ajustado a "tu cuenta fue agregada por [quien la dio de alta], está pendiente de que el Administrador la confirme".
- Nuevo comando `confirmar_usuario(email)`, **exclusivo Administrador**: firma la fila (mismo mecanismo que `asignar_rol`) — a partir de ahí la persona ya puede entrar con el rol que le asignaron.
- Aparece listado en [Ajustes → Gestión de usuarios](./FRONTEND.md#ajustes) con una etiqueta **"Pendiente de confirmar"**, visible para el Administrador con el botón `confirmar_usuario`, y visible (solo lectura) para el Líder que la dio de alta, para que sepa que todavía no tiene efecto real.
- **Trade-off asumido**: alguien dado de alta por un Líder no puede empezar a trabajar hasta que el Administrador confirme, aunque sea de forma asíncrona (no tiene que ser inmediato/en vivo). Para un equipo de 3 personas donde el Administrador está normalmente disponible, es un costo bajo a cambio de mantener el punto único de firma que es el eje de todo C1.

**Aclarado — dar de alta en `usuarios` NO le da acceso a la carpeta de Drive/OneDrive, y sin esa carpeta la persona no puede usar la app en absoluto** (simétrico a lo ya documentado para [eliminar a alguien del equipo](#identidad-de-usuario)): `aceptar_solicitud`/`crear_solicitud_manual`/`confirmar_usuario` solo tocan la tabla `usuarios` — todos comandos de BranchBoard, nada más. Pero como el `.db` local de esa persona se reconstruye leyendo `ops/` (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)), necesita **antes que nada** tener acceso real a esa carpeta compartida. Eso pasa 100% fuera de BranchBoard y de forma manual:
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

## Modelo de datos: tabla `colaboradores_externos`

Separada de `usuarios` — estas personas **sí tienen `cm` instalado** (liviano, solo para el workspace parcial vía Gluon, ver [Workspace parcial (Gluon) para subtareas simples](#workspace-parcial-gluon-para-subtareas-simples)) pero no tienen uno de los 4 roles principales — solo permiso puntual para crear subtareas simples (ver [Permisos de subtareas simples en FRONTEND.md](./FRONTEND.md#ajustes)).

| Campo | Tipo | Notas |
|---|---|---|
| `email` | texto (PK) | **corregido**: viene de `cm whoami`, igual que en `usuarios` — no se escribe/selecciona a mano dentro de la app |
| `nombre` | texto | |
| `agregado_por` | email | qué Líder/Admin le dio el permiso |
| `fecha_alta` | fecha | |

Borrar una fila = revocar el permiso; la próxima vez que `cm whoami` identifique a esa persona, ya no aparece en la lista de permitidos.

**Comandos Tauri (nuevo, resuelto)** — el panel de [Permisos de subtareas simples en FRONTEND.md](./FRONTEND.md#ajustes) ya describía los botones de agregar/revocar, pero no existía el comando de backend ni estaba en la lista de eventos, así que un alta/revocación hecha en una máquina no llegaba a las demás:
- `agregar_colaborador_externo(email, nombre)` — Líder/Administrador, inserta la fila.
- `revocar_colaborador_externo(email)` — Líder/Administrador, la borra.
- Ambos **migran a eventos** (agregarlos a la lista en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)) — de lo contrario un colaborador agregado desde la máquina de un Líder seguiría bloqueado en la máquina donde otro Líder/Admin intente verificar su permiso.

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
| `registrada_por` | email | **nuevo** — el Líder/Administrador que creó la fila al leer el correo del solicitante, ver [Solicitud de ingreso vía mailto (resuelto)](#solicitud-de-ingreso-vía-mailto-resuelto) |

Al aceptarse, se inserta la fila correspondiente en `usuarios` con el rol que le asigne quien aprueba — la solicitud no desaparece, queda como historial con `estado = aceptada`.

### Solicitud de ingreso vía mailto (resuelto)

**Problema**: BACKEND.md prometía que "el solicitante recibe un correo avisando que fue aceptado" y que la solicitud inicial "sale por correo", pero nunca hubo un mecanismo definido para mandarlo — y construir uno (SMTP propio, API de terceros tipo SendGrid) contradice la decisión ya tomada de [Sin servicios en la nube propios](./ARQUITECTURA.md#decisiones-de-arquitectura-ya-tomadas). Además, quien todavía no es parte del equipo **tampoco tiene acceso a la carpeta de Drive/OneDrive del proyecto** — no puede insertar una fila en `solicitudes_ingreso` aunque quisiera, porque ese mecanismo depende de [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos), a la que solo el equipo ya agregado tiene acceso.

**Solución — el correo lo manda el cliente de correo del propio usuario, no la app**:
- **Ida (solicitante → equipo)**: la pantalla de [Solicitud de ingreso](./FRONTEND.md#autenticación--acceso-al-proyecto) arma un enlace `mailto:` con los datos del formulario (nombre, apellido, correo, ID de organización) ya escritos en el cuerpo del mensaje, dirigido a un **correo de contacto** que el Administrador configuró de antemano en [Ajustes](./FRONTEND.md#ajustes) (nuevo campo `correo_contacto_solicitudes`). Al hacer clic, se abre el cliente de correo por defecto del solicitante (`tauri-plugin-shell`, abrir el URI `mailto:...`) — es su propio correo el que sale, la app nunca envía nada por su cuenta ni guarda credenciales SMTP.
- Quien recibe ese correo (Líder/Administrador) y decide darle curso, lo registra manualmente dentro de BranchBoard con el comando `crear_solicitud_manual(nombre, apellido, correo, id_organizacion)` — inserta la fila en `solicitudes_ingreso` con `registrada_por = su email`, y de ahí sigue el flujo normal ya definido (aparece en [Solicitudes de ingreso](./FRONTEND.md#ajustes), se acepta/rechaza con `aceptar_solicitud`/`rechazar_solicitud`).
- **Vuelta (equipo → solicitante, al aceptarse)**: mismo patrón — al aceptar, la UI ofrece un botón **"Avisar por correo"** que arma otro `mailto:` prellenado dirigido al correo del solicitante, y lo manda el cliente de correo de quien aprobó. No es automático (requiere ese clic extra), pero tampoco necesita ningún servicio propio.
- **Trade-off aceptado**: a diferencia de un correo automático real, este flujo depende de que una persona efectivamente revise su bandeja y haga los dos clics (abrir el mailto, y luego darle "Enviar" en su cliente de correo). Para un equipo chico es aceptable; si algún día se necesita 100% automático, ahí sí haría falta un servicio de envío real, lo que reabriría la discusión de "sin servicios en la nube propios".

## Modelo de datos: tabla `tareas`

Consolidando todo lo definido a lo largo de la conversación en un esquema concreto:

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | identificador único |
| `titulo` | texto | |
| `descripcion` | texto | viene del modal de confirmación al crear/tomar/mandar a revisar (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)) |
| `tipo` | enum | Scripting / Modelado 3D / Modelado 2D / Animación / Imagen / UI-Interfaz / Audio / Música / Video (**corregido**: antes eran 6 valores y no cubrían los 9 tipos reales usados por [Categorías](./PLANNING.md#alcance) y [Pre-revisión](./PLANNING.md#pre-revisión) — Modelado se separó en 2D/3D, y se agregaron UI/Interfaz y Audio como valores propios) |
| `tarea_padre_id` | nullable | si es subtarea (rama hija); `null` si es tarea general |
| `archivo_ruta` | nullable | solo subtareas simples: ruta del archivo dentro del workspace de Unity VC |
| `estado` | enum | por hacer / en producción / en revisión / terminada (**corregido**: ya no incluye "bloqueada", ver `pendiente_aprobacion`) |
| `pendiente_aprobacion` | booleano | `true` si la creó un Desarrollador o Tester y todavía no la aprueba un Líder/Admin (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)) — `false` de entrada si la crea Líder/Admin. Es independiente de `estado`: mientras es `true`, el único efecto es que `mandar_a_revision` la rechaza; no impide tomarla ni crearle rama. |
| `rama_asociada` | nullable | `null` hasta que alguien "toma" la tarea |
| `ultimo_changeset_visto` | nullable | para el polling de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling) |
| `modificada` | booleano | badge de "hay cambios nuevos" (ver polling) |
| `creado_por` | email | quien creó la tarea |
| `asignado_a` | email, nullable | quien la tomó/le fue asignada |
| `lider_revision_asignado` | email, nullable | ver [asignar_revision en Flujo de revisión](#flujo-de-revisión-y-merge-a-dev) |
| ~~`autorizada_modificacion`~~ | — | **eliminado** (flujo invertido, ya no es un permiso que se otorga por adelantado) — el estado de "edición pendiente" ahora se deriva de si existe una fila para esta tarea en [`ediciones_pendientes`](#modelo-de-datos-tabla-ediciones_pendientes), ver [Notificaciones](#notificaciones), punto 4 |
| `sprint_id` | nullable | una tarea puede no pertenecer a ningún sprint (ver [Modelo de datos: tabla `sprints`](#modelo-de-datos-tabla-sprints)) |
| `con_fecha` / `fecha_limite` | booleano / nullable | tareas con y sin tiempo (ver [Sprints y tareas en FRONTEND.md](./FRONTEND.md#alcance)) |
| `fecha_completada` | nullable | usada para la [gráfica de Avance](#datos-para-la-gráfica-de-avance) |
| `eliminada` | booleano | **soft-delete** (ver [Eliminar una tarea](#eliminar-una-tarea)) — mientras es `true` la tarea vive en [Papelera](#papelera); `eliminar_definitivamente` sí borra la fila por completo después |
| `justificacion_eliminacion` / `fecha_eliminacion` | nullable | solo si `eliminada = true` |
| `motivo_rechazo` | nullable | último motivo de rechazo, si aplica |
| `reporte_testing_origen_id` | nullable | si esta tarea nació de un reporte de Testing (ver abajo), referencia a ese reporte |

## Modelo de datos: tabla `sprints`

**Resuelto** (confirmado por el usuario: se implementan ambas variantes — con tiempo y sin tiempo — mismo patrón que `con_fecha`/`fecha_limite` en `tareas`, ver [Sprints y tareas en FRONTEND.md](./FRONTEND.md#alcance)):

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | |
| `nombre` | texto | |
| `con_fecha` | booleano | igual que en `tareas`: define si este sprint tiene compromiso de tiempo o es tipo backlog |
| `fecha_inicio` | fecha | se fija al crear el sprint, siempre tiene valor (con o sin `con_fecha`) |
| `duracion_dias` | entero, nullable | solo si `con_fecha = true` — cuántos días dura, lo pregunta el modal de creación |
| `fecha_fin` | fecha, nullable | calculada = `fecha_inicio + duracion_dias` al crear el sprint (no se recalcula después); solo si `con_fecha = true`. Es lo que dispara el [Reporte automático de cierre](./PLANNING.md#reportes) al llegar esa fecha |
| `cerrado` | booleano | `true` cuando el sprint ya terminó — automático al llegar `fecha_fin` (sprints con fecha) o manual (sprints sin fecha, ver abajo) |
| `fecha_cierre_real` | fecha, nullable | cuándo se cerró de verdad (coincide con `fecha_fin` en sprints con fecha; es la fecha del cierre manual en sprints sin fecha) |
| `creado_por` | email | |

**Comandos Tauri**:
- `crear_sprint(nombre, con_fecha, duracion_dias?)` — exclusivo Líder/Administrador, mismo criterio que crear/organizar el resto de la estructura del proyecto. Si `con_fecha = true`, `duracion_dias` es obligatorio y el backend calcula `fecha_fin` de una vez.
- `cerrar_sprint(sprint_id)` — exclusivo Líder/Administrador, **solo aplica a sprints sin fecha** (`con_fecha = false`): como no tienen un punto de cierre automático, alguien tiene que cerrarlo a mano para que dispare el mismo Reporte que un sprint con fecha genera solo. Un sprint con fecha no se puede cerrar manualmente antes de tiempo con este comando (**pendiente por definir**: si hace falta un `cerrar_sprint_anticipado` aparte para ese caso, o si simplemente no se soporta en el MVP).
- `listar_sprints()` — cualquier rol, para poblar la vista de sprints y el selector al crear/editar una tarea.

Asignar una tarea a un sprint es simplemente escribir su `sprint_id` — no hay un comando dedicado, se hace como parte de `crear_tarea`/`editar_tarea`.

## Modelo de datos: tabla `ediciones_pendientes`

Una fila = una edición de un Desarrollador sobre su propia tarea (ya aprobada) esperando que un Líder/Admin la confirme o la revierta (ver [Notificaciones](#notificaciones), punto 4).

| Campo | Tipo | Notas |
|---|---|---|
| `id` | — | |
| `tarea_id` | — | la tarea editada |
| `campos_nuevos` | JSON | los valores que ya están aplicados en `tareas` (la edición se ve de inmediato) |
| `campos_anteriores` | JSON | los valores previos a la edición — se usan para revertir si `rechazar_modificacion` |
| `motivo` | texto, nullable | por qué se quiere el cambio, escrito por quien edita (opcional) |
| `creado_por` | email | el Desarrollador que editó |
| `fecha` | fecha | |

Borrar esta fila (al aprobar o rechazar) = la tarea deja de mostrar la sombra amarilla y sale de [Pendientes](./PLANNING.md#pendientes). Solo puede existir **una** fila activa por tarea a la vez — no se acumulan varias ediciones pendientes sobre la misma tarea (pendiente por definir: si se bloquea una segunda edición mientras la primera sigue sin resolver, o si la reemplaza).

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

Comando Tauri `crear_reporte_testing(tarea_revisada_id, descripcion, adjunto?)`, exclusivo Tester: inserta el reporte y, en la misma operación, crea la fila en `tareas` de corrección (mismo `tipo` que la tarea revisada, `tarea_padre_id = null` porque es una tarea general nueva, no una subtarea). **Resuelto**: nace con `estado = 'por hacer'` y `pendiente_aprobacion = false` — no pasa por aprobación, ver [Testing en PLANNING.md](./PLANNING.md#testing).

## Modelo de datos: tabla `configuracion_proyecto`

**Nuevo, resuelto**: faltaba dónde vivir el campo `correo_contacto_solicitudes` (ver [Solicitud de ingreso vía mailto](#solicitud-de-ingreso-vía-mailto-resuelto)) — no había ninguna tabla de configuración a nivel proyecto en el modelo de datos. Clave/valor simple, pensada para que cualquier ajuste futuro de este tipo (no por tarea, no por usuario, sino del proyecto entero) tenga dónde ir sin agregar una tabla nueva cada vez:

| Campo | Tipo | Notas |
|---|---|---|
| `clave` | texto (PK) | ej. `correo_contacto_solicitudes` |
| `valor` | texto | |
| `actualizado_por` | email | |
| `fecha_actualizacion` | fecha | |

Comando Tauri `actualizar_configuracion_proyecto(clave, valor)` — exclusivo Administrador. **Migra a eventos** (agregarlo a la lista en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)) — de lo contrario cambiar el correo de contacto en una máquina no se reflejaría en las demás.

## Pendiente por definir
- Cómo se valida el permiso de rol en cada comando Tauri (mecanismo exacto de autorización, ej. middleware/decorador vs. chequeo manual al inicio de cada comando)
- Estados del flujo de una tarea (ya definidos en la práctica: por hacer / en producción / en revisión / terminada, más la bandera aparte `pendiente_aprobacion` — falta solo confirmarlos como lista cerrada)
- Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)
- Comando `cm` exacto para leer si una rama está cerrada/mergeada
- ~~Intervalo exacto de escaneo de la carpeta `ops/`~~, ~~estrategia de compactación/purga de `ops/`~~ y ~~qué comandos migran al mecanismo de eventos~~ — **corregido**: los tres quedaron resueltos más arriba en este mismo documento (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)); esta sección no se había actualizado después de resolverlos.
- Librería/mecanismo exacto de conversión `.fbx` → glTF para el visor de [Pre-revisión de Modelado 3D](./PLANNING.md#pre-revisión) — la restricción de subir solo `.fbx` ya está decidida, falta elegir qué convierte ese archivo al formato que consume el visor (`model-viewer`/Three.js).
