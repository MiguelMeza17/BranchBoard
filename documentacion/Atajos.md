# Atajos de teclado

Lista de los atajos de teclado implementados en BranchBoard.

## Globales

| Atajo | Acción | Dónde |
|---|---|---|
| `Escape` | Cierra cualquier modal abierto | Toda la app |
| `/` | Enfoca el buscador | Lista de tareas |
| `N` | Abre "Nueva tarea" | Lista de tareas |

Los atajos `/` y `N` se ignoran si ya estás escribiendo en un campo (input, textarea, select) o si hay un modal abierto.

El botón "+ Nueva tarea" ya no muestra el badge `N` al lado (se veía mal visualmente) — el atajo de teclado sigue funcionando igual, solo se quitó el hint visual del botón.

## Diálogos de confirmación (doble candado)

Usado en: Tomar tarea, Autorizar merge, Rechazar revisión/tarea, Eliminar definitivamente, Aprobar tarea/edición, Asignar a un líder.

| Atajo | Acción |
|---|---|
| `Enter` | Confirma, si el texto escrito ya coincide con la palabra pedida |
| `Ctrl/Cmd + Enter` | Confirma desde el campo de justificación (cuando aplica) |

`Enter` dentro del textarea de justificación solo hace salto de línea — no confirma (para eso es `Ctrl/Cmd + Enter`).

## Otros modales

| Modal | Atajo | Acción |
|---|---|---|
| Advertir sobre la tarea (Pendientes) | `Ctrl/Cmd + Enter` | Envía la advertencia |
| Editar y aprobar (Pendientes) | `Enter` en el título | Guarda |
| Editar y aprobar (Pendientes) | `Ctrl/Cmd + Enter` en la descripción | Guarda |

## Por qué no hay más "Enter para enviar"

En formularios con varios campos (ej. "Nueva tarea", con tipo, sprint, fecha, subtareas) se evitó a propósito el autosubmit con `Enter` — es el mismo criterio que usan apps como Linear o GitHub: un diálogo corto de confirmación (una palabra, un mensaje) sí se espera que envíe con `Enter`, pero un formulario largo de creación no, para no guardar algo a medio llenar por accidente.
