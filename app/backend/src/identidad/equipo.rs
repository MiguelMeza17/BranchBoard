//! Gestión del equipo desde la pantalla Usuarios (issues #90 y #93). Ver
//! "Comandos de usuarios y ajustes" e "Integridad de los eventos" en BACKEND.md.
//!
//! Toda fila que se crea o cambia nace **firmada por quien la escribe** (el
//! Administrador, o un Líder certificado). Antes de guardar se comprueba que la
//! fila quede verificada; si no (ej. el Líder todavía no tiene certificado), no
//! se guarda nada.
//!
//! Desde el issue #93 cada cambio viaja como **evento administrativo firmado**
//! por `ops/`:
//! - En la máquina de quien lo hace se firma la fila, se arma el evento
//!   (`crear_usuario`, `cambiar_rol`, `confirmar_usuario`, `eliminar_usuario`)
//!   con `firma_admin` y se publica con `eventos::publicar`, que lo aplica aquí
//!   mismo con [`aplicar_evento`] dentro de la misma transacción.
//! - Las demás máquinas lo reciben con `eventos::sincronizar`: comprueban la
//!   `firma_admin` ([`firmante_del_evento`]: la llave del Administrador, o la de
//!   un Líder con certificado vigente) y aplican **las mismas reglas** con
//!   [`aplicar_evento`]. Si la firma falta o no coincide, el evento se descarta
//!   sin aplicar; si ya no cumple las reglas, también.
//! - La llave privada nunca viaja: las filas llegan **ya firmadas** dentro del
//!   `payload`, y la firma de cada fila se verifica igual que si se hubiera
//!   escrito en esta máquina.

use std::path::Path;

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::db::usuarios::{self, nombre_desde_email, Usuario};
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::identidad::firmas::{self, verificar_usuarios, Verificacion, Verificador};
use crate::identidad::llaves::LlavePrivada;
use crate::modelo::{normalizar_email, Rol};

/// Tipos de evento de usuarios (todos firmados, ver `eventos::TIPOS_FIRMADOS`).
pub const TIPOS: &[&str] = &[
    "crear_usuario",
    "cambiar_rol",
    "confirmar_usuario",
    "eliminar_usuario",
];

/// Roles que un Líder puede asignar.
const ROLES_BASE: &[Rol] = &[Rol::Desarrollador, Rol::Tester, Rol::ColaboradorExterno];
/// Roles que el Administrador puede asignar (nunca otro Administrador).
const ROLES_ADMIN: &[Rol] = &[
    Rol::Lider,
    Rol::Desarrollador,
    Rol::Tester,
    Rol::ColaboradorExterno,
];

/// Quien firma un cambio: su correo, su rol verificado y su llave privada.
pub struct Firmante<'a> {
    pub email: &'a str,
    pub rol: Rol,
    pub llave: &'a LlavePrivada,
}

/// Quién emitió un evento administrativo: su correo y el rol con el que firmó
/// (el que dice su llave, no el que él declare).
#[derive(Debug, Clone)]
struct Autor {
    email: String,
    rol: Rol,
}

impl From<&Firmante<'_>> for Autor {
    fn from(f: &Firmante<'_>) -> Self {
        Autor {
            email: normalizar_email(f.email),
            rol: f.rol,
        }
    }
}

/// Una fila ya firmada, tal como viaja en el `payload` de `cambiar_rol` y
/// `confirmar_usuario`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilaFirmada {
    pub email: String,
    pub rol: Rol,
    /// Firma Ed25519 de `email|rol|firmado_en` (ver `firmas`).
    pub firma: String,
    pub firmado_en: String,
    pub firmado_por: String,
}

/// `payload` de `eliminar_usuario`: a quién se elimina y las filas que el
/// Administrador volvió a firmar (las que había firmado ese Líder).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Eliminacion {
    pub email: String,
    #[serde(default)]
    pub refirmadas: Vec<FilaFirmada>,
}

/// Roles que `actor` puede darle a una persona con rol `actual` (`None` = alta
/// nueva). Vacío = no puede cambiarla. Igual que `rolesAsignablesA` del frontend.
pub fn roles_asignables(actor: Rol, actual: Option<Rol>) -> &'static [Rol] {
    match (actor, actual) {
        (Rol::Administrador, Some(Rol::Administrador)) => &[],
        (Rol::Administrador, _) => ROLES_ADMIN,
        (Rol::Lider, None) => ROLES_BASE,
        (Rol::Lider, Some(rol)) if ROLES_BASE.contains(&rol) => ROLES_BASE,
        _ => &[],
    }
}

/// `true` si `actor` puede eliminar a alguien con rol `objetivo`. Nadie elimina
/// al Administrador: es el ancla de las firmas del proyecto.
pub fn puede_eliminar(actor: Rol, objetivo: Rol) -> bool {
    match actor {
        Rol::Administrador => objetivo != Rol::Administrador,
        Rol::Lider => matches!(objetivo, Rol::Desarrollador | Rol::ColaboradorExterno),
        _ => false,
    }
}

/// Una fila de la pantalla Usuarios. Sin correo para quien no es Líder ni
/// Administrador.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsuarioVisible {
    pub email: Option<String>,
    pub nombre: String,
    pub apellido: String,
    /// El rol guardado en la fila.
    pub rol: Rol,
    /// El que se le aplica: Desarrollador si su firma no coincide, `None` si
    /// está pendiente de confirmar.
    pub rol_efectivo: Option<Rol>,
    pub verificacion: Verificacion,
    pub fecha_alta: String,
    /// Proyecto en el servidor (#241): invitado por correo, todavía sin cuenta. Solo lo ven Líder y
    /// Administrador; no tiene rol efectivo hasta que se registre.
    pub invitado: bool,
}

/// `listar_usuarios`: todos los roles; solo Líder y Administrador ven correos.
pub fn listar(
    c: &Connection,
    publica_admin: &str,
    rol_de_quien_pide: Rol,
) -> Resultado<Vec<UsuarioVisible>> {
    let ve_correos = rol_de_quien_pide.es_lider_o_admin();
    Ok(verificar_usuarios(publica_admin, usuarios::listar(c)?)
        .into_iter()
        .map(|v| UsuarioVisible {
            email: ve_correos.then(|| v.usuario.email.clone()),
            nombre: v.usuario.nombre,
            apellido: v.usuario.apellido,
            rol: v.usuario.rol,
            rol_efectivo: v.rol_efectivo,
            verificacion: v.verificacion,
            fecha_alta: v.usuario.fecha_alta,
            invitado: false,
        })
        .collect())
}

/// Firma `email|rol|firmado_en` con la llave de quien firma.
fn firmar(firmante: &Firmante<'_>, email: &str, rol: Rol, ahora: &str) -> Resultado<String> {
    match firmante.rol {
        Rol::Administrador => firmas::firmar_rol(firmante.llave, email, rol, ahora),
        Rol::Lider => firmas::firmar_rol_como_lider(firmante.llave, email, rol, ahora),
        _ => Err(Error::SinPermiso),
    }
}

/// La fila firmada que va en el evento.
fn fila_firmada(
    firmante: &Firmante<'_>,
    email: &str,
    rol: Rol,
    ahora: &str,
) -> Resultado<FilaFirmada> {
    let email = normalizar_email(email);
    Ok(FilaFirmada {
        firma: firmar(firmante, &email, rol, ahora)?,
        email,
        rol,
        firmado_en: ahora.to_string(),
        firmado_por: normalizar_email(firmante.email),
    })
}

/// Después de escribir, la fila de `email` tiene que quedar verificada; si no,
/// la llave de quien firma no es la que el proyecto reconoce.
fn exigir_verificada(c: &Connection, publica_admin: &str, email: &str) -> Resultado<()> {
    let tabla = usuarios::listar(c)?;
    let fila = tabla
        .iter()
        .find(|u| u.email == email)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    if Verificador::nuevo(publica_admin, &tabla).verificar_firma(fila) == Verificacion::Verificado {
        Ok(())
    } else {
        Err(Error::Validacion(
            "Tu llave de firma no está certificada por el Administrador de este proyecto".into(),
        ))
    }
}

/// La fila firmada tiene que ser de quien emite el evento: si no, alguien metió
/// en un evento una fila firmada por otra persona.
fn exigir_del_autor(autor: &Autor, firmado_por: Option<&str>) -> Resultado<()> {
    if firmado_por.map(normalizar_email).as_deref() == Some(autor.email.as_str()) {
        Ok(())
    } else {
        Err(Error::Validacion(
            "La fila firmada no es de quien emitió el evento".into(),
        ))
    }
}

/// La fila de `email` y el rol que cuenta para las reglas: el verificado, o el
/// guardado si está pendiente de confirmar.
fn buscar_con_rol(c: &Connection, publica_admin: &str, email: &str) -> Resultado<(Usuario, Rol)> {
    let tabla = usuarios::listar(c)?;
    let verificador = Verificador::nuevo(publica_admin, &tabla);
    let fila = tabla
        .into_iter()
        .find(|u| u.email == email)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    let v = verificador.verificar_usuario(fila);
    let rol = v.rol_efectivo.unwrap_or(v.usuario.rol);
    Ok((v.usuario, rol))
}

/// Filas que hoy valen por la firma de `email` como Líder (sin su fila ya no se
/// podrían verificar).
fn firmadas_por(c: &Connection, publica_admin: &str, email: &str) -> Resultado<Vec<Usuario>> {
    let tabla = usuarios::listar(c)?;
    let verificador = Verificador::nuevo(publica_admin, &tabla);
    Ok(tabla
        .iter()
        .filter(|u| u.email != email)
        .filter(|u| u.firmado_por.as_deref().map(normalizar_email).as_deref() == Some(email))
        .filter(|u| verificador.verificar_firma(u) == Verificacion::Verificado)
        .cloned()
        .collect())
}

/// Arma el evento `tipo` con `carga`, lo firma con la llave de quien lo emite y
/// lo publica: `aplicar` corre en la misma transacción (ver `eventos::publicar`).
fn publicar_firmado(
    c: &Connection,
    ops: &Path,
    firmante: &Firmante<'_>,
    tipo: &str,
    carga: &impl Serialize,
    aplicar: impl FnOnce(&Connection) -> Resultado<()>,
) -> Resultado<()> {
    let payload = serde_json::to_value(carga).map_err(|e| Error::Interno(e.to_string()))?;
    let evento = Evento::nuevo(tipo, None, firmante.email, payload).firmar(firmante.llave);
    eventos::publicar(c, ops, &evento, aplicar)?;
    Ok(())
}

// ---------- En la máquina de quien hace el cambio (con su llave privada) ----------

/// `crear_usuario`: alta sin contraseña; nombre y apellido salen del correo.
#[allow(clippy::too_many_arguments)] // el correo, el rol y las dos fechas son el comando mismo
pub fn crear(
    c: &Connection,
    ops: &Path,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    rol: Rol,
    ahora: &str,
    hoy: &str,
) -> Resultado<Usuario> {
    let email = normalizar_email(email);
    let (nombre, apellido) = nombre_desde_email(&email);
    let nuevo = Usuario {
        email: email.clone(),
        nombre,
        apellido,
        rol,
        fecha_alta: hoy.to_string(),
        firma: Some(firmar(firmante, &email, rol, ahora)?),
        firmado_en: Some(ahora.to_string()),
        firmado_por: Some(normalizar_email(firmante.email)),
        llave_publica: None,
        certificado: None,
    };
    let autor = Autor::from(firmante);
    publicar_firmado(c, ops, firmante, "crear_usuario", &nuevo, |tx| {
        aplicar_crear(tx, publica_admin, &autor, &nuevo)
    })?;
    Ok(nuevo)
}

/// `cambiar_rol`. Al quitarle el rol a un Líder **no** se borran su llave
/// pública ni su certificado: así lo que firmó antes sigue verificándose.
pub fn cambiar_rol(
    c: &Connection,
    ops: &Path,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    rol_nuevo: Rol,
    ahora: &str,
) -> Resultado<()> {
    let fila = fila_firmada(firmante, email, rol_nuevo, ahora)?;
    let autor = Autor::from(firmante);
    publicar_firmado(c, ops, firmante, "cambiar_rol", &fila, |tx| {
        aplicar_cambiar_rol(tx, publica_admin, &autor, &fila)
    })
}

/// `confirmar_usuario` (solo Administrador): firma una fila **sin firma**. Una
/// fila con firma que no coincide no se confirma (podría ser un rol escrito a
/// mano): para esas, `cambiar_rol`.
pub fn confirmar(
    c: &Connection,
    ops: &Path,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    ahora: &str,
) -> Resultado<()> {
    if firmante.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }
    let email = normalizar_email(email);
    let actual = usuarios::buscar(c, &email)?
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    let fila = fila_firmada(firmante, &email, actual.rol, ahora)?;
    let autor = Autor::from(firmante);
    publicar_firmado(c, ops, firmante, "confirmar_usuario", &fila, |tx| {
        aplicar_confirmar(tx, publica_admin, &autor, &fila)
    })
}

/// `eliminar_usuario`. Si se elimina a un Líder, el Administrador vuelve a
/// firmar las filas que ese Líder firmó (sin su fila ya no se podrían
/// verificar) y esas firmas viajan en el mismo evento. Devuelve cuántas filas
/// volvió a firmar.
///
/// No le quita el acceso a la carpeta de Drive/OneDrive: eso lo tiene que hacer
/// el equipo a mano (la interfaz lo recuerda al confirmar).
pub fn eliminar(
    c: &Connection,
    ops: &Path,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    ahora: &str,
) -> Resultado<usize> {
    let email = normalizar_email(email);
    let suyas = firmadas_por(c, publica_admin, &email)?;
    if !suyas.is_empty() && firmante.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }
    let refirmadas = suyas
        .iter()
        .map(|u| fila_firmada(firmante, &u.email, u.rol, ahora))
        .collect::<Resultado<Vec<_>>>()?;
    let carga = Eliminacion { email, refirmadas };
    let autor = Autor::from(firmante);
    publicar_firmado(c, ops, firmante, "eliminar_usuario", &carga, |tx| {
        aplicar_eliminar(tx, publica_admin, &autor, &carga)
    })?;
    Ok(carga.refirmadas.len())
}

// ---------- En cualquier máquina (sin llave privada): a partir del evento ----------

/// Quién firmó un evento administrativo, según su `firma_admin`: el
/// Administrador (su llave pública fijada en esta máquina), o un Líder
/// (`evento.usuario`) con certificado del Administrador vigente en la fecha del
/// evento. `None` si la firma falta o no coincide con ninguna de las dos.
pub fn firmante_del_evento(
    c: &Connection,
    publica_admin: &str,
    evento: &Evento,
) -> Resultado<Option<Rol>> {
    if evento.firma_valida(publica_admin) {
        return Ok(Some(Rol::Administrador));
    }
    let tabla = usuarios::listar(c)?;
    let verificador = Verificador::nuevo(publica_admin, &tabla);
    Ok(verificador
        .llave_de_lider_en(&evento.usuario, &evento.timestamp)
        .filter(|llave| evento.firma_valida(llave))
        .map(|_| Rol::Lider))
}

/// Aplica un evento de usuarios **ya verificado** (`rol_firmante` es lo que
/// devolvió [`firmante_del_evento`]) con las mismas reglas que en la máquina que
/// lo emitió. `Err` con `validacion`, `sin_permiso` o `no_encontrado` = el evento
/// no cumple las reglas con el estado actual (quien sincroniza lo descarta);
/// cualquier otro error es de la base.
pub fn aplicar_evento(
    c: &Connection,
    publica_admin: &str,
    rol_firmante: Rol,
    evento: &Evento,
) -> Resultado<()> {
    fn carga<T: DeserializeOwned>(evento: &Evento) -> Resultado<T> {
        serde_json::from_value(evento.payload.clone()).map_err(|_| {
            Error::Validacion(format!(
                "El evento {} no trae los datos esperados",
                evento.tipo
            ))
        })
    }
    let autor = Autor {
        email: normalizar_email(&evento.usuario),
        rol: rol_firmante,
    };
    match evento.tipo.as_str() {
        "crear_usuario" => aplicar_crear(c, publica_admin, &autor, &carga(evento)?),
        "cambiar_rol" => aplicar_cambiar_rol(c, publica_admin, &autor, &carga(evento)?),
        "confirmar_usuario" => aplicar_confirmar(c, publica_admin, &autor, &carga(evento)?),
        "eliminar_usuario" => aplicar_eliminar(c, publica_admin, &autor, &carga(evento)?),
        otro => Err(Error::Interno(format!(
            "`{otro}` no es un evento de usuarios"
        ))),
    }
}

fn aplicar_crear(
    c: &Connection,
    publica_admin: &str,
    autor: &Autor,
    nuevo: &Usuario,
) -> Resultado<()> {
    if !roles_asignables(autor.rol, None).contains(&nuevo.rol) {
        return Err(Error::Validacion(format!(
            "No puedes crear usuarios con el rol {}",
            nuevo.rol
        )));
    }
    exigir_del_autor(autor, nuevo.firmado_por.as_deref())?;
    usuarios::insertar(c, nuevo)?;
    exigir_verificada(c, publica_admin, &normalizar_email(&nuevo.email))
}

fn aplicar_cambiar_rol(
    c: &Connection,
    publica_admin: &str,
    autor: &Autor,
    fila: &FilaFirmada,
) -> Resultado<()> {
    let email = normalizar_email(&fila.email);
    if email == autor.email {
        return Err(Error::Validacion("No puedes cambiar tu propio rol".into()));
    }
    exigir_del_autor(autor, Some(&fila.firmado_por))?;
    let (guardada, actual) = buscar_con_rol(c, publica_admin, &email)?;
    if !roles_asignables(autor.rol, Some(actual)).contains(&fila.rol) {
        return Err(Error::SinPermiso);
    }
    if guardada.rol == fila.rol && guardada.firma.is_some() && actual == guardada.rol {
        return Err(Error::Validacion(format!(
            "{email} ya tiene el rol {}",
            fila.rol
        )));
    }
    usuarios::actualizar_rol(
        c,
        &email,
        fila.rol,
        Some(&fila.firma),
        Some(&fila.firmado_en),
        Some(&fila.firmado_por),
    )?;
    exigir_verificada(c, publica_admin, &email)
}

fn aplicar_confirmar(
    c: &Connection,
    publica_admin: &str,
    autor: &Autor,
    fila: &FilaFirmada,
) -> Resultado<()> {
    if autor.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }
    exigir_del_autor(autor, Some(&fila.firmado_por))?;
    let email = normalizar_email(&fila.email);
    let tabla = usuarios::listar(c)?;
    let guardada = tabla
        .iter()
        .find(|u| u.email == email)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    if Verificador::nuevo(publica_admin, &tabla).verificar_firma(guardada)
        != Verificacion::Pendiente
    {
        return Err(Error::Validacion(format!(
            "{email} no está pendiente de confirmar"
        )));
    }
    if guardada.rol != fila.rol {
        return Err(Error::Validacion(format!(
            "{email} ya no tiene el rol que se confirmó"
        )));
    }
    usuarios::actualizar_rol(
        c,
        &email,
        fila.rol,
        Some(&fila.firma),
        Some(&fila.firmado_en),
        Some(&fila.firmado_por),
    )?;
    exigir_verificada(c, publica_admin, &email)
}

fn aplicar_eliminar(
    c: &Connection,
    publica_admin: &str,
    autor: &Autor,
    carga: &Eliminacion,
) -> Resultado<()> {
    let email = normalizar_email(&carga.email);
    if email == autor.email {
        return Err(Error::Validacion("No puedes eliminarte a ti mismo".into()));
    }
    let (_, rol) = buscar_con_rol(c, publica_admin, &email)?;
    if !puede_eliminar(autor.rol, rol) {
        return Err(Error::SinPermiso);
    }
    if autor.rol != Rol::Administrador
        && (!carga.refirmadas.is_empty() || !firmadas_por(c, publica_admin, &email)?.is_empty())
    {
        return Err(Error::SinPermiso);
    }
    for fila in &carga.refirmadas {
        exigir_del_autor(autor, Some(&fila.firmado_por))?;
        let destino = normalizar_email(&fila.email);
        if destino == email {
            return Err(Error::Validacion(
                "No se vuelve a firmar a quien se elimina".into(),
            ));
        }
        usuarios::actualizar_rol(
            c,
            &destino,
            fila.rol,
            Some(&fila.firma),
            Some(&fila.firmado_en),
            Some(&fila.firmado_por),
        )?;
    }
    usuarios::eliminar(c, &email)?;
    for fila in &carga.refirmadas {
        exigir_verificada(c, publica_admin, &normalizar_email(&fila.email))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::eventos::lectura::{self, ARCHIVO_LOG};
    use crate::eventos::{escribir_evento, sincronizar, Aplicacion, ResumenSincronizacion};
    use crate::identidad::llaves;
    use chrono::Utc;
    use serde_json::json;
    use std::path::PathBuf;

    const ANTES: &str = "2026-11-01T09:00:00Z";
    const AHORA: &str = "2026-11-10T10:00:00Z";
    const HOY: &str = "2026-11-10";
    const ADMIN: &str = "admin@virustf.dev";
    const LIDER: &str = "lider@virustf.dev";

    struct Proyecto {
        _dirs: (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir),
        db: Db,
        /// `ops/` compartida y carpeta local (para `eventos.log`) de esta máquina.
        ops: PathBuf,
        local: PathBuf,
        publica: String,
        admin: LlavePrivada,
        lider: LlavePrivada,
    }

    impl Proyecto {
        fn c(&self) -> &Connection {
            self.db.conexion()
        }
        fn como_admin(&self) -> Firmante<'_> {
            Firmante {
                email: ADMIN,
                rol: Rol::Administrador,
                llave: &self.admin,
            }
        }
        fn como_lider(&self) -> Firmante<'_> {
            Firmante {
                email: LIDER,
                rol: Rol::Lider,
                llave: &self.lider,
            }
        }
        fn rol(&self, email: &str) -> (Rol, Option<Rol>, Verificacion) {
            rol_en(self.c(), &self.publica, email)
        }
        /// Los eventos escritos en `ops/`, leídos del disco, en orden de nombre.
        fn eventos(&self) -> Vec<Evento> {
            let mut nombres: Vec<_> = match std::fs::read_dir(&self.ops) {
                Ok(d) => d.map(|e| e.unwrap().path()).collect(),
                Err(_) => Vec::new(),
            };
            nombres.sort();
            nombres
                .iter()
                .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
                .collect()
        }
        /// Otra máquina del mismo proyecto: su propia base, con las mismas
        /// filas iniciales (Administrador y Líder) y nada más.
        fn otra_maquina(&self) -> Db {
            let db = Db::en_memoria().unwrap();
            for u in usuarios::listar(self.c()).unwrap() {
                if u.email == ADMIN || u.email == LIDER {
                    usuarios::insertar(db.conexion(), &u).unwrap();
                }
            }
            db
        }
        /// Escanea `ops/` desde la base `db` con el aplicador real de la app.
        fn sincronizar(&self, db: &Db) -> ResumenSincronizacion {
            sincronizar(db.conexion(), &self.ops, &self.local, Utc::now(), |c, e| {
                lectura::aplicar_evento(c, &self.publica, e)
            })
            .unwrap()
        }
        fn log(&self) -> String {
            std::fs::read_to_string(self.local.join(ARCHIVO_LOG)).unwrap_or_default()
        }
    }

    fn rol_en(c: &Connection, publica: &str, email: &str) -> (Rol, Option<Rol>, Verificacion) {
        let lista = listar(c, publica, Rol::Administrador).unwrap();
        let u = lista
            .into_iter()
            .find(|u| u.email.as_deref() == Some(email))
            .unwrap();
        (u.rol, u.rol_efectivo, u.verificacion)
    }

    /// Administrador y un Líder certificado, los dos firmados por el Administrador.
    fn proyecto() -> Proyecto {
        let dir_admin = tempfile::tempdir().unwrap();
        let dir_lider = tempfile::tempdir().unwrap();
        let dir_nube = tempfile::tempdir().unwrap();
        let publica = llaves::generar_llaves_admin(dir_admin.path(), None).unwrap();
        let admin = llaves::cargar_llave_admin(dir_admin.path(), None).unwrap();
        let publica_lider = llaves::generar_llaves_lider(dir_lider.path(), None).unwrap();
        let lider = llaves::cargar_llave_lider(dir_lider.path(), None).unwrap();
        let db = Db::en_memoria().unwrap();
        let fila = |email: &str, rol: Rol| Usuario {
            email: email.into(),
            nombre: "N".into(),
            apellido: "A".into(),
            rol,
            fecha_alta: HOY.into(),
            firma: Some(firmas::firmar_rol(&admin, email, rol, ANTES).unwrap()),
            firmado_en: Some(ANTES.into()),
            firmado_por: Some(ADMIN.into()),
            llave_publica: None,
            certificado: None,
        };
        usuarios::insertar(db.conexion(), &fila(ADMIN, Rol::Administrador)).unwrap();
        usuarios::insertar(
            db.conexion(),
            &Usuario {
                llave_publica: Some(publica_lider.clone()),
                certificado: Some(
                    firmas::certificar_lider(&admin, LIDER, &publica_lider, ANTES).unwrap(),
                ),
                ..fila(LIDER, Rol::Lider)
            },
        )
        .unwrap();
        let ops = dir_nube.path().join("ops");
        let local = dir_nube.path().join("local");
        Proyecto {
            _dirs: (dir_admin, dir_lider, dir_nube),
            db,
            ops,
            local,
            publica,
            admin,
            lider,
        }
    }

    #[test]
    fn roles_asignables_como_en_el_frontend() {
        use Rol::*;
        assert_eq!(roles_asignables(Administrador, None), ROLES_ADMIN);
        assert!(roles_asignables(Administrador, Some(Administrador)).is_empty());
        assert_eq!(roles_asignables(Lider, None), ROLES_BASE);
        assert_eq!(roles_asignables(Lider, Some(Tester)), ROLES_BASE);
        assert!(roles_asignables(Lider, Some(Lider)).is_empty());
        assert!(roles_asignables(Desarrollador, None).is_empty());
        assert!(puede_eliminar(Administrador, Lider));
        assert!(!puede_eliminar(Administrador, Administrador));
        assert!(puede_eliminar(Lider, ColaboradorExterno));
        assert!(!puede_eliminar(Lider, Tester));
        assert!(!puede_eliminar(Tester, Desarrollador));
    }

    #[test]
    fn crear_nace_firmado_por_quien_lo_crea_y_sale_como_evento_firmado() {
        let p = proyecto();
        let u = crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "Ana.Lopez@VirusTF.dev",
            Rol::Lider,
            AHORA,
            HOY,
        )
        .unwrap();
        assert_eq!(
            (u.email.as_str(), u.nombre.as_str(), u.apellido.as_str()),
            ("ana.lopez@virustf.dev", "Ana", "Lopez")
        );
        assert_eq!(
            p.rol("ana.lopez@virustf.dev"),
            (Rol::Lider, Some(Rol::Lider), Verificacion::Verificado)
        );

        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            "beto@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();
        assert_eq!(p.rol("beto@virustf.dev").2, Verificacion::Verificado);

        // Un evento por cambio, cada uno firmado por quien lo hizo y con la fila
        // ya firmada adentro (la llave privada no viaja).
        let eventos = p.eventos();
        assert_eq!(eventos.len(), 2);
        assert!(eventos
            .iter()
            .all(|e| e.tipo == "crear_usuario" && e.exige_firma()));
        assert_eq!(eventos[0].usuario, ADMIN);
        assert!(eventos[0].firma_valida(&p.publica));
        assert_eq!(eventos[0].payload["email"], "ana.lopez@virustf.dev");
        assert_eq!(eventos[0].payload["rol"], "Líder");
        assert!(eventos[0].payload["firma"].is_string());
        assert_eq!(eventos[1].usuario, LIDER);
        assert!(!eventos[1].firma_valida(&p.publica), "la firmó el Líder");
        assert!(eventos[1].firma_valida(&p.lider.publica()));
        assert_eq!(
            firmante_del_evento(p.c(), &p.publica, &eventos[0]).unwrap(),
            Some(Rol::Administrador)
        );
        assert_eq!(
            firmante_del_evento(p.c(), &p.publica, &eventos[1]).unwrap(),
            Some(Rol::Lider)
        );
    }

    #[test]
    fn crear_rechaza_correo_repetido_y_rol_no_permitido_sin_dejar_evento() {
        let p = proyecto();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();
        let repetido = crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ANA@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        );
        assert_eq!(repetido.unwrap_err().codigo(), "validacion");
        for rol in [Rol::Lider, Rol::Administrador] {
            assert!(crear(
                p.c(),
                &p.ops,
                &p.publica,
                &p.como_lider(),
                "x@virustf.dev",
                rol,
                AHORA,
                HOY
            )
            .is_err());
        }
        assert_eq!(
            crear(
                p.c(),
                &p.ops,
                &p.publica,
                &p.como_admin(),
                "y@virustf.dev",
                Rol::Administrador,
                AHORA,
                HOY
            )
            .unwrap_err()
            .codigo(),
            "validacion"
        );
        assert_eq!(
            usuarios::listar(p.c()).unwrap().len(),
            3,
            "no quedó nada a medias"
        );
        assert_eq!(p.eventos().len(), 1, "los rechazados no dejan evento");
    }

    #[test]
    fn un_lider_sin_certificado_no_guarda_nada() {
        let p = proyecto();
        let dir = tempfile::tempdir().unwrap();
        llaves::generar_llaves_lider(dir.path(), None).unwrap();
        let otra = llaves::cargar_llave_lider(dir.path(), None).unwrap();
        let impostor = Firmante {
            email: LIDER,
            rol: Rol::Lider,
            llave: &otra,
        };
        let error = crear(
            p.c(),
            &p.ops,
            &p.publica,
            &impostor,
            "x@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap_err();
        assert_eq!(error.codigo(), "validacion");
        assert!(usuarios::buscar(p.c(), "x@virustf.dev").unwrap().is_none());
        assert!(p.eventos().is_empty());
    }

    #[test]
    fn cambiar_rol_respeta_quien_cambia_a_quien() {
        let p = proyecto();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();

        cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            "ana@virustf.dev",
            Rol::Desarrollador,
            AHORA,
        )
        .unwrap();
        assert_eq!(p.rol("ana@virustf.dev").1, Some(Rol::Desarrollador));
        let ultimo = p.eventos().pop().unwrap();
        assert_eq!(
            (ultimo.tipo.as_str(), ultimo.usuario.as_str()),
            ("cambiar_rol", LIDER)
        );
        assert_eq!(ultimo.payload["rol"], "Desarrollador");
        // El Líder no da el rol de Líder, ni toca a otro Líder ni al Administrador.
        assert!(cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            "ana@virustf.dev",
            Rol::Lider,
            AHORA
        )
        .is_err());
        assert!(cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            ADMIN,
            Rol::Tester,
            AHORA
        )
        .is_err());
        // Nadie cambia su propio rol ni le da a otro el de Administrador.
        assert!(cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            ADMIN,
            Rol::Lider,
            AHORA
        )
        .is_err());
        assert!(cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Administrador,
            AHORA
        )
        .is_err());
        // Mismo rol que ya tiene.
        assert!(cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Desarrollador,
            AHORA
        )
        .is_err());
        assert!(cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "nadie@virustf.dev",
            Rol::Tester,
            AHORA
        )
        .is_err());
        assert_eq!(p.eventos().len(), 2, "solo los dos cambios que sí pasaron");
    }

    #[test]
    fn quitarle_el_rol_a_un_lider_conserva_lo_que_firmo() {
        let p = proyecto();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            "beto@virustf.dev",
            Rol::Tester,
            "2026-11-05T10:00:00Z",
            HOY,
        )
        .unwrap();
        cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            LIDER,
            Rol::Desarrollador,
            AHORA,
        )
        .unwrap();
        let fila = usuarios::buscar(p.c(), LIDER).unwrap().unwrap();
        assert!(fila.llave_publica.is_some() && fila.certificado.is_some());
        assert_eq!(p.rol("beto@virustf.dev").2, Verificacion::Verificado);
    }

    #[test]
    fn confirmar_solo_filas_sin_firma() {
        let p = proyecto();
        let sin_firma = Usuario {
            firma: None,
            firmado_en: None,
            firmado_por: None,
            ..crear(
                p.c(),
                &p.ops,
                &p.publica,
                &p.como_admin(),
                "ana@virustf.dev",
                Rol::Tester,
                AHORA,
                HOY,
            )
            .unwrap()
        };
        usuarios::eliminar(p.c(), "ana@virustf.dev").unwrap();
        usuarios::insertar(p.c(), &sin_firma).unwrap();
        assert_eq!(p.rol("ana@virustf.dev").2, Verificacion::Pendiente);

        assert_eq!(
            confirmar(
                p.c(),
                &p.ops,
                &p.publica,
                &p.como_lider(),
                "ana@virustf.dev",
                AHORA
            )
            .unwrap_err()
            .codigo(),
            "sin_permiso"
        );
        confirmar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            AHORA,
        )
        .unwrap();
        assert_eq!(
            p.rol("ana@virustf.dev"),
            (Rol::Tester, Some(Rol::Tester), Verificacion::Verificado)
        );
        let ultimo = p.eventos().pop().unwrap();
        assert_eq!(ultimo.tipo, "confirmar_usuario");
        assert!(ultimo.firma_valida(&p.publica));
        assert!(confirmar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            AHORA
        )
        .is_err());

        // Rol escrito a mano sobre una fila firmada: no se confirma.
        usuarios::actualizar_rol(
            p.c(),
            "ana@virustf.dev",
            Rol::Lider,
            Some("x"),
            Some(AHORA),
            None,
        )
        .unwrap();
        assert!(confirmar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            AHORA
        )
        .is_err());
    }

    #[test]
    fn eliminar_respeta_quien_elimina_a_quien() {
        let p = proyecto();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "dev@virustf.dev",
            Rol::Desarrollador,
            AHORA,
            HOY,
        )
        .unwrap();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "qa@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();

        assert!(eliminar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            "qa@virustf.dev",
            AHORA
        )
        .is_err());
        assert!(eliminar(p.c(), &p.ops, &p.publica, &p.como_lider(), ADMIN, AHORA).is_err());
        assert!(eliminar(p.c(), &p.ops, &p.publica, &p.como_admin(), ADMIN, AHORA).is_err());
        assert_eq!(
            eliminar(
                p.c(),
                &p.ops,
                &p.publica,
                &p.como_lider(),
                "dev@virustf.dev",
                AHORA
            )
            .unwrap(),
            0
        );
        assert!(usuarios::buscar(p.c(), "dev@virustf.dev")
            .unwrap()
            .is_none());
        eliminar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "qa@virustf.dev",
            AHORA,
        )
        .unwrap();
        assert!(eliminar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "qa@virustf.dev",
            AHORA
        )
        .is_err());
        let eventos = p.eventos();
        assert_eq!(eventos.len(), 4);
        assert_eq!(eventos[2].tipo, "eliminar_usuario");
        assert_eq!(eventos[2].payload["email"], "dev@virustf.dev");
        assert_eq!(eventos[2].payload["refirmadas"], json!([]));
    }

    #[test]
    fn eliminar_a_un_lider_vuelve_a_firmar_lo_suyo() {
        let p = proyecto();
        for email in ["beto@virustf.dev", "caro@virustf.dev"] {
            crear(
                p.c(),
                &p.ops,
                &p.publica,
                &p.como_lider(),
                email,
                Rol::Tester,
                "2026-11-05T10:00:00Z",
                HOY,
            )
            .unwrap();
        }

        assert_eq!(
            eliminar(p.c(), &p.ops, &p.publica, &p.como_admin(), LIDER, AHORA).unwrap(),
            2
        );
        assert!(usuarios::buscar(p.c(), LIDER).unwrap().is_none());
        for email in ["beto@virustf.dev", "caro@virustf.dev"] {
            assert_eq!(p.rol(email).2, Verificacion::Verificado, "{email}");
            let fila = usuarios::buscar(p.c(), email).unwrap().unwrap();
            assert_eq!(fila.firmado_por.as_deref(), Some(ADMIN));
        }
        let ultimo = p.eventos().pop().unwrap();
        assert_eq!(ultimo.payload["refirmadas"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn listar_esconde_los_correos_a_quien_no_es_lider_ni_admin() {
        let p = proyecto();
        for (rol, ve) in [
            (Rol::Administrador, true),
            (Rol::Lider, true),
            (Rol::Tester, false),
            (Rol::Desarrollador, false),
            (Rol::ColaboradorExterno, false),
        ] {
            let lista = listar(p.c(), &p.publica, rol).unwrap();
            assert_eq!(lista.len(), 2);
            assert!(lista.iter().all(|u| u.email.is_some() == ve), "{rol}");
        }
        let json = serde_json::to_value(listar(p.c(), &p.publica, Rol::Tester).unwrap()).unwrap();
        assert!(json[0]["email"].is_null());
        assert_eq!(json[0]["verificacion"], "verificado");
    }

    #[test]
    fn otra_maquina_aplica_los_eventos_y_llega_a_la_misma_tabla() {
        let p = proyecto();
        let b = p.otra_maquina();
        // Antes de que pase nada, B ya está al día (no hay eventos).
        assert!(!p.sincronizar(&b).hubo_cambios());

        // En la máquina A pasan de todo: altas del Administrador y del Líder,
        // un cambio de rol, una confirmación y la salida del Líder.
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_lider(),
            "beto@virustf.dev",
            Rol::Desarrollador,
            AHORA,
            HOY,
        )
        .unwrap();
        cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Lider,
            AHORA,
        )
        .unwrap();
        // Una fila pendiente (sin firma) que las dos máquinas ya tenían.
        let pendiente = Usuario {
            email: "caro@virustf.dev".into(),
            nombre: "Caro".into(),
            apellido: String::new(),
            rol: Rol::Tester,
            fecha_alta: HOY.into(),
            firma: None,
            firmado_en: None,
            firmado_por: None,
            llave_publica: None,
            certificado: None,
        };
        usuarios::insertar(p.c(), &pendiente).unwrap();
        usuarios::insertar(b.conexion(), &pendiente).unwrap();
        confirmar(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "caro@virustf.dev",
            AHORA,
        )
        .unwrap();
        assert_eq!(
            eliminar(p.c(), &p.ops, &p.publica, &p.como_admin(), LIDER, AHORA).unwrap(),
            1
        );

        // B los aplica todos, en orden, y queda igual que A.
        let r = p.sincronizar(&b);
        assert_eq!((r.aplicados, r.descartados, r.desconocidos), (5, 0, 0));
        assert_eq!(
            usuarios::listar(b.conexion()).unwrap(),
            usuarios::listar(p.c()).unwrap()
        );
        assert_eq!(
            rol_en(b.conexion(), &p.publica, "ana@virustf.dev"),
            (Rol::Lider, Some(Rol::Lider), Verificacion::Verificado)
        );
        assert_eq!(
            rol_en(b.conexion(), &p.publica, "beto@virustf.dev").2,
            Verificacion::Verificado
        );
        assert_eq!(
            rol_en(b.conexion(), &p.publica, "caro@virustf.dev").2,
            Verificacion::Verificado
        );
        assert!(usuarios::buscar(b.conexion(), LIDER).unwrap().is_none());

        // A no vuelve a aplicar lo suyo; B tampoco repite.
        assert!(!p.sincronizar(&p.db).hubo_cambios());
        assert!(!p.sincronizar(&b).hubo_cambios());
        assert!(p.log().is_empty(), "nada que registrar: {}", p.log());
    }

    /// Un evento firmado por el Líder desde otra máquina, que A (sin la llave del
    /// Líder) acepta porque su certificado está vigente.
    #[test]
    fn el_evento_de_un_lider_vale_solo_mientras_su_certificado_esta_vigente() {
        let p = proyecto();
        let firmar_como_lider = |cuando: &str, email: &str| {
            let fila = fila_firmada(&p.como_lider(), email, Rol::Tester, cuando).unwrap();
            let mut e = Evento::nuevo(
                "crear_usuario",
                None,
                LIDER,
                json!({
                    "email": fila.email, "nombre": "N", "apellido": "A", "rol": "Tester",
                    "fecha_alta": HOY, "firma": fila.firma, "firmado_en": fila.firmado_en,
                    "firmado_por": fila.firmado_por, "llave_publica": null, "certificado": null
                }),
            );
            e.timestamp = format!("{}.000Z", &cuando[..19]);
            e.firmar(&p.lider)
        };
        // Vigente: el Administrador lo certificó el 2026-11-01.
        let vigente = firmar_como_lider("2026-11-05T10:00:00Z", "d1@virustf.dev");
        assert_eq!(
            firmante_del_evento(p.c(), &p.publica, &vigente).unwrap(),
            Some(Rol::Lider)
        );
        assert_eq!(
            lectura::aplicar_evento(p.c(), &p.publica, &vigente).unwrap(),
            Aplicacion::Aplicado
        );
        assert_eq!(p.rol("d1@virustf.dev").2, Verificacion::Verificado);
        // Antes del certificado: no.
        let temprano = firmar_como_lider("2026-10-20T10:00:00Z", "d2@virustf.dev");
        assert_eq!(
            firmante_del_evento(p.c(), &p.publica, &temprano).unwrap(),
            None
        );

        // El Administrador le quita el rol el 2026-11-10: lo de después ya no vale,
        // lo de antes sí.
        cambiar_rol(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            LIDER,
            Rol::Desarrollador,
            AHORA,
        )
        .unwrap();
        let despues = firmar_como_lider("2026-11-12T10:00:00Z", "d3@virustf.dev");
        assert_eq!(
            firmante_del_evento(p.c(), &p.publica, &despues).unwrap(),
            None
        );
        assert!(matches!(
            lectura::aplicar_evento(p.c(), &p.publica, &despues).unwrap(),
            Aplicacion::Descartado(motivo) if motivo.contains("firma_admin")
        ));
        assert!(usuarios::buscar(p.c(), "d3@virustf.dev").unwrap().is_none());
        let antes = firmar_como_lider("2026-11-08T10:00:00Z", "d4@virustf.dev");
        assert_eq!(
            firmante_del_evento(p.c(), &p.publica, &antes).unwrap(),
            Some(Rol::Lider)
        );
    }

    /// **Prueba con un evento falso** (issue #93): alguien con acceso a `ops/`
    /// suelta eventos administrativos que no firmó el Administrador ni un Líder
    /// certificado. Ninguna máquina los aplica, y no se reintentan.
    #[test]
    fn un_evento_falso_se_descarta_sin_aplicar() {
        let p = proyecto();
        crear(
            p.c(),
            &p.ops,
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();
        let b = p.otra_maquina();
        assert_eq!(p.sincronizar(&b).aplicados, 1);

        // Lo que un impostor querría: ser Líder (o Administrador) sin que nadie se lo dé.
        let ascenso = |rol: &str, firma: &str| {
            json!({
                "email": "ana@virustf.dev", "rol": rol, "firma": firma,
                "firmado_en": AHORA, "firmado_por": ADMIN
            })
        };
        let otro = tempfile::tempdir().unwrap();
        llaves::generar_llaves_admin(otro.path(), None).unwrap();
        let intrusa = llaves::cargar_llave_admin(otro.path(), None).unwrap();

        // 1. Sin `firma_admin`, diciendo que lo emite el Administrador.
        let sin_firma = Evento::nuevo("cambiar_rol", None, ADMIN, ascenso("Líder", "x"));
        // 2. Firmado con una llave que no es la del proyecto.
        let otra_llave =
            Evento::nuevo("cambiar_rol", None, ADMIN, ascenso("Líder", "x")).firmar(&intrusa);
        // 3. Un evento legítimo del Administrador, alterado después de firmarlo.
        let legitimo = fila_firmada(
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Desarrollador,
            AHORA,
        )
        .unwrap();
        let mut alterado = Evento::nuevo(
            "cambiar_rol",
            None,
            ADMIN,
            serde_json::to_value(&legitimo).unwrap(),
        )
        .firmar(&p.admin);
        alterado.payload["rol"] = json!("Líder");
        // 4. Firmado de verdad por el Líder, pero con algo que un Líder no puede
        //    hacer (dar el rol de Líder): la firma vale, la regla no.
        let fila_lider = FilaFirmada {
            email: "ana@virustf.dev".into(),
            rol: Rol::Lider,
            firma: p.lider.firmar(&format!("ana@virustf.dev|Líder|{AHORA}")),
            firmado_en: AHORA.into(),
            firmado_por: LIDER.into(),
        };
        let abuso_lider = Evento::nuevo(
            "cambiar_rol",
            None,
            LIDER,
            serde_json::to_value(&fila_lider).unwrap(),
        )
        .firmar(&p.lider);
        // 5. Firmado por el Administrador, pero con la fila firmada por otra llave
        //    (el evento no coincide con la fila que trae).
        let fila_ajena = FilaFirmada {
            email: "ana@virustf.dev".into(),
            rol: Rol::Lider,
            firma: intrusa.firmar(&format!("ana@virustf.dev|Líder|{AHORA}")),
            firmado_en: AHORA.into(),
            firmado_por: ADMIN.into(),
        };
        let fila_falsa = Evento::nuevo(
            "cambiar_rol",
            None,
            ADMIN,
            serde_json::to_value(&fila_ajena).unwrap(),
        )
        .firmar(&p.admin);
        // 6. Un alta sin firma, con un payload que ni siquiera es una fila.
        let basura = Evento::nuevo("crear_usuario", None, ADMIN, json!({ "hola": 1 }));
        // 7. Eliminar al Administrador, firmado con la llave intrusa.
        let golpe = Evento::nuevo(
            "eliminar_usuario",
            None,
            LIDER,
            json!({ "email": ADMIN, "refirmadas": [] }),
        )
        .firmar(&intrusa);

        let falsos = [
            sin_firma,
            otra_llave,
            alterado,
            abuso_lider,
            fila_falsa,
            basura,
            golpe,
        ];
        for e in &falsos {
            escribir_evento(&p.ops, e).unwrap();
        }

        // Ninguna de las dos máquinas aplica nada; todas las descartan y no las repiten.
        for db in [&p.db, &b] {
            let r = p.sincronizar(db);
            assert_eq!(
                (r.aplicados, r.descartados, r.desconocidos),
                (0, falsos.len(), 0)
            );
            assert_eq!(
                rol_en(db.conexion(), &p.publica, "ana@virustf.dev"),
                (Rol::Tester, Some(Rol::Tester), Verificacion::Verificado)
            );
            assert_eq!(usuarios::listar(db.conexion()).unwrap().len(), 3);
            assert_eq!(p.sincronizar(db), ResumenSincronizacion::default());
        }
        let log = p.log();
        assert_eq!(log.matches("descartado").count(), falsos.len() * 2, "{log}");
        assert!(log.contains("firma_admin falta o no coincide"), "{log}");
        assert!(log.contains("No tienes permiso"), "{log}");
        assert!(log.contains("no está certificada"), "{log}");
    }

    /// **Prueba en el entorno real** (CLAUDE.md): dos "máquinas" (cada una con su
    /// base y su carpeta local) sobre una subcarpeta de la carpeta de
    /// OneDrive/Drive de verdad. A (Administrador, con su llave) crea un usuario
    /// y le cambia el rol; B (un Desarrollador, sin llave) los aplica y queda
    /// igual que A. Después B deja en `ops/` dos eventos falsos que lo harían
    /// Líder (uno sin firma y otro firmado con una llave que él mismo generó):
    /// A y B los descartan, el rol no cambia y queda anotado en `eventos.log`.
    /// Borra la subcarpeta al final.
    ///
    /// ```text
    /// BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive cargo test --lib entorno_real_eventos_administrativos -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "usa la carpeta de OneDrive/Drive de verdad: correr a mano con BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_eventos_administrativos_firmados() {
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida};

        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let compartida = nube.join(format!(
            "BranchBoard-prueba-93-{}",
            chrono::Local::now().format("%H%M%S")
        ));
        std::fs::create_dir_all(&compartida).unwrap();
        let maquina_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let local_a = maquina_a.path().join("proyectos").join("prueba-93");
        let local_b = maquina_b.path().join("proyectos").join("prueba-93");

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            // A tiene la llave del Administrador; las dos arrancan con su fila.
            let publica = llaves::generar_llaves_admin(&local_a, None).unwrap();
            let admin = llaves::cargar_llave_admin(&local_a, None).unwrap();
            let db_a = Db::en_memoria().unwrap();
            let db_b = Db::en_memoria().unwrap();
            let fila_admin = Usuario {
                email: ADMIN.into(),
                nombre: "N".into(),
                apellido: "A".into(),
                rol: Rol::Administrador,
                fecha_alta: HOY.into(),
                firma: Some(firmas::firmar_rol(&admin, ADMIN, Rol::Administrador, ANTES).unwrap()),
                firmado_en: Some(ANTES.into()),
                firmado_por: Some(ADMIN.into()),
                llave_publica: None,
                certificado: None,
            };
            usuarios::insertar(db_a.conexion(), &fila_admin).unwrap();
            usuarios::insertar(db_b.conexion(), &fila_admin).unwrap();
            let como_admin = Firmante {
                email: ADMIN,
                rol: Rol::Administrador,
                llave: &admin,
            };
            let sincronizar_en = |db: &Db, ops: &Path, local: &Path| {
                sincronizar(db.conexion(), ops, local, Utc::now(), |c, e| {
                    lectura::aplicar_evento(c, &publica, e)
                })
                .unwrap()
            };

            // A da de alta a dev y lo pasa a Tester.
            let ahora = crate::modelo::ahora_iso();
            crear(
                db_a.conexion(),
                &ops_a,
                &publica,
                &como_admin,
                "dev@virustf.dev",
                Rol::Desarrollador,
                &ahora,
                HOY,
            )
            .unwrap();
            cambiar_rol(
                db_a.conexion(),
                &ops_a,
                &publica,
                &como_admin,
                "dev@virustf.dev",
                Rol::Tester,
                &ahora,
            )
            .unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
            let r = sincronizar_en(&db_b, &ops_b, &local_b);
            println!("B sincroniza lo de A: {r:?}");
            assert_eq!((r.aplicados, r.descartados), (2, 0));
            assert_eq!(
                usuarios::listar(db_b.conexion()).unwrap(),
                usuarios::listar(db_a.conexion()).unwrap(),
                "misma tabla que A"
            );
            assert_eq!(
                rol_en(db_b.conexion(), &publica, "dev@virustf.dev"),
                (Rol::Tester, Some(Rol::Tester), Verificacion::Verificado)
            );

            // B (dev) intenta hacerse Líder con eventos falsos: sin firma, y con
            // una llave que generó él mismo.
            llaves::generar_llaves_admin(&local_b, None).unwrap();
            let suya = llaves::cargar_llave_admin(&local_b, None).unwrap();
            let ascenso = json!({
                "email": "dev@virustf.dev", "rol": "Líder",
                "firma": suya.firmar(&format!("dev@virustf.dev|Líder|{ahora}")),
                "firmado_en": ahora, "firmado_por": ADMIN
            });
            escribir_evento(
                &ops_b,
                &Evento::nuevo("cambiar_rol", None, ADMIN, ascenso.clone()),
            )
            .unwrap();
            escribir_evento(
                &ops_b,
                &Evento::nuevo("cambiar_rol", None, ADMIN, ascenso).firmar(&suya),
            )
            .unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));

            for (nombre, db, ops, local) in [
                ("A", &db_a, &ops_a, &local_a),
                ("B", &db_b, &ops_b, &local_b),
            ] {
                let r = sincronizar_en(db, ops, local);
                println!("{nombre} sincroniza los falsos: {r:?}");
                assert_eq!((r.aplicados, r.descartados), (0, 2));
                assert_eq!(
                    rol_en(db.conexion(), &publica, "dev@virustf.dev"),
                    (Rol::Tester, Some(Rol::Tester), Verificacion::Verificado),
                    "{nombre}: el rol no cambió"
                );
                let log = std::fs::read_to_string(local.join(ARCHIVO_LOG)).unwrap();
                println!("eventos.log de {nombre}:\n{}", log.trim());
                assert_eq!(log.matches("firma_admin falta o no coincide").count(), 2);
                assert!(
                    !sincronizar_en(db, ops, local).hubo_cambios(),
                    "{nombre}: no se repiten"
                );
            }
        }));
        let borrada = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", borrada.is_ok());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }
}
