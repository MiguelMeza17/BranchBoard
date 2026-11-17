//! Listado de "Tus proyectos" locales, repositorios del servidor (`cm repo list`)
//! y clonado dentro de la app (`cm workspace create` + `cm update` con progreso, #125).

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cm::{lectura, Comando};
use crate::db::{carpeta_proyecto, NOMBRE_DB};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::RepoUnityVc;
use crate::servidor::{nombre_organizacion, region};

/// Evento emitido durante `clonar_proyecto` (#125).
pub const EVENTO_PROGRESO_CLONADO: &str = "clonado-progreso";

const TIEMPO_MAXIMO_CLONADO: Duration = Duration::from_secs(900);

/// Workspace de Unity VC en esta máquina con su estado respecto a BranchBoard (#125).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceProyecto {
    pub nombre: String,
    /// Especificación canónica `<Proyecto/Repo>@<org>@unity` cuando es Cloud, o la de `cm status --header`.
    pub repo: String,
    pub ruta: String,
    pub tiene_db_branchboard: bool,
}

/// Repositorio disponible en Unity VC Cloud al que el usuario tiene acceso (#125).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProyectoServidor {
    pub nombre: String,
    /// `<Proyecto/Repo>@<org>@unity`, listo para pasar a `clonar_proyecto`.
    pub repo: String,
    pub org: String,
    /// `true` si ya usa BranchBoard (base local o ramas de tareas `/main/dev/T-*`) → "Con tareas".
    pub tiene_db_branchboard: bool,
}

/// Evento incremental devuelto durante el clonado de un workspace (#125).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgresoClonado {
    pub porcentaje: u8,
    pub mensaje: String,
}

/// Devuelve los workspaces de Unity VC locales con información de si ya usan BranchBoard (#125).
pub fn listar_workspaces_locales_en(
    estado: &AppState,
    carpeta_datos: &Path,
) -> Resultado<Vec<WorkspaceProyecto>> {
    let workspaces = lectura::listar_workspaces(estado)?;
    let mut salida = Vec::new();
    for wk in workspaces {
        let ruta = Path::new(&wk.ruta);
        if !ruta.exists() {
            continue;
        }
        let Ok(cabecera) = lectura::rama_actual_workspace(estado, ruta) else {
            continue;
        };
        let (repo, tiene_db_branchboard) =
            match RepoUnityVc::desde_especificacion(&cabecera.repositorio) {
                Some(r) => {
                    let tiene = tiene_db_local(carpeta_datos, &r);
                    (format!("{}@{}@unity", r.repo, r.org), tiene)
                }
                None => (cabecera.repositorio, false),
            };
        salida.push(WorkspaceProyecto {
            nombre: wk.nombre,
            repo,
            ruta: wk.ruta,
            tiene_db_branchboard,
        });
    }
    Ok(salida)
}

/// "Tus proyectos": workspaces locales que ya tienen datos de BranchBoard en esta máquina (#125).
pub fn listar_tus_proyectos_en(
    estado: &AppState,
    carpeta_datos: &Path,
) -> Resultado<Vec<WorkspaceProyecto>> {
    Ok(listar_workspaces_locales_en(estado, carpeta_datos)?
        .into_iter()
        .filter(|w| w.tiene_db_branchboard)
        .collect())
}

fn tiene_db_local(carpeta_datos: &Path, repo: &RepoUnityVc) -> bool {
    carpeta_proyecto(carpeta_datos, &repo.id_proyecto())
        .map(|c| c.join(NOMBRE_DB).is_file())
        .unwrap_or(false)
}

fn organizaciones_candidatas(estado: &AppState, carpeta_datos: &Path) -> Vec<String> {
    let mut orgs: Vec<String> = Vec::new();
    let mut agregar = |o: &str| {
        let limpia = nombre_organizacion(o);
        if !limpia.is_empty() && !orgs.iter().any(|x| x.eq_ignore_ascii_case(limpia)) {
            orgs.push(limpia.to_string());
        }
    };

    if let Ok(r) = estado.repo_abierto() {
        agregar(&r.org);
    }
    if let Ok(salida) = Comando::cm()
        .args(["profile", "list", "--format={server}"])
        .ejecutar(estado)
    {
        if salida.exito() {
            for linea in salida.stdout.lines().map(str::trim) {
                if !linea.is_empty()
                    && !linea.starts_with('*')
                    && !linea.eq_ignore_ascii_case("local")
                {
                    agregar(linea);
                }
            }
        }
    }
    for org in region::organizaciones_en_cloudregions() {
        agregar(&org);
    }
    if let Ok(locales) = listar_workspaces_locales_en(estado, carpeta_datos) {
        for wk in locales {
            if let Some(r) = RepoUnityVc::desde_especificacion(&wk.repo) {
                agregar(&r.org);
            }
        }
    }
    orgs
}

/// Interpreta la salida de `cm repo list [<org>@unity] --format={repname}|{repserver}`.
pub fn interpretar_repos_servidor(
    stdout: &str,
    org_respaldo: Option<&str>,
) -> Vec<ProyectoServidor> {
    let mut proyectos = Vec::new();
    for linea in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (repname, org) = match linea.split_once('|') {
            Some((n, s)) => {
                let (n, s) = (n.trim(), s.trim());
                let org = if !s.is_empty() {
                    nombre_organizacion(s).to_string()
                } else if let Some(o) = org_respaldo {
                    nombre_organizacion(o).to_string()
                } else {
                    continue;
                };
                (n.to_string(), org)
            }
            None => match RepoUnityVc::desde_especificacion(linea) {
                Some(r) => (r.repo, r.org),
                None => continue,
            },
        };
        if repname.is_empty() || org.is_empty() {
            continue;
        }
        let spec = format!("{repname}@{org}@unity");
        if proyectos
            .iter()
            .any(|p: &ProyectoServidor| p.repo.eq_ignore_ascii_case(&spec))
        {
            continue;
        }
        let nombre = repname.rsplit('/').next().unwrap_or(&repname).to_string();
        proyectos.push(ProyectoServidor {
            nombre,
            repo: spec,
            org,
            tiene_db_branchboard: false,
        });
    }
    proyectos
}

fn repo_tiene_ramas_branchboard(estado: &AppState, spec: &str) -> bool {
    let ok_rama = Comando::cm()
        .args(["find", "branch"])
        .arg(format!("where parent='/main/dev' on repository '{spec}'"))
        .arg("--format={name}")
        .arg("--nototal")
        .ejecutar(estado);
    let Ok(salida) = ok_rama else {
        return false;
    };
    salida.exito()
        && salida
            .stdout
            .lines()
            .map(str::trim)
            .any(|l| l.starts_with("/main/dev/T-"))
}

/// Lista los repositorios accesibles en Unity VC Cloud vía `cm repo list`, marcando
/// con `tiene_db_branchboard = true` ("Con tareas") los que ya usan BranchBoard (#125).
pub fn listar_proyectos_servidor_en(
    estado: &AppState,
    carpeta_datos: &Path,
) -> Resultado<Vec<ProyectoServidor>> {
    let _email = lectura::whoami(estado)?;
    let orgs = organizaciones_candidatas(estado, carpeta_datos);

    let mut proyectos: Vec<ProyectoServidor> = Vec::new();
    let mut alguna_ok = false;
    let mut ultimo_error: Option<Error> = None;

    if orgs.is_empty() {
        let stdout = Comando::cm()
            .args(["repo", "list", "--format={repname}|{repserver}"])
            .exigir_exito(estado)?;
        alguna_ok = true;
        proyectos.extend(interpretar_repos_servidor(&stdout, None));
    } else {
        for org in &orgs {
            let servidor = format!("{org}@unity");
            let salida = Comando::cm()
                .args(["repo", "list", &servidor, "--format={repname}|{repserver}"])
                .ejecutar(estado)?;
            if salida.exito() {
                alguna_ok = true;
                for item in interpretar_repos_servidor(&salida.stdout, Some(org)) {
                    if !proyectos
                        .iter()
                        .any(|p| p.repo.eq_ignore_ascii_case(&item.repo))
                    {
                        proyectos.push(item);
                    }
                }
            } else {
                let detalle = if salida.stderr.trim().is_empty() {
                    salida.stdout.trim()
                } else {
                    salida.stderr.trim()
                };
                ultimo_error = Some(Error::Cm(format!("`cm repo list {servidor}`: {detalle}")));
            }
        }
    }

    if !alguna_ok {
        if let Some(err) = ultimo_error {
            return Err(err);
        }
    }

    for p in &mut proyectos {
        if let Some(r) = RepoUnityVc::desde_especificacion(&p.repo) {
            p.tiene_db_branchboard =
                tiene_db_local(carpeta_datos, &r) || repo_tiene_ramas_branchboard(estado, &p.repo);
        }
    }

    Ok(proyectos)
}

/// Resuelve y valida la carpeta destino para clonar sin sobrescribir carpetas existentes con datos.
pub fn preparar_ruta_clonado(carpeta_destino: &str, nombre_proyecto: &str) -> Resultado<PathBuf> {
    let limpia = carpeta_destino.trim();
    if limpia.is_empty() {
        return Err(Error::Validacion(
            "Elige una carpeta de destino para clonar el proyecto.".into(),
        ));
    }
    let base = Path::new(limpia);
    if base.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Error::Validacion(
            "La carpeta de destino no puede contener `..`.".into(),
        ));
    }
    if nombre_proyecto.trim().is_empty()
        || nombre_proyecto.contains('/')
        || nombre_proyecto.contains('\\')
        || nombre_proyecto.contains("..")
    {
        return Err(Error::Validacion(
            "El nombre del proyecto no es válido.".into(),
        ));
    }

    let ya_termina_en_proyecto = base
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case(nombre_proyecto));

    let destino = if ya_termina_en_proyecto {
        base.to_path_buf()
    } else {
        base.join(nombre_proyecto)
    };

    if destino.exists() {
        if !destino.is_dir() {
            return Err(Error::Validacion(format!(
                "{} ya existe y no es una carpeta.",
                destino.display()
            )));
        }
        let tiene_algo = std::fs::read_dir(&destino)?.next().is_some();
        if tiene_algo {
            return Err(Error::Validacion(format!(
                "La carpeta {} ya tiene archivos. Elige una carpeta vacía.",
                destino.display()
            )));
        }
    }

    Ok(destino)
}

fn nombre_workspace_disponible(base: &str, existentes: &[lectura::Workspace]) -> String {
    let saneado: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let prefijo = if saneado.is_empty() {
        "BranchBoard-wk"
    } else {
        saneado.as_str()
    };
    if !existentes
        .iter()
        .any(|w| w.nombre.eq_ignore_ascii_case(prefijo))
    {
        return prefijo.to_string();
    }
    for n in 2..=100 {
        let candidato = format!("{prefijo}-{n}");
        if !existentes
            .iter()
            .any(|w| w.nombre.eq_ignore_ascii_case(&candidato))
        {
            return candidato;
        }
    }
    format!("{prefijo}-{}", chrono::Local::now().format("%H%M%S"))
}

/// Calcula el siguiente porcentaje de progreso (25..=95) a partir de una línea de `cm update`.
pub fn interpretar_progreso_update(linea: &str, actual: u8) -> u8 {
    if let Some(pos) = linea.find('%') {
        let antes = linea[..pos].trim_end();
        let digitos: String = antes
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(pct) = digitos.parse::<f32>() {
            let mapeado = 25 + ((pct.clamp(0.0, 100.0) * 0.70).round() as u8);
            return mapeado.clamp(actual, 95);
        }
    }
    actual.saturating_add(1).min(94)
}

/// Clona un repositorio de Unity VC Cloud (`cm workspace create` + `cm update --forcedetailedprogress`)
/// reportando el progreso paso a paso (#125).
pub fn clonar_proyecto_en(
    estado: &AppState,
    carpeta_datos: &Path,
    repo_spec: &str,
    carpeta_destino: &str,
    mut al_progresar: impl FnMut(ProgresoClonado),
) -> Resultado<WorkspaceProyecto> {
    let _email = lectura::whoami(estado)?;
    let repo = RepoUnityVc::desde_especificacion(repo_spec).ok_or_else(|| {
        Error::Validacion(format!(
            "Especificación de repositorio no válida ({repo_spec}). Elige un proyecto de Unity VC Cloud."
        ))
    })?;
    let spec_canonico = format!("{}@{}@unity", repo.repo, repo.org);
    let nombre = repo.nombre().to_string();
    let ruta_ws = preparar_ruta_clonado(carpeta_destino, &nombre)?;
    let creamos_carpeta = !ruta_ws.exists();
    std::fs::create_dir_all(&ruta_ws)?;

    let existentes = lectura::listar_workspaces(estado)?;
    let nombre_wk = nombre_workspace_disponible(&nombre, &existentes);
    let ruta_str = ruta_ws.to_string_lossy().to_string();

    al_progresar(ProgresoClonado {
        porcentaje: 10,
        mensaje: format!("Creando workspace {nombre_wk}…"),
    });

    if let Err(e) = Comando::cm()
        .args(["workspace", "create", &nombre_wk, &ruta_str])
        .arg(format!("rep:{spec_canonico}"))
        .exigir_exito(estado)
    {
        if creamos_carpeta {
            let _ = std::fs::remove_dir_all(&ruta_ws);
        }
        return Err(e);
    }

    al_progresar(ProgresoClonado {
        porcentaje: 25,
        mensaje: "Descargando archivos del repositorio…".into(),
    });

    let mut porcentaje: u8 = 25;
    let salida = Comando::cm()
        .args(["update", ".", "--forcedetailedprogress", "--noinput"])
        .en(&ruta_ws)
        .tiempo_maximo(TIEMPO_MAXIMO_CLONADO)
        .ejecutar_con_progreso(estado, |linea| {
            porcentaje = interpretar_progreso_update(linea, porcentaje);
            al_progresar(ProgresoClonado {
                porcentaje,
                mensaje: linea.chars().take(120).collect(),
            });
        });

    let ok_update = match salida {
        Ok(s) if s.exito() => Ok(()),
        Ok(s) => {
            let detalle = if s.stderr.trim().is_empty() {
                s.stdout.trim()
            } else {
                s.stderr.trim()
            };
            Err(Error::Cm(format!(
                "`cm update .`: {}",
                if detalle.is_empty() {
                    "falló la descarga"
                } else {
                    detalle
                }
            )))
        }
        Err(e) => Err(e),
    };

    if let Err(e) = ok_update {
        let _ = Comando::cm()
            .args(["workspace", "delete", &ruta_str])
            .ejecutar(estado);
        if creamos_carpeta {
            let _ = std::fs::remove_dir_all(&ruta_ws);
        }
        return Err(e);
    }

    al_progresar(ProgresoClonado {
        porcentaje: 100,
        mensaje: format!("{nombre} clonado correctamente"),
    });

    let tiene_db = tiene_db_local(carpeta_datos, &repo)
        || repo_tiene_ramas_branchboard(estado, &spec_canonico);
    Ok(WorkspaceProyecto {
        nombre: nombre_wk,
        repo: spec_canonico,
        ruta: ruta_str,
        tiene_db_branchboard: tiene_db,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_repos_servidor_y_deduplica() {
        let salida = "\
VirusTF/VirusTF|MiguelM17@unity
BranchBoard-Pruebas/BranchBoard-Pruebas|MiguelM17@cloud
BranchBoard-Pruebas/BranchBoard-Pruebas|MiguelM17@unity
";
        let lista = interpretar_repos_servidor(salida, Some("MiguelM17"));
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0].nombre, "VirusTF");
        assert_eq!(lista[0].repo, "VirusTF/VirusTF@MiguelM17@unity");
        assert_eq!(lista[1].nombre, "BranchBoard-Pruebas");
        assert_eq!(
            lista[1].repo,
            "BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity"
        );
    }

    #[test]
    fn prepara_ruta_clonado_y_rechaza_traversal_o_carpeta_ocupada() {
        let tmp = tempfile::tempdir().unwrap();
        let esperado = tmp.path().join("VirusTF");
        assert_eq!(
            preparar_ruta_clonado(&tmp.path().to_string_lossy(), "VirusTF").unwrap(),
            esperado
        );

        // Si ya termina en el nombre y está vacía, la usa directamente.
        std::fs::create_dir_all(&esperado).unwrap();
        assert_eq!(
            preparar_ruta_clonado(&esperado.to_string_lossy(), "VirusTF").unwrap(),
            esperado
        );

        // Si ya tiene archivos, rechaza.
        std::fs::write(esperado.join("archivo.txt"), b"x").unwrap();
        assert!(preparar_ruta_clonado(&tmp.path().to_string_lossy(), "VirusTF").is_err());

        // Traversal bloqueado.
        assert!(preparar_ruta_clonado("../escape", "VirusTF").is_err());
    }

    #[test]
    fn interpreta_progreso_de_update_sin_retroceder() {
        assert_eq!(
            interpretar_progreso_update("Downloading 50% (10/20 MB)", 25),
            60
        );
        assert_eq!(
            interpretar_progreso_update("Descargando archivo.cs", 60),
            61
        );
        assert_eq!(interpretar_progreso_update("10% inicial", 61), 61);
        assert_eq!(interpretar_progreso_update("100%", 61), 95);
    }

    /// **Prueba en el entorno real** (CLAUDE.md):
    /// 1. Lista repositorios reales con `listar_proyectos_servidor_en` y verifica
    ///    que `BranchBoard-Pruebas` aparece marcado con `"Con tareas"` (`tiene_db_branchboard = true`).
    /// 2. Clona `BranchBoard-Pruebas` con `clonar_proyecto_en` hacia una carpeta temporal,
    ///    comprobando eventos incrementales de progreso hasta `100%` y descarga real de archivos.
    /// 3. Comprueba `listar_workspaces_locales_en`, entra al proyecto (`entrar_en`) y verifica
    ///    que `listar_tus_proyectos_en` pasa de no incluirlo (antes de datos) a listarlo con `.db`.
    /// 4. Limpia el workspace clonado al terminar.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_proyectos_y_clonar() {
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let spec = format!("{repo}@{org}@unity");
        let estado = AppState::default();
        let datos = tempfile::tempdir().unwrap();
        let destino = tempfile::tempdir().unwrap();

        let servidor = listar_proyectos_servidor_en(&estado, datos.path()).unwrap();
        let encontrado = servidor
            .iter()
            .find(|p| p.repo.eq_ignore_ascii_case(&spec))
            .expect("BranchBoard-Pruebas debe figurar en cm repo list");
        assert!(
            encontrado.tiene_db_branchboard,
            "BranchBoard-Pruebas tiene ramas /main/dev/T-* y debe marcarse Con tareas"
        );

        let mut eventos = Vec::new();
        let clonado = clonar_proyecto_en(
            &estado,
            datos.path(),
            &encontrado.repo,
            &destino.path().to_string_lossy(),
            |p| eventos.push(p),
        )
        .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert!(eventos.first().is_some_and(|e| e.porcentaje >= 10));
            assert_eq!(eventos.last().map(|e| e.porcentaje), Some(100));
            assert!(Path::new(&clonado.ruta).join(".plastic").exists());

            // Antes de entrar, no tiene `.db` local → aparece en workspaces pero no en "Tus proyectos".
            // `cm wk list` en Windows devuelve unidad en minúscula (`c:\...`).
            let misma_ruta = |a: &str, b: &str| {
                if cfg!(windows) {
                    a.replace('/', "\\")
                        .eq_ignore_ascii_case(&b.replace('/', "\\"))
                } else {
                    a == b
                }
            };
            let locales_antes = listar_workspaces_locales_en(&estado, datos.path()).unwrap();
            assert!(
                locales_antes
                    .iter()
                    .any(|w| misma_ruta(&w.ruta, &clonado.ruta)),
                "no apareció {} en {:?}",
                clonado.ruta,
                locales_antes
            );
            let tus_antes = listar_tus_proyectos_en(&estado, datos.path()).unwrap();
            assert!(!tus_antes.iter().any(|w| misma_ruta(&w.ruta, &clonado.ruta)));

            // Al entrar al proyecto se crea su `.db` y aparece en "Tus proyectos".
            crate::comandos::acceso::entrar_en(&estado, datos.path(), Path::new(&clonado.ruta))
                .unwrap();
            let tus_despues = listar_tus_proyectos_en(&estado, datos.path()).unwrap();
            assert!(tus_despues
                .iter()
                .any(|w| misma_ruta(&w.ruta, &clonado.ruta)));
        }));

        estado.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete", &clonado.ruta])
            .ejecutar(&estado);

        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
        println!(
            "Clonado {} en {} con {} eventos de progreso",
            encontrado.repo,
            clonado.ruta,
            eventos.len()
        );
    }
}
