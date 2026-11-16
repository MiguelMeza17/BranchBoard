# Instrucciones para Claude en este repositorio

## Sin datos reales en el repositorio

El repositorio es **público** (desde el 2026-11-09). Los correos de `documentacion/SEGURIDAD.md` ("Usuarios semilla") y de `app/frontend/src/data/fixtures.ts` son **de ejemplo** (`@virustf.dev`); los reales se quitaron. **No escribir correos reales, llaves ni datos personales** en el código ni en la documentación.

## Estructura del repositorio

- `documentacion/` — toda la documentación de diseño del producto (ver detalle abajo).
- `app/backend/` — backend en **Rust + Tauri** (SQLite, `cm`, eventos en `ops/`). **En implementación desde el 2026-11-09**: el diseño está en `documentacion/BACKEND.md` y el orden de trabajo en la hoja de ruta (issue #129, issues con la etiqueta `backend`).
- `app/frontend/` — código React/TypeScript/Vite/Tailwind real (mockup funcional). Sigue siendo la referencia de lo que el producto hace: el backend implementa lo que el frontend necesita (ver "Contrato con el frontend" en `documentacion/BACKEND.md`).

## Flujo de trabajo de la documentación

Varias sesiones de Claude trabajan en paralelo en `documentacion/`, cada una sobre su propio documento:
- `documentacion/PLANNING.md` — visión general y conceptos compartidos (punto de choque entre sesiones, no editar dos sesiones a la vez)
- `documentacion/BACKEND.md` — lógica de backend (Rust + Tauri)
- `documentacion/FRONTEND.md` — interfaz (React + TypeScript + Tauri)
- `documentacion/SEGURIDAD.md` — seguridad y roles/permisos
- `documentacion/ARQUITECTURA.md` — vista de pájaro de cómo encajan los componentes (monolito de escritorio Fase 1); no duplica detalle, indexa hacia `BACKEND.md`/`FRONTEND.md`
- `documentacion/pendientes.md` — lista viva de huecos/bugs de diseño encontrados en auditorías, con su resolución
- `documentacion/AGENTE.md` — **a futuro**: el agente que enseñará y responderá dudas dentro de la app. La interfaz no lleva textos explicativos (solo nombres y datos); todo texto que *explique* algo va ahí, como base de conocimiento del agente, en la sección de su pantalla

Antes de editar `documentacion/PLANNING.md`, revisar `git status` — si tiene cambios sin comitear de otra sesión, evitar editarlo al mismo tiempo. Los demás documentos (`BACKEND.md`, `FRONTEND.md`, `SEGURIDAD.md`) incluyen una sección "Contexto completo del proyecto" que resume las decisiones clave para que cualquier sesión tenga contexto sin necesitar leer los demás archivos — mantenerla sincronizada cuando cambien decisiones importantes (stack, roles, flujo de identidad, etc.).

**Siempre que se toque diseño de interfaz** (una pantalla nueva, un flujo, un componente, resolver un punto de "Pendiente por definir", etc.) **se debe actualizar `documentacion/FRONTEND.md`** para que quede reflejada la decisión — no dejarlo solo implementado en el código de `app/frontend/`.

## Flujo de ramas del repositorio de código (este repo, no el producto)

**No confundir con el modelo `dev`/`main` que BranchBoard-el-producto usa para gestionar tareas de un proyecto Unity** (eso está documentado en `PLANNING.md`/`BACKEND.md`, es lo que la app gestiona para otros). Esto es sobre cómo el equipo versiona el **código fuente de BranchBoard mismo**, una vez empiecen a implementar:

- **Hay rama `dev`** (creada el 2026-10-15 desde `main`) — es la rama de integración: los PR de features apuntan a `dev`, no a `main`.
- Cada feature/cambio vive en su propia rama, creada desde `dev`.
- Una rama se cierra **únicamente mediante un Pull Request**, y ese PR solo se mergea **si ya se resolvieron todos los conflictos antes** — nunca se mergea un PR con conflictos pendientes.
- **`dev` pasa a `main` con un PR de `dev` → `main`** (definido el 2026-10-19, primera vez en el PR #17): se abre con `gh pr create --base main --head dev`, se revisa que no tenga conflictos (`gh pr view --json mergeable,mergeStateStatus` debe dar `MERGEABLE`) y se mergea con merge commit (`gh pr merge <número> --merge`). **Nunca con `--delete-branch`**: `dev` es permanente y no se borra. El momento lo decide el equipo cuando quiera llevar lo acumulado en `dev` a `main`.

### Backend y frontend

- **El backend ya se implementa** (desde el 2026-11-09), un issue a la vez según la hoja de ruta (#129). Cada rama sale de `dev` con el número del issue (ej. `feat/95-crear-tareas`) y su PR hacia `dev` lleva `Closes #<número>`.
- **Antes de empezar a tocar un issue, se asigna** a quien lo va a trabajar: el desarrollador que lo toma, o, si lo trabaja una sesión de Claude, el desarrollador dueño de esa sesión (`gh issue edit <número> --add-assignee <usuario>`). Si el issue ya tiene a otra persona asignada, no se toca sin hablarlo con ella.
- **Ningún issue de backend se da por resuelto sin probarlo en el entorno real** (ver la sección siguiente). Los tests unitarios no bastan.
- Si al implementar algo del backend cambia o se completa el diseño (un comando `cm` exacto, una regla nueva, un pendiente que se resuelve), se actualiza `documentacion/BACKEND.md` en el mismo PR.
- El mockup del frontend tiene que seguir funcionando en el navegador (`npm run dev`) sin backend: las llamadas a comandos de Tauri pasan por un solo cliente que, fuera de Tauri, usa los datos en memoria.
- Todo cambio de frontend debe quedar reflejado en la documentación de backend correspondiente (`documentacion/BACKEND.md` y/o `documentacion/ARQUITECTURA.md`) para que, cuando se implemente el backend real, no haya que redescubrir qué asume el frontend — ver también la regla de mantener `documentacion/FRONTEND.md` sincronizada, arriba.

### Pruebas en el entorno real de Unity VC (obligatorio)

Todo el desarrollo del backend se prueba contra el repositorio de pruebas de Unity Version Control:

- **Repositorio**: `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` (organización `MiguelM17@unity`). Para comprobar el acceso: `cm repo list MiguelM17@unity`.
- **Quiénes lo usan**: los dos desarrolladores de BranchBoard, **MiguelMeza17** (dueño de la organización) y **drax2311**. Ambos son administradores del repositorio de pruebas y ambos ya han trabajado en él, así que esta regla vale igual para los dos y para la sesión de Claude de cualquiera de ellos. Cada uno prueba desde su máquina con su propia sesión de `cm` (`cm whoami` debe responder; si no, iniciar sesión en Unity VC).
- **Cómo se prueba**: el código se ejecuta de verdad contra ese repositorio (comandos `cm`, API REST y, cuando el issue toca la interfaz, la app con Tauri). El workspace de prueba va en una carpeta fuera de este repositorio de GitHub y nunca se sube.
- **Qué va en el PR**: una sección "Prueba en el entorno real" con lo que se ejecutó y lo que pasó. Las casillas de "Hecho cuando" del issue solo se marcan después de esa prueba. Si algo no se pudo probar (por ejemplo, un caso que necesita una cuenta sin rol de administrador), se dice en el PR y el issue no se cierra como completo (`Refs #N` en vez de `Closes #N`).
- **Nunca** se prueba en repositorios de proyectos reales (por ejemplo VirusTF).
- Al copiar salidas a issues o PRs: sin correos ni tokens. Las respuestas del servidor pueden traer tokens (ver `documentacion/SEGURIDAD.md`), así que se sanean antes de pegarlas.

### Cómo se está construyendo el backend

Lo que ya existe (2026-11-14) y las reglas que se siguen al agregar algo nuevo:

- **Dónde va cada cosa** (`app/backend/src/`):
  - `cm/`: todo subproceso pasa por `cm::Comando` (argumentos separados, nunca un texto de shell; tiempo máximo; bloquea el cierre de la app mientras corre). `cm/lectura.rs`: `whoami`, workspaces y rama actual.
  - `servidor/`: API REST de Unity VC. `region` (URL de la organización, de `cloudregions.conf`), `sanear` (borra `password`/`token`… de toda respuesta), `cliente` (HTTP con el PAT) y `pat` (`servidor::conectar(estado, llavero, email, org)` crea el PAT, lo guarda en el llavero de Windows y lo renueva si da 401).
  - `ramas.rs`: nombre `<id>-<título>`, crear ramas (también la del padre que falte), límite de 5 niveles, nunca check-in a `main`/`dev`, `cm switch`.
  - `checkins.rs`: check-ins de una rama → tabla `commits`.
  - `identidad/`: llaves, firmas, permisos, equipo y `acceso` (`entrar_a_proyecto`).
  - `db/`: SQLite. `comandos/`: lo que llama el frontend.
- **API o `cm`**: lo que pasa en el **servidor** (crear ramas, merge, listar ramas y changesets, dueño del repo) va por la API REST con `servidor::conectar`. Lo que toca el **workspace** de quien trabaja (`cm whoami`, `cm status`, `cm switch`, `cm checkin`) va por `cm`. **Nunca** `cm configure`: pisa la sesión de la persona.
- **Primero se prueba en vivo, después se programa**: antes de escribir el código, se corre el comando o el endpoint a mano contra BranchBoard-Pruebas para ver la respuesta real (sin imprimir tokens ni correos), y lo comprobado se anota en `documentacion/BACKEND.md`. Así salieron, por ejemplo, que `task/<id>` no se puede crear (la `/` es un nivel) y que `cm switch` con cambios pendientes se queda preguntando qué hacer (con `--noinput` se los lleva) y que cualquier `cm merge --merge` con conflicto se cuelga esperando la herramienta de merge, aun con `--nointeractiveresolution` (los comandos verificados están en la tabla "Comandos `cm` verificados" de BACKEND.md).
- **Código que se puede probar sin red**: cada llamada a `cm` o a la API pasa su salida a una función pura (`interpretar_*`, `planear_cadena`, `asegurar_pat`, `acceso::entrar` con `es_dueno` como función) que se prueba con la salida real copiada como ejemplo.
- **Seguridad**: toda respuesta de la API pasa por `servidor::sanear` antes de usarse, nunca se registra cruda, y ni el PAT ni el token aparecen en `Debug` ni en mensajes de error.
- **Decisiones**: si el texto de un issue choca con una decisión más nueva de `documentacion/BACKEND.md`, gana la más nueva y se explica en el PR (ej. #84 lee por la API y no con `cm log`). Lo que decide el usuario durante un issue se anota en BACKEND.md y en el issue al que le toca (ej. la rama del padre que falte, anotada en #96).
- **Frontend al día**: si una decisión del backend cambia lo que asume el mockup (nombres de rama, flujo de acceso), se cambian también `app/frontend` y `documentacion/FRONTEND.md` en el mismo PR.

### Cómo se prueba cada cosa

1. **Pruebas unitarias**, en `app/backend`: `cargo test`, `cargo clippy --all-targets` (sin avisos) y `cargo fmt`.
2. **Frontend**, en `app/frontend`: `npx tsc --noEmit` y `npm run build`.
3. **Pruebas en el entorno real**: son tests `#[ignore]` llamados `entorno_real_*`. Corren contra BranchBoard-Pruebas con la sesión de `cm` de quien los ejecuta:

   ```
   cd app/backend
   BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
     BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive \
     cargo test --lib entorno_real -- --ignored --nocapture --test-threads=1
   ```

   | Prueba | Qué comprueba |
   |---|---|
   | `servidor::pat::…::entorno_real_crear_usar_y_renovar_el_pat` | Crea el PAT, lo guarda en el llavero, la API responde, lo reusa, y con un token dañado (401) lo renueva |
   | `ramas::…::entorno_real_ramas_de_tareas` | Rama desde la cabeza de `dev`, subtarea y nivel 5, rechazo del nivel 6 y de la repetida, rama del padre que falte, `cm switch` y bloqueo por cambios sin check-in |
   | `checkins::…::entorno_real_checkins_de_una_rama` | Dos check-ins de verdad (uno con descripción) se leen con título, descripción, autor y hora, sin duplicar |
   | `eventos::…::entorno_real_eventos_en_la_carpeta_compartida` | Publica 3 eventos en una subcarpeta de la carpeta de OneDrive/Drive de verdad y comprueba, 20 s después, que siguen intactos (sin temporales, copias en conflicto ni renombres) y anotados como aplicados; que la marca `branchboard-proyecto.json` quedó, que otra máquina del mismo proyecto sin rol de Administrador la acepta y que una carpeta sin marca se le rechaza. Necesita además `BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive`; borra su subcarpeta al terminar |
   | `eventos::lectura::…::entorno_real_dos_maquinas_por_la_carpeta_compartida` | Dos máquinas simuladas (cada una con su base y su carpeta local) sobre la misma subcarpeta de OneDrive/Drive: A (Administrador) publica 3 eventos, uno con el reloj adelantado 2 h; B (no Administrador) elige la misma carpeta, los aplica en el mismo orden y registra el del futuro en `eventos.log`; B publica y A recibe solo eso. Las dos terminan iguales. Usa `BB_PRUEBA_CARPETA_COMPARTIDA`; borra su subcarpeta. **Falta** la prueba con dos equipos de verdad (MiguelMeza17 y drax2311) |
   | `identidad::equipo::…::entorno_real_eventos_administrativos_firmados` | Dos máquinas simuladas sobre una subcarpeta de OneDrive/Drive: A (Administrador, con su llave) crea un usuario y le cambia el rol como eventos firmados; B (sin llave) los aplica y queda igual. B deja en `ops/` dos eventos falsos para hacerse Líder (sin firma, y firmado con una llave que generó él): A y B los descartan, el rol no cambia y queda en `eventos.log`. Usa `BB_PRUEBA_CARPETA_COMPARTIDA`; borra su subcarpeta |
   | `eventos::snapshot::…::entorno_real_compactar_eventos_y_reconstruir` | Dos máquinas simuladas sobre una subcarpeta de OneDrive/Drive: A (Administrador) publica eventos de más de 30 días y un evento reciente, ejecuta `compactar`, comprueba que en `ops/` quedaron solo `snapshot_<fecha>.json` y el evento reciente (los antiguos se borraron), y B reconstruye su `.db` desde cero partiendo del snapshot más el evento reciente y llega al mismo estado que A; además descarta un snapshot falso sin firma. Usa `BB_PRUEBA_CARPETA_COMPARTIDA`; borra su subcarpeta |
   | `comandos::acceso::…::entorno_real_entrar_a_proyecto` | Con la cuenta **dueña**: configura el proyecto y entra como Administrador; la segunda vez entra sin configurar. **Falta** correrla con la cuenta de drax2311 (no dueña), que debe dar `esperar_dueno` |
   | `tareas::…::entorno_real_crear_editar_y_listar_tareas_por_ops` | Máquina A entra de verdad a `BranchBoard-Pruebas` en un workspace temporal (`cm workspace create` + `entrar_en`), configura una subcarpeta en OneDrive, crea una tarea general con subtareas como Desarrollador (quedan pendientes), la edita siendo el creador y el Líder crea otra tarea (aprobada); máquina B sincroniza desde OneDrive y `listar` devuelve la misma lista. Borra el workspace y la subcarpeta al terminar |
   | `tareas::…::entorno_real_tomar_tarea_crea_rama_cambia_workspace_y_sincroniza` | Máquina A entra de verdad a `BranchBoard-Pruebas`, crea una tarea general con una subtarea, llama a `tomar_en` sobre la subtarea, comprueba que se crearon en Unity VC las ramas del padre y de la subtarea desde la cabeza de `dev`, que el workspace local cambió (`cm switch`) a la rama de la subtarea, que el padre quedó con `rama_asociada` sin `asignado_a` y la subtarea con `asignado_a`, `modo_trabajo = 'rama'` y `rama_asociada`, y que la máquina B sincroniza desde OneDrive y llega al mismo estado. Borra el workspace, las ramas de prueba y la subcarpeta al terminar |
   | `tareas::…::entorno_real_doble_toma_resuelve_sin_y_con_changesets` | Contra `BranchBoard-Pruebas` + OneDrive, simula dos tomas simultáneas en dos casos: (1) sin changesets en la rama perdedora (se borra la rama huérfana en Unity VC, revierte la toma al ganador y notifica al perdedor `"Volver mi workspace a dev"`) y (2) con un `cm checkin` real en la rama perdedora (conserva rama y estado y avisa a ambos + Admin). Borra workspace, ramas (`--delete-changesets`) y carpeta al terminar |
   | `tareas::…::entorno_real_asignar_y_soltar_tarea` | Contra `BranchBoard-Pruebas` + OneDrive, comprueba permisos de asignación (sin subtareas, no terminada, Desarrollador rechazado), asigna creando ramas en Unity VC sin mover el workspace local y notificando al responsable en máquina B, reasigna conservando la rama, suelta sin check-ins (`desasignar_en`) y rechaza soltar tras un check-in real. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_cambiar_estado` | Contra `BranchBoard-Pruebas` + OneDrive, prueba pendiente de aprobación bloqueando revisión, ramas/check-in reales, estado del padre calculado de abajo hacia arriba, aviso de préstamos al ir a revisión, bloqueo de Desarrollador/Tester hacia terminada, cascada hacia atrás con rechazo en `PorHacer` si hay check-ins y sincronización con notificaciones `tarea_a_revision` en máquina B. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_borrar_progreso` | Contra `BranchBoard-Pruebas` + OneDrive, prueba `borrar_progreso_en` (cambios pendientes, check-ins y préstamos revertidos y ruta `cm merge --subtractive` con cherrypick hacia otra rama) y `reabrir_en` (hasta 3 veces por Líder, 4ta rechazada para Líder y permitida para Administrador), sincronizando `PorHacer`, `EnRevision` y `tarea_reabierta` en máquina B. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_aprobar_editar_y_advertir` | Contra `BranchBoard-Pruebas` + OneDrive, crea tareas pendientes como Desarrollador, verifica rechazo de permisos a Desarrollador y ejecución por Líder de `advertir_pendiente_en`, `editar_y_aprobar_en`, `aprobar_tarea_en` y `aprobar_tareas_en`, confirmando que máquina B recibe las aprobaciones/edición y al creador le llegan `advertencia` y `editada_aprobada`. Borra workspace y carpeta al terminar |
   | `tareas::…::entorno_real_ediciones_pendientes` | Contra `BranchBoard-Pruebas` + OneDrive, rechaza `proponer_edicion_en` en `EnProduccion`, propone y suma dos ediciones en `'por hacer'` (conservando valores anteriores originales incluso `detalles` vacíos), verifica notificación accionable `edicion_pendiente` en máquina B, rechaza permisos a Desarrollador y prueba `rechazar_modificacion_en`, `aprobar_modificacion_en` y `aprobar_modificaciones_en` por Líder con sync final en máquina B. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_eliminar_y_purgar` | Contra `BranchBoard-Pruebas` + OneDrive, crea padre con subtarea activa, subtarea terminada e independiente en Unity VC, ejecuta `eliminar_tarea_en` (cascada a activas sin tocar terminada, orden de Papelera más reciente primero y `tarea_eliminada` en máquina B) y `eliminar_definitivamente_en` (borra filas y purga ramas replicando en B). Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_polling_unity_vc` | Contra `BranchBoard-Pruebas` + OneDrive, detecta check-in externo y rama actual en `revisar_actualizaciones_en`, luego realiza un `cm merge` real de subtarea a rama padre (sin tocar `/main/dev`), comprueba el cierre automático a `Terminada` con `tarea_cerrada_por_deteccion` y su sincronización en máquina B. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_hacer_checkin` | Contra `BranchBoard-Pruebas` + OneDrive, prueba la guardia de rama incorrecta en `cambios_pendientes_en` y `hacer_checkin_en` (bloqueando fuera de la rama o sin permiso / en rama con subtareas y ofreciendo `cambiar_a_rama_tarea_en`), lista cambios sin check-in (`cm status`), ejecuta `hacer_checkin_en` con título, descripción y tipo conservando `tipo` al releer con `revisar_actualizaciones_en` y sincroniza el `Commit` vía OneDrive en máquina B. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_subtarea_en_espera` | Contra `BranchBoard-Pruebas` + OneDrive, simula cambios sin check-in directo en la rama de una tarea con subtareas: `revisar_actualizaciones_en` crea la subtarea `"En espera"` (`SinDefinir`), bloquea mandar al padre a revisión (`cambiar_estado_en`), replica por OneDrive en máquina B, valida la guardia de rama en `crear_rama_desde_espera_en`, crea la nueva rama hija (`cm switch --noinput` llevándose los cambios pendientes), hace `hacer_checkin_en` en la hija, y prueba también el retiro automático de `"En espera"` si se limpian los cambios. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_cambios_prestados` | Contra `BranchBoard-Pruebas` + OneDrive, prueba `traer_cambios_en` (regla de check-in previo en `EnProduccion`, guardia de rama, préstamo puntual por changeset y préstamo completo multi-commit por intervalo `--cherrypicking --interval-origin`), verifica `changeset_merge` + archivos en `cm diff`, bloqueo de `Terminada`, `devolver_cambios_prestados_en` con preview anti-conflicto y merge sustractivo real (`cm merge --subtractive --merge`) conservando filas como historial y sincronizando por OneDrive en máquina B. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_cambios_ajenos_tocados` | Contra `BranchBoard-Pruebas` + OneDrive, prueba detección automática de archivos prestados tocados sin check-in (`revisar_actualizaciones_en`), detección en check-in real (`hacer_checkin_en` y polling desde máquina B), bloqueo de `Terminada` y `devolver_cambios_prestados_en` mientras existan `archivos_tocados`, guardia de rama y restauración con `cm getfile` + check-in automático (`eliminar_cambios_ajenos_en`), comprobando que polling posterior no remarque los commits ya limpiados y que máquina B sincronice via `ops/` hasta devolver el préstamo y terminar la tarea. Borra workspace, ramas y carpeta al terminar |
   | `tareas::…::entorno_real_alerta_rama_ajena` | Contra `BranchBoard-Pruebas` + OneDrive, comprueba que `revisar_actualizaciones_en` ignora cambios exclusivos `.meta`, alerta sin deshacer nada en disco cuando hay cambios sin check-in en `/main/dev` o en la rama de una tarea ajena (sugiriendo la rama propia asignada), alerta ante check-in directo del propio usuario en una rama ajena y no genera alerta en la rama propia. Borra workspace, ramas y carpeta al terminar |
   | `sprints::…::entorno_real_sprints_y_backlogs` | Contra `BranchBoard-Pruebas` + OneDrive, entra con workspace temporal y `entrar_en`, ejerce `crear_sprint_en`, `listar_sprints_en`, `asignar_tarea_a_backlog_en`, `asignar_tarea_a_sprint_en` y `asignar_backlog_a_sprint_en` (fecha de fin calculada, rechazo a Desarrollador, exclusividad de un sprint por tarea y paso masivo solo de tareas libres) y verifica la sincronización en máquina B y el descarte de eventos falsos vía OneDrive. Borra workspace y carpeta al terminar |
   | `sprints::…::entorno_real_cierre_sprints_y_reportes` | Contra `BranchBoard-Pruebas` + OneDrive, prueba `cerrar_sprint_en` (solo Admin, cierre anticipado, copia congelada del reporte y liberación de inacabadas al backlog), evaluación automática al llegar `fecha_fin` desde `revisar_actualizaciones_en` y `listar_sprints_en` (auto-cierre si todo está terminado o reporte del día manteniendo abierto sin duplicar hasta terminar la última tarea), descarte de cierre forjado de Líder y sincronización completa de reportes en máquina B vía OneDrive. Borra workspace y carpeta al terminar |

4. **Reglas de las pruebas reales**:
   - Cada una deja el repo como estaba: borra sus ramas (con `cm branch delete --delete-changesets` si tienen check-ins), sus workspaces temporales y revoca los PAT que crea.
   - Los nombres llevan la hora (ej. `T-9132840-…`) para no chocar entre corridas.
   - Ninguna prueba puede quedarse esperando una respuesta de `cm` (lo que pasa con `cm switch` y cambios pendientes sin `--noinput`, y con `cm merge … --merge` cuando hay conflicto): se revisa antes.
   - Al terminar se comprueba que no quedó nada: `cm find branch "on repository 'BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity'" --format="{name}" --nototal` (solo las ramas originales), `cm wk list` (ningún `bb-prueba-*`) y `cm accesstoken list MiguelM17@unity` (ningún PAT sobrante).
   - Todo issue nuevo de backend que use `cm` o la API agrega su propia prueba `entorno_real_*` y la anota en esta tabla.
5. **Antes de fusionar**:
   - Si dos PRs tocan los mismos archivos, se trae `dev` a la segunda rama y se vuelven a correr **todas** las pruebas reales antes de fusionarla.
   - Los issues se cierran a mano después del merge a `dev`, porque `Closes #N` solo actúa al llegar a `main`.
