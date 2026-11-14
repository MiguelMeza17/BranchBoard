//! Ramas de tareas y sus protecciones (issue #83). Ver "Protección de ramas" en BACKEND.md.
//!
//! - Nombre: `<id>-<título en minúsculas con guiones>`, ej. `T-101-combate-jugador`.
//!   En Unity VC la `/` separa niveles, así que el `task/<id>-<nombre>` del diseño
//!   original no se puede crear (probado 2026-11-14: `BR_NOT_FOUND`).
//! - Tarea general: hija de `/main/dev`, desde su cabeza **actual**. Subtarea:
//!   hija de la rama de su tarea padre, también desde su cabeza actual.
//! - Máximo 5 niveles: `main → dev → tarea → subtarea → subtarea de subtarea`.
//!   En Unity VC la ruta completa ya es la cadena de ancestros, así que los
//!   niveles se cuentan con los segmentos de la ruta, sin pedir nada al servidor.
//! - Nunca check-in a `/main` ni a `/main/dev`, para ningún rol.
//! - Crear la rama va por la API REST (no necesita workspace); cambiar de rama
//!   (`cm switch`) va por `cm`, en el workspace de quien trabaja.

use std::path::Path;

use crate::cm::lectura::{rama_actual_workspace, EstadoWorkspace};
use crate::cm::Comando;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::servidor::{ClienteServidor, Rama};

pub const RAMA_MAIN: &str = "/main";
pub const RAMA_DEV: &str = "/main/dev";
/// Cadena máxima: `main → dev → tarea → subtarea → subtarea de subtarea`.
pub const MAX_NIVELES: usize = 5;
/// Largo máximo de la parte del título en el nombre de la rama.
const MAX_LARGO_TITULO: usize = 30;

/// Niveles de una ruta de rama: `/main` = 1, `/main/dev` = 2, …
pub fn niveles(ruta: &str) -> usize {
    ruta.split('/').filter(|s| !s.is_empty()).count()
}

/// `T-101` + `Sistema de combate` → `T-101-sistema-de-combate`.
pub fn nombre_rama(id: &str, titulo: &str) -> Resultado<String> {
    let id = id.trim();
    if !id_valido(id) {
        return Err(Error::Validacion(format!(
            "El identificador de la tarea no es válido: {id}"
        )));
    }
    let parte = titulo_para_rama(titulo);
    Ok(if parte.is_empty() {
        id.to_string()
    } else {
        format!("{id}-{parte}")
    })
}

/// `T-101`, `T-101-1`, `T-101-1-2`…
fn id_valido(id: &str) -> bool {
    let mut partes = id.split('-');
    partes.next() == Some("T") && {
        let numeros: Vec<&str> = partes.collect();
        !numeros.is_empty()
            && numeros
                .iter()
                .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    }
}

/// Minúsculas, sin tildes, solo `a-z0-9` y guiones, sin cortar una palabra a la mitad.
fn titulo_para_rama(titulo: &str) -> String {
    let mut texto = String::new();
    for c in titulo.chars().flat_map(char::to_lowercase) {
        let c = sin_tilde(c);
        if c.is_ascii_alphanumeric() {
            texto.push(c);
        } else if !texto.is_empty() && !texto.ends_with('-') {
            texto.push('-');
        }
    }
    let texto = texto.trim_end_matches('-');
    if texto.len() <= MAX_LARGO_TITULO {
        return texto.to_string();
    }
    let corto = &texto[..MAX_LARGO_TITULO];
    match corto.rfind('-') {
        Some(i) if i > 0 => corto[..i].to_string(),
        _ => corto.to_string(),
    }
}

fn sin_tilde(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        otro => otro,
    }
}

/// Ruta completa de la rama nueva. Tarea general (`rama_padre = None`): hija de
/// `/main/dev`. Subtarea: hija de la rama de su padre, que tiene que ser una rama
/// de tarea. Se rechaza si pasaría de [`MAX_NIVELES`], antes de tocar el servidor.
pub fn ruta_rama_nueva(rama_padre: Option<&str>, nombre: &str) -> Resultado<String> {
    let padre = match rama_padre {
        None => RAMA_DEV.to_string(),
        Some(p) => {
            let p = p.trim().trim_end_matches('/');
            if !p.starts_with(&format!("{RAMA_DEV}/")) {
                return Err(Error::Validacion(format!(
                    "La rama de la tarea padre ({p}) no es una rama de tarea"
                )));
            }
            p.to_string()
        }
    };
    let ruta = format!("{padre}/{nombre}");
    if niveles(&ruta) > MAX_NIVELES {
        return Err(Error::Validacion(format!(
            "No se puede crear la rama: pasaría de {MAX_NIVELES} niveles (main → dev → tarea → subtarea → subtarea de subtarea)"
        )));
    }
    Ok(ruta)
}

/// Nunca check-in a `main` ni a `dev`, para ningún rol, ni el Administrador.
/// Todo lo que corra `cm checkin` llama a esto antes.
pub fn validar_destino_checkin(rama: &str) -> Resultado<()> {
    let rama = rama.trim().trim_end_matches('/');
    if rama.eq_ignore_ascii_case(RAMA_MAIN) || rama.eq_ignore_ascii_case(RAMA_DEV) {
        return Err(Error::Validacion(format!(
            "No se puede hacer check-in directo a {rama}: los cambios entran a dev solo por el merge de una tarea revisada"
        )));
    }
    Ok(())
}

/// Crea la rama de la tarea `id` en el servidor, desde la cabeza **actual** de su
/// padre (`/main/dev` o la rama de la tarea padre). Devuelve la rama creada; su
/// `name` es lo que se guarda en `rama_asociada`.
pub fn crear_rama_tarea(
    cliente: &ClienteServidor,
    repo: &str,
    rama_padre: Option<&str>,
    id: &str,
    titulo: &str,
) -> Resultado<Rama> {
    let ruta = ruta_rama_nueva(rama_padre, &nombre_rama(id, titulo)?)?;
    let ruta_padre = rama_padre.map_or(RAMA_DEV, |p| p.trim().trim_end_matches('/'));
    let padre = cliente
        .rama(repo, ruta_padre)?
        .ok_or_else(|| Error::NoEncontrado(format!("la rama {ruta_padre} en Unity VC")))?;
    if cliente.rama(repo, &ruta)?.is_some() {
        return Err(Error::Validacion(format!("La rama {ruta} ya existe")));
    }
    let comentario = format!("Rama de {}: {}", id.trim(), titulo.trim());
    cliente.crear_rama(repo, &ruta, padre.head_changeset, &comentario)
}

/// Una tarea de la cadena que va desde la tarea general hasta la que se toma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TareaDeCadena {
    pub id: String,
    pub titulo: String,
    /// `rama_asociada` actual; `None` si todavía no tiene rama.
    pub rama: Option<String>,
}

/// Una rama que hay que crear: para qué tarea, su ruta y la de su padre.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RamaPorCrear {
    id: String,
    titulo: String,
    ruta: String,
    ruta_padre: String,
}

/// Al tomar o asignar una subtarea, **sus ancestros sin rama reciben la suya**
/// (decidido 2026-11-14): la rama de una subtarea vive dentro de la de su padre,
/// así que si el padre todavía no tiene, se crea primero, desde la cabeza actual
/// de dev o de su propio padre.
///
/// `cadena` va de la tarea general a la tarea que se toma (inclusive). Crea, de
/// arriba abajo, las ramas que falten y devuelve `(id, rama)` de cada una creada
/// para que quien llama guarde `rama_asociada`. Si una ya existe en el servidor
/// con la ruta esperada (ej. un intento anterior que se cortó a la mitad), se
/// reusa en vez de fallar. El límite de niveles se revisa para toda la cadena
/// antes de crear nada.
pub fn crear_ramas_de_cadena(
    cliente: &ClienteServidor,
    repo: &str,
    cadena: &[TareaDeCadena],
) -> Resultado<Vec<(String, Rama)>> {
    let mut creadas = Vec::new();
    for paso in planear_cadena(cadena)? {
        let rama = match cliente.rama(repo, &paso.ruta)? {
            Some(existente) => existente,
            None => {
                let padre = cliente.rama(repo, &paso.ruta_padre)?.ok_or_else(|| {
                    Error::NoEncontrado(format!("la rama {} en Unity VC", paso.ruta_padre))
                })?;
                let comentario = format!("Rama de {}: {}", paso.id, paso.titulo.trim());
                cliente.crear_rama(repo, &paso.ruta, padre.head_changeset, &comentario)?
            }
        };
        creadas.push((paso.id, rama));
    }
    Ok(creadas)
}

/// Qué ramas faltan en la cadena y con qué ruta, sin tocar el servidor.
fn planear_cadena(cadena: &[TareaDeCadena]) -> Resultado<Vec<RamaPorCrear>> {
    if cadena.is_empty() {
        return Err(Error::Validacion("No hay tarea para crear su rama".into()));
    }
    let mut padre: Option<String> = None;
    let mut faltan = Vec::new();
    for tarea in cadena {
        let ruta = match &tarea.rama {
            Some(r) => r.trim().trim_end_matches('/').to_string(),
            None => {
                let ruta =
                    ruta_rama_nueva(padre.as_deref(), &nombre_rama(&tarea.id, &tarea.titulo)?)?;
                faltan.push(RamaPorCrear {
                    id: tarea.id.trim().to_string(),
                    titulo: tarea.titulo.clone(),
                    ruta: ruta.clone(),
                    ruta_padre: padre.clone().unwrap_or_else(|| RAMA_DEV.to_string()),
                });
                ruta
            }
        };
        padre = Some(ruta);
    }
    Ok(faltan)
}

/// `cm switch` del workspace de `carpeta` a `rama`, y comprueba que quedó ahí.
/// Con cambios sin check-in **no** se cambia: `cm switch` se queda esperando una
/// respuesta que la app no puede dar (probado 2026-11-14), así que se revisa antes
/// y se avisa. Los archivos privados (nunca agregados) no cuentan: no lo bloquean.
pub fn cambiar_a_rama(estado: &AppState, carpeta: &Path, rama: &str) -> Resultado<EstadoWorkspace> {
    let rama = rama.trim().trim_end_matches('/');
    if !rama.starts_with(RAMA_MAIN) {
        return Err(Error::Validacion(format!("Rama no válida: {rama}")));
    }
    let pendientes = cambios_pendientes(estado, carpeta)?;
    if !pendientes.is_empty() {
        return Err(Error::Validacion(format!(
            "Tienes {} cambio(s) sin check-in en esta rama. Haz check-in o deshazlos antes de cambiar de rama",
            pendientes.len()
        )));
    }
    Comando::cm()
        .arg("switch")
        .arg(format!("br:{rama}"))
        .en(carpeta)
        .exigir_exito(estado)?;
    let ahora = rama_actual_workspace(estado, carpeta)?;
    if ahora.rama != rama {
        return Err(Error::Cm(format!(
            "El workspace quedó en {} en vez de {rama}",
            ahora.rama
        )));
    }
    Ok(ahora)
}

/// Archivos con cambios sin check-in en el workspace (agregados, modificados,
/// borrados, movidos o desprotegidos), sin contar los privados.
pub fn cambios_pendientes(estado: &AppState, carpeta: &Path) -> Resultado<Vec<String>> {
    let texto = Comando::cm()
        .args([
            "status",
            "--changed",
            "--added",
            "--deleted",
            "--moved",
            "--checkout",
            "--short",
        ])
        .en(carpeta)
        .exigir_exito(estado)?;
    Ok(interpretar_pendientes(&texto))
}

fn interpretar_pendientes(texto: &str) -> Vec<String> {
    texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_de_la_rama() {
        assert_eq!(
            nombre_rama("T-101", "Sistema de combate del jugador").unwrap(),
            "T-101-sistema-de-combate-del-jugador"
        );
        assert_eq!(
            nombre_rama("T-101-1", "Modelo 3D: arma").unwrap(),
            "T-101-1-modelo-3d-arma"
        );
        assert_eq!(
            nombre_rama("T-7", "Animación ¡Súper!  rápida…").unwrap(),
            "T-7-animacion-super-rapida"
        );
        assert_eq!(nombre_rama("T-7", "¿?").unwrap(), "T-7");
        assert_eq!(
            nombre_rama("T-7", "Menú; rm -rf / && echo").unwrap(),
            "T-7-menu-rm-rf-echo",
            "nada de la shell sobrevive en el nombre"
        );
    }

    #[test]
    fn el_titulo_largo_se_corta_entre_palabras() {
        let n = nombre_rama("T-1", "una dos tres cuatro cinco seis siete ocho").unwrap();
        assert_eq!(n, "T-1-una-dos-tres-cuatro-cinco");
        let sin_espacios = nombre_rama("T-1", &"a".repeat(50)).unwrap();
        assert_eq!(sin_espacios.len(), "T-1-".len() + MAX_LARGO_TITULO);
    }

    #[test]
    fn id_invalido() {
        for id in ["", "101", "T-", "T-1a", "X-1", "T-1/2", "T--1", "/main"] {
            assert_eq!(
                nombre_rama(id, "x").unwrap_err().codigo(),
                "validacion",
                "{id}"
            );
        }
    }

    #[test]
    fn niveles_de_una_ruta() {
        assert_eq!(niveles("/main"), 1);
        assert_eq!(niveles("/main/dev"), 2);
        assert_eq!(niveles("/main/dev/T-1-a/T-1-1-b/"), 4);
    }

    #[test]
    fn tarea_general_cuelga_de_dev_y_subtarea_de_su_padre() {
        assert_eq!(ruta_rama_nueva(None, "T-1-a").unwrap(), "/main/dev/T-1-a");
        assert_eq!(
            ruta_rama_nueva(Some("/main/dev/T-1-a/"), "T-1-1-b").unwrap(),
            "/main/dev/T-1-a/T-1-1-b"
        );
        assert_eq!(
            ruta_rama_nueva(Some("/main/dev/T-1-a/T-1-1-b"), "T-1-1-1-c").unwrap(),
            "/main/dev/T-1-a/T-1-1-b/T-1-1-1-c"
        );
    }

    #[test]
    fn no_pasa_de_cinco_niveles() {
        let error =
            ruta_rama_nueva(Some("/main/dev/T-1-a/T-1-1-b/T-1-1-1-c"), "T-1-1-1-1-d").unwrap_err();
        assert_eq!(error.codigo(), "validacion");
        assert!(error.to_string().contains("5 niveles"), "{error}");
    }

    fn tarea(id: &str, titulo: &str, rama: Option<&str>) -> TareaDeCadena {
        TareaDeCadena {
            id: id.into(),
            titulo: titulo.into(),
            rama: rama.map(String::from),
        }
    }

    #[test]
    fn la_cadena_crea_primero_la_rama_del_padre_que_falta() {
        // T-102 nunca se tomó; al tomar T-102-1 se crean las dos, de arriba abajo.
        let plan = planear_cadena(&[
            tarea("T-102", "IA de enemigo rastreador", None),
            tarea("T-102-1", "Animación de caminar", None),
        ])
        .unwrap();
        let rutas: Vec<(&str, &str)> = plan
            .iter()
            .map(|p| (p.ruta.as_str(), p.ruta_padre.as_str()))
            .collect();
        assert_eq!(
            rutas,
            [
                ("/main/dev/T-102-ia-de-enemigo-rastreador", "/main/dev"),
                (
                    "/main/dev/T-102-ia-de-enemigo-rastreador/T-102-1-animacion-de-caminar",
                    "/main/dev/T-102-ia-de-enemigo-rastreador"
                ),
            ]
        );
    }

    #[test]
    fn la_cadena_respeta_las_ramas_que_ya_existen() {
        let plan = planear_cadena(&[
            tarea("T-1", "a", Some("/main/dev/T-1-a")),
            tarea("T-1-1", "b", None),
            tarea("T-1-1-1", "c", None),
        ])
        .unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].ruta_padre, "/main/dev/T-1-a");
        assert_eq!(plan[1].ruta, "/main/dev/T-1-a/T-1-1-b/T-1-1-1-c");
        // Todo con rama: no hay nada que crear.
        assert!(
            planear_cadena(&[tarea("T-1", "a", Some("/main/dev/T-1-a"))])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn la_cadena_revisa_el_limite_antes_de_crear_nada() {
        let e = planear_cadena(&[
            tarea("T-1", "a", None),
            tarea("T-1-1", "b", None),
            tarea("T-1-1-1", "c", None),
            tarea("T-1-1-1-1", "d", None),
        ])
        .unwrap_err();
        assert!(e.to_string().contains("5 niveles"), "{e}");
        assert_eq!(planear_cadena(&[]).unwrap_err().codigo(), "validacion");
    }

    #[test]
    fn la_padre_tiene_que_ser_rama_de_tarea() {
        for padre in ["/main", "/main/dev", "/main/devx/T-1", "/otra/T-1", ""] {
            assert_eq!(
                ruta_rama_nueva(Some(padre), "T-1-1").unwrap_err().codigo(),
                "validacion",
                "{padre}"
            );
        }
    }

    #[test]
    fn nunca_checkin_a_main_ni_a_dev() {
        for rama in ["/main", "/main/", "/main/dev", "/MAIN/DEV", " /main/dev/ "] {
            let e = validar_destino_checkin(rama).unwrap_err();
            assert_eq!(e.codigo(), "validacion", "{rama}");
        }
        assert!(validar_destino_checkin("/main/dev/T-1-a").is_ok());
        assert!(validar_destino_checkin("/main/dev/T-1-a/T-1-1-b").is_ok());
    }

    #[test]
    fn lista_de_pendientes() {
        let texto = "c:/ws/LEEME.txt\r\n\r\n  c:/ws/Assets/a.cs\n";
        assert_eq!(
            interpretar_pendientes(texto),
            ["c:/ws/LEEME.txt", "c:/ws/Assets/a.cs"]
        );
        assert!(interpretar_pendientes("").is_empty());
    }

    #[test]
    fn cambiar_a_una_rama_rara_se_rechaza_sin_llamar_a_cm() {
        let estado = AppState::default();
        let e = cambiar_a_rama(&estado, Path::new("."), "dev; rm -rf x").unwrap_err();
        assert_eq!(e.codigo(), "validacion");
    }

    /// **Prueba en el entorno real** (CLAUDE.md), contra el repo de pruebas:
    /// crea una tarea general desde la cabeza de dev, su subtarea y una subtarea de
    /// subtarea; comprueba que la 6.ª no se crea; cambia un workspace temporal a
    /// la rama nueva con `cm switch`; y al final borra todo lo que creó.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_ramas -- --ignored --nocapture --test-threads=1
    /// ```
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_ramas_de_tareas() {
        use crate::servidor::{conectar, LlaveroSistema};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let repo_cm = format!("{repo}@{org}@unity");
        let estado = AppState::default();
        let email = crate::cm::lectura::whoami(&estado).expect("sin sesión de cm");
        let cliente = conectar(&estado, &LlaveroSistema, &email, &org).unwrap();
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let id = format!("T-9{sufijo}");
        let mut creadas: Vec<String> = Vec::new();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Tarea general: desde la cabeza actual de dev.
            let dev = cliente
                .rama(&repo, RAMA_DEV)
                .unwrap()
                .expect("no hay /main/dev");
            let general =
                crear_rama_tarea(&cliente, &repo, None, &id, "Prueba #83 combate").unwrap();
            creadas.push(general.name.clone());
            println!(
                "creada {} (cs {}, dev en cs {})",
                general.name, general.head_changeset, dev.head_changeset
            );
            assert_eq!(general.name, format!("/main/dev/{id}-prueba-83-combate"));
            assert_eq!(
                general.head_changeset, dev.head_changeset,
                "nace de la cabeza de dev"
            );

            // Subtarea y subtarea de subtarea: hijas de su padre (niveles 4 y 5).
            let sub = crear_rama_tarea(
                &cliente,
                &repo,
                Some(&general.name),
                &format!("{id}-1"),
                "Arma",
            )
            .unwrap();
            creadas.push(sub.name.clone());
            let subsub = crear_rama_tarea(
                &cliente,
                &repo,
                Some(&sub.name),
                &format!("{id}-1-1"),
                "Mango",
            )
            .unwrap();
            creadas.push(subsub.name.clone());
            println!("creadas {} y {}", sub.name, subsub.name);
            assert_eq!(niveles(&subsub.name), 5);

            // La 6.ª se rechaza antes de llamar al servidor.
            let e = crear_rama_tarea(
                &cliente,
                &repo,
                Some(&subsub.name),
                &format!("{id}-1-1-1"),
                "Nivel 6",
            )
            .unwrap_err();
            assert_eq!(e.codigo(), "validacion");
            assert!(cliente
                .rama(&repo, &format!("{}/{id}-1-1-1-nivel-6", subsub.name))
                .unwrap()
                .is_none());

            // Repetir la misma tarea: ya existe.
            let e = crear_rama_tarea(&cliente, &repo, None, &id, "Prueba #83 combate").unwrap_err();
            assert!(e.to_string().contains("ya existe"), "{e}");

            // Subtarea cuyo padre nunca se tomó: se crean las dos, el padre primero.
            let padre_id = format!("T-8{sufijo}");
            let cadena = vec![
                TareaDeCadena {
                    id: padre_id.clone(),
                    titulo: "Padre sin rama".into(),
                    rama: None,
                },
                TareaDeCadena {
                    id: format!("{padre_id}-1"),
                    titulo: "Hija".into(),
                    rama: None,
                },
            ];
            let hechas = crear_ramas_de_cadena(&cliente, &repo, &cadena).unwrap();
            creadas.extend(hechas.iter().map(|(_, r)| r.name.clone()));
            let nombres: Vec<&str> = hechas.iter().map(|(_, r)| r.name.as_str()).collect();
            println!("cadena creada: {nombres:?}");
            assert_eq!(
                nombres,
                [
                    format!("/main/dev/{padre_id}-padre-sin-rama"),
                    format!("/main/dev/{padre_id}-padre-sin-rama/{padre_id}-1-hija"),
                ]
            );
            assert_eq!(
                hechas[0].1.head_changeset, dev.head_changeset,
                "el padre nace de la cabeza de dev"
            );

            // Si se cortó a la mitad (el servidor ya las tiene, la BD no): se reusan.
            let otra_vez = crear_ramas_de_cadena(&cliente, &repo, &cadena).unwrap();
            assert_eq!(
                otra_vez.iter().map(|(_, r)| r.id).collect::<Vec<_>>(),
                hechas.iter().map(|(_, r)| r.id).collect::<Vec<_>>(),
                "reusó las mismas ramas"
            );
            // Con las ramas ya guardadas, no hay nada que crear.
            let guardadas: Vec<TareaDeCadena> = cadena
                .iter()
                .zip(&hechas)
                .map(|(t, (_, r))| TareaDeCadena {
                    rama: Some(r.name.clone()),
                    ..t.clone()
                })
                .collect();
            assert!(crear_ramas_de_cadena(&cliente, &repo, &guardadas)
                .unwrap()
                .is_empty());

            // cm switch en un workspace temporal.
            let carpeta = tempfile::tempdir().unwrap();
            let ws = format!("bb-prueba-83-{sufijo}");
            Comando::cm()
                .args(["workspace", "create"])
                .arg(&ws)
                .arg(carpeta.path().to_string_lossy().to_string())
                .arg(format!("rep:{repo_cm}"))
                .exigir_exito(&estado)
                .unwrap();
            let cambio = cambiar_a_rama(&estado, carpeta.path(), &sub.name);

            // Con un cambio sin check-in no se cambia, y no se queda esperando.
            let archivo = carpeta.path().join("pendiente-83.txt");
            std::fs::write(&archivo, "x").unwrap();
            Comando::cm()
                .arg("add")
                .arg(archivo.to_string_lossy().to_string())
                .en(carpeta.path())
                .exigir_exito(&estado)
                .unwrap();
            let inicio = std::time::Instant::now();
            let bloqueado = cambiar_a_rama(&estado, carpeta.path(), &general.name);
            let segundos = inicio.elapsed().as_secs();
            let sigue_en = rama_actual_workspace(&estado, carpeta.path()).map(|e| e.rama);
            let _ = Comando::cm()
                .args(["undo", "-r", "."])
                .en(carpeta.path())
                .ejecutar(&estado);

            let _ = Comando::cm()
                .args(["workspace", "delete"])
                .arg(carpeta.path().to_string_lossy().to_string())
                .ejecutar(&estado);
            let cambio = cambio.unwrap();
            println!("workspace cambiado a {}", cambio.rama);
            assert_eq!(cambio.rama, sub.name);
            let e = bloqueado.unwrap_err();
            println!("con cambio pendiente: {e} ({segundos} s)");
            assert_eq!(e.codigo(), "validacion");
            assert!(segundos < 30, "no se quedó esperando");
            assert_eq!(sigue_en.unwrap(), sub.name, "el workspace no se movió");
        }));

        // Limpieza: de la más profunda a la general.
        for rama in creadas.iter().rev() {
            let salida = Comando::cm()
                .args(["branch", "delete"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .ejecutar(&estado)
                .unwrap();
            println!("borrada {rama}: {}", salida.exito());
        }
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
