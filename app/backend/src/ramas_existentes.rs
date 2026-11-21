//! Ramas hijas de `/main/dev` creadas fuera de BranchBoard (issue #283).
//!
//! Un repositorio que ya tenía trabajo antes de BranchBoard (o ramas hechas a mano
//! desde Plastic) quedaba con el tablero vacío. El polling lee todas las ramas del
//! repositorio con `cm find branch` y, a las que están bajo `/main/dev` y ninguna
//! tarea usa, les crea una tarea con `rama_asociada` apuntando a esa rama (no se
//! renombra nada en Unity VC):
//!
//! - Hija directa de `/main/dev` → tarea general; sus hijas → subtareas, hasta
//!   [`MAX_NIVELES`].
//! - **En producción**; si la rama ya se unió (merge) a su rama padre, o su tarea
//!   padre entra terminada, **Terminada**.
//! - Asignada al dueño de la rama si es del equipo; si no, sin asignar.
//! - Título: el nombre de la rama legible (`CorreccionesEtapasBasicas1-6` →
//!   `Correcciones Etapas Basicas 1-6`).
//!
//! Una rama se importa la **segunda** vez que el polling la ve sin tarea: así no se
//! adelanta a "tomar tarea", que crea la rama antes de guardar `rama_asociada`.
//! Las ramas que no se pueden importar (más niveles de los permitidos, o cuya rama
//! padre no tiene tarea) se quedan como están y la alerta de rama ajena sigue.
//!
//! Viaja como el evento `importar_rama`. Si dos máquinas importan la misma rama a la
//! vez, queda en todas la importación con `detectado` más viejo (a igual hora, el id
//! menor), como la tarea roja de #225. Una tarea normal con esa rama gana siempre.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::cm::Comando;
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos::{self, Evento};
use crate::modelo::{ahora_iso, hoy_local, normalizar_email, EstadoTarea, TipoTarea};
use crate::ramas::{niveles, MAX_NIVELES, RAMA_DEV};

pub const TIPO: &str = "importar_rama";
pub const TIPOS: &[&str] = &[TIPO];

/// Una rama del repositorio, como la da `cm find branch --format={name}|{owner}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamaRemota {
    pub ruta: String,
    pub dueno: String,
}

/// `payload` de `importar_rama`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadImportar {
    pub tarea: Tarea,
    pub detectado: String,
}

fn limpia(ruta: &str) -> String {
    ruta.trim()
        .trim_start_matches("br:")
        .trim_end_matches('/')
        .to_string()
}

fn clave(ruta: &str) -> String {
    limpia(ruta).to_lowercase()
}

/// Ruta de la rama padre: `/main/dev/A/B` → `/main/dev/A`.
fn padre_de(ruta: &str) -> String {
    let ruta = limpia(ruta);
    match ruta.rfind('/') {
        Some(i) if i > 0 => ruta[..i].to_string(),
        _ => String::new(),
    }
}

/// Salida de `cm find branch "on repository '…'" --format={name}|{owner} --nototal`.
pub fn interpretar_ramas(salida: &str) -> Vec<RamaRemota> {
    salida
        .lines()
        .filter_map(|linea| {
            let (ruta, dueno) = linea.split_once('|')?;
            let ruta = limpia(ruta);
            (!ruta.is_empty()).then(|| RamaRemota {
                ruta,
                dueno: normalizar_email(dueno),
            })
        })
        .collect()
}

/// Salida de `cm find merge "on repository '…'" --format={type}|{srcbranch}|{dstbranch} --nototal`:
/// los pares (origen, destino) de los merges de verdad (sin cherrypicks), en minúsculas.
pub fn interpretar_merges(salida: &str) -> BTreeSet<(String, String)> {
    salida
        .lines()
        .filter_map(|linea| {
            let mut partes = linea.split('|');
            let tipo = partes.next()?.trim();
            let origen = partes.next()?;
            let destino = partes.next()?;
            tipo.eq_ignore_ascii_case("merge")
                .then(|| (clave(origen), clave(destino)))
        })
        .collect()
}

/// `CorreccionesEtapasBasicas1-6` → `Correcciones Etapas Basicas 1-6`;
/// `menu_principal` → `Menu principal`.
pub fn titulo_legible(ruta: &str) -> String {
    let ruta = limpia(ruta);
    let nombre = ruta.rsplit('/').next().unwrap_or_default();
    let mut texto = String::new();
    let mut anterior: Option<char> = None;
    for c in nombre.chars() {
        let c = if c == '_' { ' ' } else { c };
        if let Some(a) = anterior {
            let corte = (a.is_lowercase() && c.is_uppercase())
                || (a.is_alphabetic() && c.is_ascii_digit())
                || (a.is_ascii_digit() && c.is_alphabetic());
            if corte && a != ' ' {
                texto.push(' ');
            }
        }
        if !(c == ' ' && texto.ends_with(' ')) {
            texto.push(c);
        }
        anterior = Some(c);
    }
    let texto = texto.trim();
    let mut letras = texto.chars();
    match letras.next() {
        Some(primera) => primera.to_uppercase().chain(letras).collect(),
        None => ruta,
    }
}

/// Ramas que hay que importar, padres antes que hijas. `con_tarea` son las ramas
/// (en minúsculas) que ya usa alguna tarea, incluida la Papelera.
pub fn candidatas(ramas: &[RamaRemota], con_tarea: &BTreeSet<String>) -> Vec<RamaRemota> {
    let prefijo = format!("{}/", RAMA_DEV.to_lowercase());
    let mut lista: Vec<RamaRemota> = ramas
        .iter()
        .filter(|r| {
            let k = clave(&r.ruta);
            k.starts_with(&prefijo) && niveles(&r.ruta) <= MAX_NIVELES && !con_tarea.contains(&k)
        })
        .cloned()
        .collect();
    // Solo si su rama padre es dev, ya tiene tarea o también se importa.
    let importables: BTreeSet<String> = lista.iter().map(|r| clave(&r.ruta)).collect();
    let dev = RAMA_DEV.to_lowercase();
    lista.retain(|r| {
        let mut padre = clave(&padre_de(&r.ruta));
        loop {
            if padre == dev || con_tarea.contains(&padre) {
                return true;
            }
            if !importables.contains(&padre) {
                return false;
            }
            padre = clave(&padre_de(&padre));
        }
    });
    lista.sort_by(|a, b| {
        niveles(&a.ruta)
            .cmp(&niveles(&b.ruta))
            .then_with(|| clave(&a.ruta).cmp(&clave(&b.ruta)))
    });
    lista
}

/// Ramas que ya vio el polling sin tarea, por repositorio: se importan la vez siguiente.
fn vistas() -> &'static Mutex<BTreeMap<String, BTreeSet<String>>> {
    static VISTAS: OnceLock<Mutex<BTreeMap<String, BTreeSet<String>>>> = OnceLock::new();
    VISTAS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Deja en `lista` solo las ramas que ya estaban vistas para `repo` y recuerda las de ahora.
fn segunda_vez(repo: &str, lista: Vec<RamaRemota>) -> Vec<RamaRemota> {
    let mut vistas = vistas().lock().unwrap_or_else(|e| e.into_inner());
    let antes = vistas.remove(repo).unwrap_or_default();
    vistas.insert(
        repo.to_string(),
        lista.iter().map(|r| clave(&r.ruta)).collect(),
    );
    lista
        .into_iter()
        .filter(|r| antes.contains(&clave(&r.ruta)))
        .collect()
}

fn ramas_con_tarea(c: &Connection) -> Resultado<BTreeSet<String>> {
    Ok(db_tareas::listar(c)?
        .into_iter()
        .filter_map(|t| t.rama_asociada.map(|r| clave(&r)))
        .collect())
}

fn tarea_de_rama(c: &Connection, ruta: &str) -> Resultado<Option<Tarea>> {
    let k = clave(ruta);
    Ok(db_tareas::listar(c)?
        .into_iter()
        .find(|t| t.rama_asociada.as_deref().map(clave).as_deref() == Some(k.as_str())))
}

fn importacion_guardada(c: &Connection, ruta: &str) -> Resultado<Option<(String, String)>> {
    Ok(c.query_row(
        "SELECT tarea_id, detectado FROM ramas_importadas WHERE rama = ?1",
        [clave(ruta)],
        |f| Ok((f.get(0)?, f.get(1)?)),
    )
    .optional()?)
}

/// Crea y publica la tarea de `rama`. Devuelve su id, o `None` si ya no hacía falta.
fn importar(
    c: &Connection,
    detector: &str,
    rama: &RamaRemota,
    cerrada: bool,
    detectado: &str,
    hoy: &str,
) -> Resultado<Option<String>> {
    if tarea_de_rama(c, &rama.ruta)?.is_some() {
        return Ok(None);
    }
    let padre_ruta = padre_de(&rama.ruta);
    let padre = if padre_ruta.eq_ignore_ascii_case(RAMA_DEV) {
        None
    } else {
        match tarea_de_rama(c, &padre_ruta)? {
            Some(p) => Some(p),
            None => return Ok(None),
        }
    };
    let id = db_tareas::siguiente_id(c, padre.as_ref().map(|p| p.id.as_str()))?;
    let mut tarea = Tarea::nueva(
        &id,
        &titulo_legible(&rama.ruta),
        TipoTarea::Scripting,
        detector,
        hoy,
    );
    tarea.detalles = format!(
        "Creada automáticamente desde la rama {} de Unity VC.",
        rama.ruta
    );
    tarea.tarea_padre_id = padre.as_ref().map(|p| p.id.clone());
    tarea.rama_asociada = Some(rama.ruta.clone());
    tarea.modo_trabajo = Some("rama".into());
    let padre_terminada = padre
        .as_ref()
        .is_some_and(|p| p.estado == EstadoTarea::Terminada);
    if cerrada || padre_terminada {
        tarea.estado = EstadoTarea::Terminada;
        tarea.fecha_completada = Some(hoy.to_string());
    } else {
        tarea.estado = EstadoTarea::EnProduccion;
    }
    if !rama.dueno.is_empty() && usuarios::buscar(c, &rama.dueno)?.is_some() {
        tarea.asignado_a = Some(rama.dueno.clone());
    }
    let payload = PayloadImportar {
        tarea,
        detectado: detectado.to_string(),
    };
    let mut ev = Evento::nuevo(
        TIPO,
        Some(&id),
        detector,
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar {TIPO}: {e}")))?,
    );
    ev.timestamp = detectado.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_importar(tx, &payload, detector))?;
    Ok(Some(id))
}

/// Aplica `importar_rama` (local o recibido). Ver el desempate en el comentario del módulo.
pub fn aplicar_importar(c: &Connection, payload: &PayloadImportar, usuario: &str) -> Resultado<()> {
    let tarea = &payload.tarea;
    let rama = tarea
        .rama_asociada
        .as_deref()
        .map(limpia)
        .ok_or_else(|| Error::Validacion(format!("{TIPO} sin rama")))?;
    if !clave(&rama).starts_with(&format!("{}/", RAMA_DEV.to_lowercase()))
        || niveles(&rama) > MAX_NIVELES
    {
        return Err(Error::Validacion(format!(
            "{rama} no es una rama de tarea importable"
        )));
    }
    let padre_ruta = padre_de(&rama);
    match tarea.tarea_padre_id.as_deref() {
        None if padre_ruta.eq_ignore_ascii_case(RAMA_DEV) => {}
        Some(pid) => {
            let padre = db_tareas::obtener(c, pid)?;
            if padre.rama_asociada.as_deref().map(clave) != Some(clave(&padre_ruta)) {
                return Err(Error::Validacion(format!(
                    "La tarea {pid} no es la de la rama {padre_ruta}"
                )));
            }
        }
        None => {
            return Err(Error::Validacion(format!(
                "{rama} no es hija directa de {RAMA_DEV}"
            )))
        }
    }

    if let Some(existente) = tarea_de_rama(c, &rama)? {
        if existente.id == tarea.id {
            return Ok(()); // ya aplicado
        }
        let Some((id_guardado, detectado)) = importacion_guardada(c, &rama)? else {
            return Ok(()); // una tarea normal con esa rama gana siempre
        };
        let gana_la_otra = id_guardado != existente.id
            || (detectado.as_str(), existente.id.as_str())
                <= (payload.detectado.as_str(), tarea.id.as_str());
        if gana_la_otra {
            return Ok(());
        }
        db_tareas::borrar_fila(c, &existente.id)?;
    }

    let mut nueva = tarea.clone();
    nueva.creado_por = normalizar_email(usuario);
    nueva.rama_asociada = Some(rama.clone());
    nueva.cambios_directos = None;
    db_tareas::insertar(c, &nueva)?;
    c.execute(
        "INSERT INTO ramas_importadas (rama, tarea_id, detectado) VALUES (?1, ?2, ?3)
         ON CONFLICT(rama) DO UPDATE SET tarea_id = excluded.tarea_id, detectado = excluded.detectado",
        [clave(&rama), nueva.id.clone(), payload.detectado.clone()],
    )?;
    Ok(())
}

/// Aplica un evento de este módulo recibido del servidor.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    let payload: PayloadImportar = serde_json::from_value(evento.payload.clone())
        .map_err(|e| Error::Validacion(format!("{TIPO} inválido: {e}")))?;
    aplicar_importar(c, &payload, &evento.usuario)
}

/// Lee las ramas y los merges del repositorio abierto y crea las tareas que falten
/// (lo llama el polling). Devuelve los ids de las tareas creadas.
pub fn revisar_en(estado: &AppState, email: &str) -> Resultado<Vec<String>> {
    let Ok(repo) = estado.repo_abierto() else {
        return Ok(Vec::new());
    };
    let repo_cm = format!("{}@{}@unity", repo.repo, repo.org);
    let salida = Comando::cm()
        .args(["find", "branch"])
        .arg(format!("on repository '{repo_cm}'"))
        .args(["--format={name}|{owner}", "--nototal"])
        .exigir_exito(estado)?;
    let ramas = interpretar_ramas(&salida);
    let con_tarea = estado.con_db(|db| ramas_con_tarea(db.conexion()))?;
    let lista = segunda_vez(&repo_cm, candidatas(&ramas, &con_tarea));
    if lista.is_empty() {
        return Ok(Vec::new());
    }
    let merges = interpretar_merges(
        &Comando::cm()
            .args(["find", "merge"])
            .arg(format!("on repository '{repo_cm}'"))
            .args(["--format={type}|{srcbranch}|{dstbranch}", "--nototal"])
            .exigir_exito(estado)?,
    );
    let detector = normalizar_email(email);
    let detectado = ahora_iso();
    let hoy = hoy_local();
    estado.con_db(|db| {
        let mut creadas = Vec::new();
        for rama in &lista {
            let cerrada = merges.contains(&(clave(&rama.ruta), clave(&padre_de(&rama.ruta))));
            if let Some(id) = importar(db.conexion(), &detector, rama, cerrada, &detectado, &hoy)? {
                creadas.push(id);
            }
        }
        Ok(creadas)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::modelo::Rol;

    const ANA: &str = "ana@virustf.dev";

    fn rama(ruta: &str, dueno: &str) -> RamaRemota {
        RamaRemota {
            ruta: ruta.into(),
            dueno: dueno.into(),
        }
    }

    fn base() -> Db {
        let db = Db::en_memoria().unwrap();
        usuarios::insertar(
            db.conexion(),
            &usuarios::Usuario {
                email: ANA.into(),
                nombre: "Ana".into(),
                apellido: "Prueba".into(),
                rol: Rol::Desarrollador,
                fecha_alta: "2026-11-21".into(),
            },
        )
        .unwrap();
        db
    }

    #[test]
    fn titulos_legibles() {
        assert_eq!(
            titulo_legible("/main/dev/CorreccionesEtapasBasicas1-6"),
            "Correcciones Etapas Basicas 1-6"
        );
        assert_eq!(titulo_legible("/main/dev/menu_principal"), "Menu principal");
        assert_eq!(titulo_legible("/main/dev/T-101"), "T-101");
        assert_eq!(titulo_legible("/main/dev/a/hija"), "Hija");
    }

    #[test]
    fn interpreta_la_salida_real_de_cm() {
        // Copiada de BranchBoard-Pruebas (2026-11-21).
        let ramas = interpretar_ramas(
            "/main|miguellmezza123@gmail.com\n/main/dev|miguellmezza123@gmail.com\n/main/dev/T-101|miguellmezza123@gmail.com\n",
        );
        assert_eq!(ramas.len(), 3);
        assert_eq!(
            ramas[2],
            rama("/main/dev/T-101", "miguellmezza123@gmail.com")
        );
        let merges = interpretar_merges(
            "merge|br:/main/dev/T-101|br:/main/dev\ncherrypick|br:/main/dev/T-102|br:/main/dev/T-103\n",
        );
        assert!(merges.contains(&("/main/dev/t-101".into(), "/main/dev".into())));
        assert_eq!(merges.len(), 1, "un préstamo no cuenta como merge");
    }

    #[test]
    fn candidatas_padres_primero_y_sin_las_que_no_se_pueden() {
        let ramas = vec![
            rama("/main", ANA),
            rama("/main/dev", ANA),
            rama("/main/dev/Grande/Mini", ANA),
            rama("/main/dev/Grande", ANA),
            rama("/main/dev/Ya/Hija", ANA),
            rama("/main/dev/a/b/c/d", ANA), // 6 niveles
            rama("/main/otra", ANA),
        ];
        let con_tarea: BTreeSet<String> = ["/main/dev/ya".to_string()].into();
        let lista: Vec<String> = candidatas(&ramas, &con_tarea)
            .into_iter()
            .map(|r| r.ruta)
            .collect();
        assert_eq!(
            lista,
            [
                "/main/dev/Grande",
                "/main/dev/Grande/Mini",
                "/main/dev/Ya/Hija"
            ]
        );
        // Sin su padre importable, la hija queda fuera.
        let sin_padre = candidatas(&[rama("/main/dev/x/y", ANA)], &BTreeSet::new());
        assert!(sin_padre.is_empty());
    }

    #[test]
    fn se_importa_la_segunda_vez_que_se_ve() {
        let lista = vec![rama("/main/dev/Nueva", ANA)];
        assert!(segunda_vez("prueba-283-a", lista.clone()).is_empty());
        assert_eq!(segunda_vez("prueba-283-a", lista).len(), 1);
        // Si deja de aparecer, se olvida.
        assert!(segunda_vez("prueba-283-a", Vec::new()).is_empty());
        assert!(segunda_vez("prueba-283-a", vec![rama("/main/dev/Nueva", ANA)]).is_empty());
    }

    #[test]
    fn importa_jerarquia_estado_y_responsable() {
        let db = base();
        let c = db.conexion();
        let grande = rama("/main/dev/Grande", ANA);
        let mini = rama("/main/dev/Grande/Mini", "ajeno@otro.dev");
        let vieja = rama("/main/dev/Vieja", ANA);
        let id_g = importar(c, ANA, &grande, false, "2026-11-21T10:00:00Z", "2026-11-21")
            .unwrap()
            .unwrap();
        let id_m = importar(c, ANA, &mini, false, "2026-11-21T10:00:00Z", "2026-11-21")
            .unwrap()
            .unwrap();
        let id_v = importar(c, ANA, &vieja, true, "2026-11-21T10:00:00Z", "2026-11-21")
            .unwrap()
            .unwrap();
        let g = db_tareas::obtener(c, &id_g).unwrap();
        assert_eq!(g.titulo, "Grande");
        assert_eq!(g.estado, EstadoTarea::EnProduccion);
        assert_eq!(g.asignado_a.as_deref(), Some(ANA));
        assert_eq!(g.tarea_padre_id, None);
        let m = db_tareas::obtener(c, &id_m).unwrap();
        assert_eq!(m.tarea_padre_id.as_deref(), Some(id_g.as_str()));
        assert_eq!(m.asignado_a, None, "el dueño no es del equipo");
        assert_eq!(m.rama_asociada.as_deref(), Some("/main/dev/Grande/Mini"));
        assert_eq!(
            db_tareas::obtener(c, &id_v).unwrap().estado,
            EstadoTarea::Terminada
        );
        // Repetir no duplica.
        assert_eq!(
            importar(c, ANA, &grande, false, "2026-11-21T10:05:00Z", "2026-11-21").unwrap(),
            None
        );
        assert!(ramas_con_tarea(c).unwrap().contains("/main/dev/grande"));
    }

    #[test]
    fn dos_maquinas_a_la_vez_dejan_la_mas_vieja_en_las_dos() {
        let (a, b) = (base(), base());
        let grande = rama("/main/dev/Grande", ANA);
        importar(
            a.conexion(),
            ANA,
            &grande,
            false,
            "2026-11-21T10:00:00Z",
            "2026-11-21",
        )
        .unwrap();
        // B la importó un segundo después con otro id (T-102), sin que ese id choque en A.
        let mut en_b = Tarea::nueva("T-102", "Grande", TipoTarea::Scripting, ANA, "2026-11-21");
        en_b.rama_asociada = Some("/main/dev/Grande".into());
        en_b.estado = EstadoTarea::EnProduccion;
        aplicar_importar(
            b.conexion(),
            &PayloadImportar {
                tarea: en_b,
                detectado: "2026-11-21T10:00:01Z".into(),
            },
            ANA,
        )
        .unwrap();
        let de_a = tarea_de_rama(a.conexion(), "/main/dev/Grande")
            .unwrap()
            .unwrap();
        let de_b = tarea_de_rama(b.conexion(), "/main/dev/Grande")
            .unwrap()
            .unwrap();
        assert_ne!(de_a.id, de_b.id);
        let payload = |t: &Tarea, d: &str| PayloadImportar {
            tarea: t.clone(),
            detectado: d.into(),
        };
        // Cada una recibe la de la otra.
        aplicar_importar(a.conexion(), &payload(&de_b, "2026-11-21T10:00:01Z"), ANA).unwrap();
        aplicar_importar(b.conexion(), &payload(&de_a, "2026-11-21T10:00:00Z"), ANA).unwrap();
        for db in [&a, &b] {
            let t = tarea_de_rama(db.conexion(), "/main/dev/Grande")
                .unwrap()
                .unwrap();
            assert_eq!(t.id, de_a.id);
            let con_esa_rama = db_tareas::listar(db.conexion())
                .unwrap()
                .into_iter()
                .filter(|t| t.rama_asociada.as_deref() == Some("/main/dev/Grande"))
                .count();
            assert_eq!(con_esa_rama, 1);
        }
    }

    #[test]
    fn una_tarea_normal_con_la_rama_gana_siempre() {
        let db = base();
        let c = db.conexion();
        let mut normal = Tarea::nueva("T-101", "Normal", TipoTarea::Audio, ANA, "2026-11-21");
        normal.rama_asociada = Some("/main/dev/T-101-normal".into());
        db_tareas::insertar(c, &normal).unwrap();
        let mut importada = Tarea::nueva(
            "T-102",
            "T-101-normal",
            TipoTarea::Scripting,
            ANA,
            "2026-11-21",
        );
        importada.rama_asociada = Some("/main/dev/T-101-normal".into());
        aplicar_importar(
            c,
            &PayloadImportar {
                tarea: importada,
                detectado: "2000-01-01T00:00:00Z".into(),
            },
            ANA,
        )
        .unwrap();
        assert_eq!(db_tareas::listar(c).unwrap().len(), 1);
        assert_eq!(
            tarea_de_rama(c, "/main/dev/t-101-normal")
                .unwrap()
                .unwrap()
                .id,
            "T-101"
        );
    }

    #[test]
    fn rechaza_ramas_fuera_de_dev_o_con_otro_padre() {
        let db = base();
        let c = db.conexion();
        let mut t = Tarea::nueva("T-101", "X", TipoTarea::Scripting, ANA, "2026-11-21");
        t.rama_asociada = Some("/main/otra".into());
        let p = |t: &Tarea| PayloadImportar {
            tarea: t.clone(),
            detectado: "2026-11-21T10:00:00Z".into(),
        };
        assert!(aplicar_importar(c, &p(&t), ANA).is_err());
        t.rama_asociada = Some("/main/dev/a/b".into());
        assert!(aplicar_importar(c, &p(&t), ANA).is_err(), "sin tarea padre");
    }
}
