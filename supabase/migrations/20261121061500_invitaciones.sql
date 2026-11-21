-- Usuarios y roles por el servidor: invitar por correo (#241; hoja de ruta #248, decisiones en #231).
--
-- "Agregar a alguien = invitar a su correo; si aún no tiene cuenta, entra al registrarse con ese correo".
-- Hasta ahora `agregar_miembro` fallaba con "Esa persona todavía no tiene cuenta en BranchBoard". Ahora:
--   * Si la persona tiene cuenta, entra al proyecto de una vez (como antes) y devuelve su id.
--   * Si no, queda una invitación (proyecto, correo, rol) y devuelve null. Cuando una cuenta con ese correo
--     confirma su correo (registro con código, #236) o entra con Unity VC (#237, ya confirmado), las
--     invitaciones se convierten en membresías y se borran.
--   * `quitar_invitacion` la cancela: puede quien podría haberla hecho con ese rol.
-- Las invitaciones las ven solo Líderes y Administradores del proyecto (tienen el correo).

create table public.invitaciones (
    proyecto_id  uuid not null references public.proyectos (id) on delete cascade,
    correo       text not null check (correo = lower(correo) and position('@' in correo) > 1),
    rol          public.rol not null,
    invitado_por uuid references public.perfiles (id) on delete set null,
    invitado_en  timestamptz not null default now(),
    primary key (proyecto_id, correo)
);
create index invitaciones_por_correo on public.invitaciones (correo);
comment on table public.invitaciones is
    'Personas invitadas a un proyecto que todavía no tienen cuenta (#241). Al confirmar su correo pasan a miembros.';

alter table public.invitaciones enable row level security;
revoke all on public.invitaciones from anon, authenticated;
grant select on public.invitaciones to authenticated;

create policy invitaciones_leer on public.invitaciones
    for select to authenticated
    using (privado.mi_rol(proyecto_id) in ('Administrador', 'Líder'));

-- ---------------------------------------------------------------------------------------------
-- agregar_miembro: con cuenta entra; sin cuenta queda invitado (devuelve null)
-- ---------------------------------------------------------------------------------------------
create or replace function public.agregar_miembro(p_proyecto uuid, p_correo text, p_rol public.rol)
returns uuid
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := privado.mi_rol(p_proyecto);
    v_correo text := lower(trim(p_correo));
    v_usuario uuid;
begin
    if v_actor is null or not privado.puede_asignar(v_actor, null, p_rol) then
        raise exception 'No tienes permiso para agregar a alguien con ese rol' using errcode = '42501';
    end if;
    if v_correo !~ '^[^@\s]+@[^@\s]+\.[^@\s]+$' then
        raise exception 'Ese correo no es válido';
    end if;
    select id into v_usuario from public.perfiles where correo = v_correo;
    if v_usuario is null then
        if exists (select 1 from public.invitaciones where proyecto_id = p_proyecto and correo = v_correo) then
            raise exception 'Esa persona ya está invitada al proyecto';
        end if;
        insert into public.invitaciones (proyecto_id, correo, rol, invitado_por)
        values (p_proyecto, v_correo, p_rol, auth.uid());
        return null;
    end if;
    if exists (select 1 from public.miembros where proyecto_id = p_proyecto and usuario_id = v_usuario) then
        raise exception 'Esa persona ya está en el proyecto';
    end if;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
    values (p_proyecto, v_usuario, p_rol, auth.uid());
    return v_usuario;
end;
$$;

-- ---------------------------------------------------------------------------------------------
-- quitar_invitacion: la cancela quien podría haberla hecho con ese rol
-- ---------------------------------------------------------------------------------------------
create function public.quitar_invitacion(p_proyecto uuid, p_correo text)
returns void
language plpgsql security definer
set search_path = ''
as $$
declare
    v_actor public.rol := privado.mi_rol(p_proyecto);
    v_rol public.rol;
begin
    select rol into v_rol from public.invitaciones
    where proyecto_id = p_proyecto and correo = lower(trim(p_correo));
    if v_actor is null or v_rol is null or not privado.puede_asignar(v_actor, null, v_rol) then
        raise exception 'No tienes permiso para quitar esa invitación' using errcode = '42501';
    end if;
    delete from public.invitaciones where proyecto_id = p_proyecto and correo = lower(trim(p_correo));
end;
$$;

revoke execute on function public.quitar_invitacion(uuid, text) from public, anon;
grant execute on function public.quitar_invitacion(uuid, text) to authenticated;

-- ---------------------------------------------------------------------------------------------
-- Al confirmar el correo, las invitaciones pasan a membresías
-- ---------------------------------------------------------------------------------------------
create function privado.aceptar_invitaciones()
returns trigger
language plpgsql security definer
set search_path = ''
as $$
begin
    if new.email_confirmed_at is null then
        return new;
    end if;
    insert into public.miembros (proyecto_id, usuario_id, rol, agregado_por)
    select i.proyecto_id, new.id, i.rol, i.invitado_por
    from public.invitaciones i
    where i.correo = lower(new.email)
    on conflict (proyecto_id, usuario_id) do nothing;
    delete from public.invitaciones where correo = lower(new.email);
    return new;
end;
$$;

revoke execute on function privado.aceptar_invitaciones() from public, anon, authenticated;

-- El nombre empieza por "t" para correr después de `al_crear_cuenta` (crea el perfil, que `miembros`
-- necesita): Postgres dispara los triggers del mismo momento en orden alfabético.
create trigger tras_confirmar_aceptar_invitaciones
    after insert or update of email_confirmed_at on auth.users
    for each row execute function privado.aceptar_invitaciones();
