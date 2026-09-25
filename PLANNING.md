# BranchBoard

Herramienta de gestión de tareas y producción para equipos pequeños de desarrollo de videojuegos en Unity.

## Visión

Todo el trabajo del equipo se organiza y visualiza a través de **tareas**, en dos niveles:

- **Tareas generales** → tipo **Scripting**. Una tarea general = una rama de Git/Unity Version Control. Las crea y gestiona gente técnica, familiarizada con Unity y el control de versiones.
- **Minitareas** → el apartado donde aparece el conjunto de tareas que **no** son generales: **Modelado 3D, Animación, Imagen, Video, Audio/Música**. Cada minitarea es un entregable simple (subir un archivo), pensada para colaboradores **no especializados** en Unity. Ver detalle en [Mini-tareas](#mini-tareas).
- **Testing** → apartado debajo de Minitareas, exclusivo del rol **Tester**. Aquí el Tester documenta lo que encontró al revisar una tarea (un hallazgo/reporte), vinculado a la tarea general correspondiente. Ver detalle en [Testing](#testing).

Cada tarea, sin importar su tipo o nivel, pasa por los mismos estados y se ve reflejada en los mismos tableros.

## Pre-revisión

Antes de pasar al estado "en revisión", el entregable de cualquier tarea se puede **reproducir/previsualizar dentro de la misma interfaz** de BranchBoard, sin salir de la app ni abrir la herramienta original (Unity, Blender, editor de video, etc.):
- Video (minitarea) → reproductor embebido
- Imagen (minitarea) → visor de imagen
- Audio/Música (minitarea) → reproductor de audio embebido
- Animación (minitarea) → previsualización reproducible de la animación
- Modelado (minitarea) → visor 3D interactivo (rotar/zoom/pan sobre el modelo)
- Scripting (tarea general) → (pendiente de definir: ¿diff de la rama? ¿build/preview del cambio?)

**Primer tipo a implementar:** el visor de previsualización de **modelado 3D**, usando tecnología web de visualización 3D (ej. Three.js / `<model-viewer>` sobre glTF) embebida en la webview de Tauri.

## Subtareas (tareas anidadas)

Una tarea puede tener subtareas, y estas a su vez asociarse a una rama hija de la rama de su tarea padre (rama sobre rama). **Profundidad máxima: 3 niveles de ramas hijas por debajo de `dev`** (`dev` → rama → rama hija → rama nieta). No se permite crear una cuarta rama hija — más allá de eso el riesgo de desincronización entre ramas crece demasiado para un equipo pequeño.

Regla adicional: una rama hija debe mergearse a su rama padre antes de que la rama padre pueda mergearse/cerrarse hacia arriba (orden de merge estrictamente de abajo hacia arriba). El límite de profundidad por sí solo no evita la desincronización si una subtarea queda abierta mucho tiempo — lo que realmente desincroniza no es la profundidad sino el tiempo que una rama vive sin mergear.

## Mini-tareas

**Minitareas** es el apartado donde vive todo el conjunto de tareas que no son generales: **Modelado 3D, Animación, Imagen, Video, Audio/Música**. Pensadas para colaboradores **no especializados** en el funcionamiento de Unity/Unity Version Control.

- Una minitarea es un entregable puntual y pequeño: subir un `.fbx`, unas imágenes, un video, un audio, una animación.
- **Sí se sincroniza con Unity Version Control**: cada minitarea es, técnicamente, una **subtarea** (rama hija de la tarea general a la que se vincula) — reutiliza el mismo mecanismo descrito en "Subtareas (tareas anidadas)". La diferencia es solo de experiencia: el colaborador no técnico nunca ve ni entiende que hay una rama detrás; solo sube su archivo y la app crea la rama y hace el checkin por él.
- Un rechazo no borra la minitarea: simplemente se pide una nueva implementación (nuevos checkins sobre esa misma rama hija) y, cuando se aprueba, se mergea hacia la tarea general — igual que cualquier merge de rama.
- Vive en su propia interfaz, separada de los tableros de ramas, pero **siempre queda vinculada a una tarea general** (la que sí es rama).

En otras palabras: las minitareas son la puerta de entrada para que alguien sin conocimiento de Git/Unity VCS pueda aportar un archivo, sin tocar ni necesitar entender el sistema de ramas — aunque por debajo sí es una rama como cualquier otra.

## Testing

Apartado ubicado debajo de **Minitareas**, exclusivo del rol **Tester** (ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)).

- El Tester puede ver **todas** las tareas del proyecto (a diferencia de Desarrollador, que solo ve las suyas).
- Al revisar una tarea, si encuentra un error, el Tester **crea un reporte de testing**: documenta lo que vio, vinculado a la tarea general correspondiente (igual que una minitarea se vincula a su tarea general).
- El Tester no crea tareas de producción (Scripting/Modelado/Animación/Imagen/Video/Música) — su única capacidad de "crear" es este reporte de testing.

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
5. **"Branches"** — **nuevo**: la **gráfica de ascendencia/linaje de ramas** (árbol visual, no un kanban). Muestra `dev` como raíz, las ramas de tareas generales saliendo de ahí, las ramas hijas (subtareas) colgando de su rama padre, y las ramas nietas (mini-tareas) un nivel más abajo. Cada rama se colorea según el estado de su tarea (por hacer / en producción / en revisión / terminada-mergeada). Inspirado en el "Branch Explorer" que ya trae Unity Version Control.

## Pendiente por definir

- Campos de una tarea (título, tipo, rama asociada, estado, asignado, etc.)
- Campo `fecha_completada` en la tarea, necesario para armar la curva de Avance por día
- Librería de grafos/árboles para renderizar "Branches" (ej. `react-flow`)
- Estados exactos del flujo (por hacer → en producción → en revisión → terminada, ¿alguno más? ¿la pre-revisión es un estado propio o una acción dentro de "en revisión"?)
- Integración con Unity Version Control: ¿lee el workspace local? ¿crea la rama automáticamente al tomar la tarea?
- Formato/pipeline de assets 3D para la pre-revisión: ¿se exige exportar a glTF/GLB? ¿conversión automática desde el formato nativo (.fbx, .blend)?
- Previsualización para los demás tipos de tarea (animación, scripting) — solo se definió a fondo la de modelado 3D
- Cómo se fuerza/valida en la práctica el orden de merge de abajo hacia arriba en subtareas anidadas
- Qué pasa si se intenta crear una cuarta rama hija por debajo del límite de 3 (¿se bloquea en la UI? ¿solo se advierte?)
- Cómo se ve la interfaz de mini-tareas (¿lista aparte? ¿sección dentro de la tarea superior?)
- **Resuelto**: la mini-tarea ya es una rama con el archivo versionado desde que se subió; "integrarla" es simplemente mergear esa rama hija hacia la tarea general, sin archivos sueltos que limpiar.
- Si una mini-tarea puede tener más de un archivo adjunto o es siempre 1 mini-tarea = 1 archivo
- Permisos: ¿un colaborador no especializado puede crear mini-tareas libremente, o solo dentro de una tarea superior ya creada por alguien técnico?
