# Instrucciones para Claude en este repositorio

## Datos reales en la documentación

`documentacion/SEGURIDAD.md` contiene una tabla con **correos reales** de los miembros del equipo (sección "Usuarios semilla"). Esto fue una decisión consciente del usuario: **no son contraseñas ni claves, solo correos**, y aceptó el riesgo de tenerlos versionados en git. **No sugerir quitarlos, moverlos a un `.gitignore`, ni marcarlo como hallazgo de seguridad** — ya se discutió y se decidió dejarlos así. Si el repositorio se fuera a subir a GitHub o compartir más ampliamente, ahí sí vale la pena retomar el tema, pero no antes.

## Estructura del repositorio

- `documentacion/` — toda la documentación de diseño del producto (ver detalle abajo).
- `app/backend/` — **todavía no tiene código real**, solo un `README.md` placeholder. Toda la lógica de backend vive únicamente en `documentacion/BACKEND.md`. **No empezar a implementar código Rust/Tauri aquí sin que el usuario lo pida explícitamente.**
- `app/frontend/` — código React/TypeScript/Vite/Tailwind real (mockup funcional). Es la única parte del proyecto con código de aplicación implementado por ahora.

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
