//! Chequeo de rol en cada comando (issue #87). Ver "Roles y permisos" en
//! SEGURIDAD.md y la tabla "Contrato con el frontend" en BACKEND.md.
//!
//! El frontend esconde lo que un rol no puede hacer, pero eso no protege nada:
//! cualquiera puede llamar a un comando saltándose la interfaz. Por eso cada
//! comando que depende del rol empieza con `estado.exigir_permiso("comando")?`,
//! y los roles de cada comando están **todos en una sola tabla**, [`PERMISOS`].
//! Un comando que no está en la tabla se niega siempre.
//!
//! Lo que depende de la tarea concreta (ej. "solo el creador puede editar su
//! tarea pendiente") lo revisa cada comando después, con el usuario que devuelve
//! `exigir_permiso`.

use rusqlite::Connection;
use serde::Serialize;

use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::identidad::firmas::{verificar_usuarios, Verificacion};
use crate::modelo::{normalizar_email, Rol};

/// Quién puede usar un comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceso {
    /// No depende de un proyecto ni de un rol (ej. buscar actualizaciones).
    Libre,
    /// Solo estos roles (rol verificado por la firma, ver `firmas`).
    Roles(&'static [Rol]),
}

pub const TODOS: &[Rol] = Rol::TODOS;
pub const LIDER_O_ADMIN: &[Rol] = &[Rol::Administrador, Rol::Lider];
pub const LIDER_O_DESARROLLADOR: &[Rol] = &[Rol::Lider, Rol::Desarrollador];
pub const SOLO_ADMIN: &[Rol] = &[Rol::Administrador];

/// Todos los comandos de Tauri y quién puede usarlos. Al agregar un comando a
/// `lib.rs` hay que agregarlo aquí (una prueba lo exige).
pub const PERMISOS: &[(&str, Acceso)] = &[
    ("ping", Acceso::Libre),
    ("hay_operacion_en_curso", Acceso::Libre),
    ("elegir_carpeta", Acceso::Libre),
    ("estado_warp", Acceso::Libre),
    ("alternar_warp", Acceso::Libre),
    ("instalar_warp", Acceso::Libre),
    ("estado_cm", Acceso::Libre),
    ("instalar_cm", Acceso::Libre),
    ("sesion_unity_vc", Acceso::Libre),
    ("iniciar_sesion_unity_vc", Acceso::Libre),
    ("reconectar_unity_vc", Acceso::Libre),
    ("cerrar_sesion_unity_vc", Acceso::Libre),
    ("listar_workspaces_locales", Acceso::Libre),
    ("listar_tus_proyectos", Acceso::Libre),
    ("listar_proyectos_servidor", Acceso::Libre),
    ("clonar_proyecto", Acceso::Libre),
    ("entrar_a_proyecto", Acceso::Libre),
    ("buscar_actualizacion", Acceso::Libre),
    ("instalar_actualizacion", Acceso::Libre),
    ("listar_backups", Acceso::Roles(SOLO_ADMIN)),
    ("restaurar_backup", Acceso::Roles(SOLO_ADMIN)),
    ("carpeta_compartida", Acceso::Roles(TODOS)),
    ("configurar_carpeta_compartida", Acceso::Roles(TODOS)),
    ("sincronizar_eventos", Acceso::Roles(TODOS)),
    ("revisar_actualizaciones", Acceso::Roles(TODOS)),
    ("compactar_eventos", Acceso::Roles(SOLO_ADMIN)),
    ("exportar_llave_firma", Acceso::Roles(SOLO_ADMIN)),
    // Usuarios (issue #90). El detalle de quién cambia a quién está en `identidad::equipo`.
    ("listar_usuarios", Acceso::Roles(TODOS)),
    ("crear_usuario", Acceso::Roles(LIDER_O_ADMIN)),
    ("cambiar_rol", Acceso::Roles(LIDER_O_ADMIN)),
    ("eliminar_usuario", Acceso::Roles(LIDER_O_ADMIN)),
    ("confirmar_usuario", Acceso::Roles(SOLO_ADMIN)),
    // Tareas (issues #95, #96, #97, #98). El detalle de quién puede editar, tomar o asignar qué tarea está en `tareas`.
    ("listar_tareas", Acceso::Roles(TODOS)),
    ("crear_tarea", Acceso::Roles(TODOS)),
    ("crear_tarea_con_subtareas", Acceso::Roles(TODOS)),
    ("editar_tarea", Acceso::Roles(TODOS)),
    ("tomar_tarea", Acceso::Roles(TODOS)),
    ("asignar_tarea", Acceso::Roles(LIDER_O_ADMIN)),
    ("desasignar_tarea", Acceso::Roles(TODOS)),
    ("cambiar_estado", Acceso::Roles(TODOS)),
    ("borrar_progreso", Acceso::Roles(TODOS)),
    ("reabrir_tarea", Acceso::Roles(TODOS)),
    ("aprobar_tarea", Acceso::Roles(LIDER_O_ADMIN)),
    ("aprobar_tareas", Acceso::Roles(LIDER_O_ADMIN)),
    ("editar_y_aprobar_tarea", Acceso::Roles(LIDER_O_ADMIN)),
    ("advertir_tarea_pendiente", Acceso::Roles(LIDER_O_ADMIN)),
    ("listar_ediciones_pendientes", Acceso::Roles(TODOS)),
    ("proponer_edicion", Acceso::Roles(TODOS)),
    ("aprobar_modificacion", Acceso::Roles(LIDER_O_ADMIN)),
    ("aprobar_modificaciones", Acceso::Roles(LIDER_O_ADMIN)),
    ("rechazar_modificacion", Acceso::Roles(LIDER_O_ADMIN)),
    ("listar_papelera", Acceso::Roles(TODOS)),
    ("eliminar_tarea", Acceso::Roles(LIDER_O_ADMIN)),
    ("eliminar_definitivamente", Acceso::Roles(LIDER_O_ADMIN)),
    ("cambios_pendientes", Acceso::Roles(TODOS)),
    ("cambiar_a_rama_tarea", Acceso::Roles(TODOS)),
    ("hacer_checkin", Acceso::Roles(TODOS)),
    ("crear_rama_desde_espera", Acceso::Roles(TODOS)),
    ("listar_cambios_prestados", Acceso::Roles(TODOS)),
    ("traer_cambios", Acceso::Roles(TODOS)),
    ("devolver_cambios_prestados", Acceso::Roles(TODOS)),
    ("eliminar_cambios_ajenos", Acceso::Roles(TODOS)),
    ("volver_a_dev", Acceso::Roles(TODOS)),
    // Revisión (issue #107). `autorizar_merge` y `rechazar_revision` los puede usar
    // también quien tiene a cargo la tarea padre de una subtarea; `tareas` lo revisa.
    ("asignar_revision", Acceso::Roles(LIDER_O_ADMIN)),
    ("autorizar_merge", Acceso::Roles(TODOS)),
    ("rechazar_revision", Acceso::Roles(TODOS)),
    ("listar_commits", Acceso::Roles(TODOS)),
    ("abrir_en_unity", Acceso::Roles(TODOS)),
    ("listar_sprints", Acceso::Roles(TODOS)),
    ("crear_sprint", Acceso::Roles(LIDER_O_ADMIN)),
    ("asignar_tarea_a_sprint", Acceso::Roles(LIDER_O_ADMIN)),
    ("asignar_tarea_a_backlog", Acceso::Roles(LIDER_O_ADMIN)),
    ("asignar_backlog_a_sprint", Acceso::Roles(LIDER_O_ADMIN)),
    ("cerrar_sprint", Acceso::Roles(SOLO_ADMIN)),
    ("datos_avance_sprint", Acceso::Roles(TODOS)),
    ("datos_avance_backlog", Acceso::Roles(TODOS)),
    ("datos_avance_general", Acceso::Roles(TODOS)),
    ("obtener_notificaciones", Acceso::Roles(TODOS)),
    ("marcar_leida", Acceso::Roles(TODOS)),
    ("desmarcar_leida", Acceso::Roles(TODOS)),
    ("marcar_todas_leidas", Acceso::Roles(TODOS)),
    ("enviar_mensaje", Acceso::Roles(TODOS)),
    ("solicitar_accion_bd", Acceso::Roles(LIDER_O_DESARROLLADOR)),
    ("obtener_chat", Acceso::Roles(TODOS)),
    ("conversaciones_sin_leer", Acceso::Roles(TODOS)),
    ("enviar_chat", Acceso::Roles(TODOS)),
    ("crear_grupo_chat", Acceso::Roles(TODOS)),
    ("marcar_chat_leido", Acceso::Roles(TODOS)),
    ("eliminar_chat", Acceso::Roles(TODOS)),
    ("listar_documentos", Acceso::Roles(TODOS)),
    ("subir_documentos", Acceso::Roles(TODOS)),
    ("aprobar_documento", Acceso::Roles(LIDER_O_ADMIN)),
    ("eliminar_documento", Acceso::Roles(LIDER_O_ADMIN)),
    ("abrir_documento", Acceso::Roles(TODOS)),
    ("obtener_configuracion_proyecto", Acceso::Roles(TODOS)),
    ("renombrar_proyecto", Acceso::Roles(SOLO_ADMIN)),
    (
        "actualizar_configuracion_proyecto",
        Acceso::Roles(SOLO_ADMIN),
    ),
    ("subir_imagen_proyecto", Acceso::Roles(SOLO_ADMIN)),
    ("marcar_visto", Acceso::Roles(TODOS)),
    ("obtener_vistos", Acceso::Roles(TODOS)),
];

/// Quién puede usar `comando`; `None` si no está en la tabla.
pub fn acceso(comando: &str) -> Option<Acceso> {
    PERMISOS
        .iter()
        .find(|(nombre, _)| *nombre == comando)
        .map(|(_, acceso)| *acceso)
}

/// La persona que está usando la app, con el rol que de verdad se le aplica.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsuarioActual {
    pub email: String,
    pub rol: Rol,
    /// `false` si su firma no coincide y se le aplica Desarrollador.
    pub rol_verificado: bool,
}

/// Busca a `email` (el de `cm whoami`) en la tabla `usuarios` y verifica su rol.
/// Sin fila, o con la fila pendiente de confirmar, no puede usar nada.
pub fn usuario_actual(
    c: &Connection,
    publica_admin: &str,
    email: &str,
) -> Resultado<UsuarioActual> {
    let email = normalizar_email(email);
    let verificado = verificar_usuarios(publica_admin, usuarios::listar(c)?)
        .into_iter()
        .find(|u| u.usuario.email == email)
        .ok_or(Error::SinPermiso)?;
    let rol = verificado.rol_efectivo.ok_or(Error::SinPermiso)?;
    Ok(UsuarioActual {
        email,
        rol,
        rol_verificado: verificado.verificacion == Verificacion::Verificado,
    })
}

/// `Ok` si `usuario` puede usar `comando` según [`PERMISOS`].
pub fn exigir(comando: &str, usuario: &UsuarioActual) -> Resultado<()> {
    match acceso(comando) {
        Some(Acceso::Libre) => Ok(()),
        Some(Acceso::Roles(roles)) if roles.contains(&usuario.rol) => Ok(()),
        _ => Err(Error::SinPermiso),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::usuarios::Usuario;
    use crate::db::Db;
    use crate::identidad::{firmas, llaves};

    const FECHA: &str = "2026-11-10T10:00:00Z";

    fn fila(email: &str, rol: Rol) -> Usuario {
        Usuario {
            email: email.into(),
            nombre: "N".into(),
            apellido: "A".into(),
            rol,
            fecha_alta: "2026-11-10".into(),
            firma: None,
            firmado_en: None,
            firmado_por: None,
            llave_publica: None,
            certificado: None,
        }
    }

    /// Base con un Administrador, un Tester, alguien con el rol cambiado a mano
    /// y alguien pendiente de confirmar.
    fn proyecto() -> (tempfile::TempDir, Db, String) {
        let dir = tempfile::tempdir().unwrap();
        let publica = llaves::generar_llaves_admin(dir.path(), None).unwrap();
        let admin = llaves::cargar_llave_admin(dir.path(), None).unwrap();
        let firmada = |email: &str, rol: Rol| Usuario {
            firma: Some(firmas::firmar_rol(&admin, email, rol, FECHA).unwrap()),
            firmado_en: Some(FECHA.into()),
            ..fila(email, rol)
        };
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        usuarios::insertar(c, &firmada("admin@virustf.dev", Rol::Administrador)).unwrap();
        usuarios::insertar(c, &firmada("tester@virustf.dev", Rol::Tester)).unwrap();
        usuarios::insertar(
            c,
            &Usuario {
                rol: Rol::Administrador,
                ..firmada("tramposo@virustf.dev", Rol::Tester)
            },
        )
        .unwrap();
        usuarios::insertar(c, &fila("nuevo@virustf.dev", Rol::Lider)).unwrap();
        (dir, db, publica)
    }

    #[test]
    fn el_rol_sale_de_la_firma() {
        let (_dir, db, publica) = proyecto();
        let c = db.conexion();
        let admin = usuario_actual(c, &publica, "Admin@VirusTF.dev").unwrap();
        assert_eq!(
            (admin.rol, admin.rol_verificado),
            (Rol::Administrador, true)
        );
        assert_eq!(admin.email, "admin@virustf.dev");

        // Se escribió Administrador a mano: se le aplica Desarrollador.
        let tramposo = usuario_actual(c, &publica, "tramposo@virustf.dev").unwrap();
        assert_eq!(
            (tramposo.rol, tramposo.rol_verificado),
            (Rol::Desarrollador, false)
        );

        // Pendiente de confirmar o fuera del proyecto: nada.
        for email in ["nuevo@virustf.dev", "nadie@virustf.dev"] {
            assert_eq!(
                usuario_actual(c, &publica, email).unwrap_err().codigo(),
                "sin_permiso"
            );
        }
    }

    #[test]
    fn exigir_aplica_la_tabla() {
        let (_dir, db, publica) = proyecto();
        let c = db.conexion();
        let admin = usuario_actual(c, &publica, "admin@virustf.dev").unwrap();
        let tester = usuario_actual(c, &publica, "tester@virustf.dev").unwrap();
        let tramposo = usuario_actual(c, &publica, "tramposo@virustf.dev").unwrap();

        assert!(exigir("restaurar_backup", &admin).is_ok());
        for u in [&tester, &tramposo] {
            let error = exigir("restaurar_backup", u).unwrap_err();
            assert_eq!(error.codigo(), "sin_permiso");
            assert!(exigir("ping", u).is_ok());
        }
        // Un comando que no está en la tabla se niega, también al Administrador.
        assert!(exigir("comando_inventado", &admin).is_err());
    }

    /// Los comandos registrados en `lib.rs` (`generate_handler!`).
    fn comandos_registrados() -> Vec<String> {
        let fuente = include_str!("../lib.rs");
        let lista = fuente
            .split("generate_handler![")
            .nth(1)
            .and_then(|resto| resto.split(']').next())
            .expect("lib.rs debe tener generate_handler![...]");
        lista
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(|c| c.rsplit("::").next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn todo_comando_registrado_esta_en_la_tabla_y_al_reves() {
        let registrados = comandos_registrados();
        assert!(registrados.len() >= PERMISOS.len());
        for comando in &registrados {
            assert!(
                acceso(comando).is_some(),
                "falta `{comando}` en identidad::permisos::PERMISOS"
            );
        }
        for (comando, _) in PERMISOS {
            assert!(
                registrados.iter().any(|r| r == comando),
                "`{comando}` está en PERMISOS pero no en lib.rs"
            );
        }
    }
}
