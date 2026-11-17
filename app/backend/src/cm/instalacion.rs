//! Detección e instalación de Unity Version Control (`cm`, #123).
//!
//! Usa [`crate::instalacion`] igual que Cloudflare WARP: busca `cm.exe` en rutas
//! conocidas o `PATH` y, si falta, instala `Codice.PlasticSCM.CloudEdition` con
//! `winget` o abre la página oficial de descarga como respaldo.

use std::path::PathBuf;

use serde::Serialize;

use crate::error::Resultado;
use crate::estado::AppState;
use crate::instalacion::{self, ResultadoWinget};

/// Identificador del paquete oficial en `winget`.
pub const ID_PAQUETE_UVCS: &str = "Codice.PlasticSCM.CloudEdition";
/// Página oficial de descarga cuando `winget` falla o no está disponible.
pub const URL_DESCARGA_UVCS: &str = "https://www.plasticscm.com/download";

/// Estado de instalación de `cm` (Unity Version Control) en esta máquina (#123).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EstadoCm {
    pub instalado: bool,
    pub ruta: Option<String>,
}

/// Respuesta de [`instalar_cm`] (#123).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResultadoInstalacionCm {
    pub instalado_ahora: bool,
    pub abrio_descarga: bool,
    pub estado: EstadoCm,
    pub mensaje: String,
}

fn candidatos_cm_windows() -> Vec<PathBuf> {
    let mut rutas = vec![
        PathBuf::from(r"C:\Program Files\PlasticSCM5\client\cm.exe"),
        PathBuf::from(r"C:\Program Files\Unity Version Control\cm.exe"),
    ];
    for subruta in [
        ["PlasticSCM5", "client", "cm.exe"].as_slice(),
        ["Unity Version Control", "cm.exe"].as_slice(),
    ] {
        if let Some(pf) = instalacion::desde_env("ProgramFiles", subruta) {
            if !rutas.contains(&pf) {
                rutas.push(pf);
            }
        }
    }
    rutas
}

/// Busca `cm` en `candidatas` y después en `PATH`.
pub fn buscar_cm_en(candidatas: &[PathBuf]) -> Option<PathBuf> {
    #[cfg(windows)]
    let en_path = &["cm.exe"];
    #[cfg(not(windows))]
    let en_path = &["cm"];

    instalacion::buscar_ejecutable(candidatas, en_path)
}

/// Devuelve la ruta al binario `cm` instalado en el sistema, si existe.
pub fn resolver_cm() -> Option<PathBuf> {
    buscar_cm_en(&candidatos_cm_windows())
}

/// Consulta si `cm` está disponible en este equipo (#123).
pub fn estado_cm() -> EstadoCm {
    let ruta = resolver_cm().map(|r| r.to_string_lossy().to_string());
    EstadoCm {
        instalado: ruta.is_some(),
        ruta,
    }
}

/// Instala Unity Version Control con `winget`; si falla o no está disponible,
/// abre `https://www.plasticscm.com/download` en el navegador (#123).
pub fn instalar_cm(estado: &AppState) -> Resultado<ResultadoInstalacionCm> {
    if resolver_cm().is_some() {
        return Ok(ResultadoInstalacionCm {
            instalado_ahora: false,
            abrio_descarga: false,
            estado: estado_cm(),
            mensaje: "Unity Version Control (cm) ya está instalado en este equipo.".into(),
        });
    }

    let resultado =
        instalacion::instalar_con_winget(estado, ID_PAQUETE_UVCS, URL_DESCARGA_UVCS, || {
            resolver_cm().is_some()
        })?;
    let actual = estado_cm();

    match resultado {
        ResultadoWinget::Instalado => Ok(ResultadoInstalacionCm {
            instalado_ahora: true,
            abrio_descarga: false,
            estado: actual,
            mensaje: "Unity Version Control instalado correctamente.".into(),
        }),
        ResultadoWinget::AbiertaPaginaDescarga => Ok(ResultadoInstalacionCm {
            instalado_ahora: false,
            abrio_descarga: true,
            estado: actual,
            mensaje: "No se pudo instalar con winget. Se abrió la página oficial de descarga de Unity Version Control en el navegador.".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cm::Comando;
    use crate::error::Error;

    #[test]
    fn busca_cm_en_candidatas() {
        let dir = tempfile::tempdir().unwrap();
        let ausente = dir.path().join("no-cm.exe");
        let real = dir.path().join("cm.exe");
        std::fs::write(&real, b"stub").unwrap();
        assert_eq!(buscar_cm_en(&[ausente, real.clone()]), Some(real));
    }

    #[test]
    fn winget_fallido_abre_pagina_oficial_de_uvcs() {
        let mut abierta = String::new();
        let res = crate::instalacion::instalar_con_winget_con(
            || false,
            || Err(Error::Interno("sin winget".into())),
            URL_DESCARGA_UVCS,
            |url| {
                abierta = url.to_string();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(res, ResultadoWinget::AbiertaPaginaDescarga);
        assert_eq!(abierta, "https://www.plasticscm.com/download");
    }

    /// **Prueba en el entorno real** (CLAUDE.md):
    /// 1. Detecta `cm.exe` real en el equipo (`resolver_cm` y `estado_cm`).
    /// 2. Ejecuta `cm version` a través de `Comando::cm()`.
    /// 3. Confirma que `instalar_cm` reconoce la instalación existente sin reinstalar ni abrir navegador.
    /// 4. Comprueba `cm whoami` y consulta ramas de `BranchBoard-Pruebas` en Unity VC Cloud.
    #[test]
    #[ignore = "usa Unity VC de verdad: correr a mano con BB_PRUEBA_ORG y BB_PRUEBA_REPO"]
    fn entorno_real_cm_instalacion() {
        let repo = std::env::var("BB_PRUEBA_REPO")
            .unwrap_or_else(|_| "BranchBoard-Pruebas/BranchBoard-Pruebas".into());
        let org = std::env::var("BB_PRUEBA_ORG").unwrap_or_else(|_| "MiguelM17".into());
        let estado = AppState::default();

        let actual = estado_cm();
        assert!(
            actual.instalado,
            "cm.exe debe estar instalado en este equipo"
        );
        let ruta = actual.ruta.as_deref().unwrap();
        assert!(PathBuf::from(ruta).is_file(), "no existe {ruta}");

        let version = Comando::cm().arg("version").exigir_exito(&estado).unwrap();
        assert!(!version.trim().is_empty());

        let instalar = instalar_cm(&estado).unwrap();
        assert!(!instalar.instalado_ahora);
        assert!(!instalar.abrio_descarga);
        assert!(instalar.estado.instalado);

        let usuario = crate::cm::lectura::whoami(&estado).unwrap();
        let repo_spec = format!("'{repo}@{org}@unity'");
        let ramas = Comando::cm()
            .args([
                "find",
                "branch",
                "where",
                "parent='/main'",
                "on",
                "repository",
                &repo_spec,
            ])
            .exigir_exito(&estado)
            .unwrap();
        assert!(ramas.contains("/main/dev"));
        println!(
            "cm detectado en {ruta} (version={}), whoami={usuario}",
            version.trim()
        );
    }
}
