-- Subir al servidor un proyecto que ya existía (#245; hoja de ruta #248).
--
-- Decisión del 2026-11-21 en #245: se sube una "foto del estado" en un solo evento `importar_proyecto`
-- (tareas, sprints, chat, documentos… con sus autores originales dentro), no el historial de `ops/`.
-- Ese evento reemplaza los datos de un proyecto vacío, así que solo lo puede mandar un Administrador
-- (regla de #247: tipos en `permisos_evento` → solo esos roles).

insert into privado.permisos_evento (tipo, roles) values
    ('importar_proyecto', '{Administrador}');
