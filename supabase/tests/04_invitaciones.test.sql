-- Pruebas de las invitaciones por correo (#241).
--
-- Corren contra el proyecto de PRUEBAS dentro de una transacción que se deshace al final. Cómo correrlas:
--   npx supabase@3 db query --remote --yes --file supabase/tests/04_invitaciones.test.sql
-- La última consulta devuelve las líneas TAP y `fallidas` (tiene que ser 0).
--
-- Cuentas: A (Administrador de P), L (Líder de P), B (Desarrollador de P). N y U todavía no tienen cuenta.

begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;

create temp table _o (clave text primary key, valor text);
grant all on _o to authenticated, anon;

create function pg_temp.intentar(sql text) returns text language plpgsql as $$
begin
    execute sql;
    return 'ok';
exception when others then
    return sqlstate;
end;
$$;
grant execute on function pg_temp.intentar(text) to authenticated, anon;
create function pg_temp.o(c text) returns text language sql as $$ select valor from _o where clave = c $$;
grant execute on function pg_temp.o(text) to authenticated, anon;

-- `agregar_miembro` y `quitar_invitacion` sobre P, con el resultado o el código de error.
create function pg_temp.agregar(correo text, rol text) returns text language sql as $$
    select pg_temp.intentar(format('select public.agregar_miembro(%L, %L, %L)', pg_temp.o('p'), correo, rol));
$$;
create function pg_temp.quitar_inv(correo text) returns text language sql as $$
    select pg_temp.intentar(format('select public.quitar_invitacion(%L, %L)', pg_temp.o('p'), correo));
$$;
grant execute on function pg_temp.agregar(text, text), pg_temp.quitar_inv(text) to authenticated, anon;

insert into auth.users (id, email, email_confirmed_at, aud, role) values
    ('a0000000-0000-4000-8000-000000000001', 'a@prueba.branchboard', now(), 'authenticated', 'authenticated'),
    ('b0000000-0000-4000-8000-000000000002', 'b@prueba.branchboard', now(), 'authenticated', 'authenticated'),
    ('d0000000-0000-4000-8000-000000000004', 'l@prueba.branchboard', now(), 'authenticated', 'authenticated');

select no_plan();
create temp table _r (n serial, linea text);

-- ---------------------------------------------------------------- A crea P, agrega a L y B, invita a N
set local role authenticated;
select set_config('request.jwt.claims', '{"sub":"a0000000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('p', public.crear_proyecto('Proyecto P')::text);
insert into _o values ('a_agrega_l', pg_temp.agregar('l@prueba.branchboard', 'Líder'));
insert into _o values ('a_agrega_b', pg_temp.agregar('b@prueba.branchboard', 'Desarrollador'));
insert into _o values ('a_invita_n_devuelve',
    coalesce((select public.agregar_miembro(pg_temp.o('p')::uuid, '  N@Prueba.BranchBoard ', 'Tester'))::text, 'null'));
insert into _o values ('a_invita_n_otra_vez', pg_temp.agregar('n@prueba.branchboard', 'Tester'));
insert into _o values ('a_invita_correo_malo', pg_temp.agregar('no-es-correo', 'Tester'));
insert into _o values ('a_invita_admin', pg_temp.agregar('u@prueba.branchboard', 'Administrador'));
insert into _o values ('a_ve_invitaciones', (select count(*)::text from public.invitaciones));
insert into _o values ('a_ve_correo_n', (select correo from public.invitaciones where rol = 'Tester'));
insert into _o values ('a_escribe_invitacion_directo', pg_temp.intentar(format(
    $q$insert into public.invitaciones (proyecto_id, correo, rol) values (%L, 'x@prueba.branchboard', 'Tester')$q$, pg_temp.o('p'))));

-- ---------------------------------------------------------------- B (Desarrollador) y L (Líder)
select set_config('request.jwt.claims', '{"sub":"b0000000-0000-4000-8000-000000000002","role":"authenticated"}', true);
insert into _o values ('b_ve_invitaciones', (select count(*)::text from public.invitaciones));
insert into _o values ('b_invita', pg_temp.agregar('z@prueba.branchboard', 'Tester'));
insert into _o values ('b_quita_invitacion', pg_temp.quitar_inv('n@prueba.branchboard'));

select set_config('request.jwt.claims', '{"sub":"d0000000-0000-4000-8000-000000000004","role":"authenticated"}', true);
insert into _o values ('l_ve_invitaciones', (select count(*)::text from public.invitaciones));
insert into _o values ('l_invita_lider', pg_temp.agregar('z@prueba.branchboard', 'Líder'));
insert into _o values ('l_quita_invitacion_de_admin', pg_temp.quitar_inv('u@prueba.branchboard'));
insert into _o values ('l_quita_invitacion_de_tester', pg_temp.quitar_inv('n@prueba.branchboard'));
insert into _o values ('l_quita_invitacion_que_no_existe', pg_temp.quitar_inv('nadie@prueba.branchboard'));
-- L vuelve a invitar a N para que se registre.
insert into _o values ('l_invita_n', pg_temp.agregar('n@prueba.branchboard', 'Tester'));

-- ---------------------------------------------------------------- N se registra (sin confirmar) y confirma
reset role;
insert into auth.users (id, email, aud, role) values
    ('e0000000-0000-4000-8000-000000000005', 'N@prueba.branchboard', 'authenticated', 'authenticated');
insert into _o values ('n_sin_confirmar_es_miembro',
    (select count(*)::text from public.miembros where usuario_id = 'e0000000-0000-4000-8000-000000000005'));
update auth.users set email_confirmed_at = now() where id = 'e0000000-0000-4000-8000-000000000005';
insert into _o values ('n_confirmado_rol',
    (select rol::text from public.miembros where usuario_id = 'e0000000-0000-4000-8000-000000000005'));
insert into _o values ('n_invitacion_borrada',
    (select count(*)::text from public.invitaciones where correo = 'n@prueba.branchboard'));
-- U entra ya confirmada (como con Unity VC, #237): pasa directo.
insert into auth.users (id, email, email_confirmed_at, aud, role) values
    ('f0000000-0000-4000-8000-000000000006', 'u@prueba.branchboard', now(), 'authenticated', 'authenticated');
insert into _o values ('u_rol',
    (select rol::text from public.miembros where usuario_id = 'f0000000-0000-4000-8000-000000000006'));

-- Ya con cuenta, agregarla otra vez dice que ya está.
set local role authenticated;
select set_config('request.jwt.claims', '{"sub":"a0000000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('a_agrega_n_ya_miembro', pg_temp.agregar('n@prueba.branchboard', 'Tester'));

set local role anon;
select set_config('request.jwt.claims', '{"role":"anon"}', true);
insert into _o values ('anon_lee_invitaciones', pg_temp.intentar('select 1 from public.invitaciones'));
insert into _o values ('anon_quita_invitacion', pg_temp.quitar_inv('u@prueba.branchboard'));
reset role;

insert into _r (linea) select is(pg_temp.o('a_agrega_l'), 'ok', 'con cuenta se agrega como antes');
insert into _r (linea) select is(pg_temp.o('a_invita_n_devuelve'), 'null', 'sin cuenta queda invitada (agregar_miembro devuelve null)');
insert into _r (linea) select is(pg_temp.o('a_ve_correo_n'), 'n@prueba.branchboard', 'el correo de la invitación se guarda limpio y en minúsculas');
insert into _r (linea) select is(pg_temp.o('a_invita_n_otra_vez'), 'P0001', 'no se invita dos veces al mismo correo');
insert into _r (linea) select is(pg_temp.o('a_invita_correo_malo'), 'P0001', 'un correo mal escrito se rechaza');
insert into _r (linea) select is(pg_temp.o('a_invita_admin'), 'ok', 'el Administrador invita a otro Administrador');
insert into _r (linea) select is(pg_temp.o('a_ve_invitaciones'), '2', 'el Administrador ve las invitaciones');
insert into _r (linea) select is(pg_temp.o('a_escribe_invitacion_directo'), '42501', 'nadie escribe invitaciones directo (solo con agregar_miembro)');
insert into _r (linea) select is(pg_temp.o('b_ve_invitaciones'), '0', 'un Desarrollador no ve invitaciones (traen correos)');
insert into _r (linea) select is(pg_temp.o('b_invita'), '42501', 'un Desarrollador no invita');
insert into _r (linea) select is(pg_temp.o('b_quita_invitacion'), '42501', 'un Desarrollador no quita invitaciones');
insert into _r (linea) select is(pg_temp.o('l_ve_invitaciones'), '2', 'el Líder ve las invitaciones');
insert into _r (linea) select is(pg_temp.o('l_invita_lider'), '42501', 'el Líder no invita Líderes');
insert into _r (linea) select is(pg_temp.o('l_quita_invitacion_de_admin'), '42501', 'el Líder no quita la invitación de un Administrador');
insert into _r (linea) select is(pg_temp.o('l_quita_invitacion_de_tester'), 'ok', 'el Líder quita la invitación de un Tester');
insert into _r (linea) select is(pg_temp.o('l_quita_invitacion_que_no_existe'), '42501', 'quitar una invitación que no existe da sin permiso');
insert into _r (linea) select is(pg_temp.o('l_invita_n'), 'ok', 'el Líder invita a un Tester');
insert into _r (linea) select is(pg_temp.o('n_sin_confirmar_es_miembro'), '0', 'registrarse sin confirmar el correo no da acceso');
insert into _r (linea) select is(pg_temp.o('n_confirmado_rol'), 'Tester', 'al confirmar el correo entra con el rol de la invitación');
insert into _r (linea) select is(pg_temp.o('n_invitacion_borrada'), '0', 'la invitación aceptada se borra');
insert into _r (linea) select is(pg_temp.o('u_rol'), 'Administrador', 'una cuenta que nace confirmada (Unity VC) entra directo');
insert into _r (linea) select is(pg_temp.o('a_agrega_n_ya_miembro'), 'P0001', 'ya con cuenta, agregarla otra vez dice que ya está');
insert into _r (linea) select is(pg_temp.o('anon_lee_invitaciones'), '42501', 'sin sesión no se leen invitaciones');
insert into _r (linea) select is(pg_temp.o('anon_quita_invitacion'), '42501', 'sin sesión no se quitan invitaciones');

insert into _r (linea) select * from finish();

select string_agg(linea, E'\n' order by n) as tap,
       count(*) filter (where linea like 'not ok%') as fallidas
from _r;

rollback;
