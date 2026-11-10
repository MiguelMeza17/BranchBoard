//! Chat: `mensajes_chat` y `grupos_chat` viajan por `ops/`; `chat_leidos` y
//! `chats_eliminados` son **locales por usuario** (no salen de esta máquina).
//!
//! Una conversación es el grupo general (`grupo`), el id de un grupo creado por
//! el equipo o, en un privado, el email de la otra persona.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Resultado};

/// Grupo general del proyecto: siempre existe, están todos.
pub const GRUPO_GENERAL: &str = "grupo";

/// Un grupo creado por alguien del equipo (además del general).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrupoChat {
    pub id: String,
    pub nombre: String,
    /// Emails; incluye a quien lo creó.
    pub miembros: Vec<String>,
    pub creado_por: String,
    pub fecha: String,
}

/// Un mensaje, igual que `Mensaje` en el frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mensaje {
    pub id: String,
    pub de: String,
    /// Email (privado), `grupo` (general) o el id de un `GrupoChat`.
    pub para: String,
    pub texto: String,
    pub fecha: String,
    /// Quiénes ya lo leyeron **en esta máquina** (los leídos son locales).
    #[serde(default, rename = "leidoPor")]
    pub leido_por: Vec<String>,
}

fn lista_de_json(f: &Row<'_>, i: usize) -> rusqlite::Result<Vec<String>> {
    let texto: String = f.get(i)?;
    serde_json::from_str(&texto).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn grupo_de_fila(f: &Row<'_>) -> rusqlite::Result<GrupoChat> {
    Ok(GrupoChat {
        id: f.get(0)?,
        nombre: f.get(1)?,
        miembros: lista_de_json(f, 2)?,
        creado_por: f.get(3)?,
        fecha: f.get(4)?,
    })
}

const COLUMNAS_GRUPO: &str = "id, nombre, miembros, creado_por, fecha";

pub fn crear_grupo(c: &Connection, grupo: &GrupoChat) -> Resultado<()> {
    if grupo.nombre.trim().is_empty() {
        return Err(Error::Validacion("El grupo necesita un nombre".into()));
    }
    if grupo.id == GRUPO_GENERAL || grupo.id.contains('@') {
        return Err(Error::Validacion("Ese id de grupo no es válido".into()));
    }
    if !grupo.miembros.contains(&grupo.creado_por) {
        return Err(Error::Validacion(
            "Quien crea el grupo tiene que estar en él".into(),
        ));
    }
    if buscar_grupo(c, &grupo.id)?.is_some() {
        return Err(Error::Validacion(format!("Ya existe {}", grupo.id)));
    }
    let miembros =
        serde_json::to_string(&grupo.miembros).map_err(|e| Error::Interno(e.to_string()))?;
    c.execute(
        &format!("INSERT INTO grupos_chat ({COLUMNAS_GRUPO}) VALUES (?1, ?2, ?3, ?4, ?5)"),
        params![
            grupo.id,
            grupo.nombre,
            miembros,
            grupo.creado_por,
            grupo.fecha
        ],
    )?;
    Ok(())
}

pub fn buscar_grupo(c: &Connection, id: &str) -> Resultado<Option<GrupoChat>> {
    Ok(c.query_row(
        &format!("SELECT {COLUMNAS_GRUPO} FROM grupos_chat WHERE id = ?1"),
        [id],
        grupo_de_fila,
    )
    .optional()?)
}

/// Grupos en los que está una persona, en orden de creación.
pub fn grupos_de(c: &Connection, usuario: &str) -> Resultado<Vec<GrupoChat>> {
    let mut consulta = c.prepare(&format!(
        "SELECT {COLUMNAS_GRUPO} FROM grupos_chat ORDER BY rowid"
    ))?;
    let grupos = consulta.query_map([], grupo_de_fila)?;
    Ok(grupos
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|g| g.miembros.iter().any(|m| m == usuario))
        .collect())
}

const COLUMNAS_MENSAJE: &str = "id, de, para, texto, fecha";

fn mensaje_de_fila(f: &Row<'_>) -> rusqlite::Result<Mensaje> {
    Ok(Mensaje {
        id: f.get(0)?,
        de: f.get(1)?,
        para: f.get(2)?,
        texto: f.get(3)?,
        fecha: f.get(4)?,
        leido_por: Vec::new(),
    })
}

fn con_leidos(c: &Connection, mut mensajes: Vec<Mensaje>) -> Resultado<Vec<Mensaje>> {
    let mut consulta =
        c.prepare("SELECT usuario FROM chat_leidos WHERE mensaje_id = ?1 ORDER BY rowid")?;
    for m in &mut mensajes {
        m.leido_por = consulta
            .query_map([&m.id], |f| f.get(0))?
            .collect::<rusqlite::Result<_>>()?;
    }
    Ok(mensajes)
}

/// Guarda un mensaje. Si `leido_por` trae gente, queda marcado como leído por
/// ellos (quien lo envía ya lo cuenta como leído).
pub fn guardar_mensaje(c: &Connection, m: &Mensaje) -> Resultado<()> {
    if m.texto.trim().is_empty() {
        return Err(Error::Validacion("El mensaje está vacío".into()));
    }
    if buscar_mensaje(c, &m.id)?.is_some() {
        return Err(Error::Validacion(format!("Ya existe {}", m.id)));
    }
    c.execute(
        &format!("INSERT INTO mensajes_chat ({COLUMNAS_MENSAJE}) VALUES (?1, ?2, ?3, ?4, ?5)"),
        params![m.id, m.de, m.para, m.texto, m.fecha],
    )?;
    for usuario in &m.leido_por {
        marcar_mensaje_leido(c, &m.id, usuario)?;
    }
    Ok(())
}

pub fn buscar_mensaje(c: &Connection, id: &str) -> Resultado<Option<Mensaje>> {
    let mensaje = c
        .query_row(
            &format!("SELECT {COLUMNAS_MENSAJE} FROM mensajes_chat WHERE id = ?1"),
            [id],
            mensaje_de_fila,
        )
        .optional()?;
    Ok(match mensaje {
        Some(m) => con_leidos(c, vec![m])?.pop(),
        None => None,
    })
}

fn marcar_mensaje_leido(c: &Connection, mensaje_id: &str, usuario: &str) -> Resultado<()> {
    c.execute(
        "INSERT OR IGNORE INTO chat_leidos (mensaje_id, usuario) VALUES (?1, ?2)",
        params![mensaje_id, usuario],
    )?;
    Ok(())
}

/// Mensajes de una conversación tal como los ve `yo`: en un privado, los que
/// van en cualquiera de los dos sentidos; en un grupo, todos los del grupo.
/// Si `yo` eliminó la conversación, solo los posteriores a ese momento.
pub fn conversacion(c: &Connection, yo: &str, conversacion: &str) -> Resultado<Vec<Mensaje>> {
    let desde = eliminada_desde(c, yo, conversacion)?.unwrap_or_default();
    let mut consulta = c.prepare(&format!(
        "SELECT {COLUMNAS_MENSAJE} FROM mensajes_chat
         WHERE ((para = ?2 AND (?2 NOT LIKE '%@%' OR de = ?1)) OR (para = ?1 AND de = ?2))
           AND fecha > ?3
         ORDER BY fecha, rowid"
    ))?;
    let mensajes = consulta
        .query_map(params![yo, conversacion, desde], mensaje_de_fila)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    con_leidos(c, mensajes)
}

/// Marca como leídos por `yo` todos los mensajes de la conversación. Devuelve cuántos cambió.
pub fn marcar_conversacion_leida(c: &Connection, yo: &str, conv: &str) -> Resultado<usize> {
    let mut cambiados = 0;
    for m in conversacion(c, yo, conv)? {
        if !m.leido_por.iter().any(|u| u == yo) {
            marcar_mensaje_leido(c, &m.id, yo)?;
            cambiados += 1;
        }
    }
    Ok(cambiados)
}

/// `yo` deja de ver la conversación desde `fecha`; si llega un mensaje nuevo
/// vuelve a aparecer solo con ese.
pub fn eliminar_conversacion(c: &Connection, yo: &str, conv: &str, fecha: &str) -> Resultado<()> {
    c.execute(
        "INSERT INTO chats_eliminados (usuario, conversacion, desde) VALUES (?1, ?2, ?3)
         ON CONFLICT (usuario, conversacion) DO UPDATE SET desde = excluded.desde",
        params![yo, conv, fecha],
    )?;
    Ok(())
}

pub fn eliminada_desde(c: &Connection, yo: &str, conv: &str) -> Resultado<Option<String>> {
    Ok(c.query_row(
        "SELECT desde FROM chats_eliminados WHERE usuario = ?1 AND conversacion = ?2",
        params![yo, conv],
        |f| f.get(0),
    )
    .optional()?)
}

/// Conversaciones de `yo` con algo sin leer (lo que cuenta el menú), sin repetir.
pub fn conversaciones_sin_leer(c: &Connection, yo: &str) -> Resultado<Vec<String>> {
    let mut consulta = c.prepare(
        "SELECT DISTINCT CASE WHEN m.para = ?1 THEN m.de ELSE m.para END AS conv
         FROM mensajes_chat m
         WHERE m.de != ?1
           AND (m.para = ?1 OR m.para NOT LIKE '%@%')
           AND NOT EXISTS (SELECT 1 FROM chat_leidos l WHERE l.mensaje_id = m.id AND l.usuario = ?1)
           AND m.fecha > COALESCE((SELECT e.desde FROM chats_eliminados e
                                    WHERE e.usuario = ?1
                                      AND e.conversacion = CASE WHEN m.para = ?1 THEN m.de ELSE m.para END), '')
         ORDER BY conv",
    )?;
    let convs = consulta
        .query_map([yo], |f| f.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut visibles = Vec::new();
    for conv in convs {
        let es_grupo = !conv.contains('@');
        if !es_grupo
            || conv == GRUPO_GENERAL
            || buscar_grupo(c, &conv)?.is_some_and(|g| g.miembros.iter().any(|m| m == yo))
        {
            visibles.push(conv);
        }
    }
    Ok(visibles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    const ANA: &str = "ana@virustf.dev";
    const ZOE: &str = "zoe@virustf.dev";
    const LEO: &str = "leo@virustf.dev";

    fn mensaje(id: &str, de: &str, para: &str, fecha: &str) -> Mensaje {
        Mensaje {
            id: id.into(),
            de: de.into(),
            para: para.into(),
            texto: "Hola".into(),
            fecha: format!("2026-11-{fecha}T10:00:00Z"),
            leido_por: vec![de.into()],
        }
    }

    fn grupo(id: &str, miembros: &[&str]) -> GrupoChat {
        GrupoChat {
            id: id.into(),
            nombre: "Arte".into(),
            miembros: miembros.iter().map(|m| m.to_string()).collect(),
            creado_por: ANA.into(),
            fecha: "2026-11-09".into(),
        }
    }

    #[test]
    fn privado_se_ve_igual_desde_los_dos_lados_y_no_desde_un_tercero() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        guardar_mensaje(c, &mensaje("M-1", ANA, ZOE, "09")).unwrap();
        guardar_mensaje(c, &mensaje("M-2", ZOE, ANA, "10")).unwrap();
        guardar_mensaje(c, &mensaje("M-3", ANA, LEO, "11")).unwrap();

        let ids = |yo, conv| -> Vec<String> {
            conversacion(c, yo, conv)
                .unwrap()
                .into_iter()
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(ids(ANA, ZOE), ["M-1", "M-2"]);
        assert_eq!(ids(ZOE, ANA), ["M-1", "M-2"]);
        assert!(ids(LEO, ZOE).is_empty());
        assert_eq!(ids(LEO, ANA), ["M-3"]);
        assert_eq!(
            buscar_mensaje(c, "M-1").unwrap().unwrap().leido_por,
            vec![ANA.to_string()]
        );
        assert!(guardar_mensaje(c, &mensaje("M-1", ANA, ZOE, "12")).is_err());
    }

    #[test]
    fn leidos_y_contador_de_conversaciones() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        crear_grupo(c, &grupo("G-1", &[ANA, ZOE])).unwrap();
        guardar_mensaje(c, &mensaje("M-1", ANA, ZOE, "09")).unwrap();
        guardar_mensaje(c, &mensaje("M-2", ANA, ZOE, "10")).unwrap();
        guardar_mensaje(c, &mensaje("M-3", ANA, GRUPO_GENERAL, "11")).unwrap();
        guardar_mensaje(c, &mensaje("M-4", ANA, "G-1", "12")).unwrap();

        assert!(conversaciones_sin_leer(c, ANA).unwrap().is_empty());
        assert_eq!(
            conversaciones_sin_leer(c, ZOE).unwrap(),
            ["G-1", ANA, GRUPO_GENERAL]
        );
        assert_eq!(conversaciones_sin_leer(c, LEO).unwrap(), [GRUPO_GENERAL]);

        assert_eq!(marcar_conversacion_leida(c, ZOE, ANA).unwrap(), 2);
        assert_eq!(marcar_conversacion_leida(c, ZOE, ANA).unwrap(), 0);
        assert_eq!(
            conversaciones_sin_leer(c, ZOE).unwrap(),
            ["G-1", GRUPO_GENERAL]
        );
        let m1 = buscar_mensaje(c, "M-1").unwrap().unwrap();
        assert_eq!(m1.leido_por, vec![ANA.to_string(), ZOE.to_string()]);
        assert_eq!(serde_json::to_value(&m1).unwrap()["leidoPor"][1], ZOE);
    }

    #[test]
    fn eliminar_oculta_lo_anterior_y_lo_nuevo_la_revive() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        guardar_mensaje(c, &mensaje("M-1", ANA, ZOE, "09")).unwrap();
        eliminar_conversacion(c, ZOE, ANA, "2026-11-10T00:00:00Z").unwrap();
        assert!(conversacion(c, ZOE, ANA).unwrap().is_empty());
        assert!(conversaciones_sin_leer(c, ZOE).unwrap().is_empty());
        assert_eq!(conversacion(c, ANA, ZOE).unwrap().len(), 1);

        guardar_mensaje(c, &mensaje("M-2", ANA, ZOE, "11")).unwrap();
        let visibles = conversacion(c, ZOE, ANA).unwrap();
        assert_eq!(visibles.len(), 1);
        assert_eq!(visibles[0].id, "M-2");
        assert_eq!(conversaciones_sin_leer(c, ZOE).unwrap(), [ANA]);

        eliminar_conversacion(c, ZOE, ANA, "2026-11-12T00:00:00Z").unwrap();
        assert_eq!(
            eliminada_desde(c, ZOE, ANA).unwrap().as_deref(),
            Some("2026-11-12T00:00:00Z")
        );
        assert!(conversacion(c, ZOE, ANA).unwrap().is_empty());
    }

    #[test]
    fn grupos_validaciones_y_membresia() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        crear_grupo(c, &grupo("G-1", &[ANA, ZOE])).unwrap();
        assert!(crear_grupo(c, &grupo("G-1", &[ANA])).is_err());
        assert!(crear_grupo(c, &grupo(GRUPO_GENERAL, &[ANA])).is_err());
        assert!(crear_grupo(c, &grupo("G-2", &[ZOE])).is_err());
        let mut sin_nombre = grupo("G-3", &[ANA]);
        sin_nombre.nombre = " ".into();
        assert!(crear_grupo(c, &sin_nombre).is_err());

        assert_eq!(grupos_de(c, ZOE).unwrap(), vec![grupo("G-1", &[ANA, ZOE])]);
        assert!(grupos_de(c, LEO).unwrap().is_empty());
        assert_eq!(buscar_grupo(c, "G-1").unwrap().unwrap().miembros.len(), 2);
    }
}
