-- #269: UUID definitivo, reintento seguro y colisiones. Solo branchboard-pruebas.
begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;
create temp table _r (n serial, linea text);
create temp table _o (clave text primary key, valor text);
grant all on _o to authenticated, anon;
create function pg_temp.intentar(sql text) returns text language plpgsql as $$
begin execute sql; return 'ok'; exception when others then return sqlstate; end;
$$;
grant execute on function pg_temp.intentar(text) to authenticated, anon;
insert into auth.users (id, email, raw_user_meta_data, aud, role) values
('a2690000-0000-4000-8000-000000000001', 'a269@prueba.branchboard', '{}', 'authenticated', 'authenticated'),
('b2690000-0000-4000-8000-000000000002', 'b269@prueba.branchboard', '{}', 'authenticated', 'authenticated');
select no_plan();
set local role authenticated;
select set_config('request.jwt.claims', '{"sub":"a2690000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('id', public.crear_proyecto('Sin red', 'c2690000-0000-4000-8000-000000000003')::text);
insert into _o values ('reintento', pg_temp.intentar($q$select public.crear_proyecto('Sin red', 'c2690000-0000-4000-8000-000000000003')$q$));
insert into _o values ('otro_nombre', pg_temp.intentar($q$select public.crear_proyecto('Otro', 'c2690000-0000-4000-8000-000000000003')$q$));
insert into _o values ('vacio', pg_temp.intentar($q$select public.crear_proyecto(' ', 'd2690000-0000-4000-8000-000000000004')$q$));
select set_config('request.jwt.claims', '{"sub":"b2690000-0000-4000-8000-000000000002","role":"authenticated"}', true);
insert into _o values ('otra_cuenta', pg_temp.intentar($q$select public.crear_proyecto('Sin red', 'c2690000-0000-4000-8000-000000000003')$q$));
reset role;
set local role anon;
select set_config('request.jwt.claims', '{}', true);
insert into _o values ('anon', pg_temp.intentar($q$select public.crear_proyecto('Sin red', 'd2690000-0000-4000-8000-000000000004')$q$));
reset role;
insert into _r(linea) select is((select valor from _o where clave='id'), 'c2690000-0000-4000-8000-000000000003', 'conserva el UUID de la app');
insert into _r(linea) select is((select valor from _o where clave='reintento'), 'ok', 'reintento tras perder respuesta es idempotente');
insert into _r(linea) select is((select valor from _o where clave='otro_nombre'), '23505', 'no sobrescribe un proyecto existente');
insert into _r(linea) select is((select valor from _o where clave='otra_cuenta'), '23505', 'otra cuenta no toma el identificador ni el rol');
insert into _r(linea) select is((select valor from _o where clave='vacio'), 'P0001', 'rechaza nombre vacio');
insert into _r(linea) select is((select valor from _o where clave='anon'), '42501', 'sin sesion no crea');
insert into _r(linea) select is((select count(*)::int from public.miembros where proyecto_id='c2690000-0000-4000-8000-000000000003'), 1, 'solo hay un miembro despues de reintentar y colisionar');
insert into _r(linea) select is((select creado_por::text from public.proyectos where id='c2690000-0000-4000-8000-000000000003'), 'a2690000-0000-4000-8000-000000000001', 'conserva al creador');
insert into _r(linea) select * from finish();
select string_agg(linea, E'\n' order by n) as tap, count(*) filter(where linea like 'not ok%') as fallidas from _r;
rollback;
