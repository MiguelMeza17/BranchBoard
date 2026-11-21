-- Ajustes del revisor de seguridad de Supabase (`supabase db advisors`) sobre #234.
--
-- 1. Las funciones auxiliares (las que usan las reglas y las funciones de roles) pasan a un esquema
--    `privado` que la API no expone: antes se podían llamar como /rest/v1/rpc/<nombre>. Las reglas y los
--    triggers las siguen usando igual (se enlazan por identificador, no por nombre).
-- 2. `public.rls_auto_enable()` — la crea Supabase al activar "RLS automático en tablas nuevas" — se podía
--    llamar sin sesión. Solo la usa el event trigger, así que nadie de la API necesita ejecutarla.

create schema if not exists privado;
revoke all on schema privado from public, anon;
grant usage on schema privado to authenticated;

alter function public.es_miembro(uuid) set schema privado;
alter function public.mi_rol(uuid) set schema privado;
alter function public.comparte_proyecto(uuid) set schema privado;
alter function public.puede_asignar(public.rol, public.rol, public.rol) set schema privado;
alter function public.puede_quitar(public.rol, public.rol) set schema privado;
alter function public.exigir_otro_administrador(uuid, uuid) set schema privado;
alter function public.sellar_evento() set schema privado;
alter function public.crear_perfil_de_cuenta() set schema privado;

-- Las funciones de roles llaman a las auxiliares por nombre: se reescriben apuntando a `privado`.
-- (create or replace conserva los permisos de ejecución ya concedidos.)
create or replace function public.agregar_miembro(p_proyecto uuid, p_correo text, p_rol public.rol)
returns uuid
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := privado.mi_rol(p_proyecto);
    v_usuario uuid;
begin
    if v_actor is null or not privado.puede_asignar(v_actor, null, p_rol) then
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

create or replace function public.cambiar_rol(p_proyecto uuid, p_usuario uuid, p_rol public.rol)
returns void
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := privado.mi_rol(p_proyecto);
    v_actual public.rol;
begin
    select rol into v_actual from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
    if v_actor is null or v_actual is null or not privado.puede_asignar(v_actor, v_actual, p_rol) then
        raise exception 'No tienes permiso para cambiar ese rol' using errcode = '42501';
    end if;
    if v_actual = 'Administrador' and p_rol <> 'Administrador' then
        perform privado.exigir_otro_administrador(p_proyecto, p_usuario);
    end if;
    update public.miembros set rol = p_rol where proyecto_id = p_proyecto and usuario_id = p_usuario;
end;
$$;

create or replace function public.quitar_miembro(p_proyecto uuid, p_usuario uuid)
returns void
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := privado.mi_rol(p_proyecto);
    v_objetivo public.rol;
begin
    select rol into v_objetivo from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
    if v_actor is null or v_objetivo is null
       or not (p_usuario = auth.uid() or privado.puede_quitar(v_actor, v_objetivo)) then
        raise exception 'No tienes permiso para quitar a esa persona' using errcode = '42501';
    end if;
    if v_objetivo = 'Administrador' then
        perform privado.exigir_otro_administrador(p_proyecto, p_usuario);
    end if;
    delete from public.miembros where proyecto_id = p_proyecto and usuario_id = p_usuario;
end;
$$;

do $$
begin
    if exists (select 1 from pg_proc p join pg_namespace n on n.oid = p.pronamespace
               where n.nspname = 'public' and p.proname = 'rls_auto_enable') then
        revoke execute on function public.rls_auto_enable() from public, anon, authenticated;
    end if;
end;
$$;
