//! API REST del servidor de Unity VC (issue #159). Se usa para lo que pasa en el
//! **servidor** (crear ramas, merge, listar), sin workspace; lo del workspace
//! sigue con `cm` (ver "Acceso a la API REST del servidor de Unity VC" en BACKEND.md).
//!
//! - `region` — URL base de la organización.
//! - `sanear` — toda respuesta pasa por aquí antes de usarse (SEGURIDAD.md).
//! - `cliente` — las llamadas HTTP con el PAT.
//! - `pat` — crear el PAT con `cm`, guardarlo en el llavero y renovarlo en 401.

pub mod cliente;
pub mod pat;
pub mod region;
pub mod sanear;

pub use cliente::{Changeset, ClienteServidor, Rama, Repositorio};
pub use pat::{conectar, LlaveroSistema};

/// `MiguelM17@unity`, `MiguelM17@cloud` o `MiguelM17` → `MiguelM17`.
pub fn nombre_organizacion(especificacion: &str) -> &str {
    let texto = especificacion.trim();
    texto.split_once('@').map_or(texto, |(org, _)| org).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_organizacion_sin_sufijo() {
        assert_eq!(nombre_organizacion("MiguelM17@unity"), "MiguelM17");
        assert_eq!(nombre_organizacion(" MiguelM17@cloud "), "MiguelM17");
        assert_eq!(nombre_organizacion("MiguelM17"), "MiguelM17");
    }
}
