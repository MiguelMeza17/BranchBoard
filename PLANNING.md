# BranchBoard

Herramienta de gestión de tareas y producción para equipos pequeños de desarrollo de videojuegos en Unity.

## Visión

Todo el trabajo del equipo se organiza y visualiza a través de **tareas**, en un solo árbol jerárquico (sin un apartado aparte de "Minitareas" — eso se eliminó, ver [Subtareas](#subtareas-tareas-anidadas)):

- **Tareas generales** → tipo **Scripting**. Una tarea general = una rama de Git/Unity Version Control. Las crea y gestiona gente técnica, familiarizada con Unity y el control de versiones.
- **Subtareas** → hijas de una tarea general (o de otra subtarea), hasta 3 niveles. Pueden ser de cualquier tipo (Scripting, Modelado, Animación, Imagen, Video, Música). Las simples (un solo archivo) pueden crearlas colaboradores no especializados, con permiso otorgado por Líder/Admin. Ver detalle en [Subtareas](#subtareas-tareas-anidadas).
- **Testing** → apartado exclusivo del rol **Tester**, y **fuera del sistema de tareas/ramas** (no es tarea ni subtarea). No es un paso previo al merge — aplica sobre tareas que **ya están "terminada" (ya mergeadas a `dev`)**, para bugs que se escaparon de la revisión en [Merging](#merging) por la razón que sea. Si el Tester encuentra un error, documenta un reporte pequeño vinculado a esa tarea. Ver detalle en [Testing](#testing).
- **Revisión** (antes "Merging") → apartado nuevo, aparte de Tareas/Testing. Lista **todas** las tareas actualmente en "en revisión", organizado en **2 carpetas**: "Tareas grandes" (→ `dev`, **exclusiva de Líder/Administrador**, ni Desarrollador ni Tester la ven) y "Tareas pequeñas" (subtareas de colaboradores externos → su tarea padre, **visible para todos**, actúa quien esté a cargo de esa tarea padre — puede ser Desarrollador). También aparece reflejado como columna en el **Tablero Scrum**. Ver detalle en [Revisión](#revisión).

Cada tarea, sin importar su tipo o nivel, pasa por los mismos estados y se ve reflejada en los mismos tableros.

**Estado "bloqueada"**: si un **Desarrollador** crea una tarea, nace en este estado especial (antes de "por hacer") y no puede tomarse ni crear rama hasta que un Líder o Administrador la apruebe. Cuando Líder/Admin crean una tarea, no pasa por este estado — entra directo a "por hacer". Ver permisos exactos en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos).

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
- Testing no es un requisito antes de mergear — el Líder/Admin puede autorizar el merge en [Merging](#merging) sin que el Tester haya revisado nada. Testing aplica **después**, sobre tareas ya en estado "terminada" (ya mergeadas a `dev`), para atrapar bugs que se escaparon de esa revisión por la razón que sea.
- Si encuentra un error, el Tester **crea un reporte de testing** (algo pequeño y simple, documentando lo que vio) **y eso genera automáticamente una nueva tarea de corrección**, vinculada tanto al reporte como a la tarea general original. Alguien del equipo se asigna esa tarea de corrección igual que cualquier otra — al tomarla, se le crea su rama (partiendo de `dev`, igual que toda tarea general).
- El Tester no crea tareas de producción directamente (Scripting/Modelado/Animación/Imagen/Video/Música) ni subtareas — su única capacidad de "crear" es el reporte, que en cadena genera la tarea de corrección.

**Definido**: la tarea de corrección nace directo en **"por hacer"** (no "bloqueada") — viene de un hallazgo real del Tester, no de una idea sin validar, así que cualquiera puede tomarla de una vez y crear su rama.

## Revisión

**Renombrado** (antes "Merging"). Apartado nuevo, independiente de Tareas/Testing. Lista **todas** las tareas actualmente en estado "en revisión" — es, en esencia, una página de estado de PR (pull request): qué ramas están esperando ser mergeadas.

**Organizado en 2 carpetas** (mismo patrón visual que las carpetas del Tablero Scrum/Categorías):

1. **Tareas grandes** — tareas generales esperando mergear hacia **`dev`**. **Corregido**: visible **exclusivamente** para Líder y Administrador — Desarrollador y Tester **no ven esta carpeta en absoluto**, ni siquiera en solo lectura. Autorizar/rechazar (ver [Flujo de revisión y merge en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)) sigue siendo exclusivo de Líder/Administrador. El Administrador puede **asignarle la revisión de una tarea específica a un Líder** — delegar quién decide sobre ese merge en particular. Ver permisos en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos).
2. **Tareas pequeñas** — las subtareas simples que suben los colaboradores externos (Gluon), esperando mergear hacia **su tarea padre** (no hacia `dev` directamente). **Visible para todos los roles**. Autoriza/rechaza **quien esté a cargo de la tarea padre** — puede ser Desarrollador, Líder o Administrador, según quien la haya tomado; no está restringido a Líder/Admin, porque mergear hacia la rama de tu propia tarea no requiere el mismo nivel de permiso que mergear hacia `dev`. La revisión se hace con el visor de [Pre-revisión](#pre-revisión) correspondiente al tipo de la subtarea — **nunca requiere que quien revisa cambie de rama en su propio workspace**, justo para no ser costoso en un caso tan pequeño como subir un solo archivo.

Ambas carpetas también se reflejan como columna/estado en el **Tablero Scrum** (ya existía como uno de los 4 estados básicos: por hacer / en producción / en revisión / terminada) — Revisión es la vista dedicada y ampliada de ese mismo estado, no un estado nuevo.

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
5. **"Branches"** — **nuevo**: la **gráfica de ascendencia/linaje de ramas** (árbol visual, no un kanban). Muestra `dev` como raíz, las ramas de tareas generales saliendo de ahí, las ramas hijas (subtareas) colgando de su rama padre, y las ramas nietas (subtareas de subtareas) un nivel más abajo. Cada rama se colorea según el estado de su tarea (por hacer / en producción / en revisión / terminada-mergeada). Inspirado en el "Branch Explorer" que ya trae Unity Version Control.
6. **"Revisión"** (antes "Merging") — **nuevo**: página de estado de PR con 2 carpetas — "Tareas grandes" (exclusiva Líder/Admin, autorizan hacia `dev`) y "Tareas pequeñas" (visible para todos, quien esté a cargo de la tarea padre autoriza, puede ser Desarrollador). Ver detalle en [Revisión](#revisión).
7. **"Ajustes"** — **nuevo**: agrupa acciones administrativas que antes estaban definidas sueltas (gestión de usuarios, solicitudes de ingreso, backups, eliminar proyecto). Ver detalle en [Ajustes](./FRONTEND.md#ajustes).
8. **"Categorías"** — **nuevo**: vista tipo explorador de carpetas, aparte de la Lista de tareas (que es plana). Son **solo carpetas visuales para organizar/filtrar, nunca ramas** — abrir una carpeta simplemente filtra las tareas de ese tipo. Carpetas: **Scripting, Modelado 3D, Modelado 2D, Animación, Imagen, UI/Interfaz, Audio, Música, Video, Testing**.

## Pendiente por definir

- Librería de grafos/árboles para renderizar "Branches" (ej. `react-flow`)
- Estados exactos del flujo (por hacer → en producción → en revisión → terminada, ¿alguno más? ¿la pre-revisión es un estado propio o una acción dentro de "en revisión"?)
- Integración con Unity Version Control: ¿lee el workspace local? ¿crea la rama automáticamente al tomar la tarea?
- Formato/pipeline de assets 3D para la pre-revisión: ¿se exige exportar a glTF/GLB? ¿conversión automática desde el formato nativo (.fbx, .blend)?
- Previsualización para los demás tipos de tarea (animación, scripting) — solo se definió a fondo la de modelado 3D
- **Resuelto**: el orden de merge de abajo hacia arriba se valida en el backend antes de ejecutar el merge — ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)
- **Resuelto**: se bloquea antes de crear la rama — ver [Protección de ramas en BACKEND.md](./BACKEND.md#protección-de-ramas-main-y-dev)
- Si una subtarea simple puede tener más de un archivo adjunto o es siempre 1 subtarea = 1 archivo
- ¿Un colaborador de subtareas simples puede crear su subtarea dentro de **cualquier** tarea general existente, o solo dentro de las que un Líder/Admin le habilite específicamente?
