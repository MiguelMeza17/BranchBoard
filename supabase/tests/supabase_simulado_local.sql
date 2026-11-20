-- SOLO PARA UN POSTGRES LOCAL SIN SUPABASE (no aplicar nunca en branchboard ni en branchboard-pruebas).
--
-- Crea lo mínimo de Supabase que usan las migraciones y las pruebas de reglas (#234), para poder
-- correrlas en una base vacía de Postgres 15+:
--   * roles `anon` y `authenticated` (los que usa la API de Supabase);
--   * esquema `auth` con `auth.users` (solo las columnas que leen las migraciones) y `auth.uid()`,
--     que lee el `sub` de `request.jwt.claims` igual que Supabase.
--
-- Para simular el peor caso, da por defecto TODOS los permisos de tablas nuevas a anon y authenticated
-- (como un proyecto con la exposición automática activada): las migraciones tienen que quitarlos.
--
-- Uso (ver supabase/tests/reglas_rls_y_roles.sql):
--   createdb bb_prueba
--   psql -d bb_prueba -v ON_ERROR_STOP=1 -f supabase/tests/supabase_simulado_local.sql
--   for f in supabase/migrations/*.sql; do psql -d bb_prueba -v ON_ERROR_STOP=1 -f "$f"; done
--   psql -d bb_prueba -v ON_ERROR_STOP=1 -f supabase/tests/reglas_rls_y_roles.sql

create role anon nologin;
create role authenticated nologin;

grant usage on schema public to anon, authenticated;
alter default privileges in schema public grant all on tables to anon, authenticated;
alter default privileges in schema public grant all on sequences to anon, authenticated;
alter default privileges in schema public grant all on functions to anon, authenticated;

create schema auth;
grant usage on schema auth to anon, authenticated;

create table auth.users (
    id                 uuid primary key default gen_random_uuid(),
    email              text,
    raw_user_meta_data jsonb not null default '{}'::jsonb
);

create function auth.uid()
returns uuid
language sql
stable
as $$
    select coalesce(
        nullif(current_setting('request.jwt.claim.sub', true), ''),
        (nullif(current_setting('request.jwt.claims', true), '')::jsonb ->> 'sub')
    )::uuid;
$$;
grant execute on function auth.uid() to anon, authenticated;
