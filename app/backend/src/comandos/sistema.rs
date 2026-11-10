//! Comandos generales de la app (no dependen de un proyecto).

use tauri::State;

use crate::error::Resultado;
use crate::estado::AppState;

/// Evento que recibe el frontend cuando se intentó cerrar la ventana con una
/// operación en curso y el cierre se canceló.
pub const EVENTO_CIERRE_BLOQUEADO: &str = "cierre-bloqueado";

/// Comando de prueba: confirma que el frontend puede llamar al backend.
#[tauri::command]
pub fn ping() -> Resultado<String> {
    Ok("pong".to_string())
}

/// `true` mientras haya una operación larga corriendo; el frontend lo consulta
/// antes de dejar cerrar la ventana.
#[tauri::command]
pub fn hay_operacion_en_curso(estado: State<'_, AppState>) -> Resultado<bool> {
    Ok(estado.hay_operacion_en_curso())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_responde_pong() {
        assert_eq!(ping().unwrap(), "pong");
    }
}
