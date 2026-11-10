//! Copias de seguridad del `.db` de un proyecto, en `<carpeta del proyecto>/backups/`.
//!
//! Se copia al abrir el proyecto (antes de tocar el `.db`) y antes de restaurar
//! otra copia. Se conservan las últimas [`MAX_COPIAS`]; la copia guardada antes
//! de una restauración no cuenta para ese límite hasta que se restaure otra.
//! La copia se hace con `VACUUM INTO`, así queda consistente aunque haya un
//! `-wal` pendiente. Todo es local a la máquina: no pasa por `ops/`.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::NOMBRE_DB;
use crate::error::{Error, Resultado};

/// Subcarpeta de la carpeta del proyecto donde van las copias.
pub const CARPETA: &str = "backups";
/// Cuántas copias se conservan (sin contar la guardada antes de restaurar).
pub const MAX_COPIAS: usize = 10;
/// `id` de la fila "Actual" (la base en uso, no restaurable).
pub const ID_ACTUAL: &str = "actual";

const PREFIJO: &str = "branchboard_";
const EXTENSION: &str = ".db";

/// Una copia, con los campos de `Backup` en el frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    /// Nombre del archivo dentro de `backups/` (o [`ID_ACTUAL`]).
    pub id: String,
    /// `AAAA-MM-DD HH:MM`, hora local.
    pub fecha: String,
    /// Legible, ej. `2.4 MB`.
    pub tamano: String,
    /// Solo si se guardó al restaurar otra copia.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nota: Option<String>,
}

/// Lo que ve Ajustes: la base en uso arriba y las copias debajo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListaBackups {
    pub actual: Backup,
    /// De la más reciente a la más antigua.
    pub copias: Vec<Backup>,
}

pub fn carpeta_backups(carpeta_proyecto: &Path) -> PathBuf {
    carpeta_proyecto.join(CARPETA)
}

/// `1.234.567` bytes → `1.2 MB`.
pub fn formatear_tamano(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}

/// `branchboard_2026-11-10_14-05-33.db` → `2026-11-10 14:05`.
fn fecha_desde_nombre(nombre: &str) -> Option<String> {
    let marca = nombre.strip_prefix(PREFIJO)?.get(..16)?;
    let (dia, hora) = marca.split_once('_')?;
    Some(format!("{dia} {}", hora.replacen('-', ":", 1)))
}

fn es_copia(nombre: &str) -> bool {
    nombre.starts_with(PREFIJO) && nombre.ends_with(EXTENSION)
}

/// Nombre libre para una copia nueva en esta hora; si ya hay una del mismo
/// segundo, agrega `_02`, `_03`… (dos cifras para que el orden por nombre siga
/// siendo el cronológico).
fn nombre_libre(carpeta: &Path) -> String {
    let marca = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let base = format!("{PREFIJO}{marca}");
    let mut nombre = format!("{base}{EXTENSION}");
    let mut n = 1;
    while carpeta.join(&nombre).exists() {
        n += 1;
        nombre = format!("{base}_{n:02}{EXTENSION}");
    }
    nombre
}

/// Valida que `nombre` sea el archivo de una copia existente (sin rutas ni `..`).
fn archivo_copia(carpeta_proyecto: &Path, nombre: &str) -> Resultado<PathBuf> {
    let valido = es_copia(nombre)
        && nombre
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !nombre.contains("..");
    let ruta = carpeta_backups(carpeta_proyecto).join(nombre);
    if !valido || !ruta.is_file() {
        return Err(Error::NoEncontrado(format!("la copia {nombre}")));
    }
    Ok(ruta)
}

fn tamano_archivo(ruta: &Path) -> u64 {
    fs::metadata(ruta).map(|m| m.len()).unwrap_or(0)
}

fn leer_nota(ruta_copia: &Path) -> Option<String> {
    let nota = fs::read_to_string(ruta_copia.with_extension("nota")).ok()?;
    let nota = nota.trim();
    (!nota.is_empty()).then(|| nota.to_string())
}

fn copia_desde_ruta(ruta: &Path) -> Option<Backup> {
    let nombre = ruta.file_name()?.to_str()?;
    if !es_copia(nombre) {
        return None;
    }
    Some(Backup {
        id: nombre.to_string(),
        fecha: fecha_desde_nombre(nombre)?,
        tamano: formatear_tamano(tamano_archivo(ruta)),
        nota: leer_nota(ruta),
    })
}

/// Rutas de las copias, de la más antigua a la más reciente.
fn rutas_copias(carpeta_proyecto: &Path) -> Resultado<Vec<PathBuf>> {
    let carpeta = carpeta_backups(carpeta_proyecto);
    if !carpeta.is_dir() {
        return Ok(Vec::new());
    }
    let mut rutas: Vec<PathBuf> = fs::read_dir(&carpeta)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|r| r.is_file() && r.file_name().and_then(|n| n.to_str()).is_some_and(es_copia))
        .collect();
    rutas.sort();
    Ok(rutas)
}

/// Copia el `.db` del proyecto a `backups/`. `None` si todavía no hay `.db`
/// (proyecto recién creado). Con `nota`, la copia queda marcada (es la que se
/// guarda antes de restaurar) y no cuenta para el límite de copias.
pub fn respaldar(carpeta_proyecto: &Path, nota: Option<&str>) -> Resultado<Option<Backup>> {
    let origen = carpeta_proyecto.join(NOMBRE_DB);
    if !origen.is_file() {
        return Ok(None);
    }
    let carpeta = carpeta_backups(carpeta_proyecto);
    fs::create_dir_all(&carpeta)?;
    let destino = carpeta.join(nombre_libre(&carpeta));

    let conexion = Connection::open(&origen)?;
    let destino_texto = destino
        .to_str()
        .ok_or_else(|| Error::Archivos("La ruta de la copia no es válida".into()))?;
    conexion.execute("VACUUM INTO ?1", [destino_texto])?;
    drop(conexion);

    if let Some(nota) = nota {
        fs::write(destino.with_extension("nota"), nota)?;
    }
    podar(carpeta_proyecto)?;
    Ok(copia_desde_ruta(&destino))
}

/// Borra las copias más viejas hasta dejar [`MAX_COPIAS`], sin contar la copia
/// con nota más reciente (la guardada antes de la última restauración).
fn podar(carpeta_proyecto: &Path) -> Resultado<()> {
    let rutas = rutas_copias(carpeta_proyecto)?;
    let protegida = rutas.iter().rev().find(|r| leer_nota(r).is_some()).cloned();
    let mut candidatas: Vec<&PathBuf> = rutas
        .iter()
        .filter(|r| Some(*r) != protegida.as_ref())
        .collect();
    while candidatas.len() > MAX_COPIAS {
        let vieja = candidatas.remove(0);
        fs::remove_file(vieja)?;
        let _ = fs::remove_file(vieja.with_extension("nota"));
    }
    Ok(())
}

/// Copias disponibles, de la más reciente a la más antigua.
pub fn listar(carpeta_proyecto: &Path) -> Resultado<Vec<Backup>> {
    Ok(rutas_copias(carpeta_proyecto)?
        .iter()
        .rev()
        .filter_map(|r| copia_desde_ruta(r))
        .collect())
}

/// La base en uso, como fila "Actual" (fecha de ahora, tamaño del `.db` más su `-wal`).
pub fn actual(carpeta_proyecto: &Path) -> Backup {
    let db = carpeta_proyecto.join(NOMBRE_DB);
    let wal = carpeta_proyecto.join(format!("{NOMBRE_DB}-wal"));
    Backup {
        id: ID_ACTUAL.into(),
        fecha: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        tamano: formatear_tamano(tamano_archivo(&db) + tamano_archivo(&wal)),
        nota: None,
    }
}

pub fn lista(carpeta_proyecto: &Path) -> Resultado<ListaBackups> {
    Ok(ListaBackups {
        actual: actual(carpeta_proyecto),
        copias: listar(carpeta_proyecto)?,
    })
}

/// Reemplaza el `.db` por la copia `nombre`, guardando antes la base actual como
/// una copia más (con nota). **La base del proyecto debe estar cerrada**: quien
/// llama la cierra antes y la vuelve a abrir después, pase lo que pase.
pub fn restaurar(carpeta_proyecto: &Path, nombre: &str) -> Resultado<Backup> {
    if nombre == ID_ACTUAL {
        return Err(Error::Validacion("La base actual ya está en uso".into()));
    }
    let origen = archivo_copia(carpeta_proyecto, nombre)?;
    let elegida = copia_desde_ruta(&origen)
        .ok_or_else(|| Error::NoEncontrado(format!("la copia {nombre}")))?;
    let nota = format!("Guardada antes de restaurar la copia del {}", elegida.fecha);
    respaldar(carpeta_proyecto, Some(&nota))?;

    let db = carpeta_proyecto.join(NOMBRE_DB);
    fs::copy(&origen, &db)?;
    for sufijo in ["-wal", "-shm"] {
        let _ = fs::remove_file(carpeta_proyecto.join(format!("{NOMBRE_DB}{sufijo}")));
    }
    Ok(elegida)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn proyecto() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let carpeta = dir.path().join("VirusTF");
        (dir, carpeta)
    }

    fn guardar_meta(carpeta: &Path, valor: &str) {
        let db = Db::abrir(carpeta).unwrap();
        db.conexion()
            .execute(
                "INSERT OR REPLACE INTO meta (clave, valor) VALUES ('prueba', ?1)",
                [valor],
            )
            .unwrap();
    }

    fn leer_meta(carpeta: &Path) -> Option<String> {
        let db = Db::abrir(carpeta).unwrap();
        db.conexion()
            .query_row("SELECT valor FROM meta WHERE clave = 'prueba'", [], |f| {
                f.get(0)
            })
            .ok()
    }

    #[test]
    fn sin_db_no_hay_copia() {
        let (_dir, carpeta) = proyecto();
        assert_eq!(respaldar(&carpeta, None).unwrap(), None);
        assert!(listar(&carpeta).unwrap().is_empty());
        assert_eq!(actual(&carpeta).tamano, "0 B");
    }

    #[test]
    fn la_copia_incluye_lo_pendiente_en_el_wal() {
        let (_dir, carpeta) = proyecto();
        let db = Db::abrir(&carpeta).unwrap();
        db.conexion()
            .execute(
                "INSERT INTO meta (clave, valor) VALUES ('prueba', 'abierta')",
                [],
            )
            .unwrap();
        let copia = respaldar(&carpeta, None).unwrap().unwrap();
        drop(db);
        assert!(copia.id.starts_with(PREFIJO) && copia.id.ends_with(EXTENSION));
        assert_eq!(copia.fecha.len(), "2026-11-10 14:05".len());
        assert_eq!(copia.nota, None);

        let restaurada = Connection::open(carpeta_backups(&carpeta).join(&copia.id)).unwrap();
        let valor: String = restaurada
            .query_row("SELECT valor FROM meta WHERE clave = 'prueba'", [], |f| {
                f.get(0)
            })
            .unwrap();
        assert_eq!(valor, "abierta");
        assert_eq!(listar(&carpeta).unwrap(), vec![copia]);
        assert_eq!(lista(&carpeta).unwrap().actual.id, ID_ACTUAL);
    }

    #[test]
    fn se_conservan_diez_mas_la_guardada_antes_de_restaurar() {
        let (_dir, carpeta) = proyecto();
        guardar_meta(&carpeta, "1");
        let con_nota = respaldar(&carpeta, Some("Guardada antes de restaurar"))
            .unwrap()
            .unwrap();
        let mut normales = Vec::new();
        for _ in 0..12 {
            normales.push(respaldar(&carpeta, None).unwrap().unwrap().id);
        }
        let ids: Vec<String> = listar(&carpeta)
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids.len(), MAX_COPIAS + 1);
        assert!(ids.contains(&con_nota.id));
        assert!(!ids.contains(&normales[0]) && !ids.contains(&normales[1]));
        assert_eq!(ids[0], normales[11], "la más reciente va primero");

        let otra_con_nota = respaldar(&carpeta, Some("Otra")).unwrap().unwrap();
        let ids: Vec<String> = listar(&carpeta)
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids.len(), MAX_COPIAS + 1);
        assert!(ids.contains(&otra_con_nota.id));
        assert!(
            !ids.contains(&con_nota.id),
            "la nota vieja ya cuenta y se poda"
        );
        assert!(!carpeta_backups(&carpeta)
            .join(&con_nota.id)
            .with_extension("nota")
            .exists());
    }

    #[test]
    fn restaurar_guarda_la_actual_y_vuelve_a_la_copia() {
        let (_dir, carpeta) = proyecto();
        guardar_meta(&carpeta, "vieja");
        let copia = respaldar(&carpeta, None).unwrap().unwrap();
        guardar_meta(&carpeta, "nueva");

        let elegida = restaurar(&carpeta, &copia.id).unwrap();
        assert_eq!(elegida.id, copia.id);
        assert_eq!(leer_meta(&carpeta).as_deref(), Some("vieja"));

        let copias = listar(&carpeta).unwrap();
        assert_eq!(copias.len(), 2);
        assert_eq!(
            copias[0].nota.as_deref(),
            Some(format!("Guardada antes de restaurar la copia del {}", copia.fecha).as_str())
        );
        restaurar(&carpeta, &copias[0].id).unwrap();
        assert_eq!(leer_meta(&carpeta).as_deref(), Some("nueva"));
    }

    #[test]
    fn restaurar_rechaza_nombres_que_no_son_copias() {
        let (_dir, carpeta) = proyecto();
        guardar_meta(&carpeta, "1");
        respaldar(&carpeta, None).unwrap();
        assert_eq!(
            restaurar(&carpeta, ID_ACTUAL).unwrap_err().codigo(),
            "validacion"
        );
        for malo in ["", "otra.db", "../branchboard.db", "branchboard_x/..\\a.db"] {
            assert_eq!(
                restaurar(&carpeta, malo).unwrap_err().codigo(),
                "no_encontrado"
            );
        }
        assert_eq!(leer_meta(&carpeta).as_deref(), Some("1"));
    }

    #[test]
    fn tamanos_legibles() {
        assert_eq!(formatear_tamano(512), "512 B");
        assert_eq!(formatear_tamano(86_000), "84 KB");
        assert_eq!(formatear_tamano(2_516_582), "2.4 MB");
        assert_eq!(formatear_tamano(3 * 1024 * 1024 * 1024), "3.00 GB");
        assert_eq!(
            fecha_desde_nombre("branchboard_2026-11-10_14-05-33_02.db").as_deref(),
            Some("2026-11-10 14:05")
        );
        assert_eq!(fecha_desde_nombre("otra.db"), None);
    }
}
