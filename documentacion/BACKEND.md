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
- **Control de versiones: Unity Version Control** (antes Plastic SCM), no Git genérico. Se usa su CLI (`cm`). La integración desde el backend se hace en **Rust directo** (sin Python), invocando `cm` como subproceso. **Actualizado (2026-11-12)**: las operaciones **del servidor** (crear ramas, merge al autorizar, reporte de conflictos, diff, leer permisos) usan la **API REST del servidor de Unity VC** con un token personal (PAT) de cada usuario; `cm` queda para el **workspace** (checkin, switch, status, abrir Unity, Gluon) y para crear el PAT. Ver [Acceso a la API REST del servidor de Unity VC](#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12).
- **Flujo de vida de una tarea**: se crea visual/sin rama → al "tomarla" alguien, el backend (Rust) crea la rama en Unity VC invocando `cm` → el sprint es el tiempo que esa rama vive antes de mergear a `dev` (ver [Flujo de revisión y merge a dev](#flujo-de-revisión-y-merge-a-dev)). El merge de `dev` hacia `main` (release) queda **fuera del alcance de BranchBoard** — es una acción manual que hace alguien técnico directamente en Unity VC, no algo que la app gestione.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, SQLite, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri) que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP**: ver lista completa y autoritativa en [Pantallas / tableros del MVP en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp) — no se duplica aquí para evitar que este documento quede desactualizado cuando se agreguen pantallas nuevas.
- **Referencia: el frontend manda** (actualizado 2026-11-07). El mockup de `app/frontend/` (React, datos en memoria) y [FRONTEND.md](./FRONTEND.md) son la referencia de lo que el producto hace: este documento describe lo que el backend real tiene que hacer para que ese frontend funcione. Cuando el frontend y este documento no coinciden, se corrige este documento. Los modelos de datos del frontend (`src/types.ts`) y sus acciones (`src/store/useStore.ts`) son el contrato: cada acción del store corresponde a un comando Tauri descrito aquí (ver [Contrato con el frontend](#contrato-con-el-frontend-acciones-y-comandos)). Lo que está diseñado aquí pero el frontend todavía no tiene se marca como **"sin pantalla en el mockup"**.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite local por máquina** (sin servidor). Cada máquina tiene su propio `.db`, que **no se sincroniza directamente** — lo que se comparte por la nube (ej. Drive/OneDrive) es un log de eventos (carpeta `ops/`), y cada máquina reconstruye/actualiza su `.db` reproduciendo esos eventos (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)). Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de verdad, y este mecanismo de eventos deja de ser necesario.
- Integración con Unity Version Control desde Rust invocando el CLI `cm` como subproceso: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios). **Actualizado (2026-11-12)**: crear la rama (al tomar **o al asignar**) y el merge al autorizar pasan a la [API REST del servidor](#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12) (`POST .../branches`, `POST .../mergeto`); lo que toca el workspace local (`cm switch`, `cm status`, `cm checkin`) sigue con `cm`.
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`). **Actualizado (2026-11-07)**: tomar una tarea se confirma escribiendo la palabra **"asignar"** (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)); ya no se escribe una descripción, así que el comentario del changeset inicial de la rama lo arma el backend (ej. `Rama de T-123: <título>`). Tomar una tarea pone `asignado_a`, `modo_trabajo = 'rama'` y `rama_asociada` (ruta completa, ej. `/main/dev/T-101-combate`; ver [Protección de ramas](#protección-de-ramas-main-y-dev))
- **Condición de carrera al tomar/asignar una tarea (resuelto, ver arquitectura de eventos)**: un `UPDATE ... WHERE estado = 'por hacer'` solo evita colisiones dentro de una única BD compartida en vivo — no sirve aquí porque cada máquina tiene su propio `.db` con retraso de sincronización. La resolución real vive en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos): `tomar_tarea`/`asignar_tarea` generan un evento, y el desempate entre dos tomas simultáneas ocurre de forma determinista al reproducir los eventos en cada máquina, no con un `UPDATE` condicionado local.
- **Toda rama de tarea general nace desde la cabeza actual de `dev`, nunca desde un punto viejo**: al crear la rama (`cm branch create` o equivalente) no se fija un changeset específico guardado de antes — se apunta siempre a la última versión de `dev` en el momento exacto de "tomar" la tarea. Esto importa especialmente para tareas que se crean y se toman en momentos distintos (ej. una tarea de corrección de Testing que nadie toma hasta días después, y mientras tanto `dev` siguió recibiendo otros merges) — la rama nueva debe partir de lo último, no de cómo estaba `dev` cuando se creó la tarea.
- **Auto-switch al tomar una tarea (importante, medida de seguridad)**: justo después de `cm branch create`, `tomar_tarea` ejecuta **`cm switch <rama-nueva>`** sobre el workspace local de quien la tomó — su Unity Editor queda apuntando automáticamente a esa rama. Sin esto, la persona podría seguir escribiendo código/cambios sobre la rama en la que estaba antes (una tarea anterior, o `dev`) sin darse cuenta, mezclando trabajo de dos tareas distintas en la misma rama. Ver también [Guardia de rama incorrecta al sincronizar o crear subtarea](#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea) y [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) para los casos en que igual pase.
- **Colaborador externo solo en tareas individuales** (2026-11-07, ver [SEGURIDAD.md](./SEGURIDAD.md#colaborador-externo)): `tomar_tarea` y `asignar_tarea` rechazan la acción si quien queda a cargo es Colaborador externo y la tarea tiene subtareas (no eliminadas) o es subtarea de otra (`tarea_padre_id` no nulo). Al reproducir el evento se valida igual y se descarta si no cumple.
- **`asignar_tarea` NO hace auto-switch (corregido)**: el diseño original hacía que `asignar_tarea` ejecutara `cm switch` igual que `tomar_tarea`, pero eso corre en el proceso de Tauri de **quien invoca el comando** (el Líder/Administrador que asigna) — un `cm switch` ahí cambiaría la rama del workspace de quien asigna, no la del Desarrollador destino, que ni siquiera tiene la app abierta en ese momento. `asignar_tarea` solo crea la rama (`cm branch create`; **actualizado 2026-11-12**: por la API REST, `POST .../branches` con la cabeza de `dev`, así quien asigna no necesita tener workspace del repo — probado) y genera la notificación #3; el cambio de rama en la máquina del Desarrollador asignado ocurre después, como acción explícita de **su propio lado**. **Actualizado (2026-11-07, según el frontend)**: no hay un botón aparte de "Cambiar mi workspace a esta rama"; se hace desde el panel **"Abrir en Unity"**, que lista sus tareas abiertas y, al elegir una, cambia su workspace a esa rama (`cm switch`) y abre Unity ahí (ver [Comando "Abrir en Unity"](#comando-abrir-en-unity)). Nunca automático ni silencioso, para no interrumpirle sin avisar un cambio de rama mientras podría estar trabajando en otra cosa.
- **Quién asigna** (según el frontend): Líder o Administrador, desde la Lista de tareas, solo en tareas **sin subtareas** que no estén terminadas; si la tarea ya tiene rama, solo cambia el responsable. Se confirma escribiendo **"asignar"**.
- **Soltar una tarea** (`desasignar_tarea`, arrastrarla a "Sin asignar", **implementado #98**): quita el responsable y la deja en "por hacer" junto con sus subtareas. Se rechaza si la tarea o alguna subtarea ya tiene commits (hay progreso, ver [Estados, progreso y reaperturas](#estados-progreso-y-reaperturas)). Además `modelo::ahora_iso()` usa precisión de milisegundos (`SecondsFormat::Millis`) para que operaciones rápidas en el mismo segundo en una misma máquina conserven orden determinista en `ops/`.

## Sincronización entre máquinas (arquitectura de eventos)

Reemplaza el diseño anterior de "lock de escritura sobre el `.db` compartido": sincronizar directamente un archivo SQLite entre varias máquinas por Drive/OneDrive no funciona, porque el sync es **por archivo completo, con retraso, y sin fusión de filas** — dos máquinas escribiendo casi al mismo tiempo pueden terminar con la copia de una pisando por completo la de la otra, sin importar si tocaron la misma tarea o no.

**Diseño**: lo que se sincroniza deja de ser el `.db`. Cada máquina tiene su propio `.db` **local, no sincronizado, desechable y reconstruible**. Lo que sí viaja por la carpeta de nube es una carpeta `ops/` de **eventos inmutables**, uno por archivo:

- Cada comando Tauri que muta estado compartido (`tomar_tarea`, `asignar_tarea`, `cambiar_estado`, `aprobar_tarea`, `rechazar_revision`, crear/editar/eliminar tarea, etc.; lista completa más abajo) — en vez de hacer un `UPDATE`/`INSERT` directo que luego se sincroniza como parte del `.db` — escribe un archivo nuevo en `ops/`, nombrado `ops/<timestamp_iso>_<usuario>_<uuid>.json`, con `{ tipo, tarea_id, usuario, timestamp, payload }`. Nunca se edita ni se borra un archivo de `ops/` ya creado — solo se crean nuevos. Esto es lo que evita el problema de sincronización de archivo completo: como nadie más edita ese archivo, Drive/OneDrive nunca tiene que decidir "qué versión gana".
- Cada máquina mantiene en su `.db` local una tabla `eventos_aplicados` (id de archivo ya procesado) para no reaplicar el mismo evento dos veces.
- Al abrir la app, y luego en el mismo intervalo que [el polling de Unity VC](#detección-de-cambios-en-unity-version-control-polling), el backend escanea `ops/`, toma los archivos que no estén en `eventos_aplicados`, los ordena por `timestamp` ascendente (desempate determinista: alfabético por `usuario`) y los aplica uno por uno a su `.db` local, con la **misma regla en todas las máquinas** — así todas convergen al mismo estado sin coordinarse en vivo.

**Implementado — escribir eventos (2026-11-14, issue #91)** en `app/backend/src/eventos.rs`:
- `Evento { tipo, tarea_id, usuario, timestamp, payload }`, con `timestamp` ISO 8601 en UTC con milisegundos. El archivo se llama `ops/<AAAAMMDDTHHMMSSmmmZ>_<correo>_<32 hex>.json`: el timestamp del **nombre** va sin `:`, porque Windows no los permite en nombres de archivo, y el correo solo con `a-z0-9@.-`.
- **Inmutable**: solo existe escribir. Se escribe a un temporal (`.<nombre>.tmp`) y se renombra; si el nombre ya existe no se pisa. Quien lea `ops/` (#92) ignora lo que no termine en `.json`.
- **La máquina que lo escribe lo aplica de inmediato**: `eventos::publicar(conexion, ops, evento, aplicar)` aplica el cambio en una transacción, escribe el archivo y lo anota en la tabla local `eventos_aplicados` (migración 6). Todo o nada: si `aplicar` o la escritura fallan, la base queda como estaba y no hay archivo. Si se escribió el archivo pero falló el commit, el evento no está en `eventos_aplicados`, así que el escaneo (#92) lo aplicará después.
- **Carpeta compartida por máquina**: cada persona tiene su Drive/OneDrive en otra ruta, así que se guarda en `local.json` de la carpeta local del proyecto (`%LocalAppData%\BranchBoard\proyectos\<org>-<repo>\`) y **no** en la base, que se puede reconstruir o restaurar sin perderla. Comandos `carpeta_compartida()` y `configurar_carpeta_compartida(ruta)` (todos los roles): la ruta tiene que ser completa, existir y no ser la carpeta local de datos; se guarda como `C:\…`, sin el prefijo `\\?\` que agrega Windows.
- **Tiene que ser la carpeta de este proyecto** (2026-11-14): en la carpeta compartida queda el archivo `branchboard-proyecto.json` con `{ "proyecto": "<org>-<repo>" }`. Si la carpeta elegida tiene esa marca, debe ser de este proyecto ("Esa carpeta es de otro proyecto"). Si no la tiene, **solo el Administrador** la deja como la del proyecto y le escribe la marca; a los demás se les pide la carpeta que les compartió el Administrador (o esperar a que Drive/OneDrive termine de sincronizarla). Así nadie publica sus cambios en una carpeta que el resto del equipo nunca lee.
- **Dónde se elige**: en el acceso, justo después de `entrar_a_proyecto` y antes del tablero, si en esta máquina el proyecto todavía no tiene carpeta; y en Ajustes → "Carpeta compartida" (ver [FRONTEND.md](./FRONTEND.md#ajustes)). El explorador de carpetas lo abre el backend: comando `elegir_carpeta(titulo)` (libre), que devuelve la ruta o `null` si se cancela; así la webview no necesita acceso al disco. Lo usa todo `SelectorCarpeta` (también al clonar).
- Los primeros comandos que publican eventos son los de usuarios (#93, firmados, ver abajo); los de tareas llegan con #95.
- Prueba en el entorno real: `BB_PRUEBA_CARPETA_COMPARTIDA=<carpeta de OneDrive/Drive> cargo test --lib entorno_real_eventos -- --ignored`.

**Implementado — leer y aplicar eventos de otras máquinas (2026-11-14, issue #92)** en `app/backend/src/eventos/lectura.rs`:
- `eventos::sincronizar(conexion, ops, carpeta_proyecto, ahora, aplicar)` lee de `ops/` solo los `.json` que no están en `eventos_aplicados`; los temporales (`.…tmp`) y lo que no es `.json` se ignoran. Un archivo ilegible (a medio sincronizar) se salta **sin marcarlo** y se reintenta en el siguiente escaneo.
- **Orden igual en todas las máquinas**: por `timestamp`, luego por `usuario`, y al final por nombre de archivo, para que el orden sea total.
- **Reloj adelantado**: si un evento declara más de 5 minutos en el futuro respecto al reloj de esta máquina, se ordena por su **hora de llegada** (fecha del archivo en `ops/`) y se anota en `eventos.log` de la carpeta local del proyecto. Es solo un log: no se avisa al equipo.
- **Cada evento en su propia transacción**, con el aplicador de su tipo (`lectura::aplicar_evento`):
  - `Aplicado`: se marca.
  - `Descartado(motivo)`: lo que haya escrito el aplicador se deshace, el evento se marca igual (el resultado es el mismo en todas las máquinas) y se anota en el log. Es el caso de la doble toma de arriba.
  - `Desconocido`: lo escribió una versión más nueva de la app; no se marca y queda pendiente hasta que esta máquina se actualice.
  - Un error de la base detiene el escaneo sin marcar el evento.
  - **Tipos registrados**: los cuatro de usuarios (#93, abajo). Cada comando que pase a eventos registra el suyo (#95 tareas…).
- **Comando `sincronizar_eventos()`** (todos los roles): devuelve `{ aplicados, descartados, desconocidos, ilegibles }`. Si ya hay un escaneo en curso (`AppState::iniciar_sincronizacion`), no hace nada. Escanea `ops/` si hay carpeta compartida y, **desde #239**, si el proyecto está vinculado al servidor también sube la bandeja y baja del servidor ([Recepción desde el servidor](#recepción-desde-el-servidor-239)); el resumen suma las dos.
- **Cuándo se escanea** (frontend, `lib/useSincronizacionEventos.ts`, montado en `App`): al entrar al proyecto, al volver la ventana a primer plano (`focus` / `visibilitychange`) y cada 2 minutos. Un fallo no se muestra cada vez; se reintenta en la siguiente revisión. Lo del servidor llega antes: el backend lo consulta cada 5 s y avisa con el evento Tauri `eventos-recibidos`. La suscripción independiente `useEventosServidor`, activa mientras hay proyecto abierto, sube `versionDatos` aunque no exista sesión de Unity VC (#239).
- **El frontend se refresca solo**: si `aplicados > 0`, sube `versionDatos` en el store. **Toda pantalla que lea datos del backend** (desde #95) lo pone como dependencia de su carga para volver a pedirlos sola.
- Prueba en el entorno real: `BB_PRUEBA_CARPETA_COMPARTIDA=<carpeta de OneDrive/Drive> cargo test --lib entorno_real_dos_maquinas -- --ignored` (dos máquinas simuladas, cada una con su base y su carpeta local, sobre la misma carpeta de OneDrive).

> **Quitado en #243 (2026-11-21)**: ya no hay firmas. Los eventos de usuarios (`crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`) y `compactar_eventos` se **descartan siempre** al leer `ops/` (`eventos::TIPOS_RETIRADOS`): sin firma nadie podría probar quién los mandó. Los roles los cambian las funciones del servidor ([#241](#usuarios-y-roles-por-el-servidor-241)); un proyecto que todavía va por Drive/OneDrive puede ver su equipo pero no cambiarlo hasta pasarlo al servidor (#245). Lo de abajo queda como historia.

**Implementado — firmar los eventos administrativos (2026-11-14, issue #93)** en `app/backend/src/eventos.rs`, `eventos/lectura.rs` e `identidad/equipo.rs`. Es la primera familia de comandos que pasa por `ops/`:
- `Evento` tiene el campo opcional **`firma_admin`**: firma Ed25519 (base64) del evento completo sin ese campo, como JSON compacto (`tipo`, `tarea_id`, `usuario`, `timestamp`, `payload`). Cualquier campo cambiado después de firmar la invalida. `Evento::firmar(llave)` y `Evento::firma_valida(publica)`.
- **Tipos que la exigen** (`eventos::TIPOS_FIRMADOS`): `crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario` y `compactar_eventos` (este llega con #94 y firma con el mismo `Evento::firmar`). Al aplicar (`lectura::aplicar_evento(conexion, publica_admin, evento)`), si la firma falta o no coincide el evento se **descarta sin mirar su contenido**, se anota en `eventos.log` ("firma_admin falta o no coincide") y no se reintenta. No hay rol mínimo al que degradar un evento.
- **Quién puede firmar** (`equipo::firmante_del_evento`): la llave del Administrador que esta máquina fijó (`admin.pub`), o la de un **Líder certificado** cuando `usuario` es ese Líder y la fecha del evento cae dentro de la vigencia de su certificado (igual que las filas que firma: desde la fecha del certificado hasta que el Administrador le quitó el rol). El rol con el que se aplica es el que dice la llave, no el que declare el evento.
- **La llave privada nunca viaja**: la fila ya firmada (`email`, `rol`, `firma`, `firmado_en`, `firmado_por`) va dentro del `payload`; `crear_usuario` lleva la fila completa de `usuarios`, `eliminar_usuario` lleva `{ email, refirmadas: [...] }` con las filas que el Administrador volvió a firmar al sacar a un Líder. Las demás máquinas escriben esas firmas tal cual y las verifican como si se hubieran hecho ahí.
- **Las mismas reglas en todas las máquinas**: la máquina de quien hace el cambio firma, arma el evento y lo publica con `eventos::publicar`, que lo aplica ahí mismo con `equipo::aplicar_evento`; las demás lo aplican con la misma función al sincronizar. Un evento que ya no cumple las reglas con el estado local (correo repetido, rol que ese firmante no puede dar, fila firmada por otra persona que la que emite el evento, confirmar una fila que no está pendiente) se descarta; solo un error de la base detiene el escaneo. Lo que pasa a `Descartado` se deshace.
- **Carpeta compartida obligatoria**: como cada cambio es un evento, `crear_usuario`/`cambiar_rol`/`confirmar_usuario`/`eliminar_usuario` responden `validacion` ("Configura la carpeta compartida…") si en esta máquina el proyecto todavía no la tiene; la interfaz ya la pide antes del tablero.
- **Riesgo que sigue aceptado**: el `timestamp` del evento lo escribe quien lo emite, así que un Líder revocado podría fechar un evento antes de su revocación (mismo caso que la fecha de las filas, ver [Firma de los Líderes](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)).
- Prueba en el entorno real: `BB_PRUEBA_CARPETA_COMPARTIDA=<carpeta de OneDrive/Drive> cargo test --lib entorno_real_eventos_administrativos -- --ignored` (A crea y cambia de rol a un usuario, B lo aplica igual; B suelta dos eventos falsos para hacerse Líder, uno sin firma y otro con una llave suya, y las dos máquinas los descartan).

**Caso concreto — doble toma de la misma tarea**: dos personas le dan clic a "tomar tarea" sobre la misma tarea casi al mismo tiempo, cada una en su propia máquina, sin saber todavía que la otra lo hizo. Al aplicar el evento `tomar_tarea` de cada una, el backend solo lo efectúa si, según el estado ya calculado hasta ese punto, la tarea sigue en `'por hacer'`; si no, el evento se descarta. Así queda un **"ganador"** (a quien se le queda asignada la tarea) y un **"perdedor"** (a quien se le descarta el evento). Si el evento descartado es el que generó esta misma máquina (la persona pensó que la había tomado), se dispara una notificación tipo "X se adelantó — tu toma de la tarea Y fue revertida", y el frontend corrige la UI optimista que había mostrado. El resultado es el mismo que el `UPDATE ... WHERE estado='por hacer'` original, pero aplicado de forma diferida y determinista en vez de en vivo contra una BD compartida que no existe.

**Corregido — revertir el `.db` no deshace lo que ya pasó en Unity VC, y un evento "tardío" no siempre debería ganar solo por tener timestamp más viejo**: el diseño de arriba, aplicado tal cual, tiene dos huecos:
1. `tomar_tarea` ya ejecutó `cm branch create` + `cm switch` **en el momento de invocar el comando**, no cuando se reproduce el evento — las dos máquinas ya crearon su rama antes de que el desempate ocurra. Revertir solo el `.db` deja una **rama huérfana** en el repo y el workspace del perdedor apuntando a una rama que ya no corresponde a ninguna tarea.
2. Una máquina que estuvo offline puede sincronizar horas después con un evento de timestamp **anterior** al de quien "ganó" en vivo — si ese desempate se aplica ciegamente por timestamp, revierte retroactivamente a alguien que ya lleva horas trabajando y con changesets reales en su rama.

**Resuelto**: antes de aplicar la reversión, el backend revisa si la rama del "perdedor" tiene **changesets reales más allá del inicial** (mismo dato que ya usa [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling), comparar contra `ultimo_changeset_visto` en el momento de crear la rama):
- **Sin changesets reales todavía** (rama recién creada, nadie hizo checkin) → seguro revertir automático: además de corregir el `.db`, el backend cierra/borra esa rama huérfana vía `cm`, y la notificación al perdedor incluye un botón explícito **"Volver mi workspace a `dev`"** (mismo patrón de acción explícita ya usado para el auto-switch de `asignar_tarea` — nunca un `cm switch` silencioso en la máquina de otra persona).
- **Con changesets reales** (alguien ya trabajó de verdad en esa rama, sin importar si su evento llegó "tarde") → **no se revierte automático**: se trata igual que [Detección de ediciones en rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app) — alerta a ambas personas y a un Líder/Administrador para que decidan a mano cómo resolverlo (ej. una de las dos ramas se convierte en subtarea, o se descarta la que menos avanzó), en vez de que el sistema decida solo y arriesgarse a tirar trabajo real.

**Riesgo aceptado — manipulación del reloj local**: el desempate usa el timestamp que cada máquina le pone a su propio evento; alguien podría adelantar el reloj de su PC para "ganar" siempre una carrera. Se documenta como **riesgo aceptado de baja prioridad** dado el tamaño y nivel de confianza del equipo. Se descartó explícitamente usar el timestamp "server-side" de la API de Drive/OneDrive como desempate más confiable: requeriría credenciales OAuth por usuario y por proveedor (contradice [Sin backend propio de autenticación y Sin servicios en la nube propios](./ARQUITECTURA.md#decisiones-de-arquitectura-ya-tomadas)) y dependencia de estar online en el momento de resolver el orden, rompiendo el funcionamiento sin conexión que tiene hoy escribir un evento local.

- **Mitigación barata añadida (no requiere red ni credenciales)**: al aplicar un evento de `ops/`, cada máquina compara el `timestamp` del evento contra **su propio reloj local** (el de quien está leyendo, no el de quien lo escribió). Si el evento viene marcado más de **5 minutos en el futuro** respecto al reloj de la máquina que lo procesa, se trata como sospechoso: se aplica igual (para no bloquear el flujo), pero se reordena usando **la hora de llegada a `ops/` en vez del timestamp declarado**, y se registra un aviso interno (no notificación al equipo, solo log). Esto no elimina el riesgo — todas las máquinas confían en su propio reloj, que también podría estar mal por error genuino — pero acota cuánta ventaja puede sacar alguien adelantando su reloj: ya no es ilimitada, queda topada a la tolerancia de 5 minutos frente al reloj de las demás máquinas del equipo.

> **Quitado en #243 (2026-11-21)**: ningún evento va firmado. Cada aplicador revisa el rol del autor con la tabla `usuarios` (copia de `miembros`) y, con el proyecto en el servidor, el servidor rechaza los tipos que el rol no permite (#247).

**Integridad de los eventos — qué está firmado y qué no (resuelto)**: un evento de `ops/` es un JSON plano con `usuario` **autodeclarado** — nada impide, en principio, que alguien con acceso de escritura a la carpeta (todo el equipo, por diseño) suelte un evento suplantando a otro miembro. Distinguir dos niveles de gravedad en vez de tratar esto como un solo problema:
- **Eventos que solo un Administrador (o un Líder certificado, dentro de lo que puede asignar) puede emitir legítimamente** (`crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`, `compactar_eventos`, ver [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac); **actualizado 2026-11-07**: `asignar_rol` ya no existe, lo reemplaza `cambiar_rol`, y los Líderes también firman, ver [Firma de los Líderes](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)): **se extiende la misma firma Ed25519** que ya protege el campo `rol` de `usuarios` a estos eventos completos — el evento incluye un campo `firma_admin` (firma del JSON del evento con la llave privada del Administrador), y cada máquina la verifica contra la llave pública embebida antes de aplicarlo; si no coincide o falta, el evento se descarta sin aplicar (no se degrada, se ignora — a diferencia de una fila de `usuarios` inconsistente, aquí no hay "rol mínimo" al que degradar un evento). Esto cierra el caso más grave: nadie puede forjar una elevación de rol ni una acción administrativa aunque tenga acceso de escritura a `ops/`.
- **Eventos del día a día de un miembro regular** (`tomar_tarea`, `cambiar_estado`, `eliminar_tarea` sobre su propio trabajo, etc.): **quedan sin firmar, riesgo aceptado**. Firmar esto requeriría un par de llaves por cada persona del equipo (no solo el Administrador) — una PKI completa, desproporcionada para 3 personas de confianza. Además, quien tiene acceso de escritura a la carpeta compartida por diseño ya puede, siendo honesto, borrar o alterar tareas de cualquiera desde la propia app sin necesitar forjar nada — la firma no cambia esa exposición de fondo, que ya depende por completo de la confianza del equipo (mismo criterio que ya aceptan otros riesgos residuales de esta arquitectura, ver el de rollback de `.db` más abajo).

**Intervalo de escaneo de `ops/` (resuelto)**: tres disparadores, no uno solo —
1. **Al abrir la app**: escaneo inmediato antes de mostrar cualquier tablero, para arrancar con el estado más reciente posible.
2. **Al volver la app a primer plano** (el mismo momento en que se repite la [escucha de Unity VC](#detección-de-cambios-en-unity-version-control-polling)). **Actualizado (2026-11-07)**: el frontend no tiene un botón "Refrescar"; todo llega solo.
3. **Temporizador de fondo cada 2 minutos** mientras la app está abierta: más frecuente que el polling de Unity VC (que sí cuesta red/proceso por invocar `cm`), porque escanear `ops/` es solo leer archivos de una carpeta ya sincronizada localmente por Drive/OneDrive — barato, no justifica esperar al intervalo largo de `cm`.

> **Quitado en #243 (2026-11-21)**: la compactación y los snapshots (`eventos/snapshot.rs`, comando `compactar_eventos`, botón de Ajustes) se quitaron con las firmas: un snapshot reescribe la base entera y solo la firma del Administrador impedía falsificarlo. Un `snapshot_<fecha>.json` que siga en `ops/` se descarta; lo que una base ya tenía aplicado se queda. Con el servidor no hace falta compactar (los eventos viven en `eventos`, #233). Drive/OneDrive se quita del todo en #244.

**Compactación/purga de `ops/` (resuelto)**: la carpeta crecería sin límite si nunca se limpia. Diseño: comando Tauri `compactar_eventos()`, **exclusivo Administrador**, disponible desde [Ajustes](./FRONTEND.md#ajustes) —
- Calcula el estado resultante de todos los eventos con más de **30 días** de antigüedad y lo guarda como `ops/snapshot_<fecha>.json` (una sola foto consolidada, no un evento más).
- Borra de `ops/` todos los eventos ya incluidos en ese snapshot.
- Al reconstruir su `.db` local, cualquier máquina parte del snapshot más reciente en vez de reproducir todo el historial desde cero, y solo aplica los eventos posteriores a él.
- **Riesgo aceptado**: una máquina que estuvo offline por más de 30 días y no alcanzó a aplicar eventos ya purgados los pierde (su estado diverge del snapshot en esa franja). Mitigación: la UI de `compactar_eventos()` exige confirmar que todo el equipo sincronizó recientemente antes de habilitar el botón (mismo patrón de doble candado que otras acciones destructivas del Administrador), y no corre automáticamente.

**Implementado — compactar `ops/` con un snapshot (2026-11-15, issue #94)** en `app/backend/src/eventos/snapshot.rs`, `eventos/lectura.rs` y `comandos/eventos.rs`:
- `compactar_eventos(passphrase?)`: exclusivo **Administrador** (`identidad/permisos.rs` y validación criptográfica contra `admin.pub`; un Líder no puede emitirlo). Toma el lock de sincronización (`AppState::iniciar_sincronizacion_ops`), escanea `ops/` primero para estar al día y luego calcula en una base en memoria el estado resultante de todos los eventos con `timestamp <= ahora - 30 días` (partiendo del snapshot válido anterior, si existía, más esos eventos en el mismo orden determinista que `sincronizar`).
- Si no hay ningún evento de más de 30 días fuera del snapshot anterior, devuelve `{ compactados: 0, archivo: null }` sin escribir nada.
- Si los hay, escribe de forma atómica `ops/snapshot_<AAAA-MM-DD>.json` (si ya había uno del mismo día, `snapshot_<fecha>_02.json`…) como un `Evento` de tipo `compactar_eventos` firmado con la llave privada del Administrador (`Evento::firmar`), cuyo `payload` lleva `{ fecha, hasta, incluidos, estado }` con todas las tablas compartidas (`usuarios`, `configuracion_proyecto`, `sprints`, `tareas`, `ediciones_pendientes`, `reportes_testing`, `commits`, `cambios_prestados`, `notificaciones`, `documentos` con su `ruta` interna, `mensajes_chat`, `grupos_chat`, `reportes_sprint`). Después anota el snapshot y los incluidos en `eventos_aplicados` y borra de `ops/` los archivos incluidos y cualquier `snapshot_*.json` anterior.
- **Al reconstruir o sincronizar (`eventos/lectura.rs`)**: los `snapshot_*.json` pendientes se evalúan antes que los eventos sueltos, del más reciente al más antiguo. El primero cuya firma de Administrador (`equipo::firmante_del_evento` con `Rol::Administrador`) y `payload` sean válidos reemplaza el estado de las tablas compartidas (`snapshot::aplicar_evento`, conservando el estado local de cada máquina como `leida` en notificaciones) y marca como ` aplicados` los nombres de `incluidos`; cualquier snapshot más viejo o cualquier evento suelto con `timestamp <= hasta` del snapshot vigente se anota en `eventos_aplicados` sin reaplicarse. Un snapshot sin firma o firmado por alguien que no es el Administrador se descarta y queda en `eventos.log`.
- Prueba en el entorno real: `BB_PRUEBA_CARPETA_COMPARTIDA=<carpeta de OneDrive/Drive> cargo test --lib entorno_real_compactar -- --ignored` (A publica eventos de >30 días y uno reciente, compacta, verifica que en OneDrive quedan solo `snapshot_<fecha>.json` y el evento reciente, B reconstruye su `.db` desde cero partiendo del snapshot + el evento reciente y descarta un snapshot falso sin firma).

**Qué comandos Tauri pasan por `ops/` y cuáles quedan fuera (resuelto)**:
- **Sí migran a eventos** (mutan estado compartido entre el equipo). **Actualizado (2026-11-07)** con todas las acciones del frontend:
  - Tareas: `crear_tarea` (y `crear_tarea_con_subtareas`), `editar_tarea`, `proponer_edicion`, `tomar_tarea`, `asignar_tarea`, `desasignar_tarea`, `cambiar_estado` (incluye mandar a revisión), `borrar_progreso`, `tarea_cerrada_por_deteccion`, `eliminar_tarea`, `eliminar_definitivamente`.
  - Pendientes: `aprobar_tarea` / `aprobar_tareas`, `editar_y_aprobar_tarea`, `advertir_tarea_pendiente`, `aprobar_modificacion` / `aprobar_modificaciones`, `rechazar_modificacion`.
  - Revisión: `autorizar_merge`, `rechazar_revision`, `reabrir_tarea`, `asignar_revision`.
  - Ramas: `hacer_checkin`, `crear_rama_desde_espera`, `traer_cambios`, `devolver_cambios_prestados`, `eliminar_cambios_ajenos`, `trasladar_cambios_directos` (#225), `mover_cambios_entre_ramas` (sin pantalla en el mockup).
  - Planificación: `crear_sprint`, `cerrar_sprint`, `asignar_tarea_a_sprint`, `asignar_tarea_a_backlog`, `asignar_backlog_a_sprint`.
  - Equipo y proyecto: `crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`, `renombrar_proyecto`, `actualizar_configuracion_proyecto`, `subir_imagen_proyecto`, `solicitar_accion_bd`, `enviar_mensaje`. Los tres del proyecto viajan como un solo tipo de evento, `actualizar_configuracion_proyecto` (`{ clave, valor }`, issue #127).
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

## Servidor (Supabase)

**Nuevo (2026-11-20, #232)** — base de la migración decidida en #231 (hoja de ruta #248): Supabase reemplaza a Drive/OneDrive para sincronizar, con SQLite local en cada PC.

**Proyectos** (organización BranchBoard, plan Free):

| Proyecto | Para qué | Lo usa |
|---|---|---|
| `branchboard` | Producción | El instalador (build `release`) |
| `branchboard-pruebas` | Pruebas y desarrollo | `npm run tauri dev` (build `debug`) y las pruebas reales |

Los dos se crearon igual: Data API activada, **RLS automático en tablas nuevas**, **sin** exposición automática de tablas a la API (cada tabla se expone al escribir sus reglas) y sin integración con GitHub (las migraciones se aplican a mano, ver abajo).

**Configuración de la app** (`app/backend/src/supabase/entorno.rs`): la URL y la clave **pública** (publishable) de cada proyecto. `Entorno::actual()` elige pruebas en `debug` y producción en `release`; la variable `BB_SUPABASE=pruebas|produccion` lo fuerza. La clave pública por sí sola no permite nada: lo que se puede leer o escribir lo deciden las reglas RLS (#234). La `sb_secret_…`, la contraseña de Postgres y los tokens de la CLI **nunca** van al repo (es público); una prueba del módulo falla si alguien pega una clave secreta.

**Estructura de la base**: solo por migraciones SQL en `supabase/migrations/` (se revisan en el PR). La carpeta `supabase/` la creó la CLI (`npx supabase@2 init`); de su `config.toml` se ajustó: contraseña mínima de 8 caracteres (como la app) y **sin confirmación de correo** (decidido en #236, ver [Cuentas de BranchBoard](#cuentas-de-branchboard-236)).

**Configuración de Auth** (`supabase/config.toml` → servidor): se sube con `npx --yes supabase@3 config push` (solo el dueño de la cuenta de Supabase, primero en pruebas). Ese comando sube **todo** lo que declara el archivo, así que antes se corre `npx --yes supabase@3 config diff`: tiene que mostrar solo lo que se quiere cambiar. Hasta #236 el archivo nunca se había subido (el servidor seguía con contraseña mínima de 6 y código de 8); en #236 se igualaron al servidor los valores que no son de Auth (pooler, Storage, MFA, `site_url`…) y se subió lo de Auth. Hoy `config diff` contra pruebas da **0 diferencias**.

**Aplicar migraciones** (solo el dueño de la cuenta de Supabase, desde su máquina):
1. `npx supabase@2 login` — abre el navegador; la página muestra un **código de verificación** que se escribe en la terminal. Guarda un token personal de la CLI en esa máquina (no en el repo). Una sola vez.
2. `npx supabase@2 link --project-ref freppiixmuspvozaqrxj` (**branchboard-pruebas**) — pide la contraseña de Postgres de ese proyecto (en PowerShell se pega con **clic derecho**; no se ve al escribirla). Queda anotado en `supabase/.temp/project-ref` (ignorado por git).
3. `npx supabase@2 db push --linked` — muestra las migraciones nuevas y pregunta antes de aplicarlas en **pruebas**. Con el login ya no pide la contraseña. `--dry-run` muestra qué aplicaría sin tocar nada; `migration list --linked` compara local y servidor. (Sin `--linked` funciona hoy, pero la CLI avisa que en la próxima versión mayor apuntará a una base local.)
4. Después del merge, lo mismo con `link --project-ref rqistwyevplwdslyuvbk` (**branchboard**, producción) y `db push --linked`; después volver a vincular pruebas.

Comprobado el 2026-11-20 con la migración del esquema base (#233).

### Esquema base (#233)

Migración `supabase/migrations/20261120230000_esquema_base.sql`:

| Tabla | Qué guarda | Equivale hoy a |
|---|---|---|
| `perfiles` | Una fila por cuenta de Supabase Auth: correo (en minúsculas, único), nombre, apellido. Se crea sola al registrarse (trigger sobre `auth.users`; nombre y apellido salen de los metadatos del registro). | Las cuentas locales de `cuentas.ts` |
| `proyectos` | Nombre y, si es de Unity VC, `unity_org` + `unity_repo` (únicos: un repo es un solo proyecto aunque lo traigan dos personas). Sin Unity VC, los dos vacíos. | La marca `branchboard-proyecto.json` y la carpeta de datos `<org>-<repo>` |
| `miembros` | (proyecto, usuario, rol, quién lo agregó, cuándo). Roles: `Administrador`, `Líder`, `Desarrollador`, `Tester`, `Colaborador externo` (los de `modelo::Rol`). Puede haber varios Administradores. | La tabla `usuarios` con sus firmas Ed25519 |
| `eventos` | Registro de solo-agregar: `orden` (lo asigna el servidor), `id` (el que genera la app, único), proyecto, autor, `tipo`, `tarea_id`, `payload` (JSON), `creado_en_app` y **`recibido_en`** (hora del servidor, la que manda para ordenar y desempatar). | Los archivos de `ops/` |

**Por qué los roles no son eventos**: hoy `crear_usuario`, `cambiar_rol`, `confirmar_usuario` y `eliminar_usuario` son eventos firmados (`TIPOS_FIRMADOS`). Sin firmas (decisión 3 de #231), cualquier miembro podría insertar un evento `cambiar_rol` y darse Administrador. Por eso los roles están en `miembros` y solo los cambiarán funciones del servidor (#234). Todo lo demás (tareas, sprints, chat, documentos, notificaciones) sigue siendo un evento con el mismo `tipo` y `payload` que hoy, así la aplicación de eventos de la app se reaprovecha (#238, #239).

**Acceso**: las cuatro tablas tienen RLS activado; las reglas y funciones que deciden quién puede qué llegaron en #234 (ver abajo).

### Reglas RLS y funciones de roles (#234)

Migraciones `20261121000000_reglas_rls_y_roles.sql` y `20261121000100_ajustes_seguridad.sql`. El detalle de quién puede qué está en [SEGURIDAD.md → Reglas del servidor](./SEGURIDAD.md#reglas-del-servidor-supabase-234). Lo que la app puede usar con su sesión (rol `authenticated`):

| Qué | Cómo |
|---|---|
| Leer proyectos, miembros, eventos y perfiles | `select` normal por la API de datos; solo devuelve lo de los proyectos de los que se es miembro |
| Editar el propio nombre y apellido | `update perfiles set nombre, apellido` (el correo no) |
| Publicar un evento | `insert into eventos (id, proyecto_id, tipo, tarea_id, payload, creado_en_app)`; el servidor pone `autor`, `orden` y `recibido_en`. Si el `tipo` está en `privado.permisos_evento` y el rol de quien lo manda no está en sus roles, se rechaza con `42501` (#247, tabla en [SEGURIDAD.md](./SEGURIDAD.md#reglas-del-servidor-supabase-234)) |
| Crear un proyecto sin Unity VC | UUID v4 local; al sincronizar `rpc/crear_proyecto(p_nombre, p_id)` confirma ese mismo id y deja al creador como Administrador |
| Agregar a alguien (o invitarlo si aún no tiene cuenta, #241) | `rpc/agregar_miembro(p_proyecto, p_correo, p_rol)` → id, o `null` si quedó invitado |
| Cancelar una invitación | `rpc/quitar_invitacion(p_proyecto, p_correo)` (#241) |
| Cambiar un rol | `rpc/cambiar_rol(p_proyecto, p_usuario, p_rol)` |
| Quitar a alguien (o salirse) | `rpc/quitar_miembro(p_proyecto, p_usuario)` |

**Ojo con la bandeja de salida (#238)**: un `42501` al subir un evento no es un fallo de red y reintentarlo no sirve (pasa si a alguien le bajaron el rol mientras tenía eventos esperando). Hoy la bandeja, ante un lote que falla "sin permiso", se para y no deja pasar a los siguientes; y como manda lotes de 50, un solo evento rechazado tumba el lote entero. **Resuelto (2026-11-21)**: la bandeja aparta el evento rechazado y sigue con los demás (ver [Bandeja de salida](#bandeja-de-salida-238)).

Las funciones auxiliares que usan las reglas (`es_miembro`, `mi_rol`, `comparte_proyecto`, `puede_asignar`, `puede_quitar`, `exigir_otro_administrador`, `puede_publicar`) y los triggers viven en el esquema **`privado`**, que la API no expone. Errores: `42501` = sin permiso; `P0001` = regla de negocio (último Administrador, persona sin cuenta, ya está en el proyecto), con el mensaje en español.

**Pruebas de las reglas** (`supabase/tests/01_reglas.test.sql`, 63 casos; 21 de ellos de eventos según el rol, #247): crean cuentas, proyectos y eventos de mentira dentro de una transacción que se deshace al final, así que no dejan nada en la base. No usan `supabase test db` (necesita Docker) sino la CLI v3:

```
npx supabase@3 db query --remote --yes --file supabase/tests/01_reglas.test.sql
```

Devuelve las líneas TAP y `fallidas` (tiene que ser 0). Con una expectativa cambiada a propósito sale `not ok`, así que las pruebas detectan errores. Además, `npx supabase@3 db advisors --linked --type security` revisa la seguridad: los únicos avisos que quedan son las 4 funciones `rpc/…` de arriba, a propósito (la app las llama con su sesión y cada una revisa el rol por dentro).

### Cliente de Supabase (#235)

Módulo `app/backend/src/supabase/cliente.rs`. Se eligió un cliente en **Rust** (con `ureq`) y no `supabase-js` en la webview: así la webview sigue sin red (ver [SEGURIDAD.md](./SEGURIDAD.md)) y los tokens nunca llegan al JavaScript. Los cambios llegan por **consulta periódica** (decidido en #239, ver [Recepción desde el servidor](#recepción-desde-el-servidor-239)); Realtime queda para después.

`ClienteSupabase::new(&PROYECTO, llavero)` habla con el proyecto que elige `Entorno::actual()` y ofrece:

| Función | Qué hace |
|---|---|
| `registrar(nombre, apellido, correo, clave)` | `POST /auth/v1/signup` con `data: {nombre, apellido}` (el trigger de `perfiles` los copia); sin confirmación de correo, la respuesta ya trae la sesión y se guarda (#236) |
| `iniciar_sesion(correo, clave)` | `POST /auth/v1/token?grant_type=password`; guarda la sesión en el llavero |
| `sesion_guardada()` | La sesión del llavero, **sin red** (para arrancar sin internet). Si está dañada, la borra y responde "sin sesión" |
| `sesion_vigente()` | La guardada; si le quedan menos de 60 s, la renueva antes |
| `renovar(sesion)` | `POST /auth/v1/token?grant_type=refresh_token` y guarda la nueva |
| `cerrar_sesion()` | `POST /auth/v1/logout` si hay red (si no, da igual) y la borra del llavero |
| `llamar(metodo, ruta, cuerpo)` | Llamada con la sesión a `/rest/v1/…` (tablas) o `/rest/v1/rpc/…` (funciones); devuelve el JSON (`null` si no hay cuerpo). Si responde 401, renueva y reintenta **una** vez |

**La sesión** (`Sesion`: token de acceso, token de renovación, vencimiento, id de la cuenta, correo, nombre y apellido) se guarda como JSON en el **llavero del sistema** (el trait `Llavero` de `servidor/pat.rs`, el mismo del PAT), en la cuenta `supabase-<nombre del proyecto>`: pruebas y producción no se pisan. Nunca va a SQLite ni a archivos, y `Debug` de `Sesion` oculta los dos tokens. Se guarda con `guardar_largo` (bytes UTF-8): el `guardar` normal de `keyring` usa UTF-16 en Windows (el doble de bytes) y el límite del Administrador de credenciales es de 2560 bytes; la sesión real mide ~1270 caracteres, así que en UTF-16 quedaba a 26 bytes del límite (comprobado en #236).

**Renovación**: si el servidor rechaza el token de renovación (vencido, revocado o ya usado), la sesión se borra y se pide iniciar sesión otra vez. **Sin conexión se conserva**: la app sigue trabajando con SQLite y los cambios esperan (#238). Un error 5xx tampoco la borra.

**Errores** (variantes nuevas en `error.rs`, con su código en `api.ts`):

| Caso | Error | Código |
|---|---|---|
| Sin sesión, o ya no se puede renovar | `SinSesionServidor` | `sin_sesion_servidor` |
| Sin internet o el proyecto pausado | `ServidorSinConexion` | `servidor_sin_conexion` |
| Respuesta inesperada del servidor | `Servidor(detalle)` | `servidor` |
| Contraseña incorrecta (`invalid_credentials`), correo sin confirmar (`email_not_confirmed`), demasiados intentos (429); al registrarse: correo ya usado (`user_already_exists`), contraseña corta (`weak_password`), correo no válido (`email_address_invalid`) | `Validacion` con el mensaje en español | `validacion` |
| Función o tabla sin permiso (`42501`) | `SinPermiso` | `sin_permiso` |
| Regla de negocio de nuestras funciones (`P0001`) | `Validacion` con el mensaje del servidor | `validacion` |

Respuestas reales comprobadas en branchboard-pruebas (2026-11-21): contraseña incorrecta → `400 {"error_code":"invalid_credentials"}`; token de renovación inválido → `400 {"error_code":"validation_failed"}`; token de acceso falso en la API de datos → `401 {"code":"PGRST301"}`; registro → `200` con la sesión y `user.user_metadata.{nombre,apellido}`; correo ya registrado → `422 user_already_exists`; contraseña de menos de 8 → `422 weak_password`.

**Pruebas reales** (en `app/backend`, contra branchboard-pruebas):

```
cargo test --no-default-features --lib supabase::cliente -- --ignored --nocapture --test-threads=1
```

No necesitan variables. Desde #236, las que necesitan una cuenta se **registran** solas con `ciclo-<hora>@prueba.branchboard` y una clave aleatoria que no se imprime (`registrar_y_volver_a_entrar`, `iniciar_llamar_renovar_y_cerrar` y `la_sesion_cabe_en_el_llavero_del_sistema`; esta última usa el llavero real de Windows en la cuenta aparte `supabase-branchboard-pruebas-test`). La API pública no puede borrar cuentas, así que **al terminar** se borran con la CLI v3 (sin Docker; `supabase/.temp/project-ref` debe decir `freppiixmuspvozaqrxj`):

```
npx --yes supabase@3 db query --remote --yes "delete from auth.users where email like 'ciclo-%@prueba.branchboard'"
```

y se comprueba que quedan 0 cuentas `ciclo-%`, 0 perfiles y 0 proyectos de prueba.

### Cuentas de BranchBoard (#236)

Las cuentas pasan de `lib/cuentas.ts` (cada PC) a **Supabase Auth**: la misma cuenta sirve en cualquier PC. Quien usa la app nunca ve Supabase; la app le pasa el correo y la contraseña al backend y este a Supabase.

Comandos (`comandos/cuenta.rs`, `Acceso::Libre` porque se usan antes de entrar a un proyecto); al frontend solo le llega `{nombre, apellido, correo}`, nunca los tokens:

| Comando | Qué hace |
|---|---|
| `registrar_cuenta(nombre, apellido, correo, clave)` | Crea la cuenta y deja la sesión iniciada |
| `iniciar_sesion_branchboard(correo, clave)` | Inicia sesión |
| `sesion_branchboard()` | La cuenta con sesión guardada en este equipo, **sin red**: al abrir la app se entra directo, también sin internet |
| `cerrar_sesion_branchboard()` | Avisa al servidor si hay red y borra la sesión del llavero |

**Sin confirmación de correo** (decidido el 2026-11-21 en #236; cambia la decisión 1 de #231 y el "código de 6 números"). Se comprobó en vivo que con el **correo incluido de Supabase** no se puede: (1) solo entrega a los miembros del equipo de la organización de Supabase (a los demás, "Email address not authorized"), con 2 correos por hora; y (2) en proyectos Free creados después del 3 de junio de 2026 no deja cambiar las plantillas: `config push` respondió `entitlement_required` ("Email template modification is not available for free tier projects using the default email provider"), así que el correo traería un enlace en inglés y no el código. El equipo prefirió no conectar un SMTP propio por ahora. **Riesgo conocido**: alguien puede registrarse con el correo de otra persona y, si un Administrador agrega ese correo a un proyecto, entrar en su lugar. Lo resolverá la recuperación de contraseña con código (fuera de alcance, otro issue), que necesitará un SMTP propio por lo mismo. **"¿Olvidaste tu contraseña?"** sigue solo visual. **Ojo con #237**: `login-unity` le cambia la contraseña a una cuenta **sin confirmar** con el mismo correo, pero sin confirmación Supabase marca todas las cuentas nuevas como confirmadas (`email_confirmed_at` al registrarse), así que esa protección hoy no se activa: quien se registró con un correo ajeno conserva su contraseña aunque el dueño entre con Unity.

### Login con Unity VC y primer Administrador (#237)

**Nuevo (2026-11-21)** — dos Edge Functions en `supabase/functions/` (Deno) que convierten una sesión de Unity VC en una sesión de Supabase. El servidor **no le cree a la app**: con el token de Unity que le manda, le pregunta a Unity VC de quién es.

| Función | Pedido | Qué hace |
|---|---|---|
| `login-unity` | `POST { token, org, servidor }` (sin sesión) | Pregunta a Unity VC el correo del token. Si ya hay una cuenta con ese correo (de correo y contraseña o de Unity) es **la misma** (decisión 1 de #231); si no, la crea con el correo ya confirmado (Unity lo verificó). Devuelve `{ access_token, refresh_token, expires_in, expires_at, usuario: { id, correo }, cuenta_nueva }`. |
| `vincular-repo-unity` | `POST { token, org, servidor, repo, nombre? }` con `Authorization: Bearer <sesión de Supabase>` | Comprueba que el token de Unity sea de la misma cuenta que la sesión y que esa persona sea el **dueño del repo** (`owner`). Si lo es, crea el proyecto con `unity_org`/`unity_repo` y la deja como **Administrador** (decisión 5 de #231). Si el repo ya está en BranchBoard: devuelve el mismo proyecto si ya es miembro; si no, `409` (a un proyecto existente se entra cuando un Administrador te agrega, #241). |

- `token` es el de la sesión de `cm` que deja el login por el navegador (`accessToken` de `sso_navegador.rs`, el mismo de `tokens.conf`, empieza por `TOKEN`) o un PAT. `servidor` es el `webserver` de la organización (`servidor/region.rs`, p. ej. `https://prd-azure-eastus-01-cloud.plasticscm.com:7178`); **solo se aceptan `https://*.plasticscm.com`**, así una app modificada no puede apuntar a un servidor propio que conteste lo que quiera.
- **Endpoints de Unity VC usados** (API REST del servidor, `Bearer <token>`; probados el 2026-11-21): `GET /api/v1/organizations/{org}/user` → `200 {"name": "<correo>"}` o `401` si el token no vale; `GET /api/v1/organizations/{org}/repos/{repo}` (repo con `/` codificado como `%2F`) → `{ name, owner, … }` o `404`. Están en el swagger público del servidor (`/swagger/v1/swagger.json`) pero no en la documentación de Unity: si cambian, hay que ajustar `supabase/functions/_shared/unity.ts`. Los de `www.plasticscm.com/api/oauth/…` y `services.unity.com` (login por el navegador) siguen siendo internos y sin documentar (aviso del PR #230).
- **Cuenta sin confirmar con el mismo correo**: se confirma y se le pone una **contraseña aleatoria**. Quien la registró pudo no ser el dueño del correo y no debe poder entrar con la contraseña que puso; el dueño real entra con Unity (y, cuando exista, con "¿Olvidaste tu contraseña?").
- **Cómo se abre la sesión**: la función genera un enlace mágico con la clave secreta (`auth.admin.generateLink`, no manda correo) y lo canjea ahí mismo (`verifyOtp`). La cuenta nueva queda con nombre y apellido vacíos (Unity no los da).
- **Base de datos** (migración `20261121010000_login_unity.sql`): `cuenta_por_correo(correo)` (id y si confirmó el correo) y `vincular_repo_unity(usuario, org, repo, nombre)`. **Solo las ejecuta `service_role`** (la clave secreta, que solo tienen las Edge Functions): con sesión normal o sin sesión dan `42501`. `verify_jwt = false` en `config.toml` porque las claves nuevas (`sb_publishable_…`) no son JWT: cada función revisa lo suyo.
- **Desplegar** (pruebas primero): `npx supabase@3 functions deploy login-unity vincular-repo-unity --use-api --remote=freppiixmuspvozaqrxj` (pide confirmar). `--use-api` empaqueta en el servidor, sin Docker.
- **Pruebas**: `npx deno@2 test supabase/functions/_shared/unity.test.ts` (9 casos, `fetch` simulado: servidores no permitidos, token rechazado, sin red, repo con `/`); `supabase/tests/02_login_unity.test.sql` (15 casos, igual que las reglas de #234). De punta a punta contra pruebas, con una cuenta real de Unity VC (2026-11-21): token inválido → 401; servidor ajeno → 400; login → sesión válida en Auth con el correo confirmado; segundo login → misma cuenta; cuenta sin confirmar → queda confirmada y con otra contraseña; vincular sin sesión → 401; repo inexistente → 404; la dueña vincula `BranchBoard-Pruebas` → Administradora, y vincular otra vez → mismo proyecto. Después se borraron la cuenta y el proyecto de prueba.
- **En la app** (`app/backend/src/supabase/login_unity.rs`): `iniciar_sesion_unity_vc` (login por el navegador, o la sesión de `cm` que ya había), al confirmar el correo con `cm whoami`, abre también la sesión del servidor. Si ya hay una guardada en el llavero para ese correo no toca la red. Si no, toma de `tokens.conf` la línea `server=<org>@cloud user=<correo> token=…` (prefiere la organización por nombre sobre el id numérico), saca el `webserver` de `cloudregions.conf` (`servidor::region::webserver`) y llama a `login-unity` (`ClienteSupabase::iniciar_sesion_con_unity`), que guarda la sesión en el llavero como la de correo y contraseña. **Si falla** (sin internet, servidor pausado, Unity rechaza el token) la sesión de Unity sigue valiendo: el resultado trae `aviso_servidor` con el motivo y lo que necesita el servidor espera. Errores de la función: `401` → `SesionUnityVcCaducada` (volver a iniciar sesión con Unity); `400/403/404/409` → `Validacion` con el mensaje de la función. **Token vencido (2026-11-21, #238)**: el token de `tokens.conf` dura ~1 h y `cm` solo lo renueva al hablar con el servidor, así que con una sesión de `cm` vieja Unity lo rechazaba aunque `cm whoami` funcionara. Ahora, si `login-unity` da `401`, se corre `cm repo list <org>@unity` (renueva el token; comprobado que cambia) y se reintenta una vez.
- `vincular-repo-unity` lo usará "Cargar proyecto" (#240).

### Bandeja de salida (#238)

**Nuevo (2026-11-21)** — los cambios de este equipo suben a la tabla `eventos` del servidor (`app/backend/src/eventos/bandeja.rs`).

- **Dónde esperan**: tabla local `bandeja_salida` de SQLite (migración 11): `id` (el mismo del evento), `proyecto_servidor`, `tipo`, `tarea_id`, `payload`, `creado_en`, `intentos`, `ultimo_error`. Sobrevive a cerrar la app.
- **Quién la llena**: `eventos::publicar`, en **la misma transacción** que el cambio y que `eventos_aplicados`: si el cambio se guarda, el evento queda en la bandeja; si falla, tampoco. `tareas.rs`, `sprints.rs`, `chat.rs`… no cambian.
- **Qué no entra**:
  - Proyectos sin `proyecto_servidor_id` en `configuracion_proyecto`: ese valor lo fija vincular el proyecto con el servidor (#240). Mientras no esté, no se encola nada.
  - Eventos de roles (`TIPOS_FIRMADOS`): en el servidor los roles los cambian solo sus funciones (#234/#241).
- **Por ahora se sigue escribiendo en `ops/`** además de la bandeja. Quitar `ops/` es de #244. Así la sincronización por Drive/OneDrive sigue funcionando hasta que exista la recepción desde el servidor (#239).
- **Subida**: **actualizado (#239)** — sube cada 5 s, en la misma vuelta que la recepción y justo antes de ella (`sincronizar_con_servidor_en`, ver [Recepción desde el servidor](#recepción-desde-el-servidor-239)). También al volver la ventana a primer plano y desde `sincronizar_eventos`.
  - Manda lotes de 50 en orden de llegada a `POST /rest/v1/eventos?on_conflict=id` con `Prefer: resolution=ignore-duplicates` (`ClienteSupabase::insertar_eventos`). Un evento que ya estaba (la respuesta se perdió y se reintenta) se ignora, sin error ni duplicado.
  - Al confirmarse, se borra de la bandeja.
  - Si un lote falla por la red o la sesión, se anota `intentos`/`ultimo_error` y se para ahí: los siguientes no se adelantan.
  - **Rechazo definitivo** (`SinPermiso` por la regla de #247, `Validacion` por una regla del servidor, o HTTP 400/403/409/422): reintentar no sirve. El lote se reenvía de a uno. El rechazado pasa a la tabla local `bandeja_rechazados` (migración 12, con `motivo` y `rechazado_en`) y los demás siguen subiendo. Pasa, por ejemplo, si a alguien le bajaron el rol mientras tenía cambios esperando. Ese cambio queda aplicado en el SQLite de ese equipo pero no en el servidor.
  - La base solo se presta para leer y borrar, nunca durante la red.
- **Qué manda la app**: `id, proyecto_id, tipo, tarea_id, payload, creado_en_app`. El servidor pone `autor` (la cuenta de la sesión), `orden` y `recibido_en`.
- **Interfaz**: comando `eventos_por_subir()` (libre; 0 sin proyecto abierto) y evento Tauri `bandeja-salida` (`{ pendientes, subidos, rechazados, error }`) después de cada intento. El Topbar muestra **"N cambios pendientes de enviar"** y/o **"N cambios rechazados por el servidor"** mientras haya alguno; al pasar el mouse dice el motivo.
- **Pruebas**:
  - 9 unitarias: sin vincular no encola; roles no; sin duplicar; orden y lotes; sin red esperan; un lote que falla frena; sobrevive a cerrar; un rechazado se aparta y los demás suben en orden; un rechazado solo no se reenvía; si se cae la red al reenviar de a uno, se para ahí.
  - Real, `eventos::bandeja::…::entorno_real_un_rechazado_por_el_servidor_no_frena_a_los_demas`: con las reglas de #247, un `cambiar_rol` metido a mano entre dos cambios (el servidor no lo acepta de nadie) se aparta con `SinPermiso` y los otros dos suben. Deja el proyecto "Prueba rechazados #238" y la cuenta: borrarlos después.
  - Real, `eventos::bandeja::…::entorno_real_sin_red_esperan_y_al_volver_se_suben`: contra branchboard-pruebas, entrando con la sesión de Unity VC del equipo (#237). Crea un proyecto en el servidor, vincula una base local, publica dos cambios, y sin red esperan (dirección que no resuelve, misma sesión). Al volver la red suben en orden con el autor de la sesión, y reenviar uno no duplica. Deja la cuenta y el proyecto "Prueba bandeja #238": borrarlos después con `delete from public.proyectos where nombre = 'Prueba bandeja #238'` y `delete from auth.users where lower(email) = '<correo>'`.

### Documentos e imagen en Storage (#242)

**Nuevo (2026-11-21)** — los archivos de documentos y la imagen del proyecto pasan de la carpeta compartida a **Supabase Storage** (migración `20261121042000_storage_documentos.sql`, módulo `app/backend/src/supabase/storage.rs`).

- **Bucket** `proyectos`, privado, **50 MB por archivo** (el máximo del plan gratis). Una carpeta por proyecto: `<proyecto_id>/documentos/D-<uuid>[.ext]` y `<proyecto_id>/proyecto/icono-<uuid>.png`. Los nombres los sigue generando el backend; `ruta_objeto` rechaza ids que no son uuid y nombres con `/`, `\`, `..` o caracteres raros.
- **Reglas** (las mismas que `identidad/permisos.rs`, detalle en [SEGURIDAD.md](./SEGURIDAD.md#reglas-del-servidor-supabase-234)): leer, cualquier miembro; documentos, los sube cualquier miembro y los borra Líder o Administrador; imagen, solo Administrador. Nada se sobrescribe.
- **Cuándo se usa**: si el proyecto tiene `proyecto_servidor_id` en `configuracion_proyecto` (lo fija #240). Si no, todo sigue como antes en la carpeta compartida (se quita en #244). Lo decide el comando (`comandos/documentos.rs::con_almacen`); `documentos.rs` trabaja con el trait `Almacen` (`CarpetaCompartida` o `AlmacenServidor`).
- **Más de 50 MB** se rechaza **antes de subir nada** (decisión de #242). Si se eligieron varios archivos y uno se pasa, no se sube ninguno. En la carpeta compartida no hay límite.
- **Subir**: primero se suben los archivos (sin bloquear la base: 50 MB pueden tardar), después se publica `subir_documentos`; si publicar falla, se borran los recién subidos. **Eliminar**: se publica `eliminar_documento` y después se borra el archivo de Storage (si eso falla, solo ocupa espacio).
- **Abrir**: el documento se baja la primera vez a `<carpeta del proyecto>/documentos-servidor/` (temporal y renombre) y después se abre desde ahí, también sin red. Sin red y sin copia: `servidor_sin_conexion`.
- **Imagen**: `subir_imagen_proyecto` la procesa igual (256×256 PNG), la sube, publica `icono_proyecto`, borra la anterior en Storage y guarda copia en `<carpeta del proyecto>/proyecto-servidor/`. `obtener_configuracion_proyecto` lee esa copia y la baja si falta; sin red se ve sin imagen.
- **Sesión**: la de `ClienteSupabase` (`sesion_vigente`); si Storage no reconoce el token, renueva y repite una vez.
- **Respuestas reales de Storage** (branchboard-pruebas, 2026-11-21). Casi todas llegan con **HTTP 400** y el código de verdad en el cuerpo, así que la app mira `code`:

| Caso | Respuesta | Error de la app |
|---|---|---|
| Subir sin permiso (regla RLS) | `{"statusCode":"403","error":"Unauthorized","message":"new row violates row-level security policy","code":"AccessDenied"}` | `SinPermiso` |
| Ruta que ya existe (`x-upsert: false`) | `{"statusCode":"409","error":"Duplicate","message":"The resource already exists","code":"KeyAlreadyExists"}` | `Validacion` |
| No existe **o las reglas no lo dejan ver** | `{"statusCode":"404","error":"not_found","message":"Object not found","code":"NoSuchKey"}` | `NoEncontrado` |
| Token con la firma dañada | `{"statusCode":"404","error":"Bucket not found","message":"Bucket not found","code":"NoSuchBucket"}` (**no** da 401) | `SinSesionServidor` → renovar y repetir |
| Borrar sin permiso | `200 []` (no borró nada) | `SinPermiso` (se revisa que la lista no venga vacía) |

- **Pruebas**:
  - 5 unitarias en `supabase::storage` y 2 en `documentos` con un Storage en memoria (subir/abrir/eliminar, copia local sin red, más de 50 MB sin subir nada).
  - SQL, `supabase/tests/03_storage.test.sql` (27 casos): `npx supabase@3 db query --remote --yes --file supabase/tests/03_storage.test.sql`; `fallidas` tiene que ser 0. Usa `set local storage.allow_delete_query = 'true'` porque Supabase no deja borrar filas de `storage.objects` por SQL.
  - Real, `supabase::storage::…::entorno_real_storage_documentos_e_imagen`, por la API HTTP: necesita las cuentas confirmadas `storage-a|b|c@prueba.branchboard` con la clave de `BB_PRUEBA_SUPABASE_CLAVE`. Se crean como la de [Cliente de Supabase](#cliente-de-supabase-235) (las tres en un `insert … select … from unnest(array['a','b','c'])`) y se borran al terminar: `delete from public.proyectos where nombre = 'bb-prueba-242'` y `delete from auth.users where email like 'storage-_@prueba.branchboard'`. La prueba borra sus archivos.
  - **Falta**: probarlo de punta a punta en la app con un proyecto vinculado (depende de #240).

### Proyecto pausado y copias de seguridad (#246)

**Nuevo (2026-11-21)** — el plan gratis de Supabase **pausa el proyecto tras una semana sin actividad en la base** y **no tiene copias de seguridad automáticas**. Los planes de pago no se pausan ([Project Pausing](https://supabase.com/docs/guides/platform/free-project-pausing)).

**Qué ve la app**:
- Con el proyecto pausado, Supabase responde **HTTP 540** a todo: Auth, datos y Edge Functions. Es un código propio de Supabase ([HTTP status codes](https://supabase.com/docs/guides/troubleshooting/http-status-codes)).
- El cliente (`supabase/cliente.rs`) lo convierte en `Error::ServidorPausado` (código `servidor_pausado`), con un mensaje que dice qué hacer.
- **No borra la sesión guardada**: pausado no es "sesión inválida", y sirve igual cuando lo reactiven.
- Los cambios esperan en la bandeja (#238). `EstadoBandeja.servidor_pausado` lo marca y el Topbar muestra **"Servidor pausado · N cambios pendientes de enviar"** (punto rojo), con la explicación al pasar el mouse.
- Al iniciar sesión con Unity VC, `aviso_servidor` trae el mismo mensaje.
- Storage (#242, `supabase/storage.rs`) también lo traduce a `ServidorPausado`.
- Probado con un servidor HTTP local que responde 540 (`con_el_proyecto_pausado_se_avisa_y_la_sesion_no_se_pierde`): `llamar`, `renovar`, `iniciar_sesion`, `iniciar_sesion_con_unity` e `insertar_eventos`. Storage, con `con_el_proyecto_pausado_storage_avisa_igual`. No se probó pausando el proyecto de pruebas, porque lo usan otras pruebas al mismo tiempo.

**Reactivarlo** (solo el dueño de la cuenta de Supabase):
1. Entrar a [supabase.com/dashboard](https://supabase.com/dashboard), organización BranchBoard, y abrir el proyecto (`branchboard` o `branchboard-pruebas`). Un proyecto pausado lo dice en su página.
2. **Resume project** y confirmar. Vuelve con sus datos y su configuración en unos minutos.
3. Las apps abiertas no hay que tocarlas: en la siguiente vuelta con el servidor (cada 5 s, #239) suben lo pendiente y el aviso desaparece.
- Supabase deja un plazo para reactivarlo desde el panel; su documentación da 90 días en una página y 1 año en otra, así que hay que mirar el que muestra el panel. Pasado ese plazo, los datos solo se recuperan si se descargaron antes.
- Si se queda trabado en "Pausing" o "Restoring", la vía documentada es el soporte de Supabase.

**Mientras no haya copia automática** (pospuesta el 2026-11-20, decisiones 8 y 9 de #231):
- El respaldo es lo que queda en cada equipo: el SQLite de cada proyecto y sus `backups/` locales.
- En proyectos sin Unity VC, también "Exportar proyecto" de Ajustes (`exportar_proyecto_local`, Administrador).
- Si el proyecto del servidor se perdiera, se vuelve a subir desde la copia de un equipo con "Subir al servidor los proyectos que ya existen" (#245, pendiente).
- Para que no se pause: cualquier app abierta que lea o escriba en el servidor cuenta como actividad. Hoy se escribe con la bandeja (#238) y se leerá cada pocos segundos con la recepción (#239). Un proyecto que nadie abre en una semana se pausa.

**Cuándo pasar a Pro** (de pago): cuando el equipo use BranchBoard a diario y una pausa o la falta de copias diarias ya no sea aceptable. También cuando se acerquen los límites del plan gratis (tamaño de la base, Storage, #242) o cuando haga falta restaurar a un punto en el tiempo. Pro no se pausa e incluye copias diarias. Con Pro, la copia propia con `pg_dump` deja de hacer falta.

### Recepción desde el servidor (#239)

**Refresco de la pantalla sin Unity VC (2026-11-21)**: `App` monta `useEventosServidor(accesoCompletado && !!proyectoActual)` por separado de la sincronización de Unity. Antes la escucha de `eventos-recibidos` estaba detrás de `cmSesionActiva`: los datos podían llegar a SQLite sin actualizar la pantalla en un proyecto sin Unity. Ahora un cambio aplicado incrementa `versionDatos` y las pantallas recargan desde el backend; al desmontar se retira la escucha, también si su registro termina tarde en React StrictMode. La recepción y la escritura de eventos no cambian.

**Prueba reproducible de interfaz**: `examples/probar_refresco_239.rs` crea dos cuentas reales y un proyecto en `branchboard-pruebas`, con sesiones solo en memoria y bases locales separadas. Su recepción utiliza los mismos módulos `bandeja`/`recepcion` y el evento Tauri de la app, con cadencia de 5 s; no usa el llavero del usuario. La WebView carga la `App` y lista de tareas reales. `scripts/probar-refresco-servidor.mjs green` comprueba creación y edición desde la otra cuenta visibles sin recargar, `versionDatos` sin suscripciones duplicadas y dos vueltas adicionales sin tareas duplicadas. No llama a una recarga o sincronización manual de la pantalla durante esas comprobaciones. Los datos y las cuentas de prueba se conservan identificados; las sesiones se cierran al salir del auxiliar.

Para ejecutarla: Vite en `127.0.0.7:4300`, `BB239_DATOS` apuntando a una carpeta nueva de pruebas, un perfil WebView propio y `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9457`; ejecutar `cargo run --example probar_refresco_239 --locked` y, desde el frontend, `node scripts/probar-refresco-servidor.mjs green`. Cerrar el auxiliar con el modo `close` del mismo script.

**Nuevo (2026-11-21)** — los cambios del resto del equipo bajan de la tabla `eventos` del servidor y se aplican con el mismo código que los de `ops/` (`app/backend/src/eventos/recepcion.rs`). Solo en proyectos con `proyecto_servidor_id` (lo fija #240).

- **Cuándo** (decisión del 2026-11-20 en #239: consulta periódica, Realtime después si hace falta): cada **5 s** mientras la app está abierta (un hilo aparte en `lib.rs`), al volver la ventana a primer plano y cuando el frontend llama a `sincronizar_eventos` (al entrar al proyecto y cada 2 minutos). Sin internet, sin sesión o con el proyecto pausado, la vuelta simplemente no baja nada y se reintenta en 5 s; como cada vuelta pide "lo nuevo desde el último", al volver la conexión llega todo lo que faltaba.
- **Una vuelta** (`comandos::eventos::sincronizar_con_servidor_en`), con la guardia de sincronización (`AppState::iniciar_sincronizacion`, la misma de `ops/`):
  1. Sube la bandeja de salida (#238).
  2. Baja lo nuevo: `GET /rest/v1/eventos?select=orden,id,tipo,tarea_id,autor,payload,recibido_en&proyecto_id=eq.<id>&orden=gt.<último>&order=orden.asc&limit=500`, por páginas de 500 hasta que no haya más.
  3. Pide el correo de los autores nuevos: `GET /rest/v1/perfiles?select=id,correo&id=in.(…)`.
  4. Lo guarda en la tabla local **`eventos_servidor`** (migración 13 de SQLite: `orden`, `id`, `proyecto_servidor`, `tipo`, `tarea_id`, `usuario` = correo, `payload`, `recibido_en`). El último `orden` guardado es desde dónde se pide la próxima vez.
  5. Aplica, en el `orden` del servidor, lo de `eventos_servidor` que no está en `eventos_aplicados`, con `eventos::lectura::aplicar_evento` (el mismo aplicador de `ops/`). Si aplicó algo, emite el evento Tauri **`eventos-recibidos`** (`{ aplicados, descartados, desconocidos, ilegibles }`) y el frontend sube `versionDatos`.
- **Subir antes de bajar, en la misma vuelta**: así una toma propia está o en la bandeja o ya en el servidor, nunca "subida pero sin bajar", y la doble toma decide igual en todas las PCs (abajo).
- **Primera vez en una PC**: el último `orden` es 0, así que baja todo el historial del proyecto y arma el SQLite local.
- **Orden y desempate por el servidor**: se aplica por `orden` y la hora del evento (`timestamp` al aplicarlo) es `recibido_en`, la del servidor. Esto resuelve el riesgo aceptado del reloj local.
- **Los propios no se aplican dos veces**: el id es el mismo en `ops/`, la bandeja y el servidor, y lo que ya está en `eventos_aplicados` se salta. Por la misma razón, mientras siga `ops/` (hasta #244) un evento que llegue por las dos vías se aplica una sola vez.
- **Doble toma** (`tareas::detectar_doble_toma` con `FuenteToma::Servidor`): gana la toma con `orden` menor. Si la toma rival es de esta PC y sigue en la bandeja, llegará al servidor después, así que gana la que bajó. Si no hay toma rival (la asignó un Líder), pierde la que bajó, igual que con `ops/`.
- **Autor sin perfil visible** (alguien que ya no comparte ningún proyecto con quien baja): queda como `<uuid>@sin-perfil`.
- **Tipo desconocido** (lo escribió una versión más nueva de la app): queda pendiente en `eventos_servidor` y se reintenta en cada vuelta, sin frenar a los demás (igual que con `ops/`).
- **Red y base separadas**: la base solo se presta para leer y guardar, nunca durante la red.
- **Pruebas**:
  - 7 unitarias en `eventos::recepcion` (historial por páginas, correo del autor y sin perfil, orden y hora del servidor y saltar los propios, tipo desconocido, sin conexión, proyectos aparte, rutas) y 2 de doble toma en `tareas` (gana la que llegó antes al servidor aunque el reloj diga otra cosa; la propia aún en la bandeja pierde).
  - Real, `eventos::recepcion::…::entorno_real_dos_pcs_reciben_lo_del_otro` (`cargo test --lib entorno_real_dos_pcs -- --ignored --nocapture`): contra branchboard-pruebas con la sesión de Unity VC del equipo y **dos bases locales como dos PCs**. La PC A crea dos tareas y las sube; la PC B, nueva, baja todo el historial y las aplica con el creador correcto; A crea una tercera y B baja solo esa; A no aplica dos veces lo suyo. Deja el proyecto "Prueba recepción #239": borrarlo con `delete from public.proyectos where nombre = 'Prueba recepción #239'` (los eventos se borran en cascada).

### Usuarios y roles por el servidor (#241)

Con el proyecto en el servidor (tiene `proyecto_servidor_id` en `configuracion_proyecto`, lo fija #240), la pantalla Usuarios y los comandos `listar_usuarios`, `crear_usuario`, `cambiar_rol`, `confirmar_usuario` y `eliminar_usuario` (`comandos/usuarios.rs`) van por `identidad/equipo_servidor.rs` y `supabase/miembros.rs`, **sin firmas**. Sin `proyecto_servidor_id` (proyectos por Drive/OneDrive) siguen el camino de antes, con eventos firmados por `ops/` (hasta #243/#244).

| Comando | Con el servidor |
|---|---|
| `listar_usuarios` | Lee `miembros` (con su perfil) e `invitaciones`, rehace la copia local y lista. Sin red lista la última copia. Las invitaciones (llevan correos) solo las ven Líder y Administrador |
| `crear_usuario(email, rol)` | `rpc/agregar_miembro`: si la persona tiene cuenta entra de una vez; si no, queda **invitada** y entra sola al registrarse (o entrar con Unity VC) con ese correo |
| `cambiar_rol(email, rol)` | `rpc/cambiar_rol`. El Administrador **puede nombrar otros Administradores**; el servidor nunca deja degradar al último. A un invitado no se le cambia el rol: se quita la invitación y se vuelve a invitar |
| `eliminar_usuario(email)` | `rpc/quitar_miembro` (pierde el acceso de inmediato, también a los documentos de Storage, #242) o `rpc/quitar_invitacion` |
| `confirmar_usuario` | Ya no existe: responde `validacion` ("ya no hace falta confirmar usuarios") |

- **Copia local**: `usuarios` queda igual a `miembros` (`db::usuarios::reemplazar_todos`, todo o nada) después de cada cambio y al listar, sin firmas. **Borra las filas locales que no están en `miembros`**: si un proyecto que ya tenía equipo local (por Drive) queda vinculado al servidor sin haber subido antes ese equipo, esa PC pierde de su copia a quienes falten en el servidor (siguen en `ops/` y en las otras PCs). Por eso esos proyectos se suben con #245 antes de vincularse. Los roles de la copia valen tal cual (`permisos::usuario_actual`, `tareas::rol_de`): los hace cumplir el servidor (#234, #247). Desde #243 no hay firmas ni llave del Administrador.
- **Permiso en la app**: si la copia local todavía no tiene a quien pide (primera vez en esa PC, o el rol cambió en el servidor), se rehace una vez y se vuelve a revisar. El servidor igual revisa todo.
- **Invitaciones** (migración `20261121061500_invitaciones.sql`): tabla `invitaciones` (proyecto, correo, rol, quién invitó), que leen solo Líder y Administrador del proyecto. `agregar_miembro` ahora invita en vez de fallar con "todavía no tiene cuenta" (devuelve `null`); invitar dos veces el mismo correo da "Esa persona ya está invitada al proyecto". `quitar_invitacion` la cancela quien podría haberla hecho con ese rol. El trigger `tras_confirmar_aceptar_invitaciones` (sobre `auth.users`, después de crear el perfil) convierte las invitaciones de ese correo en membresías cuando la cuenta queda confirmada. **Desde #236 eso pasa al registrarse** (no se confirma el correo): ver el riesgo en [SEGURIDAD.md](./SEGURIDAD.md#reglas-del-servidor-supabase-234).
- **Frontend**: la pantalla Usuarios del frontend todavía **no llama** a estos comandos (también antes: `crearUsuario`, `cambiarRol` y `eliminarUsuario` del store solo cambian datos en memoria). En #241 el mockup pasó al modelo nuevo (invitar, Administrador asignable, sin "Confirmar"); conectarla al backend queda para cuando los proyectos estén en el servidor (#240).
- **Pruebas**:
  - 8 unitarias sin red en `supabase::miembros` y `identidad::equipo_servidor` (servidor en memoria con las reglas de `puede_asignar` y del último Administrador; sin red; la copia local vale con la llave vacía y no con una llave de Drive).
  - SQL: `supabase/tests/04_invitaciones.test.sql` (igual que las reglas de #234, se deshace al final) y el caso de `01_reglas` que antes esperaba "sin cuenta" y ahora espera la invitación.
  - Real, `identidad::equipo_servidor::…::entorno_real_usuarios_por_el_servidor` (`cargo test --no-default-features --lib entorno_real_usuarios_por_el_servidor -- --ignored --nocapture`): registra sus cuentas `usuarios-a|b|invitado-<hora>@prueba.branchboard` y crea `bb-prueba-241-<hora>`. A agrega a B como Desarrollador e invita a un correo sin cuenta; **el servidor le rechaza a B** cambiar roles, darse Administrador y agregar (`sin_permiso`) y B no ve invitaciones; el invitado se registra y entra solo como Tester; A nombra Administrador a B y se degrada a Líder; B, último Administrador, no se puede degradar; B quita al invitado y a A, y A deja de ver el proyecto. La API pública no borra cuentas ni proyectos; al terminar:

    ```
    npx --yes supabase@3 db query --remote --yes "delete from public.proyectos where nombre like 'bb-prueba-241%'; delete from auth.users where email like 'usuarios-%@prueba.branchboard'"
    ```

    y se comprueba que quedan 0 cuentas `usuarios-%`, 0 perfiles, 0 proyectos `bb-prueba-241%` y 0 invitaciones.

### Proyectos en el servidor (#240)

**Nuevo (2026-11-21), primera parte** — los proyectos de Unity VC quedan **vinculados** con su fila de `proyectos` del servidor (`app/backend/src/supabase/proyectos.rs`). El id se guarda en `configuracion_proyecto` como `proyecto_servidor_id`: es lo que activa la bandeja (#238), la recepción (#239), Storage (#242) y los usuarios por el servidor (#241).

- **Cuándo**: al entrar a un proyecto de Unity VC (`comandos/acceso.rs::entrar_con`, después de abrir la base), sin frenar la entrada. Sin internet o sin sesión no se vincula y se reintenta la próxima vez que se entre. También con el comando `vincular_proyecto_servidor()` (todos los roles, proyecto abierto), que devuelve `{ estado: "vinculado", proyecto_id, ahora } | { estado: "no_es_miembro" } | { estado: "con_equipo_local" }`.
- **Cómo** (`proyectos::vincular_unity`):
  1. Si ya tiene `proyecto_servidor_id`, no toca la red.
  2. Si la base local tiene **más de un usuario**, no se vincula (`con_equipo_local`). Con #241, la tabla local `usuarios` pasa a ser copia de `miembros` y se reemplaza entera, así que quien no esté en el servidor desaparecería de esa PC. Esos proyectos se suben con #245.
  3. Busca el proyecto: `GET /rest/v1/proyectos?select=id&unity_org=eq.<org>&unity_repo=eq.<Proyecto%2FRepo>`. Las reglas RLS solo lo muestran si la cuenta es miembro.
  4. Si no aparece y quien entra es **Administrador local**, lo trae con la Edge Function `vincular-repo-unity` (#237) y el token de Unity de `tokens.conf`: ella comprueba que sea el **dueño del repo**, crea el proyecto y lo deja como Administrador. Si el repo ya lo había traído otra persona, devuelve el mismo proyecto (si ya es miembro) o `409`: **un repo es un solo proyecto**. Si Unity rechaza el token (venció), se renueva con `cm repo list <org>@unity` y se reintenta una vez.
  5. Si no aparece y no es Administrador: `no_es_miembro` (lo agrega un Administrador, #241).
- **Listar**: `listar_proyectos_del_servidor()` (libre; hace falta la sesión del servidor) → `[{ id, nombre, unity_org, unity_repo, rol }]`, con `GET /rest/v1/miembros?select=rol,proyectos(id,nombre,unity_org,unity_repo)&usuario_id=eq.<cuenta>`. `listar_proyectos_servidor()` ("Cargar proyecto") lo usa para marcar como "Con tareas" los repos que ya son proyectos de la cuenta, con su `rol_servidor`. Sin internet, la lista sale igual que antes.
- **Cliente**: `ClienteSupabase::llamar_funcion(nombre, cuerpo)` hace `POST /functions/v1/<nombre>` con la sesión.
- **Entrar con el servidor** (`identidad::acceso::entrar_con_servidor`, segunda parte, 2026-11-21):
  - **Base local vinculada**: antes de decidir se rehace la copia de `miembros` (#241; sin red se usa la última) y se decide **sin firmas**, con la llave vacía, igual que `estado::llave_publica_del_proyecto`. Antes `entrar` leía `admin.pub` del disco y, con la copia sin firmas que deja #241, hasta el Administrador quedaba "pendiente".
  - **Primera vez en una PC, sin ser el dueño**: si no hay datos locales usables y el servidor muestra el proyecto (la cuenta es miembro), se crea la base local con `proyecto_servidor_id` y el nombre del proyecto, se trae el equipo y se entra con el rol del servidor. No hay llaves del Administrador en esa PC. El historial lo baja la recepción (#239).
  - Si el servidor no lo muestra (o no hay red): como antes, el dueño configura y los demás ven "espera a que lo abra su dueño".
  - Igual que hoy, después de entrar el frontend pide la carpeta compartida hasta #244.
- **Proyectos sin Unity VC en el servidor** (`proyecto_servidor.rs`, `comandos/proyecto_servidor.rs`, tercera parte, 2026-11-21). Nada va por Drive: el proyecto vive en el servidor y se abre desde cualquier PC con la cuenta.
  - `crear_proyecto_servidor(nombre)` (libre, #269): usa la sesión **guardada**, incluso con token vencido y sin red. Genera el UUID v4 definitivo en la app, crea SQLite y la copia del creador como Administrador, y persiste el alta pendiente en la misma transacción. Abre el proyecto inmediatamente (`configurado_ahora: true`), sin carpeta que elegir ni contraseña. No guarda tokens en SQLite.
  - Antes de subir cualquier evento, la sincronización automática confirma el alta con `rpc/crear_proyecto(p_nombre, p_id)`. Conserva el UUID de todas las tareas y eventos creados sin conexión. Si falla, no envía ni aparta eventos como rechazados: todo queda pendiente incluso al reiniciar. La petición de red no mantiene bloqueada la base local. Solo la cuenta que lo creó puede subirlo; cambiar de cuenta no transfiere su autoría.
  - El RPC conserva la versión anterior de un argumento. La versión con UUID no sobrescribe proyectos ni otorga roles por colisión; permite reintentar únicamente el mismo nombre, creador y Administrador de un proyecto sin Unity VC, para recuperar una respuesta perdida sin duplicar. El alta cuenta como un cambio pendiente en el indicador, incluso antes de crear tareas.
  - `listar_proyectos_del_servidor` combina la lista remota con los proyectos creados **por esta cuenta en esta PC**, sin duplicar UUID. Si falla la red, siguen disponibles para abrirlos y trabajar, incluso si el alta ya se confirmó y todavía quedan tareas por subir. Conserva solo los metadatos del origen y la copia del rol; una PC distinta necesita que el alta ya haya llegado al servidor. Al abrir un alta pendiente no consulta la red; una confirmada usa el refresco habitual del equipo y su copia si no hay conexión.
  - `abrir_proyecto_servidor(id)` (libre): uno de los de `listar_proyectos_del_servidor` sin `unity_repo`. Se sabe quién es con la sesión guardada (también sin red). El nombre viene del servidor si hay red.
  - **Datos locales**: `<datos de la app>/proyectos/servidor-<id>/`, con `proyecto_servidor_id`, el nombre y la copia del equipo (`equipo_servidor::refrescar`, #241). Entra con su rol de la copia, sin firmas; si no está, `no_registrado`. Una PC nueva sin red no puede abrirlo (no sabe el rol); una que ya lo tenía, abre con la última copia.
  - **Eventos**: hasta que #244 quite `ops/`, `publicar` escribe también en `eventos-locales/`, dentro de los datos del proyecto. Es local y privada, no se sincroniza con nada (`eventos::fijar_carpeta_compartida_local`). Lo que viaja va por la bandeja (#238) y la recepción (#239).
  - Los proyectos "por carpeta" de antes (`proyecto_local`) se siguen abriendo como estaban; subirlos al servidor es de #245.
- Crear sin internet está implementado en #269; la sesión de BranchBoard debe haberse iniciado antes en esta PC.
- **Falta (siguientes partes de #240)**:
  - Usar un proyecto vinculado sin carpeta compartida: hoy `publicar` escribe en `ops/` y la entrada pide la carpeta; eso se quita en #244.
- **Pruebas**:
  - 7 unitarias en `supabase::proyectos`: si el servidor lo muestra se guarda sin traerlo; el Administrador lo trae; quien no lo es, no; ya vinculado no toca la red; sin internet no guarda nada; con equipo local no se vincula; rutas y respuestas.
  - Real, `comandos::acceso::…::entorno_real_vincular_repo_y_listar` (`BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas cargo test --lib entorno_real_vincular -- --ignored --nocapture`), contra branchboard-pruebas con la sesión de Unity VC de la dueña del repo:
    - la PC A trae el repo con la Edge Function;
    - la PC B lo encuentra y guarda el **mismo** id sin traerlo otra vez;
    - volver a vincular A no toca la red;
    - la lista de la cuenta lo muestra como Administrador.
    - Deja el proyecto: borrarlo con `delete from public.proyectos where unity_repo = 'BranchBoard-Pruebas/BranchBoard-Pruebas'`.
  - 4 unitarias en `identidad::acceso`: un miembro entra por primera vez desde el servidor (base vinculada, sin llaves); en el servidor pero sin ser miembro → no registrado; sin servidor y sin ser dueño → espera; vinculado, el dueño entra aunque la copia no tenga firmas (y sin red).
  - Real, `comandos::acceso::…::entorno_real_miembro_entra_por_primera_vez_en_una_pc` (mismas variables, `cargo test --lib entorno_real_miembro_entra -- --ignored --nocapture`):
    - la dueña trae el repo;
    - registra una cuenta `entrar-240-<hora>@prueba.branchboard` (clave aleatoria) y la agrega como Desarrolladora;
    - esa cuenta entra en una PC sin datos como Desarrolladora y queda la base vinculada con los 2 miembros; al entrar otra vez usa esa base;
    - la dueña entra en otra PC sin datos como Administradora.
    - Borrar después: `delete from public.proyectos where unity_repo = 'BranchBoard-Pruebas/BranchBoard-Pruebas'` y `delete from auth.users where email like 'entrar-240-%@prueba.branchboard'`.
  - 3 unitarias en `proyecto_servidor`: prepara una base vinculada con su equipo y la carpeta local de eventos, y decide el rol; una PC nueva sin red no puede saber el rol; sin red se abre con la última copia.
  - Real, `comandos::proyecto_servidor::…::entorno_real_sin_unity_entre_dos_pcs` (`cargo test --lib entorno_real_sin_unity -- --ignored --nocapture`, no necesita Unity VC). **Proyecto sin Unity VC sincronizado entre dos PCs con dos cuentas**: registra `sin-unity-a|b-<hora>@prueba.branchboard` (clave aleatoria, no se imprime).
    - A crea el proyecto (Administradora) y agrega a B como Desarrollador.
    - A crea una tarea y la sube.
    - B, en una PC sin datos, abre el proyecto desde el servidor como Desarrollador, baja el historial y ve la tarea de A.
    - B crea otra tarea y la sube; A la baja y ve las dos.
    - Borrar después: `delete from public.proyectos where nombre like 'bb-prueba-240-%'` y `delete from auth.users where email like 'sin-unity-%@prueba.branchboard'`.

### Subir al servidor los proyectos que ya existen (#245)

**Nuevo (2026-11-21)** — "Subir este proyecto al servidor" (Ajustes, solo Administrador) pasa a Supabase un proyecto de antes (de Unity VC o por carpeta) sin perder sus datos (`comandos/subir_proyecto.rs`, `eventos/importacion.rs`).

- **Decisión del 2026-11-21 en #245: una foto del estado, no el historial.** El historial vive solo en `ops/` (Drive, se quita en #244). El servidor pone como autor de cada evento a quien lo sube (#234), así que subirlo uno por uno dejaría todo a nombre del Administrador, y los proyectos compactados no lo tienen completo. Se manda **un solo evento `importar_proyecto`** con las filas de las tablas compartidas de la base local, tal cual: los autores originales van dentro.
- **Qué va en la foto** (`importacion::TABLAS`): `configuracion_proyecto` (sin `proyecto_servidor_id` ni `subida_pendiente_id`), `sprints`, `tareas`, `ediciones_pendientes`, `reportes_sprint`, `commits`, `cambios_prestados`, `cambios_directos`, `documentos`, `notificaciones` (llegan **leídas**) y `grupos_chat` / `mensajes_chat`. No van las tablas de cada PC (lecturas del chat, paneles vistos, bandeja, copia del servidor, `eventos_aplicados`, changesets revisados) ni `usuarios`: el equipo se sube aparte.
- **Al aplicarla** (`importacion::aplicar`, desde `lectura::aplicar_evento`): solo en un proyecto **vacío** (sin tareas, sprints, mensajes ni documentos); si no, se descarta. Copia por nombre de columna. Una columna, tabla o formato que esta versión no conoce deja el evento **pendiente** hasta actualizar la app.
- **Servidor**: `importar_proyecto` es **solo de Administrador** en `privado.permisos_evento` (migración `20261122090000_importar_proyecto.sql`, regla de #247). Lo prueban dos casos en `01_reglas.test.sql`.
- **Comando `subir_proyecto_al_servidor()`** (solo Administrador; la sesión de BranchBoard tiene que ser la de ese Administrador) → `{ proyecto_id, miembros, invitados, filas, documentos, documentos_sin_archivo }`:
  1. **Crea el proyecto**: de Unity VC con `vincular-repo-unity` (revisa que sea el dueño del repo); por carpeta con `rpc/crear_proyecto(nombre)`.
  2. **Gana el primero** (decisión del 2026-11-20 en #245): si el repo ya lo trajo otra persona (`409`) o el proyecto del servidor ya tiene eventos, no se mezcla nada: "Este proyecto ya está en el servidor (lo subió otra persona). Pídele acceso a uno de sus Administradores.".
  3. **Equipo**: cada usuario con rol válido en la base (verificado como hasta ahora) se agrega con su rol (`rpc/agregar_miembro`, #241). Quien no tiene cuenta queda **invitado**.
  4. **Archivos**: los documentos y la imagen del proyecto se copian de la carpeta compartida de esa PC a Storage (#242) con el mismo nombre interno. Si un documento no está en esa PC, queda en la lista sin archivo (`documentos_sin_archivo`).
  5. **Foto y vínculo**: manda `importar_proyecto`, guarda `proyecto_servidor_id` y marca la foto como aplicada en esa base. Desde ahí, bandeja (#238) y recepción (#239). Los demás la reciben al abrir el proyecto desde el servidor (#240).
  - Si algo falla a medias se puede repetir: el id del proyecto creado queda en `subida_pendiente_id` y se reutiliza; quien ya está en el equipo o ya fue invitado no se agrega otra vez; un archivo que ya está en Storage no es error.
- **Comando `proyecto_en_el_servidor()`** (todos los roles): el id del proyecto abierto en el servidor o `null`. Lo usa Ajustes para mostrar la sección.
- **Límite**: la foto va en un solo pedido. Un proyecto muy grande (mucho chat o muchas tareas) podría superar el tamaño que acepta el servidor; si pasa, la subida da error y se puede repetir.
- **Pruebas**:
  - 3 unitarias en `eventos::importacion`: se aplica en un proyecto vacío con los autores originales y las notificaciones leídas; con datos se descarta; columna, formato o tabla desconocidos quedan pendientes.
  - SQL: `01_reglas.test.sql` (65 casos, `fallidas` 0): el Administrador manda `importar_proyecto` y el Líder no.
  - Real, `comandos::subir_proyecto::…::entorno_real_subir_proyecto_por_carpeta` (`cargo test --lib entorno_real_subir_proyecto_por_carpeta -- --ignored --nocapture`). Es un **proyecto de prueba** por carpeta, con llaves y filas firmadas como los de antes, y cuentas `subir-245-a|b|c-<hora>@prueba.branchboard`:
    - A lo sube: B queda miembro, C invitada (no tiene cuenta) y el documento en Storage.
    - B, en una PC sin datos, lo abre desde el servidor y recibe la foto: ve las dos tareas **con sus autores originales** y el documento, que baja de Storage.
    - Subirlo otra vez dice que ya está en el servidor.
  - Real, `…::entorno_real_gana_el_primero` (`BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas cargo test --lib entorno_real_gana_el_primero -- --ignored --nocapture`): el mismo repo de Unity VC en dos PCs de su dueña, con datos distintos. La primera lo sube; la segunda recibe "ya está en el servidor" y no queda vinculada; en el servidor queda **solo** la foto de la primera.
  - Borrar después: `begin; set local storage.allow_delete_query = 'true'; delete from storage.objects where bucket_id = 'proyectos' and name like '<id>/%'; delete from public.proyectos where id in (…); delete from auth.users where email like 'subir-245-%@prueba.branchboard'; delete from public.invitaciones where lower(correo) like 'subir-245-%'; commit;`.

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

**Implementado (2026-11-10, issue #80)** en `app/backend/src/cm/ejecutor.rs`: **todo** subproceso pasa por `Comando` (`Comando::cm().arg("log").arg(format!("br:/{rama}")).ejecutar(&estado)`), que recibe los argumentos ya separados — no existe ninguna función que acepte un comando como texto. `ejecutar` devuelve `{ codigo, stdout, stderr }` también cuando el código no es 0; `exigir_exito` pide código 0 y devuelve el `stdout`, o un error `cm` con lo que dijo el proceso. Tiempo máximo por comando (2 min por defecto, ajustable): si se pasa, el proceso se mata y se devuelve error `cm`. Si `cm` no está instalado, error `cm_no_instalado`. En Windows el proceso corre sin abrir una consola. Prueba incluida: un título con `; rm -rf … && echo hola` llega al proceso como texto literal, en una sola línea.

## Protección de ramas (`main` y `dev`)

Esta regla es **solo una capa de código en BranchBoard** — no la puede hacer cumplir Unity Version Control por sí solo (no existe un permiso nativo de VCS para "límite de niveles de anidación" ni reglas de negocio como estas). Si alguien usa el cliente de Unity VC directamente, sin pasar por la app, estas protecciones no aplican — es un acuerdo de equipo además de una validación técnica.

**Probado (2026-11-12)**: tampoco se puede reforzar desde la API REST. En Unity Cloud los permisos de Version Control van por **rol del proyecto** (grupos automáticos `<id-proyecto>-admin`, `-rw`, `-ro`), no por persona; leerlos por la API funciona (`POST /permissions?groupName=…`), pero `PATCH /permissions` para negar `ci` en `/main/dev` responde **200 sin aplicar nada** (verificado con `cm showacl` y con un checkin directo que pasó), y `cm acl --user=…` falla. **Decidido (2026-11-17, #204)**: el Dashboard de Unity Cloud tampoco sirve: no tiene permisos por rama y su apartado de permisos de Version Control no carga (probado varias veces). Los permisos de BranchBoard y los de UVCS son independientes: BranchBoard no puede impedir un check-in directo en `/main/dev` o `/main` hecho con Plastic o `cm` fuera de la app; lo bloquea dentro de ella (`validar_destino_checkin` en `ramas.rs`, antes de todo `cm checkin`) y lo detecta: en el servidor, con la [tarea roja](#tarea-roja-por-check-ins-directos-en-main-o-dev-issue-225) (#225), y en el equipo de quien tiene BranchBoard abierto, con la [alerta de rama ajena](#detección-de-ediciones-en-rama-ajena-fuera-de-la-app). No se ofrece guía de protección en la configuración inicial del Administrador.

- **Límite de profundidad, contado como cadena de ancestros**: antes de crear cualquier rama se cuenta la cadena desde `main`. **Implementado (2026-11-14, #83)**: en Unity VC la ruta completa de una rama ya es su cadena de ancestros (`/main/dev/T-1-a/T-1-1-b` = 4 niveles), así que se cuentan sus segmentos sin pedirle nada al servidor. Cadena máxima permitida: **5** — `main → dev → tarea general → subtarea → subtarea de subtarea`. Si crear la nueva rama haría que la cadena llegue a 6, la creación **se rechaza antes de llamar a `cm`**, con un mensaje explicando el límite.
- **Ningún comando de checkin puede apuntar a `main`**: cualquier función que ejecute `cm checkin` valida primero que la rama destino no sea literalmente `main` — aplica para **todos los roles, incluido el Administrador**. Si se detecta, se rechaza antes de invocar `cm`. La única excepción es la reversión de "Trasladar" en una tarea roja (#225, ver abajo).
- **Ningún comando de checkin puede apuntar a `dev` directamente**: mismo tipo de validación — todo cambio a `dev` debe entrar por el flujo de merge ya definido en [Flujo de revisión y merge a dev](#flujo-de-revisión-y-merge-a-dev), nunca por un checkin directo, sin importar el rol. **Única excepción (#225)**: la reversión que hace "Trasladar" en una [tarea roja](#tarea-roja-por-check-ins-directos-en-main-o-dev-issue-225), solo Líder o Administrador, y solo para quitar de `main`/`dev` los check-ins directos que se acaban de pasar a la rama de la tarea.
- **El merge de `dev` hacia `main` es exclusivo del Administrador**: hoy este merge queda fuera del alcance de BranchBoard (es manual, directo en Unity VC — ver [Flujo de vida de una tarea](#contexto-completo-del-proyecto)) y "esto jamás debería pasar" a través de la app. Aun así, si en algún momento se decide incorporar esa acción a BranchBoard, queda anotado desde ya como **exclusiva del Administrador** — ningún otro rol podría invocarla, ni siquiera el Líder.

**Implementado (2026-11-14, #83)** en `app/backend/src/ramas.rs`, probado contra `BranchBoard-Pruebas`:
- **Nombre de la rama**: `<id>-<título>`, con el título en minúsculas, sin tildes, solo `a-z0-9` y guiones, y cortado a 30 caracteres entre palabras (ej. `T-101-sistema-de-combate-del-jugador`). **Decidido (2026-11-14)**: sin prefijo `task/` ni `task-`, porque en Unity VC la `/` separa niveles (crear `/main/dev/task/T-1` responde `BR_NOT_FOUND`: busca una rama `task`) y la jerarquía ya dice que la rama cuelga de `dev`.
- `crear_rama_tarea(cliente, repo, rama_padre, id, titulo)`: por la API REST (`GET …/branches/<padre>` para leer su `headChangeset` en ese momento, luego `POST …/branches` con `name` = ruta completa, `changeset` = esa cabeza y `comment` = `Rama de <id>: <título>`). Tarea general → hija de `/main/dev`; subtarea → hija de la rama de su padre, que tiene que ser una rama de tarea (debajo de `/main/dev/`). Rechaza antes de llamar al servidor si pasaría de 5 niveles, y si la rama ya existe.
- **Subtarea cuyo padre no tiene rama** (decidido 2026-11-14): al tomarla o asignarla, **se crea primero la rama del padre** (y la de cualquier ancestro que falte), de arriba abajo, cada una desde la cabeza actual de su propio padre. El padre recibe `rama_asociada` pero no un responsable: su rama es el contenedor de sus subtareas, y es la que al final entra a `dev`. `crear_ramas_de_cadena(cliente, repo, cadena)` recibe la cadena desde la tarea general hasta la que se toma y devuelve `(id, rama)` de cada rama creada para guardar `rama_asociada`. Si una ya existe en el servidor con la ruta esperada (un intento anterior que se cortó), se reusa. El límite de 5 niveles se revisa para toda la cadena antes de crear nada. `tomar_tarea`/`asignar_tarea` (#96, #98) la usan en vez de `crear_rama_tarea`.
- `validar_destino_checkin(rama)`: rechaza `/main` y `/main/dev` (sin importar mayúsculas ni la `/` final). Todo lo que haga check-in (#105) la llama antes.
- `cambiar_a_rama(estado, carpeta, rama)`: `cm switch br:<rama>` en el workspace y luego comprueba con `cm status --header` que quedó ahí. **Con cambios sin check-in no se cambia**: probado que `cm switch` se queda esperando una respuesta (la app no puede contestarla y quedaría colgada hasta el tiempo máximo), así que antes se corre `cm status --changed --added --deleted --moved --checkout --short` y, si hay algo, se avisa a la persona. Los archivos privados (nunca agregados) no cuentan y no bloquean el cambio. **Precisado (2026-11-14, #82)**: lo que `cm switch` hace con cambios pendientes es preguntar (guardar en shelve / llevarlos / cancelar); con `--noinput` se los lleva solo. `cambiar_a_rama` sigue bloqueando a propósito (cambiar de tarea con trabajo sin guardar es justo lo que se quiere evitar); la variante con `--noinput` es para "Crear rama" desde la subtarea "En espera", ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14).
- Prueba en el entorno real: `BB_PRUEBA_ORG=<org> BB_PRUEBA_REPO=<Proyecto/Repo> cargo test --lib entorno_real_ramas -- --ignored --test-threads=1` (crea la tarea, la subtarea y la subtarea de subtarea, comprueba el límite, el switch y el bloqueo por cambios pendientes, y borra todo al final).

### Tarea roja por check-ins directos en `main` o `dev` (issue #225)

**Decidido (2026-11-17, MiguelMeza17) e implementado** en `app/backend/src/cambios_directos.rs`:

- **Detección en el servidor**: al final de cada `revisar_actualizaciones_en` (cada 15 s), `cambios_directos::revisar_en` lee por la API los changesets de `/main/dev` y `/main` (`GET …/changesets?branchName=`). Cuentan los posteriores a la configuración del proyecto (la `firmado_en` más vieja de un Administrador: la historia anterior del proyecto Unity no es un error), que no sean `cs:0` ni se hayan visto ya. De esos, los que son **destino de un merge** (`cm find merge "where dstbranch='br:<rama>' on repository '<rep>'" --format={dstchangeset} --nototal`: los merges de las tareas, y también los cherrypick y sustractivos) se anotan en la tabla local `changesets_revisados` y no se vuelven a consultar; los demás son **check-ins directos**. **Probado (2026-11-17)** en BranchBoard-Pruebas: en `/main/dev` esa consulta devuelve 2, 5 y 8 (merges de T-101, T-102 y T-104) y no el 6, que es el check-in directo. Lo ve todo el equipo, aunque quien hizo el check-in no tenga BranchBoard abierto.
- **La tarea roja**: tarea general de tipo `Corrección`, título `Cambios directos en dev` (o `main`), **en producción**, sin aprobación pendiente, asignada al autor del check-in si es del equipo. Lleva `cambios_directos` (tabla `cambios_directos`, migración 10): la rama, la hora de detección y cada check-in (`changeset`, `padre` = `parentChangesetId` de la API, autor, fecha, comentario y archivos de `cm diff cs:<n>`). Se avisa a Líderes y Administrador con una notificación `advertencia`. Mientras siga en rojo, los check-ins directos nuevos de esa rama se suman a la misma tarea (`agregar_cambios_directos`).
- **Viaja por `ops/`**: `detectar_cambios_directos` (la tarea completa), `agregar_cambios_directos` y `trasladar_cambios_directos`. Si dos máquinas detectan lo mismo a la vez, todas se quedan con la de `detectado` más viejo (a igual hora, el id menor) y borran la otra si nadie la trasladó. En el snapshot (#94) la fila va dentro de la tarea.
- **Se trabaja como una tarea normal**: se puede mandar a revisión, pero `autorizar_merge` la rechaza mientras siga en rojo (`plan_de_autorizacion`).
- **Trasladar** (`trasladar_cambios_directos(id)`, solo **Líder o Administrador**, desde su workspace sin cambios pendientes): 1) crea por la API la rama `/main/dev/<id>-cambios-directos-en-dev` **desde el changeset anterior al primer check-in directo** (en `main`, desde la cabeza de `dev`, porque esos cambios no están en `dev`); 2) en esa rama, `cm merge cs:<n> --cherrypicking --merge` + check-in por cada uno, del más viejo al más nuevo; 3) en `main`/`dev`, `cm merge cs:<n> --subtractive --merge` + check-in por cada uno, del más nuevo al más viejo; 4) devuelve el workspace a la rama en la que estaba y publica el traslado. La tarea queda con `rama_asociada` y deja de estar en rojo; los changesets de la reversión son destino de un merge sustractivo y no se toman como check-ins directos. Si un traslado se corta después del paso 2, al reintentarlo se reusa la rama y solo se revierte. **Es la única excepción** a "nunca check-in a `main`/`dev`" (`merge_y_checkin` en `ramas.rs`, solo llamada desde `trasladar_cambios_workspace`).
- **Probado (2026-11-17)**: el orden importa. Si la rama de la tarea nace **después** de revertir, `cm merge --cherrypicking` responde `No se han detectado merges` y no trae nada. Naciendo del changeset anterior, el cherrypick trae los cambios y la vista previa del merge de vuelta a `dev` sale sin conflicto también para un binario (`El fichero /bin225.bin#cs:408 fue modificado en origen y reemplazara la version de destino`).
- Si se borra definitivamente una tarea roja que no se trasladó, sus check-ins vuelven a aparecer como tarea roja: los cambios siguen en `dev`.
- Prueba en el entorno real: `tareas::…::entorno_real_cambios_directos` (ver la tabla de CLAUDE.md).

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
3. El backend en Rust corre algo como `cm log br:/<rama> --xml` (como subproceso) y obtiene el changeset más reciente de esa rama. **Actualizado (2026-11-14, #84)**: se lee por la API REST con `checkins::leer_checkins_rama` (ver [tablas `commits`](#modelo-de-datos-tablas-commits-y-cambios_prestados)).
4. Si el changeset más reciente es mayor al `ultimo_changeset_visto` guardado → hay cambios nuevos:
   - Marca la tarea como "modificada" (para que el frontend le ponga un badge/indicador)
   - Actualiza `ultimo_changeset_visto`
   - Opcional: extrae metadata del changeset (autor, comentario, archivos afectados) para mostrarla en el detalle de la tarea
5. **El mismo refresco también revisa si la rama se cerró** (mergeada hacia su rama padre). Unity VC guarda el estado de cierre de una rama al mergearla. Si la rama aparece cerrada → la tarea se marca automáticamente como **"terminada"**, sin intervención manual. **Verificado (2026-11-14, #82)**: Unity VC no guarda un estado de cierre; "cerrada" = hay un merge link de tipo `merge` desde la rama hacia su padre (`cm find merge "where srcbranch='br:<rama>' …"`), ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14).
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

**Implementado (2026-11-16, issue #111)** — **Cada cuánto corre**:
- La revisión corre automáticamente **cada 15 segundos** en segundo plano desde que se abre la app (`app/backend/src/lib.rs`, `disparar_polling` → `revisar_actualizaciones_en`), además de dispararse **al recuperar foco la ventana** (`WindowEvent::Focused(true)`) y bajo demanda al invocar `revisar_actualizaciones`.
- Descarta cambios automáticos exclusivos de archivos `.meta` generados por Unity (`cambios_significativos`); si hay cambios significativos en `/main`, `/main/dev` o en una rama ajena (sin check-in o check-in reciente del usuario actual), devuelve `ResumenPolling.alerta_rama_ajena` y emite el evento Tauri `"alerta-rama-ajena"` (junto con `"unity-vc-actualizado"`) **sin deshacer ni modificar ningún archivo del workspace**. Sigue pendiente de definir si en una iteración posterior también notifica por campana a Líder/Administrador.
- **En pantalla (#225, 2026-11-17)**: el frontend escucha `"alerta-rama-ajena"` (`api.alAlertaRamaAjena` en `useSincronizacionEventos`) y la muestra arriba del contenido (`AlertaRamaAjena.tsx`) con el mensaje, la rama y los archivos, hasta que la persona la cierra; la misma alerta cerrada no vuelve a salir en el polling siguiente. Como revisa el workspace propio, solo cubre el equipo donde BranchBoard está abierto; los check-ins en `/main` y `/main/dev` hechos desde cualquier equipo los cubre la [tarea roja](#tarea-roja-por-check-ins-directos-en-main-o-dev-issue-225), que lee el servidor.

## Comando "Guardar cambios y sincronizar"

**Actualizado (2026-11-07, según el frontend)**: el frontend no tiene un botón "Guardar cambios y sincronizar". Lo que existe es **hacer check-in desde BranchBoard** en los detalles de una tarea sin subtareas (sección "Pending changes" → **"Hacer check-in"**, con título, descripción y tipo) y **"Crear rama"** en una tarea con subtareas — ver [Check-ins de la rama](#check-ins-de-la-rama-subtarea-en-espera-y-cambios-entrantes). Los comandos son `hacer_checkin(rama, titulo, descripcion, tipo)` y `crear_rama_desde_espera(espera_id, titulo, detalles, tipo)`; usan el mismo cable React → comando Tauri → Rust → `cm` que "tomar tarea". Al terminar, la escucha de Unity VC refleja el changeset nuevo sin pasos extra.

## Bloquear el cierre de la app durante una operación

Igual que hace Unity Editor mientras guarda/compila: mientras hay una operación de `cm` en curso (crear rama, sincronizar, merge, etc.), no se debe poder cerrar la app a la mitad.

- El backend mantiene una bandera en memoria `operacion_en_curso: bool` (o un contador, si puede haber más de una en paralelo), que se activa justo antes de invocar `cm` y se desactiva cuando el subproceso termina (éxito o error).
- Comando Tauri `hay_operacion_en_curso()` para que el frontend lo consulte antes de permitir cerrar la ventana.
- Tauri permite interceptar el evento de cierre de ventana (`onCloseRequested` o equivalente) para cancelarlo y mostrar un aviso en vez de cerrar de inmediato — esa parte de UI queda para [FRONTEND.md](./FRONTEND.md#navegación-general).

**Implementado (2026-11-10, issue #80)**: el contador vive en `AppState::operaciones_en_curso` y lo sube/baja `Comando::ejecutar` (y cualquier otra operación larga, con `AppState::iniciar_operacion`, que descuenta sola al terminar, también si falla). El bloqueo lo hace **el backend**, no el frontend: en `lib.rs`, al recibir `CloseRequested` con el contador > 0, se cancela el cierre (`prevent_close`) y se emite el evento `cierre-bloqueado`; el frontend solo lo escucha (`api.alCierreBloqueado`) y muestra el aviso. `hay_operacion_en_curso()` sigue disponible para consultar antes de otras acciones (ej. actualizar la app).

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
   - **Actualizado (2026-11-12, probado)**: el merge se hace **del lado del servidor por la API REST**, sin workspace ni `cm switch` de quien autoriza: (1) `POST .../mergeto/report` con `{ sourceType: "Branch", source: "/main/dev/<rama>", destination: "/main/dev" }` → si `resultConflicts.fileConflicts` no está vacío, ya se sabe que va a chocar y qué archivos (`path`); (2) `POST .../mergeto` con el mismo cuerpo → `{ status: "OK", changesetNumber: N }`. **Ojo**: `GET .../mergeto/allowed/<rama>` devuelve `OK` aunque haya conflicto (solo mira permisos/reglas), no sirve para detectarlo. Ver [Acceso a la API REST](#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12).
4. **Si el merge falla** (conflicto de contenido entre archivos): **decisión tomada** — BranchBoard no implementa resolución de conflictos propia, Unity Version Control ya tiene su propia herramienta para eso. El backend detecta que `cm merge` devolvió error (**actualizado 2026-11-12**: con la API, el conflicto **no llega como error HTTP** — `mergeto` responde 200 con `status: "Conflicts"`, `changesetNumber: 0` y el mensaje de que hay que hacer un "Merge from" desde un workspace; la lista de archivos para la alerta sale del `mergeto/report`, ya saneado, ver [SEGURIDAD.md](./SEGURIDAD.md#api-rest-del-servidor-de-unity-vc-manejo-de-tokens-probado-2026-11-12)) y la tarea **no se marca como terminada** (se queda en "en revisión"). Como quien dispara el "autorizado" es el propio Líder/Admin (acción síncrona, no en segundo plano), la reacción es una **alerta/pantalla inmediata** en ese momento (no una notificación de campana) mostrando que el merge falló, con dos opciones para el Líder/Admin:
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
3. Comando Tauri `autorizar_merge(tarea_id)` (el mismo de "Tareas grandes"; **según el frontend** ya no hay un `autorizar_merge_subtarea` aparte): si la tarea tiene padre, ejecuta `cm merge` de su rama hacia la rama de su tarea padre (no hacia `dev`) — mismo patrón de invocar `cm` como subproceso (**actualizado 2026-11-12**: igual que en "Tareas grandes", por la API REST con `mergeto`, `destination` = rama de la tarea padre). Valida que quien autoriza sea quien está a cargo de la tarea padre.
4. **Si falla por conflicto**: mismo patrón que el flujo de "Tareas grandes" — alerta síncrona inmediata para quien intentó autorizar, con las mismas dos opciones (resolverlo él mismo, o rechazar con `rechazar_revision`).
5. Si se **rechaza**: la subtarea vuelve a "en producción" con su responsable, sobre la misma rama hija (ver [Subtareas en PLANNING.md](./PLANNING.md#subtareas-tareas-anidadas)).
6. Tras un merge exitoso, la subtarea pasa a "terminada" — no dispara el flujo completo de "Detección de cambios" del punto anterior (no aplica el chequeo de changesets por polling para algo tan simple), solo el cambio de estado.

**Resuelto (según el frontend)**: lleva la misma confirmación que mergear a `dev` (escribir "autorizar").

**Implementado (2026-11-15, #107; frontend conectado 2026-11-16)** — los dos flujos de arriba, en `tareas.rs` (`asignar_revision`, `plan_de_autorizacion`, `registrar_merge_autorizado`, `rechazar_revision`), `comandos/tareas.rs` (`autorizar_merge_en` arma el merge) y `servidor/cliente.rs` (`reporte_merge`, `merge_a`). En Tauri el store llama a estos comandos y usa la respuesta; `ConflictoMergeModal` muestra la alerta común a Revisión y los tableros, con resolución manual o rechazo con motivo. El navegador conserva el mockup:
- **`asignar_revision(id, email)`**: solo sobre una tarea en revisión y solo a un Líder/Administrador del proyecto. Sin revisor lo elige un Líder o el Administrador; con revisor, solo el Administrador lo cambia. Notificación `revision_asignada` al elegido. Viaja por `ops/`.
- **`autorizar_merge(id)`**, en este orden: (1) **valida en SQLite** sin tocar el servidor: tarea aprobada y en revisión (o **en producción si es un Líder/Administrador sobre su propia tarea**: el merge directo), con rama, con todas sus subtareas terminadas y sin cambios prestados sin devolver; quién puede: Líder/Administrador y, en una subtarea, quien tiene a cargo la tarea padre (`asignado_a` del padre). Destino: la rama del padre, o `/main/dev` si no tiene. (2) `POST …/mergeto/report`: si trae archivos en conflicto, **no se intenta el merge** y el comando responde `{ tareas, conflicto: { origen, destino, archivos, mensaje } }` con la tarea sin cambios, para la alerta síncrona. (3) `POST …/mergeto` con `comment` `"<id>: <título> (merge autorizado desde BranchBoard)"`: `status: "OK"` deja el changeset en el destino; `"MergeNotNeeded"` (la rama ya se conectó a mano, por ejemplo tras resolver un conflicto en Unity VC y reintentar) también vale; `"Conflicts"` responde como el paso 2. (4) Evento `autorizar_merge` en `ops/` → la tarea pasa a `terminada` con `fecha_completada` y se recalculan los padres. El workspace de quien autoriza no se toca.
- **`rechazar_revision(id, motivo)`**: motivo obligatorio y solo sobre una tarea en revisión; mismos permisos que autorizar. La tarea y todo su subárbol activo vuelven a `'en producción'` (igual que mover hacia atrás con `cambiar_estado`), queda `motivo_rechazo` y el responsable recibe `tarea_rechazada` con el motivo. Viaja por `ops/`.
- **Formas reales de la API** (probadas el 2026-11-15 contra BranchBoard-Pruebas; el JSON se sanea antes de leerse): el reporte trae `result.resultConflicts.fileConflicts[]`, cada uno con `src.path`/`dst.path` (y otras listas como `evilTwins`, `changeDeleteConflicts`… con `path` directo); cuando la rama ya está conectada no viene `resultConflicts` y `result.resultStatus` es `MergeNotNeededAlreadyConnected`. `mergeto` responde `{"status":"OK","changesetNumber":N}`, `{"status":"Conflicts","message":"… It is necessary to run a \"Merge from\" operation from a workspace …","changesetNumber":0}` o `{"status":"MergeNotNeeded","changesetNumber":0}`; acepta `comment`. Un changeset de merge en el destino se puede borrar con `cm changeset delete` mientras ningún workspace esté parado en él (así limpia `/main/dev` la prueba real).
- **"Resolver en Unity VC"** (implementado 2026-11-17, #207): el botón de la alerta de conflicto llama a `abrir_resolucion_conflicto(destino)`. Si el workspace de quien autoriza no está en la rama destino, exige que no tenga cambios sin check-in (un `cm switch` con cambios se queda preguntando) y cambia con `cm switch`; solo acepta `/main/dev` o ramas debajo. Después abre Unity VC sin esperar a que se cierre: `plastic.exe --branchexplorer=<ruta del workspace>` (argumento documentado en las notas de la versión 11 del cliente). Ahí la persona hace "Merge from this branch" desde la rama origen, resuelve los archivos y hace check-in. Luego, en una subtarea, la escucha de Unity VC detecta el merge link y la cierra (#104); en una tarea general, se vuelve a "Autorizar" y el servidor responde `MergeNotNeeded`, que la termina (#107). BranchBoard nunca intenta resolver el conflicto por su cuenta (`cm merge --merge` se cuelga con conflictos, #82).
- **Pendiente de decidir** (sigue): si con revisor asignado solo él puede autorizar o rechazar (hoy, como en el frontend, cualquier Líder/Administrador).

**Fechas de finalización (2026-11-16)**: `cambiar_estado` y `autorizar_merge` calculan el día local del emisor al terminar una tarea y lo incluyen en el evento. La reproducción conserva esa fecha, incluso si otra máquina está en otra zona horaria. Los eventos antiguos de `cambiar_estado` sin `fecha_completada` mantienen su interpretación anterior del timestamp UTC. Así Avance no cuenta como pendiente hasta mañana una tarea terminada por la noche en Colombia.

## Estados, progreso y reaperturas

**Nuevo (2026-11-07, según el frontend; `cambiar_estado` implementado 2026-11-15 en #99)**. Reglas que el frontend ya aplica y que el backend tiene que validar igual (al ejecutar el comando y al reproducir el evento):

- **Estados**: por hacer / en producción / en revisión / terminada. "Sin asignar" no es un estado: es "por hacer" sin `asignado_a`.
- **El estado de una tarea con subtareas se calcula a partir de ellas** (se recalcula de abajo hacia arriba por toda la cadena de padres):
  - todas sus subtareas en "por hacer" → "por hacer", **salvo que la tarea padre ya tenga commits** (entonces "en producción");
  - alguna en "por hacer" o "en producción" → "en producción";
  - ninguna en "por hacer"/"en producción" → "en revisión".
  - "Terminada" es final: una tarea padre ya mergeada no se vuelve a calcular.
  - Si una tarea padre avanza por sus subtareas y no tiene responsable, hereda el de alguna de ellas (ninguna tarea fuera de "por hacer" queda sin responsable).
- **Progreso = commits**: una tarea (o cualquiera de sus subtareas) con commits no puede volver a "por hacer" ni soltarse; se avisa y se rechaza.
- **Mover hacia atrás** (`cambiar_estado` a "por hacer" o "en producción") mueve también todo su subárbol; hacia "en revisión" o "terminada" solo la tarea (sus subtareas ya tuvieron que llegar).
- **Borrar progreso** (`borrar_progreso(tarea_id)`, **implementado #100**, todos los roles, palabra **"eliminar"**): pasar de "en producción" a "por hacer" una tarea con progreso. Borra los commits de la tarea y sus subtareas, sus cambios prestados y sus cambios sin check-in, y la deja en "por hacer" (sigue tomada y con su rama). En Unity VC: revertir la rama a su changeset inicial — **verificado (2026-11-14, #82)**: `cm changeset delete` de la cabeza hacia atrás si ningún changeset fue prestado, y si no, merge sustractivo de intervalo (`cm merge cs:<cabeza> --subtractive --interval-origin=cs:<base> --merge` + checkin); ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14).
- **Reabrir** (`reabrir_tarea(tarea_id, motivo)`, **implementado #100**, palabra **"reabrir"** y motivo obligatorio): una tarea "terminada" vuelve a "en revisión" (sus subtareas se quedan terminadas). Cada tarea guarda `reaperturas`; se puede reabrir **hasta 3 veces** y desde ahí **solo el Administrador** puede seguir reabriéndola. Notifica a todo Líder/Administrador (notificación `tarea_reabierta`, con el motivo).
- **De "En revisión" a "Terminada" arrastrando la tarjeta**: solo Líder/Administrador; es lo mismo que autorizar el merge (palabra "autorizar").
- **Saltar estados desde "Sin asignar"** (arrastrar directo a otra columna): primero se toma la tarea (se crea la rama) y luego sigue el flujo normal de la columna destino. Un Desarrollador o Tester no puede mandarla directo a "Terminada".
- **Colaborador externo**: solo toma o recibe tareas **individuales** (sin subtareas y que no sean subtarea de otra), ver [Alcance](#alcance).

## Eliminar una tarea

Comando Tauri `eliminar_tarea(tarea_id, justificacion)`, validado en backend para que solo Administrador o Líder puedan invocarlo (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)). Requiere el doble candado descrito en [FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop) (justificación + escribir "eliminar"). Al eliminarla también se limpian `pendiente_aprobacion` y la edición pendiente que tuviera.

- La `justificacion` se guarda en SQLite como parte del registro de auditoría (no se borra junto con la tarea, queda como historial de por qué se eliminó).
- **Confirmado**: si la tarea ya tenía una rama asociada en Unity VC, `eliminar_tarea` también **cierra esa rama** en Unity VC (**verificado 2026-11-14, #82**: Unity VC no tiene "cerrar rama"; la rama se deja como está, sin que ninguna tarea la muestre, y si se quiere ver en Unity que fue eliminada se marca con un atributo, `cm attribute set …`, ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14)), en vez de dejarla huérfana. La `justificacion` queda como el motivo tanto del borrado en BranchBoard como del cierre de la rama.
- **Definido**: esto **no borra la tarea de inmediato** — la manda a la [Papelera](#papelera), desde donde se elimina definitivamente en un segundo paso. Rechazar una tarea de [Pendientes](./PLANNING.md#pendientes) usa este mismo comando (el Líder/Admin decide que no debe existir, así que la "elimina" igual que cualquier otra tarea).
- **Señalado por el usuario**: `eliminar_tarea` también genera una notificación al **creador** de la tarea (ver [Notificaciones](#notificaciones), punto 13) — la `justificacion` escrita es lo que arma el mensaje.
- **Definido — borrado en cascada de subtareas** (el mockup todavía no lo hace: hoy solo manda a la Papelera la tarea elegida; el backend sí lo debe hacer): si la tarea tiene subtareas colgando (`tarea_padre_id` apuntando a ella, sin importar cuántos niveles de anidación), `eliminar_tarea` **arrastra en cascada solo las subtareas que siguen activas** (`estado` distinto de `terminada` y `eliminada = false`) — se mandan a Papelera junto con la tarea padre, en la misma operación, con la misma `justificacion`, y sus ramas también se cierran en Unity VC. Razón: una vez se cierra la rama padre, dejar una subtarea activa colgando de ella la deja huérfana (su flujo de merge apunta justo a esa rama, ver [Flujo de merge de subtareas pequeñas](#flujo-de-merge-de-subtareas-pequeñas-hacia-su-tarea-padre)).
  - **Las subtareas ya `terminada` (mergeadas) no se tocan** — su trabajo ya está integrado en el historial de la rama padre antes de cerrarla; borrar la tarea padre no debe deshacer trabajo ya completado y mergeado.
  - Cada creador de una subtarea arrastrada recibe la misma notificación #13 que el creador de la tarea padre, con una `justificacion` que aclara que fue un borrado en cascada (ej. "eliminada junto con su tarea padre: <motivo>").

## Papelera

**Nuevo**, va en el menú justo después de "Pendientes". **Visible para todos los roles** (Desarrollador y Tester la ven en **solo lectura**, igual que la carpeta "Tareas grandes" de Revisión) — lista todas las tareas con `eliminada = true`, sin importar si llegaron ahí por `eliminar_tarea` directo o por el rechazo de una tarea de Pendientes.

- Comando Tauri `eliminar_definitivamente(tarea_id)`, **exclusivo Líder/Administrador**, mismo doble candado que el resto de acciones destructivas (escribir "eliminar" para habilitar el botón).
- A diferencia de `eliminar_tarea` (que ya cerró la rama pero conserva la fila como soft-delete para historial), esto **borra la fila por completo** de la tabla `tareas` y purga la rama en Unity VC en vez de solo cerrarla (**verificado 2026-11-14, #82**: `cm branch delete br:<rama>@<rep> --delete-changesets`; solo funciona si la rama no se mergeó ni se prestó a otra y no tiene hijas — si ya entró a su padre, Unity VC no deja borrarla y se queda en el historial; ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14)).
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

**Devolver** (`devolver_cambios_prestados(prestamo_id)`): botón **"Devolver"** en cada préstamo. En Unity VC es un **merge sustractivo** del changeset que se trajo (**verificado 2026-11-14, #82**: `cm merge cs:<changeset del préstamo> --subtractive --merge` + `cm checkin --all -c="…"`; el préstamo mismo es `cm merge cs:<N> --cherrypicking --merge` + checkin; ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14)). El registro no se borra: queda como historial ("Devuelto el … por …").

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
- Ese email es el identificador del usuario dentro de BranchBoard — no hay contraseña ni sesión propia. **Actualizado (2026-11-12)**: el único token es el **PAT de la API REST**, que la app crea y guarda sola con la sesión de `cm` (la persona nunca lo ve ni lo escribe) — ver [Acceso a la API REST del servidor de Unity VC](#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12).
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

**Implementado (2026-11-10, issue #81)** en `app/backend/src/cm/lectura.rs`, sobre el [ejecutor](#seguridad-al-invocar-cm-evitar-inyección-de-comandos): `whoami()` corre `cm whoami` y devuelve el correo en minúsculas (toma la última línea, por si `cm` imprime un aviso antes); si no hay sesión, `sin_sesion_unity_vc`; sin `cm`, `cm_no_instalado`. `listar_workspaces()` lee `cm workspace list` (una línea por workspace: `nombre@máquina ruta`, la ruta puede llevar espacios) y devuelve `{ nombre, ruta }`; el `repo` y `tiene_db_branchboard` que muestra "Tus proyectos" los completa [entrar a un proyecto](#sesión-de-unity-vc-desde-la-app-proyectos-y-salir-según-el-frontend) (#89). `rama_actual_workspace(carpeta)` corre `cm status --header` en la carpeta del workspace y devuelve `{ rama, repositorio, changeset }` a partir de la línea `/rama@repo@servidor (cs:N - head)`. Los tres intérpretes de salida están probados con texto de ejemplo; los comandos de sesión (`iniciar_sesion_unity_vc`, `cerrar_sesion_unity_vc`) son del issue #124.

**Implementado (2026-11-14, issue #89)** — `entrar_a_proyecto(carpeta)` (libre, todavía no hay rol), en `app/backend/src/identidad/acceso.rs` y `comandos/acceso.rs`:
1. `cm whoami` → correo; `cm status --header` en la carpeta del workspace → repositorio (`Proyecto/Repo@<org>@unity`). Un workspace de un servidor local (no Unity Cloud) da error de validación.
2. Datos del proyecto en `%LocalAppData%\BranchBoard\proyectos\<org>-<repo>\`. Mientras no existan los eventos de `ops/` (#91 a #93), "el proyecto tiene datos de BranchBoard" = en esa carpeta están la base y la llave pública del Administrador.
3. **Con datos**: se busca el correo en `usuarios` y se verifica su firma → `{ estado: "dentro", email, rol, rol_verificado, proyecto, configurado_ahora: false }`, `{ estado: "pendiente" }` (sin firma) o `{ estado: "no_registrado" }`. Solo si entra queda abierta la base (`AppState::abrir_proyecto`), así los comandos con chequeo de rol ya funcionan.
4. **Sin datos**: el **dueño del repositorio** es el campo `owner` de `GET /repos` de la API (probado 2026-11-14: es el correo de la cuenta dueña, igual al de `cm whoami`). Esto resuelve el "comando `cm` exacto pendiente de verificar" de arriba. Si es quien entra: se crea la base, se generan las llaves del Administrador (`admin.key` sin cifrar, se puede respaldar con contraseña desde Ajustes), se guarda su fila firmada como Administrador y `nombre_proyecto` = nombre del repo → `dentro` con `configurado_ahora: true`. Si no: `{ estado: "esperar_dueno" }`, sin tocar nada.
Prueba en el entorno real: `BB_PRUEBA_ORG=<org> BB_PRUEBA_REPO=<Proyecto/Repo> cargo test --lib entorno_real_entrar -- --ignored` (con la cuenta dueña: la primera vez configura y queda Administrador, la segunda entra sin configurar). **Pendiente de probar con una cuenta que no sea la dueña** (debe dar `esperar_dueno`).

### Sesión de Unity VC desde la app, proyectos y salir (según el frontend)

**Caducidad SSO (#203)**: `cm::Comando` traduce la salida fallida `Error: Controlador no válido.` (también sin acento o con el carácter de reemplazo de la consola) a `sesion_unity_vc_caducada`, tanto en ejecución normal como con progreso. Solo reconoce líneas de error de `cm`, no salidas exitosas ni mensajes de otros programas. Invalida la identidad en memoria sin modificar el PAT, la sesión de Unity ID ni los datos del proyecto. No repite el comando. Los errores de red/permisos conservan su tratamiento anterior.

Las llamadas IPC notifican al frontend al recibir ese código; si se detecta en el polling, el backend emite `sesion-unity-vc-caducada`. App muestra el aviso existente de Reconectar. Solo el clic de la persona fuerza `iniciar_sesion_unity_vc(true)` y después verifica la reconexión. Cuando se conoce la organización, `reconectar_unity_vc` comprueba también `cm repo list <org>@unity`: un `whoami` local y un PAT válido no demuestran que la sesión remota de la CLI siga vigente. La identidad solo se restaura después de validar la conexión. La limitación de Unity (necesitar el SSO oficial) permanece.

**Perfiles históricos y sondeos**: el mismo mensaje aparece al consultar un perfil/servidor antiguo o sin organización predeterminada válida. Las consultas opcionales de descubrimiento usan `como_sondeo`: conservan el error tipado sin invalidar la identidad. El listado de proyectos prueba las organizaciones candidatas; si alguna responde, conserva sus proyectos y la sesión aunque otras fallen. Solo eleva caducidad si ninguna responde y todos los errores son de sesión; un fallo de permisos/red conserva su código. Las búsquedas opcionales de ramas tampoco cierran la sesión por un repositorio inaccesible. Sin organización, el error del servidor predeterminado se trata como configuración pendiente, no como SSO vencido.

Pruebas de #203: `entorno_real_clasificacion_sesion_caducada` hace lecturas reales de ramas de BranchBoard-Pruebas, incluida la salida incremental, y verifica la reconexión de una sesión sana sin abrir el navegador. Reproduce la salida de caducidad observada en la issue sobre el estado temporal de la prueba; no caduca ni modifica las credenciales reales. `npm run test:sesion` comprueba el error IPC, el aviso sin reintentos, la conservación de datos, la cancelación y la reconexión explícita con un backend controlado.

**Nuevo (2026-11-07)**. Lo que el frontend ya muestra en la [Pantalla inicial](./FRONTEND.md#autenticación--acceso-al-proyecto) y en el menú del proyecto (clic en el logo):

- **Actualizado (2026-11-20) — inicio de sesión por el navegador, sin Plastic SCM** (`app/backend/src/cm/sso_navegador.rs`; lo usan `iniciar_sesion_unity_vc` y `reconectar_unity_vc` en lugar de `plastic --configure`). Es el flujo del cliente oficial, comprobado a mano contra la cuenta real antes de programarlo:
  1. Se abre en el navegador `https://www.plasticscm.com/api/oauth/unityid/authorize/session/<uuid>` (redirige al OAuth de Unity ID, con PKCE del lado del servidor).
  2. Se consulta `GET https://www.plasticscm.com/api/oauth/token/session/<uuid>` cada 2 s (máx. 5 min): responde `{"error": …}` hasta que se termina en el navegador, y luego `{ user, accessToken, refreshToken, authAccessToken }`.
  3. `POST https://services.unity.com/api/auth/v1/genesis-token-exchange/unity` con `{ "token": authAccessToken }` → `201 { token }`; con ese token (`Bearer`), `GET https://services.unity.com/api/unity/legacy/v1/users/me/organizations` → `organizations[{ name, genesisId }]`.
  4. La sesión se escribe **donde la guarda el cliente oficial** (`%LocalAppData%\plastic4`), así que vale también para Plastic SCM / Unity VC, y una sesión iniciada allí la ve BranchBoard: `tokens.conf` (`server=<org>@cloud user=<correo> token=<accessToken> seiddata=<correo>`, por id genesis y por nombre), `client.conf` (`WorkingMode=SSOWorkingMode`, `SecurityConfig=<correo>`, `WorkspaceServer=<org>@cloud` si el actual no es de la cuenta o es `*@cloud`), `profiles.conf` (un `<ServerProfile>` SSO con el correo por servidor) y `unityorgs.conf` (`genesisId:slug`).
  - Comprobado: **sin el perfil de `profiles.conf`, `cm repo list <org>@unity` da `Controlador no válido`** aunque `<org>@cloud` funcione (la app lo tomaba como sesión caducada y volvía al login). **`server=*@cloud` no sirve**: `cm whoami` responde pero todo lo remoto falla. **`SecurityConfig` vacío ⇒ `cm whoami` vacío** aunque haya token. Los archivos se escriben directo (no con `cm configure`, que además escribe el `tokens.conf` real aunque se le pase `--clientconf` temporal, y dejaría el token en la lista de procesos). Son endpoints internos de Unity, no documentados: si cambian, hay que ajustar este módulo.
- **"Iniciar sesión con Unity VC"** (antes de 2026-11-20): si no hay sesión (`cm whoami` falla), la app abría el inicio de sesión **oficial de Unity en el navegador** vía `plastic.exe --configure` (SSO); BranchBoard nunca ve ni guarda la contraseña. Al volver, se vuelve a correr `cm whoami`. (**Implementado #124** en `app/backend/src/cm/sesion.rs` y `comandos/cm.rs`: `sesion_unity_vc()`, `iniciar_sesion_unity_vc(forzar?)`, `reconectar_unity_vc(org?)`, `cerrar_sesion_unity_vc()`.)
- **Actualizado (2026-11-20)**: "Cerrar sesión" del menú ahora solo cierra la **cuenta de BranchBoard** (frontend, `cerrarSesionBranchBoard`) y **no** llama a `cerrar_sesion_unity_vc`, que queda disponible pero sin uso en la interfaz (borraba la sesión compartida con Plastic SCM).
- **"Cerrar sesión"** (menú del proyecto, antes de 2026-11-20): revoca los PAT de BranchBoard en Unity VC (`cm accesstoken revoke`), los borra del llavero, limpia `tokens.conf` / `cloudedition.token` / `<SecurityConfig>` en `plastic4` y cierra el proyecto en memoria (`cerrar_sesion_unity_vc()`); la app queda en "No se detectó una sesión de Unity VC" y al volver a entrar pasa otra vez por el acceso al proyecto.
- **"Tus proyectos"**: los workspaces locales que ya usan BranchBoard (`cm workspace list` + si tienen datos de BranchBoard); elegir uno entra a ese proyecto (`entrar_a_proyecto`). (**Implementado #125** en `app/backend/src/cm/proyectos.rs` y `comandos/cm.rs`: `listar_workspaces_locales`, `listar_tus_proyectos`.)
- **"Cargar proyecto"**: lista los repositorios del servidor de Unity VC a los que la persona tiene acceso y que todavía no están en ese equipo (`listar_proyectos_servidor()`, vía `cm repo list`), con la etiqueta "Con tareas" si ya usan BranchBoard. Elegir uno lleva a [Clonar proyecto](#clonar-proyecto-in-app). (**Implementado #125**.) **Desde #240** también cuenta como "Con tareas" un repo que ya es proyecto de la cuenta en el servidor, y trae `rol_servidor` (ver [Proyectos en el servidor](#proyectos-en-el-servidor-240)).
- **"Nuevo proyecto"**: (a) **traer** un proyecto de Unity VC que ya está en el equipo pero nunca se configuró con BranchBoard (`traer_proyecto(carpeta)` — se configura como dice [Identidad](#identidad-de-usuario) y asegura `/main/dev`), o (b) **crear** uno nuevo: se crea su repositorio y su workspace en Unity VC (`crear_proyecto(nombre, carpeta, proyecto_unity?, org?)`, `cm repo create` dentro del proyecto padre existente en Unity Organizations + `cm workspace create` + rama base `/main/dev`) y quien lo crea queda como Administrador. La lista `cm repo list` filtra automáticamente los repositorios marcados como `__DELETED__*`. **Implementado (2026-11-16, #205)**: como `cm` no puede crear proyectos padre en Unity Organizations, el modal ofrece **"Crearlo en Unity Cloud"** (`abrir_unity_cloud(org?)`: abre en el navegador `https://cloud.unity.com/home/organizations/<id>/projects`, con el id numérico de la organización leído de `cloudregions.conf`; si la organización está anotada por nombre, se usa el id que esté en el mismo `server` cuando ese servidor tiene un solo nombre y un solo id, y si no, la portada del Dashboard) y **"Recargar proyectos"** (vuelve a correr `listar_proyectos_servidor`). El comando es libre (no exige proyecto abierto) y la URL pasa por la misma validación HTTPS que las descargas.
- **"Salir de la app"**: cierra la ventana (respetando [Bloquear el cierre durante una operación](#bloquear-el-cierre-de-la-app-durante-una-operación)).
- **Proyectos sin Unity VC ("Continuar sin Unity VC")** — **implementado (2026-11-19)**, módulo `proyecto_local.rs`:
  - **Todo el proyecto es una carpeta** que se puede mover, copiar, exportar e importar: `branchboard-proyecto-local.json` (la **marca** que la identifica: `aplicacion: "BranchBoard"`, `tipo: "local"`, `formato`, `id` único, `nombre`, `creado_en`), `datos/` (la base `branchboard.db`, `admin.key`/`admin.pub`, `backups/`) y `compartida/` (lo que en un proyecto de Unity VC va en Drive/OneDrive: `ops/`, documentos, imagen). A diferencia de los proyectos de Unity VC, aquí la base **no** va en `%LocalAppData%`: va dentro del proyecto para que se pueda llevar.
  - **Contraseña**: no se guarda ni su hash. Cifra la llave del Administrador (`datos/admin.key`, el mismo cifrado que `exportar_llave_firma`). Abrir el proyecto exige desbloquear esa llave: contraseña equivocada → "La contraseña del proyecto no es correcta.". Quien entra es el Administrador cuya fila firmada verifica `admin.pub`; la llave queda abierta en memoria (`AppState::recordar_llave_firma`) y no se vuelve a pedir al firmar.
  - **Abrir** solo acepta una carpeta con la marca válida; una carpeta cualquiera (o la marca `branchboard-proyecto.json` de la carpeta compartida de Unity VC) da "En esa carpeta no hay un proyecto de BranchBoard.". Una marca con `formato` mayor que el de la app pide actualizar. Cada vez que se abre, la carpeta compartida se vuelve a fijar en `<proyecto>/compartida` (`eventos::fijar_carpeta_compartida_local`), así sirve aunque la carpeta se haya movido o importado.
  - **Recientes**: `%LocalAppData%\BranchBoard\proyectos-locales.json` (hasta 20; solo se listan los que siguen teniendo la marca).
  - **Exportar** (solo Administrador): un `.bbproyecto` (zip) con la marca, `datos/` y `compartida/`. La base va como copia consistente (`VACUUM INTO`); no van `backups/`, `local.json` ni los `-wal`/`-shm`. La llave va cifrada, así que el archivo no sirve sin la contraseña (la base en sí no va cifrada).
  - **Importar**: revisa todas las rutas del zip antes de escribir (solo la marca y lo que cuelga de `datos/` o `compartida/`; nada con `..`) y lo descomprime en una carpeta temporal que al final se renombra a `<destino>/<nombre>` (o `<nombre> (2)`, … si ya existe). No abre el proyecto: después se abre con su contraseña.
  - **Sin resolver todavía**: las acciones que necesitan Unity VC (tomar una tarea crea su rama, check-ins, merge) fallan con "El proyecto abierto no tiene workspace local"; falta decidir cómo se trabajan las tareas sin ramas, si se suma más gente a un proyecto local y si se puede conectar a Unity VC después.

### Acceso a la API REST del servidor de Unity VC (PAT, decidido 2026-11-12)

**Decisión**: BranchBoard usa la [API REST del servidor de Unity VC](https://docs.unity.com/en-us/services-web-apis/unity-version-control-server/overview) para las operaciones **del servidor** y `cm` para las del **workspace**. No cambia la identidad (sigue siendo `cm whoami`) ni hay login propio: solo se agrega un token por persona que la app gestiona sola.

| Por la API REST (HTTP desde Rust, sin workspace) | Por `cm` (subproceso, en el workspace de quien trabaja) |
|---|---|
| Crear rama al tomar/asignar (`POST .../branches`) | `cm whoami` (identidad), `cm workspace list` |
| Reporte de conflictos y merge al autorizar (`mergeto/report`, `mergeto`) | `cm switch`, `cm status`, `cm checkin`, abrir Unity |
| Listar ramas/changesets, diff (#152) | `cm partial` (Gluon, Colaborador externo) |
| Leer permisos (ej. `rmrepository` para eliminar el proyecto) | `cm accesstoken create/reveal/revoke` (gestionar el PAT) |

**Datos del servidor** (probados contra el repo de prueba `BranchBoard-Pruebas`): la organización en `cm` se escribe `<org>@unity` (no `@cloud`); la URL base depende de la región de la organización (ej. US East: `https://prd-azure-eastus-01-cloud.plasticscm.com:7178`) — **implementado (2026-11-14, #159)**: se lee del `webserver` de la organización en `%LOCALAPPDATA%\plastic4\cloudregions.conf`, que el cliente de Unity VC escribe solo; no se guarda en `configuracion_proyecto`. **Corregido (2026-11-15, #107)**: en una máquina que entró por Unity ID el archivo trae el **id numérico** de la organización (`organization=4673426565751@cloud …`) y no su nombre; si no hay línea por nombre y todas las que hay apuntan a la misma región, se usa ese `webserver` (la API acepta el nombre en la URL igual, probado con `GET …/organizations/MiguelM17/repos`). Con dos regiones distintas sigue pidiendo abrir el proyecto en Unity VC. Las rutas son `<webserver>/api/v1/organizations/<org>/…` (ej. `GET …/repos`). El repo va en la URL con la `/` codificada (`Proyecto%2FRepo`). Al crear una rama, `name` lleva la ruta completa (`/main/dev/T-101`) y `changeset` es el número de la cabeza del padre (hay que leerla antes con `GET .../branches`).

### Comandos `cm` verificados en el repo de prueba (issue #82, 2026-11-14)

Los comandos que estaban "pendientes de verificar al implementar", probados a mano contra `BranchBoard-Pruebas` con una cuenta administradora del repo que **no** es dueña de la organización, en un workspace temporal y con ramas `it-82-<hora>-…` que se borraron al terminar. Convenciones: `<rep>` = `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity`; todo `cm` se corre **dentro de la carpeta del workspace** (como hace `cm::Comando::en`) y con la entrada estándar cerrada. `cm switch br:/main/dev --workspace=<ruta>` desde fuera respondió `br:/main/dev no existe`; desde dentro funciona tanto `br:/main/dev` como `br:/main/dev@<rep>`.

| Qué | Cómo | Resultado probado |
|---|---|---|
| **Saber si una rama ya se mergeó ("cerrada")** | Unity VC **no tiene estado de cierre** de ramas. La señal es el **merge link**: `cm find merge "where srcbranch='br:<rama>' on repository '<rep>'" --format="{type} {srcbranch}@{srcchangeset} -> {dstbranch}@{dstchangeset}" --nototal` | Vacío antes del merge; después, una fila `merge br:…/hija@25 -> br:…/padre@28`. **Un préstamo no cuenta**: el cherry-pick sale con tipo `cherrypick` (y su devolución, `cherrypicksubtractive`). "Cerrada" = existe una fila de tipo `merge` cuyo `dstbranch` es su rama padre (`/main/dev` para una tarea general). Si además se quiere marcar en Unity VC, sirven los atributos (probado, lo puede hacer un administrador del repo): `cm attribute create att:<nombre>@<rep>`, `cm attribute set att:<nombre>@<rep> br:<rama>@<rep> <valor>`, y se consulta con `cm find branch "where attribute='<nombre>' and attrvalue='<valor>' on repository '<rep>'" --format="{name}" --nototal`. |
| **Merge a `dev` o a la rama padre** | Por la API (`mergeto/report` + `mergeto`, probado el 2026-11-12). El equivalente en `cm`, también del lado del servidor y sin workspace: `cm merge br:<hija>@<rep> --to=br:<padre>@<rep> --merge -c="<comentario>"` | `Creado changeset cs:28@/main/dev/…padre…` en la rama destino, más el merge link de arriba. Sin `--merge` solo lista lo que haría (`El fichero … fue modificado en origen y reemplazara la version de destino`, `El elemento … ha sido añadido en origen …`). **No hace falta `cm checkin`**: el merge-to ya deja el changeset. |
| **Cómo se ve un conflicto** | Preview del merge-to (sin `--merge`): `cm merge br:<hija>@<rep> --to=br:<padre>@<rep>`, o el mismo con `--machinereadable` | Texto: `Se necesita hacer merge del fichero /it82-padre.txt origen cs:29 destino cs:30 base cs:25. Modificado por ambos contribuidores.` Máquina: `FILE_CONFLICT /it82-padre.txt 25 29 30 165` (ruta, base, origen, destino, id). **Nunca correr `--merge` si hay conflicto**: tanto el merge-to como el merge en workspace (`cm merge br:<hija> --merge`) se quedan esperando una herramienta de merge interactiva **aunque se pase `--nointeractiveresolution`** y la entrada esté cerrada (hubo que matar el proceso; el merge-to no dejó changeset). Si un merge en workspace quedó a medias, `cm status` muestra "Links de merge pendientes" y `cm undo -r .` lo limpia. Por eso BranchBoard mira primero el reporte (API o preview) y, si hay conflicto, manda a la persona a resolverlo en Unity VC. |
| **Traer cambios (préstamo)** | En el workspace de la rama destino: `cm merge cs:<N> --cherrypicking --merge` y luego `cm checkin --all -c="<comentario>"` | Entre los dos pasos `cm status` muestra "Links de merge pendientes → Cherry pick de cs:N" y cada archivo como `(Cherrypick de N)`. El checkin crea el changeset del préstamo; `cm find merge` lo lista como `cherrypick`. Archivos que llegaron: `cm diff cs:<anterior>@<rep> cs:<préstamo>@<rep>` → `C "it82-padre.txt"` (cambiado), `A "it82-hija.txt"` (agregado). El preview sin `--merge` **no sirve** para cherry-pick (`Especificación no válida` con `cs:N`; con `cs:N@<rep>` dice "No se han detectado merges"): se corre directo con `--merge`. |
| **Devolver (merge sustractivo)** | `cm merge cs:<changeset del préstamo> --subtractive --merge` y `cm checkin --all -c="…"` | Deshace justo lo que trajo ese changeset (el archivo editado vuelve a su revisión anterior, el agregado se borra: `Borrado (Subtractive de 26)`) sin tocar lo demás. Merge link tipo `cherrypicksubtractive`. Aquí el preview sin `--merge` sí funciona. |
| **Revertir la rama a su changeset inicial** (borrar progreso) | (a) **Con historial**: en el workspace de la rama, `cm merge cs:<cabeza> --subtractive --interval-origin=cs:<base> --merge` + `cm checkin --all -c="…"`; `<base>` es el changeset del que nació la rama (`parent` de su primer changeset: `cm find changeset "where branch='br:<rama>' on repository '<rep>'" --format="{changesetid} parent={parent}" --nototal`). (b) **Sin historial**: `cm changeset delete cs:<N>@<rep>` de la cabeza hacia atrás, uno por uno. | (a) `Borrado (Subtractive de 8 a 32)` y un changeset más que deshace todo. (b) Cada borrado da exit 0; al final `cm find changeset` de la rama no devuelve nada, la rama sigue existiendo y `cm find branch … --format="{changeset}"` vuelve a dar el changeset base. **Antes de (b) el workspace tiene que estar en otra rama** (se borra el changeset en el que estaría). No se puede borrar un changeset que sea origen de un merge, padre de otro, etiquetado ni `cs:0`. Para "borrar progreso": (b) si ningún changeset de la rama fue prestado a otra; si no, (a). |
| **Crear la rama hija llevándose los cambios sin check-in** ("En espera" → "Crear rama") | `cm branch create br:<padre>/<hija>@<rep> -c="…"` (nace de la cabeza del padre, donde está el workspace) y luego **`cm switch br:<hija> --noinput`** | Con cambios pendientes, `cm switch` sin `--noinput` **pregunta** (`1. Guardar mis cambios (crear un shelve) / 2. Llevar mis cambios / 3. Cancelar [por defecto]`): con la entrada cerrada toma "Cancelar" y no cambia nada; sin cerrarla se queda esperando. Con `--noinput` hace solo "Llevar mis cambios": crea un shelve temporal, cambia de rama y lo reaplica (`Creado shelve sh:2…`, `Aplicando el shelveset sh:2…`); los cambios quedan pendientes en la hija (`cm status`: `Reemplazado`, `Copiado (nuevo)`) y el shelve temporal desaparece solo. El `cm checkin --all` siguiente los guarda en la hija (`cs:34`, `parent` = cabeza del padre) y el padre queda sin changesets nuevos. Como la hija nace del changeset en que está el workspace, no hay nada que descargar ni conflicto posible. |
| **Changesets que le faltan al workspace** (incoming) | `cm status --header` en el workspace (`cm status --head` da solo la cabeza). Cuáles son: `cm find changeset "where branch='br:<rama>' and changesetid > <el del workspace> on repository '<rep>'" --format="{changesetid} {owner} {date} {comment}" --nototal` | Al día: `/main/dev/…padre@<rep> (cs:28 - head)`. Atrasado: `… (cs:27 - head:28)` (después de `head:` va la cabeza). `cm status --head` → `cs:28@<rep>`. Se reciben con `cm update .`. **No se puede unir uno solo** (probado 2026-11-16, #110): `cm update --changeset=cs:N` trae también todo lo anterior a `N`, y un cherry-pick del merge de una subtarea (`cm merge cs:N --cherrypicking --merge`) + `cm checkin` deja la rama padre con **dos cabezas** (el merge oficial queda colgado y aparece un check-in directo en la rama padre). Por eso los cambios entrantes se unen todos juntos. |
| **Dueño del repo y permiso `rmrepository`** | Dueño: campo `owner` de `GET …/repos` (API, ya probado). Permisos: `cm showacl rep:<rep>` (o `--extended`) | Lista cada permiso por grupo de Unity Cloud: `<id del proyecto>-admin`, `-rw` y `-ro`, y una fila `PROPIETARIO rmrepository Si`. Solo `-admin` y el PROPIETARIO tienen `rmrepository`; `-rw` no. `cm showacl repserver:<org>@unity` responde `No hay permiso para realizar la operación view` a quien no es dueño de la organización: se consulta el repo, no el servidor. |
| **Borrar una rama** (eliminar definitivamente) | `cm branch delete br:<rama>@<rep> --delete-changesets` | Funciona para una rama **no mergeada**, sin hijas y sin etiquetas. Falla con `No se puede borrar el changeset cs:25 porque es el origen de un merge. Deberá borrar el destino del merge (cs:26) primero.` si la rama ya entró a otra (también por préstamo), y con `No se puede eliminar la rama porque posee ramas hijas.` si tiene hijas. Sin `--delete-changesets` solo borra ramas vacías. **Conclusión para `eliminar_definitivamente`**: una rama mergeada o prestada no se puede purgar de Unity VC (habría que borrar changesets de la rama destino); se borra solo si no tiene merges de salida ni hijas, y si no, se queda en el historial. |
| **Permisos por rama** (#204, 2026-11-16) | `cm showacl br:/main/dev@<rep>`; para denegar: `cm acl --group=<id-proyecto>-rw -denied=+ci br:/main/dev@<rep>` (se quita con `-denied=-ci`); rutas seguras para varias ramas con `--branches=main,main/dev` | Hoy `/main` y `/main/dev` responden `ACL: 0`: no tienen entradas propias, todo lo heredan del repositorio por los grupos `<id>-admin` (todo), `<id>-rw` (`ci`, `mergefrom`, `mkchildbranch`…) y `<id>-ro` (casi todo denegado), más `PROPIETARIO rmrepository`. El Dashboard de Unity Cloud no tiene permisos por rama (y su apartado de permisos de Version Control no carga, probado varias veces): según la documentación de Unity se ponen con `cm acl` o desde la app de escritorio, y UVCS solo puede **denegar** lo que el rol de Unity ya concede. **Descartado (2026-11-17)**: no se prueba ni se usa `cm acl` por rama (denegar `ci` a `-rw` obligaría además a poner al Líder en `-admin` para que pueda hacer el merge a `dev` con su PAT). Ver [Protección de ramas](#protección-de-ramas-main-y-dev). |
| **Borrar un repositorio y reutilizar su nombre** (#206, 2026-11-16) | `cm repo rm rep:<Proyecto>/<Repo>@<org>@unity` y después `cm repo create <Proyecto>/<Repo>@<org>@unity` con el mismo nombre | Un repositorio vacío creado con `cm` desaparece de `cm repo list` **de inmediato** y el mismo nombre se puede crear otra vez al instante. Que no salga `__DELETED__` justo después no quiere decir que no vaya a salir: la prueba del 2026-11-17 (limitación 2 de abajo) vio que esa entrada puede aparecer horas después, no se purga sola y se acumula una por borrado, pero **no impide reutilizar el nombre**. |

### Limitaciones conocidas de Unity Version Control (`cm` / Unity Cloud)

Comprobadas en el entorno real contra `MiguelM17@unity` / `BranchBoard-Pruebas` (#82, #124–#126) y registradas para seguimiento en GitHub:

1. **No permite crear proyectos padre nuevos desde `cm` (`#205`)**: en Unity Organizations (`<org>@unity`), `cm repo create <Nombre>@<org>@unity` falla (`Repositories must be created under a project in Unity Organizations.`) y `cm repo create <ProyectoNuevo>/<Repo>@<org>@unity` falla (`El módulo no se puede crear porque el repositorio padre no existe.`). Tampoco existe comando en `cm showcommands --all` para crear proyectos padre. Solo se crean repositorios dentro de un proyecto existente en Unity Dashboard (selector agregado en `#126`).
2. **Repositorios eliminados persisten como `__DELETED__` (`#206`)**: tras `cm repo rm rep:<Proyecto>/<Repo>@<org>@unity`, `cm repo list <org>@unity` sigue listando `__DELETED__<Proyecto>/<Repo>|<org>@unity` y no existe comando `cm` para purgarlo definitivamente (depende de retención/Dashboard). Ya filtrado en `interpretar_repos_servidor`. **Comprobado 2026-11-17** (`BranchBoard-Pruebas/it206-221537`, cliente 11.0.16): la entrada `__DELETED__` **a veces tarda en aparecer** (justo después del borrado puede no salir y aparecer horas después) y **no se purga sola** en al menos varias horas; **se acumula una por cada borrado** del mismo nombre (tras tres borrados salen tres). **El nombre se puede reutilizar** aunque su `__DELETED__` siga en la lista: `cm repo create` con el mismo nombre exacto funciona y el repo nuevo sale normal al lado. Por eso `crear_proyecto` no necesita comprobar estas entradas (deja que `cm repo create` decida) y no hay nada que limpiar a mano: son solo ruido en `cm repo list`, que BranchBoard ya oculta.
3. **Caducidad de sesión SSO sin renovación headless (`#203`)**: cuando vence el JWT en `%LOCALAPPDATA%\plastic4\tokens.conf` (~1 h), las operaciones remotas de `cm` devuelven `Error: Controlador no válido.` y exigen volver a pasar por `plastic --configure` (navegador Unity ID). **Actualizado (2026-11-20)**: ahora "Reconectar" abre directo el inicio de sesión de Unity en el navegador (`sso_navegador.rs`); la duración del token es la misma que con el asistente. Además `cm repo list` sin `<org>@unity` devuelve el mismo error.
4. **Conflictos en `cm merge --merge` y cambios pendientes en `cm switch` (`#207`)**: con `FILE_CONFLICT`, `cm merge --merge` se bloquea esperando herramienta gráfica incluso con `--nointeractiveresolution` y entrada cerrada; y `cm switch` sin `--noinput` se cuelga ante cambios pendientes. BranchBoard siempre valida antes o redirige conflictos a Unity VC.
5. **Protección de ramas y permisos individuales en Unity Cloud (`#204`)**: `cm acl --user=...` falla y `PATCH /permissions` responde HTTP 200 sin bloquear check-ins directos a `/main` o `/main/dev`; Unity Cloud solo aplica permisos gruesos de proyecto (`-admin`, `-rw`, `-ro`) y su Dashboard no ofrece permisos por rama. **Aceptada (2026-11-17)**: los permisos de BranchBoard y los de UVCS son independientes; BranchBoard hace cumplir las reglas dentro de la app (`validar_destino_checkin`), convierte cada check-in directo en `main`/`dev` en una [tarea roja](#tarea-roja-por-check-ins-directos-en-main-o-dev-issue-225) que se traslada a su rama (#225) y avisa en pantalla de los cambios en una rama ajena en el equipo de quien tiene la app abierta (#111). Un check-in directo desde Plastic o `cm` no se puede impedir.

**Por qué no `/login` con usuario y contraseña**: el equipo entra a Unity por SSO (Unity ID/Google); el endpoint `/login` rechaza esas credenciales (`Invalid credentials`) aunque sean correctas. El **PAT** sí funciona directo como `Authorization: Bearer`, y se crea con la sesión SSO que `cm` ya tiene, sin pedir nada a la persona.

**Flujo al abrir la app**:
1. `cm whoami` → si no hay sesión, pantalla "Inicia sesión en Unity VC" ([arriba](#sesión-de-unity-vc-desde-la-app-proyectos-y-salir-según-el-frontend)) y se reintenta.
2. Buscar el correo en `usuarios` y verificar la [firma del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac) → rol.
3. ¿Hay PAT en el llavero del sistema (Windows Credential Manager, crate `keyring`) para ese correo + organización? Si no: `cm accesstoken create "BranchBoard (<máquina>)" <org>@unity --format={id}` + `cm accesstoken reveal <id>` → se guarda junto con su `id` para poder revocarlo. **Compatibilidad verificada 2026-11-16 (#107)**: en la máquina de Miguel (`11.0.16.8798`) esta sintaxis funciona y añadir `4w` produce servidor inválido. En la máquina de Draxx el comando sin vigencia devolvía ayuda. Se intenta primero sin vigencia y, solo si responde con la ayuda del comando sin ID válido, se reintenta una vez con `4w` antes del servidor. No se reintenta ante errores de red o permisos. Al vencer, el 401 renueva el PAT. El `id` no va en `configuracion_proyecto`, porque esa tabla se comparte con todo el equipo por eventos y el PAT pertenece a una persona en una máquina.
4. Probar el PAT (ej. `GET .../repos`). **401** → venció o se revocó: se borra del llavero, se revoca y se repite el paso 3 sin molestar a la persona, **una sola vez** (si el nuevo también da 401, es error `token_servidor_invalido`). Sin red no se borra ni se crea nada (error `servidor_unity_vc`).

**Implementado (2026-11-14, #159)** en `app/backend/src/servidor/`: `region` (URL base), `sanear` (borra `password`/`token`/`accessToken`/`refreshToken`/`secret` a cualquier profundidad antes de convertir a structs propios; los errores llevan solo el código HTTP y el `message`), `cliente` (`ClienteServidor`, HTTP con `ureq` desde Rust; el token no aparece en `Debug` ni en mensajes) y `pat` (`servidor::conectar(estado, llavero, email, org)` devuelve el cliente listo, con el PAT en el llavero bajo el servicio `BranchBoard` y la cuenta `<correo>|<org>`). Si `cm accesstoken create` falla, el mensaje sugiere que el Administrador de la organización puede tener que permitirlo (`cm accesstoken admin allowlist`). Prueba en el entorno real: `BB_PRUEBA_ORG=<org> cargo test entorno_real -- --ignored` (crea, reusa, fuerza un 401, renueva y al final revoca y borra todo). El botón "Reconectar" y cerrar sesión quedan para #124.

**Cerrar sesión** (`cerrar_sesion_unity_vc`): además de cerrar la sesión de Unity VC, `cm accesstoken revoke <id>` y borrarlo del llavero (la revocación **no es inmediata**: el token sigue sirviendo hasta que vence su sesión interna, 1 h por defecto según `cm accesstoken revoke --help`). **Eliminar un usuario**: su PAT vive en su máquina, así que la revocación real es sacarlo del proyecto en Unity Cloud (ya era necesario) — sin acceso al proyecto, su PAT deja de servir para ese repo.

**Reglas de seguridad** (obligatorias): ver [API REST del servidor de Unity VC: manejo de tokens en SEGURIDAD.md](./SEGURIDAD.md#api-rest-del-servidor-de-unity-vc-manejo-de-tokens-probado-2026-11-12) — en especial, `mergeto/report` devuelve el token de sesión de los autores en `owner.password`: toda respuesta se sanea y nunca se registra cruda.

<!-- ESTADO DE LO QUE FALTABA VERIFICAR (actualizado 2026-11-14, #159):
  1. Miembro sin rol de administrador creando PATs: decidido que no bloquea. Si le hace falta la
     lista permitida (`cm accesstoken admin allowlist`), la app muestra un error claro al crear el PAT.
     Se documenta en Usuarios cuando aparezca el caso.
  2. Duración del PAT: el servidor lo da con vencimiento 31/12/9999 (#159). El paso 4 lo renueva si da 401.
  3. Que el PAT sea de la persona de `cm whoami`: resuelto, porque se guarda por `<correo de whoami>|<org>`.
  4. Si otros endpoints filtran `owner.password`: sigue sin revisarse uno por uno, pero toda respuesta
     pasa por `servidor::sanear`, así que no hace falta para estar seguros.
-->

## Instalación de Unity Version Control

Mismo patrón ya usado para [Conexión VPN (Cloudflare WARP)](#conexión-vpn-cloudflare-warp): si `cm` no está instalado (sin importar el rol — **corregido**, tampoco se excluye al Colaborador externo, ver arriba), aparece un botón **"Instalar Unity Version Control"**.

- **Confirmado disponible en winget**: `winget install --id Codice.PlasticSCM.CloudEdition -e --accept-package-agreements --accept-source-agreements` — instala el cliente completo, que trae `cm` incluido (y con él, el modo Gluon para workspace parcial).
- Mismo fallback que WARP: si `winget` falla o no está disponible, el botón abre la página oficial de descarga de Unity Version Control (`https://www.plasticscm.com/download`) desde el backend en Rust. (**Implementado #123** en `app/backend/src/cm/instalacion.rs` y `comandos/cm.rs`: `estado_cm()`, `instalar_cm()`.)
- Misma nota de UX: puede disparar un prompt de UAC, mostrar estado "instalando..." mientras corre.
- **Probado de punta a punta en un equipo sin `cm` (2026-11-17, #220)**, en la máquina de drax2311 después de desinstalar el cliente: la app de escritorio mostró "Unity Version Control (cm) no está instalado" con el botón; al pulsarlo quedó en "Instalando Unity Version Control…", salió el aviso de UAC y `winget` dejó `C:\Program Files\PlasticSCM5\client\cm.exe` en unos 85 s. Sin cerrar la app, el aviso desapareció y siguió a la pantalla normal; `cm version` (11.0.16) y `cm whoami` respondieron con la sesión anterior, porque desinstalar no borra `%LOCALAPPDATA%\plastic4`. El caso "winget falla → página oficial" no se forzó; lo cubren las pruebas unitarias `winget_fallido_abre_pagina_oficial_de_uvcs` (`cm/instalacion.rs`) y `winget_fallido_o_ausente_abre_pagina_descarga` (`instalacion.rs`), que simulan winget fallido o ausente; `entorno_real_cm_instalacion` (#200) solo comprueba un `cm` ya instalado.
- **Para repetir la prueba** (desinstalar el cliente): winget lo ve como `ARP\Machine\X64\Unity Version Control <versión>`, no como `Codice.PlasticSCM.CloudEdition`. Lanzado así, el desinstalador (`C:\Program Files\PlasticSCM5\uninstall.exe`, de InstallBuilder) se quedó esperando sin mostrar ventana. Lo que funcionó fue correrlo elevado y sin preguntas: `Start-Process "C:\Program Files\PlasticSCM5\uninstall.exe" -ArgumentList "--mode","unattended" -Verb RunAs`. Si queda un desinstalador colgado, hay que cerrarlo como administrador antes de seguir, para que no actúe sobre la instalación nueva.

## Workspace parcial (Gluon) para subtareas simples

**Decisión final** (reemplaza el intento anterior de API REST + Service Account, descartado — ver nota abajo): Unity Version Control tiene un modo llamado **Gluon**, diseñado oficialmente para artistas que necesitan trabajar con archivos grandes sin descargar ni gestionar el proyecto completo. Trae su propio soporte de CLI: **`cm partial`** (`cm partial add`, `cm partial checkin`, etc.), así que el backend lo puede manejar por subproceso exactamente igual que el resto de comandos `cm` — el colaborador nunca necesita abrir una interfaz aparte.

- El Colaborador externo instala Unity Version Control (mismo botón de winget que todos, ver arriba) — es liviano, no implica tener Unity Editor. **Según el frontend**, trabaja solo **tareas individuales** (sin subtareas y que no sean subtarea de otra); el nombre de esta sección ("subtareas simples") viene del diseño anterior.
- **Definido**: la carpeta a descargar no la calcula el backend en automático — **el propio colaborador la elige**, desde un selector con las mismas categorías de [Categorías en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp): Audio, Modelado 3D, Modelado 2D, Imagen, Animación, Música, Video, UI/Interfaz (Scripting y Testing no aplican a este perfil, no son subtareas simples de archivo). El backend crea/usa un **workspace parcial** con `cm partial` scopeado solo a esa carpeta elegida — nunca descarga el proyecto completo, y solo trae la categoría en la que la persona va a trabajar, ni siquiera las demás carpetas de assets.
- **Identidad: vuelve a ser `cm whoami`, igual que todos los demás roles.** No hace falta ninguna Service Account, ni API REST, ni llave cifrada compartida, ni contraseña de equipo — todo ese diseño queda descartado (ver nota abajo).
- Esto también evita por completo el problema que motivó la idea de la Service Account: como cada colaborador usa su propia cuenta real (no una cuenta de servicio), no choca con ninguna limitación de permisos de Unity Cloud.

**Por qué se descartó la API REST + Service Account** (para que quede el porqué, no solo el qué): se comprobó en la práctica, directamente en el Dashboard de Unity Cloud, que **las Service Accounts no pueden asignarse a ningún rol/permiso de Unity Version Control** (ni los roles genéricos de proyecto, ni los Custom Roles de Version Control — se probó buscándola en el selector de usuarios de un Custom Role y no aparece como opción). Es una limitación real de la plataforma en este momento, no algo que dependiera de elegir mejor el rol. Gluon evita el problema de raíz porque no depende de ninguna cuenta de servicio.

**Aclaración (2026-11-12)**: lo descartado es la **Service Account compartida**, no la API REST en sí. La API REST **sí se adoptó** para las operaciones de servidor, pero autenticando con el **PAT personal de cada usuario** (su propia cuenta, con sus propios permisos del proyecto), así que no choca con esta limitación — ver [Acceso a la API REST del servidor de Unity VC](#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12). El Colaborador externo sigue usando Gluon (`cm partial`) para su workspace; su PAT se crea igual que el de los demás.

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
2. Comando Tauri `clonar_proyecto(repo, carpeta)`: corre `cm workspace create` + `cm update . --forcedetailedprogress --noinput` como subproceso, emitiendo el progreso por el evento `clonado-progreso`. (**Implementado #125** en `app/backend/src/cm/proyectos.rs`, `cm/ejecutor.rs` y `comandos/cm.rs`.)
3. Al terminar, dispara el mismo flujo de `cm workspace list` para que ese workspace nuevo aparezca disponible en la Pantalla inicial
4. **Según el frontend**: se llega aquí desde la Pantalla inicial (si no hay ningún workspace) o desde "Cargar proyecto". El frontend muestra una barra de progreso y, si el proyecto ya usa BranchBoard, llega con sus tareas.

## Comando "Abrir en Unity"

Botón visible dentro de la app una vez hay un proyecto cargado (ver [Panel "Abrir en Unity" en FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), solo para roles con workspace local.

- Comando Tauri `abrir_en_unity(tarea_id, rama, changeset?)`: cambia el workspace a esa rama (`cm switch`, o a un changeset concreto si se pide) y lanza el Editor de Unity (o Unity Hub) apuntando a la ruta del workspace, vía su propio CLI — mismo patrón de invocar un ejecutable externo ya usado con `cm` y `warp-cli`. Es local a la máquina, no pasa por `ops/`.
- **Según el frontend (2026-11-07)**: el botón de la barra superior abre el panel **"¿Qué tarea vas a trabajar en Unity?"** con el nombre y rol de quien lo usa, la rama en la que está el workspace (**nunca `dev`**; si no está en ninguna tarea, "No estás en ninguna tarea") y sus **tareas abiertas** (asignadas a él, no terminadas). También hay "Abrir en Unity" en los detalles de una tarea, en cada check-in (abre ese changeset) y en la pantalla de éxito al tomar una tarea. El backend tiene que exponer la rama actual del workspace (`rama_actual_workspace()`, de `cm status --header`).
- ~~Pendiente definir: comando exacto del CLI de Unity Hub para abrir un proyecto por ruta (verificar al implementar), y qué pasa si Unity Editor no está instalado (¿mismo patrón de "Instalar" con winget, o simplemente un mensaje de error?).~~ **Resuelto (#112, `app/backend/src/unity.rs`)**: prioriza `Unity.exe` de la versión declarada en `ProjectSettings/ProjectVersion.txt` (`-projectPath <ws>`) y cae a `Unity Hub.exe` (`-- --projectPath <ws>`) o error claro de validación si no está instalado; al abrir por rama devuelve `changeset: None` y al abrir un changeset puntual devuelve la rama real de la tarea (`rama_actual`, no `cs:<n>`). También admite subtarea `"En espera"` usando la rama de su padre.

## Logs locales al terminar una tarea (#153)

Decisión de Miguel, 2026-11-21: «Abrir en Unity» sigue disponible para revisar manualmente. Al pulsar «Terminar», antes del merge, `logs_unity_para_terminar(tarea_id)` lee únicamente el `Editor.log` del equipo de quien revisa. No inicia Unity, no compila y no recoge archivos del autor de la tarea.

El comando verifica el mismo permiso `autorizar_merge` y el plan de autorización de la tarea. La ruta es fija: Windows `<LocalAppData>/Unity/Editor/Editor.log`, macOS `~/Library/Logs/Unity/Editor.log`, Linux `~/.config/unity3d/Editor.log`. No acepta rutas del frontend. Devuelve nombre, fecha de modificación, contenido final (hasta 64 KiB leídos), indicador de recorte y aviso de lectura. Archivo ausente o vacío: sin registros; fallo de lectura: aviso, nunca una aprobación implícita. Los registros pueden ser de otra sesión o proyecto: se muestran su fecha y esa limitación, sin afirmar que validan la rama revisada.

Los logs son informativos: sin registros aparece «Terminar»; con registros o un fallo de lectura, «Terminar igualmente». Siempre se mantiene el candado explícito escribiendo `terminar`. Cancelar no cambia la tarea ni hace merge. Solo confirmar llama al `autorizar_merge` existente, con sus verificaciones de permisos, dependencias y conflictos. Se usa también al arrastrar a Terminada en Scrum y Categorías. Los logs no se guardan en SQLite, eventos ni Supabase, ni se comparten con otros usuarios.

## Diferencias de ramas y check-ins (#152)

Implementado en `src/diff.rs` y `comandos/diff.rs`. Se usa la API REST del servidor (decisión de 2026-11-12), aunque el texto original de #152 proponía `cm diff`. No hace falta un workspace ni cambiar de rama:

- `diff_changeset(tarea_id, hash)` — todos los roles verificados. El hash debe ser un número y pertenecer a los changesets de la rama de esa tarea antes de consultar `GET /repos/<repo>/diff/changeset/<n>`.
- `diff_rama(tarea_id)` — Líder/Administrador. `GET /repos/<repo>/diff/branch/<rama>` obtiene los cambios introducidos por la rama respecto de su base, también para subtareas.
- Ambos devuelven `{ archivos: [{ ruta, ruta_anterior, estado, binario, aviso, lineas }] }`, con `lineas: null` inicialmente. `diff_archivo(tarea_id, hash?, ruta)` devuelve ese archivo con su texto comparado, descargado al elegirlo. La ruta debe estar en el diff calculado por el backend; nunca se acepta una ruta del disco ni un ID de revisión suministrado por el frontend. Sin hash exige los permisos de `diff_rama`; con hash, los de `diff_changeset`.
- Cada revisión se lee con `GET /repos/<repo>/revisions/<id>/raw`. Los metadatos pasan por el saneador; el contenido versionado se trata como archivo, no como JSON de autenticación. Soporta UTF-8 y UTF-16 con BOM; binarios solo como aviso, sin descargarlos. Tope técnico por revisión: 1 MiB; hasta 20 000 líneas sumadas entre ambas versiones. Si se excede, se avisa. Para comparaciones grandes se acota la memoria conservando el prefijo y sufijo común y representando el centro como reemplazo. Los Xlinks a otro repositorio muestran aviso en vez de leerlo implícitamente.
- Solo lectura: no emite `ops/`, no crea préstamos ni escribe al workspace. El texto se renderiza como texto de React, sin interpretar HTML.

Verificado en `BranchBoard-Pruebas`: `diff/changeset/1` y `diff/branch/%2Fmain%2Fdev%2FT-101` devuelven grupos `Added` con `/LEEME.txt`, `type: TextFile`, `dstRev: 102`; `revisions/102/raw` devuelve 54 bytes UTF-8. Prueba automatizada: `diff::tests::entorno_real_diff_lectura` (solo lectura, con `BB_PRUEBA_ORG` y `BB_PRUEBA_REPO`).

El auxiliar `cargo run --example probar_diff_152` permite comprobar el IPC y el visor en una WebView Tauri real **oculta**, con una base temporal y la tarea local `T-152` apuntando a la rama de prueba existente `/main/dev/T-101`, sin workspace. Usa las mismas variables y un frontend en `http://127.0.0.2:4300`; solo registra los tres comandos de diff y `terminar_prueba` (exclusivo del auxiliar). Al terminar cierra la base, retira la carpeta temporal y revoca únicamente el PAT que haya creado. El binario de producción no expone ese comando auxiliar. La prueba Rust también revoca su PAT nuevo, conservando uno preexistente.

## Conexión VPN (Cloudflare WARP)

Problema real observado: en la red de la universidad, `cm` no logra conectar con el servidor de Unity Version Control (bloqueo de puerto). La solución probada en la práctica es activar **Cloudflare WARP** (cliente gratuito, `warp-cli`) — con la VPN activa, `cm` conecta sin problema.

Se agrega un **switch de encender/apagar WARP desde la pantalla inicial** de BranchBoard (ver [FRONTEND.md](./FRONTEND.md#autenticación--acceso-al-proyecto)), para no depender de que el usuario sepa buscar el ícono de WARP en la bandeja del sistema.

- **Comandos Tauri nuevos**: `estado_warp()` y `alternar_warp()` (o `activar_warp()` / `desactivar_warp()`).
- Internamente invoca `warp-cli status`, `warp-cli connect`, `warp-cli disconnect` como subproceso — mismo patrón que la integración con `cm`.
- **Detección de instalación**: si `warp-cli` no existe en la ruta esperada (Windows: `C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe`), el switch se muestra deshabilitado, y junto a él aparece un texto pequeño **"Instalar Cloudflare WARP"**.
  - Comando Tauri nuevo: `instalar_warp()` → corre `winget install --id Cloudflare.Warp -e --accept-package-agreements --accept-source-agreements` como subproceso (probado en la práctica, funciona). Al terminar, se vuelve a llamar `estado_warp()` para refrescar el switch.
  - **Fallback si falla o `winget` no está disponible**: el mismo texto abre en el navegador la página oficial de descarga (`https://1.1.1.1/download`) desde el backend en Rust, para que el usuario lo instale manualmente. (**Implementado #122** en `app/backend/src/warp.rs`, `instalacion.rs` y `comandos/warp.rs`.)
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

1. **Tarea creada por un Desarrollador o Tester esperando aprobación** — destinatarios: todos los usuarios con rol Líder o Administrador (ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)). También queda listada en el apartado Pendientes, no solo como notificación de campana. Se resuelve con un comando Tauri `aprobar_tarea(tarea_id)` que solo Líder/Admin puede invocar (validación de rol en backend), y que solo pone `pendiente_aprobacion = false` — **no toca el campo `estado`**, que ya venía avanzando por su cuenta desde que se creó la tarea (la tarea nunca estuvo bloqueada: se podía tomar y trabajar desde el primer momento, solo no se podía mandar a revisión). **Nuevo — 4 acciones posibles** sobre esta tarea, todas exclusivas de Líder/Admin (**implementado #101**, ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)):
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
8. **Recordatorio de inactividad en Unity Version Control**: si una tarea activa con rama asociada y responsable (en `"por hacer"` tras tomarse/asignarse o en `"en producción"`, incluidas las subtareas con rama propia y excluyendo los marcadores `"En espera"` y tareas padre delegadas en subtareas) no tiene **changesets nuevos** ni cambios de activación (`actividad_desde`) en **3 días** (`UMBRAL_INACTIVIDAD_DIAS = 3`), se le manda al dueño un recordatorio de tipo `inactividad` (`"Llevas X días sin actividad en [tarea], recuerda [qué le toca hacer]."`), deduplicado por ciclo de 3 días. Se apoya en el mismo mecanismo de [Detección de cambios](#detección-de-cambios-en-unity-version-control-polling), ejecutado automáticamente con el temporizador en segundo plano mientras la app está abierta (cada 15 s en `app/backend/src/lib.rs`, junto con `revisar_actualizaciones`). Pendiente definir solo si este umbral de 3 días se hace editable luego por Líder/Admin.
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

> **Actualizado (#241, #243)**: con el proyecto en el servidor estos comandos van por las funciones del servidor ([Usuarios y roles por el servidor](#usuarios-y-roles-por-el-servidor-241)); sin servidor solo funciona `listar_usuarios` (los demás responden `validacion`: "primero pásalo al servidor"). Ya no hay firmas ni `confirmar_usuario`, y los comandos no reciben `passphrase`. Lo de abajo describe el camino de antes, por `ops/`.

- `listar_usuarios()` — **todos los roles**; a quien no es Líder/Administrador solo se le devuelven nombre y rol (sin correo).
- `crear_usuario(email, rol)` — Líder/Administrador. **Sin contraseña** (decidido 2026-11-07): la identidad es `cm whoami`, igual que siempre. Valida el rol según quién invoca (`rolesAsignablesA` en el frontend): el Líder solo Desarrollador/Tester/Colaborador externo. La fila nace **firmada por quien la crea** (Administrador o Líder, ver [Firma de los Líderes](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)). **Actualizado (2026-11-12)**: `crear_usuario` **no invita a nadie en Unity Cloud** — la app no lo hace (no hay API para eso con el PAT y la invitación es del Dashboard). Hay que invitarlo aparte con el rol de proyecto que corresponde a su rol en BranchBoard (ver [Rol en Unity Cloud según el rol en BranchBoard en SEGURIDAD.md](./SEGURIDAD.md#rol-en-unity-cloud-según-el-rol-en-branchboard-2026-11-12)); sin eso, `cm whoami` le funciona pero no ve el repositorio y su PAT no sirve para ese proyecto.
- `cambiar_rol(email, rol_nuevo)` — **actualizado**: reemplaza a `ascender_a_lider`. Administrador: cualquier rol menos Administrador, sobre cualquiera menos otro Administrador (**con el proyecto en el servidor, #241, también nombra Administradores**: ver [Usuarios y roles por el servidor](#usuarios-y-roles-por-el-servidor-241)). Líder: solo entre Desarrollador/Tester/Colaborador externo, y lo firma él mismo con su llave — la persona sigue entrando y trabajando sin interrupción.
- `eliminar_usuario(email)` — Administrador: cualquiera; Líder: Desarrolladores y Colaboradores externos.
- `confirmar_usuario(email)` — exclusivo Administrador (con el proyecto en el servidor, #241, ya no existe); ahora solo hace falta para filas **sin firma** (escritas a mano o anteriores a este cambio), ver [Confirmación de usuarios nuevos](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac).
- `renombrar_proyecto(nombre)` — exclusivo Administrador; cambia el nombre en BranchBoard (clave `nombre_proyecto` en [`configuracion_proyecto`](#modelo-de-datos-tabla-configuracion_proyecto)), no el repositorio de Unity VC.
- `enviar_mensaje(para, texto)` — todos los roles; crea una notificación de tipo `mensaje` para `para` (máx. 500 caracteres). Es un aviso suelto desde el perfil, distinto del [Chat](#chat).
- `solicitar_accion_bd(motivo)` — Líder y Desarrollador (botón "Reportar problema con la base de datos" en Ajustes); notifica a todos los Administradores (notificación #9).
- `eliminar_proyecto()` — exclusivo Administrador, palabra = **el nombre exacto del proyecto**; ver [Verificación adicional sobre Unity Version Control](#verificación-adicional-sobre-unity-version-control-para-eliminar-el-proyecto).
  - **Implementación #128**: `eliminar_proyecto(confirmacion)` vuelve a comprobar el rol firmado, compara el nombre guardado en `nombre_proyecto` de forma **exacta** (incluidas mayúsculas, tildes y espacios), rechaza operaciones en curso y comprueba que `cm whoami` sea la misma persona. Lee `cm showacl --extended --xml` y cruza cada principal con el propietario de la API o sus miembros reales (`cm listusers <org>@unity --group=<grupo>`). Una concesión a un grupo ajeno no basta y cualquier denegación aplicable bloquea. ACL desconocida, error de lectura o pertenencia no comprobable → no se borra. Finalmente ejecuta `cm repo rm rep:<repo>` (Unity vuelve a exigir el permiso en el servidor) y cierra el proyecto solo si hubo éxito. Si el repo abierto cambió durante la comprobación, se cancela. El alcance es el repositorio de Unity VC: las copias en los workspaces y las carpetas compartidas quedan en disco; no se ejecuta ningún borrado de archivos del usuario. Unity puede retener el repositorio como `__DELETED__` (#206), que ya se filtra de las listas.
- **Según el frontend**: crear usuario devuelve error si el correo ya existe (sin distinguir mayúsculas); el nombre y apellido salen del correo (`ana.lopez@…` → "Ana" "Lopez"). Eliminar usuario pide escribir **"eliminar"**. `renombrar_proyecto` rechaza un nombre igual al de otro proyecto del mismo equipo.
- **Implementado (2026-11-10, issue #90)**: `listar_usuarios`, `crear_usuario`, `cambiar_rol`, `confirmar_usuario` y `eliminar_usuario` en `app/backend/src/comandos/usuarios.rs`; las reglas en `app/backend/src/identidad/equipo.rs`.
  - Cada fila nueva o cambiada se firma con la llave de quien hace el cambio y, antes de guardar, se comprueba que quede verificada. Si no (por ejemplo, un Líder sin certificado), no se guarda nada.
  - `listar_usuarios` devuelve `email` (vacío para quien no es Líder ni Administrador), `nombre`, `apellido`, `rol`, `rol_efectivo`, `verificacion` y `fecha_alta`.
  - Los comandos que firman reciben `passphrase` opcional: si la llave está cifrada y todavía no se abrió en la sesión, responden con el código `llave_cifrada` para que la interfaz pida la contraseña. Después la llave queda abierta en memoria hasta que cambia la persona.
  - `confirmar_usuario` solo firma filas **sin firma**; una fila cuya firma no coincide (posible rol escrito a mano) no se confirma, se corrige con `cambiar_rol`.
  - Nadie cambia su propio rol, nadie elimina al Administrador ni se elimina a sí mismo.
  - Al eliminar a un Líder, el Administrador vuelve a firmar en el mismo paso las filas que ese Líder había firmado (ver [Firma de los Líderes](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)). Al quitarle el rol de Líder no se borran su llave pública ni su certificado.
  - **Pendiente por definir — cómo recibe su certificado un Líder nuevo**: su llave se genera en su máquina (`generar_llaves_lider`), pero el Administrador la certifica en la suya. Falta decidir cómo llega esa llave pública al Administrador (por ejemplo, la app del Líder la publica en `ops/` la primera vez que entra como Líder y el Administrador la certifica). Mientras no tenga certificado, un Líder ve sus cambios rechazados con "Tu llave de firma no está certificada".
  - **Desde el issue #93 (2026-11-14)** cada uno sale como evento administrativo firmado a `ops/` y se aplica igual en todas las máquinas (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos), "firmar los eventos administrativos"). Necesitan la carpeta compartida configurada en esta máquina.

## Documentos del proyecto

**Nuevo (2026-11-07, según el frontend)** — ver [FRONTEND.md → Apartado del proyecto](./FRONTEND.md#apartado-del-proyecto-documentos). Archivos del proyecto (no versionados en Unity VC) que se guardan en la carpeta compartida del proyecto (ej. `proyecto/documentos/`) y viajan por Drive/OneDrive como el resto. **Con el proyecto en el servidor van a Supabase Storage** (#242, ver [Documentos e imagen en Storage](#documentos-e-imagen-en-storage-242)).

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

**Implementado (2026-11-16, issue #127)** en `app/backend/src/proyecto.rs` y `app/backend/src/comandos/proyecto.rs`. Lo que quedó:

- `subir_imagen_proyecto(ruta)` — **solo Administrador** (el Líder no; se queda como decía este diseño mientras nadie decida lo contrario). El formato sale del **contenido** del archivo, no de la extensión: PNG, JPG o WebP; más de **5 MB**, más de 8192 px de lado, o cualquier otro formato → `validacion`. Se **recorta al centro y se redimensiona a 256×256** (el ícono se ve en un círculo) y se guarda **siempre como PNG** con un nombre que genera el backend, `<carpeta compartida>/proyecto/icono-<uuid>.png`. La clave `icono_proyecto` guarda **solo ese nombre**, nunca una ruta; un valor que no sea `icono-<letras/números/guiones>.png` se rechaza al emitir y al aplicar. Al subir otra, la anterior se borra (Drive/OneDrive replica el borrado). Devuelve la imagen como `data:image/png;base64,…`.
- `obtener_configuracion_proyecto()` — **todos los roles**, local: `{ nombre, imagen }`. `imagen` es la `data:` URL, o `null` si no hay imagen, si esta máquina no configuró la carpeta compartida o si el archivo todavía no llegó por Drive/OneDrive. Antes de leer el archivo se valida de forma canónica que esté dentro de `proyecto/`.
- En las demás máquinas el evento se aplica solo si quien lo emitió es **Administrador verificado** (rol firmado, igual que `cerrar_sprint`); si no, o si la clave/valor no son válidos, se descarta.

Diseño original (2026-11-07): `subir_imagen_proyecto(ruta_archivo)` — Administrador (por definir si también el Líder). Copia la imagen elegida a la carpeta compartida del proyecto (ej. `proyecto/icono.png`, redimensionada a un tamaño fijo, p. ej. 256×256) y guarda la ruta en [`configuracion_proyecto`](#modelo-de-datos-tabla-configuracion_proyecto) con la clave `icono_proyecto`. Migra a eventos para que todas las máquinas la vean. Validar tipo (png/jpg/webp) y tamaño máximo, con la misma [validación de rutas](#validación-de-rutas-de-archivoadjuntos-resuelto-previene-path-traversal) que los adjuntos. Ver [FRONTEND.md → Apartado del proyecto](./FRONTEND.md#apartado-del-proyecto-documentos).

## Actualizaciones de la app

**Nuevo (2026-11-09)**. Quien ya tiene BranchBoard instalado recibe las versiones nuevas **sin desinstalar nada**, desde [Ajustes → "Actualizar versión"](./FRONTEND.md#ajustes). Usa el plugin oficial de actualización de Tauri.

- **Cómo se publica una versión**: se actualiza el número en `app/backend/tauri.conf.json`, `Cargo.toml` y `Cargo.lock`, `app/frontend/package.json` y `package-lock.json`, y `VERSION_MOCKUP` de `src/lib/api.ts`. Se sube una etiqueta `vX.Y.Z`. GitHub Actions (`.github/workflows/publicar-version.yml`) compila el instalador de Windows, lo **firma** con la llave privada de actualizaciones y crea la Release con el instalador, su firma (`.sig`) y `latest.json` (versión, notas, enlace y firma).
- **Cómo se actualiza la app**: consulta `https://github.com/MiguelMeza17/BranchBoard/releases/latest/download/latest.json`. Si la versión es mayor que la instalada, descarga el instalador, **verifica la firma** con la llave pública que va en `tauri.conf.json` y lo instala encima. Si la firma no coincide, no instala nada.
- **No se pierden datos**: el `.db`, los backups y las llaves viven en la carpeta de datos local, y `ops/` en Drive/OneDrive; nada de eso está en la carpeta de instalación.
- **Comandos** (locales, todos los roles): `buscar_actualizacion()` devuelve `{ version_actual, version_nueva, notas }` (`version_nueva` vacío = al día) y no descarga nada; `instalar_actualizacion()` descarga (evento `actualizacion-progreso` con `{ descargado, total }`), instala y reinicia la app. Se rechaza si hay una [operación en curso](#bloquear-el-cierre-de-la-app-durante-una-operación).
- **Nunca automático**: la app no busca ni instala sola; siempre lo pide la persona.
- **Llave de actualizaciones**: es **otra llave**, distinta de la de firma de roles. La privada está solo en la máquina de quien publica (`~/.tauri/branchboard-actualizaciones.key`) y como secreto `TAURI_SIGNING_PRIVATE_KEY` del repositorio; nunca se sube al repo. Si se pierde, las apps ya instaladas no pueden actualizarse solas: hay que reinstalar una vez con un instalador firmado con la llave nueva.
- **Un solo instalador para todos los proyectos**: ver el cambio en [Firma criptográfica del rol](#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac).

**Comprobación reproducible (#133)**: desde `app/backend`, `cargo run --example probar_actualizaciones_133 --locked` comprueba el rechazo mientras hay una operación en curso, descarga la versión pública `v0.1.0` con el plugin real y verifica su firma con la llave configurada. Después altera un byte y exige que la firma impida ejecutar el instalador. No instala, no abre proyectos ni cambia sesiones. La prueba de instalación se hace aparte: instalar `0.1.0` en una carpeta de pruebas, actualizar desde Ajustes a `0.1.1`, comprobar el reinicio y que conserve una base SQLite y sus archivos de respaldo. Nunca usar proyectos reales para esa prueba.

## Base de datos local y migraciones

**Implementado (2026-11-09, issue #74)** en `app/backend/src/db/`.

- **Dónde vive**: un `.db` por proyecto en la carpeta de datos local, `<datos>/proyectos/<id del proyecto>/branchboard.db` (nunca en Drive/OneDrive). El id se limpia para que no pueda salirse de esa carpeta.
- **Migraciones**: lista ordenada en `db/migraciones.rs`; la versión del esquema se guarda en `PRAGMA user_version`. Al abrir el `.db` se aplican las que falten, cada una en su transacción. Una migración publicada **no se edita**: los cambios van en una nueva al final.
- Si el `.db` es de una versión más nueva de la app, no se abre y se pide actualizar BranchBoard.
- Borrar el `.db` y volver a abrir lo crea de cero (después se reconstruye desde `ops/`).
- Modo WAL y llaves foráneas activas.

## Copias de seguridad y restauración de la BD

**Backup automático**: cada vez que se abre BranchBoard, antes de tocar el `.db`, se copia a una carpeta local de backups (ej. `%LocalAppData%\BranchBoard\backups\branchboard_2026-09-25.db`). Se conservan solo las **últimas 10 copias** — al llegar a la 11, se borra la más vieja. No requiere acción de nadie, corre solo.

**Restaurar backup** — **exclusivo del Administrador** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)):
- Comando Tauri `listar_backups()` (lista las copias disponibles con fecha) y `restaurar_backup(nombre_archivo)`, ambos validados para que solo un Administrador los invoque.
- Dado lo delicado de la acción (descarta todo lo escrito después del backup elegido), usa el mismo patrón de doble candado que [eliminar una tarea](#eliminar-una-tarea): confirmación + escribir la palabra **"restaurar"** (definida en el frontend) antes de habilitar el botón. Cada copia muestra fecha, tamaño y, si es la que se guardó al restaurar otra, su nota.
- **Actualizado (2026-11-07) — "Actual" y guardar antes de restaurar**: la lista siempre muestra arriba la base en uso como **"Actual"** (fecha de hoy, no restaurable). `restaurar_backup` primero copia el `.db` actual como un backup más (con nota "Guardada antes de restaurar la copia del …") y luego aplica el elegido, así siempre se puede volver al estado anterior. Esa copia previa no cuenta para el límite de 10 hasta que se restaure otra.
- Un Líder o Desarrollador que detecte un problema no puede restaurar directamente — le llega a los Administradores vía la notificación **"Solicitud de acción sobre la base de datos"** (ver [Notificaciones](#notificaciones), punto 9).

**Implementado (2026-11-10, issue #79)** en `app/backend/src/db/backups.rs` y `comandos/datos.rs`:
- Las copias van en `<carpeta del proyecto>/backups/branchboard_<AAAA-MM-DD_HH-MM-SS>.db` (junto al `.db`, una carpeta por proyecto; si hay dos en el mismo segundo, `_02`, `_03`…). Se hacen con `VACUUM INTO`, así la copia queda consistente aunque haya un `-wal` sin volcar.
- `Db::abrir_con_respaldo(carpeta)` es lo que debe usar "entrar a un proyecto" (#89): copia antes de abrir y migrar. Sin `.db` previo (proyecto nuevo) no hay copia.
- La nota de una copia vive en un archivo `.nota` al lado (`branchboard_….nota`). Al podar, la copia **con nota más reciente** no cuenta para el límite de 10; en cuanto se guarda otra con nota, la anterior pasa a contar como una copia normal.
- `listar_backups()` devuelve `{ actual, copias }`: `actual` es la fila "Actual" (`id = "actual"`, fecha de ahora, tamaño del `.db` + `-wal`, no restaurable) y `copias` va de la más reciente a la más antigua, cada una `{ id (nombre del archivo), fecha "AAAA-MM-DD HH:MM", tamano "2.4 MB", nota? }`.
- `restaurar_backup(nombre)`: valida que `nombre` sea una copia existente (sin rutas ni `..`), guarda la base actual con la nota "Guardada antes de restaurar la copia del …", cierra la base, copia el archivo encima del `.db` (borrando `-wal`/`-shm`) y la vuelve a abrir pase lo que pase. Se rechaza si hay una [operación en curso](#bloquear-el-cierre-de-la-app-durante-una-operación), y cuenta como una mientras dura. El chequeo "solo Administrador" se conecta cuando exista el de rol (#87).

## Modelo de datos: tabla `usuarios`

| Campo | Tipo | Notas |
|---|---|---|
| `email` | texto (PK) | viene de `cm whoami`, ver [Identidad de usuario](#identidad-de-usuario) — no hay contraseña que guardar |
| `nombre` | texto | al crear el usuario se toma del correo (`ana.lopez@…` → Ana), editable después |
| `apellido` | texto | ídem |
| `rol` | enum | Administrador / Líder / Desarrollador / Tester / Colaborador externo |
| `fecha_alta` | fecha | cuándo quedó registrado en el proyecto |

**Implementado (2026-11-09, issue #75)** en `app/backend/src/db/usuarios.rs`. **Actualizado (#241, #243)**: con el proyecto en el servidor es la copia de `miembros`. Hasta #243 guardaba además la firma del rol (`firma`, `firmado_en`, `firmado_por`) y la llave y el certificado de los Líderes (`llave_publica`, `certificado`); la migración local 14 quita esas columnas.

Borrar una fila de esta tabla = [eliminar a esa persona del equipo](#identidad-de-usuario) (revoca acceso de inmediato, sin sesión que cerrar).

## Firma criptográfica del rol (ancla de confianza para el RBAC)

**Quitada en #243 (2026-11-21, decisión 3 de #231).** Mientras el equipo compartía los datos por Drive/OneDrive, cualquiera con la carpeta podía escribir su propio rol, así que cada rol iba firmado (Ed25519) por el Administrador o por un Líder certificado, y los cambios de equipo y los snapshots de `ops/` viajaban firmados (issues #85, #86, #88, #93 y #94). Con el servidor eso ya no hace falta:

- **Los roles los guarda y los hace cumplir el servidor**: la tabla `miembros` solo la cambian sus funciones, que revisan el rol de quien pide (#234, #241), y los eventos que un rol no puede mandar se rechazan en el servidor (#247). La tabla `usuarios` local es una copia de `miembros`.
- **El ancla de confianza es el dueño de la cuenta de Supabase** (ver [SEGURIDAD.md](./SEGURIDAD.md#reglas-del-servidor-supabase-234)), no una llave en la PC del Administrador.

Qué se quitó:
- `identidad/llaves.rs` (`admin.key`, `admin.pub`, `lider.key`, su cifrado con Argon2id + XChaCha20-Poly1305 y el comando `exportar_llave_firma`) e `identidad/firmas.rs` (firma de filas, certificados de Líder, `Verificador`). También las dependencias `ed25519-dalek`, `argon2` y `chacha20poly1305`.
- `firma_admin` de `Evento` y la verificación al leer `ops/`; `confirmar_usuario` y el estado "pendiente de confirmar"; el error `llave_cifrada`.
- La compactación con snapshots (`eventos/snapshot.rs`, `compactar_eventos`).
- La contraseña de los proyectos sin Unity VC (decisión 4 de #231): crear y abrir ya no la piden.
- Columnas `firma`, `firmado_en`, `firmado_por`, `llave_publica` y `certificado` de `usuarios` (migración local 14).

Qué cambió:
- El rol de alguien es el de su fila en `usuarios` (`permisos::usuario_actual`, `tareas::rol_de`).
- En un proyecto que todavía va por Drive/OneDrive, los cambios de equipo ya no se aceptan por `ops/` y la pantalla Usuarios solo lista: para cambiar el equipo hay que pasarlo al servidor (#245).
- "Desde cuándo cuentan los check-ins directos" (#225) era la hora de la firma del primer Administrador; ahora es el inicio (UTC) del día de su `fecha_alta`.
- Un proyecto creado antes conserva sus archivos `admin.key`/`admin.pub`, pero ya no se leen.

## Verificación adicional sobre Unity Version Control para eliminar el proyecto

**Contexto**: se evaluó anclar también el rol de Administrador al **Owner** real de la organización en Unity Cloud (dato que Unity controla, no editable desde el `.db`). Se investigó a fondo (documentación pública + inspección del propio Unity Dashboard) y **no es viable como fuente de autorización en tiempo real**:
- No existe una API pública/documentada (ni en `services.docs.unity.com` ni en el CLI `cm`) para preguntar "¿qué user type tiene este email en la organización?".
- El dato sí existe, pero lo sirve un endpoint interno del dashboard (`services.unity.com/api/plastic/v1/organizations/{id}/profiles`) atado a la sesión del navegador — no pensado para consumo de terceros, sin SLA, y no aplicable a un backend de escritorio sin esa sesión.

**Alternativa real y sí soportada**: en vez de leer el "Owner" abstracto, apoyarse en el sistema de permisos de **Unity Version Control** que la app ya usa vía `cm` — concretamente el permiso `rmrepository` ("controla si el repositorio puede eliminarse"), visible y configurable hoy mismo en el Unity Dashboard (`Version Control > Roles personalizados` / `Permisos de usuario`).

- Antes de ejecutar `eliminar_proyecto()`, el backend corre `cm showacl` sobre el repositorio del proyecto y confirma que el email de `cm whoami` (quien invoca la acción) tiene `rmrepository` concedido (no denegado) en Unity Version Control.
- Queda como **doble candado** sobre la acción más destructiva de la app: hace falta (a) que la firma local diga `rol = Administrador` **y** (b) que Unity Version Control conceda `rmrepository` sobre el repo real — alguien que solo comprometa el `.db` local no puede borrar el proyecto si Unity no se lo permite también.
- **Verificado #128 en `BranchBoard-Pruebas`**: `showacl --extended --xml` expone `ExtendedAclInfo`, las reglas por principal y la herencia; `listusers --group` permite comprobar al usuario actual en el grupo `-admin` sin inferirlo por su nombre. Se evalúan los permisos `rmrepository`/`all`; ante una denegación aplicable se rechaza conservadoramente, incluso si también aparece una concesión. `proyecto::eliminacion::tests::entorno_real_verificar_eliminacion` solo comprueba autorización (no borra nada), con `BB_PRUEBA_ORG` y `BB_PRUEBA_REPO`. El auxiliar `examples/probar_eliminacion_128.rs` exige una autorización explícita y un repo nuevo `BranchBoard-Pruebas/bb-prueba-128-*`; nunca acepta un repo preexistente ni el repositorio original de pruebas.
- **Prueba positiva #128, autorizada por Miguel**: en WebView Tauri real oculta se rechazó el nombre incorrecto y el candado exacto invocó `eliminar_proyecto` contra el repo nuevo `BranchBoard-Pruebas/bb-prueba-128-20261117-01@MiguelM17@unity`. ACL y pertenencias comprobadas, `cm repo rm` exitoso y proyecto cerrado. Al salir: repo ausente de la lista activa (solo la marca `__DELETED__` esperada por #206), ningún PAT sobrante, mismos dos workspaces y seis ramas originales del repositorio principal de pruebas. No se borraron archivos locales ni compartidos.
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
| `rama_asociada` | nullable | `null` hasta que alguien "toma" la tarea; **ruta completa** de la rama en Unity VC: `/main/dev/<id>-<título>` para una tarea general y `<rama del padre>/<id>-<título>` para una subtarea (ej. `/main/dev/T-101-combate/T-101-1-arma`). **Corregido (2026-11-14, #83)**: antes decía `task/<id>-<nombre>`, que Unity VC no permite (la `/` separa niveles) |
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

**Implementado — crear, editar, listar y tomar tareas (2026-11-15, issues #95 y #96)** en `app/backend/src/tareas.rs`, `comandos/tareas.rs` y `eventos/lectura.rs`:
- `listar_tareas()` (todos los roles): devuelve todas las filas de `tareas` con `edicion_pendiente` calculada; `ListaTareas.tsx`, `ScrumBoard.tsx` y `CategoriaScrum.tsx` recargan automáticamente al entrar y cada vez que `useSincronizacionEventos` incrementa `versionDatos`.
- `crear_tarea(datos)` y `crear_tarea_con_subtareas(datos)` (todos los roles): publican los eventos `crear_tarea` y `crear_tarea_con_subtareas` en `ops/` (y los aplican en la misma transacción). `pendiente_aprobacion = false` si quien crea es Líder o Administrador, y `true` para Desarrollador, Tester y Colaborador externo. Rechazan títulos vacíos, el tipo interno `Sin definir`, un padre en la Papelera o superar el límite de 5 niveles de ramas (máximo 2 ancestros en `tareas`).
- `editar_tarea(id, campos)`: permitido a cualquier **Líder o Administrador** sobre tareas no eliminadas, y al **creador** sobre su propia tarea mientras siga `pendiente_aprobacion = true`. Rechaza cambiar directamente de un `sprint_id` (o `backlog_id`) a otro sin antes quitarla (`null`). Publica el evento `editar_tarea` en `ops/`.
- `tomar_tarea(id)` (todos los roles; Colaborador externo solo en tareas individuales — ni subtareas ni tareas con subtareas activas): revisa primero que la tarea esté en `'por hacer'` sin responsable y que el workspace no tenga cambios sin check-in (`ramas::cambios_pendientes`); crea por la API REST las ramas que falten en la cadena desde la tarea general hasta `id` (`ramas::crear_ramas_de_cadena`, cada una desde la cabeza actual de `/main/dev` o de su padre con el comentario `Rama de <id>: <título>`); ejecuta `cm switch` en el workspace local hacia la rama de `id` (`ramas::cambiar_a_rama`); y publica el evento `tomar_tarea` en `ops/` poniendo `asignado_a`, `modo_trabajo = 'rama'`, `rama_asociada` y `ultimo_changeset_visto` en `id`, y `rama_asociada`, `modo_trabajo = 'rama'` y `ultimo_changeset_visto` (sin `asignado_a`) en los ancestros que recibieron su rama. Al reproducir el evento en otras máquinas se revalida el rol y que la tarea siga libre.
- **Doble toma simultánea (2026-11-15, issue #97)** en `tareas.rs`, `eventos/lectura.rs`, `comandos/eventos.rs` y `comandos/tareas.rs`: al sincronizar `ops/`, si un `tomar_tarea` choca con otra toma sobre la misma tarea, se comparan los eventos en `ops/` por `(timestamp, usuario, archivo)` para determinar quién ganó (incluso si la máquina perdedora aplicó primero su propio evento). Antes de revertir, `revisar_y_cerrar_rama_perdedor` consulta en Unity VC los changesets de la rama del perdedor:
  - Sin changesets reales: borra la rama huérfana (si es distinta a la del ganador), revierte la tarea en SQLite al ganador e inserta la notificación al perdedor con `"Volver mi workspace a dev"` (`accionable: true`, comando `volver_a_dev`).
  - Con changesets reales: no revierte ni borra la rama, y notifica a ambos usuarios y a todo Líder/Administrador verificado.

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

**Implementado (2026-11-10, issue #78)** en `app/backend/src/db/commits.rs` (migración 5). `commits` no repite un changeset dentro de la misma tarea: volver a guardar el mismo `hash` reemplaza la copia (siempre refleja lo último leído de `cm log`), y borrar la fila de la tarea borra sus copias. En `cambios_prestados`, `archivos` y `archivos_tocados` se guardan como JSON, `quitado_en` y `quitado_por` van siempre juntos (lo exige la tabla) y una tarea no puede prestarse cambios a sí misma. Las demás tablas del mismo issue: [`notificaciones`](#notificaciones) (`db/notificaciones.rs`; `leida` es local, el tipo se valida contra la lista del frontend), [`documentos`](#documentos-del-proyecto) (`db/documentos.rs`; la `ruta` interna es única y no se envía al frontend), el [chat](#chat) (`db/chat.rs`: `mensajes_chat`, `grupos_chat` con `miembros` en JSON, y las locales `chat_leidos` y `chats_eliminados` — un privado se lee igual desde los dos lados, una conversación eliminada solo muestra lo posterior a esa fecha y `conversaciones_sin_leer` cuenta conversaciones, no mensajes) y la local `paneles_vistos` de los [puntos de aviso](#puntos-de-aviso-del-menú) (`db/vistos.rs`, huella por usuario y panel). Las tablas locales llevan columna `usuario` para que la huella, los leídos y los chats eliminados de una persona no se mezclen con los de otra en la misma máquina.

**Implementado (2026-11-14, issue #84)** en `app/backend/src/checkins.rs`: `leer_checkins_rama(cliente, conexion, repo, tarea_id, rama)` lee los check-ins por la **API REST** (`GET …/changesets?branchName=<rama>`) en vez de `cm log`, como manda la tabla de [Acceso a la API REST](#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12) ("Listar ramas/changesets"): no hace falta tener un workspace del repo, y quien revisa puede leer los de una rama ajena. De cada changeset se guarda: `hash` = número de changeset; `mensaje` = primera línea no vacía del comentario (`Sin comentario` si no hay); `descripcion` = el resto (`null` si no hay); `autor` = `owner` (el correo de la cuenta de Unity, en minúsculas); `fecha` = la `date` del servidor (UTC) pasada a hora local `AAAA-MM-DDTHH:MM:SS`, como la usa el frontend. No duplica: el mismo changeset reemplaza la copia **conservando su `id` y su `tipo`** (el tipo lo elige quien hace check-in desde BranchBoard y no está en el servidor). Una rama que todavía no existe en el servidor da una lista vacía. Prueba en el entorno real: `BB_PRUEBA_ORG=<org> BB_PRUEBA_REPO=<Proyecto/Repo> cargo test --lib entorno_real_checkins -- --ignored` (crea una rama, le hace dos check-ins de verdad, los lee y la borra con `cm branch delete --delete-changesets`).

## Modelo de datos: tabla `configuracion_proyecto`

Tabla clave/valor para ajustes del proyecto entero. **Actualizado (2026-11-07)**: `correo_contacto_solicitudes` ya no se usa (solicitar ingreso pasa a la versión 2); la primera clave real es `nombre_proyecto` (ver `renombrar_proyecto`). Originalmente no había ninguna tabla de configuración a nivel proyecto en el modelo de datos. Clave/valor simple, pensada para que cualquier ajuste futuro de este tipo (no por tarea, no por usuario, sino del proyecto entero) tenga dónde ir sin agregar una tabla nueva cada vez:

| Campo | Tipo | Notas |
|---|---|---|
| `clave` | texto (PK) | ej. `nombre_proyecto` |
| `valor` | texto | |
| `actualizado_por` | email | |
| `fecha_actualizacion` | fecha | |

**Implementado (2026-11-16, issue #127)**: `renombrar_proyecto(nombre)` y `actualizar_configuracion_proyecto(clave, valor)` en `app/backend/src/proyecto.rs`, los dos solo Administrador y por el evento `actualizar_configuracion_proyecto`. Reglas:
- **Clave**: minúsculas, números y `_`, empieza por letra, máximo 64. `icono_proyecto` no se cambia por el comando genérico (solo con `subir_imagen_proyecto`, que crea el archivo).
- **Valor**: máximo 1000 caracteres. `nombre_proyecto` se recorta, no puede quedar vacío, tener saltos de línea ni pasar de 80 caracteres; renombrar al mismo nombre da `validacion`.
- `fecha_actualizacion` guarda el `timestamp` del evento (ISO 8601 UTC); gana el último evento en el orden de `ops/`.
- Al entrar por primera vez, `nombre_proyecto` = nombre del repositorio de Unity VC (#89). **Falta en el frontend**: que el menú y la lista de proyectos muestren este nombre en vez del del workspace, y la regla de "no repetir el nombre de otro proyecto del equipo", que hoy solo comprueba el mockup.

Comando Tauri `actualizar_configuracion_proyecto(clave, valor)` — exclusivo Administrador. **Migra a eventos** (agregarlo a la lista en [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)) — de lo contrario cambiar un ajuste del proyecto en una máquina no se reflejaría en las demás.

## Pendiente por definir
- ~~Cómo se valida el permiso de rol en cada comando Tauri (mecanismo exacto de autorización, ej. middleware/decorador vs. chequeo manual al inicio de cada comando)~~ — **resuelto (2026-11-10, issue #87)**, implementado en `app/backend/src/identidad/permisos.rs`:
  - **Una sola tabla**, `PERMISOS`, dice qué roles pueden usar cada comando (`Libre`, o una lista de roles). Tauri no tiene *middleware* para comandos, así que cada comando que depende del rol empieza con `estado.exigir_permiso("nombre_del_comando")?`.
  - **Negado por defecto**: un comando que no está en la tabla se niega siempre. Una prueba compara la tabla con los comandos registrados en `lib.rs` y falla si falta alguno o si sobra uno.
  - **Usuario actual** = el correo de `cm whoami` (lo guarda `AppState::fijar_usuario`; lo conecta el issue #89 al entrar al proyecto) + su rol **verificado por la firma** contra la llave del Administrador que esta máquina fijó (`admin.pub`). Firma que no coincide → se le aplica Desarrollador; sin fila o pendiente de confirmar → `sin_permiso`.
  - Errores: `sin_permiso` si el rol no alcanza, `sin_sesion_unity_vc` si no hay usuario, `validacion` si no hay proyecto abierto o esta máquina no tiene la llave del Administrador.
  - Lo que depende del dato concreto (ej. "el creador sobre su tarea pendiente", "quien tiene la tarea padre") lo revisa cada comando después, con el usuario que devuelve `exigir_permiso`. En la tabla, esos comandos llevan los roles que *pueden llegar* a usarlos.
  - Hoy están en la tabla los comandos que ya existen: `listar_backups`, `restaurar_backup` y `exportar_llave_firma` (solo Administrador); `ping`, `hay_operacion_en_curso`, `buscar_actualizacion` e `instalar_actualizacion` (libres). Cada comando nuevo se agrega con los roles de la tabla "Contrato con el frontend".
- Estados del flujo de una tarea (ya definidos en la práctica: por hacer / en producción / en revisión / terminada, más la bandera aparte `pendiente_aprobacion` — falta solo confirmarlos como lista cerrada)
- ~~Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)~~ — **resuelto (2026-11-14, #84)**: se leen por la API (`GET …/changesets?branchName=`), ver [tablas `commits`](#modelo-de-datos-tablas-commits-y-cambios_prestados).
- ~~Comando `cm` exacto para leer si una rama está cerrada/mergeada~~ — **resuelto (2026-11-14, #82)**: no hay estado de cierre, se mira el merge link con `cm find merge`; junto con el resto de comandos que estaban "pendientes de verificar" (merge a la rama padre y conflicto, préstamo y devolución, revertir la rama, rama hija con cambios pendientes, incoming, `showacl` y borrar ramas), ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14).
- ~~Intervalo exacto de escaneo de la carpeta `ops/`~~, ~~estrategia de compactación/purga de `ops/`~~ y ~~qué comandos migran al mecanismo de eventos~~ — **corregido**: los tres quedaron resueltos más arriba en este mismo documento (ver [Sincronización entre máquinas](#sincronización-entre-máquinas-arquitectura-de-eventos)); esta sección no se había actualizado después de resolverlos.
- **Diferencias con el frontend que quedan por decidir** (2026-11-07): si se agrega o se descarta "Mover cambios" y `editar_tarea_ajena`; si en Revisión solo el revisor asignado puede autorizar; si se notifica el rechazo de una edición; el borrado en cascada de subtareas (diseñado aquí, el mockup no lo hace); cómo se trabajan las tareas sin ramas en los proyectos sin Unity VC (la carpeta, la contraseña, exportar e importar ya están resueltos, ver [Proyectos sin Unity VC](#sesión-de-unity-vc-desde-la-app-proyectos-y-salir-según-el-frontend)).
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
   - se crea **su rama** (`<rama del padre>/<id>-<título>`) y los cambios sin check-in **se mueven a ella** (en Unity VC, **verificado 2026-11-14, #82**: `cm branch create br:<padre>/<hija>@<rep>` desde la cabeza del padre y `cm switch br:<hija> --noinput`, que se lleva los cambios pendientes con un shelve temporal; ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14));
   - desde ahí se trabaja como cualquier rama: check-ins, revisión y merge hacia el padre (que llegará como incoming change).
3. "En espera" **no puede traer cambios** ([Cambios prestados](#cambios-prestados-traer-cambios)): sus cambios todavía están sin guardar.
4. Mientras haya "En espera", la tarea padre no puede pasar a revisión (cuenta como subtarea pendiente).

### Click en cada elemento: detalle y acciones

Cada fila de las tres secciones abre su detalle al hacer click, con **"Abrir en Unity"** y su acción propia:

| Sección | Detalle | Acción propia |
|---|---|---|
| **Pending changes** | Rama, quién trabaja, cuántos cambios y los archivos (`cm status`), y qué pasará según la rama | Rama sin subtareas: **"Hacer check-in"** (`cm checkin`), con **título, descripción y tipo** (título y descripción van en el comentario del changeset). Rama con subtareas: **"Crear rama"** (nombre, detalles y tipo) |
| **Cambios entrantes** (incoming changes) | Subtarea, rama de origen, fecha, quién la trabajó y los check-ins que trae | **"Unir todo"** (`unir_cambios_entrantes`, `cm update .`): llegan **todos juntos**, no se puede unir una subtarea sola. El detalle de cada una avisa qué más llega. Opcional |
| **Check-ins de la rama** | Mensaje, autor, fecha, changeset | Solo "Abrir en Unity", en ese changeset |

### Cambios entrantes (incoming changes)

- Cuando una **subtarea pasa a "terminada"**, su rama se mergea a la rama del padre en el servidor. Quien tiene el workspace en la rama padre todavía no tiene esos cambios: en Unity VC son **incoming changes**.
- **Solo cuenta lo terminado**: una subtarea en revisión todavía no se ha mergeado, así que no sale. Si alguien necesita su trabajo antes, lo trae como [préstamo](#cambios-prestados-traer-cambios).
- BranchBoard lo muestra en los detalles del padre, en la sección **"Cambios entrantes"** (aparece en **toda rama con subtareas**, en 0 si no hay ninguno; las ramas sin subtareas no la tienen, y una tarea terminada nunca la muestra porque ya se mergeó y su rama está cerrada): qué subtarea, cuándo terminó, cuántos check-ins trae y desde qué rama.
- **Es solo un aviso y recibirlos no es obligatorio**: no llegan solos al workspace (ni al abrir la rama en Unity). Se reciben desde **"Traer cambios"**, donde salen **primero y en azul**, en su propio apartado "Incoming changes — llegaron a esta rama", con un solo botón **"Unir todo"** (misma confirmación escribiendo "traer"), o desde la sección o el detalle del incoming change. En Unity VC es actualizar el workspace hasta la cabeza de la rama (`cm update .`): **se unen todos juntos** (decidido con Miguel el 2026-11-16, #110), porque Unity VC no deja unir una subtarea sola (ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14)). **No es un préstamo**: ya son de esta rama, no se registran en `cambios_prestados` ni hay que devolverlos.
- Se calcula **por máquina**: depende del workspace de cada uno (changeset en el que está el workspace contra el último de la rama). Es un indicador local, no pasa por `ops/`.
- **Implementado #110** en `app/backend/src/entrantes.rs`: `cambios_entrantes(tarea_id)` lee `cm status --header` (`cs:N - head:M`), los changesets de la rama posteriores a `N` (`cm find changeset`) y los merges que llegaron a ella (`cm find merge … dstbranch`, solo tipo `merge`: un préstamo no cuenta) para atribuir cada uno a su subtarea. Si el workspace está en otra rama devuelve `en_rama = false` y la interfaz pide cambiar de rama. `unir_cambios_entrantes(tarea_id)` exige estar en la rama y sin cambios sin check-in, y corre `cm update . --noinput`. Los dos son para todos los roles (actúan sobre el workspace de quien los usa).

**Pendiente de definir al implementar**: ~~comando `cm` exacto para crear la rama hija llevando los cambios pendientes~~ y ~~comando `cm` exacto para saber qué changesets de la rama le faltan al workspace (incoming)~~ — **verificados (2026-11-14, #82)**: `cm switch br:<hija> --noinput` y `cm status --header` (`(cs:27 - head:28)` cuando falta algo) + `cm find changeset "where branch='br:<rama>' and changesetid > <N> …"`, ver [Comandos `cm` verificados](#comandos-cm-verificados-en-el-repo-de-prueba-issue-82-2026-11-14). Siguen pendientes: cómo se atribuye la "En espera" si quien hizo los cambios no es el responsable de la tarea padre, y excluir de la cuenta los archivos que Unity modifica solo (ej. `.meta`).

## Contrato con el frontend: acciones y comandos

**Nuevo (2026-11-07)**. Cada acción del store del frontend (`app/frontend/src/store/useStore.ts`) y el comando Tauri que la respalda. "Local" = no pasa por `ops/`.

**Cómo llama el frontend (2026-11-09)**: todo pasa por `app/frontend/src/lib/api.ts`. Cada comando tiene ahí una función tipada que usa `llamar(comando, args, mockup)`: dentro de Tauri hace `invoke`; en el navegador (`npm run dev`) responde con su versión de mockup, así el mockup sigue funcionando sin backend. Los errores llegan siempre como `ErrorBackend` con `{ codigo, mensaje }`. Al implementar un comando nuevo se agrega su función en `api` y la acción del store la usa.

| Acción del frontend | Comando Tauri | Quién |
|---|---|---|
| "Crear un nuevo proyecto" sin Unity VC (`AccesoProyecto`) | `crear_proyecto_local` | libre (todavía no hay proyecto) |
| "Ingresar a un proyecto existente" sin Unity VC | `abrir_proyecto_local` / `listar_proyectos_locales` | libre; abrir exige la contraseña del proyecto |
| "Importar .bbproyecto…" | `importar_proyecto_local` | libre |
| Ajustes → "Exportar proyecto" | `exportar_proyecto_local` | Administrador (solo proyectos sin Unity VC) |
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
| `trasladarCambiosDirectos` | `trasladar_cambios_directos(id)` (solo Líder/Administrador) | ver [Tarea roja](#tarea-roja-por-check-ins-directos-en-main-o-dev-issue-225); la tarea llega con `cambios_directos` en `listar_tareas` |
| `unirEntrantes` | `cambios_entrantes` / `unir_cambios_entrantes` (local) | quien tiene el workspace |
| `abrirEnUnity` | `abrir_en_unity` (local) / `listar_commits` (local) | todos con workspace |
| `crearSprint` / `cerrarSprint` | `crear_sprint` / `cerrar_sprint` | Líder/Admin / solo Administrador |
| `asignarTareaASprint` / `asignarTareaABacklog` / `asignarBacklogASprint` | `asignar_tarea_a_sprint` / `asignar_tarea_a_backlog` / `asignar_backlog_a_sprint` | Líder/Admin |
| `crearUsuario` / `cambiarRol` / `eliminarUsuario` / `confirmarUsuario` | `crear_usuario` / `cambiar_rol` / `eliminar_usuario` / `confirmar_usuario` | ver [Comandos de usuarios](#comandos-de-usuarios-y-ajustes) |
| `renombrarProyecto` / `restaurarBackup` | `renombrar_proyecto` / `restaurar_backup` (local) | Administrador |
| `obtenerConfiguracionProyecto` / `actualizarConfiguracionProyecto` / `subirImagenProyecto` | `obtener_configuracion_proyecto` (local) / `actualizar_configuracion_proyecto` / `subir_imagen_proyecto` | todos / Administrador / Administrador |
| `enviarMensaje` / `solicitarAccionBd` | `enviar_mensaje` / `solicitar_accion_bd` | todos / Líder y Desarrollador |
| `listarDocumentos` / `subirDocumentos` / `aprobarDocumento` / `eliminarDocumento` / `abrirDocumento` | `listar_documentos` / `subir_documentos` / `aprobar_documento` / `eliminar_documento` / `abrir_documento` (local) | todos / todos / Líder/Admin / Líder/Admin / todos |
| `enviarChat` / `crearGrupoChat` / `marcarChatLeido` / `eliminarChat` | `enviar_chat` / `crear_grupo_chat` / `marcar_chat_leido` (local) / `eliminar_chat` (local) | todos |
| `marcarLeida` / `desmarcarLeida` / `marcarTodasLeidas` | igual (local) | todos |
| `marcarVisto` / `obtenerVistos` | `marcar_visto` / `obtener_vistos` (local) | todos |
| `cerrarSesion` / `setSesionUnityVC` / Reconectar | `cerrar_sesion_unity_vc` / `iniciar_sesion_unity_vc` / `sesion_unity_vc` / `reconectar_unity_vc` | todos |
| `entrarAProyecto` / `irAClonar` / `cambiarDeProyecto` | `entrar_a_proyecto` / `clonar_proyecto` / (vuelve al acceso) | todos |
| `crearProyectoNuevo` | `crear_proyecto` / `traer_proyecto` | crea repositorio bajo proyecto padre de Unity Cloud + workspace + `/main/dev` (o configura workspace local sin DB), y deja al dueño como Administrador |
| "Crearlo en Unity Cloud" / "Recargar proyectos" (modal Nuevo proyecto) | `abrir_unity_cloud(org?)` (local, abre el navegador) / `listar_proyectos_servidor` | todos, sin proyecto abierto (#205) |
| `toggleVpn` | `estado_warp` / `alternar_warp` / `instalar_warp` | todos |
| Ajustes → "Actualizar versión" (`lib/api.ts`) | `buscar_actualizacion` / `instalar_actualizacion` (locales) | todos |
| Ajustes → "Carpeta compartida" (`lib/api.ts`) | `carpeta_compartida` / `configurar_carpeta_compartida` (locales) / `compactar_eventos` | todos / todos / Administrador |

Las acciones `setRolActual` ("Ver como"), `setEmail` y `toggleCmSesion` son **solo del mockup** (demo) y no tienen comando: en la app real el rol viene de la tabla `usuarios` firmada y la identidad de `cm whoami`.
