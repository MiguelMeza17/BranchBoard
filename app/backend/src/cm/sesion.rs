//! Gestión de la sesión de Unity Version Control (`cm whoami`,
//! `plastic.exe --configure`, cierre de sesión, revocación de PAT y "Reconectar", #124).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::cm::{lectura, Comando};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::instalacion;
use crate::servidor::pat::{self, Llavero, LlaveroSistema};
use crate::servidor::{nombre_organizacion, region};

const TIEMPO_MAXIMO_ASISTENTE: Duration = Duration::from_secs(300);

pub const EVENTO_SESION_CADUCADA: &str = "sesion-unity-vc-caducada";

/// Estado actual de la sesión de Unity Version Control en esta máquina (#124).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EstadoSesionUnityVc {
    pub activa: bool,
    pub email: Option<String>,
    pub abrio_asistente: bool,
}

/// Resultado de intentar reconectar sesión y PAT con Unity VC (#124).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResultadoReconexionUnityVc {
    pub email: String,
    pub reconectado: bool,
    pub abrio_asistente: bool,
    pub mensaje: String,
}

fn candidatos_plastic_windows() -> Vec<PathBuf> {
    let mut rutas = Vec::new();
    if let Some(cm) = crate::cm::instalacion::resolver_cm() {
        if let Some(carpeta) = cm.parent() {
            rutas.push(carpeta.join("plastic.exe"));
        }
    }
    for fija in [
        r"C:\Program Files\PlasticSCM5\client\plastic.exe",
        r"C:\Program Files\Unity Version Control\plastic.exe",
    ] {
        let p = PathBuf::from(fija);
        if !rutas.contains(&p) {
            rutas.push(p);
        }
    }
    for sub in [
        ["PlasticSCM5", "client", "plastic.exe"].as_slice(),
        ["Unity Version Control", "plastic.exe"].as_slice(),
    ] {
        if let Some(pf) = instalacion::desde_env("ProgramFiles", sub) {
            if !rutas.contains(&pf) {
                rutas.push(pf);
            }
        }
    }
    rutas
}

/// Busca el asistente gráfico `plastic` en `candidatas` o `PATH`.
pub fn buscar_plastic_en(candidatas: &[PathBuf]) -> Option<PathBuf> {
    #[cfg(windows)]
    let en_path = &["plastic.exe"];
    #[cfg(not(windows))]
    let en_path = &["plastic"];

    instalacion::buscar_ejecutable(candidatas, en_path)
}

/// Ruta al ejecutable `plastic` (`plastic.exe --configure`), si existe.
pub fn resolver_plastic_exe() -> Option<PathBuf> {
    buscar_plastic_en(&candidatos_plastic_windows())
}

/// Argumentos para abrir Unity VC (`plastic.exe`) en el Branch Explorer del
/// workspace `workspace` (#207). `--branchexplorer=<ruta>` está documentado en
/// las notas de la versión 11 del cliente.
pub fn argumentos_branch_explorer(workspace: &Path) -> Vec<String> {
    vec![format!("--branchexplorer={}", workspace.display())]
}

/// Abre Unity VC en el Branch Explorer de `workspace` sin esperar a que la
/// persona lo cierre (#207): BranchBoard no se queda bloqueado mientras se
/// resuelve el conflicto. Devuelve la ruta de `plastic.exe` usada.
pub fn abrir_plastic_en_workspace(workspace: &Path) -> Resultado<PathBuf> {
    let ruta = resolver_plastic_exe().ok_or_else(|| {
        Error::Validacion(
            "No se encontró Unity Version Control (plastic) en este equipo para resolver el conflicto."
                .into(),
        )
    })?;
    let mut comando = std::process::Command::new(&ruta);
    comando
        .args(argumentos_branch_explorer(workspace))
        .current_dir(workspace);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        comando.creation_flags(DETACHED_PROCESS);
    }
    comando
        .spawn()
        .map_err(|e| Error::Interno(format!("No se pudo abrir Unity Version Control: {e}")))?;
    Ok(ruta)
}

/// Ruta de configuración local de Unity Version Control (`%LOCALAPPDATA%\plastic4` o `~/.plastic4`).
pub fn carpeta_config_plastic() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|b| PathBuf::from(b).join("plastic4"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".plastic4"))
    }
}

/// Consulta `cm whoami` sin abrir ningún asistente y sincroniza `AppState::usuario` (#124).
pub fn sesion_unity_vc(estado: &AppState) -> Resultado<EstadoSesionUnityVc> {
    match lectura::whoami(estado) {
        Ok(email) => {
            estado.fijar_usuario(Some(&email));
            Ok(EstadoSesionUnityVc {
                activa: true,
                email: Some(email),
                abrio_asistente: false,
            })
        }
        Err(Error::SinSesionUnityVc) => {
            estado.fijar_usuario(None);
            Ok(EstadoSesionUnityVc {
                activa: false,
                email: None,
                abrio_asistente: false,
            })
        }
        Err(e) => Err(e),
    }
}

/// Abre el inicio de sesión oficial de Unity Version Control (`plastic --configure`),
/// sin pedir ni recibir contraseñas en BranchBoard, y vuelve a correr `cm whoami` (#124).
pub fn iniciar_sesion_unity_vc(
    estado: &AppState,
    forzar_nueva: bool,
) -> Resultado<EstadoSesionUnityVc> {
    if !crate::cm::instalacion::estado_cm().instalado {
        return Err(Error::CmNoInstalado);
    }
    iniciar_sesion_unity_vc_con(
        forzar_nueva,
        || lectura::whoami(estado),
        || lanzar_plastic_configure(estado),
        |email| estado.fijar_usuario(email),
    )
}

fn lanzar_plastic_configure(estado: &AppState) -> Resultado<()> {
    // Las corridas automáticas pueden prohibir cualquier asistente interactivo.
    // No se compila en la aplicación publicada.
    #[cfg(test)]
    if std::env::var_os("BB_PRUEBA_SIN_ASISTENTE").is_some() {
        return Err(Error::Validacion(
            "La prueba automática no permite abrir el asistente de Unity VC".into(),
        ));
    }
    let ruta = resolver_plastic_exe().ok_or_else(|| {
        Error::Validacion(
            "No se encontró el asistente `plastic` de Unity Version Control para iniciar sesión."
                .into(),
        )
    })?;
    let salida = Comando::nuevo(&ruta.to_string_lossy())
        .arg("--configure")
        .tiempo_maximo(TIEMPO_MAXIMO_ASISTENTE)
        .ejecutar(estado)?;
    if salida.exito() {
        Ok(())
    } else {
        Err(Error::Cm(
            "El asistente de inicio de sesión de Unity VC no terminó correctamente.".into(),
        ))
    }
}

/// Núcleo testeable de [`iniciar_sesion_unity_vc`] (#124).
pub fn iniciar_sesion_unity_vc_con(
    forzar_nueva: bool,
    whoami_fn: impl Fn() -> Resultado<String>,
    abrir_asistente: impl FnOnce() -> Resultado<()>,
    fijar_usuario: impl Fn(Option<&str>),
) -> Resultado<EstadoSesionUnityVc> {
    if !forzar_nueva {
        match whoami_fn() {
            Ok(email) => {
                fijar_usuario(Some(&email));
                return Ok(EstadoSesionUnityVc {
                    activa: true,
                    email: Some(email),
                    abrio_asistente: false,
                });
            }
            Err(Error::SinSesionUnityVc) => {}
            Err(e) => return Err(e),
        }
    }

    abrir_asistente()?;
    match whoami_fn() {
        Ok(email) => {
            fijar_usuario(Some(&email));
            Ok(EstadoSesionUnityVc {
                activa: true,
                email: Some(email),
                abrio_asistente: true,
            })
        }
        Err(Error::SinSesionUnityVc) => {
            fijar_usuario(None);
            Err(Error::SinSesionUnityVc)
        }
        Err(e) => Err(e),
    }
}

/// Flujo de **"Reconectar"** (#159 / #124):
/// 1. Reintenta con la sesión actual (`cm whoami` + PAT en servidor si hay `org`).
/// 2. Si sigue fallando, abre `plastic.exe --configure`, reintenta `cm whoami` y
///    crea un PAT nuevo contra el servidor.
pub fn reconectar_unity_vc(
    estado: &AppState,
    org_opcional: Option<&str>,
) -> Resultado<ResultadoReconexionUnityVc> {
    if !crate::cm::instalacion::estado_cm().instalado {
        return Err(Error::CmNoInstalado);
    }
    let org = org_opcional
        .filter(|o| !o.trim().is_empty())
        .map(|o| nombre_organizacion(o).to_string())
        .or_else(|| estado.repo_abierto().ok().map(|r| r.org));

    reconectar_con(
        || lectura::whoami(estado),
        || lanzar_plastic_configure(estado),
        |email, reintento_forzado| {
            let Some(ref o) = org else {
                return Ok(());
            };
            // `whoami` es local y un PAT válido no acredita que el SSO de la
            // CLI funcione. Comprueba también una lectura remota de `cm` (#203).
            Comando::cm()
                .args(["repo", "list", &format!("{o}@unity")])
                .exigir_exito(estado)?;
            let cliente = if reintento_forzado {
                pat::renovar_pat_forzado(estado, &LlaveroSistema, email, o)?
            } else {
                pat::conectar(estado, &LlaveroSistema, email, o)?
            };
            cliente.listar_repositorios()?;
            Ok(())
        },
        |email| estado.fijar_usuario(email),
    )
}

/// Núcleo testeable de [`reconectar_unity_vc`].
pub fn reconectar_con(
    whoami_fn: impl Fn() -> Resultado<String>,
    abrir_asistente: impl FnOnce() -> Resultado<()>,
    verificar_o_renovar_pat: impl Fn(&str, bool) -> Resultado<()>,
    fijar_usuario: impl Fn(Option<&str>),
) -> Resultado<ResultadoReconexionUnityVc> {
    if let Ok(email) = whoami_fn() {
        if verificar_o_renovar_pat(&email, false).is_ok() {
            fijar_usuario(Some(&email));
            return Ok(ResultadoReconexionUnityVc {
                email,
                reconectado: true,
                abrio_asistente: false,
                mensaje: "Conexión con Unity Version Control verificada.".into(),
            });
        }
    }

    abrir_asistente()?;
    let email = whoami_fn()?;
    verificar_o_renovar_pat(&email, true)?;
    fijar_usuario(Some(&email));
    Ok(ResultadoReconexionUnityVc {
        email,
        reconectado: true,
        abrio_asistente: true,
        mensaje: "Sesión y token de acceso reconectados con Unity Version Control.".into(),
    })
}

/// Limpia `<SecurityConfig>…</SecurityConfig>` en `client.conf` sin tocar el resto de ajustes.
pub fn limpiar_security_config_xml(xml: &str) -> String {
    const INI: &str = "<SecurityConfig>";
    const FIN: &str = "</SecurityConfig>";
    let Some(pos_ini) = xml.find(INI) else {
        return xml.to_string();
    };
    let despues = &xml[pos_ini + INI.len()..];
    let Some(rel_fin) = despues.find(FIN) else {
        return xml.to_string();
    };
    let mut salida = String::with_capacity(xml.len());
    salida.push_str(&xml[..pos_ini + INI.len()]);
    salida.push_str(&xml[pos_ini + INI.len() + rel_fin..]);
    salida
}

/// Elimina las credenciales locales de sesión de Unity VC en `carpeta_plastic4` (#124).
pub fn limpiar_archivos_sesion(carpeta_plastic4: &Path) -> Resultado<()> {
    for archivo in ["tokens.conf", "cloudedition.token"] {
        let ruta = carpeta_plastic4.join(archivo);
        match std::fs::remove_file(&ruta) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(Error::Archivos(format!(
                    "No se pudo limpiar {}: {e}",
                    ruta.display()
                )))
            }
        }
    }

    let client_conf = carpeta_plastic4.join("client.conf");
    match std::fs::read_to_string(&client_conf) {
        Ok(contenido) => {
            let limpio = limpiar_security_config_xml(&contenido);
            if limpio != contenido {
                std::fs::write(&client_conf, limpio).map_err(|e| {
                    Error::Archivos(format!(
                        "No se pudo limpiar la sesión en {}: {e}",
                        client_conf.display()
                    ))
                })?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(Error::Archivos(format!(
                "No se pudo leer {}: {e}",
                client_conf.display()
            )))
        }
    }

    Ok(())
}

fn organizaciones_desde_perfiles(estado: &AppState) -> Vec<String> {
    let Ok(salida) = Comando::cm()
        .args(["profile", "list", "--format={server}"])
        .ejecutar(estado)
    else {
        return Vec::new();
    };
    if !salida.exito() {
        return Vec::new();
    }
    salida
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('*') && !l.eq_ignore_ascii_case("local"))
        .map(|s| nombre_organizacion(s).to_string())
        .filter(|o| !o.is_empty())
        .collect()
}

/// Cierra la sesión de Unity VC, revoca los PATs de BranchBoard en el servidor,
/// limpia el llavero y cierra el proyecto abierto en `AppState` (#124).
pub fn cerrar_sesion_unity_vc(estado: &AppState) -> Resultado<usize> {
    let carpeta = carpeta_config_plastic();
    cerrar_sesion_en(estado, &LlaveroSistema, carpeta.as_deref(), None)
}

/// Versión parametrizable de [`cerrar_sesion_unity_vc`] (para pruebas seguras).
pub fn cerrar_sesion_en(
    estado: &AppState,
    llavero: &dyn Llavero,
    carpeta_plastic4: Option<&Path>,
    org_extra: Option<&str>,
) -> Resultado<usize> {
    let email = estado
        .usuario_actual()
        .or_else(|| lectura::whoami(estado).ok());

    let mut organizaciones = Vec::new();
    if let Some(extra) = org_extra.filter(|o| !o.trim().is_empty()) {
        organizaciones.push(extra.to_string());
    }
    if let Ok(repo) = estado.repo_abierto() {
        organizaciones.push(repo.org);
    }
    organizaciones.extend(region::organizaciones_en_cloudregions());
    if crate::cm::instalacion::estado_cm().instalado {
        organizaciones.extend(organizaciones_desde_perfiles(estado));
    }

    let revocados = if let Some(ref correo) = email {
        pat::borrar_y_revocar_pats_usuario(estado, llavero, correo, &organizaciones)?
    } else {
        0
    };

    if let Some(carpeta) = carpeta_plastic4 {
        limpiar_archivos_sesion(carpeta)?;
    }

    estado.cerrar_proyecto();
    estado.fijar_usuario(None);
    Ok(revocados)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    #[test]
    fn el_branch_explorer_se_abre_con_la_ruta_del_workspace() {
        let ws = Path::new(r"C:\Users\ana\Proyectos\Mi Juego");
        assert_eq!(
            argumentos_branch_explorer(ws),
            [r"--branchexplorer=C:\Users\ana\Proyectos\Mi Juego".to_string()],
            "un solo argumento: Rust lo entrecomilla si la ruta tiene espacios"
        );
    }

    #[derive(Default)]
    struct LlaveroMemoria(RefCell<HashMap<String, String>>);

    impl Llavero for LlaveroMemoria {
        fn leer(&self, c: &str) -> Resultado<Option<String>> {
            Ok(self.0.borrow().get(c).cloned())
        }
        fn guardar(&self, c: &str, v: &str) -> Resultado<()> {
            self.0.borrow_mut().insert(c.into(), v.into());
            Ok(())
        }
        fn borrar(&self, c: &str) -> Resultado<()> {
            self.0.borrow_mut().remove(c);
            Ok(())
        }
    }

    #[test]
    fn iniciar_sesion_abre_asistente_si_no_hay_y_relee_whoami() {
        let llamadas_whoami = Cell::new(0);
        let abrio = Cell::new(false);
        let usuario = RefCell::new(None);

        let res = iniciar_sesion_unity_vc_con(
            false,
            || {
                let n = llamadas_whoami.get();
                llamadas_whoami.set(n + 1);
                if n == 0 {
                    Err(Error::SinSesionUnityVc)
                } else {
                    Ok("ana@equipo.dev".into())
                }
            },
            || {
                abrio.set(true);
                Ok(())
            },
            |u| *usuario.borrow_mut() = u.map(str::to_string),
        )
        .unwrap();

        assert!(res.activa);
        assert!(res.abrio_asistente);
        assert_eq!(res.email.as_deref(), Some("ana@equipo.dev"));
        assert_eq!(usuario.borrow().as_deref(), Some("ana@equipo.dev"));
    }

    #[test]
    fn reconectar_reintenta_y_si_falla_abre_configure_y_renueva_pat() {
        let abrio = Cell::new(false);
        let renovo = Cell::new(false);
        let usuario = RefCell::new(None);

        let res = reconectar_con(
            || {
                if abrio.get() {
                    Ok("ana@equipo.dev".into())
                } else {
                    Err(Error::SinSesionUnityVc)
                }
            },
            || {
                abrio.set(true);
                Ok(())
            },
            |_email, forzado| {
                if forzado {
                    renovo.set(true);
                    Ok(())
                } else {
                    Err(Error::TokenServidorInvalido)
                }
            },
            |u| *usuario.borrow_mut() = u.map(str::to_string),
        )
        .unwrap();

        assert!(abrio.get());
        assert!(renovo.get());
        assert!(res.reconectado && res.abrio_asistente);
        assert_eq!(usuario.borrow().as_deref(), Some("ana@equipo.dev"));
    }

    #[test]
    fn limpiar_archivos_deja_intactas_otras_claves_de_client_conf() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tokens.conf"), "secret").unwrap();
        std::fs::write(dir.path().join("cloudedition.token"), "secret2").unwrap();
        std::fs::write(
            dir.path().join("client.conf"),
            "<ClientConfigData>\n  <WorkingMode>SSOWorkingMode</WorkingMode>\n  <SecurityConfig>ana@equipo.dev</SecurityConfig>\n</ClientConfigData>\n",
        )
        .unwrap();

        limpiar_archivos_sesion(dir.path()).unwrap();

        assert!(!dir.path().join("tokens.conf").exists());
        assert!(!dir.path().join("cloudedition.token").exists());
        let conf = std::fs::read_to_string(dir.path().join("client.conf")).unwrap();
        assert!(conf.contains("<WorkingMode>SSOWorkingMode</WorkingMode>"));
        assert!(conf.contains("<SecurityConfig></SecurityConfig>"));
        assert!(!conf.contains("ana@equipo.dev"));
    }

    #[test]
    fn reconectar_no_restaura_identidad_si_la_cli_sigue_caducada() {
        let abrio = Cell::new(0);
        let identidades_restauradas = Cell::new(0);
        let error = reconectar_con(
            || Ok("dev@example.test".into()), // whoami local sigue funcionando.
            || {
                abrio.set(abrio.get() + 1);
                Ok(())
            },
            |_, _| Err(Error::SesionUnityVcCaducada),
            |email| {
                if email.is_some() {
                    identidades_restauradas.set(identidades_restauradas.get() + 1);
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.codigo(), "sesion_unity_vc_caducada");
        assert_eq!(abrio.get(), 1);
        assert_eq!(identidades_restauradas.get(), 0);
    }

    #[test]
    fn cerrar_sesion_limpia_llavero_plastic4_y_estado() {
        let estado = AppState::default();
        estado.abrir_proyecto(crate::db::Db::en_memoria().unwrap());
        estado.fijar_usuario(Some("ana@equipo.dev"));
        let llavero = LlaveroMemoria::default();
        let cuenta = pat::cuenta_llavero("ana@equipo.dev", "OrgPrueba");
        llavero
            .guardar(
                &cuenta,
                r#"{"id":"11111111-1111-1111-1111-111111111111","token":"secret"}"#,
            )
            .unwrap();
        let plastic_dir = tempfile::tempdir().unwrap();
        std::fs::write(plastic_dir.path().join("tokens.conf"), "tok").unwrap();

        let revocados = cerrar_sesion_en(
            &estado,
            &llavero,
            Some(plastic_dir.path()),
            Some("OrgPrueba"),
        )
        .unwrap();

        assert_eq!(revocados, 1);
        assert_eq!(llavero.leer(&cuenta).unwrap(), None);
        assert!(!plastic_dir.path().join("tokens.conf").exists());
        assert_eq!(estado.usuario_actual(), None);
        assert!(estado.con_db(|db| db.version()).is_err());
    }

    /// **Prueba en el entorno real** (CLAUDE.md):
    /// 1. Detecta `plastic.exe` real (`resolver_plastic_exe`) y confirma sesión real (`sesion_unity_vc`).
    /// 2. Crea un PAT real con `servidor::conectar` para `BB_PRUEBA_ORG` y comprueba la API contra `BranchBoard-Pruebas`.
    /// 3. Ejerce `reconectar_unity_vc` verificando sesión y PAT contra el servidor real.
    /// 4. Ejerce `cerrar_sesion_en` con el llavero real y una carpeta `plastic4` aislada (sin cerrar la sesión SSO interactiva del equipo),
    ///    comprobando que el PAT quedó eliminado del llavero y revocado en `cm accesstoken list`, que `tokens.conf`/`client.conf` quedaron limpios y `AppState` cerrado.
    #[test]
    #[ignore = "usa Unity VC real y llavero del sistema: correr a mano con BB_PRUEBA_ORG"]
    fn entorno_real_sesion_unity_vc() {
        let org = std::env::var("BB_PRUEBA_ORG").unwrap_or_else(|_| "MiguelM17".into());
        let estado = AppState::default();

        let plastic = resolver_plastic_exe().expect("plastic.exe debe estar instalado");
        println!("plastic.exe detectado en: {}", plastic.display());

        let s = sesion_unity_vc(&estado).unwrap();
        assert!(s.activa, "debe haber sesión activa en cm whoami");
        let email = s.email.clone().unwrap();

        let llavero = LlaveroSistema;
        let cuenta = pat::cuenta_llavero(&email, &org);
        // Asegurar creación fresca del PAT en Unity VC Cloud.
        let _ = pat::borrar_y_revocar_pats_usuario(
            &estado,
            &llavero,
            &email,
            std::slice::from_ref(&org),
        );
        let cliente = pat::conectar(&estado, &llavero, &email, &org).unwrap();
        let repos = cliente.listar_repositorios().unwrap();
        assert!(repos
            .iter()
            .any(|r| r.name.starts_with("BranchBoard-Pruebas")));
        let guardado: pat::Pat =
            serde_json::from_str(&llavero.leer(&cuenta).unwrap().unwrap()).unwrap();
        let id_creado = guardado.id.clone();

        // Reconectar reintenta sesión + PAT real sin abrir UI interactiva.
        let rec = reconectar_unity_vc(&estado, Some(&org)).unwrap();
        assert!(rec.reconectado);
        assert!(!rec.abrio_asistente);
        assert_eq!(rec.email, email);

        // Cierre de sesión seguro sobre carpeta `plastic4` aislada + llavero real + `cm accesstoken revoke` real.
        let plastic_tmp = tempfile::tempdir().unwrap();
        std::fs::write(plastic_tmp.path().join("tokens.conf"), "prueba").unwrap();
        std::fs::write(plastic_tmp.path().join("cloudedition.token"), "prueba").unwrap();
        std::fs::write(
            plastic_tmp.path().join("client.conf"),
            format!(
                "<ClientConfigData>\n  <WorkingMode>SSOWorkingMode</WorkingMode>\n  <SecurityConfig>{email}</SecurityConfig>\n</ClientConfigData>\n"
            ),
        )
        .unwrap();

        let revocados =
            cerrar_sesion_en(&estado, &llavero, Some(plastic_tmp.path()), Some(&org)).unwrap();
        assert!(revocados >= 1);
        assert_eq!(llavero.leer(&cuenta).unwrap(), None);
        assert!(!plastic_tmp.path().join("tokens.conf").exists());
        assert!(!plastic_tmp.path().join("cloudedition.token").exists());
        let conf_limpio = std::fs::read_to_string(plastic_tmp.path().join("client.conf")).unwrap();
        assert!(conf_limpio.contains("<SecurityConfig></SecurityConfig>"));
        assert_eq!(estado.usuario_actual(), None);

        // Verificar que el id revocado ya no aparece en `cm accesstoken list <org>@unity`.
        let org_cm = format!("{}@unity", nombre_organizacion(&org));
        let tokens_restantes = Comando::cm()
            .args(["accesstoken", "list", &org_cm])
            .exigir_exito(&estado)
            .unwrap();
        assert!(
            !tokens_restantes.contains(&id_creado),
            "el PAT {id_creado} debió revocarse: {tokens_restantes}"
        );
        println!("Sesión, Reconectar y revocación de PAT ({id_creado}) verificados para {email}");
    }
}
