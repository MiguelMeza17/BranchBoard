//! Estado compartido de la app, registrado en Tauri (`State<AppState>`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::db::Db;
use crate::error::{Error, Resultado};

#[derive(Debug, Default)]
pub struct AppState {
    /// Cuántas operaciones largas (`cm`, etc.) hay corriendo. Mientras sea mayor
    /// que 0 no se debe cerrar la app (ver "Bloquear el cierre" en BACKEND.md).
    operaciones_en_curso: AtomicUsize,
    /// Base de datos del proyecto abierto; `None` mientras no se entra a ninguno.
    db: Mutex<Option<Db>>,
}

impl AppState {
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
}
