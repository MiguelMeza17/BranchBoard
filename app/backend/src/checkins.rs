//! Check-ins de la rama de una tarea (issue #84): se leen del servidor y se
//! guardan en la tabla `commits`, que es solo una copia para mostrarlos rápido.
//!
//! Se leen por la API REST (`GET …/changesets?branchName=`), no con `cm log`: así
//! no hace falta tener un workspace del repo, y quien revisa puede ver los de
//! ramas ajenas (ver la tabla "API REST / cm" en BACKEND.md).

use chrono::{DateTime, Local};
use rusqlite::Connection;

use crate::db::commits::{self, Commit};
use crate::error::Resultado;
use crate::modelo::normalizar_email;
use crate::servidor::{Changeset, ClienteServidor};

/// Título cuando el check-in no trae comentario.
pub const SIN_COMENTARIO: &str = "Sin comentario";

/// Lee los check-ins de `rama` y los guarda como commits de `tarea_id`.
/// Devuelve todos los de la tarea, del más antiguo al más reciente.
/// Rama que todavía no existe en el servidor → lista vacía.
pub fn leer_checkins_rama(
    cliente: &ClienteServidor,
    c: &Connection,
    repo: &str,
    tarea_id: &str,
    rama: &str,
) -> Resultado<Vec<Commit>> {
    let changesets = cliente.changesets_de_rama(repo, rama)?.unwrap_or_default();
    guardar_changesets(c, tarea_id, &changesets)?;
    commits::de_tarea(c, tarea_id)
}

/// Guarda cada changeset sin duplicar: el mismo changeset en la misma tarea
/// reemplaza la copia, conservando el `id` y el `tipo` que ya tenía (el tipo lo
/// elige quien hace check-in desde BranchBoard y no está en el servidor).
/// Devuelve cuántos eran nuevos.
pub fn guardar_changesets(
    c: &Connection,
    tarea_id: &str,
    changesets: &[Changeset],
) -> Resultado<usize> {
    let ya_estaban = commits::de_tarea(c, tarea_id)?;
    let mut ordenados: Vec<&Changeset> = changesets.iter().collect();
    ordenados.sort_by_key(|cs| cs.changeset_id);
    let mut nuevos = 0;
    for cs in ordenados {
        let hash = cs.changeset_id.to_string();
        let existente = ya_estaban.iter().find(|x| x.hash == hash);
        if existente.is_none() {
            nuevos += 1;
        }
        commits::guardar(c, &a_commit(tarea_id, cs, existente))?;
    }
    Ok(nuevos)
}

fn a_commit(tarea_id: &str, cs: &Changeset, existente: Option<&Commit>) -> Commit {
    let (mensaje, descripcion) = separar_comentario(cs.comment.as_deref().unwrap_or_default());
    Commit {
        id: existente.map_or_else(
            || format!("C-{tarea_id}-{}", cs.changeset_id),
            |e| e.id.clone(),
        ),
        tarea_id: tarea_id.to_string(),
        hash: cs.changeset_id.to_string(),
        mensaje,
        descripcion,
        tipo: existente.and_then(|e| e.tipo),
        autor: autor(&cs.owner),
        fecha: fecha_local(&cs.date),
    }
}

/// Primera línea no vacía = título; el resto = descripción (`None` si no hay).
pub fn separar_comentario(comentario: &str) -> (String, Option<String>) {
    let texto = comentario.replace("\r\n", "\n");
    let texto = texto.trim();
    let (titulo, resto) = texto.split_once('\n').unwrap_or((texto, ""));
    let titulo = titulo.trim();
    let resto = resto.trim();
    (
        if titulo.is_empty() {
            SIN_COMENTARIO.to_string()
        } else {
            titulo.to_string()
        },
        (!resto.is_empty()).then(|| resto.to_string()),
    )
}

fn autor(owner: &str) -> String {
    if owner.contains('@') {
        normalizar_email(owner)
    } else {
        owner.trim().to_string()
    }
}

/// `2026-11-12T17:20:04Z` (UTC) → hora local `2026-11-12T12:20:04`, como el
/// frontend. Si no se entiende, se deja tal cual.
fn fecha_local(fecha: &str) -> String {
    DateTime::parse_from_rfc3339(fecha.trim()).map_or_else(
        |_| fecha.trim().to_string(),
        |f| {
            f.with_timezone(&Local)
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{tareas, Db};
    use crate::modelo::TipoTarea;

    fn db() -> Db {
        let db = Db::en_memoria().unwrap();
        tareas::insertar(
            db.conexion(),
            &tareas::Tarea::nueva(
                "T-101",
                "Tarea",
                TipoTarea::Scripting,
                "ana@virustf.dev",
                "2026-11-09",
            ),
        )
        .unwrap();
        db
    }

    fn cs(n: i64, comentario: Option<&str>) -> Changeset {
        Changeset {
            changeset_id: n,
            owner: "Ana@VirusTF.dev".into(),
            date: "2026-11-12T17:20:04Z".into(),
            comment: comentario.map(String::from),
            parent_changeset_id: None,
        }
    }

    #[test]
    fn separa_titulo_y_descripcion() {
        assert_eq!(
            separar_comentario("Menú principal\r\n\r\nBotones y\nnavegación\n"),
            (
                "Menú principal".to_string(),
                Some("Botones y\nnavegación".to_string())
            )
        );
        assert_eq!(
            separar_comentario("  Solo título  "),
            ("Solo título".to_string(), None)
        );
        assert_eq!(separar_comentario(""), (SIN_COMENTARIO.to_string(), None));
        assert_eq!(
            separar_comentario("\n\nTítulo tras líneas vacías"),
            ("Título tras líneas vacías".to_string(), None)
        );
    }

    #[test]
    fn guarda_sin_duplicar_y_en_orden() {
        let db = db();
        let c = db.conexion();
        let lista = [cs(9, Some("Segundo")), cs(4, Some("Primero\nDetalle"))];
        assert_eq!(guardar_changesets(c, "T-101", &lista).unwrap(), 2);
        assert_eq!(
            guardar_changesets(c, "T-101", &lista).unwrap(),
            0,
            "ya estaban"
        );
        let guardados = commits::de_tarea(c, "T-101").unwrap();
        assert_eq!(guardados.len(), 2);
        assert_eq!(guardados[0].hash, "4");
        assert_eq!(guardados[0].id, "C-T-101-4");
        assert_eq!(guardados[0].mensaje, "Primero");
        assert_eq!(guardados[0].descripcion.as_deref(), Some("Detalle"));
        assert_eq!(guardados[0].autor, "ana@virustf.dev");
        assert_eq!(guardados[1].mensaje, "Segundo");
    }

    #[test]
    fn conserva_el_id_y_el_tipo_de_un_check_in_hecho_desde_branchboard() {
        let db = db();
        let c = db.conexion();
        commits::guardar(
            c,
            &Commit {
                id: "C-7".into(),
                tarea_id: "T-101".into(),
                hash: "4".into(),
                mensaje: "Viejo".into(),
                descripcion: None,
                tipo: Some(TipoTarea::Audio),
                autor: "ana@virustf.dev".into(),
                fecha: "2026-11-12T12:00:00".into(),
            },
        )
        .unwrap();
        assert_eq!(
            guardar_changesets(c, "T-101", &[cs(4, Some("Nuevo"))]).unwrap(),
            0
        );
        let g = &commits::de_tarea(c, "T-101").unwrap()[0];
        assert_eq!(
            (g.id.as_str(), g.tipo, g.mensaje.as_str()),
            ("C-7", Some(TipoTarea::Audio), "Nuevo")
        );
    }

    #[test]
    fn la_fecha_pasa_a_hora_local_sin_zona() {
        let local = fecha_local("2026-11-12T17:20:04Z");
        assert_eq!(local.len(), "2026-11-12T17:20:04".len(), "{local}");
        assert!(!local.ends_with('Z'));
        let esperado = DateTime::parse_from_rfc3339("2026-11-12T17:20:04Z")
            .unwrap()
            .with_timezone(&Local)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        assert_eq!(local, esperado);
        assert_eq!(fecha_local("no es fecha"), "no es fecha");
    }

    /// **Prueba en el entorno real** (CLAUDE.md): crea una rama de prueba, le hace
    /// dos check-ins desde un workspace temporal (uno con descripción), los lee por
    /// la API, los guarda sin duplicar y al final borra la rama y el workspace.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_checkins -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_checkins_de_una_rama() {
        use crate::cm::Comando;
        use crate::estado::AppState;
        use crate::ramas::{cambiar_a_rama, crear_rama_tarea};
        use crate::servidor::{conectar, LlaveroSistema};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let repo_cm = format!("{repo}@{org}@unity");
        let estado = AppState::default();
        let email = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let cliente = conectar(&estado, &LlaveroSistema, &email, &org).unwrap();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let id = format!("T-7{sufijo}");
        let carpeta = tempfile::tempdir().unwrap();
        let ruta_ws = carpeta.path().to_string_lossy().to_string();
        let mut rama_creada: Option<String> = None;

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let rama = crear_rama_tarea(&cliente, &repo, None, &id, "Prueba 84").unwrap();
            rama_creada = Some(rama.name.clone());

            // Rama recién creada: todavía sin check-ins.
            let db = Db::en_memoria().unwrap();
            let c = db.conexion();
            tareas::insertar(
                c,
                &tareas::Tarea::nueva(&id, "Prueba 84", TipoTarea::Scripting, &email, "2026-11-14"),
            )
            .unwrap();
            assert!(leer_checkins_rama(&cliente, c, &repo, &id, &rama.name)
                .unwrap()
                .is_empty());

            // Dos check-ins de verdad desde un workspace temporal.
            Comando::cm()
                .args(["workspace", "create"])
                .arg(format!("bb-prueba-84-{sufijo}"))
                .arg(&ruta_ws)
                .arg(format!("rep:{repo_cm}"))
                .exigir_exito(&estado)
                .unwrap();
            cambiar_a_rama(&estado, carpeta.path(), &rama.name).unwrap();
            for (archivo, comentario) in [
                ("uno.txt", "Primer check-in de prueba"),
                (
                    "dos.txt",
                    "Segundo check-in\n\nCon una descripción\nde dos líneas",
                ),
            ] {
                std::fs::write(carpeta.path().join(archivo), archivo).unwrap();
                Comando::cm()
                    .arg("add")
                    .arg(archivo)
                    .en(carpeta.path())
                    .exigir_exito(&estado)
                    .unwrap();
                Comando::cm()
                    .args(["checkin", "-c"])
                    .arg(comentario)
                    .en(carpeta.path())
                    .exigir_exito(&estado)
                    .unwrap();
            }

            let leidos = leer_checkins_rama(&cliente, c, &repo, &id, &rama.name).unwrap();
            for l in &leidos {
                println!(
                    "cs {} | {} | {:?} | {}",
                    l.hash, l.mensaje, l.descripcion, l.fecha
                );
            }
            assert_eq!(leidos.len(), 2);
            assert_eq!(leidos[0].mensaje, "Primer check-in de prueba");
            assert_eq!(leidos[0].descripcion, None);
            assert_eq!(leidos[1].mensaje, "Segundo check-in");
            assert_eq!(
                leidos[1].descripcion.as_deref(),
                Some("Con una descripción\nde dos líneas")
            );
            assert!(
                leidos.iter().all(|l| l.autor == email),
                "el autor es quien hizo el check-in"
            );
            assert!(
                leidos[0].hash.parse::<i64>().unwrap() < leidos[1].hash.parse::<i64>().unwrap()
            );

            // Leer otra vez no duplica.
            assert_eq!(
                leer_checkins_rama(&cliente, c, &repo, &id, &rama.name)
                    .unwrap()
                    .len(),
                2
            );
            // Rama que no existe: lista vacía, sin error.
            assert!(cliente
                .changesets_de_rama(&repo, "/main/dev/no-existe-84")
                .unwrap()
                .is_none());
        }));

        // Limpieza: workspace y rama; `--delete-changesets` porque ya tiene check-ins.
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado);
        if let Some(rama) = rama_creada {
            let salida = Comando::cm()
                .args(["branch", "delete"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .arg("--delete-changesets")
                .ejecutar(&estado)
                .unwrap();
            println!(
                "borrada {rama}: {} {}",
                salida.exito(),
                salida.stderr.trim()
            );
        }
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
