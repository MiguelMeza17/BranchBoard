# BranchBoard — Frontend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que se ve en pantalla, con React + TypeScript dentro de Tauri.

## Contexto completo del proyecto

- **Nombre**: BranchBoard
- **Qué es**: herramienta de gestión de tareas y producción para un equipo pequeño de desarrollo de un videojuego en Unity.
- **Principio central**: todo el trabajo del equipo se organiza y visualiza a través de **tareas**. Cada tarea puede ser de tipo (ver [BACKEND.md](./BACKEND.md#contexto-completo-del-proyecto)):
  - **Scripting** → asociada a una rama (una tarea = una rama)
  - **Modelado 3D**
  - **Modelado 2D**
  - **Animación**
  - **Imagen** (arte 2D)
  - **UI/Interfaz**
  - **Audio**
  - **Música**
  - **Video**
- **Flujo de vida de una tarea**:
  1. Se crea la tarea desde la **Lista de tareas** — en este punto es puramente visual/informativa, **no existe rama todavía**.
  2. Cuando alguien **toma** la tarea (se la asigna), en ese momento se crea la rama correspondiente. La conexión con el control de versiones (Unity Version Control) para crear la rama se hace desde el backend en **Rust**, invocando directamente el CLI `cm` — el frontend solo dispara la acción "tomar tarea" y refleja el resultado (rama creada / en progreso / error).
  3. Los **sprints** representan el tiempo de vida de esa rama antes de mergear a `dev`. El merge de `dev` hacia `main` (release) es manual, fuera del alcance de BranchBoard.
- **Sprints y tareas, con y sin tiempo**: tanto los sprints como las tareas deben soportar dos variantes: **con fecha/duración** (planeados en el tiempo, estilo Scrum clásico) y **sin fecha** (tipo backlog, sin compromiso de tiempo). Es una decisión ya tomada — falta solo el diseño de cómo se ven en la interfaz. No es prioridad implementar aún la parte de sprints con fechas/calendario, pero el modelo de datos y la UI deben contemplarlo desde ya para no rehacer luego.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); el backend en Rust invoca directamente el CLI `cm` de Unity Version Control y usa SQLite — sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React); como Tauri no corre en navegador, esa fase necesita un backend servidor nuevo y aparte que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP**: ver lista completa y autoritativa en [Pantallas / tableros del MVP en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp) — no se duplica aquí para evitar que este documento quede desactualizado cuando se agreguen pantallas nuevas.
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (frontend) y otra en [BACKEND.md](./BACKEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Navegación general

Menú lateral fijo, visible siempre una vez dentro de un proyecto, con ícono + etiqueta corta por pantalla:

```
🗂  Tareas (Scrum)
📊  Resumen
📋  Lista de tareas
📈  Avance
🌳  Branches
🔀  Revisión
🐞  Testing
📌  Pendientes
🗑  Papelera
🗃  Categorías
📄  Reportes
⚙️  Ajustes
```

**Menú lateral desplegable**: un botón de tres rayas (☰) en la cabecera del menú lo contrae a una franja de solo íconos (64px) o lo vuelve a expandir (240px). Contraído, cada ícono muestra su etiqueta como tooltip, y los contadores (Pendientes, Papelera, Revisión) y puntos de aviso se superponen en la esquina del ícono. El contenido principal aprovecha el espacio liberado (su ancho máximo pasa de 1400px a 1640px). La preferencia se recuerda localmente entre sesiones.

Barra superior persistente: switch de VPN, campana de notificaciones, botón "Abrir en Unity", identidad (correo + rol).

**Botón de regreso en la navegación tipo carpetas**: al entrar a una tarea padre en el Tablero Scrum (ver [Navegación tipo explorador de carpetas en PLANNING.md](./PLANNING.md#navegación-tipo-explorador-de-carpetas-en-el-tablero-scrum)), aparece una migaja de pan (breadcrumb) arriba, ej. `Tareas > Modelar personaje > Textura de cara`, con cada nivel clickeable para regresar directo a él — no solo "un paso atrás", sino saltar a cualquier nivel superior de una vez.

**Corregido (validado en el mockup de UI)**: la tarea padre **nunca se mezcla como una tarjeta más** dentro de las columnas de sus propias subtareas — eso confundía, parecía que una tarea tenía una subtarea con su propio nombre. En vez de eso, al entrar a su carpeta, la tarea padre se muestra en una **franja de contexto separada** (debajo del breadcrumb, encima de las columnas): título, badges de tipo/estado, responsable, y un botón explícito "← Volver". Las columnas de abajo solo listan las subtareas reales.

**Corregido — una tarea con subtareas no debe mostrar su propio tipo como si fuera lo único que contiene**: una tarea general de tipo Scripting con subtareas de Modelado 3D y Audio no es "solo código" — es un contenedor de varias disciplinas. Por eso, tanto en la tarjeta como en la franja de contexto de arriba, **una tarea con subtareas no muestra su badge de tipo propio** — en su lugar muestra un ícono por cada tipo distinto que contiene en todo su subárbol (hijas, nietas, etc.), con un número si hay más de una del mismo tipo. **Cada ícono es un botón**: al hacer clic, salta directo a esa subtarea específica (entra a la carpeta que la contiene y abre su detalle) — sin tener que navegar carpeta por carpeta a mano. Una tarea sin subtareas (hoja) sigue mostrando su badge de tipo normal, sin cambios.

**Pendiente/dudas a resolver** (marcando explícitamente lo que no tengo claro, en vez de asumir):
- **Visibilidad de "Testing" en el menú**: ¿aparece para todos los roles en modo lectura, o solo el Tester lo ve?
- **Cerrar la app**: si hay una acción en curso (ej. sincronizando, creando una rama), ¿se bloquea el cierre con una advertencia, o simplemente se puede cerrar y esa operación queda a medias? Tampoco está definido si hace falta algún tipo de "guardado" al cerrar, dado que las escrituras a SQLite son inmediatas por acción (no hay un estado de app sin guardar en el sentido tradicional).
- **Confirmación al cerrar sesión/cambiar de proyecto**: como no hay login/logout real (es `cm whoami`), ¿"cambiar de proyecto" desde dentro de la app vuelve a la Pantalla inicial sin más, o pide confirmar si hay algo en curso?

## Alcance

- Tablero Scrum (kanban)
- Pantalla de resumen con gráfica circular (dona) de estados
- Lista de todas las tareas existentes, con formulario/flujo de creación de tarea (sin rama aún). **Definido**: si quien crea la tarea es un Desarrollador o Tester, antes de guardarla se muestra un aviso que debe leer — algo como "esta tarea quedará pendiente de aprobación de un Líder/Administrador antes de poder mandarse a revisión, pero ya puedes tomarla y trabajarla" — con un botón para continuar (no es un doble candado de escribir una palabra, solo un aviso que debe cerrar para seguir). No aplica si la crea un Líder/Administrador.
- Acción "tomar tarea": botón/interacción que dispara la creación de rama en el backend y refleja el estado (creando / lista / error)
- Vista de sprints, soportando sprints con fecha y sprints sin fecha
- Tareas con fecha y tareas sin fecha (distinción visual en tarjetas/listas)
- Tablero "Avance": gráfica de **curva en el tiempo** (tipo burndown/burnup) mostrando cantidad de tareas vs. tiempo/sprints. Misma librería de gráficos sugerida para "Resumen" (ej. Chart.js, ApexCharts) sirve para esta curva.
- "Branches": gráfica de ascendencia/linaje de ramas (árbol, no kanban) — `dev` como raíz, ramas de tareas generales, ramas hijas (subtareas anidadas) colgando de su padre, coloreadas según estado. Ver definición completa en [PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp). Librería sugerida: `react-flow`, alimentada con datos de `cm` (lista de ramas + historial de merge) combinados con el estado de cada tarea en SQLite.
- Tablero Scrum: navegación tipo "explorador de carpetas" para las tareas con hijos — clic en una tarea padre enfoca la vista en ella y sus hijos directos, ocultando las ramas hermanas; repetible recursivamente hasta 3 niveles. Ver definición en [PLANNING.md](./PLANNING.md#navegación-tipo-explorador-de-carpetas-en-el-tablero-scrum).
- **"Revisión"** (antes "Merging"): página tipo estado de PR, con **2 carpetas** (ver [PLANNING.md](./PLANNING.md#revisión)):
  - **Tareas grandes** (→ `dev`): **corregido de nuevo**, visible para **todos los roles**; Desarrollador y Tester la ven en **solo lectura** (la UI oculta los botones de acción, sin ocultar la carpeta). Para Líder/Administrador, cada fila tiene botones "Autorizar"/"Rechazar" con el doble candado ya descrito. El Administrador además ve un selector para **asignarle la revisión a un Líder específico** (`asignar_revision`, ver [BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)) — cuando una tarea tiene Líder asignado, se muestra su nombre en la fila. **Nuevo**: cada fila tiene además un menú de tres puntos, con la opción **"Asignármela"** — visible para cualquier Líder (también para el Administrador, con el mismo efecto que usar el selector de arriba pero eligiéndose a sí mismo), invoca `autoasignar_revision` (ver [BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)). No requiere el doble candado de escribir una palabra literal, a diferencia de "Autorizar"/"Rechazar" — es una acción reversible (solo cambia quién figura como responsable, no toca Unity VC ni el estado de la tarea).
  - **Tareas pequeñas** (subtareas de colaboradores externos → su tarea padre): **visible para todos los roles**. Quien esté a cargo de la tarea padre ve los botones "Autorizar"/"Rechazar" — puede ser Desarrollador, Líder o Administrador; el resto la ve en solo lectura. Al abrir la fila aparece el visor de [Pre-revisión](./PLANNING.md#pre-revisión) correspondiente al tipo (malla 3D, imagen, reproductor de audio/video, etc.), embebido ahí mismo — nadie necesita abrir Unity ni cambiar de rama en su propio workspace solo para revisar un archivo.
- **"En revisión" ↔ "Terminada" en el tablero (y en el tablero de cada Categoría)**: ningún sentido es directo, para que una tarea no se pueda intercalar de forma indefinida.
  - **En revisión → Terminada** (solo Líder/Administrador; Desarrollador/Tester siguen bloqueados): arrastrar la tarjeta abre el mismo doble candado "autorizar" que la pantalla Revisión, y pasa por `autorizarMerge`.
  - **Terminada → En revisión ("reabrir")**: doble candado con la palabra **"reabrir"** y **motivo obligatorio**, y se notifica a todo Líder/Administrador (notificación `tarea_reabierta`, con el motivo). Las subtareas se quedan "terminada", solo se reabre la rama total.
  - **Tope de reaperturas**: cada tarea se puede reabrir **hasta 3 veces en total** (no por día: un límite diario se reinicia y no evita el ida y vuelta). Llegado el tope, **solo el Administrador** puede seguir reabriéndola, así que se escala en vez de quedar bloqueada. La tarjeta muestra el contador "↩️ Reabierta n/3", en rojo al llegar al tope.
- **"Pendientes"** (ver [PLANNING.md](./PLANNING.md#pendientes)): lista **2 tipos de fila**, cada una con una etiqueta ("Nueva" / "Edición") para distinguirlas:
  - **Tareas nuevas** (creadas por Desarrollador/Tester, `pendiente_aprobacion = true`). Cada fila tiene **4 botones**, visibles solo para Líder/Administrador:
    - **"Aprobar"**: llama a `aprobar_tarea` — la tarjeta no se mueve de columna en el Tablero Scrum, solo desaparece de esta lista.
    - **"Editar y aprobar"**: abre el formulario de edición de la tarea; al guardar, en el mismo paso llama a `editar_y_aprobar_tarea` — un solo botón, no "editar" y "aprobar" por separado.
    - **"Advertir"**: abre un campo de texto obligatorio para el mensaje, llama a `advertir_tarea_pendiente` — la tarea no se mueve de esta lista ni cambia en nada, solo se le notifica al creador.
    - **"Rechazar"**: reutiliza `eliminar_tarea` (mismo doble candado de justificación + escribir "eliminar", ver [Confirmación al arrastrar tareas](#confirmación-al-arrastrar-tareas-drag--drop)) — la tarea sale de Pendientes y del Tablero Scrum, y pasa a [Papelera](#alcance).
    - **Para el creador** (Desarrollador/Tester, sobre su propia tarea mientras sigue aquí): ve un botón normal **"Editar"** → **"Guardar"** (sin candado ni aprobación de por medio, es su propia tarea todavía sin aprobar).
  - **Ediciones pendientes** (un Desarrollador editó una tarea propia ya aprobada, ver [BACKEND.md](./BACKEND.md#notificaciones), punto 4). Cada fila tiene **2 botones**, visibles solo para Líder/Administrador: **"Aprobar"** (confirma los valores nuevos) y **"Rechazar"** (revierte a los valores anteriores, se pierde el cambio). La fila muestra los valores anteriores vs. los nuevos, campo por campo, para que sea fácil compararlos antes de decidir.

  Las subtareas se muestran aquí con una referencia a su tarea padre, ya que también siguen apareciendo colgadas de ella en el árbol normal.
- **"Papelera"** (ver [BACKEND.md](./BACKEND.md#papelera)): lista todas las tareas con `eliminada = true` (llegaron ahí desde "Eliminar una tarea" o al rechazar una de Pendientes). **Visible para todos los roles**, pero Desarrollador y Tester la ven en **solo lectura** — solo Líder/Administrador tienen el botón **"Eliminar definitivamente"**, con el mismo doble candado de escribir "eliminar" que las demás acciones destructivas. No hay botón de "restaurar".
- **"Lista de tareas" como árbol de carpetas**: en vez de una fila por tarea en plano, la lista se arma por jerarquía padre/hijo (`tarea_padre_id`). Cada tarea con subtareas es una carpeta (📁/📂, con el número de hijos) que se despliega al hacer click y muestra sus subtareas un nivel más adentro, y así sucesivamente; las tareas sin hijos son hojas (📄). Hay botones **"Expandir todo"** / **"Colapsar todo"**. En cada nivel el orden es por estado — **Sin asignar** (`por hacer` sin `asignado_a`, la misma distinción visual del Tablero Scrum) → por hacer → en producción → en revisión → terminada — y a igual estado por ID ascendente. Con búsqueda o filtro activo se muestran los resultados junto con la cadena de carpetas que lleva hasta ellos (las carpetas que no coinciden por sí mismas van atenuadas) y esas carpetas se abren solas. Si el padre de una subtarea no es visible para el usuario (Desarrollador), la subtarea aparece como raíz. Las filas con la etiqueta **⏳ Pendiente** o **✏️ Edición** muestran, al pasar el mouse por encima de la etiqueta, un recuadro gris con una explicación corta (falta aprobación de Líder/Admin y no se puede enviar a revisión / se modificó tras ser aprobada y un Líder/Admin debe confirmar o revertir), para que el usuario sepa qué significa sin ir a la documentación.
- **"Categorías"**: pantalla nueva, distinta de la navegación tipo explorador de carpetas del Tablero Scrum (esa es por jerarquía padre/hijo de tareas; esta es por **tipo/disciplina**). Son **carpetas puramente visuales, nunca ramas** — Scripting, Modelado 3D, Modelado 2D, Animación, Imagen, UI/Interfaz, Audio, Música, Video, Testing. Abrir una carpeta filtra la Lista de tareas por ese tipo, es la misma tabla/tarjetas de siempre, solo con un filtro aplicado.
- **Tarjetas del Tablero Scrum: conteo por estado en vez de commits**: las tarjetas **no listan los commits** de su rama (ni hash ni mensaje), recargaban la tarjeta sin orientar al usuario. En su lugar, las tarjetas con subtareas muestran bajo el título un chip de color por estado con la cantidad de subtareas **directas** en ese estado (las mismas que aparecen al abrir la carpeta; las nietas cuentan en la tarjeta de su propio padre) — Sin asignar, Por hacer, En producción, En revisión, Terminada, mismos colores que las columnas; solo aparecen los estados con al menos una. Es el mismo patrón que los chips por categoría (tipo) que ya llevaba la tarjeta, que se mantienen: esos cuentan todo el subárbol, solo aparecen las categorías con al menos una subtarea, y **siempre muestran el número** (también cuando es 1).
- **"Mover cambios"** (ver [BACKEND.md](./BACKEND.md#mover-cambios-entre-ramas)): opción del menú de tres puntos en la fila de cualquier tarea/rama (Tablero Scrum, Revisión, Papelera). Abre un modal para elegir la **rama destino** y el **alcance**: un commit específico (de una lista de changesets), todos los cambios, o solo lo que está sin commitear. **Si la rama destino pertenece a otra persona** (acción exclusiva de Líder/Administrador), el modal pide además **nombre** y **descripción** antes de habilitar el botón — ese texto arma la notificación que le llega al dueño de la rama.

## Aviso de rama incorrecta

Antes de sincronizar o crear una subtarea, el backend valida que el workspace esté en la rama correcta para esa tarea (ver [Guardia de rama incorrecta al sincronizar o crear subtarea en BACKEND.md](./BACKEND.md#guardia-de-rama-incorrecta-al-sincronizar-o-crear-subtarea)) — **no aplica a "mandar a revisión"**, que es solo un cambio de estado y no requiere estar parado en esa rama. Si no coincide, en vez de dejar que la acción falle en seco:

- Modal de aviso: "Tu workspace está en la rama `X`, pero esta tarea es de la rama `Y`"
- Botón **"Cambiar de rama y continuar"**: BranchBoard ejecuta el `cm switch` correcto y reintenta la acción original, sin que la persona tenga que hacerlo a mano
- Botón "Cancelar": cierra el aviso sin hacer nada

## Confirmación al arrastrar tareas (drag & drop)

En el Tablero Scrum, arrastrar una tarjeta no es un simple cambio visual — dispara acciones reales sobre Unity Version Control (crear rama, mandar a revisión, mergear a `dev`). Por eso, ciertos arrastres piden confirmación antes de ejecutarse:

- **Tomar tarea / auto-asignarse** (crea la rama): modal con descripción + campo de confirmación donde el usuario debe **escribir literalmente "asignar"** para habilitar el botón — sin eso no se puede continuar. Al confirmar, el backend crea la rama y hace el auto-switch (ver [BACKEND.md](./BACKEND.md#alcance)); la pantalla de éxito que sigue incluye un botón **"Abrir en Unity"** ahí mismo (ver [Comando "Abrir en Unity" en BACKEND.md](./BACKEND.md#comando-abrir-en-unity)) — ya con el workspace en la rama correcta, para no obligar a la persona a ir a buscar Unity Hub por su cuenta.
- **Arrastrar a "En revisión"** (solo Desarrollador, ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)): modal con descripción + campo de confirmación donde el usuario debe **escribir literalmente "mandar a revisar"** para habilitar el botón de envío.
  - Al confirmar, la tarea pasa a "en revisión" con la descripción guardada, y aparece en la carpeta "Tareas grandes" del apartado [Revisión](#alcance) — visible para todos, autorizan Líder/Administrador
- **Definido**: si el dueño de la tarjeta es un **Líder o Administrador trabajando su propia tarea**, no ve la opción "Arrastrar a 'En revisión'" — en su lugar tiene un **botón "Autorizar" directo sobre su propia tarjeta** (no un destino de arrastre nuevo), que dispara el mismo comando de merge a `dev` del [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev), sin pasar por la carpeta de Revisión. Reutiliza el mismo manejo de éxito/fallo ya definido ahí (si el merge falla por conflicto, la tarjeta se queda en "en producción" con la misma alerta síncrona, en vez de en "en revisión" ya que nunca pasó por ahí).
- En ambos casos: si el usuario cancela o cierra el modal sin completar los dos pasos, la tarjeta **regresa visualmente a su columna original**, nunca queda a medias
- **Mover una tarea general** (con subtareas): su estado se deriva de sus subtareas, así que arrastrarla **hacia atrás** ("Sin asignar", "Por hacer", "En producción") **aplica ese mismo estado a todo su subárbol**; si solo cambiara el padre, el estado derivado lo devolvería a su columna original. Lo mismo al rechazar su revisión (todo el subárbol vuelve a "en producción").
- **Hacia adelante, una tarea general no arrastra a sus subtareas** (cualquier rol, en Tablero Scrum, categorías y Revisión): solo puede **mandarse a revisión** si ninguna de sus subtareas (de cualquier nivel) sigue en "por hacer" o "en producción" — en la práctica pasa sola a "en revisión" cuando todas terminan su parte. Y solo puede **autorizarse el merge a `dev`** (botón "Autorizar", arrastrarla a "Terminada" o "Autorizar" en Revisión) si todas sus subtareas ya están "terminada", es decir, mergeadas antes a su rama desde "Tareas pequeñas" (orden de abajo hacia arriba de [BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)); al autorizarla termina solo ella. Si no se cumple, la acción se bloquea **antes** de abrir la confirmación, con un aviso que lista las subtareas pendientes.
- **Líder/Admin marcando "autorizado"** (mergea la rama a `dev`, sea desde Revisión o directo desde su propia tarea): pendiente confirmar si aplica el mismo doble candado (descripción + palabra literal) y cuál sería la palabra, dado lo delicado de la acción.

**Eliminar una tarea** (solo Administrador o Líder, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): mismo patrón, no es un arrastre pero sí una acción destructiva —
- Campo de texto obligatorio para la **justificación** de por qué se elimina
- Campo de confirmación donde debe **escribir literalmente "eliminar"** para habilitar el botón
- Motivo: queda registro en Unity Version Control, así que la justificación importa para el historial del equipo, no es un borrado silencioso
- **Definido**: esto no la borra del todo — la manda a [Papelera](#alcance), desde donde un segundo "eliminar" (mismo candado) la borra definitivamente

**Eliminar el proyecto completo** (solo Administrador, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)): un escalón más estricto que eliminar una tarea, porque borra todo, no un elemento —
- En vez de escribir una palabra genérica, el Administrador debe **escribir el nombre exacto del proyecto** para habilitar el botón (mismo patrón que usa GitHub al borrar un repositorio)
- Sin campo de justificación (no aplica igual que en una tarea, ya no queda nada donde guardarla) — la acción en sí es la última que se registra

**Restaurar backup de la base de datos** (solo Administrador, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos), lógica en [BACKEND.md](./BACKEND.md#copias-de-seguridad-y-restauración-de-la-bd)): pantalla que lista los backups automáticos disponibles con fecha, y al elegir uno pide el mismo doble candado que las demás acciones destructivas (confirmación + palabra literal, pendiente definir cuál). Un Líder o Desarrollador que quiera pedir esto no ve este botón — en su lugar tienen un botón **"Reportar problema con la base de datos"** que envía la notificación a los Administradores con un campo de motivo (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 9).

## Ajustes

Pantalla nueva que agrupa las acciones administrativas — antes estaban definidas sueltas sin un lugar fijo donde vivir. Se organiza en secciones, cada una visible según el rol de quien la abre:

1. **Solicitudes de ingreso** (Líder y Administrador): lista las filas de `solicitudes_ingreso` en estado "pendiente" — nombre, apellido, correo, ID de organización, fecha. Cada fila tiene:
   - **"Aceptar"**: abre un selector de rol (Administrador/Líder/Desarrollador/Tester — con las mismas restricciones ya definidas, ej. un Líder no puede asignar el rol Líder) y confirma, insertando la fila en `usuarios`; junto con la confirmación, ofrece el botón **"Avisar por correo"** (ver [Solicitud de ingreso vía mailto en BACKEND.md](./BACKEND.md#solicitud-de-ingreso-vía-mailto-resuelto))
   - **"Rechazar"**: marca la solicitud como rechazada, sin crear usuario
   - Botón **"Registrar solicitud manual"** (`crear_solicitud_manual`): para cuando alguien recibió la solicitud por el `mailto:` fuera de la app y quiere darle curso — mismos campos que la pantalla de solicitud (nombre, apellido, correo, ID de organización), aparece en esta misma lista como si el solicitante la hubiera enviado
   - Campo **"Correo de contacto para solicitudes"** (solo Administrador): el email al que apunta el `mailto:` que ve un solicitante externo en la [Pantalla inicial](#autenticación--acceso-al-proyecto)
   - Las solicitudes ya resueltas (aceptada/rechazada) se pueden ver como historial debajo, colapsado por defecto

2. **Gestión de usuarios** (Líder y Administrador, con matices de permiso): tabla con todos los usuarios actuales del proyecto (email, nombre, rol). Botón "Eliminar" por fila:
   - Administrador: puede eliminar a cualquiera
   - Líder: solo puede eliminar Desarrolladores (el botón no aparece sobre otro Líder ni sobre el Administrador)
   - El Administrador además tiene un botón **"Ascender a Líder"** sobre las filas de Desarrollador/Tester (exclusivo suyo, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos))
   - Fila con `firma = NULL` (alta reciente vía Líder, ver [Confirmación de usuarios nuevos en BACKEND.md](./BACKEND.md#confirmación-de-usuarios-nuevos-resuelto)): etiqueta **"Pendiente de confirmar"**. El Administrador ve el botón **"Confirmar"** (`confirmar_usuario`); el Líder que la dio de alta la ve en solo lectura con esa misma etiqueta, sin poder confirmarla él mismo.
   - **Nuevo (validado en el mockup de UI): esta tabla funciona como el "perfil" de cada usuario** — no hay una pantalla de perfil aparte. En cualquier parte de la app donde se muestra el avatar de un responsable (tarjetas del Scrum, Revisión, Testing, etc.), el avatar es un botón: al pasar el mouse muestra el nombre completo (tooltip), y al hacer clic navega directo aquí (Ajustes → Gestión de usuarios) resaltando brevemente la fila de esa persona. Solo funciona para quien tiene acceso a esta sección (Líder/Administrador) — para Desarrollador/Tester el clic navega igual a Ajustes, pero no encuentran la tabla ahí (ver el aviso de "sin acciones administrativas" más abajo).

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

**Resuelto**: si `cm whoami` falla o no hay ninguna sesión de Unity VC iniciada en la máquina, en vez de los tableros se muestra una pantalla de error — mensaje explicando que no se detectó sesión de Unity VC, botón **"Abrir Unity VC"** (lanza el cliente para que el usuario inicie sesión ahí) y botón **"Reintentar"** (vuelve a correr `cm whoami`; si ya hay sesión, entra normalmente al flujo de abajo).

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
       - **Coincide, registrado y firmado** → corre `cm whoami` para identificar al usuario, busca su correo en la tabla de usuarios de ese proyecto, y si está registrado con `firma` válida, entra mostrando todo el contenido (tableros, tareas, etc.) con el rol asignado
       - **Registrado pero sin confirmar** (`firma = NULL`, ver [Confirmación de usuarios nuevos en BACKEND.md](./BACKEND.md#confirmación-de-usuarios-nuevos-resuelto)) → pantalla mostrando "tu cuenta fue agregada por [quien la dio de alta], está pendiente de que el Administrador la confirme" — sin botón de solicitar ingreso, ya está en proceso
       - **El correo no está registrado en ese proyecto** → pantalla mostrando "tu cuenta (correo detectado) no está registrada en este proyecto todavía", con botón "Solicitar ingreso a una organización" (punto 2)
       - **No coincide con ningún `.db` de BranchBoard** (proyecto de Unity VC que nunca se ha configurado con BranchBoard) → se ofrece **inicializarlo ahí mismo**. Quien lo inicializa queda asignado automáticamente como **Administrador** de ese proyecto (**corregido**: antes era Líder — ver [Bootstrapping en BACKEND.md](./BACKEND.md#identidad-de-usuario), necesario para que exista alguien capaz de generar las llaves de firma y asignar los demás roles). Mismo criterio aplica a "Crear un nuevo proyecto" cuando se implemente (sigue siendo stub por ahora).

**Panel "Abrir en Unity"**: una vez dentro de un proyecto cargado (después de todo el flujo de la Pantalla inicial), aparece de forma general dentro de la app — no escondido en Ajustes — un botón para lanzar el Editor de Unity apuntando a la carpeta del workspace local. Mismo patrón de invocar un ejecutable externo ya usado para `cm` y `warp-cli` (ver [Comando "Abrir en Unity" en BACKEND.md](./BACKEND.md#comando-abrir-en-unity)). Solo visible para roles con workspace local (Administrador/Líder/Desarrollador/Tester) — no aplica a colaboradores de subtareas simples.

2. **Pantalla de solicitud de ingreso a una organización** (**corregido** — ver [Solicitud de ingreso vía mailto en BACKEND.md](./BACKEND.md#solicitud-de-ingreso-vía-mailto-resuelto)):
   - Campos: Nombre, Apellido, Correo (idealmente pre-rellenado con el correo detectado por `cm whoami`, ya que debe ser el mismo con el que luego se valide), ID de la organización
   - Al enviar, la app **no** guarda nada por su cuenta (quien solicita todavía no tiene acceso al proyecto) — abre un enlace `mailto:` con esos datos ya escritos en el cuerpo, dirigido al correo de contacto que el Administrador configuró en Ajustes. El solicitante manda ese correo desde su propio cliente.
   - Un Líder/Administrador que reciba el correo y decida darle curso lo registra dentro de BranchBoard (`crear_solicitud_manual`), y de ahí sigue el flujo normal de [Solicitudes de ingreso](#ajustes) (aceptar/rechazar).
   - Si la solicitud es aceptada, el Admin/Líder asocia ese correo a un rol en la tabla de usuarios — no se genera ni se envía ninguna clave/contraseña. Con un botón **"Avisar por correo"** se abre otro `mailto:` prellenado hacia el solicitante (lo manda quien aprobó, desde su propio cliente) — la próxima vez que la persona abra BranchBoard, `cm whoami` ya la identifica y entra directo.

## Roles y permisos (UI)

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 4 roles confirmados (Administrador, Líder, Desarrollador, Tester). La UI debe mostrar/ocultar acciones y pantallas según el rol del usuario logueado (ej. "eliminar proyecto" solo visible para Administrador; "aprobar solicitud de ingreso" visible para Líder y Administrador). **Corregido**: el Desarrollador ya no "recibe autorización" para modificar — es al revés, él edita directamente y el Líder/Admin recibe la notificación para confirmar o revertir (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 4); el Desarrollador solo recibe la notificación del resultado final (aprobado o rechazado).

Cuando un Desarrollador o Tester crea una tarea, la interfaz debe dejar claro que está **pendiente de aprobación** (ej. una etiqueta o color distinto en la tarjeta, sin ocultarla ni impedir tomarla) hasta que un Líder/Administrador la apruebe desde [Pendientes](#alcance) — ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos).

**Nuevo**: cuando un Desarrollador edita una tarea propia ya aprobada (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 4), la tarjeta muestra una **sombra amarilla** debajo mientras la edición sigue sin confirmar por un Líder/Administrador — distinto de la etiqueta de "pendiente de aprobación" de una tarea recién creada.

## Notificaciones

Ícono de campana (visible para todos los roles) con contador de no leídas, alimentado por los comandos Tauri `obtener_notificaciones()` / `marcar_leida(id)` (ver [BACKEND.md](./BACKEND.md#notificaciones)). Casos que debe cubrir el MVP:

1. **Líder/Administrador**: "una tarea creada por [Desarrollador/Tester] espera tu aprobación" — con acción directa de aprobar desde la misma notificación (ver `aprobar_tarea` en [BACKEND.md](./BACKEND.md#notificaciones)); también aparece listada en [Pendientes](#alcance)
2. **Desarrollador**: "tu tarea [tarea] fue rechazada: [motivo]"
3. **Desarrollador**: "se te asignó la tarea [tarea]" (cuando un Líder/Administrador la asigna directamente, no autoasignación)
4. **Líder/Administrador** (**corregido, flujo invertido** — ver punto 4 en [BACKEND.md](./BACKEND.md#notificaciones)): "[Desarrollador] editó [tarea], tiene una edición pendiente de tu revisión" — con acción directa de aprobar/rechazar desde la misma notificación (`aprobar_modificacion` / `rechazar_modificacion`); también aparece listada en [Pendientes](#alcance) con la etiqueta "Edición". El Desarrollador no recibe esta notificación de entrada — solo si `rechazar_modificacion` revierte su cambio (con el motivo incluido); si se aprueba, la edición ya estaba aplicada desde que la hizo, solo desaparece la sombra amarilla
5. **Líder/Administrador**: "la tarea [tarea] pasó a revisión"
6. **Líder**: "se te asignó revisar [tarea]" (cuando el Administrador usa `asignar_revision`, ver [BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev))
7. **Quien solicitó ingreso** (**corregido**): no llega como notificación in-app — hasta ese momento la persona no tiene acceso a BranchBoard para verla. Llega como el `mailto:` de "Avisar por correo" (ver [Solicitud de ingreso vía mailto en BACKEND.md](./BACKEND.md#solicitud-de-ingreso-vía-mailto-resuelto)), enviado por quien aprobó desde su propio cliente de correo
8. **Desarrollador**: recordatorio de inactividad — "llevas X días sin actividad en [tarea]" (generado automáticamente por el backend, sin que nadie lo envíe manualmente)
9. **Todos los Administradores**: "[Líder/Desarrollador] solicita revisar la base de datos: [motivo]" (ver `solicitar_accion_bd` en [BACKEND.md](./BACKEND.md#notificaciones), punto 9)
10. **Dueño de la rama destino**: "[nombre escrito por Líder/Admin]: [descripción escrita]" — cuando un Líder/Administrador mueve cambios hacia su rama (ver [Mover cambios en BACKEND.md](./BACKEND.md#mover-cambios-entre-ramas), punto 10). A diferencia de las demás, el contenido de esta notificación no lo arma el sistema solo — lo escribe quien mueve los cambios.
11. **Desarrollador/Tester**: "tu tarea [tarea] fue editada y aprobada" — cuando un Líder/Administrador usa "Editar y aprobar" en [Pendientes](#alcance) (ver `editar_y_aprobar_tarea` en [BACKEND.md](./BACKEND.md#notificaciones), punto 11)
12. **Desarrollador/Tester**: "[mensaje escrito por Líder/Admin]" — advertencia sobre una tarea pendiente que sigue sin cambios (ver `advertir_tarea_pendiente` en [BACKEND.md](./BACKEND.md#notificaciones), punto 12). Igual que el punto 10, el contenido lo escribe quien la envía, no el sistema.
13. **Creador de la tarea**: "tu tarea [tarea] fue eliminada: [justificación]" — aplica a cualquier eliminación, no solo a un rechazo desde Pendientes (ver `eliminar_tarea` en [BACKEND.md](./BACKEND.md#notificaciones), punto 13)

## Pendiente por definir

- Componentes de React por pantalla
- Cómo se ve una tarjeta de tarea (qué datos muestra, y cómo cambia antes/después de tener rama asociada)
- Librería de gráficos para la dona y la curva de Avance (ej. Chart.js, ApexCharts)
- Diseño de la vista de sprint (cómo se agrupan tareas dentro de un sprint, con y sin fechas)
- Estado de carga/error al "tomar" una tarea mientras el backend crea la rama (llamada async a un comando Tauri)
- Qué pasa si alguien intenta "crear un nuevo proyecto" antes de que esa pantalla esté implementada (¿deshabilitado? ¿mensaje "próximamente"?)
- Validaciones del formulario de solicitud de ingreso (formato de correo, ID de organización válido, y que el correo puesto coincida con el que da `cm whoami`)
