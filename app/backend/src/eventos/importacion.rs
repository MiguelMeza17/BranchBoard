//! Subir al servidor un proyecto que ya existía (#245): una **foto del estado** en un solo evento.
//!
//! Decisión del 2026-11-21 en #245: no se sube el historial de `ops/` (vive en Drive, el servidor pondría
//! a quien lo sube como autor de todo y los proyectos compactados no lo tienen completo). Se manda un
//! evento `importar_proyecto` con las filas de las tablas compartidas de la base local, tal cual (los
//! autores originales van dentro). Solo lo acepta el servidor de un Administrador (`permisos_evento`) y
//! solo se aplica en un proyecto **vacío**.
//!
//! - **Qué va**: las tablas que llenan los eventos ([`TABLAS`]). No van las de cada PC (lecturas,
//!   paneles vistos, bandeja, copia del servidor, `eventos_aplicados`, changesets revisados) ni
//!   `usuarios` (el equipo se sube con las funciones de roles, #241) ni `proyecto_servidor_id`.
//! - **Notificaciones**: llegan leídas; si no, todo el equipo vería de golpe avisos viejos.
//! - **Versiones**: se copia por nombre de columna. Si la foto trae una columna que esta versión de la
//!   app no conoce, queda pendiente ([`Aplicacion::Desconocido`]) hasta que se actualice.

use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde_json::{Map, Value};

use super::lectura::Aplicacion;
use super::Evento;
use crate::db::configuracion::CLAVE_PROYECTO_SERVIDOR;
use crate::error::{Error, Resultado};

/// Tipo del evento.
pub const TIPO: &str = "importar_proyecto";
/// Versión del formato de la foto.
pub const FORMATO: u64 = 1;

/// Tablas compartidas, en un orden que respeta sus referencias (sprints y tareas antes que lo que
/// cuelga de ellas).
pub const TABLAS: &[&str] = &[
    "configuracion_proyecto",
    "sprints",
    "tareas",
    "ediciones_pendientes",
    "reportes_sprint",
    "commits",
    "cambios_prestados",
    "cambios_directos",
    "documentos",
    "notificaciones",
    "grupos_chat",
    "mensajes_chat",
];

/// Claves de `configuracion_proyecto` que son de cada PC y no viajan.
const CLAVES_LOCALES: &[&str] = &[CLAVE_PROYECTO_SERVIDOR, "subida_pendiente_id"];

/// Tablas que tienen que estar vacías para aceptar la foto.
const DEBEN_ESTAR_VACIAS: &[&str] = &["tareas", "sprints", "mensajes_chat", "documentos"];

fn a_json(v: Sql) -> Value {
    match v {
        Sql::Null => Value::Null,
        Sql::Integer(i) => Value::from(i),
        Sql::Real(f) => Value::from(f),
        Sql::Text(t) => Value::String(t),
        // No hay columnas BLOB en las tablas compartidas; por si acaso, como texto.
        Sql::Blob(b) => Value::String(String::from_utf8_lossy(&b).into_owned()),
    }
}

fn a_sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .or_else(|| n.as_f64().map(Sql::Real))
            .unwrap_or(Sql::Null),
        Value::String(s) => Sql::Text(s.clone()),
        otro => Sql::Text(otro.to_string()),
    }
}

/// Nombres de columna de `tabla` en esta base.
fn columnas(c: &Connection, tabla: &str) -> Resultado<Vec<String>> {
    let mut consulta = c.prepare(&format!("PRAGMA table_info({tabla})"))?;
    let nombres = consulta
        .query_map([], |f| f.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(nombres)
}

/// La foto del estado compartido de la base: `{ formato, tablas: { nombre: [ {columna: valor} ] } }`.
pub fn foto(c: &Connection) -> Resultado<Value> {
    let mut tablas = Map::new();
    for tabla in TABLAS {
        let cols = columnas(c, tabla)?;
        let mut consulta = c.prepare(&format!("SELECT * FROM {tabla}"))?;
        let mut filas = Vec::new();
        let mut resultado = consulta.query([])?;
        while let Some(f) = resultado.next()? {
            let mut fila = Map::new();
            for (i, col) in cols.iter().enumerate() {
                fila.insert(col.clone(), a_json(f.get::<_, Sql>(i)?));
            }
            if *tabla == "configuracion_proyecto"
                && fila
                    .get("clave")
                    .and_then(Value::as_str)
                    .is_some_and(|k| CLAVES_LOCALES.contains(&k))
            {
                continue;
            }
            if *tabla == "notificaciones" {
                fila.insert("leida".into(), Value::from(1));
            }
            filas.push(Value::Object(fila));
        }
        tablas.insert((*tabla).to_string(), Value::Array(filas));
    }
    Ok(serde_json::json!({ "formato": FORMATO, "tablas": tablas }))
}

/// Cuántas filas trae la foto (para el resumen de la subida).
pub fn filas_en(foto: &Value) -> usize {
    foto["tablas"]
        .as_object()
        .map(|t| t.values().filter_map(Value::as_array).map(Vec::len).sum())
        .unwrap_or(0)
}

fn vacio(c: &Connection) -> Resultado<bool> {
    for tabla in DEBEN_ESTAR_VACIAS {
        let n: i64 = c.query_row(&format!("SELECT COUNT(*) FROM {tabla}"), [], |f| f.get(0))?;
        if n > 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Aplica la foto en `c` (dentro de la transacción del evento). Ver el inicio del módulo.
pub fn aplicar(c: &Connection, evento: &Evento) -> Resultado<Aplicacion> {
    if evento.payload["formato"].as_u64() != Some(FORMATO) {
        return Ok(Aplicacion::Desconocido);
    }
    let Some(tablas) = evento.payload["tablas"].as_object() else {
        return Ok(Aplicacion::Descartado("la foto del proyecto no trae tablas".into()));
    };
    if tablas.keys().any(|t| !TABLAS.contains(&t.as_str())) {
        return Ok(Aplicacion::Desconocido);
    }
    if !vacio(c)? {
        return Ok(Aplicacion::Descartado(
            "el proyecto ya tiene datos: la foto solo se aplica en uno vacío".into(),
        ));
    }
    // Las columnas que trae la foto tienen que existir aquí (si no, es de una versión más nueva).
    for (tabla, filas) in tablas {
        let cols = columnas(c, tabla)?;
        let desconocida = filas
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .flat_map(|f| f.keys())
            .any(|k| !cols.contains(k));
        if desconocida {
            return Ok(Aplicacion::Desconocido);
        }
    }
    c.pragma_update(None, "defer_foreign_keys", "ON")?;
    for tabla in TABLAS {
        let Some(filas) = tablas.get(*tabla).and_then(Value::as_array) else {
            continue;
        };
        for fila in filas.iter().filter_map(Value::as_object) {
            let nombres: Vec<&String> = fila.keys().collect();
            if nombres.is_empty() {
                continue;
            }
            let lista = nombres.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
            let marcas = (1..=nombres.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
            // La configuración puede traer claves que la base ya tiene (p. ej. el nombre): gana la foto.
            let verbo = if *tabla == "configuracion_proyecto" {
                "INSERT OR REPLACE"
            } else {
                "INSERT"
            };
            let valores: Vec<Sql> = nombres.iter().map(|n| a_sql(&fila[*n])).collect();
            c.execute(
                &format!("{verbo} INTO {tabla} ({lista}) VALUES ({marcas})"),
                rusqlite::params_from_iter(valores),
            )
            .map_err(|e| Error::BaseDatos(format!("no se pudo copiar {tabla}: {e}")))?;
        }
    }
    Ok(Aplicacion::Aplicado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::configuracion::{self, CLAVE_NOMBRE_PROYECTO};

    fn base() -> crate::db::Db {
        crate::db::Db::en_memoria().unwrap()
    }

    fn sembrar(c: &Connection) {
        configuracion::guardar(c, CLAVE_NOMBRE_PROYECTO, "Juego", "ana@virustf.dev", "2026-11-01").unwrap();
        configuracion::guardar(c, CLAVE_PROYECTO_SERVIDOR, "local-no-viaja", "ana@virustf.dev", "2026-11-01")
            .unwrap();
        c.execute_batch(&format!(
            "INSERT INTO sprints (id, nombre, con_fecha, fecha_inicio, duracion_dias, fecha_fin, creado_por)
               VALUES ('S-1', 'Sprint 1', 1, '2026-11-01', 14, '2026-11-14', 'ana@virustf.dev');
             INSERT INTO tareas (id, titulo, detalles, tipo, estado, creado_por, sprint_id, fecha_creacion)
               VALUES ('T-101', 'Saltar', 'con física', 'Scripting', 'por hacer', 'beto@virustf.dev', 'S-1', '2026-11-02');
             INSERT INTO notificaciones (id, tipo, para, titulo, fecha) VALUES ('N-1', '{}', 'beto@virustf.dev', 'Hola', '2026-11-02');",
            crate::modelo::TipoNotificacion::TODOS[0].como_texto()
        ))
        .unwrap();
    }

    fn evento(foto: Value) -> Evento {
        Evento::nuevo(TIPO, None, "ana@virustf.dev", foto)
    }

    #[test]
    fn la_foto_se_aplica_en_un_proyecto_vacio_con_los_autores_originales() {
        let origen = base();
        sembrar(origen.conexion());
        let foto = foto(origen.conexion()).unwrap();
        assert_eq!(filas_en(&foto), 4, "nombre, sprint, tarea y notificación (no el id del servidor)");

        let destino = base();
        let c = destino.conexion();
        configuracion::guardar(c, CLAVE_PROYECTO_SERVIDOR, "el-de-destino", "x@virustf.dev", "2026-11-21").unwrap();
        let tx = c.unchecked_transaction().unwrap();
        assert_eq!(aplicar(&tx, &evento(foto)).unwrap(), Aplicacion::Aplicado);
        tx.commit().unwrap();

        let t = crate::db::tareas::obtener(c, "T-101").unwrap();
        assert_eq!(t.titulo, "Saltar");
        assert_eq!(t.creado_por, "beto@virustf.dev", "el autor es el original, no quien subió");
        assert_eq!(t.sprint_id.as_deref(), Some("S-1"));
        assert_eq!(configuracion::leer(c, CLAVE_NOMBRE_PROYECTO).unwrap().as_deref(), Some("Juego"));
        assert_eq!(
            configuracion::leer(c, CLAVE_PROYECTO_SERVIDOR).unwrap().as_deref(),
            Some("el-de-destino"),
            "el id del servidor es de cada PC"
        );
        let leida: i64 = c
            .query_row("SELECT leida FROM notificaciones WHERE id = 'N-1'", [], |f| f.get(0))
            .unwrap();
        assert_eq!(leida, 1, "las notificaciones viejas llegan leídas");
    }

    #[test]
    fn en_un_proyecto_con_datos_se_descarta() {
        let origen = base();
        sembrar(origen.conexion());
        let foto = foto(origen.conexion()).unwrap();
        let destino = base();
        sembrar(destino.conexion());
        assert!(matches!(
            aplicar(destino.conexion(), &evento(foto)).unwrap(),
            Aplicacion::Descartado(_)
        ));
    }

    #[test]
    fn una_columna_o_formato_que_no_conoce_queda_pendiente() {
        let destino = base();
        let c = destino.conexion();
        let nueva_columna = serde_json::json!({
            "formato": FORMATO,
            "tablas": { "sprints": [{ "id": "S-1", "columna_del_futuro": 1 }] }
        });
        assert_eq!(aplicar(c, &evento(nueva_columna)).unwrap(), Aplicacion::Desconocido);
        let nuevo_formato = serde_json::json!({ "formato": FORMATO + 1, "tablas": {} });
        assert_eq!(aplicar(c, &evento(nuevo_formato)).unwrap(), Aplicacion::Desconocido);
        let tabla_nueva = serde_json::json!({ "formato": FORMATO, "tablas": { "otra": [] } });
        assert_eq!(aplicar(c, &evento(tabla_nueva)).unwrap(), Aplicacion::Desconocido);
    }
}
