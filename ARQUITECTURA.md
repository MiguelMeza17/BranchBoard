# BranchBoard — Arquitectura

> Ver visión general en [PLANNING.md](./PLANNING.md). Este documento es una vista de pájaro de cómo encajan las piezas del sistema — para el detalle de cada comando/tabla, ver [BACKEND.md](./BACKEND.md) y [FRONTEND.md](./FRONTEND.md).

## Tipo de arquitectura

**Monolito de escritorio** (Fase 1). Todo corre empaquetado en un solo ejecutable de Tauri — no hay cliente-servidor separado, no hay red de por medio entre frontend y backend (se comunican dentro del mismo proceso vía comandos de Tauri). La Fase 2 (versión web, futura) rompe este monolito: ahí sí aparece un backend servidor aparte, expuesto por HTTP — pero eso queda fuera del alcance actual.

## Componentes

- **Frontend**: React + TypeScript, corriendo dentro de la webview de Tauri. Renderiza las 8 pantallas (Scrum, Resumen, Lista de tareas, Avance, Branches, Merging, Testing, Categorías, Reportes, Ajustes) y dispara acciones vía `invoke(...)` hacia el backend.
- **Backend**: Rust, dentro del mismo proceso de Tauri. Expone comandos que el frontend invoca (`tomar_tarea`, `aprobar_tarea`, `crear_reporte_testing`, etc.), valida permisos por rol, y es el único que toca SQLite y `cm`.
- **Persistencia (SQLite)**: un archivo `.db` por proyecto, sincronizado a la nube (Drive/OneDrive) para compartirlo entre el equipo. Guarda tareas, usuarios, solicitudes de ingreso, notificaciones, colaboradores externos — todo excepto el código/archivos versionados en sí.
- **Unity Version Control (vía `cm`)**: el backend invoca el CLI `cm` como subproceso para todo lo que toca ramas/changesets (crear rama, checkin, merge, leer historial). Es la fuente de verdad del código y los assets versionados — SQLite solo guarda referencias (nombre de rama, último changeset visto), nunca el contenido.
- **Gluon (workspace parcial)**: variante liviana de `cm` para colaboradores externos sin el proyecto completo instalado — mismo mecanismo de invocar subprocesos, solo que con `cm partial` en vez de los comandos normales.
- **Cloudflare WARP**: dependencia externa opcional, gestionada desde la app (`warp-cli`), necesaria quando la red bloquea la conexión a Unity VC.

## Decisiones de arquitectura ya tomadas

- **Sin backend propio de autenticación**: la identidad se resuelve con `cm whoami` para todos los roles (incluidos los colaboradores con workspace parcial) — ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario).
- **Sin servicios en la nube propios** (ni Service Account, ni API REST propia): se descartó ese diseño a favor de Gluon — ver [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples).
- **SQLite como único almacén de datos de la app**, sincronizado por una carpeta de nube ya existente del equipo (no se construye infraestructura de sincronización propia) — ver [Alcance en BACKEND.md](./BACKEND.md#alcance).
- **Todas las reglas de negocio (roles, límite de profundidad de ramas, estados) viven en el backend de BranchBoard, no en Unity VC** — Unity VC no tiene forma de conocerlas ni aplicarlas. Esto significa que solo se cumplen si el equipo usa BranchBoard como única puerta de entrada — ver [Protección de ramas en BACKEND.md](./BACKEND.md#protección-de-ramas-main-y-dev).
- **Comandos externos siempre por subproceso con argumentos separados**, nunca por string de shell — ver [Seguridad al invocar cm en BACKEND.md](./BACKEND.md#seguridad-al-invocar-cm-evitar-inyección-de-comandos).
