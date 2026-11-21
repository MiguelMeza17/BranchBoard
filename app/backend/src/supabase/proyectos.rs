//! Proyectos en el servidor de Supabase (#240): vincular el proyecto de Unity VC abierto con su fila de
//! `proyectos` y listar los proyectos de los que la cuenta es miembro.
//!
//! - **Vincular** (al entrar a un proyecto de Unity VC): si la base local todavía no tiene
//!   `proyecto_servidor_id`, se busca el proyecto por `unity_org`/`unity_repo`. Las reglas RLS solo lo
//!   muestran si la cuenta es miembro, así que si aparece, la cuenta ya está en el equipo del servidor.
//! - Si no aparece y quien entra es el Administrador local, se pide a la Edge Function
//!   `vincular-repo-unity` (#237). Ella comprueba con Unity VC que es el **dueño del repo**: si lo es, crea
//!   el proyecto y lo deja como Administrador; si el repo ya estaba (lo trajo otra persona), devuelve el
//!   mismo proyecto o `409`. Así un repo es un solo proyecto aunque lo traigan dos personas.
//! - El id queda en `configuracion_proyecto` (`proyecto_servidor_id`): desde ahí se encola en la bandeja
//!   (#238), se recibe del servidor (#239) y se usa Storage (#242).
//! - Sin internet o sin sesión no se vincula; se reintenta la próxima vez que se entre.
//! - **No cubre** (llega con #241): entrar por primera vez en una PC como miembro que no es el dueño; hoy
//!   esa persona necesita los datos locales firmados por el Administrador.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::configuracion::{self, CLAVE_PROYECTO_SERVIDOR};
use crate::error::{Error, Resultado};

/// Un trabajo sobre la base que [`vincular_unity`] le pide a quien la presta.
pub type TrabajoDb<'f> = &'f mut dyn FnMut(&Connection) -> Resultado<()>;

/// Cómo quedó el proyecto abierto respecto al servidor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "estado", rename_all = "snake_case")]
pub enum Vinculo {
    /// Vinculado con `proyecto_id`. `ahora` = se vinculó en esta llamada.
    Vinculado { proyecto_id: String, ahora: bool },
    /// El servidor no muestra el proyecto a esta cuenta y no es quien puede traerlo.
    NoEsMiembro,
    /// La base local tiene equipo (más de un usuario): no se vincula aquí, porque con #241 la copia
    /// local de `miembros` dejaría afuera a quien no esté en el servidor. Se sube con #245.
    ConEquipoLocal,
}

/// Un proyecto del que la cuenta es miembro, con su rol en el servidor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProyectoDeLaCuenta {
    pub id: String,
    pub nombre: String,
    pub unity_org: Option<String>,
    pub unity_repo: Option<String>,
    pub rol: String,
}

/// Codifica un valor para la URL de PostgREST (`Proyecto/Repo` → `Proyecto%2FRepo`).
fn codificar(texto: &str) -> String {
    texto
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Ruta para buscar el proyecto de un repo de Unity VC (solo aparece si la cuenta es miembro).
pub fn ruta_buscar_unity(org: &str, repo: &str) -> String {
    format!(
        "/rest/v1/proyectos?select=id&unity_org=eq.{}&unity_repo=eq.{}",
        codificar(org),
        codificar(repo)
    )
}

/// Ruta para listar los proyectos de la cuenta `usuario_id` con su rol.
pub fn ruta_mis_proyectos(usuario_id: &str) -> String {
    format!(
        "/rest/v1/miembros?select=rol,proyectos(id,nombre,unity_org,unity_repo)&usuario_id=eq.{}",
        codificar(usuario_id)
    )
}

/// El id de la respuesta de [`ruta_buscar_unity`], si vino alguno.
pub fn id_desde_json(json: &Value) -> Option<String> {
    json.as_array()?
        .first()?
        .get("id")?
        .as_str()
        .map(String::from)
}

/// Convierte la respuesta de [`ruta_mis_proyectos`] en la lista, ordenada por nombre.
pub fn proyectos_desde_json(json: Value) -> Resultado<Vec<ProyectoDeLaCuenta>> {
    #[derive(Deserialize)]
    struct Fila {
        rol: String,
        proyectos: Option<Datos>,
    }
    #[derive(Deserialize)]
    struct Datos {
        id: String,
        nombre: String,
        unity_org: Option<String>,
        unity_repo: Option<String>,
    }
    let filas: Vec<Fila> = serde_json::from_value(json).map_err(|e| {
        Error::Interno(format!(
            "respuesta de proyectos del servidor no válida: {e}"
        ))
    })?;
    let mut lista: Vec<ProyectoDeLaCuenta> = filas
        .into_iter()
        .filter_map(|f| {
            f.proyectos.map(|p| ProyectoDeLaCuenta {
                id: p.id,
                nombre: p.nombre,
                unity_org: p.unity_org,
                unity_repo: p.unity_repo,
                rol: f.rol,
            })
        })
        .collect();
    lista.sort_by_key(|p| p.nombre.to_lowercase());
    Ok(lista)
}

/// Los proyectos de los que es miembro la cuenta de la sesión de `cliente`, con su rol.
pub fn listar_mios<L: crate::servidor::pat::Llavero>(
    cliente: &super::ClienteSupabase<L>,
) -> Resultado<Vec<ProyectoDeLaCuenta>> {
    let sesion = cliente.sesion_vigente()?;
    cliente
        .llamar("GET", &ruta_mis_proyectos(&sesion.usuario_id), None)
        .and_then(proyectos_desde_json)
}

/// El `proyecto_servidor_id` guardado en la base, si hay.
pub fn proyecto_guardado(c: &Connection) -> Resultado<Option<String>> {
    Ok(configuracion::leer(c, CLAVE_PROYECTO_SERVIDOR)?
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty()))
}

/// Vincula la base abierta con el proyecto del servidor (ver el inicio del módulo).
///
/// - `buscar()` pregunta al servidor por el proyecto del repo (`None` si no lo muestra).
/// - `traer()` llama a `vincular-repo-unity`; solo se usa si `puede_traerlo` (Administrador local).
/// - `equipo_local`: cuántos usuarios tiene la base local; con más de uno no se vincula
///   ([`Vinculo::ConEquipoLocal`]).
/// - `email`/`hoy` quedan en la configuración como quién y cuándo se vinculó.
///
/// La base solo se presta para leer y guardar, nunca durante la red.
pub fn vincular_unity(
    con_db: impl Fn(TrabajoDb<'_>) -> Resultado<()>,
    equipo_local: usize,
    puede_traerlo: bool,
    email: &str,
    hoy: &str,
    buscar: impl FnOnce() -> Resultado<Option<String>>,
    traer: impl FnOnce() -> Resultado<String>,
) -> Resultado<Vinculo> {
    let mut guardado = None;
    con_db(&mut |c| {
        guardado = proyecto_guardado(c)?;
        Ok(())
    })?;
    if let Some(proyecto_id) = guardado {
        return Ok(Vinculo::Vinculado {
            proyecto_id,
            ahora: false,
        });
    }
    if equipo_local > 1 {
        return Ok(Vinculo::ConEquipoLocal);
    }
    let proyecto_id = match buscar()? {
        Some(id) => id,
        None if puede_traerlo => traer()?,
        None => return Ok(Vinculo::NoEsMiembro),
    };
    con_db(&mut |c| configuracion::guardar(c, CLAVE_PROYECTO_SERVIDOR, &proyecto_id, email, hoy))?;
    Ok(Vinculo::Vinculado {
        proyecto_id,
        ahora: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const ID: &str = "11111111-2222-4333-8444-555555555555";

    fn base() -> crate::db::Db {
        crate::db::Db::en_memoria().unwrap()
    }

    fn en<'a>(c: &'a Connection) -> impl Fn(TrabajoDb<'_>) -> Resultado<()> + 'a {
        move |f| f(c)
    }

    #[test]
    fn si_el_servidor_lo_muestra_se_guarda_sin_traerlo() {
        let db = base();
        let c = db.conexion();
        let v = vincular_unity(
            en(c),
            1,
            true,
            "ana@virustf.dev",
            "2026-11-21",
            || Ok(Some(ID.into())),
            || panic!("no hace falta traerlo"),
        )
        .unwrap();
        assert_eq!(
            v,
            Vinculo::Vinculado {
                proyecto_id: ID.into(),
                ahora: true
            }
        );
        assert_eq!(proyecto_guardado(c).unwrap().as_deref(), Some(ID));
    }

    #[test]
    fn el_administrador_lo_trae_si_el_servidor_no_lo_tiene() {
        let db = base();
        let c = db.conexion();
        let v = vincular_unity(
            en(c),
            1,
            true,
            "ana@virustf.dev",
            "2026-11-21",
            || Ok(None),
            || Ok(ID.into()),
        )
        .unwrap();
        assert!(matches!(v, Vinculo::Vinculado { ahora: true, .. }));
        assert_eq!(proyecto_guardado(c).unwrap().as_deref(), Some(ID));
    }

    #[test]
    fn quien_no_es_administrador_no_lo_trae() {
        let db = base();
        let c = db.conexion();
        let v = vincular_unity(
            en(c),
            1,
            false,
            "beto@virustf.dev",
            "2026-11-21",
            || Ok(None),
            || panic!("solo el Administrador lo trae"),
        )
        .unwrap();
        assert_eq!(v, Vinculo::NoEsMiembro);
        assert_eq!(proyecto_guardado(c).unwrap(), None);
    }

    #[test]
    fn ya_vinculado_no_toca_la_red() {
        let db = base();
        let c = db.conexion();
        configuracion::guardar(
            c,
            CLAVE_PROYECTO_SERVIDOR,
            ID,
            "ana@virustf.dev",
            "2026-11-21",
        )
        .unwrap();
        let llamadas = Cell::new(0);
        let v = vincular_unity(
            en(c),
            1,
            true,
            "ana@virustf.dev",
            "2026-11-21",
            || {
                llamadas.set(llamadas.get() + 1);
                Ok(None)
            },
            || Ok("otro".into()),
        )
        .unwrap();
        assert_eq!(
            v,
            Vinculo::Vinculado {
                proyecto_id: ID.into(),
                ahora: false
            }
        );
        assert_eq!(llamadas.get(), 0);
    }

    #[test]
    fn sin_internet_no_se_guarda_nada() {
        let db = base();
        let c = db.conexion();
        let e = vincular_unity(
            en(c),
            1,
            true,
            "ana@virustf.dev",
            "2026-11-21",
            || Err(Error::ServidorSinConexion),
            || Ok(ID.into()),
        )
        .unwrap_err();
        assert!(matches!(e, Error::ServidorSinConexion));
        assert_eq!(proyecto_guardado(c).unwrap(), None);
    }

    #[test]
    fn con_equipo_local_no_se_vincula_aunque_el_servidor_lo_tenga() {
        let db = base();
        let c = db.conexion();
        let v = vincular_unity(
            en(c),
            3,
            true,
            "ana@virustf.dev",
            "2026-11-21",
            || panic!("no debe buscar"),
            || panic!("no debe traer"),
        )
        .unwrap();
        assert_eq!(v, Vinculo::ConEquipoLocal);
        assert_eq!(proyecto_guardado(c).unwrap(), None);
    }

    #[test]
    fn rutas_y_respuestas() {
        assert_eq!(
            ruta_buscar_unity("MiguelM17", "BranchBoard-Pruebas/BranchBoard-Pruebas"),
            "/rest/v1/proyectos?select=id&unity_org=eq.MiguelM17&unity_repo=eq.BranchBoard-Pruebas%2FBranchBoard-Pruebas"
        );
        assert_eq!(
            id_desde_json(&serde_json::json!([{ "id": ID }])).as_deref(),
            Some(ID)
        );
        assert_eq!(id_desde_json(&serde_json::json!([])), None);
        let lista = proyectos_desde_json(serde_json::json!([
            { "rol": "Desarrollador", "proyectos": { "id": "b", "nombre": "zeta", "unity_org": null, "unity_repo": null } },
            { "rol": "Administrador", "proyectos": { "id": "a", "nombre": "Alfa", "unity_org": "Org", "unity_repo": "P/R" } },
            { "rol": "Tester", "proyectos": null }
        ]))
        .unwrap();
        assert_eq!(
            lista
                .iter()
                .map(|p| (p.nombre.as_str(), p.rol.as_str()))
                .collect::<Vec<_>>(),
            vec![("Alfa", "Administrador"), ("zeta", "Desarrollador")]
        );
    }
}
