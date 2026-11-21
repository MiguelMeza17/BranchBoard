//! Alta local durable: el UUID y la autoría no cambian al recuperar la conexión (#269).
use std::path::Path;

use rand::RngCore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{configuracion, usuarios, Db};
use crate::error::{Error, Resultado};
use crate::modelo::{hoy_local, Rol};
use crate::servidor::pat::Llavero;
use crate::supabase::cliente::Sesion;
use crate::supabase::{proyectos::ProyectoDeLaCuenta, ClienteSupabase};

const CLAVE: &str = "alta_proyecto_pendiente";
const ORIGEN: &str = "alta_proyecto_local";

#[derive(Serialize, Deserialize)]
pub struct Alta {
    pub id: String,
    pub nombre: String,
    pub usuario_id: String,
    pub correo: String,
}

pub fn leer(c: &Connection) -> Resultado<Option<Alta>> {
    configuracion::leer(c, CLAVE)?
        .map(|s| serde_json::from_str(&s).map_err(|e| Error::Interno(e.to_string())))
        .transpose()
}

/// No consulta la red ni necesita renovar el token: basta la sesión guardada.
pub fn crear(datos: &Path, nombre: &str, sesion: &Sesion) -> Resultado<Db> {
    let mut bytes = [0_u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let id = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    let carpeta = super::carpeta_datos(datos, &id)?;
    std::fs::create_dir_all(
        carpeta
            .parent()
            .ok_or_else(|| Error::Interno("Falta la carpeta de proyectos".into()))?,
    )?;
    // Una colisión nunca debe abrir ni sobrescribir otro proyecto.
    std::fs::create_dir(&carpeta)?;
    let db = Db::abrir(&carpeta)?;
    let c = db.conexion();
    let tx = c.unchecked_transaction()?;
    let hoy = hoy_local();
    configuracion::guardar(
        &tx,
        configuracion::CLAVE_PROYECTO_SERVIDOR,
        &id,
        &sesion.correo,
        &hoy,
    )?;
    configuracion::guardar(
        &tx,
        configuracion::CLAVE_NOMBRE_PROYECTO,
        nombre,
        &sesion.correo,
        &hoy,
    )?;
    let alta = Alta {
        id,
        nombre: nombre.into(),
        usuario_id: sesion.usuario_id.clone(),
        correo: sesion.correo.clone(),
    };
    configuracion::guardar(
        &tx,
        CLAVE,
        &serde_json::to_string(&alta).map_err(|e| Error::Interno(e.to_string()))?,
        &sesion.correo,
        &hoy,
    )?;
    usuarios::insertar(
        &tx,
        &usuarios::Usuario {
            email: crate::modelo::normalizar_email(&sesion.correo),
            nombre: sesion.nombre.clone(),
            apellido: sesion.apellido.clone(),
            rol: Rol::Administrador,
            fecha_alta: hoy,
        },
    )?;
    tx.commit()?;
    crate::eventos::fijar_carpeta_compartida_local(
        &carpeta,
        &carpeta.join(super::CARPETA_EVENTOS_LOCALES),
    )?;
    Ok(db)
}

/// Se confirma antes de enviar eventos. Un error deja tanto el alta como las tareas intactas.
pub fn subir<L: Llavero>(
    mut db: impl FnMut(crate::eventos::bandeja::TrabajoDb<'_>) -> Resultado<()>,
    cliente: &ClienteSupabase<L>,
) -> Resultado<()> {
    let mut alta = None;
    db(&mut |c| {
        alta = leer(c)?;
        Ok(())
    })?;
    let Some(alta) = alta else {
        return Ok(());
    };
    let sesion = cliente.sesion_guardada()?.ok_or(Error::SinSesionServidor)?;
    if sesion.usuario_id != alta.usuario_id {
        return Err(Error::Validacion(
            "Inicia sesión con la cuenta que creó este proyecto para subirlo.".into(),
        ));
    }
    let respuesta = cliente.llamar_como(
        &alta.usuario_id,
        "POST",
        "/rest/v1/rpc/crear_proyecto",
        Some(&serde_json::json!({ "p_nombre": alta.nombre, "p_id": alta.id })),
    )?;
    if respuesta.as_str() != Some(alta.id.as_str()) {
        return Err(Error::Interno(
            "El servidor no confirmó el identificador del proyecto".into(),
        ));
    }
    db(&mut |c| {
        if leer(c)?
            .is_some_and(|actual| actual.id == alta.id && actual.usuario_id == alta.usuario_id)
        {
            let tx = c.unchecked_transaction()?;
            configuracion::guardar(
                &tx,
                ORIGEN,
                &serde_json::to_string(&alta).map_err(|e| Error::Interno(e.to_string()))?,
                &alta.correo,
                &hoy_local(),
            )?;
            tx.execute(
                "DELETE FROM configuracion_proyecto WHERE clave = ?1",
                [CLAVE],
            )?;
            tx.commit()?;
        }
        Ok(())
    })
}

/// Recupera las altas de esta cuenta después de reiniciar, aunque el servidor siga sin conexión.
pub fn listar(datos: &Path, usuario: &str) -> Resultado<Vec<ProyectoDeLaCuenta>> {
    let carpeta = datos.join("proyectos");
    if !carpeta.is_dir() {
        return Ok(Vec::new());
    }
    let mut resultado = Vec::new();
    for entrada in std::fs::read_dir(carpeta)? {
        let ruta = entrada?.path().join(crate::db::NOMBRE_DB);
        if !ruta.is_file() {
            continue;
        }
        let c = Connection::open_with_flags(ruta, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let origen = configuracion::leer(&c, CLAVE)?.or(configuracion::leer(&c, ORIGEN)?);
        let alta: Option<Alta> = origen
            .map(|s| serde_json::from_str(&s).map_err(|e| Error::Interno(e.to_string())))
            .transpose()?;
        if let Some(alta) = alta.filter(|a| a.usuario_id == usuario) {
            let Some(miembro) = usuarios::buscar(&c, &alta.correo)? else {
                continue;
            };
            resultado.push(ProyectoDeLaCuenta {
                id: alta.id,
                nombre: configuracion::leer(&c, configuracion::CLAVE_NOMBRE_PROYECTO)?
                    .unwrap_or(alta.nombre),
                unity_org: None,
                unity_repo: None,
                rol: miembro.rol.to_string(),
            });
        }
    }
    Ok(resultado)
}
