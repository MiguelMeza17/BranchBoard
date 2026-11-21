-- Qué eventos acepta el servidor según el rol de quien los manda (#247; hoja de ruta #248).
--
-- Decisión (comentario del 2026-11-20 en #247): una tabla de permisos por tipo de evento, copiada de
-- `identidad/permisos.rs`. La regla de inserción de `eventos` (#234) agrega: si el `tipo` está en la
-- tabla, el rol de quien inserta tiene que estar en sus `roles`. Tipos que no están en la tabla: cualquier
-- miembro (incluidos los automáticos que genera la app de cualquiera: `tarea_cerrada_por_deteccion`,
-- `cerrar_sprint_automatico`, `reporte_sprint_fecha_fin`, `detectar_cambios_directos`, …).
--
-- No cubre (sigue en la app al aplicar): reglas finas por persona o estado, como "solo el creador edita su
-- tarea", "solo el revisor asignado autoriza el merge" o "el Líder reabre hasta 3 veces". Riesgo aceptado.
--
-- Mantenimiento: si se agrega un comando de Líder/Administrador que publica evento, se agrega su fila aquí
-- (en una migración nueva) y un caso en `supabase/tests/01_reglas.test.sql`.

create table privado.permisos_evento (
    tipo  text primary key,
    roles public.rol[] not null   -- vacío = nadie (se rechaza siempre)
);
comment on table privado.permisos_evento is
    'Tipos de evento que solo ciertos roles pueden insertar en public.eventos (#247). Un tipo que no está aquí lo puede insertar cualquier miembro.';

-- La API no expone `privado`; aun así nadie con sesión la lee ni la escribe: solo la usa la función de abajo.
alter table privado.permisos_evento enable row level security;
revoke all on privado.permisos_evento from public, anon, authenticated;

insert into privado.permisos_evento (tipo, roles) values
    -- Solo Administrador
    ('cerrar_sprint',                     '{Administrador}'),
    ('actualizar_configuracion_proyecto', '{Administrador}'),  -- renombrar, ajustes e imagen viajan con este tipo
    -- Líder o Administrador: tareas
    ('asignar_tarea',                     '{Administrador,Líder}'),
    ('aprobar_tarea',                     '{Administrador,Líder}'),
    ('aprobar_tareas',                    '{Administrador,Líder}'),
    ('editar_y_aprobar_tarea',            '{Administrador,Líder}'),
    ('advertir_tarea_pendiente',          '{Administrador,Líder}'),
    ('eliminar_tarea',                    '{Administrador,Líder}'),
    ('eliminar_definitivamente',          '{Administrador,Líder}'),
    -- Líder o Administrador: ediciones
    ('aprobar_modificacion',              '{Administrador,Líder}'),
    ('aprobar_modificaciones',            '{Administrador,Líder}'),
    ('rechazar_modificacion',             '{Administrador,Líder}'),
    -- Líder o Administrador: revisión
    ('asignar_revision',                  '{Administrador,Líder}'),
    ('trasladar_cambios_directos',        '{Administrador,Líder}'),
    -- Líder o Administrador: sprints
    ('crear_sprint',                      '{Administrador,Líder}'),
    ('asignar_tarea_a_sprint',            '{Administrador,Líder}'),
    ('asignar_tarea_a_backlog',           '{Administrador,Líder}'),
    ('asignar_backlog_a_sprint',          '{Administrador,Líder}'),
    -- Líder o Administrador: documentos
    ('aprobar_documento',                 '{Administrador,Líder}'),
    ('eliminar_documento',                '{Administrador,Líder}'),
    -- Rechazados siempre: los roles van por las funciones de #234 y con Supabase no hay compactación
    ('crear_usuario',                     '{}'),
    ('cambiar_rol',                       '{}'),
    ('confirmar_usuario',                 '{}'),
    ('eliminar_usuario',                  '{}'),
    ('compactar_eventos',                 '{}');

-- Si quien pregunta puede insertar un evento de `p_tipo` en `p_proyecto`. Ojo: si el tipo está en la tabla
-- y quien pregunta no es miembro (mi_rol = null), da false (no null), para que la regla no lo deje pasar.
create function privado.puede_publicar(p_proyecto uuid, p_tipo text)
returns boolean
language sql stable security definer
set search_path = ''
as $$
    select case
        when pe.tipo is null then true
        else coalesce(privado.mi_rol(p_proyecto) = any (pe.roles), false)
    end
    from (select 1) uno
    left join privado.permisos_evento pe on pe.tipo = p_tipo;
$$;

revoke execute on function privado.puede_publicar(uuid, text) from public, anon, authenticated;
grant execute on function privado.puede_publicar(uuid, text) to authenticated;

-- La regla de inserción de #234, con la revisión del tipo.
drop policy eventos_insertar on public.eventos;
create policy eventos_insertar on public.eventos
    for insert to authenticated
    with check (
        autor = auth.uid()
        and privado.es_miembro(proyecto_id)
        and privado.puede_publicar(proyecto_id, tipo)
    );
