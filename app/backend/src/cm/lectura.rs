//! Primeros comandos de **lectura** de `cm`: quién tiene la sesión, los
//! workspaces locales y en qué rama está parado uno. No cambian nada.
//!
//! Cada función corre `cm` con [`Comando`] y le pasa la salida a un
//! `interpretar_*` puro, que es lo que se prueba con salidas de ejemplo.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cm::{Comando, Salida};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::modelo::normalizar_email;

/// Un workspace local, como lo lista `cm workspace list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub nombre: String,
    /// Carpeta del workspace en esta máquina.
    pub ruta: String,
}

/// Dónde está parado un workspace, según `cm status --header`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoWorkspace {
    /// Rama completa, ej. `/main/task/T-101-menu`.
    pub rama: String,
    /// `repositorio@servidor`, ej. `VirusTF@MIGUELMEZA:8087`.
    pub repositorio: String,
    /// Changeset en el que está el workspace, si `cm` lo dice.
    pub changeset: Option<i64>,
}

/// Correo de la cuenta de Unity VC con sesión en esta máquina.
/// Sin sesión → `sin_sesion_unity_vc`; sin `cm` → `cm_no_instalado`.
pub fn whoami(estado: &AppState) -> Resultado<String> {
    let salida = Comando::cm().arg("whoami").ejecutar(estado)?;
    interpretar_whoami(&salida)
}

fn interpretar_whoami(salida: &Salida) -> Resultado<String> {
    let ultima_linea = salida
        .stdout
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or_default();
    let email = normalizar_email(ultima_linea);
    if !salida.exito() || !email.contains('@') || email.contains(char::is_whitespace) {
        return Err(Error::SinSesionUnityVc);
    }
    Ok(email)
}

/// Workspaces de Unity VC en esta máquina.
pub fn listar_workspaces(estado: &AppState) -> Resultado<Vec<Workspace>> {
    let texto = Comando::cm()
        .args(["workspace", "list"])
        .exigir_exito(estado)?;
    Ok(interpretar_workspaces(&texto))
}

/// Cada línea es `nombre@maquina ruta`; tanto el nombre como la ruta pueden
/// tener espacios, así que se parte en el primer espacio después de la `@`.
fn interpretar_workspaces(texto: &str) -> Vec<Workspace> {
    texto
        .lines()
        .filter_map(|linea| {
            let linea = linea.trim();
            let (nombre, resto) = linea.split_once('@')?;
            let (_maquina, ruta) = resto.split_once(char::is_whitespace)?;
            let (nombre, ruta) = (nombre.trim(), ruta.trim());
            (!nombre.is_empty() && !ruta.is_empty()).then(|| Workspace {
                nombre: nombre.to_string(),
                ruta: ruta.to_string(),
            })
        })
        .collect()
}

/// Rama (y repositorio y changeset) en que está parado el workspace de `carpeta`.
pub fn rama_actual_workspace(estado: &AppState, carpeta: &Path) -> Resultado<EstadoWorkspace> {
    let texto = Comando::cm()
        .args(["status", "--header"])
        .en(carpeta)
        .exigir_exito(estado)?;
    interpretar_cabecera(&texto).ok_or_else(|| {
        Error::Cm(format!(
            "No se pudo leer en qué rama está el workspace {}",
            carpeta.display()
        ))
    })
}

/// Primera línea de `cm status --header`: `/rama@repo@servidor (cs:N - head)`.
fn interpretar_cabecera(texto: &str) -> Option<EstadoWorkspace> {
    let linea = texto.lines().map(str::trim).find(|l| l.starts_with('/'))?;
    let (especificacion, resto) = match linea.split_once(" (") {
        Some((e, r)) => (e.trim(), Some(r)),
        None => (linea, None),
    };
    let (rama, repositorio) = especificacion.split_once('@')?;
    let changeset = resto
        .and_then(|r| r.split_once("cs:"))
        .map(|(_, despues)| despues.trim_start())
        .and_then(|d| {
            let digitos: String = d.chars().take_while(char::is_ascii_digit).collect();
            digitos.parse().ok()
        });
    Some(EstadoWorkspace {
        rama: rama.to_string(),
        repositorio: repositorio.trim().to_string(),
        changeset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salida(codigo: i32, stdout: &str, stderr: &str) -> Salida {
        Salida {
            codigo: Some(codigo),
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    #[test]
    fn whoami_da_el_correo_en_minusculas() {
        assert_eq!(
            interpretar_whoami(&salida(0, "Ana.Lopez@VirusTF.dev\r\n", "")).unwrap(),
            "ana.lopez@virustf.dev"
        );
        // Algunas versiones imprimen un aviso antes: cuenta la última línea.
        assert_eq!(
            interpretar_whoami(&salida(0, "Checking credentials...\nana@virustf.dev\n", ""))
                .unwrap(),
            "ana@virustf.dev"
        );
    }

    #[test]
    fn whoami_sin_sesion() {
        for s in [
            salida(1, "", "Not logged in"),
            salida(0, "", ""),
            salida(0, "The user is not logged in", ""),
            salida(0, "dos palabras@x.com", ""),
        ] {
            assert_eq!(
                interpretar_whoami(&s).unwrap_err().codigo(),
                "sin_sesion_unity_vc"
            );
        }
    }

    #[test]
    fn workspaces_con_rutas_con_espacios() {
        let texto = "VirusTF@MIGUELMEZA D:\\Unity\\VirusTF\r\n\
                     Otro juego@PC-ANA C:\\Users\\ana\\Mis proyectos\\Otro juego\n\
                     \n\
                     sin-ruta@PC\n";
        let lista = interpretar_workspaces(texto);
        assert_eq!(
            lista,
            vec![
                Workspace {
                    nombre: "VirusTF".into(),
                    ruta: "D:\\Unity\\VirusTF".into()
                },
                Workspace {
                    nombre: "Otro juego".into(),
                    ruta: "C:\\Users\\ana\\Mis proyectos\\Otro juego".into()
                },
            ]
        );
        assert!(interpretar_workspaces("").is_empty());
        assert!(interpretar_workspaces("linea sin arroba D:\\x").is_empty());
    }

    #[test]
    fn cabecera_con_rama_repo_y_changeset() {
        let estado =
            interpretar_cabecera("/main/task/T-101-menu@VirusTF@MIGUELMEZA:8087 (cs:142 - head)\n")
                .unwrap();
        assert_eq!(estado.rama, "/main/task/T-101-menu");
        assert_eq!(estado.repositorio, "VirusTF@MIGUELMEZA:8087");
        assert_eq!(estado.changeset, Some(142));

        let sin_cs = interpretar_cabecera("/main@VirusTF@localhost:8087").unwrap();
        assert_eq!(sin_cs.rama, "/main");
        assert_eq!(sin_cs.changeset, None);

        assert_eq!(interpretar_cabecera(""), None);
        assert_eq!(interpretar_cabecera("No es un workspace"), None);
        assert_eq!(
            interpretar_cabecera("/main (cs:3)"),
            None,
            "sin repositorio"
        );
    }

    fn cm_instalado() -> bool {
        std::process::Command::new("cm")
            .arg("version")
            .output()
            .is_ok()
    }

    #[test]
    fn sin_cm_instalado_el_error_es_claro() {
        if cm_instalado() {
            return;
        }
        let estado = AppState::default();
        assert_eq!(whoami(&estado).unwrap_err().codigo(), "cm_no_instalado");
        assert_eq!(
            listar_workspaces(&estado).unwrap_err().codigo(),
            "cm_no_instalado"
        );
        assert_eq!(
            rama_actual_workspace(&estado, Path::new("."))
                .unwrap_err()
                .codigo(),
            "cm_no_instalado"
        );
    }
}
