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
  3. Los **sprints** representan el tiempo de vida de esa rama antes de mergear a `dev`. El merge de `dev` hacia `main` (release) es manual, fuera del alcance de BranchBoard.
- **Sprints y tareas, con y sin tiempo**: tanto los sprints como las tareas deben soportar dos variantes: **con fecha/duración** (planeados en el tiempo, estilo Scrum clásico) y **sin fecha** (tipo backlog, sin compromiso de tiempo). Es una decisión ya tomada — falta solo el diseño de cómo se ven en la interfaz. No es prioridad implementar aún la parte de sprints con fechas/calendario, pero el modelo de datos y la UI deben contemplarlo desde ya para no rehacer luego.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); el backend en Rust invoca directamente el CLI `cm` de Unity Version Control y usa SQLite — sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React); como Tauri no corre en navegador, esa fase necesita un backend servidor nuevo y aparte que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — gráfica de curva en el tiempo (tareas resueltas por día + changesets por día).
  5. **"Branches"** — árbol de linaje de ramas (`dev` → tareas → subtareas anidadas), coloreado por estado.
  6. **"Merging"** — página tipo estado de PR, lista las tareas en "en revisión" pendientes de autorizar/rechazar.
  7. **"Ajustes"** — agrupa gestión de usuarios, solicitudes de ingreso, backups y eliminar proyecto.
  8. **"Categorías"** — explorador de carpetas por tipo de tarea (Scripting, Modelado 3D, Modelado 2D, Animación, Imagen, UI/Interfaz, Audio, Música, Video, Testing), solo para filtrar, sin relación con ramas.
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (frontend) y otra en [BACKEND.md](./BACKEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Navegación general

Menú lateral fijo, visible siempre una vez dentro de un proyecto, con ícono + etiqueta corta por pantalla:

```
🗂  Tareas (Scrum)
📊  Resumen
📋  Lista de tareas
📈  Avance
🌳  Branches
🔀  Merging
🐞  Testing
🗃  Categorías
📄  Reportes
⚙️  Ajustes
```

Barra superior persistente: switch de VPN, campana de notificaciones, botón "Abrir en Unity", identidad (correo + rol).

**Botón de regreso en la navegación tipo carpetas**: al entrar a una tarea padre en el Tablero Scrum (ver [Navegación tipo explorador de carpetas en PLANNING.md](./PLANNING.md#navegación-tipo-explorador-de-carpetas-en-el-tablero-scrum)), aparece una migaja de pan (breadcrumb) arriba, ej. `Tareas > Modelar personaje > Textura de cara`, con cada nivel clickeable para regresar directo a él — no solo "un paso atrás", sino saltar a cualquier nivel superior de una vez.

**Pendiente/dudas a resolver** (marcando explícitamente lo que no tengo claro, en vez de asumir):
- **Visibilidad de "Testing" en el menú**: ¿aparece para todos los roles en modo lectura (como Merging), o solo el Tester lo ve?
- **Cerrar la app**: si hay una acción en curso (ej. sincronizando, creando una rama), ¿se bloquea el cierre con una advertencia, o simplemente se puede cerrar y esa operación queda a medias? Tampoco está definido si hace falta algún tipo de "guardado" al cerrar, dado que las escrituras a SQLite son inmediatas por acción (no hay un estado de app sin guardar en el sentido tradicional).
- **Confirmación al cerrar sesión/cambiar de proyecto**: como no hay login/logout real (es `cm whoami`), ¿"cambiar de proyecto" desde dentro de la app vuelve a la Pantalla inicial sin más, o pide confirmar si hay algo en curso?

## Alcance

- Tablero Scrum (kanban)
- Pantalla de resumen con gráfica circular (dona) de estados
- Lista de todas las tareas existentes, con formulario/flujo de creación de tarea (sin rama aún)
- Acción "tomar tarea": botón/interacción que dispara la creación de rama en el backend y refleja el estado (creando / lista / error)
- Vista de sprints, soportando sprints con fecha y sprints sin fecha
- Tareas con fecha y tareas sin fecha (distinción visual en tarjetas/listas)
- Tablero "Avance": gráfica de **curva en el tiempo** (tipo burndown/burnup) mostrando cantidad de tareas vs. tiempo/sprints. Misma librería de gráficos sugerida para "Resumen" (ej. Chart.js, ApexCharts) sirve para esta curva.
- "Branches": gráfica de ascendencia/linaje de ramas (árbol, no kanban) — `dev` como raíz, ramas de tareas generales, ramas hijas (subtareas anidadas) colgando de su padre, coloreadas según estado. Ver definición completa en [PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp). Librería sugerida: `react-flow`, alimentada con datos de `cm` (lista de ramas + historial de merge) combinados con el estado de cada tarea en SQLite.
- Tablero Scrum: navegación tipo "explorador de carpetas" para las tareas con hijos — clic en una tarea padre enfoca la vista en ella y sus hijos directos, ocultando las ramas hermanas; repetible recursivamente hasta 3 niveles. Ver definición en [PLANNING.md](./PLANNING.md#navegación-tipo-explorador-de-carpetas-en-el-tablero-scrum).
- **"Merging"**: página tipo estado de PR, lista todas las tareas en "en revisión" (ver [PLANNING.md](./PLANNING.md#merging)). Para Desarrollador y Tester es **solo lectura** (la UI oculta los botones de acción, sin necesidad de ocultar la página entera). Para Líder/Administrador, cada fila tiene botones "Autorizar" / "Rechazar" (con el doble candado ya descrito). El Administrador además ve un selector para **asignarle la revisión a un Líder específico** (`asignar_revision`, ver [BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)) — cuando una tarea tiene Líder asignado, se muestra su nombre en la fila.
- **"Categorías"**: pantalla nueva, distinta de la navegación tipo explorador de carpetas del Tablero Scrum (esa es por jerarquía padre/hijo de tareas; esta es por **tipo/disciplina**). Son **carpetas puramente visuales, nunca ramas** — Scripting, Modelado 3D, Modelado 2D, Animación, Imagen, UI/Interfaz, Audio, Música, Video, Testing. Abrir una carpeta filtra la Lista de tareas por ese tipo, es la misma tabla/tarjetas de siempre, solo con un filtro aplicado.

## Confirmación al arrastrar tareas (drag & drop)

En el Tablero Scrum, arrastrar una tarjeta no es un simple cambio visual — dispara acciones reales sobre Unity Version Control (crear rama, mandar a revisión, mergear a `dev`). Por eso, ciertos arrastres piden confirmación antes de ejecutarse:

- **Tomar tarea / auto-asignarse** (crea la rama): modal con descripción + campo de confirmación donde el usuario debe **escribir literalmente "asignar"** para habilitar el botón — sin eso no se puede continuar.
- **Arrastrar a "En revisión"**: modal con descripción + campo de confirmación donde el usuario debe **escribir literalmente "mandar a revisar"** para habilitar el botón de envío.
  - Al confirmar, la tarea pasa a "en revisión" con la descripción guardada, y aparece en el apartado [Merging](#alcance) de Líder/Administrador
- En ambos casos: si el usuario cancela o cierra el modal sin completar los dos pasos, la tarjeta **regresa visualmente a su columna original**, nunca queda a medias
- **Líder marcando "autorizado"** (mergea la rama a `dev`): pendiente confirmar si aplica el mismo doble candado (descripción + palabra literal) y cuál sería la palabra, dado lo delicado de la acción.

**Eliminar una tarea** (solo Administrador o Líder, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): mismo patrón, no es un arrastre pero sí una acción destructiva —
- Campo de texto obligatorio para la **justificación** de por qué se elimina
- Campo de confirmación donde debe **escribir literalmente "eliminar"** para habilitar el botón
- Motivo: queda registro en Unity Version Control, así que la justificación importa para el historial del equipo, no es un borrado silencioso

**Eliminar el proyecto completo** (solo Administrador, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): un escalón más estricto que eliminar una tarea, porque borra todo, no un elemento —
- En vez de escribir una palabra genérica, el Administrador debe **escribir el nombre exacto del proyecto** para habilitar el botón (mismo patrón que usa GitHub al borrar un repositorio)
- Sin campo de justificación (no aplica igual que en una tarea, ya no queda nada donde guardarla) — la acción en sí es la última que se registra

**Restaurar backup de la base de datos** (solo Administrador, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos), lógica en [BACKEND.md](./BACKEND.md#copias-de-seguridad-y-restauración-de-la-bd)): pantalla que lista los backups automáticos disponibles con fecha, y al elegir uno pide el mismo doble candado que las demás acciones destructivas (confirmación + palabra literal, pendiente definir cuál). Un Líder o Desarrollador que quiera pedir esto no ve este botón — en su lugar tienen un botón **"Reportar problema con la base de datos"** que envía la notificación a los Administradores con un campo de motivo (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 9).

## Ajustes

Pantalla nueva que agrupa las acciones administrativas — antes estaban definidas sueltas sin un lugar fijo donde vivir. Se organiza en secciones, cada una visible según el rol de quien la abre:

1. **Solicitudes de ingreso** (Líder y Administrador): lista las filas de `solicitudes_ingreso` en estado "pendiente" — nombre, apellido, correo, ID de organización, fecha. Cada fila tiene:
   - **"Aceptar"**: abre un selector de rol (Administrador/Líder/Desarrollador/Tester — con las mismas restricciones ya definidas, ej. un Líder no puede asignar el rol Líder) y confirma, insertando la fila en `usuarios`
   - **"Rechazar"**: marca la solicitud como rechazada, sin crear usuario
   - Las solicitudes ya resueltas (aceptada/rechazada) se pueden ver como historial debajo, colapsado por defecto

2. **Gestión de usuarios** (Líder y Administrador, con matices de permiso): tabla con todos los usuarios actuales del proyecto (email, nombre, rol). Botón "Eliminar" por fila:
   - Administrador: puede eliminar a cualquiera
   - Líder: solo puede eliminar Desarrolladores (el botón no aparece sobre otro Líder ni sobre el Administrador)
   - El Administrador además tiene un botón **"Ascender a Líder"** sobre las filas de Desarrollador/Tester (exclusivo suyo, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos))

3. **Copias de seguridad**:
   - Administrador: ve la lista de backups con fecha (`listar_backups()`) y el botón "Restaurar" con su doble candado (ver [Confirmación al arrastrar tareas](#confirmación-al-arrastrar-tareas-drag--drop))
   - Líder/Desarrollador: en vez de la lista, ven el botón **"Reportar problema con la base de datos"** (campo de motivo → notificación a los Administradores)

4. **Zona de peligro** (solo Administrador): el botón de **"Eliminar proyecto completo"**, con el candado de escribir el nombre exacto del proyecto — separado visualmente del resto (ej. al final, con estilo de advertencia) para que no se confunda con una acción cualquiera

5. **Permisos de subtareas simples** (Líder y Administrador): panel donde se gestiona **quién puede crear subtareas simples** (colaboradores con workspace parcial vía Gluon — sí tienen `cm` instalado, pero nunca el proyecto completo, ver [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples)) — a diferencia de un Desarrollador normal, estas personas no pueden hacerlo libremente.
   - Lista de correos con permiso otorgado, con botón para agregar uno nuevo (nombre + correo) y otro para revocarlo
   - Al revocar, esa persona deja de poder subir nada la próxima vez que lo intente

## Animación de inicio (splash)

Al abrir BranchBoard, antes de mostrar la Pantalla inicial, se muestra una **animación de splash** (logo animado) mientras corren en paralelo las verificaciones de arranque: `estado_warp()`, y si WARP está conectado, `cm whoami` + listar workspaces (ver [Autenticación / acceso al proyecto](#autenticación--acceso-al-proyecto)). Sirve para no mostrar una pantalla en blanco/congelada mientras esas llamadas async resuelven.

- Duración: hasta que terminen las verificaciones, con un mínimo breve (ej. ~600ms) para que no se sienta como un parpadeo si todo responde instantáneo.
- Pendiente definir: estilo exacto de la animación (logo con loop simple vs. algo más elaborado) — no es prioridad de lógica, es una mejora de percepción de carga.

## Autenticación / acceso al proyecto

**Actualizado**: BranchBoard no tiene login propio ni contraseña — ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario). La identidad se obtiene automáticamente de `cm whoami` (la sesión de Unity Version Control ya iniciada en la máquina). Esto reemplaza el formulario de credenciales que se había planteado antes.

Pantallas de entrada a la aplicación, antes de llegar a los tableros. **Foco actual: el flujo de "ingresar a un proyecto existente". "Crear un nuevo proyecto" queda en blanco/stub por ahora, no se implementa todavía.**

1. **Pantalla inicial**: incluye un **switch de "Conexión VPN" (Cloudflare WARP)**, visible siempre en esta pantalla — enciende/apaga la VPN desde la propia app (ver [Conexión VPN (Cloudflare WARP) en BACKEND.md](./BACKEND.md#conexión-vpn-cloudflare-warp)). Necesario porque en redes como la de la universidad, `cm` no conecta sin la VPN activa; el switch evita que el usuario tenga que ir a buscar el ícono de WARP fuera de la app. Si `warp-cli` no está instalado en la máquina, el switch aparece deshabilitado con instrucciones para instalarlo.

   Además, dos opciones —
   - "Crear un nuevo proyecto" → **stub/en blanco por ahora, no tocar**
   - "Ingresar a un proyecto existente" → la app lista los **proyectos/workspaces de Unity Version Control a los que el usuario está conectado**, no un solo proyecto fijo. **Confirmado en pruebas reales**: no se le pide al usuario apuntar manualmente a la carpeta del proyecto clonado — el comando `cm workspace list` ya devuelve todos los workspaces registrados en esa máquina con su ruta local (ej. `VirusTF-VirusTF@MIGUELMEZA  d:\...\VirusTF`), sin importar en qué carpeta estén clonados. La regla no depende del rol, depende de si **ya existe un workspace o no**:
     - **Ya tiene un workspace clonado** (caso típico de Administrador/Líder, y de Desarrollador si ya trabaja en Unity): la app se conecta directo a ese repo/workspace vía `cm` — nunca se "abre" ni navega la carpeta, es solo una referencia interna.
     - **No tiene ningún workspace todavía, pero necesita uno** (Desarrollador o Tester nuevo, sin clonar aún): se le pide instalar Unity Version Control (si hace falta) y luego se le ofrece un botón **"Clonar proyecto"** directamente dentro de BranchBoard — abre el selector de carpetas nativo para elegir dónde, y de ahí BranchBoard corre `cm workspace create` + checkout mostrando el progreso. No hace falta que la persona vaya a Unity Hub por su cuenta a clonarlo antes de poder usar la app.
     - **Colaborador de subtareas simples**: **corregido** — sí necesita Unity Version Control instalado (mismo botón de instalar que los demás), pero el workspace es **parcial** (Gluon/`cm partial`, ver [BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples)) — nunca descarga el proyecto completo, solo la carpeta de su archivo. Su identidad sigue siendo `cm whoami` como todos los demás.
     - El usuario selecciona uno de la lista (si hay más de uno)
     - La app revisa si ese proyecto **coincide con una base de datos de BranchBoard** ya existente (el `.db` asociado a ese workspace):
       - **Coincide** → corre `cm whoami` para identificar al usuario, busca su correo en la tabla de usuarios de ese proyecto, y si está registrado, entra mostrando todo el contenido (tableros, tareas, etc.) con el rol asignado
       - **El correo no está registrado en ese proyecto** → pantalla mostrando "tu cuenta (correo detectado) no está registrada en este proyecto todavía", con botón "Solicitar ingreso a una organización" (punto 2)
       - **No coincide con ningún `.db` de BranchBoard** (proyecto de Unity VC que nunca se ha configurado con BranchBoard) → se ofrece **inicializarlo ahí mismo**. Quien lo inicializa queda asignado automáticamente como **Líder** de ese proyecto (no Administrador — ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario)). Mismo criterio aplica a "Crear un nuevo proyecto" cuando se implemente (sigue siendo stub por ahora).

**Panel "Abrir en Unity"**: una vez dentro de un proyecto cargado (después de todo el flujo de la Pantalla inicial), aparece de forma general dentro de la app — no escondido en Ajustes — un botón para lanzar el Editor de Unity apuntando a la carpeta del workspace local. Mismo patrón de invocar un ejecutable externo ya usado para `cm` y `warp-cli` (ver [Comando "Abrir en Unity" en BACKEND.md](./BACKEND.md#comando-abrir-en-unity)). Solo visible para roles con workspace local (Administrador/Líder/Desarrollador/Tester) — no aplica a colaboradores de subtareas simples.

2. **Pantalla de solicitud de ingreso a una organización**:
   - Campos: Nombre, Apellido, Correo (idealmente pre-rellenado con el correo detectado por `cm whoami`, ya que debe ser el mismo con el que luego se valide), ID de la organización
   - Al enviar, se genera una solicitud de ingreso al proyecto/organización
   - Si la solicitud es aceptada, el Admin/Líder asocia ese correo a un rol en la tabla de usuarios — no se genera ni se envía ninguna clave/contraseña. El solicitante recibe un correo avisando que fue aceptado, y la próxima vez que abra BranchBoard, `cm whoami` ya lo identifica y entra directo.

## Roles y permisos (UI)

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 4 roles confirmados (Administrador, Líder, Desarrollador, Tester). La UI debe mostrar/ocultar acciones y pantallas según el rol del usuario logueado (ej. "eliminar proyecto" solo visible para Administrador; "aprobar solicitud de ingreso" visible para Líder y Administrador). El Desarrollador necesita una notificación/mensaje in-app cuando recibe autorización para modificar una tarea.

Cuando un Desarrollador crea una tarea, la interfaz debe dejar claro que nace **bloqueada** (ej. una etiqueta o color distinto en la tarjeta) hasta que un Líder/Administrador la apruebe — ver [Pendiente por definir en SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos).

## Notificaciones

Ícono de campana (visible para todos los roles) con contador de no leídas, alimentado por los comandos Tauri `obtener_notificaciones()` / `marcar_leida(id)` (ver [BACKEND.md](./BACKEND.md#notificaciones)). Casos que debe cubrir el MVP:

1. **Líder/Administrador**: "una tarea creada por [Desarrollador] espera tu aprobación" — con acción directa de aprobar o **rechazar** desde la misma notificación (ver `aprobar_tarea` / `rechazar_tarea` en [BACKEND.md](./BACKEND.md#notificaciones))
2. **Desarrollador**: "tu tarea [tarea] fue rechazada: [motivo]"
3. **Desarrollador**: "se te asignó la tarea [tarea]" (cuando un Líder/Administrador la asigna directamente, no autoasignación)
4. **Desarrollador**: "tienes autorización para modificar [tarea]"
5. **Líder/Administrador**: "la tarea [tarea] pasó a revisión"
6. **Quien solicitó ingreso**: "tu solicitud de ingreso a la organización fue aceptada"
7. **Desarrollador**: recordatorio de inactividad — "llevas X días sin actividad en [tarea]" (generado automáticamente por el backend, sin que nadie lo envíe manualmente)

## Pendiente por definir

- Componentes de React por pantalla
- Cómo se ve una tarjeta de tarea (qué datos muestra, y cómo cambia antes/después de tener rama asociada)
- Librería de gráficos para la dona y la curva de Avance (ej. Chart.js, ApexCharts)
- Diseño de la vista de sprint (cómo se agrupan tareas dentro de un sprint, con y sin fechas)
- Estado de carga/error al "tomar" una tarea mientras el backend crea la rama (llamada async a un comando Tauri)
- Qué pasa si alguien intenta "crear un nuevo proyecto" antes de que esa pantalla esté implementada (¿deshabilitado? ¿mensaje "próximamente"?)
- Validaciones del formulario de solicitud de ingreso (formato de correo, ID de organización válido, y que el correo puesto coincida con el que da `cm whoami`)
- Qué pasa si `cm whoami` falla o no hay sesión de Unity VC iniciada en la máquina (mensaje de error, instrucción de iniciar sesión en Unity VC primero)
