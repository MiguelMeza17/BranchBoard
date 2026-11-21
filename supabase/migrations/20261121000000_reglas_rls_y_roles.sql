-- Reglas RLS y funciones de roles (#234; hoja de ruta #248, decisiones en #231).
--
-- Quién puede qué:
--   * Leer un proyecto, sus miembros y sus eventos: solo sus miembros. Sin sesión (anon): nada.
--   * Perfiles: el propio y los de quienes comparten algún proyecto contigo. Solo se edita el propio
--     nombre y apellido (el correo lo maneja Auth).
--   * Eventos: se insertan solo en proyectos donde eres miembro; el autor y la hora los pone el servidor.
--     No se editan ni se borran.
--   * Proyectos y roles: nunca escribiendo las tablas; solo con las funciones de abajo, que revisan el rol
--     de quien lo pide (las mismas reglas que `roles_asignables` y `puede_eliminar` de la app, más la
--     decisión 5 de #231: varios Administradores y nunca quitar ni degradar al último).
--
-- Quien puede saltarse todo esto: el dueño de la cuenta de Supabase (panel, clave secreta). Ver SEGURIDAD.md.

-- ---------------------------------------------------------------------------------------------
-- Funciones auxiliares que usan las reglas (solo dicen cosas de quien pregunta)
-- ---------------------------------------------------------------------------------------------
create function public.es_miembro(p_proyecto uuid)
returns boolean
language sql stable security definer
set search_path = ''
as $$
    select exists (
        select 1 from public.miembros m
        where m.proyecto_id = p_proyecto and m.usuario_id = auth.uid()
    );
$$;

create function public.mi_rol(p_proyecto uuid)
returns public.rol
language sql stable security definer
set search_path = ''
as $$
    select m.rol from public.miembros m
    where m.proyecto_id = p_proyecto and m.usuario_id = auth.uid();
$$;

create function public.comparte_proyecto(p_usuario uuid)
returns boolean
language sql stable security definer
set search_path = ''
as $$
    select exists (
        select 1
        from public.miembros yo
        join public.miembros otro on otro.proyecto_id = yo.proyecto_id
        where yo.usuario_id = auth.uid() and otro.usuario_id = p_usuario
    );
$$;

-- Qué rol puede dar `actor` a alguien que hoy tiene `actual` (null = alta nueva).
create function public.puede_asignar(actor public.rol, actual public.rol, nuevo public.rol)
returns boolean
language sql immutable
set search_path = ''
as $$
    select case
        when actor = 'Administrador' then true
        when actor = 'Líder' then
            (actual is null or actual in ('Desarrollador', 'Tester', 'Colaborador externo'))
            and nuevo in ('Desarrollador', 'Tester', 'Colaborador externo')
        else false
    end;
$$;

-- Si `actor` puede quitar del proyecto a alguien con rol `objetivo`.
create function public.puede_quitar(actor public.rol, objetivo public.rol)
returns boolean
language sql immutable
set search_path = ''
as $$
    select case
        when actor = 'Administrador' then true
        when actor = 'Líder' then objetivo in ('Desarrollador', 'Colaborador externo')
        else false
    end;
$$;

-- ---------------------------------------------------------------------------------------------
-- Permisos sobre las tablas (sin "Grant API access automatically": todo se concede aquí)
-- ---------------------------------------------------------------------------------------------
revoke all on public.perfiles, public.proyectos, public.miembros, public.eventos from anon, authenticated;

grant select on public.perfiles, public.proyectos, public.miembros, public.eventos to authenticated;
grant update (nombre, apellido) on public.perfiles to authenticated;
-- En eventos, quien inserta no elige `autor`, `orden` ni `recibido_en`.
grant insert (id, proyecto_id, tipo, tarea_id, payload, creado_en_app) on public.eventos to authenticated;

-- ---------------------------------------------------------------------------------------------
-- Reglas RLS
-- ---------------------------------------------------------------------------------------------
create policy perfiles_leer on public.perfiles
    for select to authenticated
    using (id = auth.uid() or public.comparte_proyecto(id));

create policy perfiles_editar_el_propio on public.perfiles
    for update to authenticated
    using (id = auth.uid())
    with check (id = auth.uid());

create policy proyectos_leer on public.proyectos
    for select to authenticated
    using (public.es_miembro(id));

create policy miembros_leer on public.miembros
    for select to authenticated
    using (public.es_miembro(proyecto_id));

create policy eventos_leer on public.eventos
    for select to authenticated
    using (public.es_miembro(proyecto_id));

create policy eventos_insertar on public.eventos
    for insert to authenticated
    with check (autor = auth.uid() and public.es_miembro(proyecto_id));

-- El servidor pone el autor y la hora: la app no puede escribir a nombre de otro ni cambiar el orden.
create function public.sellar_evento()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
    new.autor := auth.uid();
    new.recibido_en := now();
    return new;
end;
$$;

create trigger sellar_evento
    before insert on public.eventos
    for each row execute function public.sellar_evento();

-- ---------------------------------------------------------------------------------------------
-- Funciones de proyectos y roles
-- ---------------------------------------------------------------------------------------------

-- Proyecto sin Unity VC: quien lo crea queda como Administrador. (Los de Unity VC los crea la Edge
-- Function de login tras verificar al dueño del repo, #237 / #240.)
create function public.crear_proyecto(p_nombre text)
returns uuid
language plpgsql security definer
set search_path = ''
as $$
declare
    v_yo uuid := auth.uid();
    v_id uuid;
begin
    if v_yo is null then
        raise exception 'Inicia sesión para crear un proyecto' using errcode = '42501';
    end if;
    if p_nombre is null or length(trim(p_nombre)) = 0 then
        raise exception 'Escribe el nombre del proyecto';
    end if;
    insert into public.proyectos (nombre, creado_por) values (trim(p_nombre), v_yo) returning id into v_id;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por) values (v_id, v_yo, 'Administrador', v_yo);
    return v_id;
end;
$$;

-- Falla si el cambio deja al proyecto sin Administradores. Bloquea las filas del proyecto para que dos
-- cambios simultáneos no lo dejen sin ninguno.
create function public.exigir_otro_administrador(p_proyecto uuid, p_usuario uuid)
returns void
language plpgsql security definer
set search_path = ''
as $$
declare
    v_otros int;
begin
    perform 1 from public.miembros where proyecto_id = p_proyecto for update;
    select count(*) into v_otros from public.miembros
    where proyecto_id = p_proyecto and rol = 'Administrador' and usuario_id <> p_usuario;
    if v_otros = 0 then
        raise exception 'El proyecto tiene que tener al menos un Administrador';
    end if;
end;
$$;

-- Agrega a alguien que ya tiene cuenta (las invitaciones a quien aún no se registró llegan en #241).
create function public.agregar_miembro(p_proyecto uuid, p_correo text, p_rol public.rol)
returns uuid
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := public.mi_rol(p_proyecto);
    v_usuario uuid;
begin
    if v_actor is null or not public.puede_asignar(v_actor, null, p_rol) then
        raise exception 'No tienes permiso para agregar a alguien con ese rol' using errcode = '42501';
    end if;
    select id into v_usuario from public.perfiles where correo = lower(trim(p_correo));
    if v_usuario is null then
        raise exception 'Esa persona todavía no tiene cuenta en BranchBoard';
    end if;
    if exists (select 1 from public.miembros where proyecto_id = p_proyecto and usuario_id = v_usuario) then
        raise exception 'Esa persona ya está en el proyecto';
    end if;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
    values (p_proyecto, v_usuario, p_rol, auth.uid());
    return v_usuario;
end;
$$;

create function public.cambiar_rol(p_proyecto uuid, p_usuario uuid, p_rol public.rol)
returns void
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := public.mi_rol(p_proyecto);
    v_actual public.rol;
begin
    select rol into v_actual from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
    if v_actor is null or v_actual is null or not public.puede_asignar(v_actor, v_actual, p_rol) then
        raise exception 'No tienes permiso para cambiar ese rol' using errcode = '42501';
    end if;
    if v_actual = 'Administrador' and p_rol <> 'Administrador' then
        perform public.exigir_otro_administrador(p_proyecto, p_usuario);
    end if;
    update public.miembros set rol = p_rol where proyecto_id = p_proyecto and usuario_id = p_usuario;
end;
$$;

-- Quitar a alguien del proyecto (o salirse uno mismo).
create function public.quitar_miembro(p_proyecto uuid, p_usuario uuid)
returns void
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := public.mi_rol(p_proyecto);
    v_objetivo public.rol;
begin
    select rol into v_objetivo from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
    if v_actor is null or v_objetivo is null
       or not (p_usuario = auth.uid() or public.puede_quitar(v_actor, v_objetivo)) then
        raise exception 'No tienes permiso para quitar a esa persona' using errcode = '42501';
    end if;
    if v_objetivo = 'Administrador' then
        perform public.exigir_otro_administrador(p_proyecto, p_usuario);
    end if;
    delete from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
end;
$$;

-- ---------------------------------------------------------------------------------------------
-- Quién puede llamar a cada función
-- ---------------------------------------------------------------------------------------------
revoke execute on function
    public.es_miembro(uuid), public.mi_rol(uuid), public.comparte_proyecto(uuid),
    public.puede_asignar(public.rol, public.rol, public.rol), public.puede_quitar(public.rol, public.rol),
    public.sellar_evento(), public.crear_proyecto(text), public.exigir_otro_administrador(uuid, uuid),
    public.agregar_miembro(uuid, text, public.rol), public.cambiar_rol(uuid, uuid, public.rol),
    public.quitar_miembro(uuid, uuid), public.crear_perfil_de_cuenta()
from public, anon, authenticated;

-- Las auxiliares las evalúan las reglas con el rol de quien consulta.
grant execute on function
    public.es_miembro(uuid), public.mi_rol(uuid), public.comparte_proyecto(uuid),
    public.puede_asignar(public.rol, public.rol, public.rol), public.puede_quitar(public.rol, public.rol)
to authenticated;

-- Lo que la app llama.
grant execute on function
    public.crear_proyecto(text), public.agregar_miembro(uuid, text, public.rol),
    public.cambiar_rol(uuid, uuid, public.rol), public.quitar_miembro(uuid, uuid)
to authenticated;
