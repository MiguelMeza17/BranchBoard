-- Pruebas de las reglas de Storage: documentos e imagen del proyecto (#242).
--
-- Corren contra el proyecto de PRUEBAS dentro de una transacción que se deshace al final. Escriben las
-- filas de `storage.objects` directamente (como lo hace la API de Storage con la sesión de cada uno), así
-- que no suben archivos de verdad. Cómo correrlas:
--   npx supabase@3 db query --remote --yes --file supabase/tests/03_storage.test.sql
-- La última consulta devuelve las líneas TAP y `fallidas` (tiene que ser 0).
--
-- Cuentas: A (Administrador de P), L (Líder de P), B (Desarrollador de P), C (Administrador de Q, ajeno a P).

begin;
create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;
-- Supabase no deja borrar filas de storage.objects por SQL salvo con este ajuste (la API sí borra).
set local storage.allow_delete_query = 'true';

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

-- Sube (inserta la fila) `ruta` dentro del proyecto `p` (clave de _o) en el bucket `proyectos`.
create function pg_temp.subir(p text, ruta text) returns text language sql as $$
    select pg_temp.intentar(format(
        $q$insert into storage.objects (bucket_id, name, owner_id) values ('proyectos', %L, auth.uid()::text)$q$,
        pg_temp.o(p) || '/' || ruta));
$$;
grant execute on function pg_temp.subir(text, text) to authenticated, anon;

-- Cuántas filas borró (las reglas de delete filtran: 0 = no pudo).
create function pg_temp.borrar(p text, ruta text) returns text language plpgsql as $$
declare n int;
begin
    execute format($q$delete from storage.objects where bucket_id = 'proyectos' and name = %L$q$,
                   pg_temp.o(p) || '/' || ruta);
    get diagnostics n = row_count;
    return n::text;
exception when others then
    return sqlstate;
end;
$$;
grant execute on function pg_temp.borrar(text, text) to authenticated, anon;

create function pg_temp.ve(p text, ruta text) returns text language sql as $$
    select count(*)::text from storage.objects where bucket_id = 'proyectos' and name = pg_temp.o(p) || '/' || ruta;
$$;
grant execute on function pg_temp.ve(text, text) to authenticated, anon;

insert into auth.users (id, email, aud, role) values
    ('a0000000-0000-4000-8000-000000000001', 'a@prueba.branchboard', 'authenticated', 'authenticated'),
    ('b0000000-0000-4000-8000-000000000002', 'b@prueba.branchboard', 'authenticated', 'authenticated'),
    ('c0000000-0000-4000-8000-000000000003', 'c@prueba.branchboard', 'authenticated', 'authenticated'),
    ('d0000000-0000-4000-8000-000000000004', 'l@prueba.branchboard', 'authenticated', 'authenticated');

select no_plan();
create temp table _r (n serial, linea text);

insert into _r (linea) select is(
    (select public::text || ' ' || file_size_limit::text from storage.buckets where id = 'proyectos'),
    'false 52428800', 'el bucket proyectos es privado y acepta hasta 50 MB por archivo');

-- ---------------------------------------------------------------- A crea P con L y B; C crea Q
set local role authenticated;
select set_config('request.jwt.claims', '{"sub":"a0000000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('p', public.crear_proyecto('Proyecto P')::text);
select public.agregar_miembro(pg_temp.o('p')::uuid, 'l@prueba.branchboard', 'Líder');
select public.agregar_miembro(pg_temp.o('p')::uuid, 'b@prueba.branchboard', 'Desarrollador');
insert into _o values ('a_sube_doc', pg_temp.subir('p', 'documentos/D-a.pdf'));
insert into _o values ('a_sube_imagen', pg_temp.subir('p', 'proyecto/icono-a.png'));
with cambiado as (
    update storage.objects set metadata = '{}'::jsonb
    where bucket_id = 'proyectos' and name = pg_temp.o('p') || '/documentos/D-a.pdf' returning 1)
insert into _o values ('a_sobrescribe_doc', (select count(*)::text from cambiado));

select set_config('request.jwt.claims', '{"sub":"c0000000-0000-4000-8000-000000000003","role":"authenticated"}', true);
insert into _o values ('q', public.crear_proyecto('Proyecto Q')::text);
insert into _o values ('c_ve_doc_de_p', pg_temp.ve('p', 'documentos/D-a.pdf'));
insert into _o values ('c_ve_imagen_de_p', pg_temp.ve('p', 'proyecto/icono-a.png'));
insert into _o values ('c_sube_a_p', pg_temp.subir('p', 'documentos/D-c.pdf'));
insert into _o values ('c_borra_doc_de_p', pg_temp.borrar('p', 'documentos/D-a.pdf'));
insert into _o values ('c_sube_a_q', pg_temp.subir('q', 'documentos/D-q.pdf'));

-- ---------------------------------------------------------------- B (Desarrollador de P)
select set_config('request.jwt.claims', '{"sub":"b0000000-0000-4000-8000-000000000002","role":"authenticated"}', true);
insert into _o values ('b_ve_doc', pg_temp.ve('p', 'documentos/D-a.pdf'));
insert into _o values ('b_ve_imagen', pg_temp.ve('p', 'proyecto/icono-a.png'));
insert into _o values ('b_sube_doc', pg_temp.subir('p', 'documentos/D-b.pdf'));
insert into _o values ('b_sube_imagen', pg_temp.subir('p', 'proyecto/icono-b.png'));
insert into _o values ('b_sube_fuera_de_carpeta', pg_temp.subir('p', 'otra/D-b.pdf'));
insert into _o values ('b_sube_en_subcarpeta', pg_temp.subir('p', 'documentos/x/D-b.pdf'));
insert into _o values ('b_sube_sin_nombre', pg_temp.subir('p', 'documentos/'));
insert into _o values ('b_sube_con_ruta_sin_proyecto', pg_temp.intentar(
    $q$insert into storage.objects (bucket_id, name) values ('proyectos', 'no-es-uuid/documentos/D-b.pdf')$q$));
insert into _o values ('b_sube_a_q', pg_temp.subir('q', 'documentos/D-b.pdf'));
insert into _o values ('b_ve_doc_de_q', pg_temp.ve('q', 'documentos/D-q.pdf'));
insert into _o values ('b_borra_su_doc', pg_temp.borrar('p', 'documentos/D-b.pdf'));
insert into _o values ('b_borra_imagen', pg_temp.borrar('p', 'proyecto/icono-a.png'));

-- ---------------------------------------------------------------- L (Líder de P)
select set_config('request.jwt.claims', '{"sub":"d0000000-0000-4000-8000-000000000004","role":"authenticated"}', true);
insert into _o values ('l_sube_imagen', pg_temp.subir('p', 'proyecto/icono-l.png'));
insert into _o values ('l_borra_imagen', pg_temp.borrar('p', 'proyecto/icono-a.png'));
insert into _o values ('l_borra_doc_de_b', pg_temp.borrar('p', 'documentos/D-b.pdf'));

-- ---------------------------------------------------------------- A borra su imagen; sin sesión, nada
select set_config('request.jwt.claims', '{"sub":"a0000000-0000-4000-8000-000000000001","role":"authenticated"}', true);
insert into _o values ('a_borra_imagen', pg_temp.borrar('p', 'proyecto/icono-a.png'));

set local role anon;
select set_config('request.jwt.claims', '{"role":"anon"}', true);
insert into _o values ('anon_ve_doc', pg_temp.ve('p', 'documentos/D-a.pdf'));
insert into _o values ('anon_sube', pg_temp.subir('p', 'documentos/D-anon.pdf'));

reset role;

insert into _r (linea) select is(pg_temp.o('a_sube_doc'), 'ok', 'el Administrador sube un documento');
insert into _r (linea) select is(pg_temp.o('a_sube_imagen'), 'ok', 'el Administrador sube la imagen del proyecto');
insert into _r (linea) select is(pg_temp.o('a_sobrescribe_doc'), '0', 'nada se sobrescribe, ni siquiera el Administrador');
insert into _r (linea) select is(pg_temp.o('c_ve_doc_de_p'), '0', 'alguien de otro proyecto no ve los documentos');
insert into _r (linea) select is(pg_temp.o('c_ve_imagen_de_p'), '0', 'alguien de otro proyecto no ve la imagen');
insert into _r (linea) select is(pg_temp.o('c_sube_a_p'), '42501', 'alguien de otro proyecto no sube documentos');
insert into _r (linea) select is(pg_temp.o('c_borra_doc_de_p'), '0', 'alguien de otro proyecto no borra documentos');
insert into _r (linea) select is(pg_temp.o('c_sube_a_q'), 'ok', 'cada uno sube a su propio proyecto');
insert into _r (linea) select is(pg_temp.o('b_ve_doc'), '1', 'un Desarrollador ve los documentos de su proyecto');
insert into _r (linea) select is(pg_temp.o('b_ve_imagen'), '1', 'un Desarrollador ve la imagen de su proyecto');
insert into _r (linea) select is(pg_temp.o('b_sube_doc'), 'ok', 'un Desarrollador sube documentos (quedan pendientes en la app)');
insert into _r (linea) select is(pg_temp.o('b_sube_imagen'), '42501', 'un Desarrollador no cambia la imagen');
insert into _r (linea) select is(pg_temp.o('b_sube_fuera_de_carpeta'), '42501', 'solo se sube a documentos/ o proyecto/');
insert into _r (linea) select is(pg_temp.o('b_sube_en_subcarpeta'), '42501', 'no se crean subcarpetas');
insert into _r (linea) select is(pg_temp.o('b_sube_sin_nombre'), '42501', 'no se sube un archivo sin nombre');
insert into _r (linea) select is(pg_temp.o('b_sube_con_ruta_sin_proyecto'), '42501', 'una ruta que no empieza por un proyecto se rechaza');
insert into _r (linea) select is(pg_temp.o('b_sube_a_q'), '42501', 'un miembro de P no sube a Q');
insert into _r (linea) select is(pg_temp.o('b_ve_doc_de_q'), '0', 'un miembro de P no ve los documentos de Q');
insert into _r (linea) select is(pg_temp.o('b_borra_su_doc'), '0', 'un Desarrollador no borra documentos, ni los suyos');
insert into _r (linea) select is(pg_temp.o('b_borra_imagen'), '0', 'un Desarrollador no borra la imagen');
insert into _r (linea) select is(pg_temp.o('l_sube_imagen'), '42501', 'el Líder no cambia la imagen (solo Administrador)');
insert into _r (linea) select is(pg_temp.o('l_borra_imagen'), '0', 'el Líder no borra la imagen');
insert into _r (linea) select is(pg_temp.o('l_borra_doc_de_b'), '1', 'el Líder borra documentos');
insert into _r (linea) select is(pg_temp.o('a_borra_imagen'), '1', 'el Administrador borra la imagen anterior');
insert into _r (linea) select is(pg_temp.o('anon_ve_doc'), '0', 'sin sesión no se ve nada');
insert into _r (linea) select is(pg_temp.o('anon_sube'), '42501', 'sin sesión no se sube nada');

insert into _r (linea) select * from finish();

select string_agg(linea, E'\n' order by n) as tap,
       count(*) filter (where linea like 'not ok%') as fallidas
from _r;

rollback;
