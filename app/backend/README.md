# BranchBoard — Backend

Código Rust del backend (Tauri): comandos expuestos al frontend, lógica de datos (SQLite local), integración con Unity Version Control (`cm`), y la arquitectura de sincronización por eventos.

Antes de escribir código aquí, ver el diseño completo en [`../../documentacion/BACKEND.md`](../../documentacion/BACKEND.md) — en particular:
- [Sincronización entre máquinas (arquitectura de eventos)](../../documentacion/BACKEND.md#sincronización-entre-máquinas-arquitectura-de-eventos)
- [Firma criptográfica del rol](../../documentacion/BACKEND.md#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)
- [Modelo de datos](../../documentacion/BACKEND.md#modelo-de-datos-tabla-usuarios) (tablas `usuarios`, `tareas`, `sprints`, etc.)
- [Contrato con el frontend](../../documentacion/BACKEND.md#contrato-con-el-frontend-acciones-y-comandos) (qué comando respalda cada acción del store)

## Cómo correrlo

Requisitos (Windows): [Rust](https://rustup.rs) con el toolchain `stable-x86_64-pc-windows-msvc`, las herramientas de compilación de C++ de Visual Studio, WebView2 (ya viene en Windows 11) y Node.

Los comandos se corren desde `app/frontend/` (ahí está instalada la CLI de Tauri):

```
npm install
npm run tauri dev     # abre la ventana de escritorio con el frontend en vivo
npm run tauri build   # genera el instalador en app/backend/target/release/bundle/nsis/
```

Pruebas del backend, desde `app/backend/`: `cargo test`.

`npm run dev` (sin Tauri) sigue abriendo el mockup en el navegador con datos en memoria.

## Estructura

- `Cargo.toml`, `build.rs`, `src/` — código Rust (`src/lib.rs` registra los comandos).
- `tauri.conf.json` — configuración de la app (ventana, instalador, de dónde sale el frontend).
- `capabilities/` — permisos de Tauri (ver [SEGURIDAD.md](../../documentacion/SEGURIDAD.md#allowlist-de-capabilities-de-tauri-resuelto)).
- `icons/` — íconos de la app, generados desde `app/frontend/public/favicon.png` con `npm run tauri icon`.

**Estado**: en implementación desde el 2026-11-09. El orden de trabajo está en la hoja de ruta (issue #129); cada parte tiene su issue con la etiqueta `backend`.
