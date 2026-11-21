-- Pruebas de las funciones del login con Unity VC (#237).
--
-- Corren contra el proyecto de PRUEBAS dentro de una transacción que se deshace al final. Cómo correrlas:
--   npx supabase@3 db query --remote --yes --file supabase/tests/02_login_unity.test.sql
-- La última consulta devuelve las líneas TAP y `fallidas` (tiene que ser 0).
--
-- Cuentas: D (dueña del repo, confirmada), S (sin confirmar), O (otra persona).

begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;

create temp table _o (clave text primary key, valor text);
grant all on _o to authenticated, anon, service_role;

create function pg_temp.intentar(sql text) returns text language plpgsql as $$
begin
    execute sql;
    return 'ok';
exception when others then
    return sqlstate;
end;
$$;
grant execute on function pg_temp.intentar(text) to authenticated, anon, service_role;
create function pg_temp.o(c text) returns text language sql as $$ select valor from _o where clave = c $$;

insert into auth.users (id, email, email_confirmed_at, aud, role) values
    ('e0000000-0000-4000-8000-000000000001', 'duena@prueba.branchboard', now(), 'authenticated', 'authenticated'),
    ('e0000000-0000-4000-8000-000000000002', 'sin.confirmar@prueba.branchboard', null, 'authenticated', 'authenticated'),
    ('e0000000-0000-4000-8000-000000000003', 'otra@prueba.branchboard', now(), 'authenticated', 'authenticated');

select no_plan();
create temp table _r (n serial, linea text);

-- ---------------------------------------------------------------- Nadie con sesión ni sin ella las usa
set local role authenticated;
select set_config('request.jwt.claims', '{"sub":"e0000000-0000-4000-8000-000000000003","role":"authenticated"}', true);
insert into _o values ('auth_cuenta', pg_temp.intentar($q$select public.cuenta_por_correo('duena@prueba.branchboard')$q$));
insert into _o values ('auth_vincula', pg_temp.intentar(
    $q$select public.vincular_repo_unity('e0000000-0000-4000-8000-000000000003', 'Org', 'Repo/Repo', 'X')$q$));
reset role;

set local role anon;
insert into _o values ('anon_cuenta', pg_temp.intentar($q$select public.cuenta_por_correo('duena@prueba.branchboard')$q$));
insert into _o values ('anon_vincula', pg_temp.intentar(
    $q$select public.vincular_repo_unity('e0000000-0000-4000-8000-000000000003', 'Org', 'Repo/Repo', 'X')$q$));
reset role;

-- ---------------------------------------------------------------- La Edge Function (service_role)
set local role service_role;
insert into _o select 'cuenta_duena', id::text || '|' || confirmada::text
    from public.cuenta_por_correo('  Duena@Prueba.BranchBoard ');
insert into _o select 'cuenta_sin_confirmar', confirmada::text
    from public.cuenta_por_correo('sin.confirmar@prueba.branchboard');
insert into _o values ('cuenta_inexistente',
    (select count(*)::text from public.cuenta_por_correo('nadie@prueba.branchboard')));

insert into _o values ('p', public.vincular_repo_unity(
    'e0000000-0000-4000-8000-000000000001', ' OrgPrueba ', 'Repo/Repo', '')::text);
insert into _o values ('p_otra_vez', public.vincular_repo_unity(
    'e0000000-0000-4000-8000-000000000001', 'OrgPrueba', 'Repo/Repo', 'Otro nombre')::text);
insert into _o values ('otra_vincula_el_mismo', pg_temp.intentar(
    $q$select public.vincular_repo_unity('e0000000-0000-4000-8000-000000000003', 'OrgPrueba', 'Repo/Repo', 'X')$q$));
insert into _o values ('sin_repo', pg_temp.intentar(
    $q$select public.vincular_repo_unity('e0000000-0000-4000-8000-000000000003', 'OrgPrueba', ' ', 'X')$q$));
insert into _o values ('cuenta_falsa', pg_temp.intentar(
    $q$select public.vincular_repo_unity('f0000000-0000-4000-8000-000000000009', 'OrgPrueba', 'Otro/Otro', 'X')$q$));
reset role;

-- ---------------------------------------------------------------- Comprobaciones (como postgres)
insert into _r (linea) select is(pg_temp.o('auth_cuenta'), '42501', 'con sesión normal no se puede preguntar si un correo tiene cuenta');
insert into _r (linea) select is(pg_temp.o('auth_vincula'), '42501', 'con sesión normal nadie se da Administrador de un repo');
insert into _r (linea) select is(pg_temp.o('anon_cuenta'), '42501', 'sin sesión no se puede preguntar si un correo tiene cuenta');
insert into _r (linea) select is(pg_temp.o('anon_vincula'), '42501', 'sin sesión nadie se da Administrador de un repo');
insert into _r (linea) select is(pg_temp.o('cuenta_duena'), 'e0000000-0000-4000-8000-000000000001|true',
    'la cuenta se encuentra por correo sin importar mayúsculas ni espacios');
insert into _r (linea) select is(pg_temp.o('cuenta_sin_confirmar'), 'false', 'dice si la cuenta no ha confirmado el correo');
insert into _r (linea) select is(pg_temp.o('cuenta_inexistente'), '0', 'un correo sin cuenta no devuelve nada');
insert into _r (linea) select is(
    (select nombre || '|' || unity_org || '|' || unity_repo || '|' || creado_por::text from public.proyectos
     where id = pg_temp.o('p')::uuid),
    'Repo/Repo|OrgPrueba|Repo/Repo|e0000000-0000-4000-8000-000000000001',
    'el proyecto se crea con la org y el repo (sin nombre, usa el del repo)');
insert into _r (linea) select is(
    (select rol::text from public.miembros where proyecto_id = pg_temp.o('p')::uuid
     and usuario_id = 'e0000000-0000-4000-8000-000000000001'),
    'Administrador', 'la dueña del repo queda como Administradora');
insert into _r (linea) select is(pg_temp.o('p_otra_vez'), pg_temp.o('p'), 'volver a vincular siendo miembro devuelve el mismo proyecto');
insert into _r (linea) select is(
    (select count(*)::int from public.proyectos where unity_org = 'OrgPrueba'), 1, 'no se duplica el proyecto');
insert into _r (linea) select is(pg_temp.o('otra_vincula_el_mismo'), 'P0001',
    'otra persona no se vuelve miembro de un repo ya vinculado');
insert into _r (linea) select is(
    (select count(*)::int from public.miembros where proyecto_id = pg_temp.o('p')::uuid), 1,
    'el proyecto sigue con un solo miembro');
insert into _r (linea) select is(pg_temp.o('sin_repo'), 'P0001', 'sin repo no se crea nada');
insert into _r (linea) select is(pg_temp.o('cuenta_falsa'), 'P0001', 'una cuenta que no existe no queda de Administradora');

insert into _r (linea) select * from finish();

select string_agg(linea, E'\n' order by n) as tap,
       count(*) filter (where linea like 'not ok%') as fallidas
from _r;

rollback;
