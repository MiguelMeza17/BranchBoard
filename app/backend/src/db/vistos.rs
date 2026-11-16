//! Tabla `paneles_vistos` (local por usuario): la huella de los datos de cada
//! panel la última vez que la persona lo abrió. Si la huella actual es otra, el
//! menú enciende su punto de aviso.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Resultado;

/// Guarda la huella que la persona acaba de ver en un panel.
pub fn marcar_visto(c: &Connection, usuario: &str, panel: &str, huella: &str) -> Resultado<()> {
    c.execute(
        "INSERT INTO paneles_vistos (usuario, panel, huella) VALUES (?1, ?2, ?3)
         ON CONFLICT (usuario, panel) DO UPDATE SET huella = excluded.huella",
        params![usuario, panel, huella],
    )?;
    Ok(())
}

/// Huella vista por última vez; `None` si nunca entró al panel.
pub fn huella_vista(c: &Connection, usuario: &str, panel: &str) -> Resultado<Option<String>> {
    Ok(c.query_row(
        "SELECT huella FROM paneles_vistos WHERE usuario = ?1 AND panel = ?2",
        params![usuario, panel],
        |f| f.get(0),
    )
    .optional()?)
}

/// Todas las huellas guardadas de `usuario`: `{ panel -> huella }`.
pub fn listar_vistos(c: &Connection, usuario: &str) -> Resultado<BTreeMap<String, String>> {
    let mut consulta =
        c.prepare("SELECT panel, huella FROM paneles_vistos WHERE usuario = ?1 ORDER BY panel")?;
    let filas = consulta.query_map([usuario], |f| {
        Ok((f.get::<_, String>(0)?, f.get::<_, String>(1)?))
    })?;
    let mut mapa = BTreeMap::new();
    for fila in filas {
        let (panel, huella) = fila?;
        mapa.insert(panel, huella);
    }
    Ok(mapa)
}

/// `true` si el panel tiene algo nuevo para la persona (la huella cambió o nunca entró).
pub fn hay_novedades(
    c: &Connection,
    usuario: &str,
    panel: &str,
    huella_actual: &str,
) -> Resultado<bool> {
    Ok(huella_vista(c, usuario, panel)?.as_deref() != Some(huella_actual))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn la_huella_se_guarda_por_usuario_y_panel() {
        let db = Db::en_memoria().unwrap();
        let c = db.conexion();
        assert!(hay_novedades(c, "ana@virustf.dev", "usuarios", "h1").unwrap());
        marcar_visto(c, "ana@virustf.dev", "usuarios", "h1").unwrap();
        assert!(!hay_novedades(c, "ana@virustf.dev", "usuarios", "h1").unwrap());
        assert!(hay_novedades(c, "ana@virustf.dev", "usuarios", "h2").unwrap());
        assert!(hay_novedades(c, "ana@virustf.dev", "scrum", "h1").unwrap());
        assert!(hay_novedades(c, "zoe@virustf.dev", "usuarios", "h1").unwrap());

        marcar_visto(c, "ana@virustf.dev", "usuarios", "h2").unwrap();
        assert_eq!(
            huella_vista(c, "ana@virustf.dev", "usuarios")
                .unwrap()
                .as_deref(),
            Some("h2")
        );
        assert_eq!(
            huella_vista(c, "zoe@virustf.dev", "usuarios").unwrap(),
            None
        );
        let de_ana = listar_vistos(c, "ana@virustf.dev").unwrap();
        assert_eq!(de_ana.get("usuarios").map(String::as_str), Some("h2"));
        assert!(listar_vistos(c, "zoe@virustf.dev").unwrap().is_empty());
    }
}
