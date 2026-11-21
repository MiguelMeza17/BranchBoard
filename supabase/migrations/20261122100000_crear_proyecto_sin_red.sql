-- #269: UUID definitivo generado por la app, con reintento seguro si se perdió la respuesta.
-- Se conserva la función de un argumento para clientes anteriores.
create function public.crear_proyecto(p_nombre text, p_id uuid)
returns uuid
language plpgsql security definer
set search_path = ''
as $$
declare
    v_yo uuid := auth.uid();
    v_insertado uuid;
begin
    if v_yo is null then
        raise exception 'Inicia sesión para crear un proyecto' using errcode = '42501';
    end if;
    if p_id is null or p_nombre is null or length(trim(p_nombre)) = 0 then
        raise exception 'Falta el identificador o el nombre del proyecto';
    end if;
    insert into public.proyectos (id, nombre, creado_por)
    values (p_id, trim(p_nombre), v_yo)
    on conflict (id) do nothing returning id into v_insertado;
    if v_insertado is not null then
        insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
        values (p_id, v_yo, 'Administrador', v_yo);
    elsif not exists (
        select 1 from public.proyectos p
        join public.miembros m on m.proyecto_id = p.id
        where p.id = p_id and p.creado_por = v_yo and p.nombre = trim(p_nombre)
          and p.unity_org is null and p.unity_repo is null
          and m.usuario_id = v_yo and m.rol = 'Administrador'
    ) then
        raise exception 'El identificador del proyecto ya existe' using errcode = '23505';
    end if;
    return p_id;
end;
$$;
revoke all on function public.crear_proyecto(text, uuid) from public, anon;
grant execute on function public.crear_proyecto(text, uuid) to authenticated;
