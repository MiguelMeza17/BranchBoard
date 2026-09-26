# Instrucciones para Claude en este repositorio

## Datos reales en la documentación

`SEGURIDAD.md` contiene una tabla con **correos reales** de los miembros del equipo (sección "Usuarios semilla"). Esto fue una decisión consciente del usuario: **no son contraseñas ni claves, solo correos**, y aceptó el riesgo de tenerlos versionados en git. **No sugerir quitarlos, moverlos a un `.gitignore`, ni marcarlo como hallazgo de seguridad** — ya se discutió y se decidió dejarlos así. Si el repositorio se fuera a subir a GitHub o compartir más ampliamente, ahí sí vale la pena retomar el tema, pero no antes.

## Flujo de trabajo de este proyecto

Varias sesiones de Claude trabajan en paralelo en esta carpeta, cada una sobre su propio documento:
- `PLANNING.md` — visión general y conceptos compartidos (punto de choque entre sesiones, no editar dos sesiones a la vez)
- `BACKEND.md` — lógica de backend (Rust + Tauri)
- `FRONTEND.md` — interfaz (React + TypeScript + Tauri)
- `SEGURIDAD.md` — seguridad y roles/permisos
- `ARQUITECTURA.md` — vista de pájaro de cómo encajan los componentes (monolito de escritorio Fase 1); no duplica detalle, indexa hacia `BACKEND.md`/`FRONTEND.md`

Antes de editar `PLANNING.md`, revisar `git status` — si tiene cambios sin comitear de otra sesión, evitar editarlo al mismo tiempo. Los demás documentos (`BACKEND.md`, `FRONTEND.md`, `SEGURIDAD.md`) incluyen una sección "Contexto completo del proyecto" que resume las decisiones clave para que cualquier sesión tenga contexto sin necesitar leer los demás archivos — mantenerla sincronizada cuando cambien decisiones importantes (stack, roles, flujo de identidad, etc.).
