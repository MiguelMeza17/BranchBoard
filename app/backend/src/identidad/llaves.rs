//! Llaves Ed25519 del Administrador (issue #85). Ver "Firma criptográfica del
//! rol" en BACKEND.md.
//!
//! - `admin.key`: la llave **privada**. Solo existe en la máquina del
//!   Administrador, en la carpeta local del proyecto (nunca en Drive/OneDrive ni
//!   en `ops/`). Se puede cifrar en reposo con una passphrase.
//! - `admin.pub`: la llave **pública** del proyecto, que cada máquina fija la
//!   primera vez que entra. Si después llega otra distinta, se rechaza.
//!
//! Nada de esto se le entrega a la webview: solo lo lee el proceso de Rust.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use argon2::Argon2;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};

/// Llave privada del Administrador, dentro de la carpeta local del proyecto.
pub const ARCHIVO_LLAVE_ADMIN: &str = "admin.key";
/// Llave pública del Administrador que esta máquina fijó para el proyecto.
pub const ARCHIVO_LLAVE_PUBLICA: &str = "admin.pub";
/// Largo mínimo de la passphrase que cifra la llave.
pub const LARGO_MINIMO_PASSPHRASE: usize = 8;

const VERSION_ARCHIVO: u8 = 1;

/// Contenido de `admin.key` y del archivo exportado (JSON).
#[derive(Debug, Serialize, Deserialize)]
struct ArchivoLlave {
    version: u8,
    /// Llave pública (base64), para comprobar que la privada es la que dice ser.
    publica: String,
    cifrada: bool,
    /// Sal de Argon2id (base64). Solo si está cifrada.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sal: Option<String>,
    /// Nonce de XChaCha20-Poly1305 (base64). Solo si está cifrada.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
    /// La semilla de 32 bytes (base64), en claro o cifrada.
    llave: String,
}

/// Llave privada del Administrador ya cargada en memoria, lista para firmar.
pub struct LlaveAdmin {
    firma: SigningKey,
}

impl std::fmt::Debug for LlaveAdmin {
    /// Nunca muestra la llave privada (ni en logs ni en mensajes de error).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlaveAdmin")
            .field("publica", &self.publica())
            .finish_non_exhaustive()
    }
}

impl LlaveAdmin {
    /// Llave pública en base64.
    pub fn publica(&self) -> String {
        B64.encode(self.firma.verifying_key().as_bytes())
    }

    /// Firma `mensaje` y devuelve la firma en base64.
    pub fn firmar(&self, mensaje: &str) -> String {
        B64.encode(self.firma.sign(mensaje.as_bytes()).to_bytes())
    }
}

/// `true` si `firma` (base64) es la firma de `mensaje` hecha con la llave
/// privada que corresponde a `publica` (base64). Cualquier dato mal formado da `false`.
pub fn verificar(publica: &str, mensaje: &str, firma: &str) -> bool {
    let Ok(llave) = decodificar_publica(publica) else {
        return false;
    };
    let Ok(bytes) = B64.decode(firma.trim()) else {
        return false;
    };
    let Ok(firma) = Signature::from_slice(&bytes) else {
        return false;
    };
    llave.verify(mensaje.as_bytes(), &firma).is_ok()
}

fn decodificar_publica(publica: &str) -> Resultado<VerifyingKey> {
    let invalida = || Error::Validacion("La llave pública del Administrador no es válida".into());
    let bytes: [u8; 32] = B64
        .decode(publica.trim())
        .map_err(|_| invalida())?
        .try_into()
        .map_err(|_| invalida())?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| invalida())
}

fn validar_passphrase(passphrase: &str) -> Resultado<()> {
    if passphrase.chars().count() < LARGO_MINIMO_PASSPHRASE {
        return Err(Error::Validacion(format!(
            "La contraseña de la llave debe tener al menos {LARGO_MINIMO_PASSPHRASE} caracteres"
        )));
    }
    Ok(())
}

fn cifrador(passphrase: &str, sal: &[u8]) -> Resultado<XChaCha20Poly1305> {
    let mut clave = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), sal, &mut clave)
        .map_err(|e| Error::Interno(format!("no se pudo derivar la clave: {e}")))?;
    let cifrador = XChaCha20Poly1305::new((&clave).into());
    clave.fill(0);
    Ok(cifrador)
}

/// Arma el archivo de la llave: en claro si no hay passphrase, cifrado si la hay.
fn empaquetar(llave: &LlaveAdmin, passphrase: Option<&str>) -> Resultado<ArchivoLlave> {
    let mut semilla = llave.firma.to_bytes();
    let archivo = match passphrase {
        None => ArchivoLlave {
            version: VERSION_ARCHIVO,
            publica: llave.publica(),
            cifrada: false,
            sal: None,
            nonce: None,
            llave: B64.encode(semilla),
        },
        Some(passphrase) => {
            validar_passphrase(passphrase)?;
            let mut sal = [0u8; 16];
            let mut nonce = [0u8; 24];
            OsRng.fill_bytes(&mut sal);
            OsRng.fill_bytes(&mut nonce);
            let cifrado = cifrador(passphrase, &sal)?
                .encrypt(XNonce::from_slice(&nonce), semilla.as_ref())
                .map_err(|_| Error::Interno("no se pudo cifrar la llave".into()))?;
            ArchivoLlave {
                version: VERSION_ARCHIVO,
                publica: llave.publica(),
                cifrada: true,
                sal: Some(B64.encode(sal)),
                nonce: Some(B64.encode(nonce)),
                llave: B64.encode(cifrado),
            }
        }
    };
    semilla.fill(0);
    Ok(archivo)
}

/// Recupera la llave privada de un archivo. Si está cifrada exige la passphrase.
fn desempaquetar(texto: &str, passphrase: Option<&str>) -> Resultado<LlaveAdmin> {
    let danado = || Error::Validacion("El archivo de la llave está dañado".into());
    let archivo: ArchivoLlave = serde_json::from_str(texto).map_err(|_| danado())?;
    if archivo.version != VERSION_ARCHIVO {
        return Err(Error::Validacion(
            "El archivo de la llave es de una versión que esta app no conoce".into(),
        ));
    }
    let contenido = B64.decode(&archivo.llave).map_err(|_| danado())?;
    let mut semilla: Vec<u8> = if archivo.cifrada {
        let passphrase = passphrase.ok_or_else(|| {
            Error::Validacion("La llave está cifrada: hace falta su contraseña".into())
        })?;
        let sal = B64
            .decode(archivo.sal.as_deref().ok_or_else(danado)?)
            .map_err(|_| danado())?;
        let nonce = B64
            .decode(archivo.nonce.as_deref().ok_or_else(danado)?)
            .map_err(|_| danado())?;
        if nonce.len() != 24 {
            return Err(danado());
        }
        cifrador(passphrase, &sal)?
            .decrypt(XNonce::from_slice(&nonce), contenido.as_ref())
            .map_err(|_| Error::Validacion("La contraseña de la llave no es correcta".into()))?
    } else {
        contenido
    };
    let bytes: [u8; 32] = semilla.as_slice().try_into().map_err(|_| danado())?;
    semilla.fill(0);
    let llave = LlaveAdmin {
        firma: SigningKey::from_bytes(&bytes),
    };
    if llave.publica() != archivo.publica.trim() {
        return Err(danado());
    }
    Ok(llave)
}

/// Escribe un archivo nuevo; falla si ya existe (nunca pisa una llave).
fn escribir_nuevo(ruta: &Path, texto: &str) -> Resultado<()> {
    let mut opciones = OpenOptions::new();
    opciones.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.mode(0o600);
    }
    let mut archivo = opciones.open(ruta)?;
    archivo.write_all(texto.as_bytes())?;
    archivo.sync_all()?;
    Ok(())
}

fn a_json(archivo: &ArchivoLlave) -> Resultado<String> {
    serde_json::to_string_pretty(archivo).map_err(|e| Error::Interno(e.to_string()))
}

/// `true` si esta máquina tiene la llave privada del Administrador del proyecto.
pub fn existe_llave_admin(carpeta_proyecto: &Path) -> bool {
    carpeta_proyecto.join(ARCHIVO_LLAVE_ADMIN).is_file()
}

/// Genera el par de llaves del Administrador al configurar el proyecto por
/// primera vez. Guarda la privada en `admin.key` (cifrada si se da una
/// passphrase), fija la pública en esta máquina y la devuelve en base64.
/// Si ya hay una llave, no la pisa.
pub fn generar_llaves_admin(
    carpeta_proyecto: &Path,
    passphrase: Option<&str>,
) -> Resultado<String> {
    if existe_llave_admin(carpeta_proyecto) {
        return Err(Error::Validacion(
            "Este proyecto ya tiene llaves de Administrador en esta máquina".into(),
        ));
    }
    if leer_llave_publica_admin(carpeta_proyecto)?.is_some() {
        return Err(Error::Validacion(
            "Este proyecto ya tiene un Administrador con otra llave".into(),
        ));
    }
    let llave = LlaveAdmin {
        firma: SigningKey::generate(&mut OsRng),
    };
    let archivo = empaquetar(&llave, passphrase)?;
    std::fs::create_dir_all(carpeta_proyecto)?;
    escribir_nuevo(
        &carpeta_proyecto.join(ARCHIVO_LLAVE_ADMIN),
        &a_json(&archivo)?,
    )?;
    let publica = llave.publica();
    fijar_llave_publica_admin(carpeta_proyecto, &publica)?;
    Ok(publica)
}

/// Carga la llave privada para firmar. Si está cifrada hace falta la passphrase.
pub fn cargar_llave_admin(
    carpeta_proyecto: &Path,
    passphrase: Option<&str>,
) -> Resultado<LlaveAdmin> {
    let ruta = carpeta_proyecto.join(ARCHIVO_LLAVE_ADMIN);
    if !ruta.is_file() {
        return Err(Error::NoEncontrado(
            "la llave del Administrador en esta máquina".into(),
        ));
    }
    desempaquetar(&std::fs::read_to_string(ruta)?, passphrase)
}

/// `true` si `admin.key` está cifrada y hay que pedir la passphrase para firmar.
pub fn llave_admin_cifrada(carpeta_proyecto: &Path) -> Resultado<bool> {
    let ruta = carpeta_proyecto.join(ARCHIVO_LLAVE_ADMIN);
    if !ruta.is_file() {
        return Err(Error::NoEncontrado(
            "la llave del Administrador en esta máquina".into(),
        ));
    }
    let archivo: ArchivoLlave = serde_json::from_str(&std::fs::read_to_string(ruta)?)
        .map_err(|_| Error::Validacion("El archivo de la llave está dañado".into()))?;
    Ok(archivo.cifrada)
}

/// Respaldo de la llave privada: devuelve el texto del archivo **siempre
/// cifrado** con `passphrase`. Si `admin.key` ya está cifrada, la passphrase
/// tiene que ser la misma.
pub fn exportar_llave_firma(carpeta_proyecto: &Path, passphrase: &str) -> Resultado<String> {
    validar_passphrase(passphrase)?;
    let actual = if llave_admin_cifrada(carpeta_proyecto)? {
        Some(passphrase)
    } else {
        None
    };
    let llave = cargar_llave_admin(carpeta_proyecto, actual)?;
    a_json(&empaquetar(&llave, Some(passphrase))?)
}

/// Trae a esta máquina una llave exportada con [`exportar_llave_firma`] (llave
/// recuperada de un respaldo, o Administrador nuevo que la recibe). Queda
/// cifrada con la misma passphrase. Devuelve la llave pública.
pub fn importar_llave_firma(
    carpeta_proyecto: &Path,
    exportado: &str,
    passphrase: &str,
) -> Resultado<String> {
    if existe_llave_admin(carpeta_proyecto) {
        return Err(Error::Validacion(
            "Este proyecto ya tiene llaves de Administrador en esta máquina".into(),
        ));
    }
    let llave = desempaquetar(exportado, Some(passphrase))?;
    let publica = llave.publica();
    // Si esta máquina ya conoce al Administrador del proyecto, tiene que ser el mismo.
    fijar_llave_publica_admin(carpeta_proyecto, &publica)?;
    let archivo = empaquetar(&llave, Some(passphrase))?;
    escribir_nuevo(
        &carpeta_proyecto.join(ARCHIVO_LLAVE_ADMIN),
        &a_json(&archivo)?,
    )?;
    Ok(publica)
}

/// Llave pública del Administrador que esta máquina fijó, si ya entró al proyecto.
pub fn leer_llave_publica_admin(carpeta_proyecto: &Path) -> Resultado<Option<String>> {
    let ruta = carpeta_proyecto.join(ARCHIVO_LLAVE_PUBLICA);
    if !ruta.is_file() {
        return Ok(None);
    }
    let publica = std::fs::read_to_string(ruta)?.trim().to_string();
    decodificar_publica(&publica)?;
    Ok(Some(publica))
}

/// Fija la llave pública del Administrador la primera vez que esta máquina entra
/// al proyecto. Si ya había una y es otra, se rechaza: nadie cambia el ancla de
/// confianza escribiendo en la carpeta compartida.
pub fn fijar_llave_publica_admin(carpeta_proyecto: &Path, publica: &str) -> Resultado<()> {
    let publica = publica.trim();
    decodificar_publica(publica)?;
    match leer_llave_publica_admin(carpeta_proyecto)? {
        Some(fijada) if fijada == publica => Ok(()),
        Some(_) => Err(Error::Validacion(
            "La llave del Administrador no coincide con la que esta máquina tenía para el proyecto"
                .into(),
        )),
        None => {
            std::fs::create_dir_all(carpeta_proyecto)?;
            escribir_nuevo(&carpeta_proyecto.join(ARCHIVO_LLAVE_PUBLICA), publica)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAVE: &str = "una clave larga";

    fn carpeta() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn genera_guarda_y_firma() {
        let dir = carpeta();
        assert!(!existe_llave_admin(dir.path()));
        let publica = generar_llaves_admin(dir.path(), None).unwrap();
        assert!(existe_llave_admin(dir.path()));
        assert_eq!(
            leer_llave_publica_admin(dir.path()).unwrap().as_deref(),
            Some(publica.as_str())
        );
        assert!(!llave_admin_cifrada(dir.path()).unwrap());

        let llave = cargar_llave_admin(dir.path(), None).unwrap();
        assert_eq!(llave.publica(), publica);
        let firma = llave.firmar("ana@virustf.dev|Líder|2026-11-10T10:00:00Z");
        assert!(verificar(
            &publica,
            "ana@virustf.dev|Líder|2026-11-10T10:00:00Z",
            &firma
        ));
        assert!(!verificar(
            &publica,
            "ana@virustf.dev|Administrador|2026-11-10T10:00:00Z",
            &firma
        ));
    }

    #[test]
    fn no_pisa_una_llave_que_ya_existe() {
        let dir = carpeta();
        let publica = generar_llaves_admin(dir.path(), None).unwrap();
        assert!(generar_llaves_admin(dir.path(), None).is_err());
        assert_eq!(
            cargar_llave_admin(dir.path(), None).unwrap().publica(),
            publica
        );
    }

    #[test]
    fn otra_llave_no_firma_por_el_administrador() {
        let (a, b) = (carpeta(), carpeta());
        let publica_a = generar_llaves_admin(a.path(), None).unwrap();
        generar_llaves_admin(b.path(), None).unwrap();
        let firma_b = cargar_llave_admin(b.path(), None)
            .unwrap()
            .firmar("x|Líder|y");
        assert!(!verificar(&publica_a, "x|Líder|y", &firma_b));
        assert!(!verificar(&publica_a, "x|Líder|y", "no es base64 !!"));
        assert!(!verificar("mala", "x|Líder|y", &firma_b));
    }

    #[test]
    fn cifrada_exige_la_passphrase_correcta() {
        let dir = carpeta();
        let publica = generar_llaves_admin(dir.path(), Some(CLAVE)).unwrap();
        assert!(llave_admin_cifrada(dir.path()).unwrap());
        assert!(cargar_llave_admin(dir.path(), None).is_err());
        assert!(cargar_llave_admin(dir.path(), Some("otra clave mala")).is_err());
        assert_eq!(
            cargar_llave_admin(dir.path(), Some(CLAVE))
                .unwrap()
                .publica(),
            publica
        );
        // La semilla no queda legible en el archivo.
        let texto = std::fs::read_to_string(dir.path().join(ARCHIVO_LLAVE_ADMIN)).unwrap();
        assert!(texto.contains("\"cifrada\": true"));
    }

    #[test]
    fn passphrase_corta_se_rechaza() {
        let dir = carpeta();
        assert!(generar_llaves_admin(dir.path(), Some("corta")).is_err());
        assert!(!existe_llave_admin(dir.path()), "no deja nada a medias");
        generar_llaves_admin(dir.path(), None).unwrap();
        assert!(exportar_llave_firma(dir.path(), "corta").is_err());
    }

    #[test]
    fn exportar_cifra_siempre_e_importar_la_recupera() {
        let origen = carpeta();
        let publica = generar_llaves_admin(origen.path(), None).unwrap();
        let exportado = exportar_llave_firma(origen.path(), CLAVE).unwrap();
        let semilla_en_claro: ArchivoLlave = serde_json::from_str(
            &std::fs::read_to_string(origen.path().join(ARCHIVO_LLAVE_ADMIN)).unwrap(),
        )
        .unwrap();
        assert!(!exportado.contains(&semilla_en_claro.llave));

        let destino = carpeta();
        assert!(importar_llave_firma(destino.path(), &exportado, "clave equivocada").is_err());
        assert!(!existe_llave_admin(destino.path()));
        assert_eq!(
            importar_llave_firma(destino.path(), &exportado, CLAVE).unwrap(),
            publica
        );
        let firma = cargar_llave_admin(destino.path(), Some(CLAVE))
            .unwrap()
            .firmar("m");
        assert!(verificar(&publica, "m", &firma));
        // Ya hay llave: no se importa encima.
        assert!(importar_llave_firma(destino.path(), &exportado, CLAVE).is_err());
    }

    #[test]
    fn exportar_una_llave_cifrada_pide_su_misma_passphrase() {
        let dir = carpeta();
        generar_llaves_admin(dir.path(), Some(CLAVE)).unwrap();
        assert!(exportar_llave_firma(dir.path(), "otra clave distinta").is_err());
        assert!(exportar_llave_firma(dir.path(), CLAVE).is_ok());
    }

    #[test]
    fn la_llave_publica_fijada_no_se_puede_cambiar() {
        let (dir, otro) = (carpeta(), carpeta());
        let publica = generar_llaves_admin(otro.path(), None).unwrap();
        let intrusa = generar_llaves_admin(carpeta().path(), None).unwrap();

        assert_eq!(leer_llave_publica_admin(dir.path()).unwrap(), None);
        fijar_llave_publica_admin(dir.path(), &publica).unwrap();
        fijar_llave_publica_admin(dir.path(), &publica).unwrap();
        assert!(fijar_llave_publica_admin(dir.path(), &intrusa).is_err());
        assert!(fijar_llave_publica_admin(carpeta().path(), "no es una llave").is_err());
        assert_eq!(
            leer_llave_publica_admin(dir.path()).unwrap().as_deref(),
            Some(publica.as_str())
        );
        // Con otro Administrador ya fijado, esta máquina no genera ni importa llaves.
        assert!(generar_llaves_admin(dir.path(), None).is_err());
        let exportado = {
            let ajena = carpeta();
            generar_llaves_admin(ajena.path(), None).unwrap();
            exportar_llave_firma(ajena.path(), CLAVE).unwrap()
        };
        assert!(importar_llave_firma(dir.path(), &exportado, CLAVE).is_err());
        assert!(!existe_llave_admin(dir.path()));
    }

    #[test]
    fn un_archivo_alterado_se_rechaza() {
        let dir = carpeta();
        generar_llaves_admin(dir.path(), None).unwrap();
        let ruta = dir.path().join(ARCHIVO_LLAVE_ADMIN);
        let mut archivo: ArchivoLlave =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        archivo.llave = B64.encode([7u8; 32]);
        std::fs::write(&ruta, a_json(&archivo).unwrap()).unwrap();
        assert!(cargar_llave_admin(dir.path(), None).is_err());
        std::fs::write(&ruta, "esto no es json").unwrap();
        assert!(cargar_llave_admin(dir.path(), None).is_err());
    }

    #[test]
    fn el_debug_no_muestra_la_llave_privada() {
        let dir = carpeta();
        generar_llaves_admin(dir.path(), None).unwrap();
        let llave = cargar_llave_admin(dir.path(), None).unwrap();
        let semilla = B64.encode(llave.firma.to_bytes());
        assert!(!format!("{llave:?}").contains(&semilla));
    }
}
