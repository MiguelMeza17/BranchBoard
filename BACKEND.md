# BranchBoard — Backend

> Ver visión general y conceptos compartidos en [PLANNING.md](./PLANNING.md).
> Este documento cubre lo que corre del lado de Tauri (Rust) / lógica de datos.

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
- **Control de versiones: Unity Version Control** (antes Plastic SCM), no Git genérico. Se usa su CLI (`cm`). La integración desde el backend se hace en **Rust directo** (sin Python), invocando `cm` como subproceso.
- **Flujo de vida de una tarea**: se crea visual/sin rama → al "tomarla" alguien, el backend (Rust) crea la rama en Unity VC invocando `cm` → el sprint es el tiempo que esa rama vive sobre `dev` antes de mergear a `main`.
- **Stack elegido**: Tauri (Rust) + React/TypeScript. Fase 1 = app de escritorio (full desktop); backend en Rust, SQLite, sin Python. Fase 2 (futuro) = versión web, reutilizando solo el frontend (React) — necesita un backend servidor nuevo y aparte (no Tauri) que exponga por HTTP lo que hoy hacen los comandos de Tauri.
- **Usuarios**: un equipo pequeño trabajando sobre un proyecto Unity compartido.
- **Pantallas/tableros del MVP** (definidos por el usuario, hay que respetar estos nombres):
  1. **Tablero Scrum** — vista tipo kanban de las tareas.
  2. **Resumen** — pantalla con una gráfica circular (tipo dona) mostrando el conteo de tareas en 4 estados: por hacer, en producción, en revisión, terminadas.
  3. **Lista de tareas** — vista con todas las tareas existentes; es el lugar donde se crean tareas nuevas.
  4. **Tablero "Avance"** — otro tablero distinto al Scrum (su diferencia exacta con el Scrum aún no está definida — pendiente de aclarar con el usuario).
- **Flujo de trabajo actual**: dos sesiones de Claude trabajando en paralelo, una en este documento (backend) y otra en [FRONTEND.md](./FRONTEND.md), para luego unir todo en un plan consolidado. Evitar editar [PLANNING.md](./PLANNING.md) al mismo tiempo que la otra sesión.

## Alcance

- Modelo de datos de una tarea (scripting, 3D, animación, imagen, música, video)
- Persistencia: **SQLite** (un solo archivo `.db`, sin servidor). Fase 1 = archivo local; el archivo `.db` se sincroniza a la nube (ej. Drive/OneDrive) para compartirlo entre el equipo. Fase futura (versión web) = migrar a una base de datos con servidor (ej. Postgres) para manejar escrituras concurrentes de varios usuarios.
- Integración con Unity Version Control desde Rust invocando el CLI `cm` como subproceso: crear rama al tomar una tarea, detectar cambios nuevos (ver sección de detección de cambios)
- Comandos Tauri expuestos al frontend (ej. `tomar_tarea`, `revisar_actualizaciones`)

## Detección de cambios en Unity Version Control (polling)

Problema a resolver: un compañero sube un asset (ej. un modelo 3D) desde Unity directamente a una rama, y esa tarea debe reflejarse como "modificada" en BranchBoard.

**Conceptos**: cada checkin en Unity VC genera un **changeset** (número secuencial) en la rama donde se hizo. El CLI `cm` permite consultar el historial de una rama.

**Flujo propuesto (opción simple, sin triggers de servidor)**:

1. Al crear la rama de una tarea (cuando alguien la "toma"), el backend guarda `ultimo_changeset_visto` para esa tarea (el changeset que existía en ese momento).
2. El usuario dispara una revisión manual desde el frontend (botón "Refrescar", comando Tauri `revisar_actualizaciones(tarea_id)`) — no hace falta un proceso en segundo plano para el MVP.
3. El backend en Rust corre algo como `cm log br:/<rama> --xml` (como subproceso) y obtiene el changeset más reciente de esa rama.
4. Si el changeset más reciente es mayor al `ultimo_changeset_visto` guardado → hay cambios nuevos:
   - Marca la tarea como "modificada" (para que el frontend le ponga un badge/indicador)
   - Actualiza `ultimo_changeset_visto`
   - Opcional: extrae metadata del changeset (autor, comentario, archivos afectados) para mostrarla en el detalle de la tarea
5. **El mismo refresco también revisa si la rama se cerró** (mergeada hacia su rama padre). Unity VC guarda el estado de cierre de una rama al mergearla. Si la rama aparece cerrada → la tarea se marca automáticamente como **"terminada"**, sin intervención manual. (Comando `cm` exacto para leer el estado de cierre: pendiente de verificar al implementar.)
6. (Fase futura) Reemplazar el polling manual por **triggers del servidor de Unity VC** (ej. `after-checkin`) para detección casi en tiempo real, sin depender de que alguien le dé clic a "Refrescar".

## Comando "Guardar cambios y sincronizar"

Mismo patrón que "tomar tarea", reusando la arquitectura de comandos Tauri ya establecida:

1. El usuario le da clic al botón en la interfaz (React)
2. Se llama un comando Tauri, ej. `sincronizar_tarea(tarea_id)`
3. El backend en Rust corre el `cm` correspondiente (checkin/update) sobre la rama de esa tarea
4. El frontend muestra estado de carga mientras corre, y éxito/error al terminar
5. Al terminar con éxito, dispara de una vez la revisión del punto anterior (changesets nuevos + estado de cierre) para que la tarea quede actualizada sin pasos extra

No es una pieza nueva de arquitectura — es el mismo cable React → comando Tauri → Rust → `cm`, ya usado en "tomar tarea" y "revisar_actualizaciones".

## Datos para la gráfica de Avance

Ver definición de la gráfica en [PLANNING.md](./PLANNING.md#pantallas--tableros-del-mvp). Dos fuentes de datos distintas:

1. **Tareas resueltas por día** (línea principal): cada tarea guarda un campo `fecha_completada`, escrito cuando pasa a "terminada" (ya sea manual o automático al detectar la rama cerrada, ver "Detección de cambios"). Para armar la curva, se agrupan las tareas por ese campo y se cuenta cuántas hay por día — es una consulta normal a SQLite, no requiere `cm`.
2. **Changesets hechos por día** (línea secundaria, todo el equipo): a diferencia del polling por tarea (que revisa una rama a la vez), esto necesita el historial de **todo el repositorio**, filtrado por fecha — comando `cm` exacto pendiente de verificar al implementar.

## Identidad de usuario

BranchBoard **no tiene sistema propio de login/usuarios**. La identidad se obtiene directamente de la sesión de Unity Version Control ya activa en la máquina:

- Al arrancar, el backend corre `cm whoami` (subproceso) y obtiene el email de la cuenta logueada (confirmado en pruebas reales: devuelve algo como `nombre@dominio.com`).
- Ese email es el identificador del usuario dentro de BranchBoard — no hay contraseña, sesión propia, ni token que gestionar.
- La tabla de usuarios en SQLite mapea **email (de `cm whoami`) → rol** (ver Roles y permisos). El admin asigna el rol a cada email conocido del equipo la primera vez; después de eso, cada persona que abre la app en su máquina queda identificada automáticamente.
- Requisito implícito: cada miembro del equipo ya debe tener el cliente de Unity Version Control instalado y con sesión iniciada (login por SSO/navegador, una sola vez) en su propia máquina — eso ya lo resuelve `cm`, BranchBoard no reimplementa nada de eso.
- **Confirmado**: aunque todo el equipo comparte el mismo workspace de Unity Version Control, cada persona inicia sesión con su propia cuenta/correo — `cm whoami` sí distingue a cada usuario individualmente, no hay una cuenta compartida.
- **Eliminar a alguien del equipo** (rol: Administrador o Líder, ver [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos)) es tan simple como borrar su fila en la tabla de usuarios: como la identidad se valida contra esa tabla cada vez que se abre la app (no hay sesión persistente que revocar), la persona pierde el acceso de inmediato la próxima vez que intente entrar — su correo ya no aparecería registrado.

## Roles y permisos

Ver matriz completa en [SEGURIDAD.md](./SEGURIDAD.md#roles-y-permisos). Resumen: 4 roles confirmados (Administrador, Líder, Desarrollador, Tester). El modelo de datos de usuario necesita un campo de rol asociado al email de `cm whoami` (ver Identidad de usuario), y cada comando Tauri sensible debe validar permisos en el backend (no solo ocultar en la UI).

## Pendiente por definir

- Estructura exacta de la tarea (campos)
- Estructura exacta de usuario (email de `cm whoami` + rol, y cómo se valida el permiso en cada comando Tauri)
- Cómo se puebla la primera vez la tabla email→rol (¿el admin los escribe a mano? ¿se listan desde los miembros del proyecto en Unity Cloud?)
- Cómo se detecta/vincula el workspace de Unity VC del proyecto Unity (ruta local, nombre del repo)
- Estados del flujo de una tarea
- Comandos `cm` exactos a usar para leer changesets por rama y extraer metadata (autor, comentario, archivos)
- Comando `cm` exacto para leer si una rama está cerrada/mergeada
- Dónde vive `ultimo_changeset_visto` por tarea (misma base de datos del modelo de tarea)
- Diseño final del lock de escritura en SQLite: corto y por operación (no por sesión completa) + detección de sobrescritura como red de seguridad, dado el retraso de sincronización de la carpeta en la nube
