//! Comandos locales para los puntos de aviso del menú (#121).
//!
//! Cada usuario conserva en su `.db` local (`paneles_vistos`) la última huella
//! vista de cada panel; nunca sale por `ops/`.

use std::collections::BTreeMap;

use tauri::State;

use crate::db::vistos;
use crate::error::{Error, Resultado};
use crate::estado::AppState;

/// Guarda la huella vista de `panel` para el usuario actual en esta máquina (#121).
#[tauri::command]
pub fn marcar_visto(estado: State<'_, AppState>, panel: String, huella: String) -> Resultado<()> {
    marcar_visto_en(&estado, &panel, &huella)
}

/// Versión sin envoltorio Tauri de [`marcar_visto`].
pub fn marcar_visto_en(estado: &AppState, panel: &str, huella: &str) -> Resultado<()> {
    let yo = estado.exigir_permiso("marcar_visto")?;
    let panel_limpio = panel.trim();
    if panel_limpio.is_empty() {
        return Err(Error::Validacion("Indica el panel visto".into()));
    }
    estado.con_db(|db| vistos::marcar_visto(db.conexion(), &yo.email, panel_limpio, huella))
}

/// Devuelve todas las huellas guardadas `{ panel -> huella }` del usuario actual (#121).
#[tauri::command]
pub fn obtener_vistos(estado: State<'_, AppState>) -> Resultado<BTreeMap<String, String>> {
    obtener_vistos_en(&estado)
}

/// Versión sin envoltorio Tauri de [`obtener_vistos`].
pub fn obtener_vistos_en(estado: &AppState) -> Resultado<BTreeMap<String, String>> {
    let yo = estado.exigir_permiso("obtener_vistos")?;
    estado.con_db(|db| vistos::listar_vistos(db.conexion(), &yo.email))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comandos::acceso::entrar_en;
    use crate::db::usuarios::{self, Usuario};
    use crate::db::Db;
    use crate::identidad::firmas;
    use crate::identidad::llaves::{
        cargar_llave_admin, fijar_llave_publica_admin, generar_llaves_admin,
    };
    use crate::modelo::Rol;

    const ADMIN: &str = "admin@estudio.com";
    const DEV: &str = "dev@estudio.com";
    const AHORA: &str = "2026-11-16T10:00:00.000Z";
    const HOY: &str = "2026-11-16";

    #[test]
    fn marcar_visto_y_obtener_vistos_aislados_por_usuario() {
        let tmp = tempfile::tempdir().unwrap();
        let carpeta = tmp.path().join("proyectos").join("estudio-vistos");
        let publica = generar_llaves_admin(&carpeta, None).unwrap();
        let privada = cargar_llave_admin(&carpeta, None).unwrap();
        fijar_llave_publica_admin(&carpeta, &publica).unwrap();

        let db = Db::abrir(&carpeta).unwrap();
        for (correo, rol) in [(ADMIN, Rol::Administrador), (DEV, Rol::Desarrollador)] {
            usuarios::insertar(
                db.conexion(),
                &Usuario {
                    email: correo.into(),
                    nombre: correo.split('@').next().unwrap().into(),
                    apellido: "Test".into(),
                    rol,
                    fecha_alta: HOY.into(),
                },
            )
            .unwrap();
        }

        let estado = AppState::default();
        estado.abrir_proyecto(db);
        estado.fijar_usuario(Some(ADMIN));

        assert!(marcar_visto_en(&estado, "   ", "h1").is_err());
        assert!(obtener_vistos_en(&estado).unwrap().is_empty());

        marcar_visto_en(&estado, "/scrum", "h-scrum-1").unwrap();
        marcar_visto_en(&estado, "/usuarios", "h-usr-1").unwrap();
        assert_eq!(obtener_vistos_en(&estado).unwrap().len(), 2);

        estado.fijar_usuario(Some(DEV));
        assert!(obtener_vistos_en(&estado).unwrap().is_empty());
        marcar_visto_en(&estado, "/scrum", "h-scrum-dev").unwrap();

        estado.fijar_usuario(Some(ADMIN));
        assert_eq!(
            obtener_vistos_en(&estado)
                .unwrap()
                .get("/scrum")
                .map(String::as_str),
            Some("h-scrum-1")
        );
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #121): entra al repo real
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` con `entrar_en`,
    /// guarda huellas con `marcar_visto_en`, verifica aislamiento entre usuarios
    /// en la misma máquina con `obtener_vistos_en` y comprueba que al volver a
    /// abrir el proyecto `Entrada::Dentro` devuelve exactamente las huellas guardadas.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_puntos_aviso() {
        use crate::cm::Comando;
        use crate::comandos::datos::carpeta_abierta;
        use crate::identidad::acceso::Entrada;
        use crate::identidad::llaves::cargar_llave_admin;
        use crate::modelo::hoy_local;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();

        let estado = AppState::default();
        let ws = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let ruta_ws = ws.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-121-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let entrada1 = entrar_en(&estado, datos.path(), ws.path()).unwrap();
            let admin_email = match entrada1 {
                Entrada::Dentro { email, vistos, .. } => {
                    assert!(vistos.is_empty());
                    email
                }
                otro => panic!("esperaba Entrada::Dentro, llegó {otro:?}"),
            };

            marcar_visto_en(&estado, "/scrum", "T-101:por hacer").unwrap();
            marcar_visto_en(&estado, "/usuarios", "u-1").unwrap();
            assert_eq!(obtener_vistos_en(&estado).unwrap().len(), 2);

            // Registramos otro usuario en la misma máquina y validamos su aislamiento local.
            let local = carpeta_abierta(&estado).unwrap();
            let llave = cargar_llave_admin(&local, None).unwrap();
            let ahora = crate::modelo::ahora_iso();
            let hoy = hoy_local();
            estado
                .con_db(|db| {
                    usuarios::insertar(
                        db.conexion(),
                        &Usuario {
                            email: DEV.into(),
                            nombre: "Dev".into(),
                            apellido: "Test".into(),
                            rol: Rol::Desarrollador,
                            fecha_alta: hoy.clone(),
                        },
                    )
                })
                .unwrap();

            estado.fijar_usuario(Some(DEV));
            assert!(obtener_vistos_en(&estado).unwrap().is_empty());
            marcar_visto_en(&estado, "/scrum", "otra-huella").unwrap();

            // Al cerrar y volver a abrir con `entrar_en`, el dueño recibe sus huellas al abrir.
            estado.cerrar_proyecto();
            let entrada2 = entrar_en(&estado, datos.path(), ws.path()).unwrap();
            match entrada2 {
                Entrada::Dentro {
                    email,
                    configurado_ahora,
                    vistos,
                    ..
                } => {
                    assert_eq!(email, admin_email);
                    assert!(!configurado_ahora);
                    assert_eq!(
                        vistos.get("/scrum").map(String::as_str),
                        Some("T-101:por hacer")
                    );
                    assert_eq!(vistos.get("/usuarios").map(String::as_str), Some("u-1"));
                    assert_eq!(vistos.len(), 2);
                    println!("Huellas recibidas al abrir: {vistos:?}");
                }
                otro => panic!("esperaba Entrada::Dentro, llegó {otro:?}"),
            }
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
