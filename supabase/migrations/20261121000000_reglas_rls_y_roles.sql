-- Reglas RLS y funciones de roles del servidor de BranchBoard (#234; hoja de ruta #248, decisiones en #231).
--
-- Qué puede hacer cada cuenta:
--   * Leer un proyecto, sus miembros y sus eventos: solo sus MIEMBROS.
--   * Insertar eventos: solo miembros del proyecto, a su propio nombre (autor = auth.uid()); el número de
--     orden y la hora del servidor (recibido_en) los pone siempre el servidor. Los eventos no se editan ni
--     se borran (registro de solo-agregar).
--   * Proyectos y roles: SOLO por las funciones de abajo (security definer), nunca escribiendo las tablas.
--   * La clave pública sin sesión (rol `anon`) no puede leer ni escribir nada.
--
-- Quién puede saltarse estas reglas: el dueño de la cuenta de Supabase (panel, clave secreta, contraseña de
-- Postgres). Ver SEGURIDAD.md.
--
-- Pruebas: supabase/tests/reglas_rls_y_roles.sql (corre dentro de una transacción y la deshace).

-- ---------------------------------------------------------------------------------------------
-- Permisos de tabla: nada para `anon`; lo justo para `authenticated` (las filas las filtra RLS).
-- ---------------------------------------------------------------------------------------------
revoke all on public.perfiles, public.proyectos, public.miembros, public.eventos from anon, authenticated;

-- perfiles: el correo NO se expone por la tabla (hoy solo Líder y Administrador ven correos, ver
-- `equipo::listar`); se lee con public.miembros_de_proyecto().
grant select (id, nombre, apellido, creado_en) on public.perfiles to authenticated;
grant update (nombre, apellido) on public.perfiles to authenticated;

grant select on public.proyectos to authenticated;
grant select on public.miembros to authenticated;

-- eventos: se puede leer todo; al insertar, solo estas columnas (orden y recibido_en las pone el servidor).
grant select on public.eventos to authenticated;
grant insert (id, proyecto_id, autor, tipo, tarea_id, payload, creado_en_app) on public.eventos to authenticated;

-- ---------------------------------------------------------------------------------------------
-- Funciones de apoyo (security definer: leen `miembros` sin pasar por su propia regla y sin recursión).
-- ---------------------------------------------------------------------------------------------

-- Rol de la cuenta actual en un proyecto, o null si no es miembro.
create function public.mi_rol(p_proyecto uuid)
returns public.rol
language sql
stable
security definer
set search_path = ''
as $$
    select m.rol from public.miembros m
    where m.proyecto_id = p_proyecto and m.usuario_id = auth.uid();
$$;

create function public.es_miembro(p_proyecto uuid)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
    select exists (
        select 1 from public.miembros m
        where m.proyecto_id = p_proyecto and m.usuario_id = auth.uid()
    );
$$;

-- `true` si la cuenta actual comparte al menos un proyecto con `p_usuario` (para ver su nombre).
create function public.comparte_proyecto_con(p_usuario uuid)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
    select exists (
        select 1
        from public.miembros yo
        join public.miembros otro on otro.proyecto_id = yo.proyecto_id
        where yo.usuario_id = auth.uid() and otro.usuario_id = p_usuario
    );
$$;

-- Roles que `p_actor` puede dar a alguien que hoy tiene `p_actual` (null = alguien nuevo).
-- Igual que `equipo::roles_asignables` de la app, con un cambio de la decisión 5 de #231: un Administrador
-- también puede nombrar Administradores y cambiar el rol de otro Administrador (salvo dejar el proyecto sin
-- ninguno, que lo impiden las funciones de abajo).
create function public.roles_asignables(p_actor public.rol, p_actual public.rol)
returns public.rol[]
language sql
immutable
set search_path = ''
as $$
    select case
        when p_actor = 'Administrador' then
            array['Administrador', 'Líder', 'Desarrollador', 'Tester', 'Colaborador externo']::public.rol[]
        when p_actor = 'Líder'
             and (p_actual is null or p_actual in ('Desarrollador', 'Tester', 'Colaborador externo')) then
            array['Desarrollador', 'Tester', 'Colaborador externo']::public.rol[]
        else array[]::public.rol[]
    end;
$$;

-- `true` si `p_actor` puede quitar del proyecto a alguien con rol `p_objetivo`.
-- Igual que `equipo::puede_eliminar`, salvo que un Administrador ahora sí puede quitar a otro Administrador
-- (decisión 5 de #231) mientras no sea el último.
create function public.puede_quitar(p_actor public.rol, p_objetivo public.rol)
returns boolean
language sql
immutable
set search_path = ''
as $$
    select case
        when p_actor = 'Administrador' then true
        when p_actor = 'Líder' then p_objetivo in ('Desarrollador', 'Colaborador externo')
        else false
    end;
$$;

-- ---------------------------------------------------------------------------------------------
-- Reglas RLS
-- ---------------------------------------------------------------------------------------------

-- perfiles: el propio y los de quienes comparten algún proyecto conmigo. Solo edito el mío.
create policy perfiles_ver on public.perfiles
    for select to authenticated
    using (id = auth.uid() or public.comparte_proyecto_con(id));

create policy perfiles_editar_el_propio on public.perfiles
    for update to authenticated
    using (id = auth.uid())
    with check (id = auth.uid());

-- proyectos: solo sus miembros. Crear/borrar/editar, solo por funciones.
create policy proyectos_ver on public.proyectos
    for select to authenticated
    using (public.es_miembro(id));

-- miembros: los de mis proyectos. Cambiarlos, solo por funciones.
create policy miembros_ver on public.miembros
    for select to authenticated
    using (public.es_miembro(proyecto_id));

-- eventos: leer los de mis proyectos; insertar en mis proyectos y a mi nombre. Sin update ni delete.
create policy eventos_ver on public.eventos
    for select to authenticated
    using (public.es_miembro(proyecto_id));

create policy eventos_insertar on public.eventos
    for insert to authenticated
    with check (autor = auth.uid() and public.es_miembro(proyecto_id));

-- ---------------------------------------------------------------------------------------------
-- Funciones de proyectos y roles (las únicas que escriben `proyectos` y `miembros`)
-- Errores: se lanzan con un código propio en el mensaje (`no_autenticado`, `sin_permiso`, `ultimo_admin`,
-- `no_existe_cuenta`, `no_es_miembro`, `ya_es_miembro`, `validacion`) para que la app los traduzca.
-- ---------------------------------------------------------------------------------------------

create function public.exigir_sesion()
returns uuid
language plpgsql
stable
set search_path = ''
as $$
begin
    if auth.uid() is null then
        raise exception 'no_autenticado' using errcode = '28000';
    end if;
    return auth.uid();
end;
$$;

-- Crea un proyecto SIN Unity VC; quien lo crea queda como Administrador.
-- Los proyectos de Unity VC (primer Admin = dueño del repo) los crea la Edge Function de #237.
create function public.crear_proyecto(p_nombre text)
returns uuid
language plpgsql
security definer
set search_path = ''
as $$
declare
    v_yo uuid := public.exigir_sesion();
    v_id uuid;
begin
    if p_nombre is null or length(trim(p_nombre)) = 0 then
        raise exception 'validacion: el proyecto necesita un nombre' using errcode = '22023';
    end if;
    insert into public.proyectos (nombre, creado_por) values (trim(p_nombre), v_yo) returning id into v_id;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
    values (v_id, v_yo, 'Administrador', v_yo);
    return v_id;
end;
$$;

-- Bloquea las filas de Administradores del proyecto para que dos cambios a la vez no lo dejen sin ninguno,
-- y devuelve cuántos hay.
create function public.contar_admins_bloqueando(p_proyecto uuid)
returns integer
language plpgsql
set search_path = ''
as $$
declare
    v_total integer;
begin
    perform 1 from public.miembros
    where proyecto_id = p_proyecto and rol = 'Administrador'
    for update;
    select count(*) into v_total from public.miembros
    where proyecto_id = p_proyecto and rol = 'Administrador';
    return v_total;
end;
$$;

-- Agrega a una cuenta ya registrada (por correo) con un rol.
create function public.agregar_miembro(p_proyecto uuid, p_correo text, p_rol public.rol)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
    v_yo uuid := public.exigir_sesion();
    v_mi_rol public.rol := public.mi_rol(p_proyecto);
    v_usuario uuid;
begin
    if v_mi_rol is null or not (p_rol = any (public.roles_asignables(v_mi_rol, null))) then
        raise exception 'sin_permiso' using errcode = '42501';
    end if;
    select id into v_usuario from public.perfiles where correo = lower(trim(p_correo));
    if v_usuario is null then
        raise exception 'no_existe_cuenta' using errcode = 'P0002';
    end if;
    if exists (select 1 from public.miembros where proyecto_id = p_proyecto and usuario_id = v_usuario) then
        raise exception 'ya_es_miembro' using errcode = '23505';
    end if;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
    values (p_proyecto, v_usuario, p_rol, v_yo);
end;
$$;

create function public.cambiar_rol(p_proyecto uuid, p_usuario uuid, p_rol public.rol)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
    v_mi_rol public.rol;
    v_actual public.rol;
    v_admins integer;
begin
    perform public.exigir_sesion();
    v_mi_rol := public.mi_rol(p_proyecto);
    if v_mi_rol is null then
        raise exception 'sin_permiso' using errcode = '42501';
    end if;
    -- Contar (y bloquear) los Administradores ANTES de leer el rol actual, para que dos cambios a la vez
    -- se esperen uno al otro.
    v_admins := public.contar_admins_bloqueando(p_proyecto);
    select rol into v_actual from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
    if v_actual is null then
        raise exception 'no_es_miembro' using errcode = 'P0002';
    end if;
    if not (p_rol = any (public.roles_asignables(v_mi_rol, v_actual))) then
        raise exception 'sin_permiso' using errcode = '42501';
    end if;
    if v_actual = 'Administrador' and p_rol <> 'Administrador' and v_admins <= 1 then
        raise exception 'ultimo_admin' using errcode = 'P0001';
    end if;
    update public.miembros set rol = p_rol where proyecto_id = p_proyecto and usuario_id = p_usuario;
end;
$$;

create function public.quitar_miembro(p_proyecto uuid, p_usuario uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
    v_mi_rol public.rol;
    v_objetivo public.rol;
    v_admins integer;
begin
    perform public.exigir_sesion();
    v_mi_rol := public.mi_rol(p_proyecto);
    if v_mi_rol is null then
        raise exception 'sin_permiso' using errcode = '42501';
    end if;
    v_admins := public.contar_admins_bloqueando(p_proyecto);
    select rol into v_objetivo from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
    if v_objetivo is null then
        raise exception 'no_es_miembro' using errcode = 'P0002';
    end if;
    if not public.puede_quitar(v_mi_rol, v_objetivo) then
        raise exception 'sin_permiso' using errcode = '42501';
    end if;
    if v_objetivo = 'Administrador' and v_admins <= 1 then
        raise exception 'ultimo_admin' using errcode = 'P0001';
    end if;
    delete from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
end;
$$;

-- Miembros de un proyecto con nombre y rol; el correo solo para Administrador y Líder (como `equipo::listar`).
create function public.miembros_de_proyecto(p_proyecto uuid)
returns table (usuario_id uuid, correo text, nombre text, apellido text, rol public.rol, agregado_en timestamptz)
language plpgsql
stable
security definer
set search_path = ''
as $$
declare
    v_mi_rol public.rol;
begin
    perform public.exigir_sesion();
    v_mi_rol := public.mi_rol(p_proyecto);
    if v_mi_rol is null then
        raise exception 'sin_permiso' using errcode = '42501';
    end if;
    return query
        select m.usuario_id,
               case when v_mi_rol in ('Administrador', 'Líder') then p.correo end,
               p.nombre, p.apellido, m.rol, m.agregado_en
        from public.miembros m
        join public.perfiles p on p.id = m.usuario_id
        where m.proyecto_id = p_proyecto
        order by m.agregado_en;
end;
$$;

-- ---------------------------------------------------------------------------------------------
-- Quién puede llamar a cada función
-- ---------------------------------------------------------------------------------------------
revoke execute on function
    public.mi_rol(uuid), public.es_miembro(uuid), public.comparte_proyecto_con(uuid),
    public.roles_asignables(public.rol, public.rol), public.puede_quitar(public.rol, public.rol),
    public.exigir_sesion(), public.contar_admins_bloqueando(uuid),
    public.crear_proyecto(text), public.agregar_miembro(uuid, text, public.rol),
    public.cambiar_rol(uuid, uuid, public.rol), public.quitar_miembro(uuid, uuid),
    public.miembros_de_proyecto(uuid)
from public, anon;

-- Las de apoyo las usan las reglas RLS (corren como la cuenta que consulta) y la app para saber su rol.
grant execute on function
    public.mi_rol(uuid), public.es_miembro(uuid), public.comparte_proyecto_con(uuid),
    public.roles_asignables(public.rol, public.rol), public.puede_quitar(public.rol, public.rol),
    public.crear_proyecto(text), public.agregar_miembro(uuid, text, public.rol),
    public.cambiar_rol(uuid, uuid, public.rol), public.quitar_miembro(uuid, uuid),
    public.miembros_de_proyecto(uuid)
to authenticated;
-- exigir_sesion y contar_admins_bloqueando solo las usan las funciones de arriba (que corren como su dueño).
