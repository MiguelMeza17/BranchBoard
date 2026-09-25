# BranchBoard — Frontend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que se ve en pantalla, con React + TypeScript dentro de Tauri.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Cada tarea puede ser de tipo:
  - **Scripting** → asociada a una rama (una tarea = una rama)
  - **Modelado** (3D)
  - **Animación**
  - **Imagen** (arte 2D)
  - **Video**
  - **Música**
- **Flujo de vida de una tarea**:
  1. Se crea la tarea desde la **Lista de tareas** — en este punto es puramente visual/informativa, **no existe rama todavía**.
  2. Cuando alguien **toma** la tarea (se la asigna), en ese momento se crea la rama correspondiente. La conexión con el control de versiones (Unity Version Control) para crear la rama se hace desde el backend en **Rust**, invocando directamente el CLI `cm` — el frontend solo dispara la acción "tomar tarea" y refleja el resultado (rama creada / en progreso / error).
  3. Los **sprints** representan el tiempo de vida de esa rama trabajando sobre `dev`, antes de integrarse (mergear) a `main`.
- **Sprints y tareas, con y sin tiempo**: tanto los sprints como las tareas deben soportar dos variantes: **con fecha/duración** (planeados en el tiempo, estilo Scrum clásico) y **sin fecha** (tipo backlog, sin compromiso de tiempo). Es una decisión ya tomada — falta solo el diseño de cómo se ven en la interfaz. No es prioridad implementar aún la parte de sprints con fechas/calendario, pero el modelo de datos y la UI deben contemplarlo desde ya para no rehacer luego.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); el backend en Rust invoca directamente el CLI `cm` de Unity Version Control y usa SQLite — sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React); como Tauri no corre en navegador, esa fase necesita un backend servidor nuevo y aparte que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — otro tablero distinto al Scrum (su diferencia exacta con el Scrum aún no está definida — pendiente de aclarar con el usuario).
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (frontend) y otra en [BACKEND.md](./BACKEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Alcance

- Tablero Scrum (kanban)
- Pantalla de resumen con gráfica circular (dona) de estados
- Lista de todas las tareas existentes, con formulario/flujo de creación de tarea (sin rama aún)
- Acción "tomar tarea": botón/interacción que dispara la creación de rama en el backend y refleja el estado (creando / lista / error)
- Vista de sprints, soportando sprints con fecha y sprints sin fecha
- Tareas con fecha y tareas sin fecha (distinción visual en tarjetas/listas)
- Tablero "Avance": gráfica de **curva en el tiempo** (tipo burndown/burnup) mostrando cantidad de tareas vs. tiempo/sprints. Misma librería de gráficos sugerida para "Resumen" (ej. Chart.js, ApexCharts) sirve para esta curva.
- "Branches": gráfica de ascendencia/linaje de ramas (árbol, no kanban) — `dev` como raíz, ramas de tareas generales, ramas hijas (subtareas/mini-tareas) colgando de su padre, coloreadas según estado. Ver definición completa en [PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp). Librería sugerida: `react-flow`, alimentada con datos de `cm` (lista de ramas + historial de merge) combinados con el estado de cada tarea en SQLite.

## Autenticación / acceso al proyecto

**Actualizado**: BranchBoard no tiene login propio ni contraseña — ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario). La identidad se obtiene automáticamente de `cm whoami` (la sesión de Unity Version Control ya iniciada en la máquina). Esto reemplaza el formulario de credenciales que se había planteado antes.

Pantallas de entrada a la aplicación, antes de llegar a los tableros. **Foco actual: el flujo de "ingresar a un proyecto existente". "Crear un nuevo proyecto" queda en blanco/stub por ahora, no se implementa todavía.**

1. **Pantalla inicial**: incluye un **switch de "Conexión VPN" (Cloudflare WARP)**, visible siempre en esta pantalla — enciende/apaga la VPN desde la propia app (ver [Conexión VPN (Cloudflare WARP) en BACKEND.md](./BACKEND.md#conexión-vpn-cloudflare-warp)). Necesario porque en redes como la de la universidad, `cm` no conecta sin la VPN activa; el switch evita que el usuario tenga que ir a buscar el ícono de WARP fuera de la app. Si `warp-cli` no está instalado en la máquina, el switch aparece deshabilitado con instrucciones para instalarlo.

   Además, dos opciones —
   - "Crear un nuevo proyecto" → **stub/en blanco por ahora, no tocar**
   - "Ingresar a un proyecto existente" → la app lista los **proyectos/workspaces de Unity Version Control a los que el usuario está conectado** (vía `cm`), no un solo proyecto fijo:
     - El usuario selecciona uno de la lista
     - La app revisa si ese proyecto **coincide con una base de datos de BranchBoard** ya existente (el `.db` asociado a ese workspace):
       - **Coincide** → corre `cm whoami` para identificar al usuario, busca su correo en la tabla de usuarios de ese proyecto, y si está registrado, entra mostrando todo el contenido (tableros, tareas, etc.) con el rol asignado
       - **El correo no está registrado en ese proyecto** → pantalla mostrando "tu cuenta (correo detectado) no está registrada en este proyecto todavía", con botón "Solicitar ingreso a una organización" (punto 2)
       - **No coincide con ningún `.db` de BranchBoard** (proyecto de Unity VC que nunca se ha configurado con BranchBoard) → pendiente definir qué se muestra (¿opción de inicializarlo? ver "Pendiente por definir")

2. **Pantalla de solicitud de ingreso a una organización**:
   - Campos: Nombre, Apellido, Correo (idealmente pre-rellenado con el correo detectado por `cm whoami`, ya que debe ser el mismo con el que luego se valide), ID de la organización
   - Al enviar, se genera una solicitud de ingreso al proyecto/organización
   - Si la solicitud es aceptada, el Admin/Líder asocia ese correo a un rol en la tabla de usuarios — no se genera ni se envía ninguna clave/contraseña. El solicitante recibe un correo avisando que fue aceptado, y la próxima vez que abra BranchBoard, `cm whoami` ya lo identifica y entra directo.

## Roles y permisos (UI)

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 4 roles confirmados (Administrador, Líder, Desarrollador, Tester). La UI debe mostrar/ocultar acciones y pantallas según el rol del usuario logueado (ej. "eliminar proyecto" solo visible para Administrador; "aprobar solicitud de ingreso" visible para Líder y Administrador). El Desarrollador necesita una notificación/mensaje in-app cuando recibe autorización para modificar una tarea.

Cuando un Desarrollador crea una tarea, la interfaz debe dejar claro que nace **bloqueada** (ej. una etiqueta o color distinto en la tarjeta) hasta que un Líder/Administrador la apruebe — ver [Pendiente por definir en SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos).

## Notificaciones

Ícono de campana (visible para todos los roles) con contador de no leídas, alimentado por los comandos Tauri `obtener_notificaciones()` / `marcar_leida(id)` (ver [BACKEND.md](./BACKEND.md#notificaciones)). Casos que debe cubrir el MVP:

1. **Líder/Administrador**: "una tarea creada por [Desarrollador] espera tu aprobación" — con acción directa de aprobar desde la misma notificación
2. **Desarrollador**: "tienes autorización para modificar [tarea]"
3. **Quien solicitó ingreso**: "tu solicitud de ingreso a la organización fue aceptada"

## Pendiente por definir

- Componentes de React por pantalla
- Cómo se ve una tarjeta de tarea (qué datos muestra, y cómo cambia antes/después de tener rama asociada)
- Librería de gráficos para la dona y la curva de Avance (ej. Chart.js, ApexCharts)
- Qué mide exactamente el eje Y de la curva de Avance y qué marca el eje X (ver mismo pendiente en PLANNING.md)
- Diseño de la vista de sprint (cómo se agrupan tareas dentro de un sprint, con y sin fechas)
- Estado de carga/error al "tomar" una tarea mientras el backend crea la rama (llamada async a un comando Tauri)
- Qué pasa si alguien intenta "crear un nuevo proyecto" antes de que esa pantalla esté implementada (¿deshabilitado? ¿mensaje "próximamente"?)
- Validaciones del formulario de solicitud de ingreso (formato de correo, ID de organización válido, y que el correo puesto coincida con el que da `cm whoami`)
- Quién/qué aprueba la solicitud de ingreso (¿un admin desde otra pantalla? es responsabilidad de backend, pero el frontend necesita saber si hay una pantalla de "solicitudes pendientes" para el admin)
- Qué pasa si `cm whoami` falla o no hay sesión de Unity VC iniciada en la máquina (mensaje de error, instrucción de iniciar sesión en Unity VC primero)
- Comando `cm` exacto para listar los proyectos/workspaces a los que el usuario está conectado (para la lista de "ingresar a un proyecto existente")
- Qué se muestra cuando un proyecto de Unity VC listado **no tiene** un `.db` de BranchBoard asociado todavía (¿se ofrece inicializarlo ahí mismo? ¿se oculta de la lista?)
