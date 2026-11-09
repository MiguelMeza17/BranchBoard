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
- Si al implementar algo del backend cambia o se completa el diseño (un comando `cm` exacto, una regla nueva, un pendiente que se resuelve), se actualiza `documentacion/BACKEND.md` en el mismo PR.
- El mockup del frontend tiene que seguir funcionando en el navegador (`npm run dev`) sin backend: las llamadas a comandos de Tauri pasan por un solo cliente que, fuera de Tauri, usa los datos en memoria.
- Todo cambio de frontend debe quedar reflejado en la documentación de backend correspondiente (`documentacion/BACKEND.md` y/o `documentacion/ARQUITECTURA.md`) para que, cuando se implemente el backend real, no haya que redescubrir qué asume el frontend — ver también la regla de mantener `documentacion/FRONTEND.md` sincronizada, arriba.
