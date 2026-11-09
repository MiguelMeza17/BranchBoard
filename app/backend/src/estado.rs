//! Estado compartido de la app, registrado en Tauri (`State<AppState>`).

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
pub struct AppState {
    /// Cuántas operaciones largas (`cm`, etc.) hay corriendo. Mientras sea mayor
    /// que 0 no se debe cerrar la app (ver "Bloquear el cierre" en BACKEND.md).
    operaciones_en_curso: AtomicUsize,
}

impl AppState {
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
