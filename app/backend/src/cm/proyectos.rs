//! Listado de "Tus proyectos" locales, repositorios del servidor (`cm repo list`)
//! y clonado dentro de la app (`cm workspace create` + `cm update` con progreso, #125).

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cm::{lectura, Comando};
use crate::db::{carpeta_proyecto, NOMBRE_DB};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::{Entrada, RepoUnityVc};
use crate::ramas::{RAMA_DEV, RAMA_MAIN};
use crate::servidor::{self, nombre_organizacion, region};

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

/// Resultado de crear o traer un proyecto de Unity VC (#126).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResultadoProyectoNuevo {
    pub workspace: WorkspaceProyecto,
    pub entrada: Entrada,
}

fn misma_ruta(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.replace('/', "\\")
            .eq_ignore_ascii_case(&b.replace('/', "\\"))
    } else {
        a == b
    }
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
        if repname.is_empty()
            || org.is_empty()
            || repname.starts_with("__DELETED__")
            || repname
                .rsplit('/')
                .next()
                .is_some_and(|seg| seg.starts_with("__DELETED__"))
        {
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
        limpiar_workspace_y_carpeta(estado, &ruta_ws, creamos_carpeta);
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

fn limpiar_workspace_y_carpeta(estado: &AppState, ruta_ws: &Path, limpiar_carpeta: bool) {
    let ruta_str = ruta_ws.to_string_lossy();
    let _ = Comando::cm()
        .args(["workspace", "delete", &ruta_str])
        .ejecutar(estado);
    if limpiar_carpeta {
        let _ = std::fs::remove_dir_all(ruta_ws);
    }
}

fn validar_nombre_proyecto(nombre: &str) -> Resultado<&str> {
    let limpia = nombre.trim();
    if limpia.is_empty() {
        return Err(Error::Validacion(
            "Escribe un nombre para el proyecto.".into(),
        ));
    }
    if limpia.len() > 80 {
        return Err(Error::Validacion(
            "El nombre del proyecto no puede superar 80 caracteres.".into(),
        ));
    }
    if limpia == "."
        || limpia == ".."
        || limpia.contains("..")
        || limpia.to_ascii_uppercase().starts_with("__DELETED__")
        || limpia.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    '/' | '\\' | '@' | ':' | '"' | '\'' | '*' | '?' | '<' | '>' | '|'
                )
        })
    {
        return Err(Error::Validacion(
            "El nombre del proyecto no puede incluir `/`, `\\`, `@`, `:` ni caracteres especiales."
                .into(),
        ));
    }
    Ok(limpia)
}

fn asegurar_rama_dev(
    estado: &AppState,
    email: &str,
    repo: &RepoUnityVc,
    carpeta_ws: &Path,
) -> Resultado<()> {
    if let Ok(cliente) = servidor::conectar(estado, &servidor::LlaveroSistema, email, &repo.org) {
        if cliente.rama(&repo.repo, RAMA_DEV)?.is_some() {
            return Ok(());
        }
        if let Some(main) = cliente.rama(&repo.repo, RAMA_MAIN)? {
            cliente.crear_rama(
                &repo.repo,
                RAMA_DEV,
                main.head_changeset,
                "Rama base de desarrollo (BranchBoard)",
            )?;
            return Ok(());
        }
    }
    let salida = Comando::cm()
        .args([
            "branch",
            "create",
            RAMA_DEV,
            "-c=Rama base de desarrollo (BranchBoard)",
        ])
        .en(carpeta_ws)
        .ejecutar(estado)?;
    if salida.exito()
        || salida.stderr.to_ascii_lowercase().contains("ya existe")
        || salida
            .stderr
            .to_ascii_lowercase()
            .contains("already exists")
    {
        Ok(())
    } else {
        let detalle = if salida.stderr.trim().is_empty() {
            salida.stdout.trim()
        } else {
            salida.stderr.trim()
        };
        Err(Error::Cm(format!(
            "`cm branch create {RAMA_DEV}`: {detalle}"
        )))
    }
}

/// Crea un proyecto nuevo en Unity VC Cloud (`cm repo create` + `cm workspace create`),
/// asegura la rama `/main/dev` y lo configura en BranchBoard dejando al creador como
/// Administrador (#126).
pub fn crear_proyecto_en(
    estado: &AppState,
    carpeta_datos: &Path,
    nombre: &str,
    carpeta_destino: &str,
    proyecto_unity: Option<&str>,
    org_opcional: Option<&str>,
) -> Resultado<ResultadoProyectoNuevo> {
    let email = lectura::whoami(estado)?;
    let nombre = validar_nombre_proyecto(nombre)?;
    let proyecto_padre = match proyecto_unity.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Some(validar_nombre_proyecto(p)?),
        None => None,
    };
    let org = match org_opcional
        .map(nombre_organizacion)
        .filter(|s| !s.is_empty())
    {
        Some(o) => o.to_string(),
        None => organizaciones_candidatas(estado, carpeta_datos)
            .into_iter()
            .next()
            .ok_or_else(|| {
                Error::Validacion(
                    "No detectamos una organización de Unity VC Cloud. Reconecta tu sesión en Unity VC e inténtalo de nuevo."
                        .into(),
                )
            })?,
    };

    let ruta_ws = preparar_ruta_clonado(carpeta_destino, nombre)?;
    let repo_nombre = match proyecto_padre {
        Some(p) => format!("{p}/{nombre}"),
        None => nombre.to_string(),
    };
    let spec_canonico = format!("{repo_nombre}@{org}@unity");
    let repo = RepoUnityVc::desde_especificacion(&spec_canonico).ok_or_else(|| {
        Error::Validacion(format!(
            "Especificación de repositorio no válida: {spec_canonico}"
        ))
    })?;

    if let Err(err) = Comando::cm()
        .args(["repo", "create", &spec_canonico])
        .exigir_exito(estado)
    {
        let msg = err.to_string();
        if msg.contains("Repositories must be created under a project in Unity Organizations") {
            return Err(Error::Validacion(
                "En Unity VC Cloud debes crear el repositorio dentro de un proyecto existente de tu organización (por ejemplo, créalo primero en Unity Dashboard o elige uno existente en el selector).".into(),
            ));
        }
        return Err(err);
    }

    let rollback_repo = || {
        let _ = Comando::cm()
            .args(["repo", "rm", &format!("rep:{spec_canonico}")])
            .ejecutar(estado);
    };

    let creamos_carpeta = !ruta_ws.exists();
    if let Err(e) = std::fs::create_dir_all(&ruta_ws) {
        rollback_repo();
        return Err(e.into());
    }

    let existentes = match lectura::listar_workspaces(estado) {
        Ok(lista) => lista,
        Err(e) => {
            if creamos_carpeta {
                let _ = std::fs::remove_dir_all(&ruta_ws);
            }
            rollback_repo();
            return Err(e);
        }
    };
    let nombre_wk = nombre_workspace_disponible(nombre, &existentes);
    let ruta_str = ruta_ws.to_string_lossy().to_string();

    if let Err(e) = Comando::cm()
        .args(["workspace", "create", &nombre_wk, &ruta_str])
        .arg(format!("rep:{spec_canonico}"))
        .exigir_exito(estado)
    {
        if creamos_carpeta {
            let _ = std::fs::remove_dir_all(&ruta_ws);
        }
        rollback_repo();
        return Err(e);
    }

    let preparar = || -> Resultado<Entrada> {
        Comando::cm()
            .args(["update", ".", "--noinput"])
            .en(&ruta_ws)
            .exigir_exito(estado)?;
        asegurar_rama_dev(estado, &email, &repo, &ruta_ws)?;
        Comando::cm()
            .args(["switch", &format!("br:{RAMA_DEV}")])
            .en(&ruta_ws)
            .exigir_exito(estado)?;
        let entrada = crate::comandos::acceso::entrar_en(estado, carpeta_datos, &ruta_ws)?;
        if !matches!(entrada, Entrada::Dentro { .. }) {
            return Err(Error::SinPermiso);
        }
        Ok(entrada)
    };

    match preparar() {
        Ok(entrada) => Ok(ResultadoProyectoNuevo {
            workspace: WorkspaceProyecto {
                nombre: nombre_wk,
                repo: spec_canonico,
                ruta: ruta_str,
                tiene_db_branchboard: true,
            },
            entrada,
        }),
        Err(e) => {
            estado.cerrar_proyecto();
            limpiar_workspace_y_carpeta(estado, &ruta_ws, creamos_carpeta);
            rollback_repo();
            if let Ok(carpeta) = carpeta_proyecto(carpeta_datos, &repo.id_proyecto()) {
                let _ = std::fs::remove_dir_all(carpeta);
            }
            Err(e)
        }
    }
}

/// Configura con BranchBoard un workspace local de Unity VC que todavía no se usa
/// aquí (#126). Si el usuario es dueño del repositorio, queda como Administrador.
pub fn traer_proyecto_en(
    estado: &AppState,
    carpeta_datos: &Path,
    carpeta_workspace: &Path,
) -> Resultado<ResultadoProyectoNuevo> {
    let email = lectura::whoami(estado)?;
    if !carpeta_workspace.is_dir() {
        return Err(Error::Validacion(format!(
            "No existe la carpeta del workspace: {}",
            carpeta_workspace.display()
        )));
    }
    let cabecera = lectura::rama_actual_workspace(estado, carpeta_workspace)?;
    let repo = RepoUnityVc::desde_especificacion(&cabecera.repositorio).ok_or_else(|| {
        Error::Validacion(
            "Este workspace no apunta a un repositorio de Unity VC Cloud compatible con BranchBoard."
                .into(),
        )
    })?;
    if tiene_db_local(carpeta_datos, &repo) {
        return Err(Error::Validacion(
            "Este proyecto ya está configurado con BranchBoard en este equipo. Ábrelo desde «Tus proyectos»."
                .into(),
        ));
    }

    let entrada = crate::comandos::acceso::entrar_en(estado, carpeta_datos, carpeta_workspace)?;
    if let Entrada::Dentro {
        configurado_ahora, ..
    } = &entrada
    {
        if let Err(e) = asegurar_rama_dev(estado, &email, &repo, carpeta_workspace) {
            if *configurado_ahora {
                estado.cerrar_proyecto();
                if let Ok(carpeta) = carpeta_proyecto(carpeta_datos, &repo.id_proyecto()) {
                    let _ = std::fs::remove_dir_all(carpeta);
                }
            }
            return Err(e);
        }
    }

    let ruta_str = carpeta_workspace.to_string_lossy().to_string();
    let nombre = lectura::listar_workspaces(estado)
        .ok()
        .and_then(|lista| {
            lista
                .into_iter()
                .find(|w| misma_ruta(&w.ruta, &ruta_str))
                .map(|w| w.nombre)
        })
        .unwrap_or_else(|| repo.nombre().to_string());

    let tiene_db =
        matches!(entrada, Entrada::Dentro { .. }) || tiene_db_local(carpeta_datos, &repo);
    Ok(ResultadoProyectoNuevo {
        workspace: WorkspaceProyecto {
            nombre,
            repo: format!("{}@{}@unity", repo.repo, repo.org),
            ruta: ruta_str,
            tiene_db_branchboard: tiene_db,
        },
        entrada,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_repos_servidor_y_deduplica_y_filtra_eliminados() {
        let salida = "\
VirusTF/VirusTF|MiguelM17@unity
BranchBoard-Pruebas/BranchBoard-Pruebas|MiguelM17@cloud
BranchBoard-Pruebas/BranchBoard-Pruebas|MiguelM17@unity
__DELETED__BranchBoard-Pruebas/bb-viejo|MiguelM17@unity
Grupo/__DELETED__Sub|MiguelM17@unity
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
    fn valida_nombre_proyecto_rechaza_invalidos() {
        assert_eq!(validar_nombre_proyecto(" MiJuego ").unwrap(), "MiJuego");
        assert!(validar_nombre_proyecto("   ").is_err());
        assert!(validar_nombre_proyecto("a/b").is_err());
        assert!(validar_nombre_proyecto("a\\b").is_err());
        assert!(validar_nombre_proyecto("a@b").is_err());
        assert!(validar_nombre_proyecto("..").is_err());
        assert!(validar_nombre_proyecto("__DELETED__x").is_err());
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

    /// **Prueba en el entorno real** (CLAUDE.md):
    /// 1. Traer un proyecto de Unity VC (`traer_proyecto_en`) desde un workspace clonado de
    ///    `BranchBoard-Pruebas` sin `.db` local previa → configura el proyecto con el dueño como
    ///    `Administrador` (`configurado_ahora = true`) y rechaza repetirlo una segunda vez.
    /// 2. Crear un proyecto nuevo (`crear_proyecto_en`: `cm repo create` + `cm workspace create` +
    ///    `/main/dev` + `Administrador`), comprueba rama `/main/dev`, listado en "Tus proyectos" y
    ///    en `cm repo list`.
    /// 3. Limpia workspace temporal y repositorio de prueba (`cm workspace delete` + `cm repo rm`),
    ///    y verifica que los repos `__DELETED__` quedan filtrados.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_crear_y_traer_proyecto() {
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let spec = format!("{repo}@{org}@unity");
        let estado = AppState::default();

        // ── Parte A: traer proyecto local sin BranchBoard ──
        let datos_traer = tempfile::tempdir().unwrap();
        let destino_traer = tempfile::tempdir().unwrap();
        let clonado = clonar_proyecto_en(
            &estado,
            datos_traer.path(),
            &spec,
            &destino_traer.path().to_string_lossy(),
            |_| {},
        )
        .unwrap();

        let res_traer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let traido =
                traer_proyecto_en(&estado, datos_traer.path(), Path::new(&clonado.ruta)).unwrap();
            assert!(traido.workspace.tiene_db_branchboard);
            assert!(matches!(
                traido.entrada,
                Entrada::Dentro {
                    rol: crate::modelo::Rol::Administrador,
                    rol_verificado: true,
                    configurado_ahora: true,
                    ..
                }
            ));
            // Si ya tiene `.db` en esta máquina, rechaza traerlo otra vez.
            assert!(
                traer_proyecto_en(&estado, datos_traer.path(), Path::new(&clonado.ruta)).is_err()
            );
        }));
        estado.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete", &clonado.ruta])
            .ejecutar(&estado);
        if let Err(p) = res_traer {
            std::panic::resume_unwind(p);
        }

        // ── Parte B: crear proyecto nuevo en Unity VC Cloud ──
        let datos_crear = tempfile::tempdir().unwrap();
        let destino_crear = tempfile::tempdir().unwrap();
        let sufijo = chrono::Utc::now().format("%y%m%d%H%M%S");
        let nombre_nuevo = format!("bb-prueba-126-{sufijo}");
        let proyecto_unity = repo.split('/').next().unwrap_or(&repo);

        let creado = crear_proyecto_en(
            &estado,
            datos_crear.path(),
            &nombre_nuevo,
            &destino_crear.path().to_string_lossy(),
            Some(proyecto_unity),
            Some(&org),
        )
        .unwrap();

        let res_crear = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert!(creado.workspace.tiene_db_branchboard);
            assert!(matches!(
                creado.entrada,
                Entrada::Dentro {
                    rol: crate::modelo::Rol::Administrador,
                    rol_verificado: true,
                    configurado_ahora: true,
                    ..
                }
            ));
            let ruta_ws = Path::new(&creado.workspace.ruta);
            assert!(ruta_ws.join(".plastic").exists());

            let cabecera = lectura::rama_actual_workspace(&estado, ruta_ws).unwrap();
            assert_eq!(cabecera.rama, RAMA_DEV);

            let tus = listar_tus_proyectos_en(&estado, datos_crear.path()).unwrap();
            assert!(tus
                .iter()
                .any(|w| misma_ruta(&w.ruta, &creado.workspace.ruta)));

            let srv = listar_proyectos_servidor_en(&estado, datos_crear.path()).unwrap();
            assert!(srv
                .iter()
                .any(|p| p.repo.eq_ignore_ascii_case(&creado.workspace.repo)));
        }));

        estado.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete", &creado.workspace.ruta])
            .ejecutar(&estado);
        let _ = Comando::cm()
            .args(["repo", "rm", &format!("rep:{}", creado.workspace.repo)])
            .ejecutar(&estado);

        if let Err(p) = res_crear {
            std::panic::resume_unwind(p);
        }

        // Tras borrarlo, no debe volver a listarse como activo.
        let srv_despues = listar_proyectos_servidor_en(&estado, datos_crear.path()).unwrap();
        assert!(
            !srv_despues
                .iter()
                .any(|p| p.repo.eq_ignore_ascii_case(&creado.workspace.repo)),
            "el repo eliminado siguió saliendo: {:?}",
            srv_despues
        );
        println!(
            "Traído {} y creado+limpiado {}",
            spec, creado.workspace.repo
        );
    }
}
