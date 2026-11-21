//! Entrar a un proyecto (issue #89). Las reglas viven en `identidad::acceso`;
//! aquí se juntan `cm whoami`, el repositorio del workspace, el dueño del repo
//! (por la API de Unity VC) y la carpeta de datos de la app.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::cm::lectura::{rama_actual_workspace, whoami};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::{self, Entrada, RepoUnityVc};
use crate::modelo::{ahora_iso, hoy_local, normalizar_email, Rol};
use crate::servidor::{conectar, LlaveroSistema};
use crate::supabase::proyectos::{self, ProyectoDeLaCuenta, Vinculo};
use crate::supabase::{login_unity, ClienteSupabase, Entorno};

/// `entrar_a_proyecto(carpeta)`: libre (todavía no hay rol). Si la persona
/// entra, deja abierta la base del proyecto y queda identificada; si no, no
/// queda ningún proyecto abierto.
#[tauri::command]
pub async fn entrar_a_proyecto(
    app: AppHandle,
    estado: State<'_, AppState>,
    carpeta: String,
) -> Resultado<Entrada> {
    let datos = carpeta_datos(&app)?;
    entrar_en(&estado, &datos, Path::new(&carpeta))
}

/// `%LocalAppData%\BranchBoard` (ver "Base de datos local" en BACKEND.md).
pub fn carpeta_datos(app: &AppHandle) -> Resultado<PathBuf> {
    let base = app
        .path()
        .local_data_dir()
        .map_err(|e| Error::Archivos(e.to_string()))?;
    Ok(base.join("BranchBoard"))
}

/// Lo mismo que el comando, sin Tauri (para la prueba en el entorno real).
pub fn entrar_en(estado: &AppState, datos: &Path, workspace: &Path) -> Resultado<Entrada> {
    entrar_con(estado, datos, workspace, es_dueno_del_repo)
}

/// Como [`entrar_en`], pero sin preguntarle a la API quién es el dueño: quien
/// entra configura el proyecto como Administrador si todavía no tiene datos.
/// Solo para las pruebas en el entorno real (#107): así las puede correr
/// cualquiera de los dos desarrolladores, no solo el dueño de la organización.
/// El dueño de verdad se prueba aparte en `entorno_real_entrar_a_proyecto`.
pub fn entrar_como_dueno_en(
    estado: &AppState,
    datos: &Path,
    workspace: &Path,
) -> Resultado<Entrada> {
    entrar_con(estado, datos, workspace, |_, _, _| Ok(true))
}

fn entrar_con(
    estado: &AppState,
    datos: &Path,
    workspace: &Path,
    es_dueno: impl FnOnce(&AppState, &RepoUnityVc, &str) -> Resultado<bool>,
) -> Resultado<Entrada> {
    estado.cerrar_proyecto();
    let email = whoami(estado)?;
    estado.fijar_usuario(Some(&email));
    let especificacion = rama_actual_workspace(estado, workspace)?.repositorio;
    let repo = RepoUnityVc::desde_especificacion(&especificacion)
        .ok_or_else(|| acceso::error_repo_no_cloud(&especificacion))?;
    let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
    let (entrada, db) = acceso::entrar_con_servidor(
        datos,
        &repo,
        &email,
        || es_dueno(estado, &repo, &email),
        || buscar_en_servidor(estado, &cliente, &repo, &email),
        |db, proyecto| {
            login_unity::asegurar_sesion_servidor_del_equipo(estado, &email)?;
            refrescar_equipo(&cliente, db, proyecto)
        },
        &ahora_iso(),
        &hoy_local(),
    )?;
    if let Some(db) = db {
        estado.abrir_proyecto(db);
        estado.fijar_workspace(Some(workspace.to_path_buf()), Some(repo));
        let _ = crate::comandos::sprints::evaluar_cierres_en(estado, &email)?;
        // Vincular con el servidor (#240) no frena la entrada: sin internet o sin sesión se reintenta la
        // próxima vez que se entre (o con `vincular_proyecto_servidor`).
        let _ = vincular_proyecto_servidor_en(estado);
    }
    Ok(entrada)
}

/// El id del proyecto de `repo` en el servidor si la cuenta es miembro (#240). Sin internet, sin sesión
/// o sin el proyecto: `None` (la entrada sigue como siempre).
fn buscar_en_servidor<L: crate::servidor::pat::Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    repo: &RepoUnityVc,
    email: &str,
) -> Option<String> {
    login_unity::asegurar_sesion_servidor_del_equipo(estado, email).ok()?;
    cliente
        .llamar(
            "GET",
            &proyectos::ruta_buscar_unity(&repo.org, &repo.repo),
            None,
        )
        .ok()
        .and_then(|j| proyectos::id_desde_json(&j))
}

/// Rehace la copia local de `miembros` del `proyecto` en `db` (#241).
pub(crate) fn refrescar_equipo<L: crate::servidor::pat::Llavero>(
    cliente: &ClienteSupabase<L>,
    db: &crate::db::Db,
    proyecto: &str,
) -> Resultado<()> {
    crate::identidad::equipo_servidor::refrescar(
        &crate::identidad::equipo_servidor::EnSupabase { cliente, proyecto },
        |filas| crate::db::usuarios::reemplazar_todos(db.conexion(), filas),
    )
    .map(|_| ())
}

/// Vincula el proyecto de Unity VC abierto con su proyecto del servidor (#240, ver
/// `supabase::proyectos`). Si el servidor todavía no lo tiene, solo lo trae el Administrador local (la
/// Edge Function además revisa que sea el dueño del repo).
///
/// Una base local con equipo (más de un usuario) no se vincula aquí, ni trayéndolo ni encontrándolo: con
/// #241 la copia local de `miembros` dejaría afuera a quien no esté en el servidor. Esos proyectos se
/// suben con "Subir al servidor los proyectos que ya existen" (#245).
pub fn vincular_proyecto_servidor_en(estado: &AppState) -> Resultado<Vinculo> {
    let yo = estado.exigir_permiso("vincular_proyecto_servidor")?;
    let repo = estado.repo_abierto()?;
    let equipo_local =
        estado.con_db(|db| crate::db::usuarios::listar(db.conexion()).map(|u| u.len()))?;
    login_unity::asegurar_sesion_servidor_del_equipo(estado, &yo.email)?;
    let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
    proyectos::vincular_unity(
        |f| estado.con_db(|db| f(db.conexion())),
        equipo_local,
        yo.rol == Rol::Administrador,
        &yo.email,
        &hoy_local(),
        || {
            cliente
                .llamar(
                    "GET",
                    &proyectos::ruta_buscar_unity(&repo.org, &repo.repo),
                    None,
                )
                .map(|j| proyectos::id_desde_json(&j))
        },
        || traer_repo(estado, &cliente, &repo, &yo.email),
    )
}

/// Pide a `vincular-repo-unity` que traiga el repo a BranchBoard con el token de Unity de `tokens.conf`.
/// Si Unity rechaza el token (venció), lo renueva con `cm` y reintenta una vez.
pub(crate) fn traer_repo<L: crate::servidor::pat::Llavero>(
    estado: &AppState,
    cliente: &ClienteSupabase<L>,
    repo: &RepoUnityVc,
    email: &str,
) -> Resultado<String> {
    let carpeta = crate::cm::sesion::carpeta_config_plastic().ok_or(Error::SinSesionUnityVc)?;
    let servidor = crate::servidor::region::webserver(&repo.org)?;
    let pedir = || -> Resultado<serde_json::Value> {
        let credencial = login_unity::credencial_de(&carpeta, email)?;
        cliente.llamar_funcion(
            "vincular-repo-unity",
            &serde_json::json!({
                "token": credencial.token,
                "org": repo.org,
                "servidor": servidor,
                "repo": repo.repo,
                "nombre": repo.nombre(),
            }),
        )
    };
    let respuesta = match pedir() {
        Err(Error::SesionUnityVcCaducada) => {
            login_unity::renovar_token_cm(estado, &repo.org)?;
            pedir()?
        }
        otro => otro?,
    };
    respuesta["proyecto_id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| Error::Interno("vincular-repo-unity no devolvió el proyecto".into()))
}

/// `vincular_proyecto_servidor()`: vincula el proyecto abierto con el servidor (#240). La app ya lo
/// intenta al entrar; sirve para reintentar (p. ej. al volver internet).
#[tauri::command]
pub async fn vincular_proyecto_servidor(estado: State<'_, AppState>) -> Resultado<Vinculo> {
    vincular_proyecto_servidor_en(&estado)
}

/// `listar_proyectos_del_servidor()`: los proyectos de los que la cuenta de BranchBoard es miembro, con
/// su rol en el servidor (#240). Libre: no hace falta un proyecto abierto, sí la sesión del servidor.
#[tauri::command]
pub async fn listar_proyectos_del_servidor(app: AppHandle) -> Resultado<Vec<ProyectoDeLaCuenta>> {
    let cliente = ClienteSupabase::new(Entorno::actual().proyecto(), LlaveroSistema);
    let sesion = cliente.sesion_guardada()?.ok_or(Error::SinSesionServidor)?;
    let locales =
        crate::proyecto_servidor::pendientes::listar(&carpeta_datos(&app)?, &sesion.usuario_id)?;
    let mut lista = match proyectos::listar_mios(&cliente) {
        Ok(lista) => lista,
        Err(_) if !locales.is_empty() => Vec::new(),
        Err(e) => return Err(e),
    };
    for local in locales {
        if !lista.iter().any(|p| p.id == local.id) {
            lista.push(local);
        }
    }
    lista.sort_by_key(|p| p.nombre.to_lowercase());
    Ok(lista)
}

/// `true` si `email` es el dueño (`owner`) del repositorio según la API.
fn es_dueno_del_repo(estado: &AppState, repo: &RepoUnityVc, email: &str) -> Resultado<bool> {
    let cliente = conectar(estado, &LlaveroSistema, email, &repo.org)?;
    let repos = cliente.listar_repositorios()?;
    let encontrado = repos
        .iter()
        .find(|r| r.name == repo.repo)
        .ok_or_else(|| Error::NoEncontrado(format!("el repositorio {} en Unity VC", repo.repo)))?;
    Ok(normalizar_email(&encontrado.owner) == normalizar_email(email))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cm::Comando;

    #[derive(Clone, Default)]
    struct LlaveroCompartido(
        std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>>,
    );
    impl crate::servidor::pat::Llavero for LlaveroCompartido {
        fn leer(&self, cuenta: &str) -> Resultado<Option<String>> {
            Ok(self.0.borrow().get(cuenta).cloned())
        }
        fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()> {
            self.0.borrow_mut().insert(cuenta.into(), valor.into());
            Ok(())
        }
        fn borrar(&self, cuenta: &str) -> Resultado<()> {
            self.0.borrow_mut().remove(cuenta);
            Ok(())
        }
    }

    /// **Prueba en el entorno real** (#240): un miembro que **no es el dueño** entra por primera vez en una
    /// PC desde el servidor. Contra branchboard-pruebas, con la sesión de Unity VC de la dueña del repo:
    /// 1. la dueña trae el repo al servidor (`vincular-repo-unity`);
    /// 2. se registra una cuenta `entrar-240-<hora>@prueba.branchboard` (clave aleatoria, no se imprime)
    ///    y la dueña la agrega como Desarrollador;
    /// 3. en una PC sin datos, el miembro entra: queda una base vinculada y entra como Desarrollador; al
    ///    entrar otra vez usa esa base;
    /// 4. la dueña, en otra PC sin datos, entra como Administradora.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_miembro_entra -- --ignored --nocapture
    /// ```
    /// Deja el proyecto y la cuenta del miembro: borrarlos después (ver BACKEND.md → Proyectos en el
    /// servidor).
    #[test]
    #[ignore = "toca branchboard-pruebas y Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_miembro_entra_por_primera_vez_en_una_pc() {
        use crate::supabase::entorno::PRUEBAS;
        use rand::{distributions::Alphanumeric, Rng};

        let repo = RepoUnityVc {
            repo: std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO"),
            org: std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG"),
        };
        let estado = AppState::default();
        let duena = whoami(&estado).expect("sin sesión de cm");
        let carpeta = crate::cm::sesion::carpeta_config_plastic().unwrap();
        let cliente_duena = ClienteSupabase::new(&PRUEBAS, LlaveroCompartido::default());
        login_unity::asegurar_sesion_servidor(
            &cliente_duena,
            &carpeta,
            &duena,
            crate::servidor::region::webserver,
            |o| login_unity::renovar_token_cm(&estado, o),
        )
        .unwrap();

        // 1. La dueña trae el repo.
        let proyecto = traer_repo(&estado, &cliente_duena, &repo, &duena).unwrap();

        // 2. Un miembro nuevo, agregado como Desarrollador.
        let hora = chrono::Utc::now().format("%H%M%S%3f").to_string();
        let miembro = format!("entrar-240-{hora}@prueba.branchboard");
        let clave: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let cliente_miembro = ClienteSupabase::new(&PRUEBAS, LlaveroCompartido::default());
        cliente_miembro
            .registrar("Mia", "Prueba", &miembro, &clave)
            .unwrap();
        crate::supabase::miembros::agregar(&cliente_duena, &proyecto, &miembro, Rol::Desarrollador)
            .unwrap();

        let entrar_en_pc =
            |datos: &Path, correo: &str, cliente: &ClienteSupabase<LlaveroCompartido>| {
                acceso::entrar_con_servidor(
                    datos,
                    &repo,
                    correo,
                    || panic!("no hace falta preguntar por el dueño"),
                    || {
                        cliente
                            .llamar(
                                "GET",
                                &proyectos::ruta_buscar_unity(&repo.org, &repo.repo),
                                None,
                            )
                            .ok()
                            .and_then(|j| proyectos::id_desde_json(&j))
                    },
                    |db, p| refrescar_equipo(cliente, db, p),
                    &ahora_iso(),
                    &hoy_local(),
                )
                .unwrap()
            };

        // 3. El miembro, en una PC sin datos.
        let pc_miembro = tempfile::tempdir().unwrap();
        let (entrada, db) = entrar_en_pc(pc_miembro.path(), &miembro, &cliente_miembro);
        assert!(
            matches!(
                entrada,
                Entrada::Dentro {
                    rol: Rol::Desarrollador,
                    configurado_ahora: false,
                    ..
                }
            ),
            "{entrada:?}"
        );
        let db = db.expect("entra con la base abierta");
        assert_eq!(
            proyectos::proyecto_guardado(db.conexion())
                .unwrap()
                .as_deref(),
            Some(proyecto.as_str())
        );
        assert_eq!(crate::db::usuarios::listar(db.conexion()).unwrap().len(), 2);
        drop(db);
        let (entrada, _) = entrar_en_pc(pc_miembro.path(), &miembro, &cliente_miembro);
        assert!(
            matches!(
                entrada,
                Entrada::Dentro {
                    rol: Rol::Desarrollador,
                    ..
                }
            ),
            "{entrada:?}"
        );

        // 4. La dueña, en otra PC sin datos.
        let pc_duena = tempfile::tempdir().unwrap();
        let (entrada, _) = entrar_en_pc(pc_duena.path(), &duena, &cliente_duena);
        assert!(
            matches!(
                entrada,
                Entrada::Dentro {
                    rol: Rol::Administrador,
                    ..
                }
            ),
            "{entrada:?}"
        );

        cliente_miembro.cerrar_sesion().unwrap();
        cliente_duena.cerrar_sesion().unwrap();
        println!(
            "el miembro entró desde el servidor en el proyecto {proyecto} de branchboard-pruebas"
        );
    }

    /// **Prueba en el entorno real** (#240) contra branchboard-pruebas, con la sesión de Unity VC del
    /// equipo (la cuenta de `cm` tiene que ser la dueña del repo de pruebas):
    /// 1. la PC A (Administradora local) vincula el repo: el servidor no lo tiene, así que lo trae con
    ///    `vincular-repo-unity` y guarda el id;
    /// 2. la PC B (como si no fuera Administradora) lo encuentra en el servidor y guarda **el mismo id**,
    ///    sin traerlo otra vez (un repo, un proyecto);
    /// 3. volver a vincular A no toca la red;
    /// 4. `listar_proyectos_del_servidor` lo muestra con rol Administrador.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_vincular -- --ignored --nocapture
    /// ```
    /// Deja el proyecto en el servidor: borrarlo después (ver BACKEND.md → Proyectos en el servidor).
    #[test]
    #[ignore = "toca branchboard-pruebas y Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_vincular_repo_y_listar() {
        use crate::supabase::entorno::PRUEBAS;
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = RepoUnityVc {
            repo: std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO"),
            org,
        };
        let estado = AppState::default();
        let correo = whoami(&estado).expect("sin sesión de cm");
        let carpeta = crate::cm::sesion::carpeta_config_plastic().unwrap();
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroCompartido::default());
        login_unity::asegurar_sesion_servidor(
            &cliente,
            &carpeta,
            &correo,
            crate::servidor::region::webserver,
            |o| login_unity::renovar_token_cm(&estado, o),
        )
        .unwrap();
        let buscar = || {
            cliente
                .llamar(
                    "GET",
                    &proyectos::ruta_buscar_unity(&repo.org, &repo.repo),
                    None,
                )
                .map(|j| proyectos::id_desde_json(&j))
        };

        // 1. PC A trae el repo.
        let pc_a = crate::db::Db::en_memoria().unwrap();
        let v = proyectos::vincular_unity(
            |f| f(pc_a.conexion()),
            1,
            true,
            &correo,
            "2026-11-21",
            buscar,
            || traer_repo(&estado, &cliente, &repo, &correo),
        )
        .unwrap();
        let Vinculo::Vinculado {
            proyecto_id,
            ahora: true,
        } = v
        else {
            panic!("A no quedó vinculada: {v:?}")
        };

        // 2. PC B lo encuentra: mismo proyecto, sin traerlo.
        let pc_b = crate::db::Db::en_memoria().unwrap();
        let v = proyectos::vincular_unity(
            |f| f(pc_b.conexion()),
            1,
            false,
            &correo,
            "2026-11-21",
            buscar,
            || panic!("B no debe traerlo"),
        )
        .unwrap();
        assert_eq!(
            v,
            Vinculo::Vinculado {
                proyecto_id: proyecto_id.clone(),
                ahora: true
            }
        );

        // 3. Ya vinculada, A no toca la red.
        let v = proyectos::vincular_unity(
            |f| f(pc_a.conexion()),
            1,
            true,
            &correo,
            "2026-11-21",
            || panic!("no debe buscar"),
            || panic!("no debe traer"),
        )
        .unwrap();
        assert!(matches!(v, Vinculo::Vinculado { ahora: false, .. }));

        // 4. Aparece en la lista de la cuenta como Administradora.
        let lista = proyectos::listar_mios(&cliente).unwrap();
        let p = lista
            .iter()
            .find(|p| p.id == proyecto_id)
            .expect("no aparece en la lista");
        assert_eq!(p.rol, "Administrador");
        assert_eq!(p.unity_repo.as_deref(), Some(repo.repo.as_str()));

        cliente.cerrar_sesion().unwrap();
        println!("vinculado y listado: proyecto {proyecto_id} de branchboard-pruebas");
    }

    /// **Prueba en el entorno real** (CLAUDE.md): con un workspace temporal del
    /// repo de pruebas y una carpeta de datos temporal, entra como el dueño (se
    /// configura y queda Administrador) y vuelve a entrar (ya no configura).
    /// Necesita que la cuenta de `cm` sea la dueña del repo.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_entrar -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_entrar_a_proyecto() {
        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let estado = AppState::default();
        let ws = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let ruta_ws = ws.path().to_string_lossy().to_string();
        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!(
                "bb-prueba-89-{}",
                chrono::Local::now().format("%H%M%S")
            ))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let primera = entrar_en(&estado, datos.path(), ws.path()).unwrap();
            println!(
                "primera vez: {}",
                serde_json::to_string(&primera)
                    .unwrap()
                    .replace(&whoami(&estado).unwrap(), "<correo>")
            );
            assert!(matches!(
                primera,
                Entrada::Dentro {
                    rol: Rol::Administrador,
                    configurado_ahora: true,
                    ..
                }
            ));
            // Con el proyecto abierto, los comandos del Administrador ya pasan el chequeo de rol.
            assert!(estado.exigir_permiso("listar_usuarios").is_ok());

            let segunda = entrar_en(&estado, datos.path(), ws.path()).unwrap();
            println!(
                "segunda vez: {}",
                serde_json::to_string(&segunda)
                    .unwrap()
                    .replace(&whoami(&estado).unwrap(), "<correo>")
            );
            assert!(matches!(
                segunda,
                Entrada::Dentro {
                    rol: Rol::Administrador,
                    rol_verificado: true,
                    configurado_ahora: false,
                    ..
                }
            ));
        }));
        estado.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado);
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
