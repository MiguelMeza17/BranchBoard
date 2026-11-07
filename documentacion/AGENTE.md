# Agente de ayuda (a futuro)

> **Estado: idea a futuro, no implementado.** Este documento reserva el lugar y va juntando el material para cuando se construya.

## Qué es

Un **agente dentro de BranchBoard que enseña y responde dudas** sobre cómo funciona la app y el flujo de ramas: qué significa cada pantalla, cada color, cada línea, y por qué las cosas funcionan como funcionan. La idea es que alguien que no es muy técnico pueda preguntarle ("¿qué es este recuadro gris?", "¿por qué mi rama cuelga de dev?") en vez de buscar en la documentación de diseño.

## Por qué existe este documento

**Decisión (2026-10-31): la interfaz no lleva textos explicativos.** Las pantallas muestran solo lo necesario (nombres y datos), para no cargar la vista. Todo lo que *explica* —lo que antes iba en descripciones de página o en los recuadros que aparecen al pasar el mouse— se mueve aquí, como **base de conocimiento del agente**.

Regla para quien toque la interfaz: si un texto *explica* algo (en vez de mostrar un dato), no va en pantalla; va en este documento, en la sección de su pantalla.

- **Dato** (sí va en pantalla): nombre de la rama, título de la tarea, estado, fecha de fusión, a qué tarea corrige.
- **Explicación** (va aquí): qué es `dev`, por qué una rama es gris, qué pasa al hacer click en algo, cómo leer el diagrama.

## Base de conocimiento

### Branches

Textos que estaban en pantalla y se quitaron:

- **Descripción de la página**: "Árbol de ascendencia de ramas — `main` y `dev` como líneas de tiempo; las ramas abiertas cuelgan del presente de `dev`, coloreadas por estado."
- **`main`**: "Rama principal."
- **`dev`**: "Rama de integración (estado actual). Cada recuadro gris de su línea es una tarea que ya se fusionó; las ramas abiertas trabajan sobre este `dev`."
- **Punto donde nace `dev`**: "Aquí nace `dev`, desde `main`."
- **Ocultar historial**: "Click en `main` o en `dev` para ocultar su historial. Con el historial oculto: 'Historial oculto (N). Click para mostrarlo'."

Explicaciones que el agente debería poder dar (salen de lo conversado al diseñar la pantalla; el diseño completo está en [FRONTEND.md](./FRONTEND.md), entrada "Branches"):

- **¿Por qué `main` y `dev` son una línea y no un solo recuadro?** Una rama es siempre la misma rama: cada merge la hace avanzar a un estado nuevo, pero sigue siendo ella. La línea es su historia, de izquierda (pasado) a derecha (presente).
- **¿Qué son los recuadros grises?** Pasado. En la línea de `main`, estados anteriores de `main` (por ejemplo `main/dev`: el `dev` que ya se fusionó y ahora es parte de `main`). En la línea de `dev`, tareas que ya se fusionaron en `dev`.
- **¿Por qué todas las ramas abiertas cuelgan del final de `dev`?** Cada rama nace de `dev` tal como estaba en ese momento (como sacar una fotocopia). Cuando otra rama se fusiona, `dev` crece y las ramas que ya existían no reciben ese cambio solas: se quedan con su copia hasta que alguien traiga `dev` a su rama. Eso es normal y a propósito (nadie te cambia el código mientras trabajas). Pero al fusionarse, una rama se une con *todo* `dev`, sin importar de dónde nació; por eso el diagrama muestra a `dev` avanzando y a las ramas abiertas trabajando frente a su presente.
- **¿Conviene actualizar mi rama con `dev`?** Sí, de vez en cuando y sí o sí antes de mandarla a revisión: entre más crece `dev` sin traerlo, más diferencias se acumulan y más probable es tener conflictos (en Unity, escenas y prefabs son los que más duelen).
- **¿Por qué una corrección (ej. T-211) no cuelga de la tarea que corrige?** Testing revisa tareas ya terminadas y fusionadas en `dev`; la rama de la original ya se cerró. Por eso la corrección es una tarea nueva que sale de `dev`, con su propio número, y solo guarda una referencia ("Corrige T-101").
- **¿Por qué unas líneas son más gruesas?** Entre más arriba en el árbol, más cambios trae la conexión: `main`–`main` es la más gruesa, luego `main`→`dev`, luego `dev`→tarea, y cada nivel de subtarea más delgado.
- **¿Por qué algunas líneas van punteadas y moviéndose?** Son las de tareas "En producción": alguien está trabajando en ellas ahora mismo.
- **¿Por qué en los recuadros solo sale el número de la tarea?** Es corto, nunca se corta y su forma (`T-101` → `T-101-1`) ya dice quién es hija de quién. El nombre completo, el título y el estado salen al pasar el mouse.

### Usuarios

Textos que estaban en pantalla y se quitaron:

- **Crear usuario**: "Pásale a la persona su correo. También tienes que compartirle la carpeta del proyecto en Drive/OneDrive y el repositorio en Unity VC."

Explicaciones que el agente debería poder dar:

- **¿Por qué un usuario dice "Pendiente de confirmar"?** Su fila no tiene firma (por ejemplo, alguien la escribió a mano en el archivo). Lo que crean o cambian el Administrador o un Líder ya queda firmado; una fila sin firma no entra hasta que el Administrador la confirme.
- **Creé el usuario pero no puede abrir el proyecto.** Crear el usuario no le da acceso a la carpeta de Drive/OneDrive ni al repositorio de Unity VC: eso se comparte aparte.
- **¿Qué es el punto azul en Usuarios?** Hay un usuario nuevo o un cambio de rol que no has visto; se apaga al entrar.

### Avance

Textos que se quitaron de la pantalla:

- **Título "Burndown del sprint"**: ahora la gráfica lleva solo el nombre del sprint o del backlog.

Explicaciones que el agente debería poder dar:

- **¿Qué muestra la gráfica?** Cuántas tareas faltan día a día (lo que se conoce como *burndown*: la línea "se quema" hacia 0 a medida que se terminan tareas). En un backlog sube un escalón cuando entra una tarea nueva y baja cuando se cierra una.
- **¿Qué es la línea "Ideal"?** (solo sprints) El ritmo parejo que terminaría todas las tareas justo el último día del sprint. Si la línea real va por encima, vas atrasado.
- **¿Qué es la "Proyección" y el "Cierre estimado"?** Siguen el ritmo de los días que ya pasaron hasta llegar a 0. En un sprint se calcula con una recta que mejor se ajusta a lo ya hecho; en un backlog, con las tareas terminadas en las últimas 4 semanas, y supone que no entran tareas nuevas.
- **¿Qué es "General"?** El avance de todo el proyecto: suma las tareas de todos los sprints y backlogs. No es un backlog donde se guarden tareas; por eso tiene su botón aparte y es lo primero que se ve al entrar a Avance. Solo llega a 0 cuando ya no queda ninguna tarea abierta en ningún sprint ni backlog.

### Ajustes → Proyecto

- **Nombre del proyecto**: "Solo cambia el nombre en BranchBoard; el repositorio de Unity VC conserva el suyo."

### Perfil

- **Enviar mensaje**: "Le llega como notificación en la campana 🔔." No es un chat: es un aviso suelto.

