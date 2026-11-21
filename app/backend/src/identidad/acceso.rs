//! `entrar_a_proyecto` (issue #89): qué ve la persona al abrir un workspace.
//! Ver "Identidad de usuario" en BACKEND.md.
//!
//! - Registrado → entra con su rol (desde #243 no hay firmas ni "pendiente de confirmar").
//! - No registrado → "pídele a un Líder o Administrador".
//! - Proyecto sin datos de BranchBoard en esta máquina: si quien entra es el
//!   **dueño del repositorio** (el `owner` que da la API de Unity VC) se configura
//!   y queda como Administrador; si no, se le pide que espere a que lo abra su dueño.
//!
//! Los datos del proyecto viven en `<datos>/proyectos/<org>-<repo>/`. "Tener datos" es tener ahí la
//! base con al menos un Administrador (hasta #243, la base y la llave pública del Administrador).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::db::configuracion::{self, CLAVE_NOMBRE_PROYECTO};
use crate::db::usuarios::{self, nombre_desde_email, Usuario};
use crate::db::{carpeta_proyecto, vistos, Db, NOMBRE_DB};
use crate::error::{Error, Resultado};
use crate::modelo::{normalizar_email, Rol};

/// Repositorio de Unity VC Cloud de un workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoUnityVc {
    /// `Proyecto/Repo`, como lo nombra la API.
    pub repo: String,
    /// Organización, sin `@unity`.
    pub org: String,
}

impl RepoUnityVc {
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` (lo que da
    /// `cm status --header` después de la rama) → repo y organización.
    /// Un servidor local (`VirusTF@PC:8087`) no es de Unity VC Cloud → `None`.
    pub fn desde_especificacion(especificacion: &str) -> Option<Self> {
        let mut partes = especificacion.trim().split('@');
        let repo = partes.next()?.trim();
        let org = partes.next()?.trim();
        let servidor = partes.next()?.trim();
        let es_cloud =
            servidor.eq_ignore_ascii_case("unity") || servidor.eq_ignore_ascii_case("cloud");
        (es_cloud && !repo.is_empty() && !org.is_empty()).then(|| RepoUnityVc {
            repo: repo.to_string(),
            org: org.to_string(),
        })
    }

    /// Identificador de la carpeta local del proyecto.
    pub fn id_proyecto(&self) -> String {
        format!("{}-{}", self.org, self.repo)
    }

    /// Nombre que se muestra: la parte del repo después de la `/`.
    pub fn nombre(&self) -> &str {
        self.repo.rsplit('/').next().unwrap_or(&self.repo)
    }
}

/// Lo que ve la persona al entrar. Llega al frontend como `{ estado, … }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "estado", rename_all = "snake_case")]
pub enum Entrada {
    /// Entra con su rol. `configurado_ahora` = era el dueño y el proyecto se
    /// configuró en este momento.
    Dentro {
        email: String,
        rol: Rol,
        rol_verificado: bool,
        proyecto: String,
        configurado_ahora: bool,
        #[serde(default)]
        vistos: BTreeMap<String, String>,
    },
    /// Registrado, pero el Administrador todavía no confirma su rol. Ya no pasa desde #243 (no hay
    /// firmas); se deja mientras el frontend lo siga esperando.
    Pendiente { email: String, proyecto: String },
    /// Su correo no está en el proyecto.
    NoRegistrado { email: String, proyecto: String },
    /// El proyecto no tiene datos de BranchBoard y quien entra no es su dueño.
    EsperarDueno { email: String, proyecto: String },
}

/// Decide qué ve `email` en el proyecto `repo`. Si entra (o se configura),
/// devuelve también la base abierta para dejarla en el estado de la app.
/// `es_dueno` solo se consulta si el proyecto todavía no tiene datos.
pub fn entrar(
    carpeta_datos: &Path,
    repo: &RepoUnityVc,
    email: &str,
    es_dueno: impl FnOnce() -> Resultado<bool>,
    ahora: &str,
    hoy: &str,
) -> Resultado<(Entrada, Option<Db>)> {
    let email = normalizar_email(email);
    let proyecto = repo.nombre().to_string();
    let carpeta = carpeta_proyecto(carpeta_datos, &repo.id_proyecto())?;

    let db = if carpeta.join(NOMBRE_DB).is_file() {
        let db = Db::abrir(&carpeta)?;
        let con_admin = usuarios::listar(db.conexion())?
            .iter()
            .any(|u| u.rol == Rol::Administrador);
        con_admin.then_some(db)
    } else {
        None
    };
    let Some(db) = db else {
        if !es_dueno()? {
            return Ok((Entrada::EsperarDueno { email, proyecto }, None));
        }
        let db = configurar(&carpeta, repo, &email, hoy)?;
        return Ok((
            Entrada::Dentro {
                email,
                rol: Rol::Administrador,
                rol_verificado: true,
                proyecto,
                configurado_ahora: true,
                vistos: BTreeMap::new(),
            },
            Some(db),
        ));
    };

    let fila = usuarios::buscar(db.conexion(), &email)?;
    let vistos_usuario = vistos::listar_vistos(db.conexion(), &email)?;
    let entrada = match fila {
        None => Entrada::NoRegistrado { email, proyecto },
        Some(u) => Entrada::Dentro {
            email,
            rol: u.rol,
            rol_verificado: true,
            proyecto,
            configurado_ahora: false,
            vistos: vistos_usuario,
        },
    };
    let abierta = matches!(entrada, Entrada::Dentro { .. }).then_some(db);
    Ok((entrada, abierta))
}

/// Primera vez del dueño: crea la base y guarda su fila de Administrador y el nombre del proyecto.
fn configurar(
    carpeta: &Path,
    repo: &RepoUnityVc,
    email: &str,
    hoy: &str,
) -> Resultado<Db> {
    std::fs::create_dir_all(carpeta)?;
    let db = Db::abrir(carpeta)?;
    let (nombre, apellido) = nombre_desde_email(email);
    let admin = Usuario {
        email: email.to_string(),
        nombre,
        apellido,
        rol: Rol::Administrador,
        fecha_alta: hoy.to_string(),
    };
    let c = db.conexion();
    let tx = c.unchecked_transaction()?;
    // Una fila previa (configuración a medias, sin Administrador) se reemplaza.
    if usuarios::buscar(&tx, email)?.is_some() {
        usuarios::eliminar(&tx, email)?;
    }
    usuarios::insertar(&tx, &admin)?;
    configuracion::guardar(&tx, CLAVE_NOMBRE_PROYECTO, repo.nombre(), email, hoy)?;
    tx.commit()?;
    Ok(db)
}

/// Mensaje si el workspace no es de Unity VC Cloud.
pub fn error_repo_no_cloud(especificacion: &str) -> Error {
    Error::Validacion(format!(
        "El workspace no es de Unity Version Control Cloud ({especificacion}); BranchBoard trabaja con repositorios de Unity Cloud"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHORA: &str = "2026-11-14T10:00:00Z";
    const HOY: &str = "2026-11-14";

    fn repo() -> RepoUnityVc {
        RepoUnityVc::desde_especificacion("BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity")
            .unwrap()
    }

    fn entrar_como(datos: &Path, email: &str, dueno: bool) -> Entrada {
        entrar(datos, &repo(), email, || Ok(dueno), AHORA, HOY)
            .unwrap()
            .0
    }

    #[test]
    fn lee_el_repo_de_la_cabecera_de_cm() {
        let r = repo();
        assert_eq!(r.repo, "BranchBoard-Pruebas/BranchBoard-Pruebas");
        assert_eq!(r.org, "MiguelM17");
        assert_eq!(r.nombre(), "BranchBoard-Pruebas");
        assert_eq!(
            r.id_proyecto(),
            "MiguelM17-BranchBoard-Pruebas/BranchBoard-Pruebas"
        );
        assert_eq!(
            RepoUnityVc::desde_especificacion("VirusTF@MiguelM17@cloud")
                .unwrap()
                .nombre(),
            "VirusTF"
        );
        assert_eq!(
            RepoUnityVc::desde_especificacion("VirusTF@MIGUELMEZA:8087"),
            None
        );
        assert_eq!(
            RepoUnityVc::desde_especificacion("VirusTF@PC@localhost:8087"),
            None
        );
        assert_eq!(RepoUnityVc::desde_especificacion(""), None);
    }

    #[test]
    fn sin_datos_y_sin_ser_dueno_espera() {
        let datos = tempfile::tempdir().unwrap();
        assert_eq!(
            entrar_como(datos.path(), "luis@virustf.dev", false),
            Entrada::EsperarDueno {
                email: "luis@virustf.dev".into(),
                proyecto: "BranchBoard-Pruebas".into()
            }
        );
        let carpeta = carpeta_proyecto(datos.path(), &repo().id_proyecto()).unwrap();
        assert!(!carpeta.join(NOMBRE_DB).exists(), "no configuró nada");
    }

    #[test]
    fn el_dueno_lo_configura_y_queda_administrador() {
        let datos = tempfile::tempdir().unwrap();
        let (entrada, db) = entrar(
            datos.path(),
            &repo(),
            "Ana@VirusTF.dev",
            || Ok(true),
            AHORA,
            HOY,
        )
        .unwrap();
        assert_eq!(
            entrada,
            Entrada::Dentro {
                email: "ana@virustf.dev".into(),
                rol: Rol::Administrador,
                rol_verificado: true,
                proyecto: "BranchBoard-Pruebas".into(),
                configurado_ahora: true,
                vistos: BTreeMap::new(),
            }
        );
        let db = db.unwrap();
        assert_eq!(
            configuracion::leer(db.conexion(), CLAVE_NOMBRE_PROYECTO)
                .unwrap()
                .as_deref(),
            Some("BranchBoard-Pruebas")
        );
        drop(db);

        // La segunda vez ya tiene datos: entra sin configurar, y no pregunta si es dueño.
        let (otra, _) = entrar(
            datos.path(),
            &repo(),
            "ana@virustf.dev",
            || panic!("no pregunta"),
            AHORA,
            HOY,
        )
        .unwrap();
        assert!(matches!(
            otra,
            Entrada::Dentro {
                rol: Rol::Administrador,
                rol_verificado: true,
                configurado_ahora: false,
                ..
            }
        ));
    }

    #[test]
    fn registrado_pendiente_y_no_registrado() {
        let datos = tempfile::tempdir().unwrap();
        let (_, db) = entrar(
            datos.path(),
            &repo(),
            "ana@virustf.dev",
            || Ok(true),
            AHORA,
            HOY,
        )
        .unwrap();
        let db = db.unwrap();
        let carpeta = db.carpeta().unwrap().to_path_buf();
        let llave = llaves::cargar_llave_admin(&carpeta, None).unwrap();
        let fila = |email: &str, rol: Rol, firmada: bool| Usuario {
            email: email.into(),
            nombre: "N".into(),
            apellido: "A".into(),
            rol,
            fecha_alta: HOY.into(),
        };
        usuarios::insertar(
            db.conexion(),
            &fila("dev@virustf.dev", Rol::Desarrollador, true),
        )
        .unwrap();
        usuarios::insertar(
            db.conexion(),
            &fila("nuevo@virustf.dev", Rol::Tester, false),
        )
        .unwrap();
        drop(db);

        assert!(matches!(
            entrar_como(datos.path(), "dev@virustf.dev", false),
            Entrada::Dentro {
                rol: Rol::Desarrollador,
                rol_verificado: true,
                ..
            }
        ));
        assert!(matches!(
            entrar_como(datos.path(), "nuevo@virustf.dev", false),
            Entrada::Pendiente { .. }
        ));
        assert!(matches!(
            entrar_como(datos.path(), "otro@virustf.dev", false),
            Entrada::NoRegistrado { .. }
        ));
    }

    #[test]
    fn solo_se_entrega_la_base_si_entra() {
        let datos = tempfile::tempdir().unwrap();
        entrar(
            datos.path(),
            &repo(),
            "ana@virustf.dev",
            || Ok(true),
            AHORA,
            HOY,
        )
        .unwrap();
        let (_, db) = entrar(
            datos.path(),
            &repo(),
            "otro@virustf.dev",
            || Ok(false),
            AHORA,
            HOY,
        )
        .unwrap();
        assert!(db.is_none());
    }

    #[test]
    fn el_error_de_dueno_se_propaga() {
        let datos = tempfile::tempdir().unwrap();
        let e = entrar(
            datos.path(),
            &repo(),
            "ana@virustf.dev",
            || Err(Error::ServidorUnityVc("sin red".into())),
            AHORA,
            HOY,
        )
        .unwrap_err();
        assert_eq!(e.codigo(), "servidor_unity_vc");
    }
}
