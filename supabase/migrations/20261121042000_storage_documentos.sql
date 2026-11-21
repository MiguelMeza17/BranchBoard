-- Documentos e imagen del proyecto en Supabase Storage (#242; hoja de ruta #248, decisiones en #231).
--
-- Reemplaza a `<carpeta compartida>/documentos` y `<carpeta compartida>/proyecto`. Un solo bucket privado,
-- `proyectos`, con una carpeta por proyecto:
--   <proyecto_id>/documentos/D-<uuid>[.ext]   — documentos (`documentos.rs`)
--   <proyecto_id>/proyecto/icono-<uuid>.png   — imagen del proyecto (`proyecto.rs::subir_imagen`)
--
-- Reglas (las mismas que la app, `identidad/permisos.rs`):
--   * Leer: cualquier miembro del proyecto. Sin sesión o de otro proyecto: nada.
--   * Documentos: los sube cualquier miembro; los borra un Líder o Administrador.
--   * Imagen: la sube y la borra solo un Administrador.
--   * Nada se sobrescribe (no hay regla de update): cada archivo tiene un nombre nuevo.
-- Límite: 50 MB por archivo (el máximo del plan gratis); la app lo revisa antes de subir (decisión de #242).

insert into storage.buckets (id, name, public, file_size_limit)
values ('proyectos', 'proyectos', false, 52428800)
on conflict (id) do update set public = false, file_size_limit = excluded.file_size_limit;

-- El proyecto de una ruta (`<uuid>/…`), o null si la ruta no empieza por un uuid válido.
create function privado.proyecto_de_ruta(p_ruta text)
returns uuid
language sql immutable
set search_path = ''
as $$
    select case
        when split_part(p_ruta, '/', 1) ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
        then split_part(p_ruta, '/', 1)::uuid
    end;
$$;

-- La carpeta dentro del proyecto (`documentos` o `proyecto`), solo si la ruta tiene exactamente 3 partes
-- y el nombre del archivo no está vacío.
create function privado.carpeta_de_ruta(p_ruta text)
returns text
language sql immutable
set search_path = ''
as $$
    select case
        when array_length(string_to_array(p_ruta, '/'), 1) = 3 and split_part(p_ruta, '/', 3) <> ''
        then split_part(p_ruta, '/', 2)
    end;
$$;

create policy proyectos_storage_leer on storage.objects
    for select to authenticated
    using (
        bucket_id = 'proyectos'
        and privado.es_miembro(privado.proyecto_de_ruta(name))
    );

create policy proyectos_storage_subir_documento on storage.objects
    for insert to authenticated
    with check (
        bucket_id = 'proyectos'
        and privado.carpeta_de_ruta(name) = 'documentos'
        and privado.es_miembro(privado.proyecto_de_ruta(name))
    );

create policy proyectos_storage_borrar_documento on storage.objects
    for delete to authenticated
    using (
        bucket_id = 'proyectos'
        and privado.carpeta_de_ruta(name) = 'documentos'
        and privado.mi_rol(privado.proyecto_de_ruta(name)) in ('Administrador', 'Líder')
    );

create policy proyectos_storage_subir_imagen on storage.objects
    for insert to authenticated
    with check (
        bucket_id = 'proyectos'
        and privado.carpeta_de_ruta(name) = 'proyecto'
        and privado.mi_rol(privado.proyecto_de_ruta(name)) = 'Administrador'
    );

create policy proyectos_storage_borrar_imagen on storage.objects
    for delete to authenticated
    using (
        bucket_id = 'proyectos'
        and privado.carpeta_de_ruta(name) = 'proyecto'
        and privado.mi_rol(privado.proyecto_de_ruta(name)) = 'Administrador'
    );
