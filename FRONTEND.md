# BranchBoard — Frontend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que se ve en pantalla, con Vue dentro de Tauri.

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
  2. Cuando alguien **toma** la tarea (se la asigna), en ese momento se crea la rama correspondiente. La conexión con el control de versiones (Unity Version Control) para crear la rama se hace mediante **scripts Python** del lado del backend — el frontend solo dispara la acción "tomar tarea" y refleja el resultado (rama creada / en progreso / error).
  3. Los **sprints** representan el tiempo de vida de esa rama trabajando sobre `dev`, antes de integrarse (mergear) a `main`.
- **Sprints y tareas, con y sin tiempo**: tanto los sprints como las tareas deben soportar dos variantes: **con fecha/duración** (planeados en el tiempo, estilo Scrum clásico) y **sin fecha** (tipo backlog, sin compromiso de tiempo). Es una decisión ya tomada — falta solo el diseño de cómo se ven en la interfaz. No es prioridad implementar aún la parte de sprints con fechas/calendario, pero el modelo de datos y la UI deben contemplarlo desde ya para no rehacer luego.
- **Stack elegido**: Tauri + Vue. Fase 1 = app de escritorio (full desktop). Fase 2 (futuro) = versión web, con el mismo código gracias a Tauri.
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
- Tablero "Avance"

## Autenticación / acceso al proyecto

Pantallas de entrada a la aplicación, antes de llegar a los tableros. **Foco actual: el flujo de "ingresar a un proyecto existente". "Crear un nuevo proyecto" queda en blanco/stub por ahora, no se implementa todavía.**

1. **Pantalla inicial**: dos opciones —
   - "Crear un nuevo proyecto" → **stub/en blanco por ahora, no tocar**
   - "Ingresar a un proyecto existente" → lleva a la pantalla de login (punto 2)

2. **Pantalla de login** (ingresar a proyecto existente):
   - Campo(s) de credenciales
   - Botón "ver" (mostrar/ocultar contraseña)
   - Botón "guardar sesión"
   - Botón/enlace, debajo de lo anterior: "Solicitar ingreso a una organización" → lleva a la pantalla de solicitud (punto 3)

3. **Pantalla de solicitud de ingreso a una organización**:
   - Campos: Nombre, Apellido, Correo, ID de la organización
   - Al enviar, se genera una solicitud de ingreso al proyecto/organización
   - Si la solicitud es aceptada, el solicitante recibe un correo (al que puso en el formulario) con un mensaje que contiene la **clave de acceso** para entrar al sistema

## Roles y permisos (UI)

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 3 roles confirmados (Administrador, Líder, Desarrollador; falta un 4to por confirmar). La UI debe mostrar/ocultar acciones y pantallas según el rol del usuario logueado (ej. "eliminar proyecto" solo visible para Administrador; "aprobar solicitud de ingreso" visible para Líder y Administrador). El Desarrollador necesita una notificación/mensaje in-app cuando recibe autorización para modificar una tarea.

## Pendiente por definir

- Componentes de Vue por pantalla
- Cómo se ve una tarjeta de tarea (qué datos muestra, y cómo cambia antes/después de tener rama asociada)
- Diferencia visual/funcional entre tablero Scrum y tablero Avance
- Librería de gráficos para la dona (ej. Chart.js, ApexCharts)
- Diseño de la vista de sprint (cómo se agrupan tareas dentro de un sprint, con y sin fechas)
- Estado de carga/error al "tomar" una tarea mientras el backend crea la rama (llamada async a un comando Tauri)
- Campos exactos de "credenciales" en el login (¿usuario, correo, o ambos + contraseña?)
- Qué pasa si alguien intenta "crear un nuevo proyecto" antes de que esa pantalla esté implementada (¿deshabilitado? ¿mensaje "próximamente"?)
- Validaciones del formulario de solicitud de ingreso (formato de correo, ID de organización válido)
- Quién/qué aprueba la solicitud de ingreso (¿un admin desde otra pantalla? es responsabilidad de backend, pero el frontend necesita saber si hay una pantalla de "solicitudes pendientes" para el admin)
