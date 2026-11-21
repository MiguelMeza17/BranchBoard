//! A qué proyecto de Supabase habla la app (#232).
//!
//! Solo van aquí la URL y la clave **pública** (publishable) de cada proyecto: el repositorio es público y
//! esa clave por sí sola no permite nada (lo deciden las reglas RLS, #234). La clave secreta (`sb_secret_…`),
//! la contraseña de Postgres y los tokens de la CLI **nunca** van al código; una prueba de abajo falla si
//! alguien pega una clave secreta.

/// Un proyecto de Supabase: su dirección y su clave pública.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProyectoSupabase {
    /// Nombre del proyecto en el panel de Supabase.
    pub nombre: &'static str,
    /// `https://<ref>.supabase.co`
    pub url: &'static str,
    /// Clave publishable (`sb_publishable_…`).
    pub clave_publica: &'static str,
}

/// El que usa el instalador (build `release`).
pub const PRODUCCION: ProyectoSupabase = ProyectoSupabase {
    nombre: "branchboard",
    url: "https://rqistwyevplwdslyuvbk.supabase.co",
    clave_publica: "sb_publishable_DoI9B-8SOrJ94wvbRU4YzQ_oe-m542B",
};

/// El de pruebas y desarrollo (`npm run tauri dev`, pruebas reales). Nunca se prueba en producción.
pub const PRUEBAS: ProyectoSupabase = ProyectoSupabase {
    nombre: "branchboard-pruebas",
    url: "https://freppiixmuspvozaqrxj.supabase.co",
    clave_publica: "sb_publishable_FOfX79pVZQqXV5GcNNe1qQ_1EGowrlP",
};

/// Variable de entorno que fuerza el proyecto: `pruebas` o `produccion`.
pub const VARIABLE: &str = "BB_SUPABASE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entorno {
    Produccion,
    Pruebas,
}

impl Entorno {
    /// En desarrollo (`debug`) pruebas, en el instalador (`release`) producción; `BB_SUPABASE` lo fuerza.
    pub fn actual() -> Self {
        Self::desde(
            std::env::var(VARIABLE).ok().as_deref(),
            cfg!(debug_assertions),
        )
    }

    fn desde(variable: Option<&str>, es_debug: bool) -> Self {
        match variable.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("pruebas") => Entorno::Pruebas,
            Some("produccion") | Some("producción") => Entorno::Produccion,
            _ if es_debug => Entorno::Pruebas,
            _ => Entorno::Produccion,
        }
    }

    pub fn proyecto(self) -> &'static ProyectoSupabase {
        match self {
            Entorno::Produccion => &PRODUCCION,
            Entorno::Pruebas => &PRUEBAS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desarrollo_usa_pruebas_y_el_instalador_produccion() {
        assert_eq!(Entorno::desde(None, true), Entorno::Pruebas);
        assert_eq!(Entorno::desde(None, false), Entorno::Produccion);
    }

    #[test]
    fn la_variable_fuerza_el_proyecto() {
        assert_eq!(
            Entorno::desde(Some("produccion"), true),
            Entorno::Produccion
        );
        assert_eq!(Entorno::desde(Some(" PRUEBAS "), false), Entorno::Pruebas);
        // Un valor desconocido no cambia nada.
        assert_eq!(Entorno::desde(Some("otra"), true), Entorno::Pruebas);
        assert_eq!(Entorno::desde(Some("otra"), false), Entorno::Produccion);
    }

    #[test]
    fn los_proyectos_son_distintos() {
        assert_ne!(PRODUCCION.url, PRUEBAS.url);
        assert_ne!(PRODUCCION.clave_publica, PRUEBAS.clave_publica);
    }

    /// El repo es público: aquí solo puede haber claves publishable, nunca secretas.
    #[test]
    fn solo_hay_claves_publicas_y_direcciones_de_supabase() {
        for p in [PRODUCCION, PRUEBAS] {
            assert!(
                p.url.starts_with("https://") && p.url.ends_with(".supabase.co"),
                "{}",
                p.nombre
            );
            assert!(
                p.clave_publica.starts_with("sb_publishable_"),
                "{}",
                p.nombre
            );
            assert!(!p.clave_publica.contains("secret"), "{}", p.nombre);
        }
    }

    /// Contra el proyecto de pruebas de verdad: la clave pública responde, el registro por correo está
    /// activo y **no** se confirma el correo (decidido en #236: el correo incluido de Supabase solo llega
    /// al equipo de la organización). Solo lee la configuración pública de Auth; no crea nada.
    #[test]
    #[ignore = "toca el proyecto de pruebas de Supabase: correr a mano con --ignored"]
    fn entorno_real_la_clave_publica_de_pruebas_responde() {
        let respuesta = ureq::get(&format!("{}/auth/v1/settings", PRUEBAS.url))
            .set("apikey", PRUEBAS.clave_publica)
            .call()
            .expect("el proyecto de pruebas no respondió");
        let ajustes: serde_json::Value = respuesta.into_json().unwrap();
        assert_eq!(
            ajustes["external"]["email"], true,
            "registro por correo activado"
        );
        assert_eq!(
            ajustes["mailer_autoconfirm"], true,
            "la cuenta queda lista sin confirmar el correo"
        );
        assert_eq!(ajustes["disable_signup"], false);
    }
}
