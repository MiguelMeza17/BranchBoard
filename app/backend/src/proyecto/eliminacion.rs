//! Doble autorización para eliminar el repositorio del proyecto (#128).
//! La ACL se cruza con el dueño y las pertenencias reales de Unity VC.
use std::collections::HashMap;

use serde::Serialize;

use crate::cm::{lectura, Comando};
use crate::db::configuracion::{self, CLAVE_NOMBRE_PROYECTO};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::identidad::acceso::RepoUnityVc;
use crate::modelo::normalizar_email;
use crate::servidor::{self, ClienteServidor};

#[derive(Debug, PartialEq, Eq)]
struct EntradaAcl {
    principal: String,
    permitido: bool,
    denegado: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProyectoEliminado {
    pub repo: String,
    pub nombre: String,
}

fn contiene_permiso(texto: &str) -> bool {
    texto
        .split_whitespace()
        .any(|p| matches!(p, "rmrepository" | "all"))
}

fn interpretar_acl(xml: &str) -> Resultado<Vec<EntradaAcl>> {
    if xml.len() > 2 * 1024 * 1024 {
        return Err(Error::Validacion(
            "La respuesta de permisos excede el límite permitido.".into(),
        ));
    }
    let documento = roxmltree::Document::parse(xml).map_err(|_| {
        Error::Validacion("No se pudieron interpretar los permisos de Unity VC.".into())
    })?;
    if documento.root_element().tag_name().name() != "ExtendedAclInfo" {
        return Err(Error::Validacion(
            "Unity VC no devolvió una ACL extendida reconocida.".into(),
        ));
    }
    let mut lista = Vec::new();
    for nodo in documento
        .descendants()
        .filter(|n| n.has_tag_name("UserPermissions"))
    {
        let principal = nodo
            .children()
            .find(|n| n.has_tag_name("UserName"))
            .and_then(|n| n.text())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Validacion("La ACL tiene una identidad vacía.".into()))?;
        let (mut permitido, mut denegado) = (false, false);
        for permiso in nodo
            .children()
            .filter(|n| n.is_element() && !n.has_tag_name("UserName"))
        {
            if !contiene_permiso(permiso.text().unwrap_or_default()) {
                continue;
            }
            match permiso.tag_name().name() {
                "Allowed" | "OverrideAllowed" => permitido = true,
                // Ante una denegación o una regla no reconocida no se borra.
                _ => denegado = true,
            }
        }
        if permitido || denegado {
            lista.push(EntradaAcl {
                principal: principal.into(),
                permitido,
                denegado,
            });
        }
    }
    Ok(lista)
}

fn permiso_efectivo(
    acl: &[EntradaAcl],
    mut pertenece: impl FnMut(&str) -> Resultado<bool>,
) -> Resultado<bool> {
    let mut permitido = false;
    for regla in acl {
        if pertenece(&regla.principal)? {
            if regla.denegado {
                return Ok(false);
            }
            permitido |= regla.permitido;
        }
    }
    Ok(permitido)
}

fn comprobar_permiso(
    estado: &AppState,
    cliente: &ClienteServidor,
    repo: &RepoUnityVc,
    email: &str,
) -> Resultado<()> {
    let spec = format!("rep:{}@{}@unity", repo.repo, repo.org);
    let xml = Comando::cm()
        .args(["showacl", "--extended", "--xml", &spec])
        .exigir_exito(estado)?;
    let acl = interpretar_acl(&xml)?;
    let dueno = cliente
        .listar_repositorios()?
        .into_iter()
        .find(|r| r.name == repo.repo)
        .ok_or_else(|| {
            Error::NoEncontrado("El repositorio ya no está disponible en Unity VC.".into())
        })?
        .owner;
    let mut cache = HashMap::<String, bool>::new();
    let permitido = permiso_efectivo(&acl, |principal| {
        if let Some(valor) = cache.get(principal) {
            return Ok(*valor);
        }
        let valor = if principal.eq_ignore_ascii_case("PROPIETARIO")
            || principal.eq_ignore_ascii_case("OWNER")
        {
            normalizar_email(&dueno) == email
        } else if principal.eq_ignore_ascii_case("ALL USERS") {
            true
        } else if principal.contains('@') {
            normalizar_email(principal) == email
        } else {
            let grupo = format!("--group={principal}");
            let servidor = format!("{}@unity", repo.org);
            // Solo la salida se compara; no se muestra ni se guarda la lista de personas.
            let miembros = Comando::cm()
                .args(["listusers", &servidor, &grupo])
                .exigir_exito(estado)?;
            miembros
                .lines()
                .any(|l| normalizar_email(l.trim()) == email)
        };
        cache.insert(principal.into(), valor);
        Ok(valor)
    })?;
    if permitido {
        Ok(())
    } else {
        Err(Error::SinPermiso)
    }
}

fn preparar_local(
    estado: &AppState,
    confirmacion: &str,
) -> Resultado<(RepoUnityVc, String, String)> {
    let yo = estado.exigir_permiso("eliminar_proyecto")?;
    if !yo.rol_verificado {
        return Err(Error::SinPermiso);
    }
    let repo = estado.repo_abierto()?;
    let nombre = estado.con_db(|db| {
        Ok(configuracion::leer(db.conexion(), CLAVE_NOMBRE_PROYECTO)?
            .unwrap_or_else(|| repo.nombre().into()))
    })?;
    if confirmacion != nombre || nombre.is_empty() {
        return Err(Error::Validacion(
            "Escribe el nombre exacto del proyecto para confirmar.".into(),
        ));
    }
    if estado.hay_operacion_en_curso() {
        return Err(Error::Validacion(
            "Espera a que termine la operación en curso antes de eliminar el proyecto.".into(),
        ));
    }
    Ok((repo, nombre, yo.email))
}

fn preparar(estado: &AppState, confirmacion: &str) -> Resultado<(RepoUnityVc, String)> {
    let (repo, nombre, email) = preparar_local(estado, confirmacion)?;
    if lectura::whoami(estado)? != email {
        return Err(Error::SinPermiso);
    }
    let cliente = servidor::conectar(estado, &servidor::LlaveroSistema, &email, &repo.org)?;
    comprobar_permiso(estado, &cliente, &repo, &email)?;
    Ok((repo, nombre))
}

/// Solo comprueba los candados. Sirve para verificar el acceso sin borrar.
pub fn verificar_en(estado: &AppState, confirmacion: &str) -> Resultado<ProyectoEliminado> {
    let (repo, nombre) = preparar(estado, confirmacion)?;
    Ok(ProyectoEliminado {
        repo: format!("{}@{}@unity", repo.repo, repo.org),
        nombre,
    })
}

/// `cm repo rm` vuelve a exigir el permiso en el servidor al ejecutar la acción.
/// No borra carpetas del usuario ni cambia los otros workspaces de su equipo.
pub fn eliminar_en(estado: &AppState, confirmacion: &str) -> Resultado<ProyectoEliminado> {
    let resultado = verificar_en(estado, confirmacion)?;
    let repo_actual = estado.repo_abierto()?;
    if format!("{}@{}@unity", repo_actual.repo, repo_actual.org) != resultado.repo {
        return Err(Error::Validacion(
            "El proyecto abierto cambió durante la comprobación. Intenta de nuevo.".into(),
        ));
    }
    let _operacion = estado.iniciar_operacion();
    Comando::cm()
        .args(["repo", "rm", &format!("rep:{}", resultado.repo)])
        .exigir_exito(estado)?;
    estado.cerrar_proyecto();
    Ok(resultado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::usuarios,
        identidad::{acceso, firmas, llaves},
        modelo::{ahora_iso, hoy_local, Rol},
    };

    fn escenario(email: &str, repo: &RepoUnityVc) -> (tempfile::TempDir, AppState) {
        let datos = tempfile::tempdir().unwrap();
        let (_, db) = acceso::entrar(
            datos.path(),
            repo,
            email,
            || Ok(true),
            &ahora_iso(),
            &hoy_local(),
        )
        .unwrap();
        let estado = AppState::default();
        estado.abrir_proyecto(db.unwrap());
        estado.fijar_usuario(Some(email));
        estado.fijar_workspace(None, Some(repo.clone()));
        (datos, estado)
    }

    #[test]
    fn nombre_exacto_firma_y_operacion_se_validan_sin_ejecutar_cm() {
        let repo = RepoUnityVc {
            repo: "Pruebas/Juego".into(),
            org: "OrgPrueba".into(),
        };
        let (_datos, estado) = escenario("admin@virustf.dev", &repo);
        estado
            .con_db(|db| {
                configuracion::guardar(
                    db.conexion(),
                    CLAVE_NOMBRE_PROYECTO,
                    "Juego Ágil",
                    "admin@virustf.dev",
                    &ahora_iso(),
                )
            })
            .unwrap();
        for nombre in [
            "",
            "juego ágil",
            "Juego Agil",
            "Juego Ágil ",
            " Juego Ágil",
            "Juego",
        ] {
            assert!(preparar_local(&estado, nombre).is_err());
        }
        assert!(preparar_local(&estado, "Juego Ágil").is_ok());
        let operacion = estado.iniciar_operacion();
        assert!(preparar_local(&estado, "Juego Ágil").is_err());
        drop(operacion);
        estado
            .con_db(|db| {
                usuarios::actualizar_rol(
                    db.conexion(),
                    "admin@virustf.dev",
                    Rol::Administrador,
                    Some("forjada"),
                    Some(&ahora_iso()),
                    Some("admin@virustf.dev"),
                )
            })
            .unwrap();
        assert!(preparar_local(&estado, "Juego Ágil").is_err());
        estado.cerrar_proyecto();
    }

    #[test]
    fn otros_roles_aunque_estan_firmados_no_pueden_eliminar() {
        let repo = RepoUnityVc {
            repo: "Pruebas/Juego".into(),
            org: "OrgPrueba".into(),
        };
        let (_datos, estado) = escenario("admin@virustf.dev", &repo);
        for rol in Rol::TODOS {
            estado
                .con_db(|db| {
                    let llave = llaves::cargar_llave_admin(db.carpeta().unwrap(), None)?;
                    let ahora = ahora_iso();
                    let firma = firmas::firmar_rol(&llave, "admin@virustf.dev", *rol, &ahora)?;
                    usuarios::actualizar_rol(
                        db.conexion(),
                        "admin@virustf.dev",
                        *rol,
                        Some(&firma),
                        Some(&ahora),
                        Some("admin@virustf.dev"),
                    )
                })
                .unwrap();
            assert_eq!(
                preparar_local(&estado, "Juego").is_ok(),
                *rol == Rol::Administrador
            );
        }
        estado.cerrar_proyecto();
    }

    #[test]
    fn permiso_es_del_usuario_no_de_cualquier_grupo_y_denegar_prevalece() {
        let acl = interpretar_acl(r#"<ExtendedAclInfo><Users>
          <UserPermissions><UserName>otro-admin</UserName><Allowed>rmrepository</Allowed></UserPermissions>
          <UserPermissions><UserName>yo-rw</UserName><Allowed>read view</Allowed></UserPermissions>
        </Users><InheritedFrom><ExtendedAclInfo><Users>
          <UserPermissions><UserName>yo-admin</UserName><Allowed>all</Allowed></UserPermissions>
          <UserPermissions><UserName>yo-ro</UserName><Denied>rmrepository</Denied></UserPermissions>
        </Users></ExtendedAclInfo></InheritedFrom></ExtendedAclInfo>"#).unwrap();
        assert!(!permiso_efectivo(&acl, |p| Ok(p == "yo-rw")).unwrap());
        assert!(permiso_efectivo(&acl, |p| Ok(p == "yo-admin")).unwrap());
        assert!(!permiso_efectivo(&acl, |p| Ok(p.starts_with("yo-"))).unwrap());
        assert!(permiso_efectivo(&acl, |_| Err(Error::SinPermiso)).is_err());
    }

    #[test]
    fn acl_malformada_o_incompleta_no_concede_y_decodifica_identidades() {
        assert!(interpretar_acl("<x").is_err());
        assert!(interpretar_acl("<AclEntries />").is_err());
        assert!(interpretar_acl("<ExtendedAclInfo><UserPermissions><Allowed>all</Allowed></UserPermissions></ExtendedAclInfo>").is_err());
        let acl = interpretar_acl("<ExtendedAclInfo><Users><UserPermissions><UserName>grupo&amp;uno</UserName><Allowed>rmrepositoryExtra</Allowed></UserPermissions></Users></ExtendedAclInfo>").unwrap();
        assert!(acl.is_empty());
        let acl = interpretar_acl("<ExtendedAclInfo><Users><UserPermissions><UserName>grupo&amp;uno</UserName><Allowed>rmrepository</Allowed></UserPermissions></Users></ExtendedAclInfo>").unwrap();
        assert_eq!(acl[0].principal, "grupo&uno");
    }

    #[test]
    #[ignore = "solo verifica ACL real, no borra: BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_verificar_eliminacion() {
        use crate::servidor::pat::{cuenta_llavero, Llavero, Pat};
        struct PatTemporal {
            cuenta: String,
            org: String,
            previo: Option<String>,
        }
        impl Drop for PatTemporal {
            fn drop(&mut self) {
                if let Ok(Some(valor)) = servidor::LlaveroSistema.leer(&self.cuenta) {
                    if let Ok(pat) = serde_json::from_str::<Pat>(&valor) {
                        if self.previo.as_deref() != Some(pat.id.as_str()) {
                            servidor::pat::revocar_pat(&AppState::default(), &self.org, &pat.id);
                            let _ = servidor::LlaveroSistema.borrar(&self.cuenta);
                        }
                    }
                }
            }
        }
        let org = std::env::var("BB_PRUEBA_ORG").unwrap();
        let repo = std::env::var("BB_PRUEBA_REPO").unwrap();
        assert_eq!(repo, "BranchBoard-Pruebas/BranchBoard-Pruebas");
        let sesion = AppState::default();
        let email = lectura::whoami(&sesion).unwrap();
        let cuenta = cuenta_llavero(&email, &org);
        let _pat = PatTemporal {
            previo: servidor::LlaveroSistema
                .leer(&cuenta)
                .unwrap()
                .map(|v| serde_json::from_str::<Pat>(&v).unwrap().id),
            cuenta,
            org: format!("{}@unity", servidor::nombre_organizacion(&org)),
        };
        let cliente = servidor::conectar(&sesion, &servidor::LlaveroSistema, &email, &org).unwrap();
        assert!(cliente
            .listar_repositorios()
            .unwrap()
            .iter()
            .any(|r| r.name == repo && normalizar_email(&r.owner) == email));
        let repo = RepoUnityVc {
            repo,
            org: servidor::nombre_organizacion(&org).into(),
        };
        let (_datos, estado) = escenario(&email, &repo);
        assert!(verificar_en(&estado, "branchboard-pruebas").is_err());
        let comprobado = verificar_en(&estado, "BranchBoard-Pruebas").unwrap();
        assert!(comprobado
            .repo
            .starts_with("BranchBoard-Pruebas/BranchBoard-Pruebas@"));
        assert!(cliente
            .listar_repositorios()
            .unwrap()
            .iter()
            .any(|r| r.name == repo.repo));
        estado.cerrar_proyecto();
        println!("ACL real y pertenencias verificadas; nombre distinto rechazado; repositorio intacto; ningún workspace creado ni borrado.");
    }
}
