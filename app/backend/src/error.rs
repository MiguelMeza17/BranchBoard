//! Error común del backend. Llega al frontend como `{ codigo, mensaje }`.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Resultado de cualquier operación del backend.
pub type Resultado<T> = Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// El rol de quien llama no puede hacer esta acción.
    #[error("No tienes permiso para hacer esto")]
    SinPermiso,
    /// Lo que se pidió no existe (tarea, usuario, sprint…).
    #[error("No se encontró {0}")]
    NoEncontrado(String),
    /// Los datos no cumplen una regla de negocio. El texto es el mensaje para la persona.
    #[error("{0}")]
    Validacion(String),
    /// `cm` (Unity Version Control) no está instalado en la máquina.
    #[error("Unity Version Control (cm) no está instalado")]
    CmNoInstalado,
    /// No hay sesión de Unity VC iniciada (`cm whoami` falla).
    #[error("No se detectó una sesión de Unity VC")]
    SinSesionUnityVc,
    /// `cm` respondió con error.
    #[error("Unity Version Control respondió con un error: {0}")]
    Cm(String),
    /// Falló SQLite.
    #[error("Error de la base de datos: {0}")]
    BaseDatos(String),
    /// Falló leer o escribir un archivo.
    #[error("Error de archivos: {0}")]
    Archivos(String),
    /// La llave de firma está cifrada: hay que pedirle su contraseña a la persona.
    #[error("La llave de firma está cifrada: escribe su contraseña")]
    LlaveCifrada,
    /// Falló buscar, descargar o instalar una versión nueva.
    #[error("No se pudo actualizar: {0}")]
    Actualizacion(String),
    /// Cualquier otro fallo inesperado.
    #[error("Error interno: {0}")]
    Interno(String),
}

impl Error {
    /// Código estable que el frontend usa para decidir qué mostrar.
    pub fn codigo(&self) -> &'static str {
        match self {
            Error::SinPermiso => "sin_permiso",
            Error::NoEncontrado(_) => "no_encontrado",
            Error::Validacion(_) => "validacion",
            Error::CmNoInstalado => "cm_no_instalado",
            Error::SinSesionUnityVc => "sin_sesion_unity_vc",
            Error::Cm(_) => "cm",
            Error::BaseDatos(_) => "base_datos",
            Error::Archivos(_) => "archivos",
            Error::LlaveCifrada => "llave_cifrada",
            Error::Actualizacion(_) => "actualizacion",
            Error::Interno(_) => "interno",
        }
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut e = s.serialize_struct("Error", 2)?;
        e.serialize_field("codigo", self.codigo())?;
        e.serialize_field("mensaje", &self.to_string())?;
        e.end()
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Archivos(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_serializa_con_codigo_y_mensaje() {
        let json = serde_json::to_value(Error::NoEncontrado("la tarea T-12".into())).unwrap();
        assert_eq!(json["codigo"], "no_encontrado");
        assert_eq!(json["mensaje"], "No se encontró la tarea T-12");
    }

    #[test]
    fn validacion_muestra_el_texto_tal_cual() {
        let json =
            serde_json::to_value(Error::Validacion("El título es obligatorio".into())).unwrap();
        assert_eq!(json["codigo"], "validacion");
        assert_eq!(json["mensaje"], "El título es obligatorio");
    }
}
