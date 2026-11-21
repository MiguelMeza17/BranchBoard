//! Ajustes del proyecto entero (issue #127): el nombre dentro de BranchBoard, la
//! tabla clave/valor `configuracion_proyecto` y la imagen del proyecto.
//!
//! - Todo es **exclusivo del Administrador** con firma válida: en la máquina que
//!   hace el cambio lo exige `exigir_permiso` y, en las demás, `aplicar_evento`
//!   vuelve a comprobar el rol verificado de quien emitió el evento.
//! - Los tres comandos publican el mismo evento, `actualizar_configuracion_proyecto`
//!   (`{ clave, valor }`), para que el cambio llegue a todas las máquinas por `ops/`.
//! - `renombrar_proyecto` solo cambia la clave `nombre_proyecto`: el repositorio de
//!   Unity VC conserva su nombre.
//! - La imagen se valida por su **contenido** (PNG, JPG o WebP, máximo 5 MB), se
//!   recorta y redimensiona a 256×256 y se guarda como PNG con un nombre que genera
//!   el backend (`icono-<uuid>.png`) en `<carpeta compartida>/proyecto/`. La clave
//!   `icono_proyecto` guarda solo ese nombre, nunca una ruta: al leerla se valida de
//!   forma canónica que el archivo esté dentro de `proyecto/` (sin path traversal).

use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader, Limits};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::configuracion::{self, Ajuste, CLAVE_ICONO_PROYECTO, CLAVE_NOMBRE_PROYECTO};
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::{normalizar_email, Rol};
use crate::tareas::rol_de;

pub mod eliminacion;

/// Tipos de evento de los ajustes del proyecto que viajan por `ops/`.
pub const TIPOS: &[&str] = &["actualizar_configuracion_proyecto"];

/// Largo máximo del nombre del proyecto (en caracteres).
pub const LARGO_MAXIMO_NOMBRE: usize = 80;
/// Largo máximo de la clave y del valor de un ajuste genérico (en caracteres).
pub const LARGO_MAXIMO_CLAVE: usize = 64;
pub const LARGO_MAXIMO_VALOR: usize = 1000;
/// Lado de la imagen del proyecto ya procesada (cuadrada).
pub const LADO_IMAGEN: u32 = 256;
/// Peso máximo del archivo de imagen que se elige.
pub const TAMANO_MAXIMO_IMAGEN: u64 = 5 * 1024 * 1024;
/// Dimensión máxima (ancho o alto) que se acepta decodificar: evita imágenes que
/// pesan poco en disco pero ocupan gigas al abrirlas.
const DIMENSION_MAXIMA: u32 = 8192;
const PREFIJO_ICONO: &str = "icono-";
const EXTENSION_ICONO: &str = ".png";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PayloadAjuste {
    clave: String,
    valor: String,
}

/// Lo que el frontend necesita del proyecto entero.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfiguracionProyecto {
    /// Nombre dentro de BranchBoard; `None` si nunca se configuró.
    pub nombre: Option<String>,
    /// La imagen como `data:image/png;base64,…`; `None` si no hay imagen o si su
    /// archivo todavía no llegó a esta máquina por Drive/OneDrive.
    pub imagen: Option<String>,
}

// ---------- Validaciones (las mismas al emitir y al aplicar) ----------

fn validar_clave(clave: &str) -> Resultado<String> {
    let limpia = clave.trim();
    let valida = !limpia.is_empty()
        && limpia.chars().count() <= LARGO_MAXIMO_CLAVE
        && limpia.starts_with(|c: char| c.is_ascii_lowercase())
        && limpia
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valida {
        return Err(Error::Validacion(format!(
            "La clave \"{limpia}\" no es válida: usa minúsculas, números y guion bajo (máximo {LARGO_MAXIMO_CLAVE})"
        )));
    }
    Ok(limpia.to_string())
}

fn validar_nombre(nombre: &str) -> Resultado<String> {
    let limpio = nombre.trim();
    if limpio.is_empty() {
        return Err(Error::Validacion(
            "El nombre del proyecto es obligatorio".into(),
        ));
    }
    if limpio.chars().count() > LARGO_MAXIMO_NOMBRE {
        return Err(Error::Validacion(format!(
            "El nombre del proyecto admite como máximo {LARGO_MAXIMO_NOMBRE} caracteres"
        )));
    }
    if limpio.chars().any(char::is_control) {
        return Err(Error::Validacion(
            "El nombre del proyecto no puede tener saltos de línea ni caracteres de control".into(),
        ));
    }
    Ok(limpio.to_string())
}

/// `icono-<letras, números o guiones>.png`: un nombre de archivo suelto, sin rutas.
fn es_nombre_icono(nombre: &str) -> bool {
    nombre
        .strip_prefix(PREFIJO_ICONO)
        .and_then(|resto| resto.strip_suffix(EXTENSION_ICONO))
        .is_some_and(|medio| {
            !medio.is_empty()
                && medio.len() <= 64
                && medio.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

fn validar_valor(clave: &str, valor: &str) -> Resultado<String> {
    match clave {
        CLAVE_NOMBRE_PROYECTO => validar_nombre(valor),
        CLAVE_ICONO_PROYECTO => {
            let limpio = valor.trim();
            if es_nombre_icono(limpio) {
                Ok(limpio.to_string())
            } else {
                Err(Error::Validacion(
                    "La imagen del proyecto no tiene un nombre de archivo válido".into(),
                ))
            }
        }
        _ => {
            if valor.chars().count() > LARGO_MAXIMO_VALOR {
                return Err(Error::Validacion(format!(
                    "El valor admite como máximo {LARGO_MAXIMO_VALOR} caracteres"
                )));
            }
            Ok(valor.to_string())
        }
    }
}

fn exigir_admin(rol: Option<Rol>) -> Resultado<()> {
    if rol == Some(Rol::Administrador) {
        Ok(())
    } else {
        Err(Error::SinPermiso)
    }
}

/// Guarda el ajuste con las reglas de siempre (rol, clave y valor). Es lo único
/// que escribe en `configuracion_proyecto` desde un evento.
fn aplicar_ajuste(
    c: &Connection,
    rol: Option<Rol>,
    autor: &str,
    fecha: &str,
    payload: &PayloadAjuste,
) -> Resultado<Ajuste> {
    exigir_admin(rol)?;
    let clave = validar_clave(&payload.clave)?;
    let valor = validar_valor(&clave, &payload.valor)?;
    configuracion::guardar(c, &clave, &valor, autor, fecha)?;
    Ok(Ajuste {
        clave,
        valor,
        actualizado_por: normalizar_email(autor),
        fecha_actualizacion: fecha.to_string(),
    })
}

/// Publica `actualizar_configuracion_proyecto` y lo aplica en la misma transacción.
fn publicar_ajuste(
    c: &Connection,
    ops: &Path,
    actor: &str,
    rol: Rol,
    clave: &str,
    valor: &str,
    ahora: &str,
) -> Resultado<Ajuste> {
    exigir_admin(Some(rol))?;
    let actor = normalizar_email(actor);
    let payload = PayloadAjuste {
        clave: validar_clave(clave)?,
        valor: valor.to_string(),
    };
    let payload = PayloadAjuste {
        valor: validar_valor(&payload.clave, &payload.valor)?,
        ..payload
    };
    let mut ev = Evento::nuevo(
        "actualizar_configuracion_proyecto",
        None,
        &actor,
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar actualizar_configuracion_proyecto: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    let mut guardado = None;
    eventos::publicar(c, ops, &ev, |tx| {
        guardado = Some(aplicar_ajuste(tx, Some(rol), &actor, ahora, &payload)?);
        Ok(())
    })?;
    Ok(guardado.expect("ajuste guardado en la transacción"))
}

// ---------- En la máquina de quien hace el cambio ----------

/// `actualizar_configuracion_proyecto(clave, valor)`: ajuste genérico del proyecto.
/// La imagen no se cambia por aquí (va por [`subir_imagen`], que crea el archivo).
pub fn actualizar(
    c: &Connection,
    ops: &Path,
    actor: &str,
    rol: Rol,
    clave: &str,
    valor: &str,
    ahora: &str,
) -> Resultado<Ajuste> {
    exigir_admin(Some(rol))?;
    if clave.trim() == CLAVE_ICONO_PROYECTO {
        return Err(Error::Validacion(
            "La imagen del proyecto se cambia con subir_imagen_proyecto".into(),
        ));
    }
    publicar_ajuste(c, ops, actor, rol, clave, valor, ahora)
}

/// `renombrar_proyecto(nombre)`: cambia el nombre en BranchBoard, no el repositorio de Unity VC.
pub fn renombrar(
    c: &Connection,
    ops: &Path,
    actor: &str,
    rol: Rol,
    nombre: &str,
    ahora: &str,
) -> Resultado<Ajuste> {
    exigir_admin(Some(rol))?;
    let nuevo = validar_nombre(nombre)?;
    if configuracion::leer(c, CLAVE_NOMBRE_PROYECTO)?.as_deref() == Some(nuevo.as_str()) {
        return Err(Error::Validacion("El proyecto ya tiene ese nombre".into()));
    }
    publicar_ajuste(c, ops, actor, rol, CLAVE_NOMBRE_PROYECTO, &nuevo, ahora)
}

/// Lee, valida y procesa la imagen elegida: PNG del lado [`LADO_IMAGEN`].
fn procesar_imagen(origen_raw: &str) -> Resultado<Vec<u8>> {
    let ruta_txt = origen_raw.trim();
    if ruta_txt.is_empty() {
        return Err(Error::Validacion("Elige una imagen".into()));
    }
    let origen = fs::canonicalize(Path::new(ruta_txt))
        .map_err(|e| Error::NoEncontrado(format!("la imagen {ruta_txt} ({e})")))?;
    let meta = fs::metadata(&origen)?;
    if !meta.is_file() {
        return Err(Error::Validacion(format!(
            "{ruta_txt} no es un archivo regular"
        )));
    }
    if meta.len() > TAMANO_MAXIMO_IMAGEN {
        return Err(Error::Validacion(format!(
            "La imagen pesa más de {} MB",
            TAMANO_MAXIMO_IMAGEN / 1024 / 1024
        )));
    }
    let bytes = fs::read(&origen)?;
    // El formato sale del contenido, no de la extensión.
    let formato = match image::guess_format(&bytes) {
        Ok(f @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)) => f,
        _ => {
            return Err(Error::Validacion(
                "La imagen tiene que ser PNG, JPG o WebP".into(),
            ))
        }
    };
    let mut lector = ImageReader::with_format(Cursor::new(&bytes), formato);
    let mut limites = Limits::default();
    limites.max_image_width = Some(DIMENSION_MAXIMA);
    limites.max_image_height = Some(DIMENSION_MAXIMA);
    lector.limits(limites);
    let imagen = lector
        .decode()
        .map_err(|e| Error::Validacion(format!("No se pudo leer la imagen ({e})")))?;
    // Recorta al centro para llenar el cuadrado (el ícono se muestra en un círculo).
    let cuadrada = imagen.resize_to_fill(LADO_IMAGEN, LADO_IMAGEN, FilterType::Lanczos3);
    let mut png = Vec::new();
    cuadrada
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|e| Error::Interno(format!("no se pudo guardar la imagen como PNG: {e}")))?;
    Ok(png)
}

fn base_canonica(carpeta: &Path) -> Resultado<PathBuf> {
    fs::create_dir_all(carpeta)?;
    Ok(fs::canonicalize(carpeta)?)
}

/// Ruta del ícono `nombre` dentro de `base` (ya canónica), solo si el nombre es válido.
fn ruta_icono(base: &Path, nombre: &str) -> Option<PathBuf> {
    es_nombre_icono(nombre).then(|| base.join(nombre))
}

fn data_url_png(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", B64.encode(bytes))
}

/// `subir_imagen_proyecto(ruta)`: procesa la imagen, la deja en
/// `<compartida>/proyecto/icono-<uuid>.png`, publica el cambio de `icono_proyecto`
/// y borra la imagen anterior. Devuelve la imagen nueva como `data:` URL.
pub fn subir_imagen(
    c: &Connection,
    ops: &Path,
    carpeta_imagen: &Path,
    actor: &str,
    rol: Rol,
    ruta_origen: &str,
    ahora: &str,
) -> Resultado<String> {
    exigir_admin(Some(rol))?;
    let png = procesar_imagen(ruta_origen)?;
    let base = base_canonica(carpeta_imagen)?;
    let nombre = format!("{PREFIJO_ICONO}{}{EXTENSION_ICONO}", eventos::uuid());
    let destino = ruta_icono(&base, &nombre)
        .ok_or_else(|| Error::Interno(format!("nombre de imagen generado no válido: {nombre}")))?;

    let escrito = (|| -> Resultado<()> {
        let mut archivo = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destino)?;
        archivo.write_all(&png)?;
        archivo.sync_all()?;
        if !fs::canonicalize(&destino)?.starts_with(&base) {
            return Err(Error::Validacion(
                "La imagen quedó fuera de la carpeta del proyecto".into(),
            ));
        }
        Ok(())
    })();
    if let Err(e) = escrito {
        let _ = fs::remove_file(&destino);
        return Err(e);
    }

    let anterior = configuracion::leer(c, CLAVE_ICONO_PROYECTO)?;
    if let Err(e) = publicar_ajuste(c, ops, actor, rol, CLAVE_ICONO_PROYECTO, &nombre, ahora) {
        let _ = fs::remove_file(&destino);
        return Err(e);
    }
    // La anterior ya no la usa nadie: se borra (Drive/OneDrive lo replica en las demás máquinas).
    if let Some(vieja) = anterior.as_deref().and_then(|n| ruta_icono(&base, n)) {
        if vieja != destino {
            if let Ok(canon) = fs::canonicalize(&vieja) {
                if canon.starts_with(&base) && canon.is_file() {
                    let _ = fs::remove_file(canon);
                }
            }
        }
    }
    Ok(data_url_png(&png))
}

/// El nombre y la imagen del proyecto. `carpeta_imagen` es `None` si esta máquina
/// todavía no configuró la carpeta compartida (entonces no hay imagen que leer).
pub fn leer(c: &Connection, carpeta_imagen: Option<&Path>) -> Resultado<ConfiguracionProyecto> {
    let nombre = configuracion::leer(c, CLAVE_NOMBRE_PROYECTO)?;
    let imagen = match (
        configuracion::leer(c, CLAVE_ICONO_PROYECTO)?,
        carpeta_imagen,
    ) {
        (Some(icono), Some(carpeta)) => leer_imagen(carpeta, &icono),
        _ => None,
    };
    Ok(ConfiguracionProyecto { nombre, imagen })
}

/// Lee el ícono solo si está, de forma canónica, dentro de `proyecto/`.
fn leer_imagen(carpeta: &Path, icono: &str) -> Option<String> {
    let base = fs::canonicalize(carpeta).ok()?;
    let canon = fs::canonicalize(ruta_icono(&base, icono)?).ok()?;
    let meta = fs::metadata(&canon).ok()?;
    if !canon.starts_with(&base) || !meta.is_file() || meta.len() > TAMANO_MAXIMO_IMAGEN {
        return None;
    }
    fs::read(&canon).ok().map(|b| data_url_png(&b))
}

// ---------- Con el proyecto en el servidor (#242) ----------
//
// La imagen va a Supabase Storage (`<proyecto>/proyecto/icono-<uuid>.png`) y cada equipo guarda una copia
// en una carpeta local para leerla sin red. El comando hace la red fuera del bloqueo de la base:
// `preparar_imagen` → subir → `publicar_imagen` → borrar la anterior → `guardar_en_cache`.

/// Procesa la imagen elegida (las mismas reglas que [`subir_imagen`]) y le da un nombre nuevo.
pub fn preparar_imagen(ruta_origen: &str) -> Resultado<(String, Vec<u8>)> {
    let png = procesar_imagen(ruta_origen)?;
    let nombre = format!("{PREFIJO_ICONO}{}{EXTENSION_ICONO}", eventos::uuid());
    Ok((nombre, png))
}

/// Publica `icono_proyecto = nombre` (solo Administrador). Devuelve el ícono anterior, para borrarlo.
pub fn publicar_imagen(
    c: &Connection,
    ops: &Path,
    actor: &str,
    rol: Rol,
    nombre: &str,
    ahora: &str,
) -> Resultado<Option<String>> {
    exigir_admin(Some(rol))?;
    let anterior = configuracion::leer(c, CLAVE_ICONO_PROYECTO)?;
    publicar_ajuste(c, ops, actor, rol, CLAVE_ICONO_PROYECTO, nombre, ahora)?;
    Ok(anterior.filter(|a| a != nombre && es_nombre_icono(a)))
}

/// El ícono configurado si todavía no está en la carpeta local `cache` (hay que bajarlo).
pub fn icono_sin_copia(c: &Connection, cache: &Path) -> Resultado<Option<String>> {
    let Some(icono) = configuracion::leer(c, CLAVE_ICONO_PROYECTO)? else {
        return Ok(None);
    };
    let base = base_canonica(cache)?;
    Ok(ruta_icono(&base, &icono)
        .filter(|r| !r.is_file())
        .map(|_| icono))
}

/// Guarda la copia local del ícono (escribe en un temporal y renombra: nunca queda uno a medias).
pub fn guardar_en_cache(cache: &Path, nombre: &str, png: &[u8]) -> Resultado<()> {
    let base = base_canonica(cache)?;
    let destino = ruta_icono(&base, nombre)
        .ok_or_else(|| Error::Validacion("El nombre de la imagen no es válido".into()))?;
    let temporal = base.join(format!("{nombre}.bajando"));
    fs::write(&temporal, png)?;
    if let Err(e) = fs::rename(&temporal, &destino) {
        let _ = fs::remove_file(&temporal);
        return Err(e.into());
    }
    Ok(())
}

/// La imagen como `data:image/png;base64,…` (lo que devuelve `subir_imagen_proyecto`).
pub fn imagen_como_data_url(png: &[u8]) -> String {
    data_url_png(png)
}

// ---------- En las demás máquinas (al leer `ops/`) ----------

/// Reproduce un evento de ajustes del proyecto con las mismas reglas que en la
/// máquina que lo emitió: solo un Administrador verificado, clave y valor válidos.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    match evento.tipo.as_str() {
        "actualizar_configuracion_proyecto" => {
            let rol = rol_de(c, &evento.usuario)?;
            let payload: PayloadAjuste =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("actualizar_configuracion_proyecto inválido: {e}"))
                })?;
            aplicar_ajuste(c, rol, &evento.usuario, &evento.timestamp, &payload).map(|_| ())
        }
        otro => Err(Error::Validacion(format!(
            "tipo de evento del proyecto no soportado: {otro}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use image::{Rgb, RgbImage};
    use tempfile::TempDir;

    use super::*;
    use crate::db::{usuarios, Db};
    use crate::eventos::lectura::{self, sincronizar};
    use crate::eventos::{carpeta_imagen_proyecto, carpeta_ops, configurar_carpeta_compartida};

    const ADMIN: &str = "admin@estudio.com";
    const LIDER: &str = "lider@estudio.com";
    const DEV: &str = "dev@estudio.com";
    const AHORA: &str = "2026-11-16T10:00:00.000Z";
    const HOY: &str = "2026-11-16";

    struct Escenario {
        tmp: TempDir,
        db_a: Db,
        ops: PathBuf,
        imagen_a: PathBuf,
        db_b: Db,
        local_b: PathBuf,
    }

    fn setup() -> Escenario {
        let tmp = TempDir::new().unwrap();
        let compartida = tmp.path().join("compartida");
        fs::create_dir_all(&compartida).unwrap();
        let local_a = tmp.path().join("datos_a").join("proyectos").join("p127");
        let local_b = tmp.path().join("datos_b").join("proyectos").join("p127");
        configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
        configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
        let ops = carpeta_ops(&local_a).unwrap();
        let imagen_a = carpeta_imagen_proyecto(&local_a).unwrap();

        let db_a = Db::abrir(&local_a).unwrap();
        let db_b = Db::abrir(&local_b).unwrap();
        for (correo, rol) in [
            (ADMIN, Rol::Administrador),
            (LIDER, Rol::Lider),
            (DEV, Rol::Desarrollador),
        ] {
            let fila = usuarios::Usuario {
                email: correo.into(),
                nombre: correo.split('@').next().unwrap().into(),
                apellido: "Test".into(),
                rol,
                fecha_alta: HOY.into(),
            };
            usuarios::insertar(db_a.conexion(), &fila).unwrap();
            usuarios::insertar(db_b.conexion(), &fila).unwrap();
        }
        Escenario {
            tmp,
            db_a,
            ops,
            imagen_a,
            db_b,
            local_b,
        }
    }

    /// Escribe una imagen `ancho`×`alto` en `ruta` con el formato de su extensión.
    fn imagen_de_prueba(ruta: &Path, ancho: u32, alto: u32) {
        let img = RgbImage::from_fn(ancho, alto, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        });
        img.save(ruta).unwrap();
    }

    fn png_de_data_url(url: &str) -> image::DynamicImage {
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        image::load_from_memory_with_format(&B64.decode(b64).unwrap(), ImageFormat::Png).unwrap()
    }

    #[test]
    fn validaciones_de_clave_nombre_e_icono() {
        assert!(validar_clave("nombre_proyecto").is_ok());
        assert!(validar_clave("tema_2").is_ok());
        for mala in ["", "  ", "Nombre", "2tema", "con espacio", "../x", "a-b"] {
            assert!(
                validar_clave(mala).is_err(),
                "{mala:?} debería ser inválida"
            );
        }
        assert_eq!(validar_nombre("  Virus TF  ").unwrap(), "Virus TF");
        assert!(validar_nombre("   ").is_err());
        assert!(validar_nombre("con\nsalto").is_err());
        assert!(validar_nombre(&"x".repeat(LARGO_MAXIMO_NOMBRE + 1)).is_err());
        assert!(es_nombre_icono("icono-0f3a-99.png"));
        for malo in [
            "icono-.png",
            "icono-../x.png",
            "icono-a/b.png",
            "icono-a\\b.png",
            "otro-a.png",
            "icono-a.jpg",
            "../icono-a.png",
        ] {
            assert!(!es_nombre_icono(malo), "{malo:?} debería ser inválido");
        }
    }

    #[test]
    fn renombrar_y_ajustes_solo_administrador_y_sincroniza() {
        let e = setup();
        let c = e.db_a.conexion();
        for (quien, rol) in [(LIDER, Rol::Lider), (DEV, Rol::Desarrollador)] {
            assert_eq!(
                renombrar(c, &e.ops, quien, rol, "Otro", AHORA)
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );
            assert_eq!(
                actualizar(c, &e.ops, quien, rol, "tema", "oscuro", AHORA)
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );
        }
        let ajuste = renombrar(
            c,
            &e.ops,
            ADMIN,
            Rol::Administrador,
            "  Virus TF  ",
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        assert_eq!(ajuste.valor, "Virus TF");
        assert_eq!(ajuste.actualizado_por, ADMIN);
        assert_eq!(
            renombrar(c, &e.ops, ADMIN, Rol::Administrador, "Virus TF", AHORA)
                .unwrap_err()
                .codigo(),
            "validacion"
        );
        assert!(renombrar(c, &e.ops, ADMIN, Rol::Administrador, "", AHORA).is_err());
        actualizar(
            c,
            &e.ops,
            ADMIN,
            Rol::Administrador,
            "tema",
            "oscuro",
            "2026-11-16T10:02:00.000Z",
        )
        .unwrap();
        // La imagen no se cambia por el comando genérico, ni con claves inválidas.
        assert!(actualizar(
            c,
            &e.ops,
            ADMIN,
            Rol::Administrador,
            CLAVE_ICONO_PROYECTO,
            "icono-a.png",
            AHORA
        )
        .is_err());
        assert!(actualizar(c, &e.ops, ADMIN, Rol::Administrador, "../x", "v", AHORA).is_err());

        // Eventos forjados en `ops/`: un Líder que renombra y un icono con traversal.
        let forjado = |usuario: &str, clave: &str, valor: &str, ts: &str| {
            let mut ev = Evento::nuevo(
                "actualizar_configuracion_proyecto",
                None,
                usuario,
                serde_json::to_value(PayloadAjuste {
                    clave: clave.into(),
                    valor: valor.into(),
                })
                .unwrap(),
            );
            ev.timestamp = ts.into();
            eventos::escribir_evento(&e.ops, &ev).unwrap();
        };
        forjado(
            LIDER,
            CLAVE_NOMBRE_PROYECTO,
            "Hackeado",
            "2026-11-16T10:03:00.000Z",
        );
        forjado(
            ADMIN,
            CLAVE_ICONO_PROYECTO,
            "../../fuera.png",
            "2026-11-16T10:04:00.000Z",
        );

        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:10:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync = sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, ev)
        })
        .unwrap();
        assert_eq!(sync.aplicados, 2);
        assert_eq!(sync.descartados, 2);
        let en_b = leer(e.db_b.conexion(), None).unwrap();
        assert_eq!(en_b.nombre.as_deref(), Some("Virus TF"));
        assert_eq!(
            configuracion::leer(e.db_b.conexion(), "tema")
                .unwrap()
                .as_deref(),
            Some("oscuro")
        );
        assert_eq!(
            configuracion::leer(e.db_b.conexion(), CLAVE_ICONO_PROYECTO).unwrap(),
            None
        );
    }

    #[test]
    fn subir_imagen_valida_redimensiona_reemplaza_y_sincroniza() {
        let e = setup();
        let c = e.db_a.conexion();
        let origen = e.tmp.path().join("origen");
        fs::create_dir_all(&origen).unwrap();
        let png = origen.join("logo.png");
        let jpg = origen.join("foto.jpg");
        let webp = origen.join("arte.webp");
        imagen_de_prueba(&png, 600, 300);
        imagen_de_prueba(&jpg, 300, 900);
        imagen_de_prueba(&webp, 120, 120);
        let disfrazado = origen.join("texto.png");
        fs::write(&disfrazado, b"esto no es una imagen").unwrap();
        let gif = origen.join("animado.gif");
        fs::write(&gif, b"GIF89a\x01\x00\x01\x00\x00\x00\x00;").unwrap();
        let pesado = origen.join("pesado.png");
        fs::write(&pesado, vec![0u8; (TAMANO_MAXIMO_IMAGEN + 1) as usize]).unwrap();
        let ruta = |p: &Path| p.to_string_lossy().to_string();

        // Permisos y archivos rechazados (sin dejar nada en `proyecto/`).
        assert_eq!(
            subir_imagen(
                c,
                &e.ops,
                &e.imagen_a,
                LIDER,
                Rol::Lider,
                &ruta(&png),
                AHORA
            )
            .unwrap_err()
            .codigo(),
            "sin_permiso"
        );
        for mala in [&disfrazado, &gif, &pesado, &origen] {
            assert_eq!(
                subir_imagen(
                    c,
                    &e.ops,
                    &e.imagen_a,
                    ADMIN,
                    Rol::Administrador,
                    &ruta(mala),
                    AHORA
                )
                .unwrap_err()
                .codigo(),
                "validacion",
                "{mala:?}"
            );
        }
        assert!(subir_imagen(
            c,
            &e.ops,
            &e.imagen_a,
            ADMIN,
            Rol::Administrador,
            &ruta(&origen.join("no.png")),
            AHORA
        )
        .is_err());
        assert!(!e.imagen_a.exists() || fs::read_dir(&e.imagen_a).unwrap().next().is_none());

        // PNG y WebP: quedan en 256×256 y solo existe la última.
        let url = subir_imagen(
            c,
            &e.ops,
            &e.imagen_a,
            ADMIN,
            Rol::Administrador,
            &ruta(&png),
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        let primera = png_de_data_url(&url);
        assert_eq!(
            (primera.width(), primera.height()),
            (LADO_IMAGEN, LADO_IMAGEN)
        );
        let icono_1 = configuracion::leer(c, CLAVE_ICONO_PROYECTO)
            .unwrap()
            .unwrap();
        assert!(e.imagen_a.join(&icono_1).is_file());

        subir_imagen(
            c,
            &e.ops,
            &e.imagen_a,
            ADMIN,
            Rol::Administrador,
            &ruta(&webp),
            "2026-11-16T10:02:00.000Z",
        )
        .unwrap();
        let url = subir_imagen(
            c,
            &e.ops,
            &e.imagen_a,
            ADMIN,
            Rol::Administrador,
            &ruta(&jpg),
            "2026-11-16T10:03:00.000Z",
        )
        .unwrap();
        let icono_3 = configuracion::leer(c, CLAVE_ICONO_PROYECTO)
            .unwrap()
            .unwrap();
        assert_ne!(icono_1, icono_3);
        assert!(!e.imagen_a.join(&icono_1).exists());
        assert_eq!(fs::read_dir(&e.imagen_a).unwrap().count(), 1);
        assert_eq!(
            leer(c, Some(&e.imagen_a)).unwrap().imagen.as_deref(),
            Some(url.as_str())
        );

        // Una fila alterada con traversal no se lee.
        configuracion::guardar(c, CLAVE_ICONO_PROYECTO, "../llaves/admin.key", ADMIN, AHORA)
            .unwrap();
        assert_eq!(leer(c, Some(&e.imagen_a)).unwrap().imagen, None);

        // B sincroniza: se queda con la última imagen (comparte la carpeta).
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:10:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync = sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, ev)
        })
        .unwrap();
        assert_eq!(sync.aplicados, 3);
        let imagen_b = carpeta_imagen_proyecto(&e.local_b).unwrap();
        let en_b = leer(e.db_b.conexion(), Some(&imagen_b)).unwrap();
        assert_eq!(en_b.imagen.as_deref(), Some(url.as_str()));
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #127): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// ejerce los comandos `obtener_configuracion_proyecto_en`, `renombrar_proyecto_en`,
    /// `actualizar_configuracion_proyecto_en` y `subir_imagen_proyecto_en`:
    /// 1. Al entrar, el nombre es el del repositorio y no hay imagen.
    /// 2. Un Líder no puede renombrar, cambiar ajustes ni la imagen (`sin_permiso`).
    /// 3. El Administrador renombra, guarda un ajuste y sube dos imágenes: quedan en
    ///    `<OneDrive>/proyecto/icono-<uuid>.png` en 256×256 y solo sobrevive la última.
    /// 4. Un renombre forjado por el Líder y un icono con path traversal en `ops/` se
    ///    descartan; la máquina B sincroniza por OneDrive y ve el mismo nombre e imagen.
    ///
    /// Al terminar borra el workspace y la subcarpeta de OneDrive.
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_ajustes_del_proyecto() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::proyecto::{
            actualizar_configuracion_proyecto_en, obtener_configuracion_proyecto_en,
            renombrar_proyecto_en, subir_imagen_proyecto_en,
        };
        use crate::estado::AppState;

        use crate::modelo::hoy_local;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-127-{sufijo}"));
        fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let origen = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-127-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            // 1. Sin carpeta compartida todavía: el nombre es el del repositorio, sin imagen.
            let inicial = obtener_configuracion_proyecto_en(&estado_a).unwrap();
            let nombre_repo = repo.rsplit('/').next().unwrap().to_string();
            assert_eq!(inicial.nombre.as_deref(), Some(nombre_repo.as_str()));
            assert_eq!(inicial.imagen, None);

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();
            let imagen_a = carpeta_imagen_proyecto(&local_a).unwrap();

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();
            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));
            estado_a
                .con_db(|db_a| {
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            LIDER,
                            Rol::Lider,
                            &hoy_local(),
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })
                })
                .unwrap();

            let png = origen.path().join("logo.png");
            let jpg = origen.path().join("foto.jpg");
            imagen_de_prueba(&png, 1024, 768);
            imagen_de_prueba(&jpg, 400, 1200);
            let ruta = |p: &Path| p.to_string_lossy().to_string();

            // 2. El Líder no puede tocar los ajustes del proyecto.
            estado_a.fijar_usuario(Some(LIDER));
            for err in [
                renombrar_proyecto_en(&estado_a, "Del Lider").unwrap_err(),
                actualizar_configuracion_proyecto_en(&estado_a, "tema", "claro").unwrap_err(),
                subir_imagen_proyecto_en(&estado_a, &ruta(&png)).unwrap_err(),
            ] {
                assert_eq!(err.codigo(), "sin_permiso");
            }

            // 3. El Administrador renombra, guarda un ajuste y cambia dos veces la imagen.
            estado_a.fijar_usuario(Some(&admin_fila.email));
            let nuevo_nombre = format!("Prueba 127 {sufijo}");
            pausa();
            let ajuste = renombrar_proyecto_en(&estado_a, &format!("  {nuevo_nombre} ")).unwrap();
            assert_eq!(ajuste.valor, nuevo_nombre);
            assert_eq!(
                renombrar_proyecto_en(&estado_a, &nuevo_nombre)
                    .unwrap_err()
                    .codigo(),
                "validacion"
            );
            pausa();
            actualizar_configuracion_proyecto_en(&estado_a, "tema", "oscuro").unwrap();
            pausa();
            let url_1 = subir_imagen_proyecto_en(&estado_a, &ruta(&png)).unwrap();
            let img_1 = png_de_data_url(&url_1);
            assert_eq!((img_1.width(), img_1.height()), (LADO_IMAGEN, LADO_IMAGEN));
            let archivos = |carpeta: &Path| -> Vec<String> {
                fs::read_dir(carpeta)
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
                    .collect()
            };
            let primera = archivos(&imagen_a);
            assert_eq!(primera.len(), 1);
            assert!(es_nombre_icono(&primera[0]));
            pausa();
            let url_2 = subir_imagen_proyecto_en(&estado_a, &ruta(&jpg)).unwrap();
            let segunda = archivos(&imagen_a);
            assert_eq!(segunda.len(), 1);
            assert_ne!(primera, segunda);
            let en_a = obtener_configuracion_proyecto_en(&estado_a).unwrap();
            assert_eq!(en_a.nombre.as_deref(), Some(nuevo_nombre.as_str()));
            assert_eq!(en_a.imagen.as_deref(), Some(url_2.as_str()));
            println!(
                "OneDrive/proyecto tras dos subidas: {:?} ({} bytes en data URL)",
                segunda,
                url_2.len()
            );

            // 4. Eventos forjados: renombre de un Líder e icono con path traversal.
            for (usuario, clave, valor) in [
                (LIDER, CLAVE_NOMBRE_PROYECTO, "Hackeado"),
                (
                    admin_fila.email.as_str(),
                    CLAVE_ICONO_PROYECTO,
                    "../../ops/x.png",
                ),
            ] {
                let mut ev = Evento::nuevo(
                    "actualizar_configuracion_proyecto",
                    None,
                    usuario,
                    serde_json::to_value(PayloadAjuste {
                        clave: clave.into(),
                        valor: valor.into(),
                    })
                    .unwrap(),
                );
                pausa();
                ev.timestamp = crate::modelo::ahora_iso();
                eventos::escribir_evento(&ops_a, &ev).unwrap();
            }

            std::thread::sleep(std::time::Duration::from_secs(2));

            // La máquina B sincroniza por OneDrive.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, e)
            })
            .unwrap();
            println!("B sincroniza ajustes del proyecto desde OneDrive: {sync_b:?}");
            assert_eq!(sync_b.descartados, 2);
            let imagen_b = carpeta_imagen_proyecto(&local_b).unwrap();
            let en_b = leer(db_b.conexion(), Some(&imagen_b)).unwrap();
            assert_eq!(en_b.nombre.as_deref(), Some(nuevo_nombre.as_str()));
            assert_eq!(en_b.imagen.as_deref(), Some(url_2.as_str()));
            assert_eq!(
                configuracion::leer(db_b.conexion(), "tema")
                    .unwrap()
                    .as_deref(),
                Some("oscuro")
            );
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
