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
