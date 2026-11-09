# BranchBoard — Backend

Código Rust del backend (Tauri): comandos expuestos al frontend, lógica de datos (SQLite local), integración con Unity Version Control (`cm`), y la arquitectura de sincronización por eventos.

Antes de escribir código aquí, ver el diseño completo en [`../../documentacion/BACKEND.md`](../../documentacion/BACKEND.md) — en particular:
- [Sincronización entre máquinas (arquitectura de eventos)](../../documentacion/BACKEND.md#sincronización-entre-máquinas-arquitectura-de-eventos)
- [Firma criptográfica del rol](../../documentacion/BACKEND.md#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)
- [Modelo de datos](../../documentacion/BACKEND.md#modelo-de-datos-tabla-usuarios) (tablas `usuarios`, `tareas`, `sprints`, etc.)
- [Contrato con el frontend](../../documentacion/BACKEND.md#contrato-con-el-frontend-acciones-y-comandos) (qué comando respalda cada acción del store)

**Estado**: en implementación desde el 2026-11-09. El orden de trabajo está en la hoja de ruta (issue #129); cada parte tiene su issue con la etiqueta `backend`. El primer paso es crear aquí el proyecto Tauri (issue #70).
