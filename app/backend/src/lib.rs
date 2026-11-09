//! Backend de BranchBoard (Tauri). Diseño completo en `documentacion/BACKEND.md`.

/// Comando de prueba: confirma que el frontend puede llamar al backend.
#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ping])
        .run(tauri::generate_context!())
        .expect("error al iniciar BranchBoard");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_responde_pong() {
        assert_eq!(ping(), "pong");
    }
}
