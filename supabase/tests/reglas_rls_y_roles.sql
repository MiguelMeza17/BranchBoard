-- Pruebas de las reglas RLS y de las funciones de roles (#234).
--
-- Corre TODO dentro de una transacción y al final la deshace (rollback): no deja cuentas, proyectos ni
-- eventos. Si una prueba falla, se detiene con "FALLÓ: …".
--
-- Dónde correrlas:
--   * branchboard-pruebas (el entorno real), como el usuario `postgres`, con la cadena de conexión del
--     proyecto (la tiene solo el dueño de la cuenta; nunca en el repo):
--       psql "$BB_DB_PRUEBAS" -v ON_ERROR_STOP=1 -f supabase/tests/reglas_rls_y_roles.sql
--   * Un Postgres local sin Supabase: primero supabase/tests/supabase_simulado_local.sql y las migraciones
--     (instrucciones en ese archivo).
--
-- Cómo simula a cada cuenta: `request.jwt.claims` con su id (lo que lee auth.uid()) y `set local role
-- authenticated` (o `anon`), que es lo que hace la API de Supabase con cada petición.

begin;

-- ---------------------------------------------------------------------------------------------
-- Ayudas
-- ---------------------------------------------------------------------------------------------
create function pg_temp.como(p_usuario uuid) returns void language plpgsql as $$
begin
    perform set_config('request.jwt.claims', json_build_object('sub', p_usuario, 'role', 'authenticated')::text, true);
    execute 'set local role authenticated';
end $$;

create function pg_temp.como_anon() returns void language plpgsql as $$
begin
    perform set_config('request.jwt.claims', '', true);
    execute 'set local role anon';
end $$;

create function pg_temp.como_postgres() returns void language plpgsql as $$
begin
    execute 'reset role';
    perform set_config('request.jwt.claims', '', true);
end $$;

-- Ejecuta `p_sql` y exige que falle con un mensaje que contenga `p_esperado`.
create function pg_temp.debe_fallar(p_prueba text, p_sql text, p_esperado text) returns void language plpgsql as $$
begin
    begin
        execute p_sql;
    exception when others then
        if position(p_esperado in sqlerrm) = 0 then
            raise exception 'FALLÓ: % (error inesperado: %)', p_prueba, sqlerrm;
        end if;
        raise notice 'ok: %', p_prueba;
        return;
    end;
    raise exception 'FALLÓ: % (no dio error y debía dar "%")', p_prueba, p_esperado;
end $$;

create function pg_temp.debe_ser(p_prueba text, p_obtenido bigint, p_esperado bigint) returns void language plpgsql as $$
begin
    if p_obtenido is distinct from p_esperado then
        raise exception 'FALLÓ: % (dio %, se esperaba %)', p_prueba, p_obtenido, p_esperado;
    end if;
    raise notice 'ok: %', p_prueba;
end $$;

grant execute on all functions in schema pg_temp to anon, authenticated;

-- Las funciones de pg_temp corren como quien las llama: la tabla de ids la pueden leer todos.
create temp table ids (nombre text primary key, id uuid);
grant select, insert on ids to anon, authenticated;

-- ---------------------------------------------------------------------------------------------
-- Cuentas de prueba (correos de ejemplo, nunca reales). El trigger de #233 crea sus perfiles.
-- A: Administrador de P1 · B: Líder de P1 · C: Desarrollador de P1 · D: Administrador de P2 (ajeno a P1)
-- E: registrada sin proyectos
-- ---------------------------------------------------------------------------------------------
insert into auth.users (id, email, raw_user_meta_data) values
    (gen_random_uuid(), 'prueba.a@rls.example', '{"nombre":"Ana","apellido":"A"}'),
    (gen_random_uuid(), 'prueba.b@rls.example', '{"nombre":"Beto","apellido":"B"}'),
    (gen_random_uuid(), 'prueba.c@rls.example', '{"nombre":"Caro","apellido":"C"}'),
    (gen_random_uuid(), 'prueba.d@rls.example', '{"nombre":"Dani","apellido":"D"}'),
    (gen_random_uuid(), 'Prueba.E@RLS.example', '{"nombre":"Eva","apellido":"E"}');
insert into ids select upper(substr(email, 8, 1)), id from auth.users where lower(email) like 'prueba._@rls.example';

select pg_temp.debe_ser('el trigger crea un perfil por cuenta, con el correo en minúsculas',
    (select count(*) from public.perfiles where correo like 'prueba._@rls.example'), 5);

-- A crea P1 y arma su equipo; D crea P2.
select pg_temp.como((select id from ids where nombre = 'A'));
insert into ids values ('P1', public.crear_proyecto('Proyecto uno'));
select public.agregar_miembro((select id from ids where nombre = 'P1'), 'prueba.b@rls.example', 'Líder');
select public.agregar_miembro((select id from ids where nombre = 'P1'), 'PRUEBA.C@rls.example', 'Desarrollador');
select pg_temp.como((select id from ids where nombre = 'D'));
insert into ids values ('P2', public.crear_proyecto('Proyecto dos'));
select pg_temp.como_postgres();

select pg_temp.debe_ser('quien crea un proyecto queda como su único Administrador',
    (select count(*) from public.miembros m join ids p on p.id = m.proyecto_id and p.nombre = 'P1'
     where m.rol = 'Administrador'), 1);

-- ---------------------------------------------------------------------------------------------
-- 1. Sin sesión (clave pública sola): nada
-- ---------------------------------------------------------------------------------------------
select pg_temp.como_anon();
select pg_temp.debe_fallar('anon no lee proyectos', 'select * from public.proyectos', 'permission denied');
select pg_temp.debe_fallar('anon no lee eventos', 'select * from public.eventos', 'permission denied');
select pg_temp.debe_fallar('anon no lee perfiles', 'select nombre from public.perfiles', 'permission denied');
select pg_temp.debe_fallar('anon no crea proyectos', $q$select public.crear_proyecto('x')$q$, 'permission denied');

-- ---------------------------------------------------------------------------------------------
-- 2. Alguien de otro proyecto no ve nada de P1
-- ---------------------------------------------------------------------------------------------
select pg_temp.como((select id from ids where nombre = 'D'));
select pg_temp.debe_ser('D no ve P1', (select count(*) from public.proyectos where id = (select id from ids where nombre = 'P1')), 0);
select pg_temp.debe_ser('D solo ve su proyecto', (select count(*) from public.proyectos), 1);
select pg_temp.debe_ser('D no ve miembros de P1', (select count(*) from public.miembros where proyecto_id = (select id from ids where nombre = 'P1')), 0);
select pg_temp.debe_ser('D no ve el perfil de A', (select count(*) from public.perfiles where id = (select id from ids where nombre = 'A')), 0);
select pg_temp.debe_fallar('D no lista los miembros de P1',
    $q$select * from public.miembros_de_proyecto((select id from ids where nombre = 'P1'))$q$, 'sin_permiso');
select pg_temp.debe_fallar('D no inserta eventos en P1',
    $q$insert into public.eventos (id, proyecto_id, autor, tipo)
       values ('ev-d', (select id from ids where nombre = 'P1'), (select id from ids where nombre = 'D'), 'crear_tarea')$q$,
    'row-level security');
select pg_temp.debe_fallar('D no se agrega a P1',
    $q$select public.agregar_miembro((select id from ids where nombre = 'P1'), 'prueba.d@rls.example', 'Administrador')$q$,
    'sin_permiso');

-- ---------------------------------------------------------------------------------------------
-- 3. Eventos: solo a nombre propio, sin tocar orden ni hora del servidor, sin editar ni borrar
-- ---------------------------------------------------------------------------------------------
select pg_temp.como((select id from ids where nombre = 'C'));
insert into public.eventos (id, proyecto_id, autor, tipo, tarea_id, payload, creado_en_app)
values ('ev-c-1', (select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'),
        'crear_tarea', 'T-1', '{"titulo":"x"}', now() - interval '2 hours');
select pg_temp.debe_ser('C (Desarrollador) inserta un evento propio en P1 y lo ve',
    (select count(*) from public.eventos where id = 'ev-c-1'), 1);
select pg_temp.debe_ser('recibido_en lo pone el servidor, no la hora de la app',
    (select count(*) from public.eventos where id = 'ev-c-1' and recibido_en > creado_en_app + interval '1 hour'), 1);
select pg_temp.debe_fallar('C no inserta un evento a nombre de A',
    $q$insert into public.eventos (id, proyecto_id, autor, tipo)
       values ('ev-c-2', (select id from ids where nombre = 'P1'), (select id from ids where nombre = 'A'), 'cambiar_estado')$q$,
    'row-level security');
select pg_temp.debe_fallar('C no fija recibido_en',
    $q$insert into public.eventos (id, proyecto_id, autor, tipo, recibido_en)
       values ('ev-c-3', (select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'x', now() - interval '1 day')$q$,
    'permission denied');
select pg_temp.debe_fallar('C no fija el orden',
    $q$insert into public.eventos (orden, id, proyecto_id, autor, tipo)
       values (1, 'ev-c-4', (select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'x')$q$,
    'non-DEFAULT');  -- `orden` es identity always: Postgres lo rechaza antes de revisar permisos
select pg_temp.debe_fallar('C no edita eventos', $q$update public.eventos set tipo = 'otro' where id = 'ev-c-1'$q$, 'permission denied');
select pg_temp.debe_fallar('C no borra eventos', $q$delete from public.eventos where id = 'ev-c-1'$q$, 'permission denied');
select pg_temp.debe_fallar('el mismo id de evento no se sube dos veces',
    $q$insert into public.eventos (id, proyecto_id, autor, tipo)
       values ('ev-c-1', (select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'crear_tarea')$q$,
    'duplicate key');

-- ---------------------------------------------------------------------------------------------
-- 4. Un Desarrollador no se da roles ni toca las tablas directo
-- ---------------------------------------------------------------------------------------------
select pg_temp.debe_fallar('C no se hace Administrador',
    $q$select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'Administrador')$q$,
    'sin_permiso');
select pg_temp.debe_fallar('C no agrega miembros',
    $q$select public.agregar_miembro((select id from ids where nombre = 'P1'), 'prueba.e@rls.example', 'Tester')$q$, 'sin_permiso');
select pg_temp.debe_fallar('C no escribe en miembros directo',
    $q$update public.miembros set rol = 'Administrador' where usuario_id = (select id from ids where nombre = 'C')$q$,
    'permission denied');
select pg_temp.debe_fallar('C no inserta en miembros directo',
    $q$insert into public.miembros (proyecto_id, usuario_id, rol)
       values ((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'Administrador')$q$,
    'permission denied');
select pg_temp.debe_fallar('C no crea proyectos escribiendo la tabla', $q$insert into public.proyectos (nombre) values ('x')$q$, 'permission denied');
select pg_temp.debe_fallar('C no lee correos por la tabla de perfiles', 'select correo from public.perfiles', 'permission denied');
select pg_temp.debe_ser('C ve los nombres de los 3 miembros de P1', (select count(*) from public.perfiles), 3);
select pg_temp.debe_ser('miembros_de_proyecto no le da correos a un Desarrollador',
    (select count(*) from public.miembros_de_proyecto((select id from ids where nombre = 'P1')) where correo is not null), 0);
-- Intentar cambiar el nombre de otro no da error: la regla filtra la fila (se comprueba abajo que no cambió).
update public.perfiles set nombre = 'X' where id = (select id from ids where nombre = 'A');
update public.perfiles set nombre = 'Carolina' where id = (select id from ids where nombre = 'C');
select pg_temp.como_postgres();
select pg_temp.debe_ser('nadie cambió el nombre de A', (select count(*) from public.perfiles where id = (select id from ids where nombre = 'A') and nombre = 'Ana'), 1);
select pg_temp.debe_ser('C sí cambió el suyo', (select count(*) from public.perfiles where id = (select id from ids where nombre = 'C') and nombre = 'Carolina'), 1);

-- ---------------------------------------------------------------------------------------------
-- 5. Líder: solo roles base
-- ---------------------------------------------------------------------------------------------
select pg_temp.como((select id from ids where nombre = 'B'));
select public.agregar_miembro((select id from ids where nombre = 'P1'), 'prueba.e@rls.example', 'Tester');
select pg_temp.debe_ser('el Líder agrega un Tester', (select count(*) from public.miembros where usuario_id = (select id from ids where nombre = 'E')), 1);
select pg_temp.debe_ser('el Líder ve correos en miembros_de_proyecto',
    (select count(*) from public.miembros_de_proyecto((select id from ids where nombre = 'P1')) where correo is not null), 4);
select pg_temp.debe_fallar('el Líder no nombra Líderes',
    $q$select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'Líder')$q$, 'sin_permiso');
select pg_temp.debe_fallar('el Líder no toca al Administrador',
    $q$select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'A'), 'Tester')$q$, 'sin_permiso');
select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'Tester');
select pg_temp.debe_fallar('el Líder no quita Testers (como puede_eliminar)',
    $q$select public.quitar_miembro((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'))$q$, 'sin_permiso');
select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'), 'Desarrollador');
select public.quitar_miembro((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'C'));
select pg_temp.debe_ser('el Líder quita a un Desarrollador', (select count(*) from public.miembros where usuario_id = (select id from ids where nombre = 'C')), 0);
select pg_temp.debe_fallar('cuenta que no existe',
    $q$select public.agregar_miembro((select id from ids where nombre = 'P1'), 'nadie@rls.example', 'Tester')$q$, 'no_existe_cuenta');
select pg_temp.debe_fallar('no se agrega dos veces',
    $q$select public.agregar_miembro((select id from ids where nombre = 'P1'), 'prueba.e@rls.example', 'Desarrollador')$q$, 'ya_es_miembro');

-- Quitado del proyecto, C ya no ve nada de P1.
select pg_temp.como((select id from ids where nombre = 'C'));
select pg_temp.debe_ser('C, ya quitado, no ve P1', (select count(*) from public.proyectos), 0);
select pg_temp.debe_ser('C, ya quitado, no ve los eventos que subió', (select count(*) from public.eventos), 0);

-- ---------------------------------------------------------------------------------------------
-- 6. Varios Administradores y nunca el proyecto sin ninguno
-- ---------------------------------------------------------------------------------------------
select pg_temp.como((select id from ids where nombre = 'A'));
select pg_temp.debe_fallar('el único Admin no se degrada',
    $q$select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'A'), 'Líder')$q$, 'ultimo_admin');
select pg_temp.debe_fallar('el único Admin no se quita',
    $q$select public.quitar_miembro((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'A'))$q$, 'ultimo_admin');
select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'B'), 'Administrador');
select pg_temp.debe_ser('un Admin nombra a otro Admin',
    (select count(*) from public.miembros where proyecto_id = (select id from ids where nombre = 'P1') and rol = 'Administrador'), 2);
select public.cambiar_rol((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'A'), 'Líder');
select pg_temp.debe_ser('con dos Admins, uno se puede degradar',
    (select count(*) from public.miembros where proyecto_id = (select id from ids where nombre = 'P1') and rol = 'Administrador'), 1);
select pg_temp.como((select id from ids where nombre = 'B'));
select pg_temp.debe_fallar('el nuevo único Admin tampoco se quita',
    $q$select public.quitar_miembro((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'B'))$q$, 'ultimo_admin');
select public.quitar_miembro((select id from ids where nombre = 'P1'), (select id from ids where nombre = 'A'));
select pg_temp.debe_ser('un Admin quita a un Líder', (select count(*) from public.miembros where usuario_id = (select id from ids where nombre = 'A') and proyecto_id = (select id from ids where nombre = 'P1')), 0);

select pg_temp.como_postgres();
do $$ begin raise notice 'TODAS LAS PRUEBAS PASARON'; end $$;
rollback;
