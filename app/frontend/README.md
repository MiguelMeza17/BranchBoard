# BranchBoard — Frontend

Interfaz visual completa (mockup funcional) de BranchBoard: React + TypeScript + Vite.
**Solo frontend** — sin backend, sin Tauri, sin SQLite. Todos los datos son fixtures en
memoria manipulados con estado de React (Zustand); un refresh de página resetea todo.

Ver el diseño completo en [`../../documentacion/FRONTEND.md`](../../documentacion/FRONTEND.md)
y la lista oficial de pantallas en [`../../documentacion/PLANNING.md`](../../documentacion/PLANNING.md#pantallas--tableros-del-mvp).

## Cómo correrlo

```bash
cd app/frontend
npm install
npm run dev      # abre en http://127.0.0.1:4300/
```

> El puerto es **4300** (no el 5173 por defecto de Vite) porque en esta máquina el rango
> 5173–5272 está reservado por Windows (`netsh interface ipv4 show excludedportrange`).
> Se puede cambiar en `vite.config.ts`.

Otros scripts: `npm run build` (compila a `dist/`), `npm run typecheck`, `npm run preview`.

## Stack y librerías elegidas

- **Vite + React 18 + TypeScript** — base rápida.
- **Tailwind CSS** — sistema de diseño consistente (paleta, tipografía, escala, radios,
  sombras) definido una vez en `tailwind.config.js` + `src/index.css`, aplicado a las 12 pantallas.
- **Zustand** (`src/store/useStore.ts`) — estado global en memoria y todas las "acciones"
  (mover tarea, aprobar, eliminar, etc.). Más liviano que Context para tanta interacción.
- **@dnd-kit/core** — drag & drop real entre columnas del Tablero Scrum.
- **Chart.js + react-chartjs-2** — dona (Resumen) y curva burndown (Avance).
- **reactflow** — árbol de linaje de ramas (Branches).
- **react-router-dom** (HashRouter) — navegación entre las 12 pantallas con layout compartido.

## Pantallas (las 12 del MVP)

| # | Ruta | Qué hace |
|---|------|----------|
| 1 | `/scrum` | Tablero kanban con **drag & drop real** entre las 4 columnas (por hacer / en producción / en revisión / terminada), navegación tipo **explorador de carpetas** (breadcrumb + foco en subtareas), badges de estado (pendiente de aprobación, edición pendiente con sombra amarilla, cambios nuevos), doble candado para "tomar tarea" (escribir `asignar`) y "mandar a revisar", botón "Autorizar" directo para Líder/Admin sobre su propia tarea. |
| 2 | `/resumen` | Gráfica de **dona** por estado + tiles de KPIs + distribución por tipo. |
| 3 | `/lista` | Tabla de todas las tareas con filtros; **crear tarea** con el aviso obligatorio de "pendiente de aprobación" para Desarrollador/Tester. |
| 4 | `/avance` | **Curva burndown** (tareas restantes vs. ideal) con línea opcional de changesets del equipo, doble eje. |
| 5 | `/branches` | **Árbol de ramas** (reactflow): dev raíz, ramas y subramas coloreadas por estado. |
| 6 | `/revision` | Estado tipo PR con **2 carpetas** (Tareas grandes → dev / Tareas pequeñas → padre), Autorizar/Rechazar con doble candado, selector de Líder revisor (Admin), menú "Asignármela", visor de **pre-revisión** por tipo (3D, imagen, audio, video). |
| 7 | `/testing` | Exclusivo del rol **Tester** (oculto en el menú para los demás): crear reporte que genera tarea de corrección automática. |
| 8 | `/pendientes` | 2 tipos de fila: **tareas nuevas** (Aprobar / Editar y aprobar / Advertir / Rechazar) y **ediciones pendientes** (diff antes→después, Aprobar / Rechazar). |
| 9 | `/papelera` | Tareas soft-deleted; "Eliminar definitivamente" con doble candado, solo Líder/Admin; solo lectura para Desarrollador/Tester. |
| 10 | `/categorias` | Carpetas visuales por tipo/disciplina que filtran la Lista de tareas. |
| 11 | `/reportes` | Reporte de cierre de sprint: análisis de tareas + actividad por persona. |
| 12 | `/ajustes` | Solicitudes de ingreso, gestión de usuarios (confirmar/ascender/eliminar según rol), backups (o "reportar problema BD"), permisos de colaboradores externos, y **zona de peligro** (eliminar proyecto escribiendo el nombre exacto). |

## Roles (simulación)

Como no hay autenticación real, el **selector "Ver como"** en la barra superior cambia el rol del
usuario activo (Administrador / Líder / Desarrollador / Tester) y con ello qué botones, acciones y
pantallas se muestran, siguiendo la matriz de [`SEGURIDAD.md`](../../documentacion/SEGURIDAD.md).
La lógica de permisos vive en `src/lib/permisos.ts`.

## Estructura

```
src/
  main.tsx, App.tsx        # bootstrap + rutas
  types.ts                 # modelo de dominio (derivado de BACKEND.md)
  data/fixtures.ts         # datos de mentira (equipo VirusTF, tareas, sprints, etc.)
  store/useStore.ts        # estado + acciones (Zustand)
  lib/                     # permisos, colores/estilos, registro de Chart.js
  components/              # Layout, Sidebar, Topbar, Modal, ConfirmLock, TaskCard, badges…
  pages/                   # una por pantalla
```

## Notas / simplificaciones honestas

- Datos 100% en memoria: no hay persistencia; recargar resetea al estado inicial de los fixtures.
- Los visores de **pre-revisión** son maquetas (placeholders con íconos), no cargan modelos/media reales.
- La barra superior "Abrir en Unity", "Avisar por correo" (mailto), etc. muestran un toast — no
  invocan procesos externos (eso es trabajo del backend Rust/Tauri, fuera de alcance aquí).
- El árbol de Branches usa un layout por capas simple (no un algoritmo de grafos avanzado).
- "Crear un nuevo proyecto", splash de inicio y el flujo de pantalla inicial/login (`cm whoami`)
  quedaron fuera: el mockup entra directo al proyecto ya cargado, como pedía el enfoque del MVP.
```
