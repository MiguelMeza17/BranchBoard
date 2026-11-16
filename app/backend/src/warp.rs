//! Control local de Cloudflare WARP (`warp-cli`) para el switch de VPN (#122).
//!
//! En redes que bloquean puertos de Unity Version Control, activar WARP permite
//! que `cm` conecte al servidor. Solo invoca los subcomandos fijos `status`,
//! `connect` y `disconnect` mediante [`Comando`].

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cm::Comando;
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::instalacion::{self, ResultadoWinget};

/// Identificador del paquete oficial en `winget`.
pub const ID_PAQUETE_WARP: &str = "Cloudflare.Warp";
/// Página oficial de descarga cuando `winget` no está disponible o falla.
pub const URL_DESCARGA_WARP: &str = "https://1.1.1.1/download";

const TIEMPO_MAXIMO_WARP: Duration = Duration::from_secs(20);
const ESPERA_TRANSICION: Duration = Duration::from_secs(15);
const INTERVALO_POLL_ESTADO: Duration = Duration::from_millis(250);

/// Estado concreto de la conexión de Cloudflare WARP en esta máquina.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EstadoConexionWarp {
    NoInstalado,
    Conectado,
    Conectando,
    Desconectado,
    SinRegistro,
    Desconocido,
}

/// Estado actual de Cloudflare WARP para el frontend (#122).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EstadoWarp {
    pub instalado: bool,
    pub conectado: bool,
    pub conectando: bool,
    pub estado: EstadoConexionWarp,
    pub detalle: Option<String>,
}

/// Respuesta de [`instalar_warp`] (#122).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResultadoInstalacionWarp {
    pub instalado_ahora: bool,
    pub abrio_descarga: bool,
    pub estado: EstadoWarp,
    pub mensaje: String,
}

#[derive(Debug, Deserialize)]
struct StatusJson {
    #[serde(default)]
    status: String,
    #[serde(default)]
    reason: Option<serde_json::Value>,
}

fn extraer_detalle_razon(reason: Option<serde_json::Value>) -> Option<String> {
    match reason? {
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_json::Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
}

impl EstadoWarp {
    pub fn no_instalado() -> Self {
        EstadoWarp {
            instalado: false,
            conectado: false,
            conectando: false,
            estado: EstadoConexionWarp::NoInstalado,
            detalle: None,
        }
    }
}

/// Rutas estándar donde vive `warp-cli.exe` en Windows.
fn candidatos_windows() -> Vec<PathBuf> {
    let mut rutas = vec![PathBuf::from(
        r"C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe",
    )];
    if let Some(pf) = instalacion::desde_env("ProgramFiles", &["Cloudflare", "Cloudflare WARP", "warp-cli.exe"]) {
        if !rutas.contains(&pf) {
            rutas.push(pf);
        }
    }
    rutas
}

/// Busca `warp-cli` en la lista de rutas candidatas y luego en `PATH`.
pub fn buscar_warp_cli_en(candidatas: &[PathBuf]) -> Option<PathBuf> {
    #[cfg(windows)]
    let en_path = &["warp-cli.exe"];
    #[cfg(not(windows))]
    let en_path = &["warp-cli"];

    instalacion::buscar_ejecutable(candidatas, en_path)
}

/// Devuelve la ruta al binario `warp-cli` instalado en el sistema, si existe.
pub fn resolver_warp_cli() -> Option<PathBuf> {
    buscar_warp_cli_en(&candidatos_windows())
}

fn clasificar_estado(texto: &str, detalle: Option<String>) -> EstadoWarp {
    let normalizado = texto.trim().to_ascii_lowercase().replace(' ', "");
    let (estado, conectado, conectando) = if normalizado == "connected"
        || normalizado.contains("statusupdate:connected")
    {
        (EstadoConexionWarp::Conectado, true, false)
    } else if normalizado == "connecting" || normalizado.contains("statusupdate:connecting") {
        (EstadoConexionWarp::Conectando, false, true)
    } else if normalizado == "disconnected"
        || normalizado.contains("statusupdate:disconnected")
    {
        (EstadoConexionWarp::Desconectado, false, false)
    } else if normalizado.contains("registrationmissing") {
        (EstadoConexionWarp::SinRegistro, false, false)
    } else {
        (EstadoConexionWarp::Desconocido, false, false)
    };

    EstadoWarp {
        instalado: true,
        conectado,
        conectando,
        estado,
        detalle,
    }
}

/// Interpreta la salida JSON o en texto plano de `warp-cli status`.
pub fn interpretar_estado_warp(stdout: &str, stderr: &str) -> EstadoWarp {
    let trimmed = stdout.trim();
    if trimmed.starts_with('{') {
        if let Ok(parsed) = serde_json::from_str::<StatusJson>(trimmed) {
            let detalle = extraer_detalle_razon(parsed.reason);
            return clasificar_estado(&parsed.status, detalle);
        }
    }

    let combinado = if trimmed.is_empty() {
        stderr.trim()
    } else {
        trimmed
    };
    let detalle = combinado
        .lines()
        .find(|l| !l.trim().is_empty() && !l.contains("Status update:"))
        .map(|l| l.trim().to_string());
    clasificar_estado(combinado, detalle)
}

fn ejecutar_warp(estado: &AppState, ruta: &Path, args: &[&str]) -> Resultado<crate::cm::Salida> {
    Comando::nuevo(&ruta.to_string_lossy())
        .args(args.iter().copied())
        .tiempo_maximo(TIEMPO_MAXIMO_WARP)
        .ejecutar(estado)
}

fn consultar_estado_con_ruta(estado: &AppState, ruta: &Path) -> Resultado<EstadoWarp> {
    let salida = ejecutar_warp(estado, ruta, &["--json", "status"])?;
    if !salida.exito() {
        let falla = format!("{}\n{}", salida.stderr, salida.stdout);
        if falla.contains("--json") || falla.to_ascii_lowercase().contains("unexpected argument") {
            let plano = ejecutar_warp(estado, ruta, &["status"])?;
            if plano.exito() || plano.stdout.to_ascii_lowercase().contains("registration") {
                return Ok(interpretar_estado_warp(&plano.stdout, &plano.stderr));
            }
            let msg = if plano.stderr.trim().is_empty() {
                plano.stdout.trim()
            } else {
                plano.stderr.trim()
            };
            return Err(Error::Interno(format!(
                "No se pudo consultar Cloudflare WARP: {msg}"
            )));
        }
        let interpretado = interpretar_estado_warp(&salida.stdout, &salida.stderr);
        if interpretado.estado != EstadoConexionWarp::Desconocido {
            return Ok(interpretado);
        }
        return Err(Error::Interno(format!(
            "No se pudo consultar Cloudflare WARP: {}",
            falla.trim()
        )));
    }
    Ok(interpretar_estado_warp(&salida.stdout, &salida.stderr))
}

/// Consulta si `warp-cli` está instalado y si el túnel VPN está activo (#122).
pub fn estado_warp(estado: &AppState) -> Resultado<EstadoWarp> {
    let Some(ruta) = resolver_warp_cli() else {
        return Ok(EstadoWarp::no_instalado());
    };
    consultar_estado_con_ruta(estado, &ruta)
}

/// Conecta o desconecta Cloudflare WARP según su estado actual (#122).
pub fn alternar_warp(estado: &AppState) -> Resultado<EstadoWarp> {
    let Some(ruta) = resolver_warp_cli() else {
        return Err(Error::Validacion(
            "Cloudflare WARP no está instalado en este equipo.".into(),
        ));
    };

    let actual = consultar_estado_con_ruta(estado, &ruta)?;
    if actual.conectado || actual.conectando {
        let salida = ejecutar_warp(estado, &ruta, &["disconnect"])?;
        if !salida.exito() {
            return Err(Error::Interno(format!(
                "No se pudo desconectar Cloudflare WARP: {}",
                mensaje_salida(&salida)
            )));
        }
        return esperar_estado(estado, &ruta, false);
    }

    if actual.estado == EstadoConexionWarp::SinRegistro {
        return Err(Error::Validacion(
            "Cloudflare WARP aún no está registrado en este equipo. Ábrelo una vez para completar la configuración inicial.".into(),
        ));
    }

    let salida = ejecutar_warp(estado, &ruta, &["connect"])?;
    if !salida.exito() {
        let msg = mensaje_salida(&salida);
        if msg.to_ascii_lowercase().contains("registration") {
            return Err(Error::Validacion(
                "Cloudflare WARP aún no está registrado en este equipo. Ábrelo una vez para completar la configuración inicial.".into(),
            ));
        }
        return Err(Error::Interno(format!(
            "No se pudo conectar Cloudflare WARP: {msg}"
        )));
    }
    esperar_estado(estado, &ruta, true)
}

fn esperar_estado(
    estado: &AppState,
    ruta: &Path,
    esperar_conectado: bool,
) -> Resultado<EstadoWarp> {
    let inicio = Instant::now();
    loop {
        let actual = consultar_estado_con_ruta(estado, ruta)?;
        let listo = if esperar_conectado {
            actual.conectado
        } else {
            !actual.conectado && !actual.conectando
        };
        if listo || inicio.elapsed() >= ESPERA_TRANSICION {
            return Ok(actual);
        }
        thread::sleep(INTERVALO_POLL_ESTADO);
    }
}

fn mensaje_salida(salida: &crate::cm::Salida) -> String {
    let stderr = salida.stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    let stdout = salida.stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }
    match salida.codigo {
        Some(c) => format!("terminó con código {c}"),
        None => "proceso interrumpido".into(),
    }
}

/// Instala Cloudflare WARP usando `winget`; si falla o no está disponible,
/// abre `https://1.1.1.1/download` en el navegador predeterminado (#122).
pub fn instalar_warp(estado: &AppState) -> Resultado<ResultadoInstalacionWarp> {
    if resolver_warp_cli().is_some() {
        let actual = estado_warp(estado)?;
        return Ok(ResultadoInstalacionWarp {
            instalado_ahora: false,
            abrio_descarga: false,
            estado: actual,
            mensaje: "Cloudflare WARP ya está instalado en este equipo.".into(),
        });
    }

    let resultado = instalacion::instalar_con_winget(
        estado,
        ID_PAQUETE_WARP,
        URL_DESCARGA_WARP,
        || resolver_warp_cli().is_some(),
    )?;
    let actual = estado_warp(estado)?;

    match resultado {
        ResultadoWinget::Instalado => Ok(ResultadoInstalacionWarp {
            instalado_ahora: true,
            abrio_descarga: false,
            estado: actual,
            mensaje: "Cloudflare WARP instalado correctamente.".into(),
        }),
        ResultadoWinget::AbiertaPaginaDescarga => Ok(ResultadoInstalacionWarp {
            instalado_ahora: false,
            abrio_descarga: true,
            estado: actual,
            mensaje: "No se pudo instalar con winget. Se abrió la página oficial de descarga de Cloudflare WARP en el navegador.".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_json_de_warp_cli() {
        let conectado =
            interpretar_estado_warp("{\n  \"status\": \"Connected\",\n  \"reason\": \"NetworkHealthy\"\n}", "");
        assert!(conectado.instalado);
        assert!(conectado.conectado);
        assert!(!conectado.conectando);
        assert_eq!(conectado.estado, EstadoConexionWarp::Conectado);
        assert_eq!(conectado.detalle.as_deref(), Some("NetworkHealthy"));

        let conectando = interpretar_estado_warp(r#"{"status":"Connecting"}"#, "");
        assert!(!conectando.conectado);
        assert!(conectando.conectando);
        assert_eq!(conectando.estado, EstadoConexionWarp::Conectando);

        let desconectado = interpretar_estado_warp(r#"{"status":"Disconnected"}"#, "");
        assert!(!desconectado.conectado);
        assert!(!desconectado.conectando);
        assert_eq!(desconectado.estado, EstadoConexionWarp::Desconectado);

        let desconectado_objeto = interpretar_estado_warp(
            r#"{"status":"Disconnected","reason":{"SettingsChanged":{"current":{"always_on":false}}}}"#,
            "",
        );
        assert!(!desconectado_objeto.conectado);
        assert_eq!(desconectado_objeto.estado, EstadoConexionWarp::Desconectado);
        assert_eq!(
            desconectado_objeto.detalle.as_deref(),
            Some("SettingsChanged")
        );

        let sin_reg = interpretar_estado_warp(r#"{"status":"Registration Missing"}"#, "");
        assert_eq!(sin_reg.estado, EstadoConexionWarp::SinRegistro);
    }

    #[test]
    fn interpreta_texto_plano_de_warp_cli() {
        let conectado = interpretar_estado_warp("Status update: Connected\nNetwork: healthy\n", "");
        assert!(conectado.conectado);
        assert_eq!(conectado.estado, EstadoConexionWarp::Conectado);

        let desconectado = interpretar_estado_warp("Status update: Disconnected\n", "");
        assert!(!desconectado.conectado);
        assert_eq!(desconectado.estado, EstadoConexionWarp::Desconectado);
    }

    #[test]
    fn busca_candidata_instalada() {
        let dir = tempfile::tempdir().unwrap();
        let falso = dir.path().join("no-existe.exe");
        let real = dir.path().join("warp-cli.exe");
        std::fs::write(&real, b"stub").unwrap();
        let encontrada = buscar_warp_cli_en(&[falso, real.clone()]);
        assert_eq!(encontrada, Some(real));
    }

    /// **Prueba en el entorno real** (CLAUDE.md):
    /// 1. Detecta `warp-cli` instalado en el equipo y consulta su estado inicial.
    /// 2. Alterna WARP dos veces (`alternar_warp`) comprobando el cambio real y el
    ///    regreso exacto al estado inicial (con guarda `Drop` para nunca dejar la VPN caída).
    /// 3. Confirma que `instalar_warp` reconoce la instalación existente sin reinstalar ni abrir navegador.
    /// 4. Verifica que `cm whoami` y el repo de pruebas `BranchBoard-Pruebas` responden con normalidad.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   cargo test --lib entorno_real_warp -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa Cloudflare WARP y Unity VC de verdad: correr a mano"]
    fn entorno_real_warp() {
        let repo = std::env::var("BB_PRUEBA_REPO")
            .unwrap_or_else(|_| "BranchBoard-Pruebas/BranchBoard-Pruebas".into());
        let org = std::env::var("BB_PRUEBA_ORG").unwrap_or_else(|_| "MiguelM17".into());
        let estado = AppState::default();

        let ruta = resolver_warp_cli().expect("warp-cli.exe debe estar instalado en este equipo");
        println!("warp-cli detectado en: {}", ruta.display());

        let inicial = estado_warp(&estado).unwrap();
        assert!(inicial.instalado);
        assert!(
            matches!(
                inicial.estado,
                EstadoConexionWarp::Conectado | EstadoConexionWarp::Desconectado
            ),
            "estado inicial inesperado de WARP: {:?}",
            inicial
        );

        struct RestaurarWarp {
            conectado_inicial: bool,
        }
        impl Drop for RestaurarWarp {
            fn drop(&mut self) {
                let estado = AppState::default();
                if let Ok(actual) = estado_warp(&estado) {
                    if actual.instalado && actual.conectado != self.conectado_inicial {
                        let _ = alternar_warp(&estado);
                    }
                }
            }
        }
        let _guarda = RestaurarWarp {
            conectado_inicial: inicial.conectado,
        };

        let tras_primero = alternar_warp(&estado).unwrap();
        assert_ne!(
            tras_primero.conectado, inicial.conectado,
            "primer alternar_warp no cambió el estado conectado"
        );
        if inicial.conectado {
            assert_eq!(tras_primero.estado, EstadoConexionWarp::Desconectado);
        } else {
            assert_eq!(tras_primero.estado, EstadoConexionWarp::Conectado);
        }

        let tras_segundo = alternar_warp(&estado).unwrap();
        assert_eq!(tras_segundo.conectado, inicial.conectado);

        let instalar_sin_cambios = instalar_warp(&estado).unwrap();
        assert!(!instalar_sin_cambios.instalado_ahora);
        assert!(!instalar_sin_cambios.abrio_descarga);
        assert!(instalar_sin_cambios.estado.instalado);

        // Asegurar conectividad final con Unity VC (BranchBoard-Pruebas).
        let usuario = crate::cm::lectura::whoami(&estado).unwrap();
        assert!(!usuario.trim().is_empty());
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
        assert!(ramas.contains("/main/dev"), "respuesta inesperada: {ramas}");
        println!(
            "WARP verificado (inicial={}, primero={}, final={}) y cm whoami={usuario}",
            inicial.conectado, tras_primero.conectado, tras_segundo.conectado
        );
    }
}
