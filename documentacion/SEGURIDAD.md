# BranchBoard — Seguridad

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre los aspectos de seguridad de la aplicación.

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
- **Control de versiones: Unity Version Control** (no Git genérico), integrado desde el backend en **Rust directo** (sin Python) invocando el CLI `cm` como subproceso. **Actualizado (2026-11-12)**: las operaciones de servidor (crear ramas, merge, diff, leer permisos) van por la API REST del servidor de Unity VC con un PAT personal de cada usuario (ver [API REST del servidor de Unity VC](#api-rest-del-servidor-de-unity-vc-manejo-de-tokens-probado-2026-11-12)).
- **Persistencia**: SQLite local por máquina (no compartido directamente) — lo que se sincroniza a la nube es un log de eventos, ver [Sincronización entre máquinas en BACKEND.md](./BACKEND.md#sincronización-entre-máquinas-arquitectura-de-eventos).
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri).
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP**: ver lista completa y autoritativa en [Pantallas / tableros del MVP en PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp) — no se duplica aquí para evitar que este documento quede desactualizado cuando se agreguen pantallas nuevas.
- **Flujo de trabajo actual**: varias sesiones de Claude trabajando en paralelo (una en [BACKEND.md](./BACKEND.md), otra en [FRONTEND.md](./FRONTEND.md), esta en seguridad), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que otra sesión.

## Roles y permisos

5 roles: Administrador, Líder, Desarrollador, **Tester** y **Colaborador externo** (nuevo 2026-11-07).

### Reglas del servidor (Supabase, #234)

**Nuevo (2026-11-20)** — con la migración a Supabase (#231, hoja de ruta #248) los roles los aplica **el servidor**, no solo la app (`supabase/migrations/20261121000000_reglas_rls_y_roles.sql` y `…000100_ajustes_seguridad.sql`):

- **Leer**: un proyecto, sus miembros y sus eventos los ven solo sus miembros. Perfiles: el propio y los de quienes comparten algún proyecto contigo. **Sin sesión no se lee nada.**
- **Eventos**: se insertan solo en proyectos donde eres miembro; **el autor y la hora los pone el servidor** (no se puede escribir a nombre de otro ni cambiar el orden). No se editan ni se borran.
- **Roles**: viven en la tabla `miembros`, que **nadie escribe directo**; solo las funciones `crear_proyecto`, `agregar_miembro`, `cambiar_rol` y `quitar_miembro`, que revisan el rol de quien lo pide:
  - Administrador: asigna cualquier rol, **incluido Administrador** (puede haber varios, decisión 5 de #231), y quita a cualquiera.
  - Líder: solo da Desarrollador, Tester o Colaborador externo, y solo a gente que tiene uno de esos roles (o a alguien nuevo); solo quita Desarrolladores y Colaboradores externos. Igual que `roles_asignables` y `puede_eliminar` de la app.
  - **Nunca se puede quitar ni degradar al último Administrador** de un proyecto.
  - Cualquiera puede salirse de un proyecto (salvo el último Administrador).
- Probado con 42 casos contra el proyecto de pruebas (ver BACKEND.md → Reglas RLS y funciones de roles); con los de #247 (eventos según el rol, abajo) son 63.

**Archivos en Storage (#242)**: documentos e imagen del proyecto van al bucket privado `proyectos`, en `<proyecto_id>/documentos/…` y `<proyecto_id>/proyecto/…` (migración `20261121042000_storage_documentos.sql`). Reglas sobre `storage.objects`:
- **Leer**: solo miembros del proyecto. Sin sesión o de otro proyecto no se ve nada, ni siquiera que existe ("no encontrado").
- **Documentos**: los sube cualquier miembro; los borra un Líder o Administrador (un Desarrollador no borra ni los suyos).
- **Imagen**: la sube y la borra solo un Administrador.
- **Nada se sobrescribe** (no hay regla de `update`). Solo se acepta exactamente `<uuid>/documentos|proyecto/<archivo>`. Máximo 50 MB por archivo (el bucket lo rechaza y la app lo revisa antes).
- Probado con 27 casos (`supabase/tests/03_storage.test.sql`) y por la API con tres cuentas (BACKEND.md → Documentos e imagen en Storage). Con el proyecto en el servidor, quitar a alguien le quita también los archivos (antes había que revocar Drive a mano, pendiente N3).

**Login con Unity VC y primer Administrador (#237)**: las Edge Functions `login-unity` y `vincular-repo-unity` no le creen a la app. Con el token de Unity que reciben, le preguntan al servidor de Unity VC el correo de la cuenta y el `owner` del repo; solo hablan con servidores `https://*.plasticscm.com`. El token no se guarda ni se registra. Las funciones SQL que usan (`cuenta_por_correo`, `vincular_repo_unity`) solo las ejecuta `service_role`: nadie con sesión normal puede preguntar si un correo tiene cuenta ni darse Administrador de un repo. Si ya había una cuenta **sin confirmar** con ese correo, se confirma y se le cambia la contraseña por una aleatoria, para que quien la registró sin ser dueño del correo no entre con ella. Detalle en BACKEND.md → Login con Unity VC y primer Administrador.

**Sin firmas (#243, 2026-11-21)**: con los roles en el servidor ya no hay firmas Ed25519, `admin.key` ni contraseña de proyecto (decisiones 3 y 4 de #231). En un proyecto que todavía va por Drive/OneDrive, los cambios de equipo que lleguen por `ops/` se descartan siempre (no hay cómo probar quién los mandó) y el equipo no se puede cambiar hasta pasarlo al servidor (#245). Detalle en [Firma criptográfica del rol en BACKEND.md](./BACKEND.md#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac).

**Quién se lo puede saltar**: el **dueño de la cuenta de Supabase** (panel, clave secreta, contraseña de Postgres). Es el nuevo ancla de confianza en lugar de `admin.key` (decisión 2 y 3 de #231); riesgo aceptado.

**Cuentas sin confirmar el correo (#236, riesgo aceptado el 2026-11-21)**: al registrarse no se comprueba que el correo sea de quien lo escribe (el correo incluido de Supabase no lo permite; ver [Cuentas de BranchBoard en BACKEND.md](./BACKEND.md#cuentas-de-branchboard-236)). Alguien podría registrarse con el correo de otra persona y, si un Administrador agrega ese correo a un proyecto, entrar en su lugar. Mientras tanto, conviene que quien agrega a alguien confirme con esa persona que ya creó su cuenta. Lo resolverá la recuperación de contraseña con código (otro issue, necesita un SMTP propio). Por lo mismo, la protección de `login-unity` (#237) para cuentas sin confirmar no se activa: todas quedan confirmadas al registrarse. **Con las invitaciones (#241) pasa solo**: si un Líder o Administrador invita un correo que aún no tiene cuenta, quien se registre primero con ese correo entra al proyecto con el rol de la invitación, sin que nadie lo apruebe. Antes de invitar, conviene confirmar con la persona que va a registrarse ella.

**Eventos según el rol (#247, decidido 2026-11-20)**: una app modificada podría insertar en `eventos` acciones que su rol no permite. Por eso el servidor revisa el **tipo** del evento contra la tabla `privado.permisos_evento` (migración `20261121020000_permisos_evento.sql`), copiada de `identidad/permisos.rs`. La regla de inserción de `eventos` agrega: si el tipo está en la tabla, el rol de quien lo inserta tiene que estar en sus roles; si no está, lo inserta cualquier miembro.

| Quién | Tipos de evento |
|---|---|
| Solo Administrador | `cerrar_sprint`, `actualizar_configuracion_proyecto` (renombrar, ajustes e imagen viajan con este tipo) |
| Líder o Administrador | Tareas: `asignar_tarea`, `aprobar_tarea`, `aprobar_tareas`, `editar_y_aprobar_tarea`, `advertir_tarea_pendiente`, `eliminar_tarea`, `eliminar_definitivamente`. Ediciones: `aprobar_modificacion`, `aprobar_modificaciones`, `rechazar_modificacion`. Revisión: `asignar_revision`, `trasladar_cambios_directos`. Sprints: `crear_sprint`, `asignar_tarea_a_sprint`, `asignar_tarea_a_backlog`, `asignar_backlog_a_sprint`. Documentos: `aprobar_documento`, `eliminar_documento` |
| Nadie (rechazados siempre) | `crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario` (los roles van por las funciones de arriba) y `compactar_eventos` (con Supabase no hay compactación) |
| Cualquier miembro | El resto, incluidos los automáticos que genera la app de cualquiera (`tarea_cerrada_por_deteccion`, `cerrar_sprint_automatico`, `reporte_sprint_fecha_fin`, `detectar_cambios_directos`, `agregar_cambios_directos`, `crear_en_espera`, `quitar_en_espera`, `crear_notificacion(es)`) |

- **No cubre** (sigue en la app al aplicar el evento): reglas finas por persona o estado, como "solo el creador edita su tarea", "solo el revisor asignado autoriza el merge" o "el Líder reabre hasta 3 veces". Riesgo aceptado por ahora.
- **Mantenimiento**: si se agrega un comando de Líder/Administrador que publica evento, se agrega su fila a `privado.permisos_evento` (en una migración nueva) y un caso en `supabase/tests/01_reglas.test.sql`.
- La tabla está en `privado` y nadie con sesión la lee ni la escribe; la usa solo `privado.puede_publicar(proyecto, tipo)`, que da `false` (no `null`) si quien pregunta no tiene rol en el proyecto.

### Administrador
- Gestiona y administra todo tipo de usuarios, incluyendo **eliminar cualquier usuario** (Líder o Desarrollador) — ej. cuando alguien deja el equipo
- Es el único que puede asignar el rol de **Líder** a otro usuario, o **degradarlo**/reasignar entre Desarrollador, Tester y Colaborador externo (`cambiar_rol`, que reemplaza a `ascender_a_lider`) — ver [BACKEND.md](./BACKEND.md#comandos-de-usuarios-y-ajustes)
- Acceso total al proyecto
- Único rol que puede **eliminar el proyecto completo** — **definido**: un escalón más estricto que eliminar una tarea (ver [Confirmación al arrastrar tareas en FRONTEND.md](./FRONTEND.md#confirmación-al-arrastrar-tareas-drag--drop)), dado que borra absolutamente todo, no solo un elemento. En vez de escribir una palabra genérica como "eliminar", el Administrador debe **escribir el nombre exacto del proyecto** para confirmar (mismo patrón que usa GitHub al borrar un repositorio) — así se evita una confirmación "en automático" sin medir la gravedad.
- Único rol que puede **restaurar un backup de la base de datos** (ver [Copias de seguridad en BACKEND.md](./BACKEND.md#copias-de-seguridad-y-restauración-de-la-bd)) — Líder y Desarrollador solo pueden pedírselo vía notificación, no hacerlo ellos mismos.
- Si toma y trabaja una tarea general él mismo, puede **mergearla directo a `dev`** al terminar, sin pasar por "en revisión" — ya tiene permiso de autorizar, ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)

### Líder (por debajo del Administrador)
- Puede crear usuarios y cambiar roles entre Desarrollador, Tester y Colaborador externo (no crea ni asciende Líderes ni Administradores). Con el proyecto en el servidor lo hace cumplir el servidor (`privado.puede_asignar`, #234/#241); hasta #243 lo firmaba con su llave certificada por el Administrador
- Puede **eliminar usuarios** (ej. cuando alguien deja el equipo), pero **solo Desarrolladores y Colaboradores externos** — **definido**: no puede eliminar a otro Líder, esa acción es exclusiva del Administrador
- Puede crear tareas, poner permisos en tareas, eliminar tareas de otros usuarios
- Puede ver todas las tareas del proyecto
- Si toma y trabaja una tarea general él mismo, puede **mergearla directo a `dev`** al terminar, sin pasar por "en revisión" — mismo caso que el Administrador, ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev)
- **No puede**: crear/gestionar Líderes, eliminar el proyecto

### Desarrollador
- Puede ver sus propias tareas y el avance general del proyecto
- **Puede crear nuevas tareas y subtareas, pero quedan pendientes de aprobación** (`pendiente_aprobacion = true`, **corregido** — ya no existe un estado "bloqueada" aparte): la tarea sigue su curso normal desde ya (aparece en "por hacer", se puede tomar y crear su rama sin esperar nada) — lo único que no puede hacer mientras tanto es **mandarla a revisión**. La notificación de "hay una tarea por aprobar" llega **solo a Líderes y Administradores**, nunca a otros Desarrolladores, y también queda listada en el apartado [Pendientes](./PLANNING.md#pendientes).
- La aprobación solo quita esa bandera — no toca el `estado` de la tarea, que ya venía avanzando por su cuenta (por hacer o en producción, según si alguien ya la había tomado)
- **Corregido, flujo invertido**: puede **modificar** una tarea propia ya aprobada en cualquier momento, sin pedir permiso antes — el cambio se aplica de inmediato, pero queda marcado como **edición pendiente** (sombra amarilla en la tarjeta, también listada en [Pendientes](./PLANNING.md#pendientes)) hasta que un Líder/Administrador la confirme o la revierta (ver [Notificaciones en BACKEND.md](./BACKEND.md#notificaciones), punto 4). Si el Líder/Admin la rechaza, el campo vuelve a su valor anterior. **Excepción**: mientras la tarea sigue `pendiente_aprobacion = true` (recién creada, sin aprobar todavía), la edita libremente sin que quede marcada como pendiente — es su propia tarea todavía sin confirmar por nadie.
- **No puede modificar** tareas que no son suyas, y en los tableros principales (Scrum, Lista de tareas, Resumen, Avance) **solo ve las suyas** — **corregido/acotado**: esta regla no aplica a las pantallas que ya están explícitamente documentadas como visibles para todos los roles, donde sí ve tareas y actividad de otras personas en modo lectura: [Revisión](./PLANNING.md#revisión) ("Tareas grandes", solo lectura para Desarrollador), [Papelera](./PLANNING.md#alcance) (solo lectura), [Branches](./PLANNING.md#pantallas--tableros-del-mvp) (árbol completo de ramas del proyecto) y [Reportes](./PLANNING.md#reportes) (desglose de actividad de todo el equipo). En ninguna de esas cuatro puede modificar nada ajeno, solo verlo.
- No puede crear usuarios
- Al terminar de trabajar una tarea, solo puede **mandarla a revisión** — nunca puede mergearla/conectarla él mismo a `dev`, eso es exclusivo de Líder/Administrador (ver [Flujo de revisión y merge a dev en BACKEND.md](./BACKEND.md#flujo-de-revisión-y-merge-a-dev))

### Tester (4to rol)
- Puede ver **todas** las tareas del proyecto (a diferencia de Desarrollador)
- **Corregido**: sí puede **crear tareas de producción** (Scripting/Modelado/Animación/Imagen/Video/Música), con la misma restricción que el Desarrollador — quedan `pendiente_aprobacion = true` hasta que un Líder/Administrador las apruebe desde [Pendientes](./PLANNING.md#pendientes), ver arriba. Mismo caso que el Desarrollador: puede editarlas libremente mientras siguen pendientes de aprobación, sin pedir autorización
- Sí puede **crear reportes de Testing**: al revisar una tarea, documenta lo que encontró en el apartado [Testing](./PLANNING.md#testing), vinculado a la tarea revisada
- No puede modificar tareas de otros (solo reportar sobre ellas vía Testing)
- No puede crear usuarios ni gestionar el proyecto

### Colaborador externo

**Nuevo (2026-11-07)**: reemplaza la lista aparte `colaboradores_externos`. Es alguien que sube archivos puntuales (imagen, audio, video, animación, `.fbx`) en subtareas simples, con workspace parcial (Gluon); no descarga el proyecto completo ni necesita Unity Editor.
- **Ve lo mismo que un Desarrollador** (decidido 2026-11-07) y tiene sus mismas restricciones (sus tareas quedan pendientes de aprobación, no termina tareas directo).
- **Solo puede desarrollar tareas individuales**: no toma tareas grandes (las que tienen subtareas) ni subtareas (hijas de otra tarea). **Aplicado en el mockup (2026-11-07)**: no ve "Tomar tarea" en esas tarjetas (Scrum y Categorías), arrastrarlas desde "Sin asignar" se bloquea con un aviso, y en "Asignar" de la Lista de tareas no aparece como opción para una subtarea (`puedeTomarTarea` en `lib/permisos.ts`). El backend lo valida igual en `tomar_tarea` y `asignar_tarea`.
- Lo crea o le cambia el rol un Líder o el Administrador desde [Usuarios](./FRONTEND.md#usuarios); también puede pasar a otro rol si el Líder/Administrador lo decide.

### Matriz resumen

| Acción | Administrador | Líder | Desarrollador | Tester |
|---|---|---|---|---|
| Eliminar el proyecto | ✅ | ❌ | ❌ | ❌ |
| Merge de `dev` a `main` (si algún día se implementa en la app) | ✅ (exclusivo) | ❌ | ❌ | ❌ |
| Checkin directo sobre `main` o `dev` | ❌ (nadie, ni el Admin) | ❌ | ❌ | ❌ |
| Restaurar backup de la BD | ✅ | ❌ | ❌ | ❌ |
| Solicitar acción sobre la BD (notificación al Admin) | — | ✅ | ✅ | ❌ |
| Asignar rol de Líder | ✅ | ❌ | ❌ | ❌ |
| Crear usuarios | ✅ | ✅ (no Líderes ni Administradores) | ❌ | ❌ |
| Eliminar usuarios | ✅ (cualquiera) | ✅ (Desarrolladores y Colaboradores externos) | ❌ | ❌ |
| Cambiar rol (entre Desarrollador, Tester y Colaborador externo) | ✅ | ✅ | ❌ | ❌ |
| Ver la lista de usuarios (nombre y rol) | ✅ | ✅ | ✅ | ✅ |
| Crear tareas (producción) | ✅ (directo) | ✅ (directo) | ⚠️ (queda pendiente de aprobación, ver [Pendientes en PLANNING.md](./PLANNING.md#pendientes)) | ⚠️ (mismo caso que Desarrollador) |
| Aprobar tarea pendiente (creada por Desarrollador o Tester) | ✅ | ✅ | ❌ | ❌ |
| Editar y aprobar una tarea pendiente en un solo paso | ✅ | ✅ | ❌ | ❌ |
| Advertir sobre una tarea pendiente (mensaje, sin modificarla) | ✅ | ✅ | ❌ | ❌ |
| Editar tarea propia mientras sigue pendiente de aprobación | — | — | ✅ | ✅ |
| Rechazar tarea en revisión o por conflicto de merge (con motivo, vuelve al Desarrollador) | ✅ | ✅ | ❌ | ❌ |
| Asignar tarea directamente a un Desarrollador | ✅ | ✅ | ❌ | ❌ |
| Crear reporte de Testing | — | — | — | ✅ (exclusivo) |
| Poner permisos en tareas | ✅ | ✅ | ❌ | ❌ |
| Eliminar tareas de otros | ✅ | ✅ | ❌ | ❌ |
| Ver "Revisión" — carpeta "Tareas grandes" | ✅ | ✅ | 👁️ solo lectura | 👁️ solo lectura |
| Autorizar/rechazar — carpeta "Tareas grandes" (hacia `dev`) | ✅ | ✅ | ❌ | ❌ |
| Mergear su propia tarea directo a `dev`, sin pasar por "en revisión" (solo si él mismo la trabajó) | ✅ | ✅ | ❌ (siempre debe mandar a revisión) | ❌ (**corregido**: sí puede trabajar tareas grandes — ej. su propia tarea de corrección de Testing — siempre debe mandar a revisión, mismo trato que Desarrollador) |
| Ver "Revisión" — carpeta "Tareas pequeñas" | ✅ | ✅ | ✅ | ✅ |
| Autorizar/rechazar — carpeta "Tareas pequeñas" (hacia la tarea padre) | ✅ | ✅ | ✅ (solo si es dueño de la tarea padre) | ❌ |
| Asignar la revisión de una tarea a otro Líder específico | ✅ (exclusivo) | ❌ | ❌ | ❌ |
| Auto-asignarse la revisión de una tarea ("Asignármela") | ✅ | ✅ | ❌ | ❌ |
| Ver todas las tareas del proyecto (tableros principales: Scrum, Lista de tareas, Resumen, Avance) | ✅ | ✅ | ❌ (solo las suyas) | ✅ |
| Ver Revisión / Papelera / Branches / Reportes (con tareas y actividad de todo el equipo, solo lectura para quien no autoriza) | ✅ | ✅ | ✅ (solo lectura) | ✅ |
| Ver avance del proyecto | ✅ | ✅ | ✅ | ✅ |
| Modificar tareas propias ya aprobadas | ✅ | ✅ | ⚠️ se aplica de inmediato, pero queda como edición pendiente hasta que Líder/Admin la confirme | ⚠️ mismo caso que Desarrollador |
| Aprobar/rechazar una edición pendiente de otra persona | ✅ | ✅ | ❌ | ❌ |
| Modificar tareas de otros (comando `editar_tarea_ajena`, ver [BACKEND.md](./BACKEND.md#notificaciones)) | ✅ | ✅ | ❌ | ❌ |
| Ver "Papelera" | ✅ | ✅ | 👁️ solo lectura | 👁️ solo lectura |
| Eliminar definitivamente desde "Papelera" | ✅ | ✅ | ❌ | ❌ |
| Mover cambios entre ramas propias | ✅ | ✅ | ✅ | ✅ |
| "Trasladar a una rama" en una tarea roja (#225): pasa a la rama de la tarea los check-ins hechos directamente en `main`/`dev` y los revierte ahí. Es la **única excepción** a "nunca check-in a `main`/`dev`" y solo sirve para esa reversión | ✅ | ✅ | ❌ | ❌ |
| Mover cambios entre ramas de dos personas distintas | ✅ (exclusivo de Líder/Admin) | ✅ (exclusivo de Líder/Admin) | ❌ | ❌ |

### Usuarios semilla (equipo VirusTF)

Basado en los miembros del proyecto en Unity Cloud, con **correos de ejemplo** (el repositorio es público; identidad = email de `cm whoami`, ver [BACKEND.md](./BACKEND.md#identidad-de-usuario)):

| Nombre | Email | Rol en Unity Cloud | Rol en BranchBoard |
|---|---|---|---|
| Miguel Meza | miguel.meza@virustf.dev | Propietario | **Administrador** |
| jo-an | joan@virustf.dev | Usuario | **Líder** |
| Sebas | sebas@virustf.dev | Usuario | **Desarrollador** |

Nota: el rol de Unity Cloud (Propietario/Usuario) no distingue Líder de Desarrollador — esa asignación es decisión del equipo, no se deriva automáticamente de Unity Cloud.

### Rol en Unity Cloud según el rol en BranchBoard (2026-11-12)

Para que alguien pueda usar BranchBoard en un proyecto, además de crearlo en [Usuarios](./FRONTEND.md#usuarios) hay que **invitarlo en Unity Cloud** (Dashboard → la organización → miembros/invitar, con el correo de su cuenta de Unity) y darle un rol en el proyecto. Unity Cloud traduce el rol del proyecto a uno de **3 grupos automáticos de Version Control**: `<id-proyecto>-admin` (todos los permisos, incluidos `chgperm` y `rmrepository`), `<id-proyecto>-rw` (leer, crear ramas, `ci`, `mergefrom`; sin `chgperm` ni `rmrepository`) y `<id-proyecto>-ro` (solo lectura). Probado en `BranchBoard-Pruebas` con `cm showacl`.

| Rol en BranchBoard | Grupo de Version Control | Rol en la organización |
|---|---|---|
| Administrador (dueño del repo) | `-admin` | Propietario |
| Líder | `-rw` (o `-admin` si también administra el proyecto en Unity Cloud) | Usuario (o Manager) |
| Desarrollador, Tester, Colaborador externo | `-rw` | Usuario |
| — | `-ro` no se usa: nadie de BranchBoard es solo lectura (todos hacen checkin en su rama) | — |

- **Mínimo privilegio**: como la API REST se llama con el [PAT de cada persona](#api-rest-del-servidor-de-unity-vc-manejo-de-tokens-probado-2026-11-12), lo que puede hacer en el servidor lo limita su grupo de Unity, no BranchBoard. Por eso nadie fuera del Administrador va en `-admin` salvo decisión explícita: `-admin` permite borrar el repo y cambiar permisos aunque BranchBoard no lo ofrezca.
- **`-rw` permite mergear a `dev`** (tiene `ci` y `mergefrom`): la regla "solo Líder/Administrador autorizan el merge a `dev`" sigue siendo de la app, no de Unity (ver [Protección de ramas en BACKEND.md](./BACKEND.md#protección-de-ramas-main-y-dev)).
- **Los permisos de BranchBoard y los de UVCS son independientes** (decidido 2026-11-17, #204): Unity Cloud no tiene permisos por rama en el Dashboard y no se usa `cm acl` por rama, así que cualquiera en `-rw` puede hacer check-in directo en `main` o `dev` desde Plastic. BranchBoard no lo impide: lo detecta en el servidor y lo convierte en una [tarea roja](./BACKEND.md#tarea-roja-por-check-ins-directos-en-main-o-dev-issue-225) que un Líder o el Administrador traslada a su rama (#225). Para esa reversión quien traslada necesita `ci` en `dev`, que `-rw` y `-admin` tienen.
- **Rol en la organización**: no dar **Propietario** a nadie más; puede tocar la facturación y borrar la organización completa (todos sus proyectos). Para alguien que ayude a administrar, **Manager**.
- **Administrador de BranchBoard es uno solo** (proyectos por Drive/OneDrive): es el dueño del repositorio ([Identidad](./BACKEND.md#identidad-de-usuario)) y `cambiar_rol` no asigna Administrador. **Con el proyecto en el servidor (#241) puede haber varios** (decisión 5 de #231): un Administrador nombra a otro con `cambiar_rol`, y el servidor no deja quitar ni degradar al último. Alguien con todos los permisos en Unity (`-admin`) puede ser, como mucho, **Líder** en BranchBoard.
- Los nombres exactos de los roles de proyecto en el Dashboard (cuál cae en `-admin`/`-rw`/`-ro`) se confirman al invitar al primer miembro; esta tabla está en términos de los grupos, que es lo que se verificó.

## Alcance

- Permisos de Tauri: qué comandos/API del sistema (filesystem, shell/git) se exponen al frontend y con qué restricciones — **resuelto**, ver [Allowlist de capabilities de Tauri](#allowlist-de-capabilities-de-tauri-resuelto)
- Ejecución de comandos `cm` (Unity VC) desde la app: **resuelto**, ver [Seguridad al invocar cm en BACKEND.md](./BACKEND.md#seguridad-al-invocar-cm-evitar-inyección-de-comandos) — siempre pasar argumentos por separado a `Command`, nunca construir un string de shell con datos del usuario
- Alcance del acceso a disco: limitar al workspace de Unity VC configurado, no acceso libre al filesystem
- Manejo de datos del equipo: **resuelto**, no hay login propio, ni siquiera para el colaborador de subtareas simples — identidad vía `cm whoami` para **todos** los roles (ver [Identidad de usuario en BACKEND.md](./BACKEND.md#identidad-de-usuario) y [Workspace parcial (Gluon) en BACKEND.md](./BACKEND.md#workspace-parcial-gluon-para-subtareas-simples)). El diseño anterior de Service Account/API REST se descartó por una limitación real de la plataforma (las Service Accounts no pueden recibir permisos de Version Control), así que no hay ninguna llave/secreto compartido que proteger. **Actualizado (2026-11-12)**: la API REST sí se adoptó, pero con un **PAT personal por usuario** (no compartido), guardado en el llavero del sistema de su propia máquina — ver [API REST del servidor de Unity VC](#api-rest-del-servidor-de-unity-vc-manejo-de-tokens-probado-2026-11-12).
- Control de acceso por rol (autorización): cada comando Tauri sensible (eliminar proyecto, crear líder, eliminar tarea de otro, crear usuario o cambiar rol) debe validar el rol del usuario que lo invoca, no solo ocultar el botón en la UI — la UI oculta, pero el backend es quien debe negar la acción aunque alguien intente saltarse la interfaz
- **Resuelto (hallazgo C1 de auditoría; desde #243 lo resuelve el servidor)**: con el proyecto en el servidor el rol lo guardan y lo hacen cumplir las reglas del servidor ([Reglas del servidor](#reglas-del-servidor-supabase-234)) y las firmas se quitaron en #243 (ver la nota en [Firma criptográfica del rol en BACKEND.md](./BACKEND.md#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac)). Antes: el `rol` en la tabla `usuarios` no bastaba como ancla de confianza porque el `.db` se sincroniza por Drive/OneDrive con lectura/escritura para todo el equipo — cualquiera podía editarlo a mano. Ver [Firma criptográfica del rol en BACKEND.md](./BACKEND.md#firma-criptográfica-del-rol-ancla-de-confianza-para-el-rbac): cada fila de rol se firma con Ed25519 (llave privada solo en la máquina del Administrador, nunca sincronizada; llave pública embebida en la app) y se verifica al leer. Adicionalmente, **eliminar el proyecto** exige también permiso real `rmrepository` en Unity Version Control (ver [Verificación adicional sobre Unity Version Control en BACKEND.md](./BACKEND.md#verificación-adicional-sobre-unity-version-control-para-eliminar-el-proyecto)) — doble candado, no solo la firma local.

## Allowlist de capabilities de Tauri (resuelto)

Enumerado a partir de todo lo ya documentado en [BACKEND.md](./BACKEND.md) — nada de acceso "abierto" a shell o filesystem, solo lo que cada funcionalidad ya descrita necesita:

**Implementado (2026-11-09, issue #71) — cómo quedó en el código**:
- **La webview (interfaz) casi no tiene permisos**: `app/backend/capabilities/default.json` solo concede `core:app`, `core:event`, `core:window` (más `core:window:allow-close`, para que "Salir de la app" cierre la ventana pasando por el mismo bloqueo de cierre que la X) y `dialog:allow-open` (selector nativo de archivos/carpetas). **No** tiene permiso de shell, de archivos ni de red.
- **Subprocesos y archivos los hace solo el backend en Rust** (`std::process::Command` y `std::fs`), nunca la webview. Por eso no se instalan los plugins `shell` ni `fs` de Tauri: la lista de binarios y carpetas de abajo se hace cumplir en el código Rust (un único ejecutor de subprocesos y validación de rutas), no con permisos de la webview.
- **CSP estricta** en `tauri.conf.json`: la interfaz solo carga recursos propios y solo se conecta al backend (`ipc:`). Cualquier `fetch` a internet queda bloqueado (probado). Las fuentes (Inter, JetBrains Mono) van **dentro de la app**, ya no se piden a Google Fonts.
- `freezePrototype` queda **desactivado**: rompe la librería de la gráfica de Branches (React Flow).

**Subprocesos (shell)** — solo estos binarios, siempre invocados con argumentos en array separado, nunca con un string armado a mano (ver [Seguridad al invocar cm en BACKEND.md](./BACKEND.md#seguridad-al-invocar-cm-evitar-inyección-de-comandos)):
- `cm` (CLI de Unity Version Control) — todo lo del workspace (switch, status, checkin, Gluon) y gestionar el PAT. **Actualizado (2026-11-12)**: crear ramas, merge y diff van por la API REST desde el proceso Rust (no es un comando del shell ni de la webview).
- `warp-cli` (Cloudflare WARP) — solo los subcomandos fijos de conectar/desconectar/estado, nunca argumentos libres del usuario.
- El ejecutable del Editor de Unity, solo para [Abrir en Unity](./BACKEND.md#comando-abrir-en-unity) — se lanza apuntando a la carpeta del workspace, ruta descubierta/validada, no un comando arbitrario.
- Ningún otro binario ni shell genérica (`sh`, `cmd.exe`) expuesta.

**Filesystem** — acceso restringido a rutas específicas resueltas una vez al configurar el proyecto, nunca a una ruta arbitraria que llegue del frontend sin validar (ver también [Validación de rutas de archivo/adjuntos en BACKEND.md](./BACKEND.md#validación-de-rutas-de-archivoadjuntos-resuelto-previene-path-traversal)):
- La carpeta del workspace local de Unity VC (lectura para [Pre-revisión](./PLANNING.md#pre-revisión), escritura para subir el archivo de una subtarea simple).
- La subcarpeta `ops/` dentro de la carpeta del proyecto sincronizada por Drive/OneDrive (leer/crear eventos, ver [Sincronización entre máquinas en BACKEND.md](./BACKEND.md#sincronización-entre-máquinas-arquitectura-de-eventos)) — nunca editar/borrar un evento ya escrito por otra máquina.
- La carpeta de datos locales de la app (ej. `%LocalAppData%\BranchBoard\`): `.db` local y backups. (Hasta #243 también `admin.key`, que ya no existe.)
- Ninguna otra carpeta del sistema; no se solicita el permiso amplio de "acceso a disco" que trae Tauri por default.

**Dialog** (selector nativo de archivos/carpetas): para elegir dónde clonar el proyecto y para elegir el archivo al subir una subtarea simple/adjunto — el usuario elige explícitamente, la app no navega el filesystem por su cuenta más allá de eso.

**Notificaciones nativas del SO**: capability de `notification`, condicionada a que se confirme implementar el aviso de escritorio (ver pendiente en [Notificaciones de BACKEND.md](./BACKEND.md#notificaciones)).

**Actualizaciones de la app (nuevo, 2026-11-09)**: el plugin `updater` está activo, pero solo lo usa el backend en Rust (la webview no recibe ningún permiso de red). Solo consulta la dirección fija de las Releases del repositorio en GitHub y solo instala lo que venga **firmado** con la llave de actualizaciones. Ver [Actualizaciones de la app en BACKEND.md](./BACKEND.md#actualizaciones-de-la-app).

**Sin capability de red/HTTP para la webview**: no hace falta — ni la sincronización de datos (es solo lectura/escritura de archivos en una carpeta que Drive/OneDrive sincroniza por su cuenta) ni `cm` pasan por el `fetch`/HTTP del webview; toda la red real la maneja el propio proceso `cm` por fuera del control de Tauri. **Actualizado (2026-11-12)**: las llamadas a la API REST de Unity VC tampoco cambian esto — las hace el proceso Rust (ej. `reqwest`), nunca la webview. **Actualizado (2026-11-21, #235)**: el proceso Rust se conecta además al proyecto de Supabase configurado (la URL `*.supabase.co` de `app/backend/src/supabase/entorno.rs`: pruebas en desarrollo, producción en el instalador) para la sesión y los datos. Los tokens de la sesión van en el llavero del sistema, nunca en SQLite, archivos ni registros, y `Debug` los oculta. La webview sigue sin red: la CSP no cambia (`connect-src ipc: http://ipc.localhost`). Ver [Cliente de Supabase en BACKEND.md](./BACKEND.md#cliente-de-supabase-235).

Todo lo demás (clipboard, global-shortcut, etc.) queda **desactivado** hasta que un caso de uso concreto lo necesite.

## API REST del servidor de Unity VC: manejo de tokens (probado 2026-11-12)

Se probó la [API REST del servidor de Unity VC](https://docs.unity.com/en-us/services-web-apis/unity-version-control-server/overview) contra un repo de prueba (`BranchBoard-Pruebas`): crear ramas, `mergeto/report` y `mergeto` (merge del lado del servidor) funcionan **sin `cm` ni workspace**, autenticando con un **token de acceso personal (PAT)** de cada usuario — no una Service Account, así que no choca con la limitación que hizo descartar el diseño anterior (ver [Alcance](#alcance)). **Adoptada (2026-11-12)** para las operaciones de servidor — flujo completo en [Acceso a la API REST del servidor de Unity VC en BACKEND.md](./BACKEND.md#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12). Estas reglas son obligatorias:

**Hallazgo — la API filtra el token de sesión**: la respuesta de `POST .../mergeto/report` incluye, dentro de cada revisión (`base`, `revInfo`, en `src`, `dst`, `srcDiff`, `dstDiff`), un objeto `owner` con el campo **`password`**, que trae el **token de sesión de Unity (JWT `unityid`) del autor de esa revisión** (`"password": "TOKENeyJ..."`). Cualquiera que pida el reporte recibe el token de los autores de los archivos en conflicto. Puede que otros endpoints que devuelven revisiones (diff, changesets, revisiones) hagan lo mismo.

**Solución (obligatoria en el backend)**:
- **Sanear toda respuesta de la API antes de usarla**: el cliente HTTP de Rust deserializa solo a structs propios con los campos que BranchBoard necesita (ej. `path`, `changeset`, `status`) — nunca se guarda ni se reenvía el JSON crudo. Como defensa extra, un paso que recorra el JSON y **borre cualquier `owner.password`** (y cualquier clave `password`/`token`) antes de deserializar.
- **Nunca registrar respuestas crudas** de la API en logs, en `ops/`, en la SQLite ni en mensajes de error mostrados al usuario. Los errores se registran solo con código HTTP y `status`/`message`.
- **El frontend nunca recibe el JSON de la API**: los comandos Tauri devuelven solo los structs ya saneados (ej. la lista de archivos en conflicto con su ruta).

**Manejo del PAT de cada usuario**:
- Se crea una vez por usuario con `cm accesstoken create` (usa la sesión SSO de su cliente, sin contraseña) y se lee con `cm accesstoken reveal` dentro del backend, sin mostrarlo en pantalla.
- Se guarda en el **llavero del sistema operativo** (Windows Credential Manager, vía crate `keyring`), nunca en la SQLite, en `ops/`, en archivos de configuración ni en la carpeta de Drive/OneDrive.
- Viaja solo en el header `Authorization: Bearer` por HTTPS hacia el servidor de la organización; nunca en la URL.
- Al cerrar sesión, al [eliminar un usuario](./BACKEND.md#identidad-de-usuario) o si hay sospecha de filtración: `cm accesstoken revoke <id>` y borrarlo del llavero. La revocación no es inmediata (el token puede seguir sirviendo hasta ~1 h). El `id` del PAT se guarda en el mismo llavero, junto al secreto, para poder revocarlo.
- **Implementado (2026-11-14, #159)** en `app/backend/src/servidor/` (ver [BACKEND.md](./BACKEND.md#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12)). Las pruebas comprueban que el token no sale en `Debug` ni en errores y que el saneado borra `owner.password` a cualquier profundidad.
- La red la hace el **proceso Rust** (ej. `reqwest`), no el `fetch` de la webview — se mantiene la regla de [sin capability de red/HTTP para la webview](#allowlist-de-capabilities-de-tauri-resuelto).

## Pendiente por definir

- ~~Adoptar o no la API REST del servidor de Unity VC~~ — **resuelto (2026-11-12)**: se adopta para operaciones de servidor con PAT personal; `cm` se mantiene para el workspace. Ver [Acceso a la API REST en BACKEND.md](./BACKEND.md#acceso-a-la-api-rest-del-servidor-de-unity-vc-pat-decidido-2026-11-12) (incluye lo que falta verificar: PAT para miembros sin rol de administrador, duración del PAT)

- Si la autorización de modificación es por tarea puntual o un permiso general (quién la envía ya quedó definido: Líder o Administrador)
- Consideraciones de seguridad para la futura versión web (autenticación, permisos por usuario/equipo)
