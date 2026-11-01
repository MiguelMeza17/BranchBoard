# BranchBoard

Herramienta de gestión de tareas y producción para equipos pequeños de desarrollo de videojuegos en Unity.

## Visión

Todo el trabajo del equipo se organiza y visualiza a través de **tareas**, en un solo árbol jerárquico (sin un apartado aparte de "Minitareas" — eso se eliminó, ver [Subtareas](#subtareas-tareas-anidadas)):

- **Tareas generales** → tipo **Scripting**. Una tarea general = una rama de Git/Unity Version Control. Las crea y gestiona gente técnica, familiarizada con Unity y el control de versiones.
- **Subtareas** → hijas de una tarea general (o de otra subtarea), hasta 3 niveles. Pueden ser de cualquier tipo (Scripting, Modelado 3D, Modelado 2D, Animación, Imagen, UI/Interfaz, Audio, Música, Video — ver [Contexto completo del proyecto en BACKEND.md](./BACKEND.md#contexto-completo-del-proyecto)). Las simples (un solo archivo) pueden crearlas colaboradores no especializados, con permiso otorgado por Líder/Admin. Ver detalle en [Subtareas](#subtareas-tareas-anidadas).
- **Testing** → apartado exclusivo del rol **Tester**, y **fuera del sistema de tareas/ramas** (no es tarea ni subtarea). No es un paso previo al merge — aplica sobre tareas que **ya están "terminada" (ya mergeadas a `dev`)**, para bugs que se escaparon de la revisión en [Revisión](#revisión) por la razón que sea. Si el Tester encuentra un error, documenta un reporte pequeño vinculado a esa tarea. Ver detalle en [Testing](#testing).
- **Revisión** (antes "Merging") → apartado nuevo, aparte de Tareas/Testing. Lista **todas** las tareas actualmente en "en revisión", organizado en **2 carpetas**: "Tareas grandes" (→ `dev`, **visible para todos, solo lectura para Desarrollador/Tester**, autorizan solo Líder/Administrador) y "Tareas pequeñas" (subtareas de colaboradores externos → su tarea padre, **visible para todos**, actúa quien esté a cargo de esa tarea padre — puede ser Desarrollador). También aparece reflejado como columna en el **Tablero Scrum**. Ver detalle en [Revisión](#revisión).

Cada tarea, sin importar su tipo o nivel, pasa por los mismos estados y se ve reflejada en los mismos tableros.

**Pendiente de aprobación** (reemplaza el antiguo estado "bloqueada" — **corregido**): si un **Desarrollador** o un **Tester** crea una tarea, se marca con una bandera `pendiente_aprobacion = true`, pero la tarea sigue su curso normal desde ya — aparece de inmediato en "por hacer", se puede **tomar y crear su rama sin esperar nada**. Lo único que bloquea esa bandera es el paso final: **no se puede mandar a revisión** hasta que un Líder o Administrador la apruebe desde el nuevo apartado [Pendientes](#pendientes). Al crearse, se dispara una notificación automática a todos los Líderes/Administradores, que además queda listada en Pendientes. Al aprobarse, la tarea desaparece de Pendientes y conserva el estado que ya tenía en ese momento (por hacer, o en producción si alguien ya la había tomado mientras tanto). Las subtareas creadas dentro de una tarea grande siguen la misma lógica si las crea un Desarrollador o Tester — se guardan en su lugar normal (colgadas de su tarea padre) y **además** se duplican en Pendientes hasta que se aprueben. Cuando Líder/Admin crean una tarea (o subtarea) ellos mismos, no pasa por esto — no requiere aprobación. Ver permisos exactos en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos).

## Pre-revisión

Antes de pasar al estado "en revisión", el entregable de cualquier tarea se puede **reproducir/previsualizar dentro de la misma interfaz** de BranchBoard, sin salir de la app ni abrir la herramienta original (Unity, Blender, editor de video, etc.) — **quien revisa nunca necesita abrir Unity ni cambiar de rama en su propio workspace** para ver el contenido, eso es justo el problema que resuelve la pre-revisión.

**Definido por tipo (con restricción de archivo permitido al subir):**

- **Modelado 3D** → solo se permite subir `.fbx`. Visor: la malla del modelo (con material si lo tiene), cantidad de polígonos, y geometría/shading que permita ver la **orientación de las caras** (normales). Botón aparte **"Abrir en Unity"** como alternativa.
- **Modelado 2D** → visor de imagen simple (solo mostrar la imagen).
- **Animación** → mismo visor que Modelado 3D (malla + material + polígonos + orientación de caras) **más** reproducir la animación sobre esa malla.
- **Imagen** → visor de imagen.
- **UI/Interfaz** → visor de imagen (mostrar el mockup/elemento de UI).
- **Audio** → reproductor de audio embebido.
- **Música** → reproductor de audio embebido.
- **Video** → reproductor de video embebido. **Restricción de archivo**: valida la extensión real del archivo, no solo el nombre — no se puede subir una imagen disfrazada de video para esta categoría.
- **Scripting** (tarea general) → (pendiente de definir: ¿diff de la rama? ¿build/preview del cambio?)

**Primer tipo a implementar:** el visor de previsualización de **modelado 3D**, usando tecnología web de visualización 3D (ej. Three.js / `<model-viewer>` sobre glTF) embebida en la webview de Tauri.

## Subtareas (tareas anidadas)

Una tarea puede tener subtareas, y estas a su vez asociarse a una rama hija de la rama de su tarea padre (rama sobre rama). **Profundidad máxima: 3 niveles de ramas hijas por debajo de `dev`** (`dev` → rama → rama hija → rama nieta). No se permite crear una cuarta rama hija — más allá de eso el riesgo de desincronización entre ramas crece demasiado para un equipo pequeño.

**Cómo se valida en código**: en vez de solo contar "niveles de subtarea", el backend cuenta la cadena completa de ancestros desde `main` (`main → dev → tarea general → subtarea → subtarea de subtarea` = 5 ramas). Si crear una nueva rama haría que la cadena llegue a 6, la app la bloquea antes de tocar `cm`. Esto es lo mismo que la regla de arriba, solo que expresado de forma verificable con código. Ver [Protección de ramas en BACKEND.md](./BACKEND.md#protección-de-ramas-main-y-dev) — incluye también que nadie (ni el Administrador) puede hacer checkin directo sobre `main` o `dev`, y que el merge `dev → main` sería exclusivo del Administrador si algún día se implementa en la app. **Importante**: esto solo se puede hacer cumplir a través de BranchBoard — si alguien usa el cliente de Unity VC directamente, la app no puede impedirlo.

Regla adicional: una rama hija debe mergearse a su rama padre antes de que la rama padre pueda mergearse/cerrarse hacia arriba (orden de merge estrictamente de abajo hacia arriba). El límite de profundidad por sí solo no evita la desincronización si una subtarea queda abierta mucho tiempo — lo que realmente desincroniza no es la profundidad sino el tiempo que una rama vive sin mergear.

**Subtareas simples (antes "mini-tareas")**: ya no existe un apartado separado — una subtarea puede ser tan simple como subir un solo archivo (`.fbx`, imágenes, video, audio, animación), pensada para colaboradores **no especializados** en Unity/Unity Version Control.

- **Sí se sincroniza con Unity Version Control**: técnicamente es una subtarea normal (rama hija). La diferencia es solo de experiencia: el colaborador no técnico nunca ve ni entiende que hay una rama detrás; solo sube su archivo y la app crea la rama y hace el checkin por él.
- **Corregido**: sí necesita Unity Version Control instalado (liviano, mismo instalador que todos), pero usa un **workspace parcial** (modo Gluon, `cm partial`) — nunca descarga el proyecto completo, solo la carpeta de su archivo. Ver [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples). Su identidad es `cm whoami`, igual que todos los demás roles — no hay ninguna cuenta de servicio ni mecanismo aparte.
- **Requiere permiso**: a diferencia de un Desarrollador normal, un colaborador no especializado necesita que un Líder/Administrador le otorgue el permiso de crear este tipo de subtarea, desde un panel en [Ajustes](./FRONTEND.md#ajustes) — no puede hacerlo libremente por su cuenta.
- Un rechazo no borra la subtarea: simplemente se pide una nueva implementación (nuevos checkins sobre esa misma rama hija) y, cuando se aprueba, se mergea hacia la tarea general — igual que cualquier merge de rama.

### Navegación tipo "explorador de carpetas" en el Tablero Scrum

Con hasta 3 niveles de anidación, mostrar todo el árbol plano de una vez es difícil de leer. En su lugar:

- Cada tarea con hijos se comporta como una carpeta
- Al hacer clic en una tarea padre, la vista se enfoca en ella: las ramas hermanas desaparecen de la pantalla, y solo se muestran esa tarea y sus hijos directos
- Entrar a una de esas hijas repite el mismo comportamiento (se enfoca en ella y en sus propios hijos), como navegar carpeta por carpeta

## Testing

Apartado exclusivo del rol **Tester** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)). El reporte en sí no es una tarea ni crea rama — es solo texto (y opcionalmente un adjunto) vinculado a la tarea general que revisó — pero **sí genera una tarea de corrección automáticamente**.

- El Tester puede ver **todas** las tareas del proyecto (a diferencia de Desarrollador, que solo ve las suyas).
- Testing no es un requisito antes de mergear — el Líder/Admin puede autorizar el merge en [Revisión](#revisión) sin que el Tester haya revisado nada. Testing aplica **después**, sobre tareas ya en estado "terminada" (ya mergeadas a `dev`), para atrapar bugs que se escaparon de esa revisión por la razón que sea.
- Si encuentra un error, el Tester **crea un reporte de testing** (algo pequeño y simple, documentando lo que vio) **y eso genera automáticamente una nueva tarea de corrección**, vinculada tanto al reporte como a la tarea general original. **Definido (2026-10-31)**: la tarea de corrección es una **tarea general nueva que sale de `dev`**, con su propio número (ej. `T-211`), **no una subtarea** de la original (no `T-101-3`). La original ya está terminada y mergeada, así que su rama está cerrada; el vínculo con ella es solo una referencia (`tarea_origen_id`), que se muestra en la interfaz (ej. "Corrige T-101" en Branches). Lo mismo aplica a cualquier tarea de seguimiento de una tarea ya terminada. Alguien del equipo se asigna esa tarea de corrección igual que cualquier otra — al tomarla, se le crea su rama (partiendo de `dev`, igual que toda tarea general).
- **Corregido**: el Tester también puede crear tareas de producción (Scripting/Modelado/Animación/Imagen/Video/Música) y subtareas directamente, igual que un Desarrollador — quedan `pendiente_aprobacion = true` hasta que un Líder/Administrador las apruebe desde [Pendientes](#pendientes). Esto es aparte de su capacidad de crear el reporte de Testing, que en cadena genera la tarea de corrección (ver siguiente punto).

**Definido**: la tarea de corrección que genera un reporte de Testing nace directo en **"por hacer"**, sin `pendiente_aprobacion` — viene de un hallazgo real ya reportado, no de una idea sin validar, así que cualquiera puede tomarla de una vez y crear su rama.

## Revisión

**Renombrado** (antes "Merging"). Apartado nuevo, independiente de Tareas/Testing. Lista **todas** las tareas actualmente en estado "en revisión" — es, en esencia, una página de estado de PR (pull request): qué ramas están esperando ser mergeadas.

**Organizado en 2 carpetas** (mismo patrón visual que las carpetas del Tablero Scrum/Categorías):

1. **Tareas grandes** — tareas generales esperando mergear hacia **`dev`**. **Corregido de nuevo**: visible para **todos los roles**, pero Desarrollador y Tester la ven en **solo lectura** — pueden ver qué está esperando merge, pero no tienen permiso de actuar sobre ello. Autorizar/rechazar (ver [Flujo de revisión y merge en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)) sigue siendo exclusivo de Líder/Administrador. El Administrador puede **asignarle la revisión de una tarea específica a un Líder** — delegar quién decide sobre ese merge en particular. Ver permisos en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). **Definido, corregido**: esta carpeta recibe las tareas trabajadas por **Desarrollador o Tester** (ninguno de los dos puede mergear directo, mismo trato) — si quien la trabajó fue un Líder o Administrador, mergea directo a `dev` sin pasar por aquí en absoluto (ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)). Esto cubre también el caso de un Tester que toma una tarea general (incluida la tarea de corrección que se le genera automáticamente al reportar un bug, ver [Testing](#testing)) — antes no tenía ningún camino de merge definido.
2. **Tareas pequeñas** — las subtareas simples que suben los colaboradores externos (Gluon), esperando mergear hacia **su tarea padre** (no hacia `dev` directamente). **Visible para todos los roles**. Autoriza/rechaza **quien esté a cargo de la tarea padre** — puede ser Desarrollador, Líder o Administrador, según quien la haya tomado; no está restringido a Líder/Admin, porque mergear hacia la rama de tu propia tarea no requiere el mismo nivel de permiso que mergear hacia `dev`. La revisión se hace con el visor de [Pre-revisión](#pre-revisión) correspondiente al tipo de la subtarea — **nunca requiere que quien revisa cambie de rama en su propio workspace**, justo para no ser costoso en un caso tan pequeño como subir un solo archivo.

Ambas carpetas también se reflejan como columna/estado en el **Tablero Scrum** (ya existía como uno de los 4 estados básicos: por hacer / en producción / en revisión / terminada) — Revisión es la vista dedicada y ampliada de ese mismo estado, no un estado nuevo.

## Pendientes

**Nuevo**, va en el menú justo después de **Testing** y antes de **Categorías**. Reemplaza al antiguo estado "bloqueada" (ver [Visión](#visión)).

- Lista las tareas (y subtareas) creadas por un **Desarrollador** o un **Tester** que todavía tienen `pendiente_aprobacion = true` — es decir, que un Líder/Administrador todavía no ha aprobado.
- La tarea **no está oculta ni bloqueada** mientras espera aquí: aparece con normalidad en el Tablero Scrum (en "por hacer", o más adelante en "en producción" si alguien ya la tomó) — lo único que no puede hacer es **mandarse a revisión**, hasta que se apruebe desde aquí.
- Una **subtarea** creada por Desarrollador/Tester dentro de una tarea grande se muestra **en dos lugares a la vez**: colgada de su tarea padre (como siempre) y también listada aquí en Pendientes, hasta que se apruebe.
- Al **aprobar** una tarea (Líder/Administrador, ver [Roles y permisos en SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): desaparece de Pendientes, ya puede mandarse a revisión, y su `estado` no cambia — sigue siendo el que ya tenía (por hacer / en producción).
- Cuando Líder/Admin crean una tarea o subtarea ellos mismos, nunca aparece aquí — no necesita aprobación.
- **Nuevo — 4 acciones posibles sobre una tarea recién creada, pendiente** (Líder/Administrador):
  1. **Aprobar**: tal como está, sin tocar nada (ver arriba).
  2. **Editar y aprobar**: el Líder/Admin corrige directamente los campos de la tarea (título, descripción, tipo, etc.) y la aprueba en el mismo paso — un solo botón, no dos acciones separadas. Se le notifica al creador que su tarea fue editada y aprobada.
  3. **Advertir**: manda un mensaje de alerta al creador señalando qué está mal, **sin tocar la tarea** — sigue pendiente de aprobación. Es para darle al creador la oportunidad de corregir su propio error.
  4. **Eliminar** (rechazo definitivo): manda la tarea a Papelera, igual que cualquier otra eliminación (ver [Papelera en BACKEND.md](./BACKEND.md#papelera)).
- **Nuevo — segundo tipo de fila: ediciones pendientes**. Cuando un Desarrollador edita una tarea propia **ya aprobada** (fuera de este flujo de creación) usando `editar_tarea_con_autorizacion` (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 4), el cambio se aplica de inmediato y la tarjeta se marca con una **sombra amarilla** — y la fila **también aparece aquí en Pendientes**, distinguida con una etiqueta ("Nueva" vs. "Edición") de las tareas recién creadas. Solo tiene 2 acciones (Líder/Admin):
  - **Aprobar**: confirma los valores nuevos, quita la sombra amarilla.
  - **Rechazar**: **revierte los campos a sus valores anteriores** — el cambio se pierde, se notifica al Desarrollador.
- **Mientras una tarea está pendiente de aprobación, su creador (Desarrollador/Tester) puede editarla libremente**, sin pedir ninguna autorización — la regla de "necesita autorización explícita para modificar una tarea" (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 4) solo aplica una vez que la tarea ya fue aprobada y sigue su curso normal. Esta edición del creador es un simple **Editar → Guardar** (sin aprobación de por medio, porque el que edita es el propio dueño, no quien aprueba).

## Reportes

Apartado nuevo. Cuando un **sprint con fecha** llega a su fin, se genera automáticamente un reporte con cómo fue el flujo de trabajo de ese sprint. **No tiene nada que ver con el rol Tester** — es un análisis general del sprint, con dos partes:

1. **Análisis de tareas del sprint**: tareas hechas, tareas que faltaron (no llegaron a "terminada"), tareas que se quedaron en revisión — para ver qué se cumplió, qué falló y por qué no se cumplió lo planeado.
2. **Desglose de actividad por persona**: qué hizo cada quien durante el sprint, con su nombre — Administrador, Líderes, Desarrolladores, Tester, y colaboradores de subtareas simples, todos por igual. **Simplificado**: como ahora todos usan `cm whoami` (incluidos los colaboradores de subtareas simples, ver [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples)), cada changeset ya viene atribuido directamente a la persona real — no hace falta ninguna traducción desde una cuenta de servicio.

Solo aplica a sprints **con fecha** — los sprints sin fecha no tienen un punto de cierre que dispare el reporte. Se apoya en el mismo mecanismo de temporizador ya usado para el [recordatorio de inactividad en BACKEND.md](./BACKEND.md#notificaciones) (revisión periódica mientras la app está abierta, comparando contra `fecha_limite`).

**Definido**: los Reportes son visibles para **todos los roles**, sin restricción.

## Stack

- **Framework**: Tauri (Rust) + React/TypeScript
- **Fase 1**: aplicación de escritorio (full desktop). El backend (Rust) invoca directamente el CLI `cm` de Unity Version Control y usa SQLite — no se usa Python.
- **Fase 2** (futuro): versión web. Solo se reutiliza el **frontend** (React). Tauri no corre en un navegador, así que esta fase necesita un **backend servidor nuevo y aparte** (no es Tauri) que exponga por HTTP lo que hoy hacen los comandos de Tauri — el navegador no puede ejecutar `cm` ni tocar el filesystem local directamente.

## Usuarios

Equipo pequeño de desarrollo de un proyecto Unity.

## Pantallas / tableros del MVP

1. **Tablero Scrum** — vista tipo kanban de las tareas
2. **Resumen** — gráfica circular (dona) con el conteo de tareas: por hacer, en producción, en revisión, terminadas
3. **Lista de tareas** — todas las tareas existentes, lugar donde se crean
4. **Tablero "Avance"** — **definido**: gráfica de **curva en el tiempo**, eje X = días, eje Y = **tareas resueltas ese día** (línea principal — se cuenta usando la fecha en que cada tarea pasó a "terminada"). Opcionalmente, una segunda línea muestra **cambios/changesets hechos ese día en todo el proyecto** (no por tarea, es actividad general del equipo vía `cm log` sobre todo el repositorio) — sirve para contrastar "mucha actividad pero pocas tareas cerrándose" vs. lo contrario. Responde "¿vamos bien o mal respecto al tiempo?".
5. **"Branches"** — **nuevo**: la **gráfica de ascendencia/linaje de ramas** (árbol visual, no un kanban). Muestra `dev` como raíz, las ramas de tareas generales saliendo de ahí, las ramas hijas (subtareas) colgando de su rama padre, y las ramas nietas (subtareas de subtareas) un nivel más abajo. Cada rama se colorea según el estado de su tarea (por hacer / en producción / en revisión / terminada-mergeada). Inspirado en el "Branch Explorer" que ya trae Unity Version Control. **Actualizado (2026-10-31)**: arriba de `dev` va `main`, y ambas se dibujan como líneas de tiempo (cada merge las hace avanzar); las ramas abiertas cuelgan del presente de `dev` y las ya fusionadas quedan como recuadros grises dentro de la línea de `dev`. Diseño visual completo en [FRONTEND.md](./FRONTEND.md).
6. **"Revisión"** (antes "Merging") — **nuevo**: página de estado de PR con 2 carpetas — "Tareas grandes" (visible para todos, solo lectura para Desarrollador/Tester, autorizan Líder/Admin hacia `dev`) y "Tareas pequeñas" (visible para todos, quien esté a cargo de la tarea padre autoriza, puede ser Desarrollador). Ver detalle en [Revisión](#revisión).
7. **"Testing"** — apartado exclusivo del rol Tester para documentar reportes sobre tareas ya "terminada". Ver detalle en [Testing](#testing).
8. **"Pendientes"** — **nuevo**: lista las tareas creadas por Desarrollador o Tester que todavía no han sido aprobadas por un Líder/Administrador. Va en el menú justo después de "Testing" y antes de "Papelera". Ver detalle en [Pendientes](#pendientes).
9. **"Papelera"** — **nuevo**: lista las tareas eliminadas (soft-delete), con eliminación definitiva exclusiva de Líder/Administrador. Va justo después de "Pendientes" y antes de "Categorías". Ver detalle en [Papelera en BACKEND.md](./BACKEND.md#papelera).
10. **"Categorías"** — **nuevo**: vista tipo explorador de carpetas, aparte de la Lista de tareas (que es plana). Son **solo carpetas visuales para organizar/filtrar, nunca ramas** — abrir una carpeta simplemente filtra las tareas de ese tipo. Carpetas: **Scripting, Modelado 3D, Modelado 2D, Animación, Imagen, UI/Interfaz, Audio, Música, Video, Testing, Corrección, Ajustes**.
11. **"Reportes"** — reporte automático de cierre de sprint (con fecha), visible para todos los roles. Ver detalle en [Reportes](#reportes).
12. **"Ajustes"** — **nuevo**: agrupa acciones administrativas que antes estaban definidas sueltas (gestión de usuarios, solicitudes de ingreso, backups, eliminar proyecto). Ver detalle en [Ajustes](./FRONTEND.md#ajustes).

## Pendiente por definir

- Librería de grafos/árboles para renderizar "Branches" (ej. `react-flow`)
- Estados exactos del flujo (por hacer → en producción → en revisión → terminada, ¿alguno más? ¿la pre-revisión es un estado propio o una acción dentro de "en revisión"?). **Aclarado**: "en revisión" no es un paso obligatorio para todas las tareas — si quien trabajó la tarea es Líder/Administrador, salta directo de "en producción" a "terminada" (ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)).
- Integración con Unity Version Control: ¿lee el workspace local? ¿crea la rama automáticamente al tomar la tarea?
- **Acotado**: la restricción de subida ya está decidida — solo `.fbx` (ver [tabla de validación en BACKEND.md](./BACKEND.md#validación-de-tipo-de-archivo-al-subir-una-subtarea-simple)), no `.blend` ni otros. Lo que sigue sin definir es solo el mecanismo/librería de conversión `.fbx` → glTF/GLB para el visor (`model-viewer`/Three.js, ver [Pendiente por definir en BACKEND.md](./BACKEND.md#pendiente-por-definir)).
- Previsualización para los demás tipos de tarea (animación, scripting) — solo se definió a fondo la de modelado 3D
- **Resuelto**: el orden de merge de abajo hacia arriba se valida en el backend antes de ejecutar el merge — ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)
- **Resuelto**: se bloquea antes de crear la rama — ver [Protección de ramas en BACKEND.md](./BACKEND.md#protección-de-ramas-main-y-dev)
- Si una subtarea simple puede tener más de un archivo adjunto o es siempre 1 subtarea = 1 archivo
- **Resuelto**: el permiso es **por persona, no por tarea** — un colaborador con permiso otorgado (ver [Permisos de subtareas simples en FRONTEND.md](./FRONTEND.md#ajustes)) puede crear su subtarea dentro de **cualquier** tarea general existente, no hay una lista de tareas específicas habilitadas. Ya estaba decidido por el diseño de ese panel (solo lista correos con permiso, sin granularidad por tarea), faltaba propagarlo aquí.
