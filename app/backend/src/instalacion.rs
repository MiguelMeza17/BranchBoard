//! Utilidades locales para detectar e instalar dependencias externas
//! (`winget` y apertura segura de URLs oficiales de descarga en el navegador).
//! Usado por `warp` (#122) y por la instalación de Unity VC (#123).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use crate::cm::Comando;
use crate::error::{Error, Resultado};
use crate::estado::AppState;

/// Tiempo máximo para una instalación desatendida con `winget` (puede esperar UAC y descarga).
const TIEMPO_MAXIMO_WINGET: Duration = Duration::from_secs(300);

/// Resultado común de intentar instalar una herramienta con `winget`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultadoWinget {
    /// Quedó disponible en el equipo al terminar `winget`.
    Instalado,
    /// `winget` no estuvo disponible o falló; se abrió la página oficial de descarga.
    AbiertaPaginaDescarga,
}

/// Busca un ejecutable primero en `conocidas` y después en las carpetas de `PATH`.
pub fn buscar_ejecutable(conocidas: &[PathBuf], en_path: &[&str]) -> Option<PathBuf> {
    if let Some(encontrada) = conocidas.iter().find(|ruta| ruta.is_file()) {
        return Some(encontrada.clone());
    }
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        for nombre in en_path {
            let candidato = dir.join(nombre);
            if candidato.is_file() {
                return Some(candidato);
            }
        }
    }
    None
}

/// Verifica que la URL sea HTTPS fija y sin caracteres de control o shell antes de abrirla.
pub fn validar_url_https(url: &str) -> Resultado<()> {
    if !url.starts_with("https://") {
        return Err(Error::Validacion(
            "Solo se pueden abrir direcciones HTTPS oficiales.".into(),
        ));
    }
    let sin_esquema = &url["https://".len()..];
    if sin_esquema.is_empty()
        || url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '&' | '|' | ';' | '"' | '\''))
    {
        return Err(Error::Validacion("La URL de descarga no es válida.".into()));
    }
    Ok(())
}

/// Abre una URL HTTPS oficial en el navegador predeterminado del sistema desde Rust.
pub fn abrir_url(url: &str) -> Resultado<()> {
    validar_url_https(url)?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| Error::Interno(format!("No se pudo abrir el navegador en {url}: {e}")))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| Error::Interno(format!("No se pudo abrir el navegador en {url}: {e}")))?;
        Ok(())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| Error::Interno(format!("No se pudo abrir el navegador en {url}: {e}")))?;
        Ok(())
    }
}

/// Instala un paquete por su identificador exacto de `winget`; si `winget` falta o falla
/// y la herramienta sigue sin detectarse, abre `url_fallback` en el navegador.
pub fn instalar_con_winget(
    estado: &AppState,
    id_paquete: &str,
    url_fallback: &str,
    ya_instalado: impl Fn() -> bool,
) -> Resultado<ResultadoWinget> {
    instalar_con_winget_con(
        ya_instalado,
        || {
            let salida = Comando::nuevo("winget")
                .args([
                    "install",
                    "--id",
                    id_paquete,
                    "-e",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ])
                .tiempo_maximo(TIEMPO_MAXIMO_WINGET)
                .ejecutar(estado)?;
            Ok(salida.exito())
        },
        url_fallback,
        abrir_url,
    )
}

/// Núcleo testeable de [`instalar_con_winget`] con invocadores inyectables.
pub fn instalar_con_winget_con(
    ya_instalado: impl Fn() -> bool,
    ejecutar_winget: impl FnOnce() -> Resultado<bool>,
    url_fallback: &str,
    abrir_navegador: impl FnOnce(&str) -> Resultado<()>,
) -> Resultado<ResultadoWinget> {
    if ya_instalado() {
        return Ok(ResultadoWinget::Instalado);
    }

    let _ = ejecutar_winget();
    if ya_instalado() {
        return Ok(ResultadoWinget::Instalado);
    }

    abrir_navegador(url_fallback)?;
    Ok(ResultadoWinget::AbiertaPaginaDescarga)
}

/// Construye la ruta `base/subrutas...` si `env_var` está definida.
pub fn desde_env(env_var: &str, subrutas: &[&str]) -> Option<PathBuf> {
    let base = std::env::var_os(env_var)?;
    let mut ruta = Path::new(&base).to_path_buf();
    for parte in subrutas {
        ruta.push(parte);
    }
    Some(ruta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn instala_con_winget_si_queda_disponible() {
        let llamado = Cell::new(false);
        let disponible = Cell::new(false);
        let navegador = Cell::new(false);

        let res = instalar_con_winget_con(
            || disponible.get(),
            || {
                llamado.set(true);
                disponible.set(true);
                Ok(true)
            },
            "https://1.1.1.1/download",
            |_| {
                navegador.set(true);
                Ok(())
            },
        )
        .unwrap();

        assert!(llamado.get());
        assert!(!navegador.get());
        assert_eq!(res, ResultadoWinget::Instalado);
    }

    #[test]
    fn winget_fallido_o_ausente_abre_pagina_descarga() {
        let mut url_abierta = String::new();
        let res = instalar_con_winget_con(
            || false,
            || Err(Error::Interno("winget no encontrado".into())),
            "https://1.1.1.1/download",
            |url| {
                url_abierta = url.to_string();
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(res, ResultadoWinget::AbiertaPaginaDescarga);
        assert_eq!(url_abierta, "https://1.1.1.1/download");
    }

    #[test]
    fn valida_urls_https_estrictas() {
        assert!(validar_url_https("https://1.1.1.1/download").is_ok());
        assert!(validar_url_https("http://1.1.1.1/download").is_err());
        assert!(validar_url_https("file:///C:/Windows/calc.exe").is_err());
        assert!(validar_url_https("https://ejemplo.test && calc").is_err());
    }
}
