-- Pruebas de las reglas RLS y las funciones de roles (#234).
--
-- Corren contra el proyecto de PRUEBAS dentro de una transacción que se deshace al final: crean cuentas,
-- proyectos y eventos de mentira y no dejan nada. Cómo correrlas: ver "Servidor (Supabase)" en BACKEND.md
--   npx supabase@3 db query --remote --yes --file supabase/tests/01_reglas.test.sql
-- La última consulta devuelve las líneas TAP ("ok N - …" / "not ok N - …") y el resumen.
--
-- Cuentas: A (Administrador de P), B (Desarrollador de P), L (Líder de P), C (Administrador de Q, ajeno a P).
-- Cada acción "como" alguien corre con el rol `authenticated` y su `sub`, como haría la app con su sesión;
-- su resultado se guarda en _o y se comprueba después como postgres.

begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;

create temp table _o (clave text primary key, valor text);
grant all on _o to authenticated, anon;

-- Ejecuta `sql` con el rol actual y devuelve 'ok' o el código de error (p. ej. 42501).
create function pg_temp.intentar(sql text) returns text language plpgsql as $$
begin
    execute sql;
    return 'ok';
exception when others then
    return sqlstate;
end;
$$;
grant execute on function pg_temp.intentar(text) to authenticated, anon;

-- Cuentas de mentira (el trigger de #233 les crea el perfil).
insert into auth.users (id, email, raw_user_meta_data, aud, role) values
    ('a0000000-0000-4000-8000-000000000001', 'a@prueba.branchboard', '{"nombre":"Ana","apellido":"Admin"}', 'authenticated', 'authenticated'),
    ('b0000000-0000-4000-8000-000000000002', 'b@prueba.branchboard', '{"nombre":"Beto"}', 'authenticated', 'authenticated'),
    ('c0000000-0000-4000-8000-000000000003', 'c@prueba.branchboard', '{"nombre":"Caro"}', 'authenticated', 'authenticated'),
    ('d0000000-0000-4000-8000-000000000004', 'l@prueba.branchboard', '{"nombre":"Lia"}', 'authenticated', 'authenticated');

select no_plan();
create temp table _r (n serial, linea text);

insert into _r (linea) select is(
    (select count(*)::int from public.perfiles where correo like '%@prueba.branchboard'), 4,
    'el trigger crea un perfil por cada cuenta nueva');
insert into _r (linea) select is(
    (select nombre || ' ' || apellido from public.perfiles where correo = 'a@prueba.branchboard'), 'Ana Admin',
    'el perfil toma nombre y apellido de los metadatos del registro');

-- ---------------------------------------------------------------- A crea P y arma su equipo
set local role authenticated;
select set_config('request.jwt.claims', '{"sub":"a0000000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('p', public.crear_proyecto('Proyecto P')::text);
insert into _o values ('a_agrega_b', pg_temp.intentar(format(
    $q$select public.agregar_miembro(%L, 'b@prueba.branchboard', 'Desarrollador')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('a_agrega_l', pg_temp.intentar(format(
    $q$select public.agregar_miembro(%L, 'L@Prueba.BranchBoard', 'Líder')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('a_agrega_sin_cuenta', pg_temp.intentar(format(
    $q$select public.agregar_miembro(%L, 'nadie@prueba.branchboard', 'Tester')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('a_ve_proyectos', (select count(*)::text from public.proyectos));
insert into _o values ('a_degrada_al_ultimo_admin', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'a0000000-0000-4000-8000-000000000001', 'Desarrollador')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('a_se_quita_siendo_ultimo_admin', pg_temp.intentar(format(
    $q$select public.quitar_miembro(%L, 'a0000000-0000-4000-8000-000000000001')$q$, (select valor from _o where clave = 'p'))));

-- ---------------------------------------------------------------- C crea Q (otro equipo)
select set_config('request.jwt.claims', '{"sub":"c0000000-0000-4000-8000-000000000003","role":"authenticated"}', true);
insert into _o values ('q', public.crear_proyecto('Proyecto Q')::text);
insert into _o values ('c_ve_p', (select count(*)::text from public.proyectos where id = (select valor::uuid from _o where clave = 'p')));
insert into _o values ('c_ve_miembros_de_p', (select count(*)::text from public.miembros where proyecto_id = (select valor::uuid from _o where clave = 'p')));
insert into _o values ('c_ve_perfil_de_b', (select count(*)::text from public.perfiles where id = 'b0000000-0000-4000-8000-000000000002'));
insert into _o values ('c_se_agrega_a_p', pg_temp.intentar(format(
    $q$select public.agregar_miembro(%L, 'c@prueba.branchboard', 'Administrador')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('c_inserta_evento_en_p', pg_temp.intentar(format(
    $q$insert into public.eventos (id, proyecto_id, tipo) values ('ev-c', %L, 'crear_tarea')$q$, (select valor from _o where clave = 'p'))));

-- ---------------------------------------------------------------- B (Desarrollador de P)
select set_config('request.jwt.claims', '{"sub":"b0000000-0000-4000-8000-000000000002","role":"authenticated"}', true);
insert into _o values ('b_se_da_admin', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'b0000000-0000-4000-8000-000000000002', 'Administrador')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_agrega_a_alguien', pg_temp.intentar(format(
    $q$select public.agregar_miembro(%L, 'c@prueba.branchboard', 'Tester')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_escribe_miembros_directo', pg_temp.intentar(format(
    $q$update public.miembros set rol = 'Administrador' where proyecto_id = %L$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_renombra_proyecto_directo', pg_temp.intentar(format(
    $q$update public.proyectos set nombre = 'mío' where id = %L$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_inserta_evento', pg_temp.intentar(format(
    $q$insert into public.eventos (id, proyecto_id, tipo, payload, creado_en_app) values ('ev-b', %L, 'crear_tarea', '{"titulo":"x"}', '2000-01-01')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_inserta_evento_como_otro', pg_temp.intentar(format(
    $q$insert into public.eventos (id, proyecto_id, tipo, autor) values ('ev-b2', %L, 'crear_tarea', 'a0000000-0000-4000-8000-000000000001')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_pone_la_hora', pg_temp.intentar(format(
    $q$insert into public.eventos (id, proyecto_id, tipo, recibido_en) values ('ev-b3', %L, 'crear_tarea', '2000-01-01')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('b_inserta_en_q', pg_temp.intentar(format(
    $q$insert into public.eventos (id, proyecto_id, tipo) values ('ev-b4', %L, 'crear_tarea')$q$, (select valor from _o where clave = 'q'))));
insert into _o values ('b_edita_evento', pg_temp.intentar($q$update public.eventos set tipo = 'otro' where id = 'ev-b'$q$));
insert into _o values ('b_borra_evento', pg_temp.intentar($q$delete from public.eventos where id = 'ev-b'$q$));
insert into _o values ('b_ve_eventos_de_p', (select count(*)::text from public.eventos where proyecto_id = (select valor::uuid from _o where clave = 'p')));
insert into _o values ('b_ve_perfiles', (select count(*)::text from public.perfiles where correo like '%@prueba.branchboard'));
insert into _o values ('b_edita_su_nombre', pg_temp.intentar($q$update public.perfiles set nombre = 'Beto B' where id = 'b0000000-0000-4000-8000-000000000002'$q$));
insert into _o values ('b_edita_su_correo', pg_temp.intentar($q$update public.perfiles set correo = 'x@x.x' where id = 'b0000000-0000-4000-8000-000000000002'$q$));
with cambiado as (update public.perfiles set nombre = 'hackeado' where id = 'a0000000-0000-4000-8000-000000000001' returning 1)
insert into _o values ('b_edita_nombre_de_a', (select count(*)::text from cambiado));

-- ---------------------------------------------------------------- L (Líder de P)
select set_config('request.jwt.claims', '{"sub":"d0000000-0000-4000-8000-000000000004","role":"authenticated"}', true);
insert into _o values ('l_agrega_c_tester', pg_temp.intentar(format(
    $q$select public.agregar_miembro(%L, 'c@prueba.branchboard', 'Tester')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('l_da_lider', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'b0000000-0000-4000-8000-000000000002', 'Líder')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('l_cambia_rol_de_admin', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'a0000000-0000-4000-8000-000000000001', 'Tester')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('l_quita_tester', pg_temp.intentar(format(
    $q$select public.quitar_miembro(%L, 'c0000000-0000-4000-8000-000000000003')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('l_cambia_b_a_tester', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'b0000000-0000-4000-8000-000000000002', 'Tester')$q$, (select valor from _o where clave = 'p'))));

-- ---------------------------------------------------------------- C ya es Tester de P: ahora ve B
select set_config('request.jwt.claims', '{"sub":"c0000000-0000-4000-8000-000000000003","role":"authenticated"}', true);
insert into _o values ('c_ve_perfil_de_b_tras_unirse', (select count(*)::text from public.perfiles where id = 'b0000000-0000-4000-8000-000000000002'));

-- ---------------------------------------------------------------- A nombra a L Administradora y se degrada
select set_config('request.jwt.claims', '{"sub":"a0000000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('a_nombra_admin_a_l', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'd0000000-0000-4000-8000-000000000004', 'Administrador')$q$, (select valor from _o where clave = 'p'))));
insert into _o values ('a_se_degrada_con_otro_admin', pg_temp.intentar(format(
    $q$select public.cambiar_rol(%L, 'a0000000-0000-4000-8000-000000000001', 'Desarrollador')$q$, (select valor from _o where clave = 'p'))));

select set_config('request.jwt.claims', '{"sub":"d0000000-0000-4000-8000-000000000004","role":"authenticated"}', true);
insert into _o values ('l_se_quita_siendo_ultima_admin', pg_temp.intentar(format(
    $q$select public.quitar_miembro(%L, 'd0000000-0000-4000-8000-000000000004')$q$, (select valor from _o where clave = 'p'))));

-- ---------------------------------------------------------------- Sin sesión
set local role anon;
select set_config('request.jwt.claims', '{"role":"anon"}', true);
insert into _o values ('anon_lee_proyectos', pg_temp.intentar('select * from public.proyectos'));
insert into _o values ('anon_lee_eventos', pg_temp.intentar('select * from public.eventos'));
insert into _o values ('anon_crea_proyecto', pg_temp.intentar($q$select public.crear_proyecto('x')$q$));

reset role;

-- ---------------------------------------------------------------- Comprobaciones
create function pg_temp.o(c text) returns text language sql as $$ select valor from _o where clave = c $$;

insert into _r (linea) select is(pg_temp.o('a_agrega_b'), 'ok', 'el Administrador agrega un Desarrollador');
insert into _r (linea) select is(pg_temp.o('a_agrega_l'), 'ok', 'el Administrador agrega un Líder (el correo da igual en mayúsculas)');
insert into _r (linea) select is(pg_temp.o('a_agrega_sin_cuenta'), 'P0001', 'no se agrega a alguien sin cuenta (invitaciones en #241)');
insert into _r (linea) select is(pg_temp.o('a_ve_proyectos'), '1', 'el Administrador ve su proyecto');
insert into _r (linea) select is(pg_temp.o('a_degrada_al_ultimo_admin'), 'P0001', 'no se puede degradar al último Administrador');
insert into _r (linea) select is(pg_temp.o('a_se_quita_siendo_ultimo_admin'), 'P0001', 'el último Administrador no se puede quitar');
insert into _r (linea) select is(pg_temp.o('c_ve_p'), '0', 'otro equipo no ve el proyecto');
insert into _r (linea) select is(pg_temp.o('c_ve_miembros_de_p'), '0', 'otro equipo no ve sus miembros');
insert into _r (linea) select is(pg_temp.o('c_ve_perfil_de_b'), '0', 'no se ven perfiles de gente con la que no compartes proyecto');
insert into _r (linea) select is(pg_temp.o('c_se_agrega_a_p'), '42501', 'nadie se mete solo en un proyecto ajeno');
insert into _r (linea) select is(pg_temp.o('c_inserta_evento_en_p'), '42501', 'otro equipo no inserta eventos en el proyecto');
insert into _r (linea) select is(pg_temp.o('b_se_da_admin'), '42501', 'un Desarrollador no puede darse Administrador');
insert into _r (linea) select is(pg_temp.o('b_agrega_a_alguien'), '42501', 'un Desarrollador no agrega gente');
insert into _r (linea) select is(pg_temp.o('b_escribe_miembros_directo'), '42501', 'nadie escribe la tabla miembros directo');
insert into _r (linea) select is(pg_temp.o('b_renombra_proyecto_directo'), '42501', 'nadie escribe la tabla proyectos directo');
insert into _r (linea) select is(pg_temp.o('b_inserta_evento'), 'ok', 'un miembro inserta eventos en su proyecto');
insert into _r (linea) select is(pg_temp.o('b_inserta_evento_como_otro'), '42501', 'no se puede elegir el autor de un evento');
insert into _r (linea) select is(pg_temp.o('b_pone_la_hora'), '42501', 'no se puede elegir la hora del servidor');
insert into _r (linea) select is(pg_temp.o('b_inserta_en_q'), '42501', 'no se insertan eventos en un proyecto ajeno');
insert into _r (linea) select is(pg_temp.o('b_edita_evento'), '42501', 'los eventos no se editan');
insert into _r (linea) select is(pg_temp.o('b_borra_evento'), '42501', 'los eventos no se borran');
insert into _r (linea) select is(pg_temp.o('b_ve_eventos_de_p'), '1', 'un miembro ve los eventos de su proyecto');
insert into _r (linea) select is(
    (select autor::text from public.eventos where id = 'ev-b'), 'b0000000-0000-4000-8000-000000000002',
    'el autor del evento lo pone el servidor');
insert into _r (linea) select ok(
    (select recibido_en > now() - interval '1 hour' from public.eventos where id = 'ev-b'),
    'la hora del evento la pone el servidor (no la de la app)');
insert into _r (linea) select is(pg_temp.o('b_ve_perfiles'), '3', 'un miembro ve los perfiles de su equipo (A, B, L), no los de otros');
insert into _r (linea) select is(pg_temp.o('b_edita_su_nombre'), 'ok', 'cada uno edita su nombre');
insert into _r (linea) select is(pg_temp.o('b_edita_su_correo'), '42501', 'el correo no se edita (lo maneja Auth)');
insert into _r (linea) select is(pg_temp.o('b_edita_nombre_de_a'), '0', 'nadie edita el perfil de otro');
insert into _r (linea) select is(pg_temp.o('l_agrega_c_tester'), 'ok', 'el Líder agrega un Tester');
insert into _r (linea) select is(pg_temp.o('l_da_lider'), '42501', 'el Líder no da el rol de Líder');
insert into _r (linea) select is(pg_temp.o('l_cambia_rol_de_admin'), '42501', 'el Líder no cambia el rol de un Administrador');
insert into _r (linea) select is(pg_temp.o('l_quita_tester'), '42501', 'el Líder no quita Testers (igual que puede_eliminar de la app)');
insert into _r (linea) select is(pg_temp.o('l_cambia_b_a_tester'), 'ok', 'el Líder cambia un Desarrollador a Tester');
insert into _r (linea) select is(pg_temp.o('c_ve_perfil_de_b_tras_unirse'), '1', 'al compartir proyecto se ven los perfiles');
insert into _r (linea) select is(pg_temp.o('a_nombra_admin_a_l'), 'ok', 'un Administrador nombra a otro Administrador (decisión 5 de #231)');
insert into _r (linea) select is(pg_temp.o('a_se_degrada_con_otro_admin'), 'ok', 'con otro Administrador, uno se puede degradar');
insert into _r (linea) select is(pg_temp.o('l_se_quita_siendo_ultima_admin'), 'P0001', 'la nueva última Administradora tampoco se puede quitar');
insert into _r (linea) select is(pg_temp.o('anon_lee_proyectos'), '42501', 'sin sesión no se leen proyectos');
insert into _r (linea) select is(pg_temp.o('anon_lee_eventos'), '42501', 'sin sesión no se leen eventos');
insert into _r (linea) select is(pg_temp.o('anon_crea_proyecto'), '42501', 'sin sesión no se crean proyectos');

insert into _r (linea) select * from finish();

select string_agg(linea, E'\n' order by n) as tap,
       count(*) filter (where linea like 'not ok%') as fallidas
from _r;

rollback;
