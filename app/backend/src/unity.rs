//! Localización de Unity Editor / Unity Hub y cambio previo de workspace (#112).
//!
//! Es estrictamente local a esta máquina (no emite eventos por `ops/`).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::cm::lectura::{rama_actual_workspace, EstadoWorkspace};
use crate::db::{commits, tareas as db_tareas, Db};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::modelo::TipoTarea;
use crate::ramas;

/// Ejecutable elegido para abrir el proyecto en Unity (#112).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanzadorUnity {
    pub ejecutable: PathBuf,
    pub es_hub: bool,
}

/// Respuesta devuelta al frontend por `abrir_en_unity` (#112).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResultadoAbrirUnity {
    pub rama_actual: String,
    pub changeset: Option<i64>,
    pub ejecutable: String,
    pub usa_hub: bool,
}

/// Lee `m_EditorVersion` de `<workspace>/ProjectSettings/ProjectVersion.txt` si existe.
pub fn version_editor_proyecto(workspace: &Path) -> Option<String> {
    let texto =
        std::fs::read_to_string(workspace.join("ProjectSettings").join("ProjectVersion.txt"))
            .ok()?;
    for linea in texto.lines() {
        if let Some(resto) = linea.trim().strip_prefix("m_EditorVersion:") {
            let ver = resto.trim();
            if !ver.is_empty() && !ver.contains('/') && !ver.contains('\\') && !ver.contains("..") {
                return Some(ver.to_string());
            }
        }
    }
    None
}

fn ejecutable_en_carpeta_editor(carpeta_version: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let candidatos = [carpeta_version.join("Editor").join("Unity.exe")];
    #[cfg(target_os = "macos")]
    let candidatos = [carpeta_version
        .join("Unity.app")
        .join("Contents")
        .join("MacOS")
        .join("Unity")];
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let candidatos = [carpeta_version.join("Editor").join("Unity")];

    candidatos.into_iter().find(|p| p.is_file())
}

/// Busca Unity Editor (priorizando la versión declarada por el proyecto) y como respaldo
/// Unity Hub entre las rutas proporcionadas. Falla con mensaje claro si no está instalado.
pub fn buscar_lanzador_en(
    version_proyecto: Option<&str>,
    candidatos_editor: &[PathBuf],
    carpetas_hub_editors: &[PathBuf],
    candidatos_hub: &[PathBuf],
) -> Resultado<LanzadorUnity> {
    if let Some(ver) = version_proyecto.map(str::trim).filter(|v| !v.is_empty()) {
        for base in carpetas_hub_editors {
            if let Some(exe) = ejecutable_en_carpeta_editor(&base.join(ver)) {
                return Ok(LanzadorUnity {
                    ejecutable: exe,
                    es_hub: false,
                });
            }
        }
    }

    for ruta in candidatos_editor {
        if ruta.is_file() {
            return Ok(LanzadorUnity {
                ejecutable: ruta.clone(),
                es_hub: false,
            });
        }
    }

    let mut instalados: Vec<PathBuf> = Vec::new();
    for base in carpetas_hub_editors {
        let Ok(entradas) = std::fs::read_dir(base) else {
            continue;
        };
        for ent in entradas.flatten() {
            if ent.path().is_dir() {
                if let Some(exe) = ejecutable_en_carpeta_editor(&ent.path()) {
                    instalados.push(exe);
                }
            }
        }
    }
    instalados.sort();
    if let Some(ultimo) = instalados.pop() {
        return Ok(LanzadorUnity {
            ejecutable: ultimo,
            es_hub: false,
        });
    }

    for hub in candidatos_hub {
        if hub.is_file() {
            return Ok(LanzadorUnity {
                ejecutable: hub.clone(),
                es_hub: true,
            });
        }
    }

    Err(Error::Validacion(
        "No se encontró Unity Editor ni Unity Hub instalados en esta máquina. Instala Unity desde Unity Hub para abrir el proyecto.".into(),
    ))
}

fn rutas_estandares_unity() -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut editores = Vec::new();
    let mut carpetas_hub = Vec::new();
    let mut hubs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Ok(base) = std::env::var(var) {
                let p = PathBuf::from(base);
                carpetas_hub.push(p.join("Unity").join("Hub").join("Editor"));
                editores.push(p.join("Unity").join("Editor").join("Unity.exe"));
                hubs.push(p.join("Unity Hub").join("Unity Hub.exe"));
                hubs.push(p.join("Programs").join("Unity Hub").join("Unity Hub.exe"));
            }
        }
        editores.push(PathBuf::from(r"C:\Program Files\Unity\Editor\Unity.exe"));
        carpetas_hub.push(PathBuf::from(r"C:\Program Files\Unity\Hub\Editor"));
        hubs.push(PathBuf::from(r"C:\Program Files\Unity Hub\Unity Hub.exe"));
    }

    #[cfg(target_os = "macos")]
    {
        carpetas_hub.push(PathBuf::from("/Applications/Unity/Hub/Editor"));
        editores.push(PathBuf::from(
            "/Applications/Unity/Unity.app/Contents/MacOS/Unity",
        ));
        hubs.push(PathBuf::from(
            "/Applications/Unity Hub.app/Contents/MacOS/Unity Hub",
        ));
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let h = PathBuf::from(home);
            carpetas_hub.push(h.join("Unity").join("Hub").join("Editor"));
        }
        editores.push(PathBuf::from("/usr/bin/unity-editor"));
        hubs.push(PathBuf::from("/usr/bin/unityhub"));
    }

    (editores, carpetas_hub, hubs)
}

/// Busca el ejecutable de Unity Editor o Unity Hub para `workspace` en el sistema actual.
pub fn buscar_lanzador(workspace: &Path) -> Resultado<LanzadorUnity> {
    let version = version_editor_proyecto(workspace);
    let (editores, carpetas_hub, hubs) = rutas_estandares_unity();
    buscar_lanzador_en(version.as_deref(), &editores, &carpetas_hub, &hubs)
}

fn parse_changeset_opcional(changeset: Option<&str>) -> Resultado<Option<i64>> {
    let Some(raw) = changeset.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let limpio = raw
        .strip_prefix("cs:")
        .or_else(|| raw.strip_prefix("CS:"))
        .unwrap_or(raw)
        .trim();
    let num: i64 = limpio
        .parse()
        .map_err(|_| Error::Validacion(format!("El changeset \"{raw}\" no es válido")))?;
    if num <= 0 {
        return Err(Error::Validacion(
            "El número de changeset debe ser positivo".into(),
        ));
    }
    Ok(Some(num))
}

fn resolver_rama_tarea(c: &rusqlite::Connection, tarea: &db_tareas::Tarea) -> Resultado<String> {
    if let Some(ref rama) = tarea.rama_asociada {
        return Ok(rama.clone());
    }
    if tarea.tipo == TipoTarea::SinDefinir {
        if let Some(ref padre_id) = tarea.tarea_padre_id {
            if let Ok(padre) = db_tareas::obtener(c, padre_id) {
                if let Some(rama_padre) = padre.rama_asociada {
                    return Ok(rama_padre);
                }
            }
        }
    }
    Err(Error::Validacion(format!(
        "La tarea {} no tiene rama asociada",
        tarea.id
    )))
}

fn normalizar_estado_apertura(
    mut estado_final: EstadoWorkspace,
    rama_tarea: &str,
    cs_solicitado: Option<i64>,
) -> EstadoWorkspace {
    if let Some(cs) = cs_solicitado {
        if estado_final.rama.to_ascii_lowercase().starts_with("cs:") {
            estado_final.rama = rama_tarea.to_string();
        }
        estado_final.changeset = Some(cs);
    } else {
        // Apertura por rama (head): no fija un changeset puntual en la UI.
        estado_final.changeset = None;
    }
    estado_final
}

/// Valida tarea/rama/changeset, localiza Unity instalado y cambia el workspace a la rama
/// o changeset solicitado sin lanzar aún la ventana GUI (#112).
pub fn preparar_apertura(
    estado: &AppState,
    db: &Db,
    workspace: &Path,
    tarea_id: &str,
    rama: &str,
    changeset: Option<&str>,
) -> Resultado<(LanzadorUnity, EstadoWorkspace)> {
    let tid = tarea_id.trim();
    let rama_limpia = rama.trim().trim_end_matches('/');
    if tid.is_empty() || rama_limpia.is_empty() {
        return Err(Error::Validacion(
            "Indica la tarea y su rama para abrir en Unity".into(),
        ));
    }

    let tarea = db_tareas::obtener(db.conexion(), tid)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    let rama_tarea = resolver_rama_tarea(db.conexion(), &tarea)?;
    if !rama_tarea.eq_ignore_ascii_case(rama_limpia) {
        return Err(Error::Validacion(format!(
            "La rama {rama_limpia} no corresponde a la tarea {} ({rama_tarea})",
            tarea.id
        )));
    }
    let cs_num = parse_changeset_opcional(changeset)?;
    if let Some(cs) = cs_num {
        let cs_txt = cs.to_string();
        let de_la_tarea = commits::de_tarea(db.conexion(), &tarea.id)?;
        let existe_en_copia = de_la_tarea.iter().any(|c| c.hash == cs_txt);
        if !de_la_tarea.is_empty() && !existe_en_copia {
            return Err(Error::Validacion(format!(
                "El changeset {cs} no pertenece a la tarea {}",
                tarea.id
            )));
        }
    }

    let lanzador = buscar_lanzador(workspace)?;
    let actual = rama_actual_workspace(estado, workspace)?;

    let estado_final = if let Some(cs) = cs_num {
        if actual.changeset == Some(cs) {
            actual
        } else {
            ramas::cambiar_a_changeset(estado, workspace, cs)?
        }
    } else if actual.rama.eq_ignore_ascii_case(&rama_tarea) {
        actual
    } else {
        ramas::cambiar_a_rama(estado, workspace, &rama_tarea)?
    };

    Ok((
        lanzador,
        normalizar_estado_apertura(estado_final, &rama_tarea, cs_num),
    ))
}

/// Lanza Unity Editor o Unity Hub apuntando a `workspace` (#112).
pub fn lanzar_unity(lanzador: &LanzadorUnity, workspace: &Path) -> Resultado<()> {
    let mut cmd = Command::new(&lanzador.ejecutable);
    if lanzador.es_hub {
        cmd.args(["--", "--projectPath"]).arg(workspace);
    } else {
        cmd.arg("-projectPath").arg(workspace);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    cmd.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::modelo::TipoTarea;

    #[test]
    fn detectar_version_y_buscar_lanzador_prioriza_editor_exacto_y_falla_claro() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        std::fs::create_dir_all(ws.join("ProjectSettings")).unwrap();
        std::fs::write(
            ws.join("ProjectSettings").join("ProjectVersion.txt"),
            "m_EditorVersion: 6000.4.8f1\nm_EditorVersionWithRevision: 6000.4.8f1 (abc)\n",
        )
        .unwrap();
        assert_eq!(version_editor_proyecto(&ws).as_deref(), Some("6000.4.8f1"));

        // Sin nada instalado -> error claro.
        let err = buscar_lanzador_en(Some("6000.4.8f1"), &[], &[], &[]).unwrap_err();
        assert_eq!(err.codigo(), "validacion");
        assert!(err.to_string().contains("Unity"));

        // Solo Hub -> cae al Hub.
        let hub = tmp.path().join("Unity Hub.exe");
        std::fs::write(&hub, b"").unwrap();
        let solo_hub =
            buscar_lanzador_en(Some("6000.4.8f1"), &[], &[], std::slice::from_ref(&hub)).unwrap();
        assert!(solo_hub.es_hub);

        // Editor en Hub/Editor/<version> gana sobre otro editor y sobre Hub.
        let base_hub = tmp.path().join("HubEditors");
        let exe_6000 = {
            #[cfg(target_os = "windows")]
            {
                base_hub.join("6000.4.8f1").join("Editor").join("Unity.exe")
            }
            #[cfg(target_os = "macos")]
            {
                base_hub
                    .join("6000.4.8f1")
                    .join("Unity.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Unity")
            }
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            {
                base_hub.join("6000.4.8f1").join("Editor").join("Unity")
            }
        };
        std::fs::create_dir_all(exe_6000.parent().unwrap()).unwrap();
        std::fs::write(&exe_6000, b"").unwrap();

        let exacto = buscar_lanzador_en(
            Some("6000.4.8f1"),
            &[],
            std::slice::from_ref(&base_hub),
            &[hub],
        )
        .unwrap();
        assert!(!exacto.es_hub);
        assert_eq!(exacto.ejecutable, exe_6000);
    }

    #[test]
    fn validaciones_de_changeset_y_rama() {
        assert_eq!(parse_changeset_opcional(None).unwrap(), None);
        assert_eq!(parse_changeset_opcional(Some("  ")).unwrap(), None);
        assert_eq!(parse_changeset_opcional(Some("42")).unwrap(), Some(42));
        assert_eq!(parse_changeset_opcional(Some("cs:42")).unwrap(), Some(42));
        assert!(parse_changeset_opcional(Some("0")).is_err());
        assert!(parse_changeset_opcional(Some("abc")).is_err());

        let tmp = TempDir::new().unwrap();
        let db = Db::en_memoria().unwrap();
        db_tareas::insertar(
            db.conexion(),
            &db_tareas::Tarea::nueva(
                "T-101",
                "Sin rama",
                TipoTarea::Scripting,
                "admin@estudio.com",
                "2026-11-16",
            ),
        )
        .unwrap();
        let estado_app = AppState::default();
        assert!(preparar_apertura(
            &estado_app,
            &db,
            tmp.path(),
            "T-101",
            "/main/dev/T-101",
            None
        )
        .is_err());
    }

    #[test]
    fn resuelve_rama_de_espera_y_normaliza_estado_de_apertura() {
        let db = Db::en_memoria().unwrap();
        let mut padre = db_tareas::Tarea::nueva(
            "T-200",
            "Padre",
            TipoTarea::Scripting,
            "admin@estudio.com",
            "2026-11-16",
        );
        padre.rama_asociada = Some("/main/dev/T-200-padre".into());
        db_tareas::insertar(db.conexion(), &padre).unwrap();

        let mut espera = db_tareas::Tarea::nueva(
            "T-200-1",
            "En espera",
            TipoTarea::SinDefinir,
            "admin@estudio.com",
            "2026-11-16",
        );
        espera.tarea_padre_id = Some("T-200".into());
        db_tareas::insertar(db.conexion(), &espera).unwrap();

        assert_eq!(
            resolver_rama_tarea(db.conexion(), &espera).unwrap(),
            "/main/dev/T-200-padre"
        );

        let por_rama = normalizar_estado_apertura(
            EstadoWorkspace {
                rama: "/main/dev/T-200-padre".into(),
                repositorio: "repo@org@unity".into(),
                changeset: Some(42),
            },
            "/main/dev/T-200-padre",
            None,
        );
        assert_eq!(por_rama.rama, "/main/dev/T-200-padre");
        assert_eq!(por_rama.changeset, None);

        let por_cs = normalizar_estado_apertura(
            EstadoWorkspace {
                rama: "cs:41".into(),
                repositorio: "repo@org@unity".into(),
                changeset: Some(41),
            },
            "/main/dev/T-200-padre",
            Some(41),
        );
        assert_eq!(por_cs.rama, "/main/dev/T-200-padre");
        assert_eq!(por_cs.changeset, Some(41));
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #112): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity`:
    /// 1. Entra al proyecto, crea una tarea, la toma (`tomar_en`), agrega dos check-ins
    ///    reales (`hacer_checkin_en`) y vuelve a `/main/dev` (`volver_a_dev_en`).
    /// 2. Ejecuta `abrir_en_unity_preparar_en` apuntando a la rama -> cambia con
    ///    `cm switch` real y localiza el ejecutable real de Unity (`Unity.exe` o `Unity Hub`).
    /// 3. Ejecuta `abrir_en_unity_preparar_en` apuntando al changeset #1 -> cambia el
    ///    workspace real exactamente a `cs:<cs1>`.
    /// 4. Comprueba `listar_commits_en` con los check-ins sincronizados, limpia cambiando a
    ///    `/main/dev`, borrando workspace y borrando ramas con `--delete-changesets`.
    #[test]
    #[ignore = "usa Unity VC real (y Unity Editor/Hub instalado): correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_abrir_en_unity() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::tareas::{
            abrir_en_unity_preparar_en, hacer_checkin_en, listar_commits_en, tomar_en,
        };
        
        use crate::modelo::{ahora_iso, hoy_local, Rol};
        use crate::tareas::{crear, AutorTarea, DatosNuevaTarea};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let repo_cm = format!("{repo}@{org}@unity");

        let estado = AppState::default();
        let ws = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let compartida = tempfile::tempdir().unwrap();
        let ruta_ws = ws.path().to_string_lossy().to_string();
        let mut ramas_creadas = Vec::new();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-a2-112-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo_cm}"))
            .exigir_exito(&estado)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado, datos.path(), ws.path()).unwrap();

            let admin = estado.exigir_permiso("tomar_tarea").unwrap().email;
            let tarea = estado
                .con_db(|db| {
                    crear(
                        db.conexion(),
                        &AutorTarea {
                            email: &admin,
                            rol: Rol::Administrador,
                        },
                        &DatosNuevaTarea {
                            titulo: format!("A2 Abrir Unity 112 {sufijo}"),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        &ahora_iso(),
                        &hoy_local(),
                    )
                })
                .unwrap();

            let tomadas = tomar_en(&estado, &tarea.id).unwrap();
            let rama = tomadas[0].rama_asociada.clone().unwrap();
            ramas_creadas.push(rama.clone());

            // Hacemos 2 check-ins reales en esa rama.
            for n in [1, 2] {
                let archivo = ws.path().join(format!("unity_112_{sufijo}_{n}.txt"));
                std::fs::write(&archivo, format!("check-in {n}")).unwrap();
                Comando::cm()
                    .arg("add")
                    .arg(archivo.to_string_lossy().as_ref())
                    .en(ws.path())
                    .exigir_exito(&estado)
                    .unwrap();
                hacer_checkin_en(
                    &estado,
                    &rama,
                    &format!("Check-in {n}"),
                    "",
                    TipoTarea::Scripting,
                )
                .unwrap();
            }

            let commits_tarea = listar_commits_en(&estado, Some(&tarea.id)).unwrap();
            assert_eq!(commits_tarea.len(), 2);
            let cs_primero = commits_tarea[0].hash.clone();

            // Volvemos a dev primero para verificar el switch real desde otra rama.
            ramas::cambiar_a_rama(&estado, ws.path(), ramas::RAMA_DEV).unwrap();
            assert_eq!(
                rama_actual_workspace(&estado, ws.path()).unwrap().rama,
                ramas::RAMA_DEV
            );

            // 1. Abrir por rama -> cambia workspace a `rama` y detecta Unity instalado.
            let res_rama = abrir_en_unity_preparar_en(&estado, &tarea.id, &rama, None).unwrap();
            println!(
                "Unity detectado para #112: {} (hub={})",
                res_rama.ejecutable, res_rama.usa_hub
            );
            assert!(Path::new(&res_rama.ejecutable).is_file());
            assert!(res_rama.rama_actual.eq_ignore_ascii_case(&rama));
            assert_eq!(res_rama.changeset, None);

            // 2. Abrir en changeset puntual -> workspace cambia exactamente a `cs_primero`
            //    y devuelve la rama real de la tarea (no "cs:<n>").
            let res_cs =
                abrir_en_unity_preparar_en(&estado, &tarea.id, &rama, Some(&cs_primero)).unwrap();
            assert_eq!(res_cs.changeset, cs_primero.parse::<i64>().ok());
            assert!(res_cs.rama_actual.eq_ignore_ascii_case(&rama));

            // 3. Volver a abrir por rama desde el changeset puntual -> regresa a head.
            let res_vuelta = abrir_en_unity_preparar_en(&estado, &tarea.id, &rama, None).unwrap();
            assert!(res_vuelta.rama_actual.eq_ignore_ascii_case(&rama));
            assert_eq!(res_vuelta.changeset, None);

            // Regresamos a dev antes de borrar el workspace y las ramas de prueba.
            ramas::cambiar_a_rama(&estado, ws.path(), ramas::RAMA_DEV).unwrap();
        }));

        estado.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado);
        for rama in ramas_creadas.iter().rev() {
            let salida = Comando::cm()
                .args(["branch", "delete", "--delete-changesets"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .ejecutar(&estado)
                .unwrap();
            println!("limpieza rama {rama}: {}", salida.exito());
        }
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
