//! Creación, edición y listado de tareas (issue #95). Ver "Modelo de datos:
//! tabla `tareas`", "Notificaciones" (punto 1) y "Sincronización entre
//! máquinas" en `documentacion/BACKEND.md`.
//!
//! Cada cambio que muta `tareas` sale como evento a `ops/` (`crear_tarea`,
//! `crear_tarea_con_subtareas`, `editar_tarea`) con `eventos::publicar`, que lo
//! aplica en la misma transacción local, y las demás máquinas lo reproducen con
//! [`aplicar_evento`] al sincronizar.

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Deserializer, Serialize};

use crate::db::commits;
use crate::db::notificaciones::{self, Notificacion};
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::identidad::firmas::{verificar_usuarios, Verificacion};
use crate::modelo::{normalizar_email, EstadoTarea, Rol, TipoNotificacion, TipoTarea};
use crate::ramas::TareaDeCadena;
use crate::servidor::Rama;

/// Tipos de evento que maneja este módulo.
pub const TIPOS: &[&str] = &[
    "crear_tarea",
    "crear_tarea_con_subtareas",
    "editar_tarea",
    "tomar_tarea",
    "asignar_tarea",
    "desasignar_tarea",
    "cambiar_estado",
];

/// Máximo de ancestros que puede tener una tarea en `tareas`:
/// `tarea general (0) -> subtarea (1) -> subtarea de subtarea (2)`, que junto
/// con `/main/dev` suman los 5 niveles máximos de Unity VC (#83).
const MAX_ANCESTROS_TAREA: usize = 2;

/// Valor de `modo_trabajo` de una tarea que ya tiene su rama.
const MODO_RAMA: &str = "rama";

/// Quién ejecuta una acción sobre tareas (con su rol ya verificado).
#[derive(Debug, Clone)]
pub struct AutorTarea<'a> {
    pub email: &'a str,
    pub rol: Rol,
}

/// Datos para crear una tarea (`crear_tarea`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatosNuevaTarea {
    pub titulo: String,
    #[serde(default)]
    pub detalles: String,
    pub tipo: TipoTarea,
    #[serde(default)]
    pub tarea_padre_id: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
    #[serde(default)]
    pub backlog_id: Option<String>,
    #[serde(default)]
    pub con_fecha: bool,
    #[serde(default)]
    pub fecha_limite: Option<String>,
}

/// Cada subtarea incluida al crear una tarea general (`crear_tarea_con_subtareas`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatosSubtareaNueva {
    pub tipo: TipoTarea,
    pub titulo: String,
    #[serde(default)]
    pub detalles: String,
}

/// Datos para `crear_tarea_con_subtareas`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatosNuevaTareaConSubtareas {
    pub titulo: String,
    #[serde(default)]
    pub detalles: String,
    pub tipo: TipoTarea,
    #[serde(default)]
    pub tarea_padre_id: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
    #[serde(default)]
    pub backlog_id: Option<String>,
    #[serde(default)]
    pub con_fecha: bool,
    #[serde(default)]
    pub fecha_limite: Option<String>,
    #[serde(default)]
    pub subtareas: Vec<DatosSubtareaNueva>,
}

/// Deserializa un campo opcional donde `undefined` (ausente) es `None` (no
/// tocar), `null` es `Some(None)` (limpiar) y `"valor"` es `Some(Some("valor"))`.
mod campo_anulable {
    use super::*;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer).map(Some)
    }
}

/// Campos editables con `editar_tarea`. Solo los presentes (`Some`) se tocan.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CambiosTarea {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titulo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detalles: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tipo: Option<TipoTarea>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "campo_anulable::deserialize"
    )]
    pub sprint_id: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "campo_anulable::deserialize"
    )]
    pub backlog_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub con_fecha: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "campo_anulable::deserialize"
    )]
    pub fecha_limite: Option<Option<String>>,
}

/// `payload` del evento `crear_tarea_con_subtareas`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PayloadTareaConSubtareas {
    tarea: Tarea,
    subtareas: Vec<Tarea>,
}

/// `payload` del evento `editar_tarea`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PayloadEditarTarea {
    id: String,
    cambios: CambiosTarea,
}

/// Rama asignada a un ancestro sin rama al tomar o asignar una subtarea (#96, #98).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RamaAncestro {
    pub id: String,
    pub rama_asociada: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ultimo_changeset_visto: Option<i64>,
}

/// `payload` del evento `tomar_tarea` (#96) y `asignar_tarea` (#98).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadTomarTarea {
    pub id: String,
    pub asignado_a: String,
    pub rama_asociada: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ultimo_changeset_visto: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ramas_ancestros: Vec<RamaAncestro>,
}

/// `payload` del evento `desasignar_tarea` (#98).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadDesasignarTarea {
    pub id: String,
}

/// `payload` del evento `cambiar_estado` (#99).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadCambiarEstado {
    pub id: String,
    pub estado: EstadoTarea,
}

/// Respuesta de [`cambiar_estado`] (#99): tareas actualizadas y posibles
/// tareas origen con cambios prestados vigentes al pasar a revisión.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResultadoCambioEstado {
    pub tareas: Vec<Tarea>,
    pub aviso_prestamos: Vec<String>,
}

/// Información de una colisión de doble toma (#97) entre dos eventos `tomar_tarea`.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoDobleToma {
    pub tarea_id: String,
    pub titulo: String,
    pub ganador: String,
    pub rama_ganador: String,
    pub perdedor: String,
    pub rama_perdedor: String,
    pub cs_inicial_perdedor: Option<i64>,
    pub evento_perdedor_id: String,
    /// Si `Some`, el evento entrante era anterior al que estaba aplicado en local
    /// y debe reemplazar al actual cuando la rama del perdedor no tenga changesets.
    pub revertir_a_entrante: Option<PayloadTomarTarea>,
}

fn limpiar_opcional(valor: Option<String>) -> Option<String> {
    valor.and_then(|s| {
        let t = s.trim().to_string();
        (!t.is_empty()).then_some(t)
    })
}

fn validar_tipo_creacion(tipo: TipoTarea) -> Resultado<()> {
    if tipo == TipoTarea::SinDefinir {
        return Err(Error::Validacion(
            "El tipo 'Sin definir' es exclusivo de la subtarea En espera".into(),
        ));
    }
    Ok(())
}

/// Comprueba que `id_padre` exista, no esté en la Papelera y no supere el
/// límite de niveles de anidación (`MAX_ANCESTROS_TAREA`). Devuelve cuántos
/// ancestros tendrá la nueva tarea (1 = hija directa de una tarea general).
fn validar_padre(c: &Connection, id_padre: &str) -> Resultado<usize> {
    let mut actual_id = Some(id_padre.to_string());
    let mut niveles = 0usize;
    while let Some(id) = actual_id {
        let padre = db_tareas::obtener(c, &id)?;
        if padre.eliminada {
            return Err(Error::Validacion(format!(
                "La tarea padre {id} está en la Papelera"
            )));
        }
        niveles += 1;
        if niveles > MAX_ANCESTROS_TAREA {
            return Err(Error::Validacion(
                "No se puede crear la subtarea: superaría el límite de 5 niveles de ramas".into(),
            ));
        }
        actual_id = padre.tarea_padre_id;
    }
    Ok(niveles)
}

fn armar_nueva_tarea(
    id: String,
    datos: &DatosNuevaTarea,
    autor: &AutorTarea<'_>,
    hoy: &str,
) -> Resultado<Tarea> {
    let titulo = datos.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion("El título es obligatorio".into()));
    }
    validar_tipo_creacion(datos.tipo)?;
    let fecha_limite = if datos.con_fecha {
        limpiar_opcional(datos.fecha_limite.clone())
    } else {
        None
    };
    let mut t = Tarea::nueva(&id, titulo, datos.tipo, &normalizar_email(autor.email), hoy);
    t.detalles = datos.detalles.trim().to_string();
    t.tarea_padre_id = limpiar_opcional(datos.tarea_padre_id.clone());
    t.pendiente_aprobacion = !autor.rol.es_lider_o_admin();
    t.sprint_id = limpiar_opcional(datos.sprint_id.clone());
    t.backlog_id = limpiar_opcional(datos.backlog_id.clone());
    t.con_fecha = datos.con_fecha;
    t.fecha_limite = fecha_limite;
    Ok(t)
}

/// Lista todas las tareas del proyecto (en orden de creación, con
/// `edicion_pendiente` calculada).
pub fn listar(c: &Connection) -> Resultado<Vec<Tarea>> {
    db_tareas::listar(c)
}

/// Crea una tarea y publica el evento `crear_tarea` en `ops/`.
/// Queda `pendiente_aprobacion = true` si quien la crea no es Líder ni Administrador.
pub fn crear(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    datos: &DatosNuevaTarea,
    ahora: &str,
    hoy: &str,
) -> Resultado<Tarea> {
    let padre_id = limpiar_opcional(datos.tarea_padre_id.clone());
    if let Some(ref pid) = padre_id {
        validar_padre(c, pid)?;
    }
    let id = db_tareas::siguiente_id(c, padre_id.as_deref())?;
    let mut datos_limpios = datos.clone();
    datos_limpios.tarea_padre_id = padre_id;
    let nueva = armar_nueva_tarea(id, &datos_limpios, autor, hoy)?;

    let mut ev = Evento::nuevo(
        "crear_tarea",
        Some(&nueva.id),
        &normalizar_email(autor.email),
        serde_json::to_value(&nueva)
            .map_err(|e| Error::Interno(format!("no se pudo serializar la tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| aplicar_crear_tarea(tx, &nueva))?;
    db_tareas::obtener(c, &nueva.id)
}

/// Crea una tarea y, si trae subtareas, las crea encadenadas a ella en una sola
/// operación y un solo evento (`crear_tarea_con_subtareas`). Si `subtareas`
/// viene vacío, delega en [`crear`].
pub fn crear_con_subtareas(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    datos: &DatosNuevaTareaConSubtareas,
    ahora: &str,
    hoy: &str,
) -> Resultado<Vec<Tarea>> {
    let raiz_datos = DatosNuevaTarea {
        titulo: datos.titulo.clone(),
        detalles: datos.detalles.clone(),
        tipo: datos.tipo,
        tarea_padre_id: datos.tarea_padre_id.clone(),
        sprint_id: datos.sprint_id.clone(),
        backlog_id: datos.backlog_id.clone(),
        con_fecha: datos.con_fecha,
        fecha_limite: datos.fecha_limite.clone(),
    };
    if datos.subtareas.is_empty() {
        let unica = crear(c, ops, autor, &raiz_datos, ahora, hoy)?;
        return Ok(vec![unica]);
    }

    let padre_id = limpiar_opcional(datos.tarea_padre_id.clone());
    let niveles_raiz = match padre_id {
        Some(ref pid) => validar_padre(c, pid)?,
        None => 0,
    };
    if niveles_raiz + 1 > MAX_ANCESTROS_TAREA {
        return Err(Error::Validacion(
            "No se pueden agregar subtareas en este nivel: superaría el límite de 5 niveles de ramas"
                .into(),
        ));
    }

    let id_raiz = db_tareas::siguiente_id(c, padre_id.as_deref())?;
    let mut raiz_limpia = raiz_datos;
    raiz_limpia.tarea_padre_id = padre_id;
    let tarea_raiz = armar_nueva_tarea(id_raiz.clone(), &raiz_limpia, autor, hoy)?;

    let mut subtareas = Vec::with_capacity(datos.subtareas.len());
    for (idx, sub) in datos.subtareas.iter().enumerate() {
        let sub_datos = DatosNuevaTarea {
            titulo: sub.titulo.clone(),
            detalles: sub.detalles.clone(),
            tipo: sub.tipo,
            tarea_padre_id: Some(id_raiz.clone()),
            sprint_id: tarea_raiz.sprint_id.clone(),
            backlog_id: tarea_raiz.backlog_id.clone(),
            con_fecha: false,
            fecha_limite: None,
        };
        let sub_id = format!("{id_raiz}-{}", idx + 1);
        subtareas.push(armar_nueva_tarea(sub_id, &sub_datos, autor, hoy)?);
    }

    let payload = PayloadTareaConSubtareas {
        tarea: tarea_raiz.clone(),
        subtareas: subtareas.clone(),
    };
    let mut ev = Evento::nuevo(
        "crear_tarea_con_subtareas",
        Some(&tarea_raiz.id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar las tareas: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_crear_tarea_con_subtareas(tx, &payload)
    })?;

    let mut creadas = Vec::with_capacity(1 + subtareas.len());
    creadas.push(db_tareas::obtener(c, &tarea_raiz.id)?);
    for st in &subtareas {
        creadas.push(db_tareas::obtener(c, &st.id)?);
    }
    Ok(creadas)
}

/// Edita los campos permitidos de una tarea (`editar_tarea`) y publica el
/// evento en `ops/`.
///
/// Permiso:
/// - **Líder o Administrador**: puede editar cualquier tarea no eliminada.
/// - **Creador** (Desarrollador, Tester o Colaborador externo): solo su propia
///   tarea mientras siga `pendiente_aprobacion = true`.
pub fn editar(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    cambios: &CambiosTarea,
    ahora: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let payload = PayloadEditarTarea {
        id: id.to_string(),
        cambios: cambios.clone(),
    };
    let mut ev = Evento::nuevo(
        "editar_tarea",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar la edición: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_editar_tarea(tx, autor.email, Some(autor.rol), &payload)
    })?;
    db_tareas::obtener(c, id)
}

/// `true` si `tarea` no es subtarea de otra (`tarea_padre_id == None`) ni tiene
/// subtareas propias no eliminadas. Es la condición para que un Colaborador
/// externo pueda tomarla o recibirla asignada.
pub fn es_tarea_individual(c: &Connection, tarea: &Tarea) -> Resultado<bool> {
    if tarea.tarea_padre_id.is_some() {
        return Ok(false);
    }
    let hijas = db_tareas::hijas(c, &tarea.id)?;
    Ok(hijas.iter().all(|h| h.eliminada))
}

fn cadena_hacia_raiz(c: &Connection, tarea: &Tarea, accion: &str) -> Resultado<Vec<TareaDeCadena>> {
    let mut inversa = vec![TareaDeCadena {
        id: tarea.id.clone(),
        titulo: tarea.titulo.clone(),
        rama: tarea.rama_asociada.clone(),
    }];
    let mut padre_id = tarea.tarea_padre_id.clone();
    let mut ancestros = 0usize;
    while let Some(pid) = padre_id {
        let padre = db_tareas::obtener(c, &pid)?;
        if padre.eliminada {
            return Err(Error::Validacion(format!(
                "La tarea padre {} está en la Papelera",
                padre.id
            )));
        }
        ancestros += 1;
        if ancestros > MAX_ANCESTROS_TAREA {
            return Err(Error::Validacion(format!(
                "No se puede {accion} la tarea: superaría el límite de 5 niveles de ramas"
            )));
        }
        inversa.push(TareaDeCadena {
            id: padre.id.clone(),
            titulo: padre.titulo.clone(),
            rama: padre.rama_asociada.clone(),
        });
        padre_id = padre.tarea_padre_id;
    }
    inversa.reverse();
    Ok(inversa)
}

fn armar_payload_rama(
    cadena: &[TareaDeCadena],
    creadas: &[(String, Rama)],
    asignado_a: String,
    etiqueta_vacio: &str,
) -> Resultado<PayloadTomarTarea> {
    let objetivo = cadena
        .last()
        .ok_or_else(|| Error::Validacion(etiqueta_vacio.to_string()))?;
    let (rama_asociada, ultimo_changeset_visto) =
        match creadas.iter().find(|(tid, _)| tid == &objetivo.id) {
            Some((_, r)) => (r.name.clone(), Some(r.head_changeset)),
            None => match objetivo.rama {
                Some(ref r) => (r.clone(), None),
                None => {
                    return Err(Error::Validacion(format!(
                        "Falta la rama creada para la tarea {}",
                        objetivo.id
                    )))
                }
            },
        };

    let mut ramas_ancestros = Vec::new();
    for paso in &cadena[..cadena.len() - 1] {
        if let Some((_, r)) = creadas.iter().find(|(tid, _)| tid == &paso.id) {
            ramas_ancestros.push(RamaAncestro {
                id: paso.id.clone(),
                rama_asociada: r.name.clone(),
                ultimo_changeset_visto: Some(r.head_changeset),
            });
        } else if paso.rama.is_none() {
            return Err(Error::Validacion(format!(
                "Falta la rama creada para la tarea padre {}",
                paso.id
            )));
        }
    }

    Ok(PayloadTomarTarea {
        id: objetivo.id.clone(),
        asignado_a,
        rama_asociada,
        ultimo_changeset_visto,
        ramas_ancestros,
    })
}

/// Valida que `autor` pueda tomar la tarea `id` y arma la cadena desde la tarea
/// general hasta `id` (inclusive) para [`crate::ramas::crear_ramas_de_cadena`].
pub fn cadena_para_tomar(
    c: &Connection,
    autor: &AutorTarea<'_>,
    id: &str,
) -> Resultado<Vec<TareaDeCadena>> {
    let tarea = db_tareas::obtener(c, id.trim())?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede tomar",
            tarea.id
        )));
    }
    if tarea.estado != EstadoTarea::PorHacer || tarea.asignado_a.is_some() {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está asignada o no está por hacer",
            tarea.id
        )));
    }
    if autor.rol == Rol::ColaboradorExterno && !es_tarea_individual(c, &tarea)? {
        return Err(Error::Validacion(
            "Solo puedes tomar tareas individuales".into(),
        ));
    }
    cadena_hacia_raiz(c, &tarea, "tomar")
}

/// Registra en la base local y publica en `ops/` el evento `tomar_tarea` una vez
/// que se crearon las ramas en Unity VC y se cambió el workspace local.
/// Devuelve todas las tareas de `cadena` actualizadas (ancestros + la tarea tomada).
pub fn registrar_toma(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    cadena: &[TareaDeCadena],
    creadas: &[(String, Rama)],
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let asignado_a = normalizar_email(autor.email);
    let payload = armar_payload_rama(
        cadena,
        creadas,
        asignado_a.clone(),
        "No hay tarea para tomar",
    )?;
    let mut ev = Evento::nuevo(
        "tomar_tarea",
        Some(&payload.id),
        &asignado_a,
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar tomar_tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_tomar_tarea(tx, autor.email, Some(autor.rol), &payload)
    })?;

    cadena
        .iter()
        .map(|paso| db_tareas::obtener(c, &paso.id))
        .collect()
}

/// Toma la tarea `id` (`tomar_tarea`, issue #96): valida permisos y cadena de
/// ancestros, ejecuta `preparar_ramas_y_workspace` (creación de ramas en el
/// servidor + `cm switch` en el workspace local) y publica el evento en `ops/`.
pub fn tomar(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    preparar_ramas_y_workspace: impl FnOnce(&[TareaDeCadena]) -> Resultado<Vec<(String, Rama)>>,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let cadena = cadena_para_tomar(c, autor, id)?;
    let creadas = preparar_ramas_y_workspace(&cadena)?;
    registrar_toma(c, ops, autor, &cadena, &creadas, ahora)
}

/// Valida que `autor` (Líder/Admin) pueda asignar `id` a `email_destinatario`
/// y arma la cadena desde la tarea general hasta `id` (inclusive) (#98).
pub fn cadena_para_asignar(
    c: &Connection,
    autor: &AutorTarea<'_>,
    id: &str,
    email_destinatario: &str,
) -> Resultado<Vec<TareaDeCadena>> {
    if !matches!(autor.rol, Rol::Lider | Rol::Administrador) {
        return Err(Error::Validacion(
            "Solo un Líder o Administrador puede asignar tareas".into(),
        ));
    }
    let dest = normalizar_email(email_destinatario);
    let usuario = usuarios::buscar(c, &dest)?
        .ok_or_else(|| Error::Validacion(format!("El usuario {dest} no existe en el proyecto")))?;
    let tarea = db_tareas::obtener(c, id.trim())?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede asignar",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    if db_tareas::hijas(c, &tarea.id)?.iter().any(|h| !h.eliminada) {
        return Err(Error::Validacion(
            "No se puede asignar una tarea que tiene subtareas".into(),
        ));
    }
    if usuario.rol == Rol::ColaboradorExterno && !es_tarea_individual(c, &tarea)? {
        return Err(Error::Validacion(
            "Un colaborador externo solo puede recibir tareas individuales".into(),
        ));
    }
    if tarea.asignado_a.as_deref().map(normalizar_email).as_deref() == Some(dest.as_str()) {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está asignada a {dest}",
            tarea.id
        )));
    }
    cadena_hacia_raiz(c, &tarea, "asignar")
}

/// Registra en la base local y publica en `ops/` el evento `asignar_tarea` (#98).
/// A diferencia de `tomar_tarea`, nunca cambia el workspace de quien asigna.
pub fn registrar_asignacion(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    cadena: &[TareaDeCadena],
    creadas: &[(String, Rama)],
    email_destinatario: &str,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let dest = normalizar_email(email_destinatario);
    let payload = armar_payload_rama(cadena, creadas, dest, "No hay tarea para asignar")?;
    let mut ev = Evento::nuevo(
        "asignar_tarea",
        Some(&payload.id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar asignar_tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_asignar_tarea(tx, autor.email, Some(autor.rol), &payload, ahora)
    })?;

    cadena
        .iter()
        .map(|paso| db_tareas::obtener(c, &paso.id))
        .collect()
}

/// Asigna la tarea `id` a `email_destinatario` (`asignar_tarea`, #98):
/// crea en Unity VC solo las ramas que falten (sin tocar el workspace local)
/// y publica el evento en `ops/` con notificación `tarea_asignada`.
pub fn asignar(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    email_destinatario: &str,
    preparar_ramas: impl FnOnce(&[TareaDeCadena]) -> Resultado<Vec<(String, Rama)>>,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let cadena = cadena_para_asignar(c, autor, id, email_destinatario)?;
    let creadas = if cadena.iter().any(|p| p.rama.is_none()) {
        preparar_ramas(&cadena)?
    } else {
        Vec::new()
    };
    registrar_asignacion(c, ops, autor, &cadena, &creadas, email_destinatario, ahora)
}

fn descendientes_activos(c: &Connection, padre_id: &str) -> Resultado<Vec<Tarea>> {
    let mut todos = Vec::new();
    let mut cola = vec![padre_id.to_string()];
    while let Some(actual) = cola.pop() {
        for hija in db_tareas::hijas(c, &actual)? {
            if !hija.eliminada {
                cola.push(hija.id.clone());
                todos.push(hija);
            }
        }
    }
    Ok(todos)
}

/// Suelta la tarea `id` (`desasignar_tarea`, #98): quita el responsable y deja la
/// tarea y sus subtareas activas en `'por hacer'`. Rechaza si hay check-ins.
pub fn desasignar(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let tarea = db_tareas::obtener(c, id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede soltar",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    if tarea.asignado_a.is_none() && tarea.estado == EstadoTarea::PorHacer {
        return Err(Error::Validacion(format!(
            "La tarea {} no está asignada",
            tarea.id
        )));
    }
    if !commits::de_tarea(c, &tarea.id)?.is_empty() {
        return Err(Error::Validacion(format!(
            "No se puede soltar la tarea {} porque ya tiene check-ins",
            tarea.id
        )));
    }
    for sub in descendientes_activos(c, &tarea.id)? {
        if !commits::de_tarea(c, &sub.id)?.is_empty() {
            return Err(Error::Validacion(format!(
                "No se puede soltar la tarea {} porque la subtarea {} ya tiene check-ins",
                tarea.id, sub.id
            )));
        }
    }

    let payload = PayloadDesasignarTarea {
        id: tarea.id.clone(),
    };
    let mut ev = Evento::nuevo(
        "desasignar_tarea",
        Some(&tarea.id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar desasignar_tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| aplicar_desasignar_tarea(tx, &payload))?;
    db_tareas::obtener(c, &tarea.id)
}

fn prestamos_vigentes_origenes(c: &Connection, ids: &[String]) -> Resultado<Vec<String>> {
    let mut origenes = Vec::new();
    for tid in ids {
        for p in commits::prestamos_vigentes(c, tid)? {
            if !origenes.contains(&p.origen_id) {
                origenes.push(p.origen_id);
            }
        }
    }
    Ok(origenes)
}

/// Cambia el estado de la tarea `id` (`cambiar_estado`, #99) aplicando todas las
/// reglas de negocio (subtareas, progreso/commits, préstamos, rol para terminar,
/// propagación hacia atrás y recálculo de padres de abajo hacia arriba).
pub fn cambiar_estado(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    estado: EstadoTarea,
    ahora: &str,
) -> Resultado<ResultadoCambioEstado> {
    let id = id.trim();
    let payload = PayloadCambiarEstado {
        id: id.to_string(),
        estado,
    };
    let mut ev = Evento::nuevo(
        "cambiar_estado",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar cambiar_estado: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    let mut aviso_prestamos = Vec::new();
    eventos::publicar(c, ops, &ev, |tx| {
        aviso_prestamos =
            aplicar_cambiar_estado(tx, autor.email, Some(autor.rol), &payload, ahora)?;
        Ok(())
    })?;

    Ok(ResultadoCambioEstado {
        tareas: db_tareas::listar(c)?,
        aviso_prestamos,
    })
}

fn aplicar_crear_tarea(c: &Connection, tarea: &Tarea) -> Resultado<()> {
    validar_tipo_creacion(tarea.tipo)?;
    if tarea.estado != EstadoTarea::PorHacer {
        return Err(Error::Validacion(
            "Una tarea nueva debe nacer en 'por hacer'".into(),
        ));
    }
    if let Some(ref pid) = tarea.tarea_padre_id {
        validar_padre(c, pid)?;
    }
    db_tareas::insertar(c, tarea)
}

fn aplicar_crear_tarea_con_subtareas(
    c: &Connection,
    payload: &PayloadTareaConSubtareas,
) -> Resultado<()> {
    aplicar_crear_tarea(c, &payload.tarea)?;
    for st in &payload.subtareas {
        if st.tarea_padre_id.as_deref() != Some(payload.tarea.id.as_str()) {
            return Err(Error::Validacion(format!(
                "La subtarea {} no apunta a la tarea raíz {}",
                st.id, payload.tarea.id
            )));
        }
        aplicar_crear_tarea(c, st)?;
    }
    Ok(())
}

/// Resuelve el rol verificado de `email` en la tabla `usuarios`, si existe.
fn rol_verificado_de(c: &Connection, publica_admin: &str, email: &str) -> Resultado<Option<Rol>> {
    let email = normalizar_email(email);
    if publica_admin.is_empty() {
        return Ok(usuarios::buscar(c, &email)?.map(|u| u.rol));
    }
    Ok(verificar_usuarios(publica_admin, usuarios::listar(c)?)
        .into_iter()
        .find(|u| u.usuario.email == email && u.verificacion == Verificacion::Verificado)
        .and_then(|u| u.rol_efectivo))
}

fn aplicar_editar_tarea(
    c: &Connection,
    autor_email: &str,
    rol_conocido: Option<Rol>,
    payload: &PayloadEditarTarea,
) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede editar",
            tarea.id
        )));
    }

    let autor_email = normalizar_email(autor_email);
    let es_creador_pendiente =
        tarea.pendiente_aprobacion && normalizar_email(&tarea.creado_por) == autor_email;
    let es_lider_o_admin = rol_conocido.is_some_and(|r| r.es_lider_o_admin());
    if !es_lider_o_admin && !es_creador_pendiente {
        return Err(Error::SinPermiso);
    }

    let c_edit = &payload.cambios;
    if let Some(ref titulo) = c_edit.titulo {
        let limpio = titulo.trim();
        if limpio.is_empty() {
            return Err(Error::Validacion("El título es obligatorio".into()));
        }
        tarea.titulo = limpio.to_string();
    }
    if let Some(ref detalles) = c_edit.detalles {
        tarea.detalles = detalles.trim().to_string();
    }
    if let Some(tipo) = c_edit.tipo {
        validar_tipo_creacion(tipo)?;
        tarea.tipo = tipo;
    }
    if let Some(ref nuevo_sprint) = c_edit.sprint_id {
        let nuevo = limpiar_opcional(nuevo_sprint.clone());
        if let (Some(ref actual), Some(ref destino)) = (&tarea.sprint_id, &nuevo) {
            if actual != destino {
                return Err(Error::Validacion(
                    "La tarea ya está en otro sprint; primero hay que quitarla de ese".into(),
                ));
            }
        }
        tarea.sprint_id = nuevo;
    }
    if let Some(ref nuevo_backlog) = c_edit.backlog_id {
        let nuevo = limpiar_opcional(nuevo_backlog.clone());
        if let (Some(ref actual), Some(ref destino)) = (&tarea.backlog_id, &nuevo) {
            if actual != destino {
                return Err(Error::Validacion(
                    "La tarea ya está en otro backlog; primero hay que quitarla de ese".into(),
                ));
            }
        }
        tarea.backlog_id = nuevo;
    }
    if let Some(con_fecha) = c_edit.con_fecha {
        tarea.con_fecha = con_fecha;
        if !con_fecha {
            tarea.fecha_limite = None;
        }
    }
    if let Some(ref fecha_limite) = c_edit.fecha_limite {
        tarea.fecha_limite = if tarea.con_fecha {
            limpiar_opcional(fecha_limite.clone())
        } else {
            None
        };
    }

    db_tareas::actualizar(c, &tarea)
}

/// Aplica un evento de tareas recibido desde `ops/` (`eventos/lectura.rs` y
/// `eventos/snapshot.rs`).
pub fn aplicar_evento(c: &Connection, publica_admin: &str, evento: &Evento) -> Resultado<()> {
    match evento.tipo.as_str() {
        "crear_tarea" => {
            let mut tarea: Tarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_tarea inválido: {e}")))?;
            tarea.creado_por = normalizar_email(&evento.usuario);
            if let Some(rol) = rol_verificado_de(c, publica_admin, &evento.usuario)? {
                tarea.pendiente_aprobacion = !rol.es_lider_o_admin();
            }
            aplicar_crear_tarea(c, &tarea)
        }
        "crear_tarea_con_subtareas" => {
            let mut payload: PayloadTareaConSubtareas =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("crear_tarea_con_subtareas inválido: {e}"))
                })?;
            let autor = normalizar_email(&evento.usuario);
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            payload.tarea.creado_por = autor.clone();
            if let Some(r) = rol {
                payload.tarea.pendiente_aprobacion = !r.es_lider_o_admin();
            }
            for st in &mut payload.subtareas {
                st.creado_por = autor.clone();
                if let Some(r) = rol {
                    st.pendiente_aprobacion = !r.es_lider_o_admin();
                }
            }
            aplicar_crear_tarea_con_subtareas(c, &payload)
        }
        "editar_tarea" => {
            let payload: PayloadEditarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("editar_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_editar_tarea(c, &evento.usuario, rol, &payload)
        }
        "tomar_tarea" => {
            let payload: PayloadTomarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("tomar_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_tomar_tarea(c, &evento.usuario, rol, &payload)
        }
        "asignar_tarea" => {
            let payload: PayloadTomarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("asignar_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_asignar_tarea(c, &evento.usuario, rol, &payload, &evento.timestamp)
        }
        "desasignar_tarea" => {
            let payload: PayloadDesasignarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("desasignar_tarea inválido: {e}")))?;
            aplicar_desasignar_tarea(c, &payload)
        }
        "cambiar_estado" => {
            let payload: PayloadCambiarEstado = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("cambiar_estado inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_cambiar_estado(c, &evento.usuario, rol, &payload, &evento.timestamp)?;
            Ok(())
        }
        otro => Err(Error::Validacion(format!(
            "tipo de evento de tarea no soportado: {otro}"
        ))),
    }
}

fn validar_ruta_rama_tarea(rama: &str) -> Resultado<String> {
    let limpia = rama.trim().trim_end_matches('/');
    if !limpia.starts_with(&format!("{}/", crate::ramas::RAMA_DEV))
        || crate::ramas::niveles(limpia) > crate::ramas::MAX_NIVELES
    {
        return Err(Error::Validacion(format!(
            "La rama {limpia} no es una rama de tarea válida"
        )));
    }
    Ok(limpia.to_string())
}

fn aplicar_ramas_ancestros(c: &Connection, ancestros: &[RamaAncestro]) -> Resultado<()> {
    for anc in ancestros {
        let rama_anc = validar_ruta_rama_tarea(&anc.rama_asociada)?;
        let mut fila_anc = db_tareas::obtener(c, &anc.id)?;
        if fila_anc.eliminada {
            return Err(Error::Validacion(format!(
                "La tarea padre {} está en la Papelera",
                fila_anc.id
            )));
        }
        if fila_anc.rama_asociada.is_none() {
            fila_anc.rama_asociada = Some(rama_anc);
            fila_anc.modo_trabajo = Some(MODO_RAMA.to_string());
            if fila_anc.ultimo_changeset_visto.is_none() {
                fila_anc.ultimo_changeset_visto = anc.ultimo_changeset_visto;
            }
            db_tareas::actualizar(c, &fila_anc)?;
        }
    }
    Ok(())
}

fn aplicar_tomar_tarea(
    c: &Connection,
    autor_email: &str,
    rol_conocido: Option<Rol>,
    payload: &PayloadTomarTarea,
) -> Resultado<()> {
    let autor = normalizar_email(autor_email);
    if normalizar_email(&payload.asignado_a) != autor {
        return Err(Error::Validacion(
            "El responsable de tomar_tarea no coincide con quien emite el evento".into(),
        ));
    }
    let rama = validar_ruta_rama_tarea(&payload.rama_asociada)?;

    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede tomar",
            tarea.id
        )));
    }
    if tarea.estado != EstadoTarea::PorHacer || tarea.asignado_a.is_some() {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está asignada o no está por hacer",
            tarea.id
        )));
    }
    if rol_conocido == Some(Rol::ColaboradorExterno) && !es_tarea_individual(c, &tarea)? {
        return Err(Error::Validacion(
            "Solo puedes tomar tareas individuales".into(),
        ));
    }

    aplicar_ramas_ancestros(c, &payload.ramas_ancestros)?;

    tarea.asignado_a = Some(autor);
    tarea.modo_trabajo = Some(MODO_RAMA.to_string());
    tarea.rama_asociada = Some(rama);
    if tarea.ultimo_changeset_visto.is_none() {
        tarea.ultimo_changeset_visto = payload.ultimo_changeset_visto;
    }
    db_tareas::actualizar(c, &tarea)
}

fn aplicar_asignar_tarea(
    c: &Connection,
    _autor_email: &str,
    rol_conocido: Option<Rol>,
    payload: &PayloadTomarTarea,
    ahora: &str,
) -> Resultado<()> {
    if let Some(rol) = rol_conocido {
        if !matches!(rol, Rol::Lider | Rol::Administrador) {
            return Err(Error::Validacion(
                "Solo un Líder o Administrador puede asignar tareas".into(),
            ));
        }
    }
    let rama = validar_ruta_rama_tarea(&payload.rama_asociada)?;
    let dest = normalizar_email(&payload.asignado_a);
    let usuario = usuarios::buscar(c, &dest)?
        .ok_or_else(|| Error::Validacion(format!("El usuario {dest} no existe en el proyecto")))?;

    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede asignar",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    if db_tareas::hijas(c, &tarea.id)?.iter().any(|h| !h.eliminada) {
        return Err(Error::Validacion(
            "No se puede asignar una tarea que tiene subtareas".into(),
        ));
    }
    if usuario.rol == Rol::ColaboradorExterno && !es_tarea_individual(c, &tarea)? {
        return Err(Error::Validacion(
            "Un colaborador externo solo puede recibir tareas individuales".into(),
        ));
    }

    aplicar_ramas_ancestros(c, &payload.ramas_ancestros)?;

    tarea.asignado_a = Some(dest.clone());
    if tarea.rama_asociada.is_none() {
        tarea.rama_asociada = Some(rama);
        tarea.modo_trabajo = Some(MODO_RAMA.to_string());
        if tarea.ultimo_changeset_visto.is_none() {
            tarea.ultimo_changeset_visto = payload.ultimo_changeset_visto;
        }
    }
    db_tareas::actualizar(c, &tarea)?;

    insertar_notificacion_si_falta(
        c,
        &Notificacion {
            id: format!("noti_asignar_{}_{}_{}", tarea.id, dest, ahora),
            tipo: TipoNotificacion::TareaAsignada,
            para: dest,
            titulo: "Tarea asignada".into(),
            cuerpo: format!("Se te asignó la tarea {}", tarea.id),
            leida: false,
            fecha: ahora.to_string(),
            tarea_id: Some(tarea.id),
            accionable: false,
        },
    )
}

fn aplicar_desasignar_tarea(c: &Connection, payload: &PayloadDesasignarTarea) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede soltar",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    if !commits::de_tarea(c, &tarea.id)?.is_empty() {
        return Err(Error::Validacion(format!(
            "No se puede soltar la tarea {} porque ya tiene check-ins",
            tarea.id
        )));
    }
    let descendientes = descendientes_activos(c, &tarea.id)?;
    for sub in &descendientes {
        if !commits::de_tarea(c, &sub.id)?.is_empty() {
            return Err(Error::Validacion(format!(
                "No se puede soltar la tarea {} porque la subtarea {} ya tiene check-ins",
                tarea.id, sub.id
            )));
        }
    }

    tarea.asignado_a = None;
    tarea.estado = EstadoTarea::PorHacer;
    db_tareas::actualizar(c, &tarea)?;

    for mut sub in descendientes {
        sub.asignado_a = None;
        sub.estado = EstadoTarea::PorHacer;
        db_tareas::actualizar(c, &sub)?;
    }
    recalcular_padres(c, tarea.tarea_padre_id)?;
    Ok(())
}

/// Recalcula el estado de la cadena de padres desde `padre_id` hacia la raíz (#99).
/// - "terminada" es final: no se vuelve a derivar.
/// - Todas en "por hacer" -> "por hacer" (salvo que el padre tenga commits -> "en producción").
/// - Alguna en "por hacer" o "en producción" -> "en producción".
/// - Ninguna en "por hacer"/"en producción" -> "en revisión".
/// - Si sale de "por hacer" y no tiene responsable, hereda el de alguna hija.
fn recalcular_padres(c: &Connection, mut padre_id: Option<String>) -> Resultado<()> {
    while let Some(pid) = padre_id {
        let mut padre = db_tareas::obtener(c, &pid)?;
        padre_id = padre.tarea_padre_id.clone();
        if padre.eliminada || padre.estado == EstadoTarea::Terminada {
            continue;
        }
        let hijas: Vec<Tarea> = db_tareas::hijas(c, &padre.id)?
            .into_iter()
            .filter(|h| !h.eliminada)
            .collect();
        if hijas.is_empty() {
            continue;
        }
        let padre_con_commits = !commits::de_tarea(c, &padre.id)?.is_empty();
        let derivado = if hijas.iter().all(|h| h.estado == EstadoTarea::PorHacer) {
            if padre_con_commits {
                EstadoTarea::EnProduccion
            } else {
                EstadoTarea::PorHacer
            }
        } else if hijas
            .iter()
            .any(|h| matches!(h.estado, EstadoTarea::PorHacer | EstadoTarea::EnProduccion))
        {
            EstadoTarea::EnProduccion
        } else {
            EstadoTarea::EnRevision
        };

        let heredado = if derivado != EstadoTarea::PorHacer && padre.asignado_a.is_none() {
            hijas.iter().find_map(|h| h.asignado_a.clone())
        } else {
            None
        };
        let cambio_estado = padre.estado != derivado;
        let cambio_responsable = heredado.is_some();
        if cambio_estado || cambio_responsable {
            padre.estado = derivado;
            if let Some(resp) = heredado {
                padre.asignado_a = Some(resp);
            }
            db_tareas::actualizar(c, &padre)?;
        }
    }
    Ok(())
}

fn fecha_corta_de_iso(ahora: &str) -> String {
    ahora.get(..10).unwrap_or(ahora).to_string()
}

fn aplicar_cambiar_estado(
    c: &Connection,
    autor_email: &str,
    rol_conocido: Option<Rol>,
    payload: &PayloadCambiarEstado,
    ahora: &str,
) -> Resultado<Vec<String>> {
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if payload.estado == EstadoTarea::Terminada {
        if let Some(rol) = rol_conocido {
            if !rol.es_lider_o_admin() {
                return Err(Error::Validacion(
                    "Solo un Líder o Administrador puede marcar una tarea como terminada".into(),
                ));
            }
        }
    }

    let descendientes = descendientes_activos(c, &tarea.id)?;
    let mut ids_subarbol = vec![tarea.id.clone()];
    ids_subarbol.extend(descendientes.iter().map(|d| d.id.clone()));

    let mut aviso_prestamos = Vec::new();
    if matches!(
        payload.estado,
        EstadoTarea::EnRevision | EstadoTarea::Terminada
    ) {
        if tarea.pendiente_aprobacion {
            return Err(Error::Validacion(format!(
                "La tarea {} sigue pendiente de aprobación",
                tarea.id
            )));
        }
        let abiertas: Vec<&str> = descendientes
            .iter()
            .filter(|h| {
                if payload.estado == EstadoTarea::Terminada {
                    h.estado != EstadoTarea::Terminada
                } else {
                    matches!(h.estado, EstadoTarea::PorHacer | EstadoTarea::EnProduccion)
                }
            })
            .map(|h| h.id.as_str())
            .collect();
        if !abiertas.is_empty() {
            return Err(Error::Validacion(format!(
                "La tarea {} todavía tiene subtareas abiertas: {}",
                tarea.id,
                abiertas.join(", ")
            )));
        }
        let prestamos = prestamos_vigentes_origenes(c, &ids_subarbol)?;
        if payload.estado == EstadoTarea::Terminada && !prestamos.is_empty() {
            return Err(Error::Validacion(format!(
                "Antes de terminar {}, devuelve los cambios prestados de {}",
                tarea.id,
                prestamos.join(", ")
            )));
        }
        aviso_prestamos = prestamos;
    }

    if payload.estado == EstadoTarea::PorHacer {
        for tid in &ids_subarbol {
            if !commits::de_tarea(c, tid)?.is_empty() {
                return Err(Error::Validacion(format!(
                    "No se puede volver a 'por hacer': {tid} ya tiene check-ins (hay progreso)"
                )));
            }
        }
    }

    let anterior_estado = tarea.estado;
    let autor = normalizar_email(autor_email);
    let propaga = !matches!(
        payload.estado,
        EstadoTarea::EnRevision | EstadoTarea::Terminada
    );

    if payload.estado != EstadoTarea::PorHacer && tarea.asignado_a.is_none() {
        tarea.asignado_a = descendientes
            .iter()
            .find_map(|d| d.asignado_a.clone())
            .or(Some(autor.clone()));
    }
    tarea.estado = payload.estado;
    if payload.estado == EstadoTarea::Terminada {
        tarea.fecha_completada = Some(fecha_corta_de_iso(ahora));
    }
    db_tareas::actualizar(c, &tarea)?;

    if propaga {
        for mut sub in descendientes {
            if payload.estado != EstadoTarea::PorHacer && sub.asignado_a.is_none() {
                sub.asignado_a = Some(autor.clone());
            }
            sub.estado = payload.estado;
            db_tareas::actualizar(c, &sub)?;
        }
    }

    recalcular_padres(c, tarea.tarea_padre_id)?;

    if anterior_estado != EstadoTarea::EnRevision && payload.estado == EstadoTarea::EnRevision {
        for u in usuarios::listar(c)? {
            if u.rol.es_lider_o_admin() {
                insertar_notificacion_si_falta(
                    c,
                    &Notificacion {
                        id: format!("noti_rev_{}_{}_{}", tarea.id, u.email, ahora),
                        tipo: TipoNotificacion::TareaARevision,
                        para: u.email,
                        titulo: "Una tarea pasó a revisión".into(),
                        cuerpo: format!("\"{}\" ({}) pasó a revisión.", tarea.titulo, tarea.id),
                        leida: false,
                        fecha: ahora.to_string(),
                        tarea_id: Some(tarea.id.clone()),
                        accionable: false,
                    },
                )?;
            }
        }
    }

    Ok(aviso_prestamos)
}

/// Comprueba si un evento `tomar_tarea` entrante (`evento_id`, `evento`) choca
/// contra una toma previa de otra persona sobre la misma tarea (#97).
/// Determina quién ganó y quién perdió según el orden `(timestamp, usuario, archivo)`.
pub fn detectar_doble_toma(
    c: &Connection,
    ops: &Path,
    evento_id: &str,
    evento: &Evento,
) -> Resultado<Option<InfoDobleToma>> {
    if evento.tipo != "tomar_tarea" {
        return Ok(None);
    }
    let Ok(entrante) = serde_json::from_value::<PayloadTomarTarea>(evento.payload.clone()) else {
        return Ok(None);
    };
    let Ok(tarea) = db_tareas::obtener(c, &entrante.id) else {
        return Ok(None);
    };
    if tarea.eliminada || tarea.estado != EstadoTarea::PorHacer {
        return Ok(None);
    }
    let Some(ref actual_email) = tarea.asignado_a else {
        return Ok(None);
    };
    let entrante_email = normalizar_email(&evento.usuario);
    if normalizar_email(&entrante.asignado_a) != entrante_email
        || normalizar_email(actual_email) == entrante_email
    {
        return Ok(None);
    }
    let rama_actual = tarea.rama_asociada.clone().unwrap_or_default();
    let rama_entrante = match validar_ruta_rama_tarea(&entrante.rama_asociada) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    // Buscamos en `ops/` el evento con el que se asignó `actual_email` para comparar
    // orden determinista `(timestamp, usuario, id)` incluso si la máquina local
    // aplicó su propio evento antes de recibir el remoto.
    let actual_en_ops = buscar_evento_toma_en_ops(ops, &tarea.id, actual_email);
    let entrante_gana = match &actual_en_ops {
        Some((id_actual, ev_actual)) => {
            (&evento.timestamp, &entrante_email, evento_id)
                < (
                    &ev_actual.timestamp,
                    &normalizar_email(&ev_actual.usuario),
                    id_actual.as_str(),
                )
        }
        None => false,
    };

    if entrante_gana {
        let (id_actual, ev_actual) = actual_en_ops.unwrap();
        let payload_actual = serde_json::from_value::<PayloadTomarTarea>(ev_actual.payload).ok();
        let cs_inicial_perdedor = payload_actual
            .as_ref()
            .and_then(|p| p.ultimo_changeset_visto)
            .or(tarea.ultimo_changeset_visto);
        Ok(Some(InfoDobleToma {
            tarea_id: tarea.id,
            titulo: tarea.titulo,
            ganador: entrante_email,
            rama_ganador: rama_entrante,
            perdedor: normalizar_email(actual_email),
            rama_perdedor: rama_actual,
            cs_inicial_perdedor,
            evento_perdedor_id: id_actual,
            revertir_a_entrante: Some(entrante),
        }))
    } else {
        Ok(Some(InfoDobleToma {
            tarea_id: tarea.id,
            titulo: tarea.titulo,
            ganador: normalizar_email(actual_email),
            rama_ganador: rama_actual,
            perdedor: entrante_email,
            rama_perdedor: rama_entrante,
            cs_inicial_perdedor: entrante.ultimo_changeset_visto,
            evento_perdedor_id: evento_id.to_string(),
            revertir_a_entrante: None,
        }))
    }
}

fn buscar_evento_toma_en_ops(
    ops: &Path,
    tarea_id: &str,
    usuario: &str,
) -> Option<(String, Evento)> {
    let usuario = normalizar_email(usuario);
    let entradas = std::fs::read_dir(ops).ok()?;
    let mut mejor: Option<(String, Evento)> = None;
    for entrada in entradas.flatten() {
        let nombre = entrada.file_name().to_string_lossy().to_string();
        if nombre.starts_with('.') || !nombre.ends_with(".json") {
            continue;
        }
        let Ok(texto) = std::fs::read_to_string(entrada.path()) else {
            continue;
        };
        let Ok(ev) = serde_json::from_str::<Evento>(&texto) else {
            continue;
        };
        if ev.tipo == "tomar_tarea"
            && ev.tarea_id.as_deref() == Some(tarea_id)
            && normalizar_email(&ev.usuario) == usuario
        {
            let reemplazar = match &mejor {
                None => true,
                Some((m_id, m_ev)) => (&ev.timestamp, &nombre) < (&m_ev.timestamp, m_id),
            };
            if reemplazar {
                mejor = Some((nombre, ev));
            }
        }
    }
    mejor
}

fn insertar_notificacion_si_falta(c: &Connection, n: &Notificacion) -> Resultado<()> {
    if notificaciones::buscar(c, &n.id)?.is_none() {
        notificaciones::insertar(c, n)?;
    }
    Ok(())
}

/// Resuelve una colisión de doble toma (#97) una vez comprobado si la rama del
/// perdedor ya tiene changesets reales:
/// - Sin changesets: revierte la toma al ganador (si hace falta) y notifica al
///   perdedor con la acción `"Volver mi workspace a dev"`.
/// - Con changesets: no revierte ni borra; avisa a ambos y a cada Líder/Admin.
///
/// Devuelve `true` si el evento entrante se aplicó como ganador (reemplazando la
/// toma local previa), o `false` si se conservó al ganador actual.
pub fn aplicar_resolucion_doble_toma(
    c: &Connection,
    publica_admin: &str,
    info: &InfoDobleToma,
    tiene_changesets: bool,
    ahora: &str,
) -> Resultado<bool> {
    if tiene_changesets {
        let mut destinatarios = vec![info.ganador.clone(), info.perdedor.clone()];
        let usuarios = if publica_admin.is_empty() {
            usuarios::listar(c)?
                .into_iter()
                .filter(|u| u.rol.es_lider_o_admin())
                .map(|u| u.email)
                .collect::<Vec<_>>()
        } else {
            verificar_usuarios(publica_admin, usuarios::listar(c)?)
                .into_iter()
                .filter(|u| {
                    u.verificacion == Verificacion::Verificado
                        && u.rol_efectivo.is_some_and(|r| r.es_lider_o_admin())
                })
                .map(|u| u.usuario.email)
                .collect::<Vec<_>>()
        };
        for correo in usuarios {
            if !destinatarios.contains(&correo) {
                destinatarios.push(correo);
            }
        }
        let titulo = format!("Doble toma con cambios en {}", info.tarea_id);
        let cuerpo = format!(
            "{} y {} tomaron {} ({}) casi a la vez y ya hay check-ins en {}. Decidan manualmente cómo resolverlo.",
            info.ganador, info.perdedor, info.tarea_id, info.titulo, info.rama_perdedor
        );
        for para in destinatarios {
            let n = Notificacion {
                id: format!("N-doble-cs-{}-{para}", info.evento_perdedor_id),
                tipo: TipoNotificacion::Advertencia,
                para: para.clone(),
                titulo: titulo.clone(),
                cuerpo: cuerpo.clone(),
                leida: false,
                fecha: ahora.to_string(),
                tarea_id: Some(info.tarea_id.clone()),
                accionable: false,
            };
            insertar_notificacion_si_falta(c, &n)?;
        }
        return Ok(false);
    }

    let mut aplicado_entrante = false;
    if let Some(ref ganador_payload) = info.revertir_a_entrante {
        let mut tarea = db_tareas::obtener(c, &info.tarea_id)?;
        tarea.asignado_a = Some(normalizar_email(&ganador_payload.asignado_a));
        tarea.modo_trabajo = Some(MODO_RAMA.to_string());
        tarea.rama_asociada = Some(validar_ruta_rama_tarea(&ganador_payload.rama_asociada)?);
        tarea.ultimo_changeset_visto = ganador_payload.ultimo_changeset_visto;
        db_tareas::actualizar(c, &tarea)?;

        for anc in &ganador_payload.ramas_ancestros {
            if let Ok(mut fila_anc) = db_tareas::obtener(c, &anc.id) {
                if !fila_anc.eliminada && fila_anc.rama_asociada.is_none() {
                    fila_anc.rama_asociada = Some(validar_ruta_rama_tarea(&anc.rama_asociada)?);
                    fila_anc.modo_trabajo = Some(MODO_RAMA.to_string());
                    if fila_anc.ultimo_changeset_visto.is_none() {
                        fila_anc.ultimo_changeset_visto = anc.ultimo_changeset_visto;
                    }
                    db_tareas::actualizar(c, &fila_anc)?;
                }
            }
        }
        aplicado_entrante = true;
    }

    let n = Notificacion {
        id: format!("N-doble-{}-{}", info.evento_perdedor_id, info.perdedor),
        tipo: TipoNotificacion::Advertencia,
        para: info.perdedor.clone(),
        titulo: format!("Tu toma de {} fue revertida", info.tarea_id),
        cuerpo: format!(
            "{} se adelantó al tomar {} ({}). Tu rama fue cerrada; usa \"Volver mi workspace a dev\".",
            info.ganador, info.tarea_id, info.titulo
        ),
        leida: false,
        fecha: ahora.to_string(),
        tarea_id: Some(info.tarea_id.clone()),
        accionable: true,
    };
    insertar_notificacion_si_falta(c, &n)?;
    Ok(aplicado_entrante)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::eventos::lectura::{self, sincronizar};
    use crate::identidad::equipo::{self, Firmante};
    use crate::identidad::firmas;
    use crate::identidad::llaves::{
        cargar_llave_admin, fijar_llave_publica_admin, generar_llaves_admin, LlavePrivada,
    };
    use chrono::{DateTime, Utc};
    use std::path::PathBuf;

    const ADMIN: &str = "admin@virustf.dev";
    const LIDER: &str = "lider@virustf.dev";
    const DEV: &str = "dev@virustf.dev";
    const TESTER: &str = "tester@virustf.dev";
    const COLAB: &str = "colab@virustf.dev";
    const AHORA: &str = "2026-11-15T15:00:00.000Z";
    const HOY: &str = "2026-11-15";

    struct Escenario {
        _dir: tempfile::TempDir,
        ops: PathBuf,
        local_b: PathBuf,
        publica: String,
        llave_admin: LlavePrivada,
        db_a: Db,
        db_b: Db,
    }

    impl Escenario {
        fn nuevo() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let ops = dir.path().join("ops");
            let local_a = dir.path().join("local_a");
            let local_b = dir.path().join("local_b");
            let publica = generar_llaves_admin(&local_a, None).unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let llave_admin = cargar_llave_admin(&local_a, None).unwrap();

            let db_a = Db::en_memoria().unwrap();
            let db_b = Db::en_memoria().unwrap();
            for db in [&db_a, &db_b] {
                let c = db.conexion();
                usuarios::insertar(
                    c,
                    &usuarios::Usuario {
                        email: ADMIN.into(),
                        nombre: "Admin".into(),
                        apellido: "Uno".into(),
                        rol: Rol::Administrador,
                        fecha_alta: HOY.into(),
                        firma: Some(
                            firmas::firmar_rol(&llave_admin, ADMIN, Rol::Administrador, AHORA)
                                .unwrap(),
                        ),
                        firmado_en: Some(AHORA.into()),
                        firmado_por: Some(ADMIN.into()),
                        llave_publica: None,
                        certificado: None,
                    },
                )
                .unwrap();
            }
            let esc = Self {
                _dir: dir,
                ops,
                local_b,
                publica,
                llave_admin,
                db_a,
                db_b,
            };
            let firmante = Firmante {
                email: ADMIN,
                rol: Rol::Administrador,
                llave: &esc.llave_admin,
            };
            for (correo, rol) in [
                (LIDER, Rol::Lider),
                (DEV, Rol::Desarrollador),
                (TESTER, Rol::Tester),
                (COLAB, Rol::ColaboradorExterno),
            ] {
                equipo::crear(
                    esc.db_a.conexion(),
                    &esc.ops,
                    &esc.publica,
                    &firmante,
                    correo,
                    rol,
                    AHORA,
                    HOY,
                )
                .unwrap();
            }
            esc.sincronizar_b();
            esc
        }

        fn sincronizar_b(&self) -> lectura::ResumenSincronizacion {
            let ahora = DateTime::parse_from_rfc3339(AHORA)
                .unwrap()
                .with_timezone(&Utc);
            sincronizar(
                self.db_b.conexion(),
                &self.ops,
                &self.local_b,
                ahora,
                |c, e| lectura::aplicar_evento(c, &self.publica, e),
            )
            .unwrap()
        }
    }

    fn datos_base(titulo: &str) -> DatosNuevaTarea {
        DatosNuevaTarea {
            titulo: titulo.into(),
            detalles: "Detalles iniciales".into(),
            tipo: TipoTarea::Scripting,
            tarea_padre_id: None,
            sprint_id: Some("S-1".into()),
            backlog_id: Some("B-1".into()),
            con_fecha: true,
            fecha_limite: Some("2026-12-01".into()),
        }
    }

    #[test]
    fn crear_tarea_deja_pendiente_segun_el_rol_y_viaja_por_ops() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();

        // Líder y Administrador: aprobada de entrada (`pendiente_aprobacion = false`).
        let t_admin = crear(
            c,
            &e.ops,
            &AutorTarea {
                email: ADMIN,
                rol: Rol::Administrador,
            },
            &datos_base("Tarea de Admin"),
            "2026-11-15T15:00:01.000Z",
            HOY,
        )
        .unwrap();
        assert_eq!(t_admin.id, "T-101");
        assert!(!t_admin.pendiente_aprobacion);

        let t_lider = crear(
            c,
            &e.ops,
            &AutorTarea {
                email: LIDER,
                rol: Rol::Lider,
            },
            &datos_base("Tarea de Líder"),
            "2026-11-15T15:00:02.000Z",
            HOY,
        )
        .unwrap();
        assert_eq!(t_lider.id, "T-102");
        assert!(!t_lider.pendiente_aprobacion);

        // Desarrollador, Tester y Colaborador externo: nace con `pendiente_aprobacion = true`.
        for (esperado_id, correo, rol, ts) in [
            ("T-103", DEV, Rol::Desarrollador, "2026-11-15T15:00:03.000Z"),
            ("T-104", TESTER, Rol::Tester, "2026-11-15T15:00:04.000Z"),
            (
                "T-105",
                COLAB,
                Rol::ColaboradorExterno,
                "2026-11-15T15:00:05.000Z",
            ),
        ] {
            let t = crear(
                c,
                &e.ops,
                &AutorTarea { email: correo, rol },
                &datos_base(&format!("Tarea de {correo}")),
                ts,
                HOY,
            )
            .unwrap();
            assert_eq!(t.id, esperado_id);
            assert!(t.pendiente_aprobacion, "{rol:?} debe quedar pendiente");
            assert_eq!(t.creado_por, correo);
        }

        // B sincroniza y llega exactamente a la misma lista.
        let r = e.sincronizar_b();
        assert_eq!(r.aplicados, 5);
        assert_eq!(
            listar(e.db_b.conexion()).unwrap(),
            listar(e.db_a.conexion()).unwrap()
        );
    }

    #[test]
    fn crear_tarea_con_subtareas_encadena_ids_y_valida_niveles() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };

        let creadas = crear_con_subtareas(
            c,
            &e.ops,
            &dev,
            &DatosNuevaTareaConSubtareas {
                titulo: "Sistema de inventario".into(),
                detalles: "Con arrastrar y soltar".into(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: Some("S-1".into()),
                backlog_id: Some("B-1".into()),
                con_fecha: false,
                fecha_limite: Some("2026-12-01".into()),
                subtareas: vec![
                    DatosSubtareaNueva {
                        tipo: TipoTarea::UiInterfaz,
                        titulo: "Diseño de casillas".into(),
                        detalles: "Grid 4x5".into(),
                    },
                    DatosSubtareaNueva {
                        tipo: TipoTarea::Audio,
                        titulo: "Sonido al equipar".into(),
                        detalles: String::new(),
                    },
                ],
            },
            "2026-11-15T15:00:01.000Z",
            HOY,
        )
        .unwrap();

        assert_eq!(creadas.len(), 3);
        assert_eq!(creadas[0].id, "T-101");
        assert_eq!(
            creadas[0].fecha_limite, None,
            "con_fecha=false limpia fecha_limite"
        );
        assert!(creadas[0].pendiente_aprobacion);
        assert_eq!(creadas[1].id, "T-101-1");
        assert_eq!(creadas[1].tarea_padre_id.as_deref(), Some("T-101"));
        assert_eq!(creadas[1].sprint_id.as_deref(), Some("S-1"));
        assert_eq!(creadas[1].backlog_id.as_deref(), Some("B-1"));
        assert!(creadas[1].pendiente_aprobacion);
        assert_eq!(creadas[2].id, "T-101-2");

        // Subtarea de subtarea (`T-101-1-1`, 5° nivel en Unity VC): permitida.
        let nieto = crear(
            c,
            &e.ops,
            &dev,
            &DatosNuevaTarea {
                tarea_padre_id: Some("T-101-1".into()),
                ..datos_base("Icono de casilla activa")
            },
            "2026-11-15T15:00:02.000Z",
            HOY,
        )
        .unwrap();
        assert_eq!(nieto.id, "T-101-1-1");

        // Un nivel más (`T-101-1-1-1`, 6° nivel) se rechaza.
        let err = crear(
            c,
            &e.ops,
            &dev,
            &DatosNuevaTarea {
                tarea_padre_id: Some("T-101-1-1".into()),
                ..datos_base("Demasiado profundo")
            },
            AHORA,
            HOY,
        )
        .unwrap_err();
        assert_eq!(err.codigo(), "validacion");

        // B sincroniza el evento compuesto y el individual.
        let r = e.sincronizar_b();
        assert_eq!(r.aplicados, 2);
        assert_eq!(
            listar(e.db_b.conexion()).unwrap(),
            listar(e.db_a.conexion()).unwrap()
        );
    }

    #[test]
    fn editar_tarea_permite_al_creador_sobre_su_pendiente_y_a_lider_o_admin() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let tester = AutorTarea {
            email: TESTER,
            rol: Rol::Tester,
        };
        let lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };

        let t = crear(c, &e.ops, &dev, &datos_base(" Combate v1 "), AHORA, HOY).unwrap();
        assert!(t.pendiente_aprobacion);

        // El creador puede editar su tarea mientras sigue pendiente.
        let editada = editar(
            c,
            &e.ops,
            &dev,
            &t.id,
            &CambiosTarea {
                titulo: Some("Combate v2".into()),
                detalles: Some("Ajustado por el creador".into()),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap();
        assert_eq!(editada.titulo, "Combate v2");
        assert_eq!(editada.detalles, "Ajustado por el creador");

        // Otro Desarrollador/Tester NO puede editar la tarea ajena.
        let err = editar(
            c,
            &e.ops,
            &tester,
            &t.id,
            &CambiosTarea {
                titulo: Some("Intento ajeno".into()),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap_err();
        assert_eq!(err.codigo(), "sin_permiso");

        // No se puede cambiar directamente de un sprint a otro sin quitarla primero.
        let err_sprint = editar(
            c,
            &e.ops,
            &lider,
            &t.id,
            &CambiosTarea {
                sprint_id: Some(Some("S-2".into())),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap_err();
        assert_eq!(err_sprint.codigo(), "validacion");

        // Quitarla del sprint (`Some(None)`) y luego asignarla a otro sí vale.
        editar(
            c,
            &e.ops,
            &lider,
            &t.id,
            &CambiosTarea {
                sprint_id: Some(None),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap();
        let con_s2 = editar(
            c,
            &e.ops,
            &lider,
            &t.id,
            &CambiosTarea {
                sprint_id: Some(Some("S-2".into())),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap();
        assert_eq!(con_s2.sprint_id.as_deref(), Some("S-2"));

        // Cuando la tarea ya NO está pendiente de aprobación, el creador ya no
        // puede usar `editar_tarea` directo (tendrá que usar `proponer_edicion`, #102);
        // un Líder/Admin sí puede.
        let mut aprobada = con_s2;
        aprobada.pendiente_aprobacion = false;
        db_tareas::actualizar(c, &aprobada).unwrap();

        let err_creador = editar(
            c,
            &e.ops,
            &dev,
            &t.id,
            &CambiosTarea {
                titulo: Some("Cambio directo tras aprobar".into()),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap_err();
        assert_eq!(err_creador.codigo(), "sin_permiso");

        let por_lider = editar(
            c,
            &e.ops,
            &lider,
            &t.id,
            &CambiosTarea {
                titulo: Some("Ajuste del Líder".into()),
                ..Default::default()
            },
            AHORA,
        )
        .unwrap();
        assert_eq!(por_lider.titulo, "Ajuste del Líder");
    }

    fn simular_ramas_cadena(cadena: &[TareaDeCadena]) -> Resultado<Vec<(String, Rama)>> {
        let mut padre: Option<String> = None;
        let mut creadas = Vec::new();
        for (i, paso) in cadena.iter().enumerate() {
            let ruta = match paso.rama {
                Some(ref r) => r.clone(),
                None => {
                    let r = crate::ramas::ruta_rama_nueva(
                        padre.as_deref(),
                        &crate::ramas::nombre_rama(&paso.id, &paso.titulo)?,
                    )?;
                    creadas.push((
                        paso.id.clone(),
                        Rama {
                            id: 100 + i as i64,
                            name: r.clone(),
                            head_changeset: 50 + i as i64,
                        },
                    ));
                    r
                }
            };
            padre = Some(ruta);
        }
        Ok(creadas)
    }

    #[test]
    fn tomar_subtarea_crea_rama_del_padre_sin_asignarlo_y_viaja_por_ops() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let tester = AutorTarea {
            email: TESTER,
            rol: Rol::Tester,
        };

        // Crea T-101 con dos subtareas T-101-1 y T-101-2 (ninguna tiene rama todavía).
        crear_con_subtareas(
            c,
            &e.ops,
            &dev,
            &DatosNuevaTareaConSubtareas {
                titulo: "IA de enemigo".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![
                    DatosSubtareaNueva {
                        tipo: TipoTarea::Animacion,
                        titulo: "Caminar".into(),
                        detalles: String::new(),
                    },
                    DatosSubtareaNueva {
                        tipo: TipoTarea::Audio,
                        titulo: "Rugido".into(),
                        detalles: String::new(),
                    },
                ],
            },
            "2026-11-15T15:00:01.000Z",
            HOY,
        )
        .unwrap();

        // Dev toma T-101-1: T-101 recibe `rama_asociada` y `modo_trabajo = Rama`, pero NO responsable;
        // T-101-1 recibe además `asignado_a = dev`.
        let actualizadas = tomar(
            c,
            &e.ops,
            &dev,
            "T-101-1",
            simular_ramas_cadena,
            "2026-11-15T15:00:02.000Z",
        )
        .unwrap();
        assert_eq!(actualizadas.len(), 2);
        let padre = &actualizadas[0];
        let hija = &actualizadas[1];
        assert_eq!(padre.id, "T-101");
        assert_eq!(
            padre.rama_asociada.as_deref(),
            Some("/main/dev/T-101-ia-de-enemigo")
        );
        assert_eq!(padre.modo_trabajo.as_deref(), Some(MODO_RAMA));
        assert_eq!(padre.asignado_a, None, "el padre no recibe responsable");
        assert_eq!(padre.ultimo_changeset_visto, Some(50));

        assert_eq!(hija.id, "T-101-1");
        assert_eq!(hija.asignado_a.as_deref(), Some(DEV));
        assert_eq!(hija.modo_trabajo.as_deref(), Some(MODO_RAMA));
        assert_eq!(
            hija.rama_asociada.as_deref(),
            Some("/main/dev/T-101-ia-de-enemigo/T-101-1-caminar")
        );
        assert_eq!(hija.ultimo_changeset_visto, Some(51));

        // Intentar volver a tomar T-101-1 ya tomada se rechaza.
        let err = tomar(
            c,
            &e.ops,
            &tester,
            "T-101-1",
            simular_ramas_cadena,
            "2026-11-15T15:00:03.000Z",
        )
        .unwrap_err();
        assert_eq!(err.codigo(), "validacion");

        // Tester toma T-101-2: el padre ya tiene rama, solo se crea la de T-101-2.
        let segunda = tomar(
            c,
            &e.ops,
            &tester,
            "T-101-2",
            simular_ramas_cadena,
            "2026-11-15T15:00:04.000Z",
        )
        .unwrap();
        assert_eq!(segunda.len(), 2);
        assert_eq!(segunda[1].asignado_a.as_deref(), Some(TESTER));
        assert_eq!(
            segunda[1].rama_asociada.as_deref(),
            Some("/main/dev/T-101-ia-de-enemigo/T-101-2-rugido")
        );

        // Máquina B sincroniza y llega exactamente al mismo estado.
        let r = e.sincronizar_b();
        assert_eq!(r.aplicados, 3);
        assert_eq!(r.descartados, 0);
        assert_eq!(
            listar(e.db_b.conexion()).unwrap(),
            listar(e.db_a.conexion()).unwrap()
        );
    }

    #[test]
    fn colaborador_externo_solo_toma_tareas_individuales() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };
        let colab = AutorTarea {
            email: COLAB,
            rol: Rol::ColaboradorExterno,
        };

        // T-101 con subtarea T-101-1, y T-102 individual.
        crear_con_subtareas(
            c,
            &e.ops,
            &lider,
            &DatosNuevaTareaConSubtareas {
                titulo: "Tarea grande".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![DatosSubtareaNueva {
                    tipo: TipoTarea::Audio,
                    titulo: "Subtarea".into(),
                    detalles: String::new(),
                }],
            },
            "2026-11-15T15:00:01.000Z",
            HOY,
        )
        .unwrap();
        crear(
            c,
            &e.ops,
            &lider,
            &datos_base("Modelo 3D suelto"),
            "2026-11-15T15:00:02.000Z",
            HOY,
        )
        .unwrap();

        // Colaborador externo NO puede tomar la tarea con subtareas (T-101) ni la subtarea (T-101-1).
        for id in ["T-101", "T-101-1"] {
            let err = tomar(
                c,
                &e.ops,
                &colab,
                id,
                simular_ramas_cadena,
                "2026-11-15T15:00:03.000Z",
            )
            .unwrap_err();
            assert_eq!(err.codigo(), "validacion");
            assert!(err.to_string().contains("individuales"), "{err}");
        }

        // Sí puede tomar la tarea individual (T-102).
        let tomada = tomar(
            c,
            &e.ops,
            &colab,
            "T-102",
            simular_ramas_cadena,
            "2026-11-15T15:00:04.000Z",
        )
        .unwrap();
        assert_eq!(tomada.len(), 1);
        assert_eq!(tomada[0].asignado_a.as_deref(), Some(COLAB));

        // Si alguien suelta a mano en `ops/` un evento `tomar_tarea` de Colaborador externo
        // sobre una subtarea, al sincronizar se descarta.
        let mut falso = Evento::nuevo(
            "tomar_tarea",
            Some("T-101-1"),
            COLAB,
            serde_json::to_value(PayloadTomarTarea {
                id: "T-101-1".into(),
                asignado_a: COLAB.into(),
                rama_asociada: "/main/dev/T-101-tarea-grande/T-101-1-subtarea".into(),
                ultimo_changeset_visto: Some(10),
                ramas_ancestros: vec![RamaAncestro {
                    id: "T-101".into(),
                    rama_asociada: "/main/dev/T-101-tarea-grande".into(),
                    ultimo_changeset_visto: Some(9),
                }],
            })
            .unwrap(),
        );
        falso.timestamp = "2026-11-15T15:00:05.000Z".into();
        eventos::escribir_evento(&e.ops, &falso).unwrap();

        let r = e.sincronizar_b();
        assert_eq!(r.aplicados, 3);
        assert_eq!(r.descartados, 1, "el evento falso se descarta");
        assert_eq!(
            db_tareas::obtener(e.db_b.conexion(), "T-101-1")
                .unwrap()
                .asignado_a,
            None
        );
    }

    #[test]
    fn doble_toma_sin_changesets_gana_el_primero_y_avisa_al_perdedor() {
        let e = Escenario::nuevo();
        let dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let tester = AutorTarea {
            email: TESTER,
            rol: Rol::Tester,
        };

        // A crea T-101 y B la recibe al sincronizar.
        crear(
            e.db_a.conexion(),
            &e.ops,
            &dev,
            &datos_base("Colisión"),
            "2026-11-15T15:00:01.000Z",
            HOY,
        )
        .unwrap();
        assert_eq!(e.sincronizar_b().aplicados, 1);

        // B toma T-101 en su máquina con timestamp posterior (15:00:05), antes de recibir a A.
        tomar(
            e.db_b.conexion(),
            &e.ops,
            &tester,
            "T-101",
            |_| {
                Ok(vec![(
                    "T-101".into(),
                    Rama {
                        id: 202,
                        name: "/main/dev/T-101-colision-b".into(),
                        head_changeset: 50,
                    },
                )])
            },
            "2026-11-15T15:00:05.000Z",
        )
        .unwrap();

        // A toma T-101 con timestamp anterior (15:00:02) → A es el ganador real.
        tomar(
            e.db_a.conexion(),
            &e.ops,
            &dev,
            "T-101",
            |_| {
                Ok(vec![(
                    "T-101".into(),
                    Rama {
                        id: 201,
                        name: "/main/dev/T-101-colision-a".into(),
                        head_changeset: 50,
                    },
                )])
            },
            "2026-11-15T15:00:02.000Z",
        )
        .unwrap();

        // Ahora ambas máquinas sincronizan (sin changesets reales en la rama perdedora).
        let ahora = DateTime::parse_from_rfc3339("2026-11-15T15:10:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_a = lectura::sincronizar_con_doble_toma(
            e.db_a.conexion(),
            &e.ops,
            &e.local_b,
            ahora,
            &e.publica,
            |c, ev| lectura::aplicar_evento(c, &e.publica, ev),
            |_| Ok(false),
        )
        .unwrap();
        let sync_b = lectura::sincronizar_con_doble_toma(
            e.db_b.conexion(),
            &e.ops,
            &e.local_b,
            ahora,
            &e.publica,
            |c, ev| lectura::aplicar_evento(c, &e.publica, ev),
            |_| Ok(false),
        )
        .unwrap();

        assert_eq!(sync_a.descartados, 1);
        assert_eq!(sync_b.aplicados, 1, "B revierte su toma local a A");

        // Ambas máquinas convergen al ganador (DEV) y su rama.
        for db in [&e.db_a, &e.db_b] {
            let t = db_tareas::obtener(db.conexion(), "T-101").unwrap();
            assert_eq!(t.asignado_a.as_deref(), Some(DEV));
            assert_eq!(
                t.rama_asociada.as_deref(),
                Some("/main/dev/T-101-colision-a")
            );
            let notifs_b = notificaciones::de_usuario(db.conexion(), TESTER).unwrap();
            assert_eq!(notifs_b.len(), 1);
            assert!(notifs_b[0].accionable);
            assert!(notifs_b[0].cuerpo.contains("Volver mi workspace a dev"));
            assert!(notificaciones::de_usuario(db.conexion(), DEV)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn doble_toma_con_changesets_no_revierte_y_avisa_a_ambos_y_lider_admin() {
        let e = Escenario::nuevo();
        let dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let tester = AutorTarea {
            email: TESTER,
            rol: Rol::Tester,
        };

        crear(
            e.db_a.conexion(),
            &e.ops,
            &dev,
            &datos_base("Colisión con commits"),
            "2026-11-15T15:00:01.000Z",
            HOY,
        )
        .unwrap();
        assert_eq!(e.sincronizar_b().aplicados, 1);

        tomar(
            e.db_b.conexion(),
            &e.ops,
            &tester,
            "T-101",
            |_| {
                Ok(vec![(
                    "T-101".into(),
                    Rama {
                        id: 302,
                        name: "/main/dev/T-101-con-commits-b".into(),
                        head_changeset: 50,
                    },
                )])
            },
            "2026-11-15T15:00:05.000Z",
        )
        .unwrap();

        tomar(
            e.db_a.conexion(),
            &e.ops,
            &dev,
            "T-101",
            |_| {
                Ok(vec![(
                    "T-101".into(),
                    Rama {
                        id: 301,
                        name: "/main/dev/T-101-con-commits-a".into(),
                        head_changeset: 50,
                    },
                )])
            },
            "2026-11-15T15:00:02.000Z",
        )
        .unwrap();

        let ahora = DateTime::parse_from_rfc3339("2026-11-15T15:10:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync_b = lectura::sincronizar_con_doble_toma(
            e.db_b.conexion(),
            &e.ops,
            &e.local_b,
            ahora,
            &e.publica,
            |c, ev| lectura::aplicar_evento(c, &e.publica, ev),
            |_| Ok(true),
        )
        .unwrap();
        assert_eq!(sync_b.aplicados, 0, "con changesets no revierte en local");
        assert_eq!(sync_b.descartados, 1);

        // No se revirtió en B.
        assert_eq!(
            db_tareas::obtener(e.db_b.conexion(), "T-101")
                .unwrap()
                .asignado_a
                .as_deref(),
            Some(TESTER)
        );
        // Se avisó a ambos, al Líder y al Administrador (no al Colaborador externo).
        for correo in [DEV, TESTER, LIDER, ADMIN] {
            let notifs = notificaciones::de_usuario(e.db_b.conexion(), correo).unwrap();
            assert_eq!(notifs.len(), 1, "falta notificación para {correo}");
            assert!(!notifs[0].accionable);
            assert!(notifs[0].cuerpo.contains("T-101"));
        }
        assert!(notificaciones::de_usuario(e.db_b.conexion(), COLAB)
            .unwrap()
            .is_empty());
    }

    /// **Prueba en el entorno real** (CLAUDE.md): entra de verdad al repositorio
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` en un workspace
    /// temporal (`cm workspace create` + `entrar_en`), configura una subcarpeta
    /// en OneDrive/Drive (`BB_PRUEBA_CARPETA_COMPARTIDA`), da de alta a un
    /// Desarrollador y un Líder, crea una tarea general con subtareas (como
    /// Desarrollador → nace pendiente de aprobación), la edita siendo su creador,
    /// el Líder crea otra tarea (nace aprobada), y una segunda máquina B sincroniza
    /// desde OneDrive y `listar` devuelve exactamente las mismas tareas en ambas
    /// máquinas. Al terminar borra el workspace temporal y la subcarpeta de OneDrive.
    ///
    /// ```text
    /// BB_PRUEBA_ORG=MiguelM17 BB_PRUEBA_REPO=BranchBoard-Pruebas/BranchBoard-Pruebas \
    ///   BB_PRUEBA_CARPETA_COMPARTIDA=C:\Users\<tú>\OneDrive \
    ///   cargo test --lib entorno_real_crear_editar_y_listar_tareas -- --ignored --nocapture --test-threads=1
    /// ```
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_crear_editar_y_listar_tareas_por_ops() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::estado::{llave_publica_del_proyecto, AppState};
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-95-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-95-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Máquina A entra de verdad a BranchBoard-Pruebas como Administrador dueño del repo.
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let (publica, admin_fila) = estado_a
                .con_db(|db| {
                    let pub_k = llave_publica_del_proyecto(db)?;
                    let todos = usuarios::listar(db.conexion())?;
                    Ok((pub_k, todos.into_iter().next().unwrap()))
                })
                .unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let llave_admin = cargar_llave_admin(&local_a, None).unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            estado_a
                .con_db(|db_a| {
                    let firmante_admin = Firmante {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                        llave: &llave_admin,
                    };
                    equipo::crear(
                        db_a.conexion(),
                        &ops_a,
                        &publica,
                        &firmante_admin,
                        DEV,
                        Rol::Desarrollador,
                        "2026-11-15T15:00:01.000Z",
                        HOY,
                    )?;
                    equipo::crear(
                        db_a.conexion(),
                        &ops_a,
                        &publica,
                        &firmante_admin,
                        LIDER,
                        Rol::Lider,
                        "2026-11-15T15:00:02.000Z",
                        HOY,
                    )?;

                    // 1. El Desarrollador crea una tarea general con 2 subtareas (quedan pendientes).
                    let creadas = crear_con_subtareas(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: DEV,
                            rol: Rol::Desarrollador,
                        },
                        &DatosNuevaTareaConSubtareas {
                            titulo: "Menú principal".into(),
                            detalles: "Pantalla de inicio".into(),
                            tipo: TipoTarea::UiInterfaz,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                            subtareas: vec![
                                DatosSubtareaNueva {
                                    tipo: TipoTarea::UiInterfaz,
                                    titulo: "Botones".into(),
                                    detalles: String::new(),
                                },
                                DatosSubtareaNueva {
                                    tipo: TipoTarea::Audio,
                                    titulo: "Música de fondo".into(),
                                    detalles: String::new(),
                                },
                            ],
                        },
                        "2026-11-15T15:00:03.000Z",
                        HOY,
                    )?;
                    assert_eq!(creadas.len(), 3);
                    assert!(creadas.iter().all(|t| t.pendiente_aprobacion));

                    // 2. El mismo Desarrollador edita su tarea pendiente.
                    let editada = editar(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: DEV,
                            rol: Rol::Desarrollador,
                        },
                        "T-101",
                        &CambiosTarea {
                            titulo: Some("Menú principal definitivo".into()),
                            detalles: Some("Con opciones de audio y video".into()),
                            ..Default::default()
                        },
                        "2026-11-15T15:00:04.000Z",
                    )?;
                    assert_eq!(editada.titulo, "Menú principal definitivo");

                    // 3. El Líder crea una tarea individual (nace aprobada).
                    let del_lider = crear(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: LIDER,
                            rol: Rol::Lider,
                        },
                        &DatosNuevaTarea {
                            titulo: "Iluminación del escenario".into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Modelado3D,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        "2026-11-15T15:00:05.000Z",
                        HOY,
                    )?;
                    assert_eq!(del_lider.id, "T-102");
                    assert!(!del_lider.pendiente_aprobacion);
                    Ok(())
                })
                .unwrap();

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 4. B sincroniza desde la carpeta compartida en OneDrive y obtiene la misma lista.
            let ahora = DateTime::parse_from_rfc3339("2026-11-15T15:05:00.000Z")
                .unwrap()
                .with_timezone(&Utc);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            println!("B sincroniza tareas desde OneDrive: {sync_b:?}");
            assert_eq!(sync_b.aplicados, 5);
            assert_eq!(sync_b.descartados, 0);

            let tareas_a = estado_a.con_db(|db_a| listar(db_a.conexion())).unwrap();
            let tareas_b = listar(db_b.conexion()).unwrap();
            assert_eq!(tareas_b, tareas_a);
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #96): entra al repositorio
    /// real `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity`, crea una
    /// tarea general con una subtarea, llama a `comandos::tareas::tomar_en` sobre
    /// la subtarea (`T-101-1`), verifica que:
    /// 1. Se crearon en Unity VC tanto la rama del padre como la de la subtarea,
    ///    partiendo de la cabeza actual de `/main/dev`, con el comentario armado
    ///    por el backend (`Rama de <id>: <título>`).
    /// 2. El workspace local cambió (`cm switch`) a la rama de la subtarea.
    /// 3. El padre quedó con `rama_asociada` y sin `asignado_a`, y la subtarea con
    ///    `asignado_a`, `modo_trabajo = Rama` y `rama_asociada`.
    /// 4. Una segunda máquina B sincroniza desde OneDrive y obtiene las mismas filas.
    ///
    /// Al terminar borra el workspace, las ramas de prueba en Unity VC y la
    /// subcarpeta de OneDrive.
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_tomar_tarea_crea_rama_cambia_workspace_y_sincroniza() {
        use crate::cm::lectura::rama_actual_workspace;
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::tareas::tomar_en;
        use crate::estado::{llave_publica_del_proyecto, AppState};
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let repo_cm = format!("{repo}@{org}@unity");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-96-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();
        let mut ramas_creadas: Vec<String> = Vec::new();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-96-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo_cm}"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let (publica, admin_fila) = estado_a
                .con_db(|db| {
                    let pub_k = llave_publica_del_proyecto(db)?;
                    let todos = usuarios::listar(db.conexion())?;
                    Ok((pub_k, todos.into_iter().next().unwrap()))
                })
                .unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            // Crea T-101 con subtarea T-101-1 (con sufijo único para la rama en el repo de pruebas).
            estado_a
                .con_db(|db_a| {
                    crear_con_subtareas(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: &admin_fila.email,
                            rol: Rol::Administrador,
                        },
                        &DatosNuevaTareaConSubtareas {
                            titulo: format!("Padre 96 {sufijo}"),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                            subtareas: vec![DatosSubtareaNueva {
                                tipo: TipoTarea::Animacion,
                                titulo: format!("Hija 96 {sufijo}"),
                                detalles: String::new(),
                            }],
                        },
                        "2026-11-15T15:00:01.000Z",
                        HOY,
                    )
                })
                .unwrap();

            // Toma T-101-1 usando el flujo real (`tomar_en`): API REST + `cm switch` + evento en `ops/`.
            let actualizadas = tomar_en(&estado_a, "T-101-1").unwrap();
            assert_eq!(actualizadas.len(), 2);
            let padre = &actualizadas[0];
            let hija = &actualizadas[1];
            let rama_padre = padre.rama_asociada.clone().expect("padre con rama");
            let rama_hija = hija.rama_asociada.clone().expect("hija con rama");
            ramas_creadas.push(rama_padre.clone());
            ramas_creadas.push(rama_hija.clone());

            assert_eq!(padre.asignado_a, None);
            assert_eq!(padre.modo_trabajo.as_deref(), Some(MODO_RAMA));
            assert_eq!(hija.asignado_a.as_deref(), Some(admin_fila.email.as_str()));
            assert_eq!(hija.modo_trabajo.as_deref(), Some(MODO_RAMA));

            // El workspace local quedó en la rama de la subtarea tomada.
            let ws_actual = rama_actual_workspace(&estado_a, ws_a.path()).unwrap();
            println!("workspace tras tomar_en: {}", ws_actual.rama);
            assert_eq!(ws_actual.rama, rama_hija);

            std::thread::sleep(std::time::Duration::from_secs(2));

            // Máquina B sincroniza desde OneDrive y llega al mismo estado.
            let ahora = DateTime::parse_from_rfc3339("2026-11-16T23:59:59.000Z")
                .unwrap()
                .with_timezone(&Utc);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            println!("B sincroniza tomar_tarea desde OneDrive: {sync_b:?}");
            assert_eq!(sync_b.aplicados, 2);
            assert_eq!(sync_b.descartados, 0);
            assert_eq!(
                listar(db_b.conexion()).unwrap(),
                estado_a.con_db(|db_a| listar(db_a.conexion())).unwrap()
            );
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        for rama in ramas_creadas.iter().rev() {
            let salida = Comando::cm()
                .args(["branch", "delete"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .ejecutar(&estado_a)
                .unwrap();
            println!("borrada rama de prueba {rama}: {}", salida.exito());
        }
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #97): contra
    /// `BranchBoard-Pruebas` + `OneDrive`:
    /// 1. Crea dos ramas reales para la misma tarea (ganador y perdedor sin
    ///    changesets). Al sincronizar la doble toma via `revisar_y_cerrar_rama_perdedor`,
    ///    la rama huérfana se borra del servidor, la toma vuelve al ganador y el
    ///    perdedor recibe la notificación `"Volver mi workspace a dev"`.
    /// 2. Crea otras dos ramas, hace un `cm checkin` real en la del perdedor y
    ///    sincroniza: la rama NO se borra, la toma NO se revierte y se avisa a
    ///    ambos y al Administrador.
    ///
    /// Al terminar borra workspace, ramas (con `--delete-changesets`) y carpeta
    /// de OneDrive.
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_doble_toma_resuelve_sin_y_con_changesets() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::eventos::revisar_y_cerrar_rama_perdedor;
        use crate::estado::{llave_publica_del_proyecto, AppState};
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida};
        use crate::ramas::{cambiar_a_rama, crear_rama_tarea, RAMA_DEV};
        use crate::servidor::{conectar, LlaveroSistema};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let repo_cm = format!("{repo}@{org}@unity");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-97-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();
        let mut ramas_creadas: Vec<String> = Vec::new();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-97-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo_cm}"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let (publica, admin_fila) = estado_a
                .con_db(|db| {
                    let pub_k = llave_publica_del_proyecto(db)?;
                    let todos = usuarios::listar(db.conexion())?;
                    Ok((pub_k, todos.into_iter().next().unwrap()))
                })
                .unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let llave_admin = cargar_llave_admin(&local_a, None).unwrap();
            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let cliente = conectar(&estado_a, &LlaveroSistema, &admin_fila.email, &org).unwrap();

            // Alta de DEV en A y creación de T-101 y T-102.
            estado_a
                .con_db(|db_a| {
                    let firmante_admin = Firmante {
                        email: &admin_fila.email,
                        rol: Rol::Administrador,
                        llave: &llave_admin,
                    };
                    equipo::crear(
                        db_a.conexion(),
                        &ops_a,
                        &publica,
                        &firmante_admin,
                        DEV,
                        Rol::Desarrollador,
                        "2026-11-15T15:00:01.000Z",
                        HOY,
                    )?;
                    crear(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: DEV,
                            rol: Rol::Desarrollador,
                        },
                        &datos_base("Doble sin commits"),
                        "2026-11-15T15:00:02.000Z",
                        HOY,
                    )?;
                    crear(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: DEV,
                            rol: Rol::Desarrollador,
                        },
                        &datos_base("Doble con commits"),
                        "2026-11-15T15:00:03.000Z",
                        HOY,
                    )?;
                    Ok(())
                })
                .unwrap();

            // B sincroniza DEV, T-101 y T-102.
            let ahora = DateTime::parse_from_rfc3339("2026-11-16T23:59:59.000Z")
                .unwrap()
                .with_timezone(&Utc);
            let sync_init =
                lectura::sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                    lectura::aplicar_evento(c, &publica, e)
                })
                .unwrap();
            assert_eq!(sync_init.aplicados, 3);

            // --- CASO 1: Sin changesets en la rama del perdedor (T-101) ---
            let rama_g1 =
                crear_rama_tarea(&cliente, &repo, None, "T-101", &format!("gana-{sufijo}"))
                    .unwrap();
            ramas_creadas.push(rama_g1.name.clone());
            let rama_p1 =
                crear_rama_tarea(&cliente, &repo, None, "T-101", &format!("pierde-{sufijo}"))
                    .unwrap();
            ramas_creadas.push(rama_p1.name.clone());

            // B toma T-101 con timestamp posterior (15:00:10).
            tomar(
                db_b.conexion(),
                &ops_b,
                &AutorTarea {
                    email: &admin_fila.email,
                    rol: Rol::Administrador,
                },
                "T-101",
                |_| Ok(vec![("T-101".into(), rama_p1.clone())]),
                "2026-11-15T15:00:10.000Z",
            )
            .unwrap();
            // A (DEV) toma T-101 con timestamp anterior (15:00:05).
            estado_a
                .con_db(|db_a| {
                    tomar(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: DEV,
                            rol: Rol::Desarrollador,
                        },
                        "T-101",
                        |_| Ok(vec![("T-101".into(), rama_g1.clone())]),
                        "2026-11-15T15:00:05.000Z",
                    )
                    .map(|_| ())
                })
                .unwrap();

            std::thread::sleep(std::time::Duration::from_secs(2));
            let sync_b1 = lectura::sincronizar_con_doble_toma(
                db_b.conexion(),
                &ops_b,
                &local_b,
                ahora,
                &publica,
                |c, e| lectura::aplicar_evento(c, &publica, e),
                |info| revisar_y_cerrar_rama_perdedor(&estado_a, &admin_fila.email, info),
            )
            .unwrap();
            println!("Caso 1 (sin changesets) B sync: {sync_b1:?}");
            assert_eq!(sync_b1.aplicados, 1);
            assert!(
                cliente.rama(&repo, &rama_p1.name).unwrap().is_none(),
                "la rama perdedora sin changesets se cerró en Unity VC"
            );
            assert_eq!(
                db_tareas::obtener(db_b.conexion(), "T-101")
                    .unwrap()
                    .asignado_a
                    .as_deref(),
                Some(DEV)
            );
            assert_eq!(
                notificaciones::de_usuario(db_b.conexion(), &admin_fila.email)
                    .unwrap()
                    .len(),
                1
            );

            // --- CASO 2: Con changeset real en la rama del perdedor (T-102) ---
            let rama_g2 =
                crear_rama_tarea(&cliente, &repo, None, "T-102", &format!("gana2-{sufijo}"))
                    .unwrap();
            ramas_creadas.push(rama_g2.name.clone());
            let rama_p2 =
                crear_rama_tarea(&cliente, &repo, None, "T-102", &format!("pierde2-{sufijo}"))
                    .unwrap();
            ramas_creadas.push(rama_p2.name.clone());

            // Hacemos un check-in real en rama_p2.
            cambiar_a_rama(&estado_a, ws_a.path(), &rama_p2.name).unwrap();
            let archivo = ws_a.path().join(format!("prueba-97-{sufijo}.txt"));
            std::fs::write(&archivo, "cambio real").unwrap();
            Comando::cm()
                .arg("add")
                .arg(archivo.to_string_lossy().to_string())
                .en(ws_a.path())
                .exigir_exito(&estado_a)
                .unwrap();
            Comando::cm()
                .args(["checkin", "-c", "Check-in prueba #97"])
                .arg(archivo.to_string_lossy().to_string())
                .en(ws_a.path())
                .exigir_exito(&estado_a)
                .unwrap();
            cambiar_a_rama(&estado_a, ws_a.path(), RAMA_DEV).unwrap();

            tomar(
                db_b.conexion(),
                &ops_b,
                &AutorTarea {
                    email: &admin_fila.email,
                    rol: Rol::Administrador,
                },
                "T-102",
                |_| Ok(vec![("T-102".into(), rama_p2.clone())]),
                "2026-11-15T15:00:20.000Z",
            )
            .unwrap();
            estado_a
                .con_db(|db_a| {
                    tomar(
                        db_a.conexion(),
                        &ops_a,
                        &AutorTarea {
                            email: DEV,
                            rol: Rol::Desarrollador,
                        },
                        "T-102",
                        |_| Ok(vec![("T-102".into(), rama_g2.clone())]),
                        "2026-11-15T15:00:15.000Z",
                    )
                    .map(|_| ())
                })
                .unwrap();

            std::thread::sleep(std::time::Duration::from_secs(2));
            let sync_b2 = lectura::sincronizar_con_doble_toma(
                db_b.conexion(),
                &ops_b,
                &local_b,
                ahora,
                &publica,
                |c, e| lectura::aplicar_evento(c, &publica, e),
                |info| revisar_y_cerrar_rama_perdedor(&estado_a, &admin_fila.email, info),
            )
            .unwrap();
            println!("Caso 2 (con changeset) B sync: {sync_b2:?}");
            assert_eq!(sync_b2.descartados, 1);
            assert!(
                cliente.rama(&repo, &rama_p2.name).unwrap().is_some(),
                "con changesets reales la rama NO se borra"
            );
            assert_eq!(
                db_tareas::obtener(db_b.conexion(), "T-102")
                    .unwrap()
                    .asignado_a
                    .as_deref(),
                Some(admin_fila.email.as_str())
            );
            assert_eq!(
                notificaciones::de_usuario(db_b.conexion(), DEV)
                    .unwrap()
                    .len(),
                1
            );
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        for rama in ramas_creadas.iter().rev() {
            let salida = Comando::cm()
                .args(["branch", "delete", "--delete-changesets"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .ejecutar(&estado_a)
                .unwrap();
            println!("limpieza rama {rama}: {}", salida.exito());
        }
        let _ = std::fs::remove_dir_all(&compartida);
        println!("subcarpeta de prueba borrada: {}", !compartida.exists());
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    #[test]
    fn asignar_tarea_valida_permisos_y_genera_notificacion() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let externo = "ext@ejemplo.com";
        for conn in [c, e.db_b.conexion()] {
            usuarios::insertar(
                conn,
                &usuarios::Usuario {
                    email: externo.into(),
                    nombre: "Ext".into(),
                    apellido: "Uno".into(),
                    rol: Rol::ColaboradorExterno,
                    fecha_alta: HOY.into(),
                    firma: Some(
                        firmas::firmar_rol(&e.llave_admin, externo, Rol::ColaboradorExterno, AHORA)
                            .unwrap(),
                    ),
                    firmado_en: Some(AHORA.into()),
                    firmado_por: Some(ADMIN.into()),
                    llave_publica: None,
                    certificado: None,
                },
            )
            .unwrap();
        }
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };
        let autor_dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };

        crear_con_subtareas(
            c,
            &e.ops,
            &autor_lider,
            &DatosNuevaTareaConSubtareas {
                titulo: "Padre con hija".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![DatosSubtareaNueva {
                    tipo: TipoTarea::UiInterfaz,
                    titulo: "Subtarea 1".into(),
                    detalles: String::new(),
                }],
            },
            "2026-11-15T10:00:00Z",
            HOY,
        )
        .unwrap();

        // Desarrollador no puede asignar.
        let err = asignar(
            c,
            &e.ops,
            &autor_dev,
            "T-101-1",
            DEV,
            |_| unreachable!(),
            "2026-11-15T10:01:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("Líder o Administrador"));

        // No se puede asignar una tarea que tiene subtareas.
        let err = asignar(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            DEV,
            |_| unreachable!(),
            "2026-11-15T10:01:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("subtareas"));

        // Colaborador externo no puede recibir una subtarea.
        let err = asignar(
            c,
            &e.ops,
            &autor_lider,
            "T-101-1",
            externo,
            |_| unreachable!(),
            "2026-11-15T10:01:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("colaborador externo"));

        // Líder asigna T-101-1 a DEV: crea rama del padre y de la hija + notificación.
        let res = asignar(
            c,
            &e.ops,
            &autor_lider,
            "T-101-1",
            DEV,
            |cadena| {
                assert_eq!(cadena.len(), 2);
                Ok(vec![
                    (
                        "T-101".into(),
                        Rama {
                            id: 1,
                            name: "/main/dev/T-101-padre-con-hija".into(),
                            head_changeset: 10,
                        },
                    ),
                    (
                        "T-101-1".into(),
                        Rama {
                            id: 2,
                            name: "/main/dev/T-101-padre-con-hija/T-101-1-subtarea-1".into(),
                            head_changeset: 11,
                        },
                    ),
                ])
            },
            "2026-11-15T10:02:00Z",
        )
        .unwrap();
        assert_eq!(res[1].asignado_a.as_deref(), Some(DEV));
        assert_eq!(
            res[1].rama_asociada.as_deref(),
            Some("/main/dev/T-101-padre-con-hija/T-101-1-subtarea-1")
        );

        let notis = notificaciones::de_usuario(c, DEV).unwrap();
        assert_eq!(notis.len(), 1);
        assert_eq!(notis[0].tipo, TipoNotificacion::TareaAsignada);
        assert_eq!(notis[0].tarea_id.as_deref(), Some("T-101-1"));

        // Reasignar cuando ya tiene rama no llama a preparar_ramas y conserva la rama.
        let res2 = asignar(
            c,
            &e.ops,
            &autor_lider,
            "T-101-1",
            LIDER,
            |_| panic!("no debe crear ramas si ya existen"),
            "2026-11-15T10:03:00Z",
        )
        .unwrap();
        assert_eq!(res2[1].asignado_a.as_deref(), Some(LIDER));
        assert_eq!(
            res2[1].rama_asociada.as_deref(),
            Some("/main/dev/T-101-padre-con-hija/T-101-1-subtarea-1")
        );

        // Máquina B sincroniza y llega al mismo estado + notificaciones.
        let sync = e.sincronizar_b();
        assert_eq!(sync.aplicados, 3);
        let t_b = db_tareas::obtener(e.db_b.conexion(), "T-101-1").unwrap();
        assert_eq!(t_b.asignado_a.as_deref(), Some(LIDER));
        assert_eq!(
            notificaciones::de_usuario(e.db_b.conexion(), DEV)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn desasignar_tarea_limpia_responsable_y_rechaza_con_commits() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };
        let autor_dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };

        crear(
            c,
            &e.ops,
            &autor_lider,
            &datos_base("Con progreso"),
            "2026-11-15T10:00:00Z",
            HOY,
        )
        .unwrap();

        tomar(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            |_| {
                Ok(vec![(
                    "T-101".into(),
                    Rama {
                        id: 3,
                        name: "/main/dev/T-101-con-progreso".into(),
                        head_changeset: 5,
                    },
                )])
            },
            "2026-11-15T10:01:00Z",
        )
        .unwrap();

        // Sin commits: se puede soltar y vuelve a 'por hacer' sin asignado.
        let soltada = desasignar(c, &e.ops, &autor_dev, "T-101", "2026-11-15T10:02:00Z").unwrap();
        assert_eq!(soltada.asignado_a, None);
        assert_eq!(soltada.estado, EstadoTarea::PorHacer);

        // Volver a tomar y registrar un commit: ya no se puede soltar.
        tomar(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            |_| Ok(vec![]),
            "2026-11-15T10:03:00Z",
        )
        .unwrap();
        commits::guardar(
            c,
            &commits::Commit {
                id: "c1".into(),
                tarea_id: "T-101".into(),
                hash: "42".into(),
                mensaje: "Avance".into(),
                descripcion: None,
                tipo: Some(TipoTarea::Audio),
                autor: DEV.into(),
                fecha: "2026-11-15T10:04:00Z".into(),
            },
        )
        .unwrap();

        let err = desasignar(c, &e.ops, &autor_dev, "T-101", "2026-11-15T10:05:00Z").unwrap_err();
        assert!(err.to_string().contains("check-ins"));
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_asignar_y_soltar_tarea() {
        use crate::cm::lectura::rama_actual_workspace;
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::tareas::{asignar_en, desasignar_en};
        use crate::estado::{llave_publica_del_proyecto, AppState};
        use crate::eventos::{carpeta_ops, configurar_carpeta_compartida, lectura};
        use chrono::{DateTime, Utc};

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let repo_cm = format!("{repo}@{org}@unity");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-98-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();
        let mut ramas_creadas: Vec<String> = Vec::new();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-98-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo_cm}"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let (publica, admin_fila) = estado_a
                .con_db(|db| {
                    let pub_k = llave_publica_del_proyecto(db)?;
                    let todos = usuarios::listar(db.conexion())?;
                    Ok((pub_k, todos.into_iter().next().unwrap()))
                })
                .unwrap();
            let dev_fila = usuarios::Usuario {
                email: DEV.into(),
                nombre: "Dev".into(),
                apellido: "Prueba".into(),
                rol: Rol::Desarrollador,
                fecha_alta: HOY.into(),
                firma: None,
                firmado_en: None,
                firmado_por: None,
                llave_publica: None,
                certificado: None,
            };
            estado_a
                .con_db(|db| usuarios::insertar(db.conexion(), &dev_fila))
                .unwrap();

            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();
            usuarios::insertar(db_b.conexion(), &dev_fila).unwrap();

            let ws_inicial = rama_actual_workspace(&estado_a, ws_a.path()).unwrap();
            let autor_admin = AutorTarea {
                email: &admin_fila.email,
                rol: Rol::Administrador,
            };
            estado_a
                .con_db(|db_a| {
                    crear(
                        db_a.conexion(),
                        &ops_a,
                        &autor_admin,
                        &DatosNuevaTarea {
                            titulo: format!("Asignable 98 {sufijo}"),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        "2026-11-15T16:00:00.000Z",
                        HOY,
                    )
                })
                .unwrap();

            // Admin asigna T-101 a DEV: crea rama en Unity VC y NO cambia su workspace.
            let res = asignar_en(&estado_a, "T-101", DEV).unwrap();
            assert_eq!(res.len(), 1);
            let rama = res[0].rama_asociada.clone().expect("debe tener rama");
            ramas_creadas.push(rama.clone());
            assert_eq!(res[0].asignado_a.as_deref(), Some(DEV));
            let ws_despues = rama_actual_workspace(&estado_a, ws_a.path()).unwrap();
            assert_eq!(
                ws_despues.rama, ws_inicial.rama,
                "asignar no debe tocar workspace"
            );

            // Reasignar cuando ya tiene rama conserva la rama y cambia el responsable.
            let res2 = asignar_en(&estado_a, "T-101", &admin_fila.email).unwrap();
            assert_eq!(res2[0].rama_asociada.as_deref(), Some(rama.as_str()));
            assert_eq!(
                res2[0].asignado_a.as_deref(),
                Some(admin_fila.email.as_str())
            );

            // Soltar la tarea sin commits la deja sin responsable y por hacer.
            let soltada = desasignar_en(&estado_a, "T-101").unwrap();
            assert_eq!(soltada.asignado_a, None);
            assert_eq!(soltada.estado, EstadoTarea::PorHacer);

            std::thread::sleep(std::time::Duration::from_secs(2));
            let ahora = DateTime::parse_from_rfc3339("2026-11-16T23:59:59.000Z")
                .unwrap()
                .with_timezone(&Utc);
            let sync_b = lectura::sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, &publica, e)
            })
            .unwrap();
            assert_eq!(sync_b.aplicados, 4);
            assert_eq!(sync_b.descartados, 0);
            let t_b = db_tareas::obtener(db_b.conexion(), "T-101").unwrap();
            assert_eq!(t_b.asignado_a, None);
            assert_eq!(t_b.estado, EstadoTarea::PorHacer);
            assert_eq!(
                notificaciones::de_usuario(db_b.conexion(), DEV)
                    .unwrap()
                    .len(),
                1
            );
        }));

        estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ruta_ws)
            .ejecutar(&estado_a);
        for rama in ramas_creadas.iter().rev() {
            let _ = Comando::cm()
                .args(["branch", "delete", "--delete-changesets"])
                .arg(format!("br:{rama}@{repo_cm}"))
                .ejecutar(&estado_a);
        }
        let _ = std::fs::remove_dir_all(&compartida);
        if let Err(p) = resultado {
            std::panic::resume_unwind(p);
        }
    }

    #[test]
    fn cambiar_estado_calcula_padres_subarbol_commits_prestamos_y_roles() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };
        let autor_dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };

        // 1. Tarea pendiente de aprobación no puede ir a revisión.
        crear(
            c,
            &e.ops,
            &autor_dev,
            &datos_base("Pendiente"),
            "2026-11-15T10:00:00Z",
            HOY,
        )
        .unwrap();
        let err = cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            EstadoTarea::EnRevision,
            "2026-11-15T10:01:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("pendiente de aprobación"));

        // 2. Tarea padre con dos subtareas: calcular estado de abajo hacia arriba.
        crear_con_subtareas(
            c,
            &e.ops,
            &autor_lider,
            &DatosNuevaTareaConSubtareas {
                titulo: "Sistema de UI".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![
                    DatosSubtareaNueva {
                        tipo: TipoTarea::UiInterfaz,
                        titulo: "Botones".into(),
                        detalles: String::new(),
                    },
                    DatosSubtareaNueva {
                        tipo: TipoTarea::Audio,
                        titulo: "Click".into(),
                        detalles: String::new(),
                    },
                ],
            },
            "2026-11-15T10:02:00Z",
            HOY,
        )
        .unwrap();

        // Al pasar una hija a EnProduccion, el padre pasa a EnProduccion y hereda responsable.
        let r1 = cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-102-1",
            EstadoTarea::EnProduccion,
            "2026-11-15T10:03:00Z",
        )
        .unwrap();
        let padre = r1.tareas.iter().find(|t| t.id == "T-102").unwrap();
        assert_eq!(padre.estado, EstadoTarea::EnProduccion);
        assert_eq!(padre.asignado_a.as_deref(), Some(DEV));

        // Padre no puede ir a EnRevision si T-102-2 sigue abierta.
        let err = cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-102",
            EstadoTarea::EnRevision,
            "2026-11-15T10:04:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("subtareas abiertas"));

        // Pasar ambas hijas a EnRevision (con préstamo en T-102-2 -> avisa préstamo y notifica).
        commits::insertar_prestamo(
            c,
            &commits::CambioPrestado {
                id: "p1".into(),
                tarea_id: "T-102-2".into(),
                origen_id: "T-101".into(),
                hash: None,
                autor: DEV.into(),
                fecha: HOY.into(),
                archivos: vec!["Assets/a.cs".into()],
                archivos_tocados: vec![],
                quitado_en: None,
                quitado_por: None,
            },
        )
        .unwrap();

        cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-102-1",
            EstadoTarea::EnRevision,
            "2026-11-15T10:05:00Z",
        )
        .unwrap();
        let r2 = cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-102-2",
            EstadoTarea::EnRevision,
            "2026-11-15T10:06:00Z",
        )
        .unwrap();
        assert_eq!(r2.aviso_prestamos, vec!["T-101".to_string()]);
        let padre = r2.tareas.iter().find(|t| t.id == "T-102").unwrap();
        assert_eq!(padre.estado, EstadoTarea::EnRevision);
        assert!(!notificaciones::de_usuario(c, LIDER).unwrap().is_empty());

        // Desarrollador no puede mandar a Terminada.
        let err = cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-102-1",
            EstadoTarea::Terminada,
            "2026-11-15T10:07:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("Líder o Administrador"));

        // No puede terminar T-102-2 mientras tenga préstamo vigente; al quitarlo sí puede.
        let err = cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-102-2",
            EstadoTarea::Terminada,
            "2026-11-15T10:08:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("cambios prestados"));
        commits::quitar_prestamo(c, "p1", LIDER, HOY).unwrap();

        // Mover hacia atrás (EnProduccion) baja todo el subárbol; con commits no vuelve a PorHacer.
        let r3 = cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-102",
            EstadoTarea::EnProduccion,
            "2026-11-15T10:09:00Z",
        )
        .unwrap();
        assert!(r3
            .tareas
            .iter()
            .filter(|t| t.id.starts_with("T-102"))
            .all(|t| t.estado == EstadoTarea::EnProduccion));

        commits::guardar(
            c,
            &commits::Commit {
                id: "c99".into(),
                tarea_id: "T-102-1".into(),
                hash: "77".into(),
                mensaje: "Trabajo".into(),
                descripcion: None,
                tipo: Some(TipoTarea::UiInterfaz),
                autor: DEV.into(),
                fecha: "2026-11-15T10:10:00Z".into(),
            },
        )
        .unwrap();
        let err = cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-102",
            EstadoTarea::PorHacer,
            "2026-11-15T10:11:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("check-ins"));

        // Máquina B sincroniza y llega al mismo estado.
        let sync = e.sincronizar_b();
        assert!(sync.aplicados >= 6);
        assert_eq!(sync.descartados, 0);
        assert_eq!(
            db_tareas::obtener(e.db_b.conexion(), "T-102")
                .unwrap()
                .estado,
            EstadoTarea::EnProduccion
        );
    }
}
