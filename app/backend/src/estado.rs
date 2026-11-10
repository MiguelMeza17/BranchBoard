//! Estado compartido de la app, registrado en Tauri (`State<AppState>`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::db::Db;
use crate::error::{Error, Resultado};
use crate::identidad::llaves;
use crate::identidad::permisos::{self, Acceso, UsuarioActual};
use crate::modelo::normalizar_email;

#[derive(Debug, Default)]
pub struct AppState {
    /// Cuántas operaciones largas (`cm`, etc.) hay corriendo. Mientras sea mayor
    /// que 0 no se debe cerrar la app (ver "Bloquear el cierre" en BACKEND.md).
    operaciones_en_curso: AtomicUsize,
    /// Base de datos del proyecto abierto; `None` mientras no se entra a ninguno.
    db: Mutex<Option<Db>>,
    /// Correo de quien usa la app (`cm whoami`); `None` sin sesión de Unity VC.
    usuario: Mutex<Option<String>>,
}

impl AppState {
    /// Guarda quién usa la app (el correo de `cm whoami`), o `None` al cerrar sesión.
    pub fn fijar_usuario(&self, email: Option<&str>) {
        *self.usuario.lock().unwrap_or_else(|e| e.into_inner()) = email.map(normalizar_email);
    }

    /// Primera línea de todo comando que depende del rol: devuelve quién lo usa
    /// si su rol (verificado por la firma) puede usar `comando` según la tabla
    /// `identidad::permisos::PERMISOS`; si no, `sin_permiso`.
    pub fn exigir_permiso(&self, comando: &str) -> Resultado<UsuarioActual> {
        match permisos::acceso(comando) {
            Some(Acceso::Roles(_)) => {}
            Some(Acceso::Libre) => {
                return Err(Error::Interno(format!(
                    "`{comando}` es libre: no lleva chequeo de rol"
                )))
            }
            None => return Err(Error::SinPermiso),
        }
        let email = self
            .usuario
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(Error::SinSesionUnityVc)?;
        let usuario = self.con_db(|db| {
            let carpeta = db.carpeta().ok_or_else(|| {
                Error::Validacion("El proyecto abierto no tiene carpeta de datos".into())
            })?;
            let publica = llaves::leer_llave_publica_admin(carpeta)?.ok_or_else(|| {
                Error::Validacion(
                    "Esta máquina todavía no tiene la llave del Administrador del proyecto".into(),
                )
            })?;
            permisos::usuario_actual(db.conexion(), &publica, &email)
        })?;
        permisos::exigir(comando, &usuario)?;
        Ok(usuario)
    }

    /// Deja abierta la base de datos del proyecto al que se entró.
    pub fn abrir_proyecto(&self, db: Db) {
        *self.db.lock().unwrap_or_else(|e| e.into_inner()) = Some(db);
    }

    /// Cierra el proyecto abierto (al cambiar de proyecto o cerrar sesión).
    pub fn cerrar_proyecto(&self) {
        *self.db.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Corre `f` con la base de datos del proyecto abierto.
    pub fn con_db<T>(&self, f: impl FnOnce(&mut Db) -> Resultado<T>) -> Resultado<T> {
        let mut guarda = self.db.lock().unwrap_or_else(|e| e.into_inner());
        match guarda.as_mut() {
            Some(db) => f(db),
            None => Err(Error::Validacion("No hay ningún proyecto abierto".into())),
        }
    }

    pub fn hay_operacion_en_curso(&self) -> bool {
        self.operaciones_en_curso.load(Ordering::SeqCst) > 0
    }

    /// Marca una operación como iniciada. Termina sola cuando se suelta la guarda,
    /// también si la operación falla.
    pub fn iniciar_operacion(&self) -> GuardaOperacion<'_> {
        self.operaciones_en_curso.fetch_add(1, Ordering::SeqCst);
        GuardaOperacion { estado: self }
    }
}

pub struct GuardaOperacion<'a> {
    estado: &'a AppState,
}

impl Drop for GuardaOperacion<'_> {
    fn drop(&mut self) {
        self.estado
            .operaciones_en_curso
            .fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn con_db_exige_un_proyecto_abierto() {
        let estado = AppState::default();
        assert!(estado.con_db(|db| db.version()).is_err());
        estado.abrir_proyecto(Db::en_memoria().unwrap());
        assert_eq!(
            estado.con_db(|db| db.version()).unwrap(),
            crate::db::VERSION_ESQUEMA
        );
        estado.cerrar_proyecto();
        assert!(estado.con_db(|db| db.version()).is_err());
    }

    #[test]
    fn la_guarda_cuenta_y_descuenta() {
        let estado = AppState::default();
        assert!(!estado.hay_operacion_en_curso());
        {
            let _a = estado.iniciar_operacion();
            let _b = estado.iniciar_operacion();
            assert!(estado.hay_operacion_en_curso());
        }
        assert!(!estado.hay_operacion_en_curso());
    }

    #[test]
    fn exigir_permiso_de_punta_a_punta() {
        use crate::db::{carpeta_proyecto, usuarios};
        use crate::identidad::firmas;
        use crate::modelo::Rol;

        let datos = tempfile::tempdir().unwrap();
        let carpeta = carpeta_proyecto(datos.path(), "VirusTF").unwrap();
        llaves::generar_llaves_admin(&carpeta, None).unwrap();
        let admin = llaves::cargar_llave_admin(&carpeta, None).unwrap();
        let db = Db::abrir(&carpeta).unwrap();
        for (email, rol) in [
            ("admin@virustf.dev", Rol::Administrador),
            ("tester@virustf.dev", Rol::Tester),
        ] {
            let fecha = "2026-11-10T10:00:00Z";
            usuarios::insertar(
                db.conexion(),
                &usuarios::Usuario {
                    email: email.into(),
                    nombre: "N".into(),
                    apellido: "A".into(),
                    rol,
                    fecha_alta: "2026-11-10".into(),
                    firma: Some(firmas::firmar_rol(&admin, email, rol, fecha).unwrap()),
                    firmado_en: Some(fecha.into()),
                    firmado_por: None,
                    llave_publica: None,
                    certificado: None,
                },
            )
            .unwrap();
        }

        let estado = AppState::default();
        let codigo = |r: Resultado<UsuarioActual>| r.unwrap_err().codigo();
        // Sin proyecto abierto o sin sesión.
        estado.fijar_usuario(Some("admin@virustf.dev"));
        assert_eq!(
            codigo(estado.exigir_permiso("restaurar_backup")),
            "validacion"
        );
        estado.abrir_proyecto(db);
        estado.fijar_usuario(None);
        assert_eq!(
            codigo(estado.exigir_permiso("restaurar_backup")),
            "sin_sesion_unity_vc"
        );

        estado.fijar_usuario(Some("Admin@VirusTF.dev"));
        let yo = estado.exigir_permiso("restaurar_backup").unwrap();
        assert_eq!(
            (yo.email.as_str(), yo.rol),
            ("admin@virustf.dev", Rol::Administrador)
        );

        estado.fijar_usuario(Some("tester@virustf.dev"));
        assert_eq!(
            codigo(estado.exigir_permiso("restaurar_backup")),
            "sin_permiso"
        );
        assert_eq!(
            codigo(estado.exigir_permiso("comando_inventado")),
            "sin_permiso"
        );
        assert_eq!(codigo(estado.exigir_permiso("ping")), "interno");
    }
}
