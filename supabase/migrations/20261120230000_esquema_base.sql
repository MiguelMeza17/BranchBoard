-- Esquema base del servidor de BranchBoard (#233; hoja de ruta #248, decisiones en #231).
--
-- Reemplaza a la carpeta compartida de Drive/OneDrive (`ops/`) y a las firmas Ed25519:
--   * perfiles  — uno por cuenta de Supabase Auth.
--   * proyectos — con o sin repositorio de Unity VC.
--   * miembros  — quién está en cada proyecto y con qué rol. LOS ROLES VIVEN AQUÍ, no en eventos:
--                 sin firmas, un evento `cambiar_rol` lo podría insertar cualquier miembro.
--                 Solo los cambian funciones del servidor (#234).
--   * eventos   — registro de solo-agregar de todo lo demás (tareas, sprints, chat, documentos,
--                 notificaciones), con la hora del servidor y un número de orden.
--
-- Todas las tablas nacen con RLS activado y SIN reglas: hasta #234 nadie puede leer ni escribir nada
-- por la API, ni siquiera con la clave pública.

-- Roles de un proyecto: los mismos de la app (`modelo::Rol`).
create type public.rol as enum ('Administrador', 'Líder', 'Desarrollador', 'Tester', 'Colaborador externo');

-- ---------------------------------------------------------------------------------------------
-- Perfiles
-- ---------------------------------------------------------------------------------------------
create table public.perfiles (
    id          uuid primary key references auth.users (id) on delete cascade,
    correo      text not null unique check (correo = lower(correo)),
    nombre      text not null default '',
    apellido    text not null default '',
    creado_en   timestamptz not null default now()
);
comment on table public.perfiles is 'Una fila por cuenta de Supabase Auth (correo y contraseña o Unity VC; se unen por correo, decisión 1 de #231).';

-- Al crear una cuenta en Auth se crea su perfil (nombre y apellido vienen de los metadatos del registro).
create function public.crear_perfil_de_cuenta()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
    insert into public.perfiles (id, correo, nombre, apellido)
    values (
        new.id,
        lower(new.email),
        coalesce(new.raw_user_meta_data ->> 'nombre', ''),
        coalesce(new.raw_user_meta_data ->> 'apellido', '')
    );
    return new;
end;
$$;

create trigger al_crear_cuenta
    after insert on auth.users
    for each row execute function public.crear_perfil_de_cuenta();

-- ---------------------------------------------------------------------------------------------
-- Proyectos
-- ---------------------------------------------------------------------------------------------
create table public.proyectos (
    id          uuid primary key default gen_random_uuid(),
    nombre      text not null check (length(trim(nombre)) > 0),
    -- Proyecto de Unity VC: organización y repositorio (como `RepoUnityVc` de la app). Sin Unity VC: los dos null.
    unity_org   text,
    unity_repo  text,
    creado_por  uuid references public.perfiles (id) on delete set null,
    creado_en   timestamptz not null default now(),
    constraint proyectos_unity_completo check ((unity_org is null) = (unity_repo is null)),
    -- Un repositorio de Unity VC es un solo proyecto, aunque lo traigan dos personas.
    constraint proyectos_unity_unico unique (unity_org, unity_repo)
);
comment on table public.proyectos is 'Proyectos de BranchBoard. unity_org/unity_repo vacíos = proyecto sin Unity VC.';

-- ---------------------------------------------------------------------------------------------
-- Miembros y roles
-- ---------------------------------------------------------------------------------------------
create table public.miembros (
    proyecto_id  uuid not null references public.proyectos (id) on delete cascade,
    usuario_id   uuid not null references public.perfiles (id) on delete cascade,
    rol          public.rol not null,
    agregado_por uuid references public.perfiles (id) on delete set null,
    agregado_en  timestamptz not null default now(),
    primary key (proyecto_id, usuario_id)
);
create index miembros_por_usuario on public.miembros (usuario_id);
comment on table public.miembros is 'Quién está en cada proyecto y con qué rol. Solo lo cambian funciones del servidor (#234); puede haber varios Administradores (decisión 5 de #231).';

-- ---------------------------------------------------------------------------------------------
-- Eventos
-- ---------------------------------------------------------------------------------------------
create table public.eventos (
    -- Orden creciente asignado por el servidor: es lo que usa cada PC para pedir "lo nuevo desde X" (#239).
    orden        bigint generated always as identity primary key,
    -- Id que genera la app al publicar (hoy el nombre del archivo de `ops/`): evita subir dos veces lo mismo (#238).
    id           text not null unique,
    proyecto_id  uuid not null references public.proyectos (id) on delete cascade,
    autor        uuid not null references public.perfiles (id),
    -- Nombre del comando que lo generó, como `Evento::tipo` (p. ej. `crear_tarea`).
    tipo         text not null check (length(tipo) > 0),
    tarea_id     text,
    payload      jsonb not null default '{}'::jsonb,
    -- Hora del cambio en la PC que lo hizo (informativa) y hora del servidor (la que manda para ordenar y
    -- desempatar; resuelve el riesgo aceptado del reloj local).
    creado_en_app timestamptz,
    recibido_en   timestamptz not null default now()
);
create index eventos_por_proyecto on public.eventos (proyecto_id, orden);
comment on table public.eventos is 'Registro de solo-agregar de los cambios de cada proyecto (reemplaza a ops/). Los roles NO van aquí: ver public.miembros.';

-- ---------------------------------------------------------------------------------------------
-- RLS: activado en todas, sin reglas todavía (#234). Así nadie accede por la API hasta que estén.
-- ---------------------------------------------------------------------------------------------
alter table public.perfiles  enable row level security;
alter table public.proyectos enable row level security;
alter table public.miembros  enable row level security;
alter table public.eventos   enable row level security;
