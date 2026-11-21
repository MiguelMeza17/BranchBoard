//! Proyectos locales: proyectos de BranchBoard que **no** están en Unity VC
//! ("Continuar sin Unity VC" en la pantalla de acceso). Ver "Proyectos sin Unity VC"
//! en BACKEND.md.
//!
//! Todo el proyecto vive en una sola carpeta, que se puede mover, copiar,
//! exportar e importar:
//!
//! ```text
//! <Proyecto>/
//!   branchboard-proyecto-local.json   ← marca: identifica la carpeta como proyecto de BranchBoard
//!   datos/                            ← base SQLite y copias de seguridad
//!   compartida/                       ← lo que en un proyecto de Unity VC va en Drive/OneDrive (ops/, documentos, imagen)
//! ```
//!
//! Desde #243 no lleva contraseña (decisión 4 de #231): antes cifraba la llave del Administrador
//! (`datos/admin.key`), que se quitó con las firmas. Un proyecto creado antes conserva sus archivos de
//! llave, pero ya no se usan.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::configuracion::{self, CLAVE_NOMBRE_PROYECTO};
use crate::db::usuarios::{self, Usuario};
use crate::db::{backups, vistos, Db, NOMBRE_DB};
use crate::error::{Error, Resultado};
use crate::estado::AppState;
use crate::eventos;
use crate::identidad::acceso::Entrada;
use crate::modelo::{normalizar_email, Rol};

/// Archivo que identifica la carpeta como un proyecto local de BranchBoard.
pub const ARCHIVO_MARCA: &str = "branchboard-proyecto-local.json";
/// Subcarpeta con la base y sus copias de seguridad.
pub const CARPETA_DATOS: &str = "datos";
/// Subcarpeta con `ops/`, documentos e imagen del proyecto.
pub const CARPETA_COMPARTIDA: &str = "compartida";
/// Extensión del archivo de un proyecto exportado (un `.zip`).
pub const EXTENSION_EXPORTADO: &str = "bbproyecto";
/// Versión del formato de la carpeta. Si cambia la estructura, se sube.
pub const FORMATO: u32 = 1;
/// Lista de proyectos locales abiertos en esta máquina, en la carpeta de datos de la app.
const ARCHIVO_RECIENTES: &str = "proyectos-locales.json";
/// Tope de lo que se descomprime al importar (protege de archivos manipulados).
const MAXIMO_IMPORTADO: u64 = 4 * 1024 * 1024 * 1024;
const NOMBRE_APP: &str = "BranchBoard";
const TIPO_LOCAL: &str = "local";

/// Contenido de [`ARCHIVO_MARCA`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarcaLocal {
    pub aplicacion: String,
    pub tipo: String,
    pub formato: u32,
    /// Identificador único del proyecto (no cambia al moverlo o importarlo).
    pub id: String,
    pub nombre: String,
    pub creado_en: String,
}

/// Un proyecto local que esta máquina conoce ("Recientes").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProyectoLocal {
    pub nombre: String,
    pub ruta: String,
}

/// Datos de "Crear un nuevo proyecto" sin Unity VC.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NuevoProyectoLocal {
    /// Carpeta donde se crea la carpeta del proyecto.
    pub carpeta: String,
    pub proyecto: String,
    pub nombre: String,
    pub correo: String,
}

fn no_es_proyecto() -> Error {
    Error::Validacion("En esa carpeta no hay un proyecto de BranchBoard.".into())
}

/// Lee y valida la marca de `carpeta`. Sin marca (o de otra cosa) → "no hay un proyecto".
pub fn leer_marca(carpeta: &Path) -> Resultado<MarcaLocal> {
    let texto = match std::fs::read_to_string(carpeta.join(ARCHIVO_MARCA)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(no_es_proyecto()),
        Err(e) => return Err(e.into()),
    };
    validar_marca(&texto)
}

fn validar_marca(texto: &str) -> Resultado<MarcaLocal> {
    let marca: MarcaLocal = serde_json::from_str(texto).map_err(|_| {
        Error::Validacion(format!(
            "El archivo {ARCHIVO_MARCA} de esa carpeta está dañado"
        ))
    })?;
    if marca.aplicacion != NOMBRE_APP || marca.tipo != TIPO_LOCAL || marca.id.trim().is_empty() {
        return Err(no_es_proyecto());
    }
    if marca.formato > FORMATO {
        return Err(Error::Validacion(
            "Este proyecto se guardó con una versión más nueva de BranchBoard. Actualiza la app para abrirlo."
                .into(),
        ));
    }
    Ok(marca)
}

/// Nombre válido como carpeta de Windows a partir del nombre del proyecto.
fn nombre_carpeta(proyecto: &str) -> Resultado<String> {
    let limpio: String = proyecto
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let limpio = limpio.trim_end_matches(['.', ' ']).trim().to_string();
    let reservado = {
        let base = limpio.split('.').next().unwrap_or("").to_ascii_uppercase();
        matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((base.starts_with("COM") || base.starts_with("LPT"))
                && base.len() == 4
                && base.as_bytes()[3].is_ascii_digit())
    };
    if limpio.is_empty() || limpio.chars().all(|c| c == '_') || reservado {
        return Err(Error::Validacion(
            "El nombre del proyecto no sirve como nombre de carpeta".into(),
        ));
    }
    Ok(limpio)
}

/// Primera ruta libre `<padre>/<nombre>`, `<padre>/<nombre> (2)`, …
fn ruta_libre(padre: &Path, nombre: &str) -> PathBuf {
    let primera = padre.join(nombre);
    if !primera.exists() {
        return primera;
    }
    (2..)
        .map(|n| padre.join(format!("{nombre} ({n})")))
        .find(|r| !r.exists())
        .expect("siempre hay un nombre libre")
}

fn carpeta_padre_valida(carpeta: &str) -> Resultado<PathBuf> {
    let padre = PathBuf::from(carpeta.trim());
    if !padre.is_absolute() || !padre.is_dir() {
        return Err(Error::Validacion(
            "Elige una carpeta que exista en tu equipo".into(),
        ));
    }
    Ok(padre)
}

fn id_nuevo() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Crea la carpeta del proyecto dentro de `nuevo.carpeta` con su marca, la base y su Administrador.
/// Devuelve la carpeta del proyecto. No lo abre.
pub fn crear(nuevo: &NuevoProyectoLocal, ahora: &str, hoy: &str) -> Resultado<PathBuf> {
    let proyecto = nuevo.proyecto.trim();
    let nombre = nuevo.nombre.trim();
    let correo = normalizar_email(&nuevo.correo);
    if proyecto.is_empty() {
        return Err(Error::Validacion("Escribe el nombre del proyecto.".into()));
    }
    if nombre.is_empty() {
        return Err(Error::Validacion("Escribe tu nombre.".into()));
    }
    let partes: Vec<&str> = correo.split('@').collect();
    if partes.len() != 2 || partes[0].is_empty() || !partes[1].contains('.') {
        return Err(Error::Validacion(
            "El correo no es válido (ej. nombre@correo.com).".into(),
        ));
    }
    let padre = carpeta_padre_valida(&nuevo.carpeta)?;
    let carpeta = padre.join(nombre_carpeta(proyecto)?);
    if carpeta.exists() {
        return Err(Error::Validacion(format!(
            "Ya existe la carpeta {}. Elige otro nombre u otra carpeta.",
            carpeta.display()
        )));
    }
    let resultado = crear_en(&carpeta, proyecto, nombre, &correo, ahora, hoy);
    if resultado.is_err() {
        // A medio crear no sirve: no deja una carpeta que parezca un proyecto.
        let _ = std::fs::remove_dir_all(&carpeta);
    }
    resultado.map(|_| carpeta)
}

fn crear_en(
    carpeta: &Path,
    proyecto: &str,
    nombre: &str,
    correo: &str,
    ahora: &str,
    hoy: &str,
) -> Resultado<()> {
    std::fs::create_dir_all(carpeta)?;
    let datos = carpeta.join(CARPETA_DATOS);
    let db = Db::abrir(&datos)?;
    let mut palabras = nombre.split_whitespace();
    let admin = Usuario {
        email: correo.to_string(),
        nombre: palabras.next().unwrap_or(nombre).to_string(),
        apellido: palabras.collect::<Vec<_>>().join(" "),
        rol: Rol::Administrador,
        fecha_alta: hoy.to_string(),
    };
    let c = db.conexion();
    let tx = c.unchecked_transaction()?;
    usuarios::insertar(&tx, &admin)?;
    configuracion::guardar(&tx, CLAVE_NOMBRE_PROYECTO, proyecto, correo, hoy)?;
    tx.commit()?;
    drop(db);
    std::fs::create_dir_all(carpeta.join(CARPETA_COMPARTIDA).join("ops"))?;
    // La marca va al final: sin ella, una creación fallida no parece un proyecto.
    let marca = MarcaLocal {
        aplicacion: NOMBRE_APP.into(),
        tipo: TIPO_LOCAL.into(),
        formato: FORMATO,
        id: id_nuevo(),
        nombre: proyecto.to_string(),
        creado_en: ahora.to_string(),
    };
    let json = serde_json::to_string_pretty(&marca).map_err(|e| Error::Interno(e.to_string()))?;
    let mut archivo = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(carpeta.join(ARCHIVO_MARCA))?;
    archivo.write_all(json.as_bytes())?;
    archivo.sync_all()?;
    Ok(())
}

/// Abre el proyecto local de `carpeta`: queda abierto en `estado` con su Administrador como usuario y
/// se anota en "Recientes".
pub fn abrir(
    estado: &AppState,
    datos_app: &Path,
    carpeta: &Path,
    configurado_ahora: bool,
) -> Resultado<Entrada> {
    let carpeta = carpeta_existente(carpeta)?;
    let marca = leer_marca(&carpeta)?;
    let datos = carpeta.join(CARPETA_DATOS);
    if !datos.join(NOMBRE_DB).is_file() {
        return Err(Error::Validacion(
            "A ese proyecto le faltan archivos (carpeta datos). Puede que la copia esté incompleta."
                .into(),
        ));
    }
    let db = Db::abrir_con_respaldo(&datos)?;
    let admin = usuarios::listar(db.conexion())?
        .into_iter()
        .find(|u| u.rol == Rol::Administrador)
        .ok_or_else(|| Error::Validacion("El proyecto no tiene un Administrador".into()))?;
    let email = admin.email.clone();
    let proyecto =
        configuracion::leer(db.conexion(), CLAVE_NOMBRE_PROYECTO)?.unwrap_or(marca.nombre.clone());
    let vistos_usuario = vistos::listar_vistos(db.conexion(), &email)?;
    eventos::fijar_carpeta_compartida_local(&datos, &carpeta.join(CARPETA_COMPARTIDA))?;

    estado.cerrar_proyecto();
    estado.fijar_usuario(Some(&email));
    estado.abrir_proyecto(db);
    estado.fijar_workspace(None, None);
    registrar_reciente(
        datos_app,
        ProyectoLocal {
            nombre: proyecto.clone(),
            ruta: carpeta.to_string_lossy().to_string(),
        },
    )?;
    let _ = crate::comandos::sprints::evaluar_cierres_en(estado, &email);
    Ok(Entrada::Dentro {
        email,
        rol: Rol::Administrador,
        rol_verificado: true,
        proyecto,
        configurado_ahora,
        vistos: vistos_usuario,
    })
}

fn carpeta_existente(carpeta: &Path) -> Resultado<PathBuf> {
    let texto = carpeta.to_string_lossy();
    let limpia = PathBuf::from(texto.trim().trim_end_matches(['\\', '/']));
    if !limpia.is_absolute() || !limpia.is_dir() {
        return Err(Error::Validacion(
            "Elige una carpeta que exista en tu equipo".into(),
        ));
    }
    Ok(limpia)
}

/// Carpeta del proyecto local abierto (la que tiene la marca), o error si el
/// proyecto abierto es de Unity VC.
pub fn carpeta_abierta(estado: &AppState) -> Resultado<PathBuf> {
    let datos = crate::comandos::datos::carpeta_abierta(estado)?;
    let carpeta = datos
        .parent()
        .filter(|p| datos.file_name().is_some_and(|n| n == CARPETA_DATOS) && p.join(ARCHIVO_MARCA).is_file())
        .ok_or_else(|| {
            Error::Validacion(
                "Solo se pueden exportar los proyectos sin Unity VC: los de Unity VC ya están en su repositorio y en la carpeta compartida."
                    .into(),
            )
        })?;
    Ok(carpeta.to_path_buf())
}

/// Escribe en `destino` el proyecto local abierto como un archivo `.bbproyecto`
/// (zip). La base se copia de forma consistente (`VACUUM INTO`); no se incluyen
/// las copias de seguridad ni los ajustes propios de esta máquina.
pub fn exportar(estado: &AppState, destino: &Path) -> Resultado<()> {
    estado.exigir_permiso("exportar_proyecto_local")?;
    let carpeta = carpeta_abierta(estado)?;
    let temporal = tempdir_en(destino.parent().unwrap_or(Path::new(".")))?;
    let copia_db = temporal.join(NOMBRE_DB);
    estado.con_db(|db| {
        db.conexion()
            .execute("VACUUM INTO ?1", [copia_db.to_string_lossy().as_ref()])?;
        Ok(())
    })?;
    let resultado = escribir_zip(&carpeta, &copia_db, destino);
    let _ = std::fs::remove_dir_all(&temporal);
    if resultado.is_err() {
        let _ = std::fs::remove_file(destino);
    }
    resultado
}

/// Carpeta temporal oculta junto a `base` (mismo disco que el destino).
fn tempdir_en(base: &Path) -> Resultado<PathBuf> {
    let ruta = base.join(format!(".branchboard-exportando-{}", id_nuevo()));
    std::fs::create_dir_all(&ruta)?;
    Ok(ruta)
}

fn escribir_zip(carpeta: &Path, copia_db: &Path, destino: &Path) -> Resultado<()> {
    let zip_err = |e: zip::result::ZipError| Error::Archivos(e.to_string());
    let mut zip = zip::ZipWriter::new(File::create(destino)?);
    let opciones = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let agregar = |zip: &mut zip::ZipWriter<File>, nombre: &str, origen: &Path| -> Resultado<()> {
        zip.start_file(nombre, opciones).map_err(zip_err)?;
        let mut f = File::open(origen)?;
        std::io::copy(&mut f, zip)?;
        Ok(())
    };
    agregar(&mut zip, ARCHIVO_MARCA, &carpeta.join(ARCHIVO_MARCA))?;
    agregar(&mut zip, &format!("{CARPETA_DATOS}/{NOMBRE_DB}"), copia_db)?;
    let excluidos_datos = |relativa: &Path| {
        let primero = relativa
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();
        primero.starts_with(NOMBRE_DB) // .db, -wal, -shm: va la copia consistente
            || primero == backups::CARPETA
            || primero == eventos::ARCHIVO_LOCAL
    };
    for (raiz, excluir) in [
        (CARPETA_DATOS, &excluidos_datos as &dyn Fn(&Path) -> bool),
        (CARPETA_COMPARTIDA, &|_: &Path| false),
    ] {
        let base = carpeta.join(raiz);
        if !base.is_dir() {
            continue;
        }
        for archivo in archivos_de(&base)? {
            let relativa = archivo.strip_prefix(&base).expect("está dentro");
            if excluir(relativa) {
                continue;
            }
            let nombre = format!(
                "{raiz}/{}",
                relativa
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            );
            agregar(&mut zip, &nombre, &archivo)?;
        }
    }
    zip.finish().map_err(zip_err)?.sync_all()?;
    Ok(())
}

/// Todos los archivos (no carpetas) debajo de `base`, sin seguir enlaces.
fn archivos_de(base: &Path) -> Resultado<Vec<PathBuf>> {
    let mut pendientes = vec![base.to_path_buf()];
    let mut archivos = Vec::new();
    while let Some(dir) = pendientes.pop() {
        for entrada in std::fs::read_dir(&dir)? {
            let entrada = entrada?;
            let tipo = entrada.file_type()?;
            if tipo.is_dir() {
                pendientes.push(entrada.path());
            } else if tipo.is_file() {
                archivos.push(entrada.path());
            }
        }
    }
    archivos.sort();
    Ok(archivos)
}

/// Descomprime un `.bbproyecto` dentro de `carpeta` (en una subcarpeta con el
/// nombre del proyecto) y devuelve dónde quedó. No lo abre: para eso hace falta
/// la contraseña. Rechaza el archivo entero si no es un proyecto de BranchBoard
/// o si trae rutas fuera de su estructura.
pub fn importar(archivo: &Path, carpeta: &str) -> Resultado<ProyectoLocal> {
    let padre = carpeta_padre_valida(carpeta)?;
    let zip_err = |_| {
        Error::Validacion(format!(
            "Ese archivo no es un proyecto exportado de BranchBoard (.{EXTENSION_EXPORTADO})"
        ))
    };
    let mut zip = zip::ZipArchive::new(File::open(archivo)?).map_err(zip_err)?;
    let marca = {
        let mut entrada = zip.by_name(ARCHIVO_MARCA).map_err(zip_err)?;
        let mut texto = String::new();
        entrada
            .by_ref()
            .take(64 * 1024)
            .read_to_string(&mut texto)
            .map_err(|_| no_es_proyecto())?;
        validar_marca(&texto)?
    };
    // Revisa todas las rutas antes de escribir nada.
    let mut total: u64 = 0;
    let mut tiene_db = false;
    for i in 0..zip.len() {
        let entrada = zip.by_index(i).map_err(zip_err)?;
        let ruta = entrada.enclosed_name().ok_or_else(ruta_no_permitida)?;
        if !ruta_permitida(&ruta) {
            return Err(ruta_no_permitida());
        }
        tiene_db |= ruta == Path::new(CARPETA_DATOS).join(NOMBRE_DB);
        total = total.saturating_add(entrada.size());
    }
    if !tiene_db {
        return Err(Error::Validacion(
            "Al archivo le falta la base del proyecto".into(),
        ));
    }
    if total > MAXIMO_IMPORTADO {
        return Err(Error::Validacion("El archivo es demasiado grande".into()));
    }
    let destino = ruta_libre(&padre, &nombre_carpeta(&marca.nombre)?);
    let temporal = padre.join(format!(".branchboard-importando-{}", id_nuevo()));
    let resultado = (|| -> Resultado<()> {
        for i in 0..zip.len() {
            let mut entrada = zip.by_index(i).map_err(zip_err)?;
            let ruta = temporal.join(entrada.enclosed_name().ok_or_else(ruta_no_permitida)?);
            if entrada.is_dir() {
                std::fs::create_dir_all(&ruta)?;
                continue;
            }
            if let Some(dir) = ruta.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut salida = File::create(&ruta)?;
            std::io::copy(&mut entrada.by_ref().take(MAXIMO_IMPORTADO), &mut salida)?;
        }
        std::fs::create_dir_all(temporal.join(CARPETA_COMPARTIDA).join("ops"))?;
        std::fs::rename(&temporal, &destino)?;
        Ok(())
    })();
    if let Err(e) = resultado {
        let _ = std::fs::remove_dir_all(&temporal);
        return Err(e);
    }
    Ok(ProyectoLocal {
        nombre: marca.nombre,
        ruta: destino.to_string_lossy().to_string(),
    })
}

fn ruta_no_permitida() -> Error {
    Error::Validacion(
        "El archivo trae rutas que no son de un proyecto de BranchBoard; no se importó nada".into(),
    )
}

/// Solo la marca y lo que cuelga de `datos/` o `compartida/`.
fn ruta_permitida(ruta: &Path) -> bool {
    let partes: Vec<Component> = ruta.components().collect();
    if partes.iter().any(|c| !matches!(c, Component::Normal(_))) {
        return false;
    }
    match partes.first() {
        Some(Component::Normal(p)) if *p == ARCHIVO_MARCA => partes.len() == 1,
        Some(Component::Normal(p)) => *p == CARPETA_DATOS || *p == CARPETA_COMPARTIDA,
        _ => false,
    }
}

fn leer_recientes(datos_app: &Path) -> Vec<ProyectoLocal> {
    std::fs::read_to_string(datos_app.join(ARCHIVO_RECIENTES))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Pone `proyecto` primero en "Recientes" (sin repetir la misma carpeta).
pub fn registrar_reciente(datos_app: &Path, proyecto: ProyectoLocal) -> Resultado<()> {
    let mut lista = leer_recientes(datos_app);
    lista.retain(|p| !p.ruta.eq_ignore_ascii_case(&proyecto.ruta));
    lista.insert(0, proyecto);
    lista.truncate(20);
    std::fs::create_dir_all(datos_app)?;
    let json = serde_json::to_string_pretty(&lista).map_err(|e| Error::Interno(e.to_string()))?;
    std::fs::write(datos_app.join(ARCHIVO_RECIENTES), json)?;
    Ok(())
}

/// Proyectos locales abiertos antes en esta máquina que siguen en su carpeta.
pub fn listar_recientes(datos_app: &Path) -> Vec<ProyectoLocal> {
    let mut vistos = BTreeMap::new();
    leer_recientes(datos_app)
        .into_iter()
        .filter(|p| leer_marca(Path::new(&p.ruta)).is_ok())
        .filter(|p| vistos.insert(p.ruta.to_lowercase(), ()).is_none())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHORA: &str = "2026-11-19T10:00:00Z";
    const HOY: &str = "2026-11-19";

    fn nuevo(carpeta: &Path, proyecto: &str) -> NuevoProyectoLocal {
        NuevoProyectoLocal {
            carpeta: carpeta.to_string_lossy().to_string(),
            proyecto: proyecto.into(),
            nombre: "Ana María Pérez".into(),
            correo: "Ana@Ejemplo.dev".into(),
        }
    }

    #[test]
    fn crear_deja_la_marca_y_abrir_entra_como_administrador() {
        let base = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        let carpeta = crear(&nuevo(base.path(), "Mi Juego"), AHORA, HOY).unwrap();
        assert_eq!(carpeta, base.path().join("Mi Juego"));
        let marca = leer_marca(&carpeta).unwrap();
        assert_eq!(
            (marca.nombre.as_str(), marca.formato),
            ("Mi Juego", FORMATO)
        );
        assert!(carpeta.join(CARPETA_DATOS).join(NOMBRE_DB).is_file());
        // Sin llaves ni contraseña desde #243.
        assert!(!carpeta.join(CARPETA_DATOS).join("admin.key").exists());

        let estado = AppState::default();
        let entrada = abrir(&estado, app.path(), &carpeta, true).unwrap();
        assert_eq!(
            entrada,
            Entrada::Dentro {
                email: "ana@ejemplo.dev".into(),
                rol: Rol::Administrador,
                rol_verificado: true,
                proyecto: "Mi Juego".into(),
                configurado_ahora: true,
                vistos: BTreeMap::new(),
            }
        );
        // Queda como Administrador con permisos y la carpeta compartida dentro del proyecto.
        assert!(estado.exigir_permiso("exportar_proyecto_local").is_ok());
        assert_eq!(
            eventos::carpeta_compartida(&carpeta.join(CARPETA_DATOS)).unwrap(),
            Some(carpeta.join(CARPETA_COMPARTIDA))
        );
        let usuario = estado
            .con_db(|db| usuarios::buscar(db.conexion(), "ana@ejemplo.dev"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (usuario.nombre.as_str(), usuario.apellido.as_str()),
            ("Ana", "María Pérez")
        );
        assert_eq!(
            listar_recientes(app.path()),
            vec![ProyectoLocal {
                nombre: "Mi Juego".into(),
                ruta: carpeta.to_string_lossy().to_string()
            }]
        );
    }

    #[test]
    fn una_carpeta_cualquiera_no_es_un_proyecto() {
        let base = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        let estado = AppState::default();
        std::fs::write(base.path().join("algo.txt"), "hola").unwrap();
        let e = abrir(&estado, app.path(), base.path(), false).unwrap_err();
        assert_eq!(
            e.to_string(),
            "En esa carpeta no hay un proyecto de BranchBoard."
        );
        // Una marca de otra cosa (p. ej. la de la carpeta compartida) tampoco.
        std::fs::write(base.path().join(ARCHIVO_MARCA), r#"{"proyecto":"x"}"#).unwrap();
        assert!(abrir(&estado, app.path(), base.path(), false).is_err());
        // Una marca de una versión más nueva pide actualizar.
        let nueva = serde_json::json!({
            "aplicacion": "BranchBoard", "tipo": "local", "formato": FORMATO + 1,
            "id": "abc", "nombre": "X", "creado_en": AHORA
        });
        std::fs::write(base.path().join(ARCHIVO_MARCA), nueva.to_string()).unwrap();
        let e = leer_marca(base.path()).unwrap_err();
        assert!(e.to_string().contains("versión más nueva"));
    }

    #[test]
    fn crear_valida_los_datos_y_no_pisa_carpetas() {
        let base = tempfile::tempdir().unwrap();
        let mut datos = nuevo(base.path(), "Juego");
        datos.correo = "sin-arroba".into();
        assert!(crear(&datos, AHORA, HOY).is_err());
        assert!(!base.path().join("Juego").exists(), "nada a medias");
        std::fs::create_dir(base.path().join("Ocupada")).unwrap();
        assert!(crear(&nuevo(base.path(), "Ocupada"), AHORA, HOY).is_err());
        assert!(crear(&nuevo(base.path(), "CON"), AHORA, HOY).is_err());
        assert_eq!(nombre_carpeta("a:b/c?").unwrap(), "a_b_c_");
    }

    #[test]
    fn exportar_e_importar_conserva_el_proyecto() {
        let base = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        let carpeta = crear(&nuevo(base.path(), "Juego"), AHORA, HOY).unwrap();
        let estado = AppState::default();
        abrir(&estado, app.path(), &carpeta, false).unwrap();
        std::fs::write(
            carpeta.join(CARPETA_COMPARTIDA).join("ops").join("e1.json"),
            "{}",
        )
        .unwrap();
        estado
            .con_db(|db| {
                configuracion::guardar(db.conexion(), "prueba", "42", "ana@ejemplo.dev", HOY)
            })
            .unwrap();

        let salida = tempfile::tempdir().unwrap();
        let archivo = salida.path().join("Juego.bbproyecto");
        exportar(&estado, &archivo).unwrap();
        let nombres: Vec<String> = {
            let zip = zip::ZipArchive::new(File::open(&archivo).unwrap()).unwrap();
            zip.file_names().map(String::from).collect()
        };
        assert!(nombres.contains(&ARCHIVO_MARCA.to_string()));
        assert!(nombres.contains(&"datos/branchboard.db".to_string()));
        assert!(nombres.contains(&"compartida/ops/e1.json".to_string()));
        assert!(!nombres
            .iter()
            .any(|n| n.contains("backups") || n == "datos/local.json" || n.ends_with("-wal")));
        assert!(
            std::fs::read_dir(salida.path()).unwrap().count() == 1,
            "no deja temporales"
        );

        // Otra máquina: importa en otra carpeta (dos veces → no pisa) y abre.
        let otra = tempfile::tempdir().unwrap();
        let otra_app = tempfile::tempdir().unwrap();
        let destino = otra.path().to_string_lossy().to_string();
        let importado = importar(&archivo, &destino).unwrap();
        assert_eq!(importado.ruta, otra.path().join("Juego").to_string_lossy());
        let segunda = importar(&archivo, &destino).unwrap();
        assert_eq!(
            segunda.ruta,
            otra.path().join("Juego (2)").to_string_lossy()
        );

        let estado_b = AppState::default();
        let ruta = PathBuf::from(&importado.ruta);
        abrir(&estado_b, otra_app.path(), &ruta, false).unwrap();
        let valor = estado_b
            .con_db(|db| configuracion::leer(db.conexion(), "prueba"))
            .unwrap();
        assert_eq!(valor.as_deref(), Some("42"));
        assert_eq!(
            eventos::carpeta_compartida(&ruta.join(CARPETA_DATOS)).unwrap(),
            Some(ruta.join(CARPETA_COMPARTIDA)),
            "la carpeta compartida apunta a la copia nueva, no a la original"
        );
        assert!(ruta
            .join(CARPETA_COMPARTIDA)
            .join("ops")
            .join("e1.json")
            .is_file());
    }

    #[test]
    fn importar_rechaza_archivos_ajenos_o_manipulados() {
        let dir = tempfile::tempdir().unwrap();
        let destino = dir.path().to_string_lossy().to_string();
        let opciones = zip::write::SimpleFileOptions::default();
        let marca = serde_json::json!({
            "aplicacion": "BranchBoard", "tipo": "local", "formato": FORMATO,
            "id": "abc", "nombre": "Malo", "creado_en": AHORA
        })
        .to_string();
        let armar = |nombre: &str, entradas: &[(&str, &str)]| {
            let ruta = dir.path().join(nombre);
            let mut zip = zip::ZipWriter::new(File::create(&ruta).unwrap());
            for (n, contenido) in entradas {
                zip.start_file(*n, opciones).unwrap();
                zip.write_all(contenido.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
            ruta
        };
        // Un zip sin marca.
        let sin_marca = armar("a.bbproyecto", &[("datos/branchboard.db", "x")]);
        assert!(importar(&sin_marca, &destino).is_err());
        // Una ruta fuera de la estructura.
        let fuera = armar(
            "b.bbproyecto",
            &[
                (ARCHIVO_MARCA, &marca),
                ("datos/branchboard.db", "x"),
                ("otra/virus.exe", "x"),
            ],
        );
        assert!(importar(&fuera, &destino).is_err());
        // Un archivo que ni siquiera es zip.
        let texto = dir.path().join("c.bbproyecto");
        std::fs::write(&texto, "hola").unwrap();
        assert!(importar(&texto, &destino).is_err());
        assert!(!dir.path().join("Malo").exists(), "no escribió nada");
        assert!(ruta_permitida(Path::new("datos/admin.key")));
        assert!(!ruta_permitida(Path::new("../datos/admin.key")));
        assert!(!ruta_permitida(Path::new(
            "branchboard-proyecto-local.json/x"
        )));
    }
}
