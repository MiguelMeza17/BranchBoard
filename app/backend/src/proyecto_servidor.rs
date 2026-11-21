//! Proyectos sin Unity VC en el servidor de Supabase (#240). Reemplazan, para lo nuevo, a los proyectos
//! "por carpeta" de `proyecto_local` (que siguen abriéndose hasta que se suban, #245): nada va por Drive.
//!
//! - **Crear**: con la sesión de BranchBoard, `rpc/crear_proyecto(nombre)`; quien lo crea queda como
//!   Administrador en el servidor (#234). Sin carpeta que elegir, sin contraseña, sin llaves.
//! - **Datos locales**: `<datos de la app>/proyectos/servidor-<id>/`, con `proyecto_servidor_id`, el nombre
//!   y la copia del equipo (`miembros`, #241). Así la bandeja (#238), la recepción (#239), Storage (#242)
//!   y los roles por el servidor (#241) funcionan igual que en un proyecto de Unity VC.
//! - **Abrir desde cualquier PC**: con la cuenta, de la lista de sus proyectos en el servidor. Si la PC no
//!   tiene los datos, se crean; el historial lo baja la recepción (#239).
//! - **Eventos**: hasta que #244 quite `ops/`, `publicar` escribe también en una carpeta **local y
//!   privada** (`eventos-locales/`, dentro de los datos del proyecto), que no se sincroniza con nada.

use std::path::{Path, PathBuf};

use crate::db::configuracion::{self, CLAVE_NOMBRE_PROYECTO, CLAVE_PROYECTO_SERVIDOR};
use crate::db::{carpeta_proyecto, usuarios, vistos, Db};
use crate::error::{Error, Resultado};
use crate::eventos;
use crate::identidad::acceso::Entrada;
use crate::modelo::normalizar_email;

/// Carpeta de datos del proyecto del servidor `id` en esta PC.
pub fn carpeta_datos(datos_app: &Path, id: &str) -> Resultado<PathBuf> {
    if id.trim().is_empty() {
        return Err(Error::Validacion("Falta el proyecto".into()));
    }
    carpeta_proyecto(datos_app, &format!("servidor-{}", id.trim()))
}

/// Deja lista la base local del proyecto `id` (la crea si falta): vinculada, con su nombre, la carpeta
/// local de eventos y la copia del equipo (`refrescar`; si falla y la base ya tenía una copia, se usa
/// esa). Devuelve la base abierta.
pub fn preparar(
    datos_app: &Path,
    id: &str,
    nombre: &str,
    email: &str,
    hoy: &str,
    refrescar: impl FnOnce(&Db) -> Resultado<()>,
) -> Resultado<Db> {
    let carpeta = carpeta_datos(datos_app, id)?;
    let nueva = !carpeta.join(crate::db::NOMBRE_DB).is_file();
    std::fs::create_dir_all(&carpeta)?;
    let db = Db::abrir(&carpeta)?;
    let c = db.conexion();
    if configuracion::leer(c, CLAVE_PROYECTO_SERVIDOR)?.is_none() {
        configuracion::guardar(c, CLAVE_PROYECTO_SERVIDOR, id, email, hoy)?;
    }
    if !nombre.trim().is_empty() {
        configuracion::guardar(c, CLAVE_NOMBRE_PROYECTO, nombre.trim(), email, hoy)?;
    }
    match refrescar(&db) {
        Ok(()) => {}
        // Sin red: sirve la copia que ya había; una PC nueva sin copia no puede saber el rol.
        Err(e) if nueva || usuarios::listar(c)?.is_empty() => return Err(e),
        Err(_) => {}
    }
    Ok(db)
}

/// Qué ve `email` en la base preparada: entra con su rol de la copia del equipo (sin firmas: los roles
/// los hace cumplir el servidor) o "no registrado".
pub fn decidir(db: &Db, email: &str, configurado_ahora: bool) -> Resultado<Entrada> {
    let email = normalizar_email(email);
    let c = db.conexion();
    let proyecto = configuracion::leer(c, CLAVE_NOMBRE_PROYECTO)?.unwrap_or_default();
    let Some(fila) = usuarios::listar(c)?.into_iter().find(|u| u.email == email) else {
        return Ok(Entrada::NoRegistrado { email, proyecto });
    };
    Ok(Entrada::Dentro {
        vistos: vistos::listar_vistos(c, &email)?,
        email,
        rol: fila.rol,
        rol_verificado: true,
        proyecto,
        configurado_ahora,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::usuarios::Usuario;
    use crate::modelo::Rol;

    const ID: &str = "11111111-2222-4333-8444-555555555555";
    const HOY: &str = "2026-11-21";

    fn copia(miembros: &[(&str, Rol)]) -> Vec<Usuario> {
        miembros
            .iter()
            .map(|(email, rol)| Usuario {
                email: (*email).into(),
                nombre: "N".into(),
                apellido: String::new(),
                rol: *rol,
                fecha_alta: HOY.into(),
            })
            .collect()
    }

    fn preparar_con(datos: &Path, miembros: &[(&str, Rol)]) -> Db {
        let filas = copia(miembros);
        preparar(datos, ID, "Juego", "ana@virustf.dev", HOY, |db| {
            usuarios::reemplazar_todos(db.conexion(), &filas)
        })
        .unwrap()
    }

    #[test]
    fn prepara_una_base_vinculada_con_su_equipo_y_eventos_locales() {
        let datos = tempfile::tempdir().unwrap();
        let db = preparar_con(
            datos.path(),
            &[("ana@virustf.dev", Rol::Administrador), ("beto@virustf.dev", Rol::Tester)],
        );
        let c = db.conexion();
        assert_eq!(configuracion::leer(c, CLAVE_PROYECTO_SERVIDOR).unwrap().as_deref(), Some(ID));
        assert_eq!(configuracion::leer(c, CLAVE_NOMBRE_PROYECTO).unwrap().as_deref(), Some("Juego"));
        let carpeta = carpeta_datos(datos.path(), ID).unwrap();
        assert!(carpeta.ends_with(format!("servidor-{ID}")));
        assert!(matches!(
            decidir(&db, "Beto@VirusTF.dev", false).unwrap(),
            Entrada::Dentro { rol: Rol::Tester, rol_verificado: true, .. }
        ));
        assert!(matches!(
            decidir(&db, "nadie@virustf.dev", false).unwrap(),
            Entrada::NoRegistrado { .. }
        ));
    }

    #[test]
    fn una_pc_nueva_sin_red_no_puede_saber_el_rol() {
        let datos = tempfile::tempdir().unwrap();
        let e = preparar(datos.path(), ID, "Juego", "ana@virustf.dev", HOY, |_| {
            Err(Error::ServidorSinConexion)
        })
        .unwrap_err();
        assert!(matches!(e, Error::ServidorSinConexion));
    }

    #[test]
    fn sin_red_se_abre_con_la_ultima_copia() {
        let datos = tempfile::tempdir().unwrap();
        drop(preparar_con(datos.path(), &[("ana@virustf.dev", Rol::Administrador)]));
        let db = preparar(datos.path(), ID, "", "ana@virustf.dev", HOY, |_| {
            Err(Error::ServidorSinConexion)
        })
        .unwrap();
        assert!(matches!(
            decidir(&db, "ana@virustf.dev", false).unwrap(),
            Entrada::Dentro { rol: Rol::Administrador, .. }
        ));
        // Un nombre vacío no pisa el que ya tenía.
        assert_eq!(
            configuracion::leer(db.conexion(), CLAVE_NOMBRE_PROYECTO).unwrap().as_deref(),
            Some("Juego")
        );
    }
}
