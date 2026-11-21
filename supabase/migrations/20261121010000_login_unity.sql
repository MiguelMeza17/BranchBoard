-- Login con Unity VC y primer Administrador de un repo de Unity VC (#237).
--
-- Las llaman solo las Edge Functions `login-unity` y `vincular-repo-unity` con la clave secreta (rol
-- `service_role`), DESPUÉS de preguntarle a Unity VC de quién es el token. Nadie con sesión normal ni sin
-- sesión puede ejecutarlas: si pudiera, se daría Administrador de cualquier repo o vería si un correo
-- tiene cuenta.

-- La cuenta de Auth de un correo (si existe) y si ya confirmó el correo. La Edge Function la usa para unir
-- "Iniciar sesión con Unity VC" con una cuenta de correo y contraseña ya existente (decisión 1 de #231).
create function public.cuenta_por_correo(p_correo text)
returns table (id uuid, confirmada boolean)
language sql security definer
set search_path = ''
as $$
    select u.id, u.email_confirmed_at is not null
    from auth.users u
    where lower(u.email) = lower(trim(p_correo))
    limit 1;
$$;

-- Crea el proyecto de un repo de Unity VC con `p_usuario` como primer Administrador (decisión 5 de #231).
-- La Edge Function ya comprobó con la API de Unity VC que `p_usuario` es el `owner` del repo.
-- Si el repo ya está en BranchBoard: devuelve su id cuando la persona ya es miembro; si no, error
-- (para entrar a un proyecto existente la agrega un Administrador, #241).
create function public.vincular_repo_unity(p_usuario uuid, p_org text, p_repo text, p_nombre text)
returns uuid
language plpgsql security definer
set search_path = ''
as $$
declare
    v_org  text := trim(p_org);
    v_repo text := trim(p_repo);
    v_id   uuid;
begin
    if p_usuario is null or not exists (select 1 from public.perfiles where id = p_usuario) then
        raise exception 'Esa cuenta no existe';
    end if;
    if coalesce(v_org, '') = '' or coalesce(v_repo, '') = '' then
        raise exception 'Falta la organización o el repositorio de Unity VC';
    end if;

    select id into v_id from public.proyectos where unity_org = v_org and unity_repo = v_repo;
    if v_id is not null then
        if exists (select 1 from public.miembros where proyecto_id = v_id and usuario_id = p_usuario) then
            return v_id;
        end if;
        raise exception 'Ese repositorio ya está en BranchBoard: pídele a un Administrador del proyecto que te agregue';
    end if;

    insert into public.proyectos (nombre, unity_org, unity_repo, creado_por)
    values (coalesce(nullif(trim(p_nombre), ''), v_repo), v_org, v_repo, p_usuario)
    returning id into v_id;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
    values (v_id, p_usuario, 'Administrador', p_usuario);
    return v_id;
end;
$$;

revoke execute on function
    public.cuenta_por_correo(text), public.vincular_repo_unity(uuid, text, text, text)
from public, anon, authenticated;
grant execute on function
    public.cuenta_por_correo(text), public.vincular_repo_unity(uuid, text, text, text)
to service_role;
