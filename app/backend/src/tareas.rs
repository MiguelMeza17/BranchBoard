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
use crate::db::ediciones;
use crate::db::notificaciones::Notificacion;
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::identidad::firmas::{verificar_usuarios, Verificacion};
use crate::modelo::{normalizar_email, EstadoTarea, Rol, TipoNotificacion, TipoTarea};
use crate::notificaciones::{emitir_a_roles_si_falta, emitir_si_falta};
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
    "borrar_progreso",
    "reabrir_tarea",
    "aprobar_tarea",
    "aprobar_tareas",
    "editar_y_aprobar_tarea",
    "advertir_tarea_pendiente",
    "proponer_edicion",
    "aprobar_modificacion",
    "aprobar_modificaciones",
    "rechazar_modificacion",
    "eliminar_tarea",
    "eliminar_definitivamente",
    "hacer_checkin",
    "crear_en_espera",
    "quitar_en_espera",
    "crear_rama_desde_espera",
    "traer_cambios",
    "devolver_cambios_prestados",
    "eliminar_cambios_ajenos",
    "tarea_cerrada_por_deteccion",
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

/// `payload` del evento `borrar_progreso` (#100).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadBorrarProgreso {
    pub id: String,
}

/// `payload` del evento `reabrir_tarea` (#100).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadReabrirTarea {
    pub id: String,
    pub motivo: String,
}

/// `payload` de `aprobar_tareas` (#101).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadAprobarTareas {
    pub ids: Vec<String>,
}

/// `payload` de `advertir_tarea_pendiente` (#101).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadAdvertirTarea {
    pub id: String,
    pub mensaje: String,
}

/// `payload` de `proponer_edicion` (#102).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadProponerEdicion {
    pub id: String,
    pub edicion_id: String,
    pub titulo: String,
    pub detalles: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motivo: Option<String>,
    pub fecha: String,
}

/// Respuesta de `proponer_edicion` con la tarea ya actualizada y la fila pendiente (#102).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultadoProponerEdicion {
    pub tarea: Tarea,
    pub edicion: ediciones::EdicionPendiente,
}

/// `payload` de `eliminar_tarea` (#103).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadEliminarTarea {
    pub id: String,
    pub justificacion: String,
}

/// `payload` de `tarea_cerrada_por_deteccion` (#104).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadTareaCerradaDeteccion {
    pub id: String,
    pub fecha_completada: String,
}

/// Datos para registrar un check-in ejecutado desde BranchBoard (#105).
#[derive(Debug, Clone)]
pub struct DatosCheckin<'a> {
    pub tarea_id: &'a str,
    pub changeset: i64,
    pub titulo: &'a str,
    pub descripcion: &'a str,
    pub tipo: TipoTarea,
}

/// `payload` de `hacer_checkin` (#105).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadHacerCheckin {
    pub commit: commits::Commit,
    pub changeset: i64,
}

/// Respuesta de `cambios_pendientes` con guardia de rama del workspace (#105).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendientesRama {
    pub rama_actual: String,
    pub en_rama_esperada: bool,
    pub archivos: Vec<String>,
}

/// Datos para convertir una subtarea "En espera" en subtarea con rama propia (#106).
#[derive(Debug, Clone)]
pub struct DatosRamaDesdeEspera<'a> {
    pub espera_id: &'a str,
    pub titulo: &'a str,
    pub detalles: &'a str,
    pub tipo: TipoTarea,
    pub asignado_a: &'a str,
    pub rama: &'a str,
    pub ultimo_changeset_visto: i64,
}

/// `payload` de `crear_rama_desde_espera` (#106).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadCrearRamaDesdeEspera {
    pub id: String,
    pub titulo: String,
    pub detalles: String,
    pub tipo: TipoTarea,
    pub asignado_a: String,
    pub rama: String,
    pub ultimo_changeset_visto: i64,
}

/// Ramas resueltas al validar un préstamo `traer_cambios` (#108).
#[derive(Debug, Clone, PartialEq)]
pub struct DestinoYOrigenPrestamo {
    pub destino_id: String,
    pub origen_id: String,
    pub rama_destino: String,
    pub rama_origen: String,
}

/// Datos para registrar un préstamo `traer_cambios` en `ops/` (#108).
#[derive(Debug, Clone)]
pub struct DatosTraerCambios<'a> {
    pub destino_id: &'a str,
    pub origen_id: &'a str,
    pub hash: Option<&'a str>,
    pub changeset_merge: i64,
    pub archivos: Vec<String>,
}

/// `payload` de `traer_cambios` (#108).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadTraerCambios {
    pub prestamo: commits::CambioPrestado,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ultimo_changeset_visto: Option<i64>,
}

/// `payload` de `devolver_cambios_prestados` (#108).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadDevolverCambios {
    pub prestamo_id: String,
    pub quitado_en: String,
    pub quitado_por: String,
}

/// `payload` de `eliminar_cambios_ajenos` (#109).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadEliminarCambiosAjenos {
    pub prestamo_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset_limpio: Option<i64>,
}

/// Alerta en pantalla cuando el workspace está en `main`, `dev` o una rama que no
/// pertenece al usuario y tiene cambios significativos (#111). Solo avisa; nunca deshace.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlertaRamaAjena {
    pub rama_actual: String,
    pub tarea_ajena_id: Option<String>,
    pub dueno_rama: Option<String>,
    pub rama_sugerida: Option<String>,
    pub tarea_sugerida_id: Option<String>,
    pub archivos: Vec<String>,
    pub mensaje: String,
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

/// Lista las tareas en Papelera ordenadas de la eliminación más reciente a la
/// más antigua (#103).
pub fn listar_papelera(c: &Connection) -> Resultado<Vec<Tarea>> {
    db_tareas::listar_papelera(c)
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

    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_crear_tarea(tx, &nueva, ahora, true)
    })?;
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
        aplicar_crear_tarea_con_subtareas(tx, &payload, ahora)
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
        aplicar_tomar_tarea(tx, autor.email, Some(autor.rol), &payload, ahora)
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

fn origenes_prestamos_filtrados(
    c: &Connection,
    ids: &[String],
    solo_tocados: bool,
) -> Resultado<Vec<String>> {
    let mut origenes = Vec::new();
    for tid in ids {
        for p in commits::prestamos_vigentes(c, tid)? {
            if (!solo_tocados || !p.archivos_tocados.is_empty()) && !origenes.contains(&p.origen_id)
            {
                origenes.push(p.origen_id);
            }
        }
    }
    Ok(origenes)
}

fn prestamos_vigentes_origenes(c: &Connection, ids: &[String]) -> Resultado<Vec<String>> {
    origenes_prestamos_filtrados(c, ids, false)
}

fn prestamos_con_tocados_origenes(c: &Connection, ids: &[String]) -> Resultado<Vec<String>> {
    origenes_prestamos_filtrados(c, ids, true)
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

/// Lista las ramas asociadas de `id` y su subárbol activo (de hojas hacia la raíz)
/// para revertirlas en Unity VC al borrar progreso (#100).
pub fn ramas_de_subarbol(c: &Connection, id: &str) -> Resultado<Vec<String>> {
    let tarea = db_tareas::obtener(c, id.trim())?;
    let mut ramas = Vec::new();
    for sub in descendientes_activos(c, &tarea.id)?.into_iter().rev() {
        if let Some(r) = sub.rama_asociada {
            if !ramas.contains(&r) {
                ramas.push(r);
            }
        }
    }
    if let Some(r) = tarea.rama_asociada {
        if !ramas.contains(&r) {
            ramas.push(r);
        }
    }
    Ok(ramas)
}

/// Borra commits, préstamos y progreso de `id` y su subárbol (#100), dejando
/// la tarea tomada y con su rama en `'por hacer'`.
pub fn borrar_progreso(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let id = id.trim();
    let payload = PayloadBorrarProgreso { id: id.to_string() };
    let mut ev = Evento::nuevo(
        "borrar_progreso",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar borrar_progreso: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| aplicar_borrar_progreso(tx, &payload))?;
    db_tareas::listar(c)
}

/// Reabre una tarea `'terminada'` a `'en revisión'` con motivo obligatorio (#100).
/// Tope 3 reaperturas; a partir de 3, solo el Administrador puede volver a reabrirla.
pub fn reabrir(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    motivo: &str,
    ahora: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let payload = PayloadReabrirTarea {
        id: id.to_string(),
        motivo: motivo.trim().to_string(),
    };
    let mut ev = Evento::nuevo(
        "reabrir_tarea",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar reabrir_tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();

    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_reabrir_tarea(tx, Some(autor.rol), &payload, ahora)
    })?;
    db_tareas::obtener(c, id)
}

fn exigir_lider_o_admin(rol_conocido: Option<Rol>) -> Resultado<()> {
    if let Some(rol) = rol_conocido {
        if !rol.es_lider_o_admin() {
            return Err(Error::SinPermiso);
        }
    }
    Ok(())
}

/// Aprueba una tarea pendiente (`aprobar_tarea`, #101): solo Líder/Admin.
/// Pone `pendiente_aprobacion = false` sin tocar `estado`.
pub fn aprobar_tarea(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let payload = PayloadDesasignarTarea { id: id.to_string() };
    let mut ev = Evento::nuevo(
        "aprobar_tarea",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar aprobar_tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_aprobar_tareas(tx, Some(autor.rol), std::slice::from_ref(&payload.id))
    })?;
    db_tareas::obtener(c, id)
}

/// Aprueba varias tareas pendientes en una sola operación (`aprobar_tareas`, #101).
pub fn aprobar_tareas(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    ids: &[String],
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let limpias: Vec<String> = ids
        .iter()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect();
    if limpias.is_empty() {
        return Err(Error::Validacion(
            "Selecciona al menos una tarea para aprobar".into(),
        ));
    }
    let payload = PayloadAprobarTareas { ids: limpias };
    let mut ev = Evento::nuevo(
        "aprobar_tareas",
        payload.ids.first().map(String::as_str),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar aprobar_tareas: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_aprobar_tareas(tx, Some(autor.rol), &payload.ids)
    })?;
    db_tareas::listar(c)
}

/// Edita campos de una tarea pendiente y la aprueba en la misma operación
/// (`editar_y_aprobar_tarea`, #101). Emite notificación `editada_aprobada` (#11).
pub fn editar_y_aprobar(
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
        "editar_y_aprobar_tarea",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar editar_y_aprobar_tarea: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_editar_y_aprobar_tarea(tx, Some(autor.rol), &payload, ahora)
    })?;
    db_tareas::obtener(c, id)
}

/// Envía una advertencia escrita al creador de una tarea pendiente sin aprobarla
/// (`advertir_tarea_pendiente`, #101). Emite notificación `advertencia` (#12).
pub fn advertir_pendiente(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    mensaje: &str,
    ahora: &str,
) -> Resultado<()> {
    let id = id.trim();
    let payload = PayloadAdvertirTarea {
        id: id.to_string(),
        mensaje: mensaje.trim().to_string(),
    };
    let mut ev = Evento::nuevo(
        "advertir_tarea_pendiente",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar advertir_tarea_pendiente: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_advertir_tarea_pendiente(tx, Some(autor.rol), &payload, ahora)
    })?;
    Ok(())
}

/// Ediciones pendientes registradas en la base local (#102).
pub fn listar_ediciones_pendientes(c: &Connection) -> Resultado<Vec<ediciones::EdicionPendiente>> {
    ediciones::listar(c)
}

/// Cualquier rol edita título y/o detalles en `'por hacer'` (`proponer_edicion`, #102).
/// Aplica de inmediato en `tareas`, suma si ya existía fila pendiente y notifica a Líderes/Admin.
#[allow(clippy::too_many_arguments)]
pub fn proponer_edicion(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    titulo: &str,
    detalles: &str,
    motivo: Option<&str>,
    ahora: &str,
    hoy: &str,
) -> Resultado<ResultadoProponerEdicion> {
    let id = id.trim();
    let payload = PayloadProponerEdicion {
        id: id.to_string(),
        edicion_id: format!("EP-{id}-{ahora}"),
        titulo: titulo.to_string(),
        detalles: detalles.to_string(),
        motivo: limpiar_opcional(motivo.map(str::to_string)),
        fecha: hoy.to_string(),
    };
    let mut ev = Evento::nuevo(
        "proponer_edicion",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar proponer_edicion: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_proponer_edicion(tx, &ev.usuario, &payload, ahora)
    })?;
    Ok(ResultadoProponerEdicion {
        tarea: db_tareas::obtener(c, id)?,
        edicion: ediciones::de_tarea(c, id)?
            .ok_or_else(|| Error::Interno("No se encontró la edición pendiente guardada".into()))?,
    })
}

/// Líder/Admin confirma la edición pendiente de `id` (`aprobar_modificacion`, #102).
pub fn aprobar_modificacion(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let payload = PayloadDesasignarTarea { id: id.to_string() };
    let mut ev = Evento::nuevo(
        "aprobar_modificacion",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar aprobar_modificacion: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_aprobar_modificaciones(tx, Some(autor.rol), std::slice::from_ref(&payload.id))
    })?;
    db_tareas::obtener(c, id)
}

/// Líder/Admin confirma varias ediciones pendientes en una operación (`aprobar_modificaciones`, #102).
pub fn aprobar_modificaciones(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    ids: &[String],
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let limpias: Vec<String> = ids
        .iter()
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect();
    if limpias.is_empty() {
        return Err(Error::Validacion(
            "Selecciona al menos una edición para aprobar".into(),
        ));
    }
    let payload = PayloadAprobarTareas { ids: limpias };
    let mut ev = Evento::nuevo(
        "aprobar_modificaciones",
        payload.ids.first().map(String::as_str),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar aprobar_modificaciones: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_aprobar_modificaciones(tx, Some(autor.rol), &payload.ids)
    })?;
    db_tareas::listar(c)
}

/// Líder/Admin rechaza la edición pendiente de `id` y restaura sus valores anteriores (#102).
pub fn rechazar_modificacion(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let payload = PayloadDesasignarTarea { id: id.to_string() };
    let mut ev = Evento::nuevo(
        "rechazar_modificacion",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!("no se pudo serializar rechazar_modificacion: {e}"))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_rechazar_modificacion(tx, Some(autor.rol), &payload.id)
    })?;
    db_tareas::obtener(c, id)
}

/// Envía una tarea a la Papelera con cascada sobre sus subtareas activas (#103).
/// Solo Líder o Administrador. Limpia `pendiente_aprobacion` y ediciones pendientes,
/// y emite notificación `tarea_eliminada` (#13) al creador de cada tarea afectada.
/// Devuelve todas las tareas actualizadas (primero la raíz, luego las arrastradas).
pub fn eliminar(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    justificacion: &str,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let id = id.trim();
    let payload = PayloadEliminarTarea {
        id: id.to_string(),
        justificacion: justificacion.trim().to_string(),
    };
    let mut ev = Evento::nuevo(
        "eliminar_tarea",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar eliminar_tarea: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    let mut afectadas = Vec::new();
    eventos::publicar(c, ops, &ev, |tx| {
        afectadas = aplicar_eliminar_tarea(tx, Some(autor.rol), &payload, ahora)?;
        Ok(())
    })?;
    Ok(afectadas)
}

/// Purga definitivamente una tarea de la Papelera y sus subtareas en Papelera (#103).
/// Solo Líder o Administrador. Devuelve las filas eliminadas en orden raíz -> hojas
/// para que la capa de comandos pueda purgar sus ramas de hojas hacia raíz.
pub fn eliminar_definitivamente(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    let id = id.trim();
    let payload = PayloadDesasignarTarea { id: id.to_string() };
    let mut ev = Evento::nuevo(
        "eliminar_definitivamente",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar eliminar_definitivamente: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    let mut purgadas = Vec::new();
    eventos::publicar(c, ops, &ev, |tx| {
        purgadas = aplicar_eliminar_definitivamente(tx, Some(autor.rol), id)?;
        Ok(())
    })?;
    Ok(purgadas)
}

/// Actualiza los indicadores locales de detección (`ultimo_changeset_visto` y
/// `modificada`) en SQLite sin emitir evento a `ops/` (#104).
/// Devuelve `true` si la fila cambió.
pub fn actualizar_deteccion_local(
    c: &Connection,
    id: &str,
    ultimo_cs: Option<i64>,
    marcar_modificada: bool,
) -> Resultado<bool> {
    let mut tarea = db_tareas::obtener(c, id)?;
    let mut cambio = false;
    if let Some(cs) = ultimo_cs {
        if tarea.ultimo_changeset_visto != Some(cs) {
            tarea.ultimo_changeset_visto = Some(cs);
            cambio = true;
        }
    }
    if marcar_modificada && !tarea.modificada {
        tarea.modificada = true;
        cambio = true;
    }
    if cambio {
        db_tareas::actualizar(c, &tarea)?;
    }
    Ok(cambio)
}

/// Marca una tarea como `terminada` al detectar que su rama fue mergeada a su
/// padre en Unity VC (`tarea_cerrada_por_deteccion`, #104), emitiendo evento en `ops/`.
pub fn cerrar_por_deteccion(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    id: &str,
    ahora: &str,
    fecha_completada: &str,
) -> Resultado<Tarea> {
    let id = id.trim();
    let payload = PayloadTareaCerradaDeteccion {
        id: id.to_string(),
        fecha_completada: fecha_completada.to_string(),
    };
    let mut ev = Evento::nuevo(
        "tarea_cerrada_por_deteccion",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar tarea_cerrada_por_deteccion: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_cerrar_por_deteccion(tx, id, fecha_completada)
    })?;
    db_tareas::obtener(c, id)
}

/// Verifica que `rama` corresponda a una tarea elegible para `hacer_checkin`
/// (existe, no eliminada ni terminada, sin subtareas activas y asignada al autor, #105).
pub fn tarea_para_checkin(c: &Connection, autor_email: &str, rama: &str) -> Resultado<Tarea> {
    let limpia = rama.trim().trim_end_matches('/');
    if limpia.is_empty() {
        return Err(Error::Validacion("Indica la rama de la tarea".into()));
    }
    let tarea = db_tareas::listar(c)?
        .into_iter()
        .find(|t| {
            !t.eliminada
                && t.rama_asociada
                    .as_deref()
                    .map(|r| r.trim_end_matches('/').eq_ignore_ascii_case(limpia))
                    .unwrap_or(false)
        })
        .ok_or_else(|| {
            Error::NoEncontrado(format!("No hay ninguna tarea activa en la rama {limpia}"))
        })?;
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    if !descendientes_activos(c, &tarea.id)?.is_empty() {
        return Err(Error::Validacion(
            "Esta rama tiene subtareas: no hagas check-in en el padre, crea una rama".into(),
        ));
    }
    let autor = normalizar_email(autor_email);
    let asignado = tarea.asignado_a.as_deref().map(normalizar_email);
    if asignado.as_deref() != Some(autor.as_str()) {
        return Err(Error::SinPermiso);
    }
    Ok(tarea)
}

/// Resuelve la rama asociada de `id` para cambiar el workspace desde la guardia (#105).
pub fn rama_para_cambiar(c: &Connection, id: &str) -> Resultado<String> {
    let tarea = db_tareas::obtener(c, id.trim())?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            tarea.id
        )));
    }
    tarea
        .rama_asociada
        .ok_or_else(|| Error::Validacion("La tarea no tiene rama asignada".into()))
}

/// Valida tipo y da formato `título\n\ndescripción` para `cm checkin` (#105).
pub fn armar_comentario_checkin(
    titulo: &str,
    descripcion: &str,
    tipo: TipoTarea,
) -> Resultado<String> {
    validar_tipo_creacion(tipo)?;
    let titulo = titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion(
            "El título del check-in es obligatorio".into(),
        ));
    }
    let desc = descripcion.trim();
    if desc.is_empty() {
        Ok(titulo.to_string())
    } else {
        Ok(format!("{titulo}\n\n{desc}"))
    }
}

/// Publica en `ops/` el check-in ejecutado desde BranchBoard conservando `tipo` (#105).
pub fn registrar_checkin(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    datos: &DatosCheckin<'_>,
    ahora: &str,
    fecha_commit: &str,
) -> Resultado<commits::Commit> {
    let titulo = datos.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion(
            "El título del check-in es obligatorio".into(),
        ));
    }
    validar_tipo_creacion(datos.tipo)?;
    let tarea_id = datos.tarea_id.trim();
    let commit = commits::Commit {
        id: format!("C-{tarea_id}-{}", datos.changeset),
        tarea_id: tarea_id.to_string(),
        hash: datos.changeset.to_string(),
        mensaje: titulo.to_string(),
        descripcion: limpiar_opcional(Some(datos.descripcion.to_string())),
        tipo: Some(datos.tipo),
        autor: normalizar_email(autor.email),
        fecha: fecha_commit.to_string(),
    };
    let payload = PayloadHacerCheckin {
        commit: commit.clone(),
        changeset: datos.changeset,
    };
    let mut ev = Evento::nuevo(
        "hacer_checkin",
        Some(tarea_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar hacer_checkin: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| aplicar_hacer_checkin(tx, &payload))?;
    Ok(commit)
}

/// Crea la subtarea comodín `"En espera"` (`SinDefinir`) bajo `padre_id` al detectar
/// cambios sin check-in en una rama con subtareas (#106), publicándola en `ops/`.
pub fn crear_en_espera(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    padre_id: &str,
    cambios: usize,
    ahora: &str,
    hoy: &str,
) -> Resultado<Tarea> {
    let padre = db_tareas::obtener(c, padre_id.trim())?;
    let email = normalizar_email(autor.email);
    let id = db_tareas::siguiente_id(c, Some(&padre.id))?;
    let mut espera = Tarea::nueva(&id, "En espera", TipoTarea::SinDefinir, &email, hoy);
    espera.detalles = format!(
        "{cambios} cambio(s) sin check-in hechos directamente en la rama de {}. Crea su rama para guardarlos.",
        padre.id
    );
    espera.estado = EstadoTarea::EnProduccion;
    espera.pendiente_aprobacion = false;
    espera.asignado_a = Some(email.clone());
    espera.tarea_padre_id = Some(padre.id);
    espera.sprint_id = padre.sprint_id;
    espera.backlog_id = padre.backlog_id;

    let mut ev = Evento::nuevo(
        "crear_en_espera",
        Some(&espera.id),
        &email,
        serde_json::to_value(&espera)
            .map_err(|e| Error::Interno(format!("no se pudo serializar crear_en_espera: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| aplicar_crear_en_espera(tx, &espera))?;
    db_tareas::obtener(c, &espera.id)
}

/// Quita la subtarea temporal `"En espera"` cuando la rama padre vuelve a quedar limpia (#106).
pub fn quitar_en_espera(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    espera_id: &str,
    ahora: &str,
) -> Resultado<()> {
    let id = espera_id.trim();
    let payload = PayloadDesasignarTarea { id: id.to_string() };
    let mut ev = Evento::nuevo(
        "quitar_en_espera",
        Some(id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar quitar_en_espera: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| aplicar_quitar_en_espera(tx, &payload.id))?;
    Ok(())
}

/// Valida datos y permisos para convertir una subtarea `"En espera"` en subtarea
/// normal con su propia rama (`crear_rama_desde_espera`, #106).
/// Devuelve `(espera, rama_del_padre, asignado_final)`.
pub fn preparar_rama_desde_espera(
    c: &Connection,
    autor_email: &str,
    rol: Rol,
    espera_id: &str,
    titulo: &str,
    tipo: TipoTarea,
) -> Resultado<(Tarea, String, String)> {
    validar_tipo_creacion(tipo)?;
    if titulo.trim().is_empty() {
        return Err(Error::Validacion(
            "El título de la subtarea es obligatorio".into(),
        ));
    }
    let espera = db_tareas::obtener(c, espera_id.trim())?;
    if espera.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            espera.id
        )));
    }
    if espera.tipo != TipoTarea::SinDefinir {
        return Err(Error::Validacion(format!(
            "La tarea {} no está En espera",
            espera.id
        )));
    }
    let padre_id = espera
        .tarea_padre_id
        .as_deref()
        .ok_or_else(|| Error::Validacion("La tarea En espera no tiene padre".into()))?;
    let padre = db_tareas::obtener(c, padre_id)?;
    if padre.eliminada || padre.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea padre {} ya está cerrada o en la Papelera",
            padre.id
        )));
    }
    let rama_padre = padre.rama_asociada.clone().ok_or_else(|| {
        Error::Validacion(format!(
            "La tarea padre {} no tiene rama asociada",
            padre.id
        ))
    })?;

    let yo = normalizar_email(autor_email);
    let dueno = espera
        .asignado_a
        .as_deref()
        .or(padre.asignado_a.as_deref())
        .map(normalizar_email);
    if !rol.es_lider_o_admin() {
        if let Some(ref d) = dueno {
            if d != &yo {
                return Err(Error::SinPermiso);
            }
        }
    }
    Ok((espera, rama_padre, dueno.unwrap_or(yo)))
}

/// Convierte la subtarea `"En espera"` en subtarea normal ya apuntando a su nueva rama (#106).
pub fn convertir_espera_en_subtarea(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    datos: &DatosRamaDesdeEspera<'_>,
    ahora: &str,
) -> Resultado<Tarea> {
    let payload = PayloadCrearRamaDesdeEspera {
        id: datos.espera_id.trim().to_string(),
        titulo: datos.titulo.trim().to_string(),
        detalles: datos.detalles.trim().to_string(),
        tipo: datos.tipo,
        asignado_a: normalizar_email(datos.asignado_a),
        rama: validar_ruta_rama_tarea(datos.rama)?,
        ultimo_changeset_visto: datos.ultimo_changeset_visto,
    };
    let mut ev = Evento::nuevo(
        "crear_rama_desde_espera",
        Some(&payload.id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar crear_rama_desde_espera: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_crear_rama_desde_espera(tx, &payload, ahora)
    })?;
    db_tareas::obtener(c, &payload.id)
}

/// Lista los cambios prestados de una tarea o de todo el proyecto si `tarea_id` es `None` (#108).
pub fn listar_cambios_prestados(
    c: &Connection,
    tarea_id: Option<&str>,
) -> Resultado<Vec<commits::CambioPrestado>> {
    match tarea_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => commits::prestamos_de_tarea(c, id),
        None => commits::todos_los_prestamos(c),
    }
}

fn parsear_changeset_hash(hash: Option<&str>) -> Resultado<Option<i64>> {
    let Some(limpio) = hash.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let sin_prefijo = limpio
        .strip_prefix("cs:")
        .or_else(|| limpio.strip_prefix("CS:"))
        .unwrap_or(limpio)
        .trim();
    let cs = sin_prefijo
        .parse::<i64>()
        .map_err(|_| Error::Validacion("El changeset indicado no es válido".into()))?;
    if cs <= 0 {
        return Err(Error::Validacion(
            "El changeset indicado no es válido".into(),
        ));
    }
    Ok(Some(cs))
}

/// Valida si `autor` puede traer cambios desde `origen_id` (`hash` opcional) hacia
/// `destino_id` según las reglas de estado, rol y rama (#108).
pub fn preparar_traer_cambios(
    c: &Connection,
    autor: &AutorTarea<'_>,
    destino_id: &str,
    origen_id: &str,
    hash: Option<&str>,
) -> Resultado<(DestinoYOrigenPrestamo, Option<i64>)> {
    let destino_id = destino_id.trim();
    let origen_id = origen_id.trim();
    if destino_id.is_empty() || origen_id.is_empty() {
        return Err(Error::Validacion(
            "Debes indicar la tarea destino y la tarea origen".into(),
        ));
    }
    if destino_id == origen_id {
        return Err(Error::Validacion(
            "Una tarea no puede traer cambios de sí misma".into(),
        ));
    }
    let cs_puntual = parsear_changeset_hash(hash)?;

    let destino = db_tareas::obtener(c, destino_id)?;
    if destino.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            destino.id
        )));
    }
    if destino.tipo == TipoTarea::SinDefinir {
        return Err(Error::Validacion(
            "No se puede traer cambios: esta tarea todavía tiene cambios sin guardar (sin check-in)"
                .into(),
        ));
    }
    match destino.estado {
        EstadoTarea::PorHacer => {
            if destino.asignado_a.is_none() || destino.rama_asociada.is_none() {
                return Err(Error::Validacion(
                    "Solo se pueden traer cambios a una tarea en 'por hacer' que ya tenga rama"
                        .into(),
                ));
            }
        }
        EstadoTarea::EnProduccion => {}
        EstadoTarea::EnRevision => {
            if !autor.rol.es_lider_o_admin() {
                return Err(Error::SinPermiso);
            }
        }
        EstadoTarea::Terminada => {
            return Err(Error::Validacion(
                "No se pueden traer cambios a una tarea terminada".into(),
            ));
        }
    }
    let rama_destino = destino
        .rama_asociada
        .as_deref()
        .map(validar_ruta_rama_tarea)
        .transpose()?
        .ok_or_else(|| Error::Validacion("La tarea destino no tiene rama asignada".into()))?;

    let origen = db_tareas::obtener(c, origen_id)?;
    if origen.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea origen {} está en la Papelera",
            origen.id
        )));
    }
    if origen.tipo == TipoTarea::SinDefinir {
        return Err(Error::Validacion(
            "No se pueden traer cambios desde una subtarea En espera sin guardar".into(),
        ));
    }
    let rama_origen = origen
        .rama_asociada
        .as_deref()
        .map(validar_ruta_rama_tarea)
        .transpose()?
        .ok_or_else(|| Error::Validacion("La tarea origen no tiene rama asignada".into()))?;

    match origen.estado {
        EstadoTarea::Terminada => {
            if origen.tarea_padre_id.as_deref() == Some(destino.id.as_str()) {
                return Err(Error::Validacion(format!(
                    "Los cambios de {} ya entraron a esta rama; usa Unir cambios entrantes",
                    origen.id
                )));
            }
        }
        EstadoTarea::EnRevision => {}
        EstadoTarea::EnProduccion => {
            if commits::de_tarea(c, &origen.id)?.is_empty() {
                return Err(Error::Validacion(
                    "La tarea origen en producción todavía no tiene check-ins para traer".into(),
                ));
            }
        }
        EstadoTarea::PorHacer => {
            return Err(Error::Validacion(
                "Solo se pueden traer cambios de tareas terminadas, en revisión o en producción con check-ins"
                    .into(),
            ));
        }
    }

    let hash_str = cs_puntual.map(|n| n.to_string());
    if commits::prestamos_vigentes(c, &destino.id)?
        .into_iter()
        .any(|p| p.origen_id == origen.id && p.hash == hash_str)
    {
        return Err(Error::Validacion(
            "Esos cambios ya están vigentes como préstamo en esta tarea".into(),
        ));
    }

    Ok((
        DestinoYOrigenPrestamo {
            destino_id: destino.id,
            origen_id: origen.id,
            rama_destino,
            rama_origen,
        },
        cs_puntual,
    ))
}

/// Publica en `ops/` el préstamo ejecutado en Unity VC (`traer_cambios`, #108).
pub fn registrar_traer_cambios(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    datos: &DatosTraerCambios<'_>,
    ahora: &str,
) -> Resultado<commits::CambioPrestado> {
    let destino_id = datos.destino_id.trim();
    let origen_id = datos.origen_id.trim();
    let hash_normalizado = parsear_changeset_hash(datos.hash)?.map(|n| n.to_string());
    let prestamo = commits::CambioPrestado {
        id: format!("CT-{destino_id}-{}", datos.changeset_merge),
        tarea_id: destino_id.to_string(),
        origen_id: origen_id.to_string(),
        hash: hash_normalizado,
        autor: normalizar_email(autor.email),
        fecha: ahora.to_string(),
        archivos: datos.archivos.clone(),
        archivos_tocados: Vec::new(),
        quitado_en: None,
        quitado_por: None,
        changeset_merge: Some(datos.changeset_merge),
        changeset_limpio: None,
    };
    let payload = PayloadTraerCambios {
        prestamo: prestamo.clone(),
        ultimo_changeset_visto: Some(datos.changeset_merge),
    };
    let mut ev = Evento::nuevo(
        "traer_cambios",
        Some(destino_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload)
            .map_err(|e| Error::Interno(format!("no se pudo serializar traer_cambios: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| aplicar_traer_cambios(tx, &payload))?;
    Ok(prestamo)
}

/// Valida si `autor` puede devolver un préstamo vigente mediante merge sustractivo (#108).
/// Devuelve `(prestamo, rama_destino, cs_merge)`.
pub fn preparar_devolver_cambios(
    c: &Connection,
    autor: &AutorTarea<'_>,
    prestamo_id: &str,
) -> Resultado<(commits::CambioPrestado, String, i64)> {
    let prestamo = commits::obtener_prestamo(c, prestamo_id.trim())?;
    if prestamo.quitado_en.is_some() {
        return Err(Error::Validacion(
            "Esos cambios ya se habían devuelto".into(),
        ));
    }
    if !prestamo.archivos_tocados.is_empty() {
        return Err(Error::Validacion(
            "Se modificaron archivos prestados; elimina primero los cambios ajenos antes de devolver el préstamo"
                .into(),
        ));
    }
    let cs_merge = prestamo.changeset_merge.ok_or_else(|| {
        Error::Validacion(
            "El préstamo no tiene changeset de merge registrado para devolver en Unity VC".into(),
        )
    })?;
    let rama_destino = validar_destino_para_prestamo(c, autor, &prestamo.tarea_id)?;
    Ok((prestamo, rama_destino, cs_merge))
}

fn puede_operar_prestamo(c: &Connection, autor: &AutorTarea<'_>, destino: &Tarea) -> bool {
    let actor = normalizar_email(autor.email);
    let es_dueno_destino = destino
        .asignado_a
        .as_deref()
        .map(normalizar_email)
        .as_deref()
        == Some(actor.as_str());
    let es_dueno_padre = destino
        .tarea_padre_id
        .as_deref()
        .and_then(|pid| db_tareas::buscar(c, pid).ok().flatten())
        .and_then(|p| p.asignado_a)
        .map(|e| normalizar_email(&e) == actor)
        .unwrap_or(false);

    match destino.estado {
        EstadoTarea::EnRevision => autor.rol.es_lider_o_admin() || es_dueno_padre,
        EstadoTarea::PorHacer | EstadoTarea::EnProduccion => {
            es_dueno_destino || autor.rol.es_lider_o_admin()
        }
        EstadoTarea::Terminada => false,
    }
}

fn validar_destino_para_prestamo(
    c: &Connection,
    autor: &AutorTarea<'_>,
    tarea_id: &str,
) -> Resultado<String> {
    let destino = db_tareas::obtener(c, tarea_id)?;
    if destino.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            destino.id
        )));
    }
    if destino.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            destino.id
        )));
    }
    if !puede_operar_prestamo(c, autor, &destino) {
        return Err(Error::SinPermiso);
    }
    destino
        .rama_asociada
        .as_deref()
        .map(validar_ruta_rama_tarea)
        .transpose()?
        .ok_or_else(|| Error::Validacion("La tarea no tiene rama asociada".into()))
}

/// Normaliza y cruza rutas de cambios (pendientes o de un `cm diff`) contra la
/// lista de archivos prestados, devolviendo sus nombres canónicos ordenados (#109).
pub fn cruzar_archivos_tocados(
    archivos_prestados: &[String],
    candidatos_rutas: &[String],
) -> Vec<String> {
    let normalizar = |r: &str| {
        r.trim()
            .trim_matches('"')
            .trim()
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_lowercase()
    };
    let mut por_norm: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for a in archivos_prestados {
        let norm = normalizar(a);
        if !norm.is_empty() {
            por_norm.entry(norm).or_insert_with(|| a.clone());
        }
    }
    let mut tocados = std::collections::BTreeSet::new();
    for c in candidatos_rutas {
        let norm = normalizar(c);
        if let Some(canon) = por_norm.get(&norm) {
            tocados.insert(canon.clone());
        }
    }
    tocados.into_iter().collect()
}

/// Actualiza `archivos_tocados` de los préstamos vigentes de `tarea_id` (#109)
/// cruzando cambios pendientes del workspace (si está en la rama) y diffs de
/// check-ins posteriores al merge/última limpieza. Devuelve `true` si cambió algo.
pub fn actualizar_tocados_de_prestamos(
    c: &Connection,
    tarea_id: &str,
    rutas_pendientes_en_rama: Option<&[String]>,
    diffs_commits: &[(i64, Vec<String>)],
) -> Resultado<bool> {
    let prestamos = commits::prestamos_vigentes(c, tarea_id)?;
    let mut hubo_cambios = false;
    for p in prestamos {
        let piso = p
            .changeset_limpio
            .into_iter()
            .chain(p.changeset_merge)
            .max()
            .unwrap_or(0);

        let mut candidatos: Vec<String> = Vec::new();
        for (cs, rutas) in diffs_commits {
            if *cs > piso {
                candidatos.extend(rutas.iter().cloned());
            }
        }
        if let Some(pendientes) = rutas_pendientes_en_rama {
            candidatos.extend(pendientes.iter().cloned());
        }

        let mut nuevos = cruzar_archivos_tocados(&p.archivos, &candidatos);
        if rutas_pendientes_en_rama.is_none() && !p.archivos_tocados.is_empty() {
            let mut conjunto: std::collections::BTreeSet<String> =
                p.archivos_tocados.iter().cloned().collect();
            conjunto.extend(nuevos);
            nuevos = conjunto.into_iter().collect();
        }

        if nuevos != p.archivos_tocados {
            commits::marcar_tocados(c, &p.id, &nuevos)?;
            hubo_cambios = true;
        }
    }
    Ok(hubo_cambios)
}

/// Valida si `autor` puede eliminar cambios ajenos modificados en `prestamo_id` (#109).
/// Devuelve `(prestamo, rama_destino, cs_merge)`.
pub fn preparar_eliminar_cambios_ajenos(
    c: &Connection,
    autor: &AutorTarea<'_>,
    prestamo_id: &str,
) -> Resultado<(commits::CambioPrestado, String, i64)> {
    let prestamo = commits::obtener_prestamo(c, prestamo_id.trim())?;
    if prestamo.quitado_en.is_some() {
        return Err(Error::Validacion(
            "Esos cambios ya se habían devuelto".into(),
        ));
    }
    if prestamo.archivos_tocados.is_empty() {
        return Err(Error::Validacion(
            "No hay cambios ajenos tocados que eliminar en este préstamo".into(),
        ));
    }
    let cs_merge = prestamo.changeset_merge.ok_or_else(|| {
        Error::Validacion("El préstamo no tiene changeset de merge registrado".into())
    })?;
    let rama_destino = validar_destino_para_prestamo(c, autor, &prestamo.tarea_id)?;
    Ok((prestamo, rama_destino, cs_merge))
}

/// Limpia `archivos_tocados`, registra `changeset_limpio` y publica el evento
/// `eliminar_cambios_ajenos` en `ops/` (#109).
pub fn registrar_eliminar_cambios_ajenos(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    prestamo_id: &str,
    changeset_limpio: Option<i64>,
    ahora: &str,
) -> Resultado<commits::CambioPrestado> {
    let prestamo = commits::obtener_prestamo(c, prestamo_id.trim())?;
    let payload = PayloadEliminarCambiosAjenos {
        prestamo_id: prestamo.id.clone(),
        changeset_limpio,
    };
    let mut ev = Evento::nuevo(
        "eliminar_cambios_ajenos",
        Some(&prestamo.tarea_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar eliminar_cambios_ajenos: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| {
        aplicar_eliminar_cambios_ajenos(tx, &payload)
    })?;
    commits::obtener_prestamo(c, &prestamo.id)
}

/// Marca como devuelto en SQLite y publica `devolver_cambios_prestados` en `ops/`
/// conservando la fila como historial (`quitado_en`, `quitado_por`, #108).
pub fn registrar_devolver_cambios(
    c: &Connection,
    ops: &Path,
    autor: &AutorTarea<'_>,
    prestamo_id: &str,
    ahora: &str,
) -> Resultado<commits::CambioPrestado> {
    let prestamo = commits::obtener_prestamo(c, prestamo_id.trim())?;
    let payload = PayloadDevolverCambios {
        prestamo_id: prestamo.id.clone(),
        quitado_en: ahora.to_string(),
        quitado_por: normalizar_email(autor.email),
    };
    let mut ev = Evento::nuevo(
        "devolver_cambios_prestados",
        Some(&prestamo.tarea_id),
        &normalizar_email(autor.email),
        serde_json::to_value(&payload).map_err(|e| {
            Error::Interno(format!(
                "no se pudo serializar devolver_cambios_prestados: {e}"
            ))
        })?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, ops, &ev, |tx| aplicar_devolver_cambios(tx, &payload))?;
    commits::obtener_prestamo(c, &prestamo.id)
}

/// Filtra cambios menores automáticos de Unity (ej. `.meta`) para no disparar
/// falsas alertas al detectar ediciones en una rama ajena (#111).
pub fn cambios_significativos(rutas: &[String]) -> Vec<String> {
    let mut significativos = std::collections::BTreeSet::new();
    for r in rutas {
        let limpia = r
            .trim()
            .trim_matches('"')
            .trim()
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        if !limpia.is_empty() && !limpia.to_ascii_lowercase().ends_with(".meta") {
            significativos.insert(limpia);
        }
    }
    significativos.into_iter().collect()
}

/// Evalúa si el workspace actual apunta a `main`, `dev` o una rama que no es de las
/// tareas asignadas al usuario y registra cambios significativos sin check-in o un
/// check-in reciente del propio usuario (#111). No modifica ni deshace nada.
pub fn evaluar_alerta_rama_ajena(
    todas: &[Tarea],
    usuario_email: &str,
    rama_actual: &str,
    pendientes_ws: &[String],
    commits_recientes_propios: &[String],
) -> Option<AlertaRamaAjena> {
    let rama = rama_actual.trim().trim_end_matches('/');
    if rama.is_empty() {
        return None;
    }
    let yo = normalizar_email(usuario_email);
    if yo.is_empty() {
        return None;
    }

    let mut candidatos = Vec::with_capacity(pendientes_ws.len() + commits_recientes_propios.len());
    candidatos.extend(pendientes_ws.iter().cloned());
    candidatos.extend(commits_recientes_propios.iter().cloned());
    let archivos = cambios_significativos(&candidatos);
    if archivos.is_empty() {
        return None;
    }

    let activa_en_rama = todas.iter().find(|t| {
        !t.eliminada
            && t.rama_asociada
                .as_deref()
                .map(|r| r.trim().trim_end_matches('/').eq_ignore_ascii_case(rama))
                .unwrap_or(false)
    });

    if let Some(t) = activa_en_rama {
        if t.estado != EstadoTarea::Terminada {
            let es_mia =
                t.asignado_a.as_deref().map(normalizar_email).as_deref() == Some(yo.as_str());
            if es_mia {
                return None;
            }
            // Si es rama padre contenedora sin asignar donde el usuario trabaja una subtarea
            // y son solo cambios locales sin check-in, el flujo "En espera" (#106) los recoge.
            if t.asignado_a.is_none() && commits_recientes_propios.is_empty() {
                let tiene_subtarea_mia = todas.iter().any(|h| {
                    !h.eliminada
                        && h.tarea_padre_id.as_deref() == Some(t.id.as_str())
                        && h.tipo != TipoTarea::SinDefinir
                        && h.asignado_a.as_deref().map(normalizar_email).as_deref()
                            == Some(yo.as_str())
                });
                if tiene_subtarea_mia {
                    return None;
                }
            }
        }
    }

    let sugerida = [
        EstadoTarea::EnProduccion,
        EstadoTarea::PorHacer,
        EstadoTarea::EnRevision,
    ]
    .into_iter()
    .find_map(|est| {
        todas.iter().find(|t| {
            !t.eliminada
                && t.estado == est
                && t.rama_asociada.is_some()
                && t.asignado_a.as_deref().map(normalizar_email).as_deref() == Some(yo.as_str())
        })
    });

    let rama_sugerida = sugerida.and_then(|t| t.rama_asociada.clone());
    let tarea_sugerida_id = sugerida.map(|t| t.id.clone());
    let tarea_ajena_id = activa_en_rama.map(|t| t.id.clone());
    let dueno_rama = activa_en_rama.and_then(|t| t.asignado_a.clone());

    let donde = match tarea_ajena_id {
        Some(ref tid) => format!("la rama de la tarea {tid}, que no te pertenece"),
        None => format!("{rama}, que no pertenece a ninguna de tus tareas"),
    };
    let recomendacion = match rama_sugerida {
        Some(ref r) => format!(" Cambia a tu rama asignada {r} antes de continuar."),
        None => String::new(),
    };
    let mensaje = format!("Estás haciendo cambios en {donde}.{recomendacion}");

    Some(AlertaRamaAjena {
        rama_actual: rama.to_string(),
        tarea_ajena_id,
        dueno_rama,
        rama_sugerida,
        tarea_sugerida_id,
        archivos,
        mensaje,
    })
}

fn aplicar_crear_tarea(
    c: &Connection,
    tarea: &Tarea,
    ahora: &str,
    notificar_pendiente: bool,
) -> Resultado<()> {
    validar_tipo_creacion(tarea.tipo)?;
    if tarea.estado != EstadoTarea::PorHacer {
        return Err(Error::Validacion(
            "Una tarea nueva debe nacer en 'por hacer'".into(),
        ));
    }
    if let Some(ref pid) = tarea.tarea_padre_id {
        validar_padre(c, pid)?;
    }
    db_tareas::insertar(c, tarea)?;
    if notificar_pendiente && tarea.pendiente_aprobacion {
        emitir_a_roles_si_falta(c, &[Rol::Lider, Rol::Administrador], |correo| {
            Notificacion {
                id: format!("noti_apr_pend_{}_{}_{}", tarea.id, correo, ahora),
                tipo: TipoNotificacion::AprobacionTarea,
                para: correo.to_string(),
                titulo: "Tarea pendiente de aprobación".into(),
                cuerpo: format!(
                    "\"{}\" ({}) fue creada por {} y espera tu aprobación.",
                    tarea.titulo, tarea.id, tarea.creado_por
                ),
                leida: false,
                fecha: ahora.to_string(),
                tarea_id: Some(tarea.id.clone()),
                accionable: true,
            }
        })?;
    }
    Ok(())
}

fn aplicar_crear_tarea_con_subtareas(
    c: &Connection,
    payload: &PayloadTareaConSubtareas,
    ahora: &str,
) -> Resultado<()> {
    aplicar_crear_tarea(c, &payload.tarea, ahora, true)?;
    for st in &payload.subtareas {
        if st.tarea_padre_id.as_deref() != Some(payload.tarea.id.as_str()) {
            return Err(Error::Validacion(format!(
                "La subtarea {} no apunta a la tarea raíz {}",
                st.id, payload.tarea.id
            )));
        }
        aplicar_crear_tarea(c, st, ahora, false)?;
    }
    Ok(())
}

/// Resuelve el rol verificado de `email` en la tabla `usuarios`, si existe.
pub(crate) fn rol_verificado_de(
    c: &Connection,
    publica_admin: &str,
    email: &str,
) -> Resultado<Option<Rol>> {
    let email = normalizar_email(email);
    if publica_admin.is_empty() {
        return Ok(usuarios::buscar(c, &email)?.map(|u| u.rol));
    }
    Ok(verificar_usuarios(publica_admin, usuarios::listar(c)?)
        .into_iter()
        .find(|u| u.usuario.email == email && u.verificacion == Verificacion::Verificado)
        .and_then(|u| u.rol_efectivo))
}

fn aplicar_cambios_en_fila(tarea: &mut Tarea, c_edit: &CambiosTarea) -> Resultado<()> {
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
    Ok(())
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

    aplicar_cambios_en_fila(&mut tarea, &payload.cambios)?;
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
            aplicar_crear_tarea(c, &tarea, &evento.timestamp, true)
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
            aplicar_crear_tarea_con_subtareas(c, &payload, &evento.timestamp)
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
            aplicar_tomar_tarea(c, &evento.usuario, rol, &payload, &evento.timestamp)
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
        "borrar_progreso" => {
            let payload: PayloadBorrarProgreso = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("borrar_progreso inválido: {e}")))?;
            aplicar_borrar_progreso(c, &payload)
        }
        "reabrir_tarea" => {
            let payload: PayloadReabrirTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("reabrir_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_reabrir_tarea(c, rol, &payload, &evento.timestamp)
        }
        "aprobar_tarea" => {
            let payload: PayloadDesasignarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("aprobar_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_aprobar_tareas(c, rol, &[payload.id])
        }
        "aprobar_tareas" => {
            let payload: PayloadAprobarTareas = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("aprobar_tareas inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_aprobar_tareas(c, rol, &payload.ids)
        }
        "editar_y_aprobar_tarea" => {
            let payload: PayloadEditarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("editar_y_aprobar_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_editar_y_aprobar_tarea(c, rol, &payload, &evento.timestamp)
        }
        "advertir_tarea_pendiente" => {
            let payload: PayloadAdvertirTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| {
                    Error::Validacion(format!("advertir_tarea_pendiente inválido: {e}"))
                })?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_advertir_tarea_pendiente(c, rol, &payload, &evento.timestamp)
        }
        "proponer_edicion" => {
            let payload: PayloadProponerEdicion = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("proponer_edicion inválido: {e}")))?;
            aplicar_proponer_edicion(c, &evento.usuario, &payload, &evento.timestamp)
        }
        "aprobar_modificacion" => {
            let payload: PayloadDesasignarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("aprobar_modificacion inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_aprobar_modificaciones(c, rol, &[payload.id])
        }
        "aprobar_modificaciones" => {
            let payload: PayloadAprobarTareas = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("aprobar_modificaciones inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_aprobar_modificaciones(c, rol, &payload.ids)
        }
        "rechazar_modificacion" => {
            let payload: PayloadDesasignarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("rechazar_modificacion inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_rechazar_modificacion(c, rol, &payload.id)
        }
        "eliminar_tarea" => {
            let payload: PayloadEliminarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("eliminar_tarea inválido: {e}")))?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_eliminar_tarea(c, rol, &payload, &evento.timestamp)?;
            Ok(())
        }
        "eliminar_definitivamente" => {
            let payload: PayloadDesasignarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| {
                    Error::Validacion(format!("eliminar_definitivamente inválido: {e}"))
                })?;
            let rol = rol_verificado_de(c, publica_admin, &evento.usuario)?;
            aplicar_eliminar_definitivamente(c, rol, &payload.id)?;
            Ok(())
        }
        "hacer_checkin" => {
            let payload: PayloadHacerCheckin = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("hacer_checkin inválido: {e}")))?;
            aplicar_hacer_checkin(c, &payload)
        }
        "crear_en_espera" => {
            let tarea: Tarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_en_espera inválido: {e}")))?;
            aplicar_crear_en_espera(c, &tarea)
        }
        "quitar_en_espera" => {
            let payload: PayloadDesasignarTarea = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("quitar_en_espera inválido: {e}")))?;
            aplicar_quitar_en_espera(c, &payload.id)
        }
        "crear_rama_desde_espera" => {
            let payload: PayloadCrearRamaDesdeEspera =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("crear_rama_desde_espera inválido: {e}"))
                })?;
            aplicar_crear_rama_desde_espera(c, &payload, &evento.timestamp)
        }
        "traer_cambios" => {
            let payload: PayloadTraerCambios = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("traer_cambios inválido: {e}")))?;
            aplicar_traer_cambios(c, &payload)
        }
        "devolver_cambios_prestados" => {
            let payload: PayloadDevolverCambios = serde_json::from_value(evento.payload.clone())
                .map_err(|e| {
                    Error::Validacion(format!("devolver_cambios_prestados inválido: {e}"))
                })?;
            aplicar_devolver_cambios(c, &payload)
        }
        "eliminar_cambios_ajenos" => {
            let payload: PayloadEliminarCambiosAjenos =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("eliminar_cambios_ajenos inválido: {e}"))
                })?;
            aplicar_eliminar_cambios_ajenos(c, &payload)
        }
        "tarea_cerrada_por_deteccion" => {
            let payload: PayloadTareaCerradaDeteccion =
                serde_json::from_value(evento.payload.clone()).map_err(|e| {
                    Error::Validacion(format!("tarea_cerrada_por_deteccion inválido: {e}"))
                })?;
            aplicar_cerrar_por_deteccion(c, &payload.id, &payload.fecha_completada)
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
    ahora: &str,
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
    tarea.actividad_desde = Some(ahora.to_string());
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
    tarea.actividad_desde = Some(ahora.to_string());
    if tarea.rama_asociada.is_none() {
        tarea.rama_asociada = Some(rama);
        tarea.modo_trabajo = Some(MODO_RAMA.to_string());
        if tarea.ultimo_changeset_visto.is_none() {
            tarea.ultimo_changeset_visto = payload.ultimo_changeset_visto;
        }
    }
    db_tareas::actualizar(c, &tarea)?;

    emitir_si_falta(
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
    tarea.actividad_desde = None;
    db_tareas::actualizar(c, &tarea)?;

    for mut sub in descendientes {
        sub.asignado_a = None;
        sub.estado = EstadoTarea::PorHacer;
        sub.actividad_desde = None;
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
        if tarea.tipo == TipoTarea::SinDefinir {
            return Err(Error::Validacion(
                "La subtarea En espera aún no tiene rama propia; crea su rama primero".into(),
            ));
        }
        if tarea.pendiente_aprobacion {
            return Err(Error::Validacion(format!(
                "La tarea {} sigue pendiente de aprobación",
                tarea.id
            )));
        }
        if descendientes
            .iter()
            .any(|h| h.tipo == TipoTarea::SinDefinir)
        {
            return Err(Error::Validacion(format!(
                "La tarea {} tiene cambios En espera: crea una rama para ellos antes de enviarla a revisión",
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
        if payload.estado == EstadoTarea::Terminada {
            let tocados = prestamos_con_tocados_origenes(c, &ids_subarbol)?;
            if !tocados.is_empty() {
                return Err(Error::Validacion(format!(
                    "Se tocaron cambios ajenos prestados de {}: elimínalos antes de terminar {}",
                    tocados.join(", "),
                    tarea.id
                )));
            }
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
    if matches!(
        payload.estado,
        EstadoTarea::PorHacer | EstadoTarea::EnProduccion
    ) && anterior_estado != payload.estado
    {
        tarea.actividad_desde = Some(ahora.to_string());
    }
    if payload.estado == EstadoTarea::Terminada {
        tarea.fecha_completada = Some(fecha_corta_de_iso(ahora));
    }
    db_tareas::actualizar(c, &tarea)?;

    if propaga {
        for mut sub in descendientes {
            if payload.estado != EstadoTarea::PorHacer && sub.asignado_a.is_none() {
                sub.asignado_a = Some(autor.clone());
            }
            if matches!(
                payload.estado,
                EstadoTarea::PorHacer | EstadoTarea::EnProduccion
            ) && sub.estado != payload.estado
            {
                sub.actividad_desde = Some(ahora.to_string());
            }
            sub.estado = payload.estado;
            db_tareas::actualizar(c, &sub)?;
        }
    }

    recalcular_padres(c, tarea.tarea_padre_id)?;

    if anterior_estado != EstadoTarea::EnRevision && payload.estado == EstadoTarea::EnRevision {
        emitir_a_roles_si_falta(c, &[Rol::Lider, Rol::Administrador], |correo| {
            Notificacion {
                id: format!("noti_rev_{}_{}_{}", tarea.id, correo, ahora),
                tipo: TipoNotificacion::TareaARevision,
                para: correo.to_string(),
                titulo: "Una tarea pasó a revisión".into(),
                cuerpo: format!("\"{}\" ({}) pasó a revisión.", tarea.titulo, tarea.id),
                leida: false,
                fecha: ahora.to_string(),
                tarea_id: Some(tarea.id.clone()),
                accionable: false,
            }
        })?;
    }

    Ok(aviso_prestamos)
}

fn aplicar_borrar_progreso(c: &Connection, payload: &PayloadBorrarProgreso) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada; reábrela primero si necesitas revertirla",
            tarea.id
        )));
    }

    let descendientes = descendientes_activos(c, &tarea.id)?;
    commits::borrar_de_tarea(c, &tarea.id)?;
    commits::borrar_prestamos_de_tarea(c, &tarea.id)?;
    tarea.estado = EstadoTarea::PorHacer;
    db_tareas::actualizar(c, &tarea)?;

    for mut sub in descendientes {
        commits::borrar_de_tarea(c, &sub.id)?;
        commits::borrar_prestamos_de_tarea(c, &sub.id)?;
        sub.estado = EstadoTarea::PorHacer;
        db_tareas::actualizar(c, &sub)?;
    }

    recalcular_padres(c, tarea.tarea_padre_id)?;
    Ok(())
}

fn aplicar_reabrir_tarea(
    c: &Connection,
    rol_conocido: Option<Rol>,
    payload: &PayloadReabrirTarea,
    ahora: &str,
) -> Resultado<()> {
    let motivo = payload.motivo.trim();
    if motivo.is_empty() {
        return Err(Error::Validacion(
            "Escribe un motivo para reabrir la tarea".into(),
        ));
    }
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.estado != EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "Solo se pueden reabrir tareas terminadas ({} está en '{}')",
            tarea.id,
            tarea.estado.como_texto()
        )));
    }
    if tarea.reaperturas >= 3 {
        if let Some(rol) = rol_conocido {
            if rol != Rol::Administrador {
                return Err(Error::Validacion(
                    "Esta tarea ya se reabrió 3 veces; solo un Administrador puede volver a reabrirla"
                        .into(),
                ));
            }
        }
    }

    tarea.reaperturas += 1;
    tarea.estado = EstadoTarea::EnRevision;
    db_tareas::actualizar(c, &tarea)?;
    recalcular_padres(c, tarea.tarea_padre_id.clone())?;

    emitir_a_roles_si_falta(c, &[Rol::Lider, Rol::Administrador], |correo| {
        Notificacion {
            id: format!("noti_reabrir_{}_{}_{}", tarea.id, tarea.reaperturas, correo),
            tipo: TipoNotificacion::TareaReabierta,
            para: correo.to_string(),
            titulo: "Tarea reabierta".into(),
            cuerpo: format!("{} ({}/3): {motivo}", tarea.id, tarea.reaperturas),
            leida: false,
            fecha: ahora.to_string(),
            tarea_id: Some(tarea.id.clone()),
            accionable: false,
        }
    })?;
    Ok(())
}

fn aplicar_aprobar_tareas(
    c: &Connection,
    rol_conocido: Option<Rol>,
    ids: &[String],
) -> Resultado<()> {
    exigir_lider_o_admin(rol_conocido)?;
    if ids.is_empty() {
        return Err(Error::Validacion(
            "Selecciona al menos una tarea para aprobar".into(),
        ));
    }
    for id in ids {
        let mut tarea = db_tareas::obtener(c, id)?;
        if tarea.eliminada {
            return Err(Error::Validacion(format!(
                "La tarea {} está en la Papelera y no se puede aprobar",
                tarea.id
            )));
        }
        if tarea.pendiente_aprobacion {
            tarea.pendiente_aprobacion = false;
            db_tareas::actualizar(c, &tarea)?;
        }
    }
    Ok(())
}

fn aplicar_editar_y_aprobar_tarea(
    c: &Connection,
    rol_conocido: Option<Rol>,
    payload: &PayloadEditarTarea,
    ahora: &str,
) -> Resultado<()> {
    exigir_lider_o_admin(rol_conocido)?;
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    aplicar_cambios_en_fila(&mut tarea, &payload.cambios)?;
    tarea.pendiente_aprobacion = false;
    db_tareas::actualizar(c, &tarea)?;

    emitir_si_falta(
        c,
        &Notificacion {
            id: format!("noti_edit_apr_{}_{}", tarea.id, ahora),
            tipo: TipoNotificacion::EditadaAprobada,
            para: normalizar_email(&tarea.creado_por),
            titulo: "Tu tarea pendiente fue editada y aprobada".into(),
            cuerpo: format!(
                "\"{}\" ({}) fue editada y aprobada.",
                tarea.titulo, tarea.id
            ),
            leida: false,
            fecha: ahora.to_string(),
            tarea_id: Some(tarea.id),
            accionable: false,
        },
    )
}

fn aplicar_advertir_tarea_pendiente(
    c: &Connection,
    rol_conocido: Option<Rol>,
    payload: &PayloadAdvertirTarea,
    ahora: &str,
) -> Resultado<()> {
    exigir_lider_o_admin(rol_conocido)?;
    let mensaje = payload.mensaje.trim();
    if mensaje.is_empty() {
        return Err(Error::Validacion(
            "Escribe un mensaje para advertir sobre la tarea".into(),
        ));
    }
    let tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if !tarea.pendiente_aprobacion {
        return Err(Error::Validacion(format!(
            "La tarea {} ya fue aprobada",
            tarea.id
        )));
    }
    emitir_si_falta(
        c,
        &Notificacion {
            id: format!("noti_adv_{}_{}", tarea.id, ahora),
            tipo: TipoNotificacion::Advertencia,
            para: normalizar_email(&tarea.creado_por),
            titulo: "Advertencia sobre tu tarea pendiente".into(),
            cuerpo: format!("Sobre \"{}\" ({}): {mensaje}", tarea.titulo, tarea.id),
            leida: false,
            fecha: ahora.to_string(),
            tarea_id: Some(tarea.id),
            accionable: false,
        },
    )
}

fn aplicar_proponer_edicion(
    c: &Connection,
    autor_email: &str,
    payload: &PayloadProponerEdicion,
    ahora: &str,
) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera y no se puede editar",
            tarea.id
        )));
    }
    if tarea.estado != EstadoTarea::PorHacer {
        return Err(Error::Validacion(
            "Solo se puede editar una tarea en 'Sin asignar' o 'Por hacer'".into(),
        ));
    }

    let titulo = payload.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion("El título es obligatorio".into()));
    }
    let detalles = payload.detalles.trim();

    let mut nuevos = ediciones::Campos::new();
    let mut anteriores = ediciones::Campos::new();
    if titulo != tarea.titulo {
        nuevos.insert("titulo".into(), titulo.to_string());
        anteriores.insert("titulo".into(), tarea.titulo.clone());
    }
    if detalles != tarea.detalles {
        nuevos.insert("detalles".into(), detalles.to_string());
        anteriores.insert("detalles".into(), tarea.detalles.clone());
    }
    if nuevos.is_empty() {
        return Err(Error::Validacion(
            "No hay cambios en título ni detalles".into(),
        ));
    }

    let campos_cambiados: Vec<&str> = nuevos.keys().map(String::as_str).collect();
    let resumen_campos = campos_cambiados.join(", ");
    let creador = normalizar_email(autor_email);

    ediciones::guardar_o_sumar(
        c,
        &ediciones::EdicionPendiente {
            id: payload.edicion_id.clone(),
            tarea_id: tarea.id.clone(),
            campos_nuevos: nuevos,
            campos_anteriores: anteriores,
            motivo: limpiar_opcional(payload.motivo.clone()),
            creado_por: creador.clone(),
            fecha: payload.fecha.clone(),
        },
    )?;

    tarea.titulo = titulo.to_string();
    tarea.detalles = detalles.to_string();
    db_tareas::actualizar(c, &tarea)?;

    emitir_a_roles_si_falta(c, &[Rol::Lider, Rol::Administrador], |correo| {
        Notificacion {
            id: format!("noti_ep_{}_{}_{}", tarea.id, correo, ahora),
            tipo: TipoNotificacion::EdicionPendiente,
            para: correo.to_string(),
            titulo: "Hay una edición de tarea por aprobar".into(),
            cuerpo: format!(
                "\"{}\" ({}) fue editada por {} ({}).",
                tarea.titulo, tarea.id, creador, resumen_campos
            ),
            leida: false,
            fecha: ahora.to_string(),
            tarea_id: Some(tarea.id.clone()),
            accionable: true,
        }
    })?;
    Ok(())
}

fn aplicar_aprobar_modificaciones(
    c: &Connection,
    rol_conocido: Option<Rol>,
    ids: &[String],
) -> Resultado<()> {
    exigir_lider_o_admin(rol_conocido)?;
    if ids.is_empty() {
        return Err(Error::Validacion(
            "Selecciona al menos una edición para aprobar".into(),
        ));
    }
    for id in ids {
        let tarea = db_tareas::obtener(c, id)?;
        if tarea.eliminada {
            return Err(Error::Validacion(format!(
                "La tarea {} está en la Papelera",
                tarea.id
            )));
        }
        if ediciones::quitar(c, &tarea.id)?.is_none() {
            return Err(Error::Validacion(format!(
                "La tarea {} no tiene ediciones pendientes",
                tarea.id
            )));
        }
    }
    Ok(())
}

fn aplicar_rechazar_modificacion(
    c: &Connection,
    rol_conocido: Option<Rol>,
    id: &str,
) -> Resultado<()> {
    exigir_lider_o_admin(rol_conocido)?;
    let mut tarea = db_tareas::obtener(c, id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    let edicion = ediciones::quitar(c, &tarea.id)?.ok_or_else(|| {
        Error::Validacion(format!(
            "La tarea {} no tiene ediciones pendientes",
            tarea.id
        ))
    })?;
    if let Some(prev_titulo) = edicion.campos_anteriores.get("titulo") {
        tarea.titulo = prev_titulo.clone();
    }
    if let Some(prev_detalles) = edicion.campos_anteriores.get("detalles") {
        tarea.detalles = prev_detalles.clone();
    }
    db_tareas::actualizar(c, &tarea)
}

fn recolectar_descendientes_activas(
    c: &Connection,
    padre_id: &str,
    salida: &mut Vec<Tarea>,
) -> Resultado<()> {
    for hija in db_tareas::hijas(c, padre_id)? {
        if !hija.eliminada && hija.estado != EstadoTarea::Terminada {
            let hija_id = hija.id.clone();
            salida.push(hija);
            recolectar_descendientes_activas(c, &hija_id, salida)?;
        }
    }
    Ok(())
}

fn recolectar_descendientes_papelera(
    c: &Connection,
    padre_id: &str,
    salida: &mut Vec<Tarea>,
) -> Resultado<()> {
    for hija in db_tareas::hijas(c, padre_id)? {
        if hija.eliminada {
            let hija_id = hija.id.clone();
            salida.push(hija);
            recolectar_descendientes_papelera(c, &hija_id, salida)?;
        }
    }
    Ok(())
}

fn aplicar_eliminar_tarea(
    c: &Connection,
    rol_conocido: Option<Rol>,
    payload: &PayloadEliminarTarea,
    ahora: &str,
) -> Resultado<Vec<Tarea>> {
    exigir_lider_o_admin(rol_conocido)?;
    let justificacion = payload.justificacion.trim();
    if justificacion.is_empty() {
        return Err(Error::Validacion(
            "Escribe la justificación para eliminar la tarea".into(),
        ));
    }
    let raiz = db_tareas::obtener(c, &payload.id)?;
    if raiz.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está en la Papelera",
            raiz.id
        )));
    }
    let raiz_id = raiz.id.clone();
    let padre_raiz = raiz.tarea_padre_id.clone();

    let mut cola = vec![raiz];
    recolectar_descendientes_activas(c, &raiz_id, &mut cola)?;

    let mut actualizadas = Vec::with_capacity(cola.len());
    for (idx, mut tarea) in cola.into_iter().enumerate() {
        let es_raiz = idx == 0;
        let motivo_fila = if es_raiz {
            justificacion.to_string()
        } else {
            format!("eliminada junto con su tarea padre {raiz_id}: {justificacion}")
        };
        let cuerpo = if es_raiz {
            format!(
                "\"{}\" ({}) fue enviada a la Papelera. Motivo: {}",
                tarea.titulo, tarea.id, justificacion
            )
        } else {
            format!(
                "\"{}\" ({}) fue enviada a la Papelera junto con su tarea padre {raiz_id}. Motivo: {justificacion}",
                tarea.titulo, tarea.id
            )
        };

        tarea.eliminada = true;
        tarea.justificacion_eliminacion = Some(motivo_fila);
        tarea.fecha_eliminacion = Some(ahora.to_string());
        tarea.pendiente_aprobacion = false;
        ediciones::quitar(c, &tarea.id)?;
        db_tareas::actualizar(c, &tarea)?;

        emitir_si_falta(
            c,
            &Notificacion {
                id: format!("noti_elim_{}_{}", tarea.id, ahora),
                tipo: TipoNotificacion::TareaEliminada,
                para: normalizar_email(&tarea.creado_por),
                titulo: "Tu tarea fue eliminada".into(),
                cuerpo,
                leida: false,
                fecha: ahora.to_string(),
                tarea_id: Some(tarea.id.clone()),
                accionable: false,
            },
        )?;
        actualizadas.push(tarea);
    }

    if let Some(pid) = padre_raiz.as_deref() {
        if let Some(padre) = db_tareas::buscar(c, pid)? {
            if !padre.eliminada {
                recalcular_padres(c, Some(pid.to_string()))?;
            }
        }
    }

    Ok(actualizadas)
}

fn aplicar_eliminar_definitivamente(
    c: &Connection,
    rol_conocido: Option<Rol>,
    id: &str,
) -> Resultado<Vec<Tarea>> {
    exigir_lider_o_admin(rol_conocido)?;
    let raiz = db_tareas::obtener(c, id)?;
    if !raiz.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} debe estar en la Papelera para eliminarla definitivamente",
            raiz.id
        )));
    }
    let raiz_id = raiz.id.clone();
    let mut purgadas = vec![raiz];
    recolectar_descendientes_papelera(c, &raiz_id, &mut purgadas)?;

    // Borrar de hojas a raíz por orden limpio.
    for t in purgadas.iter().rev() {
        db_tareas::borrar_fila(c, &t.id)?;
    }
    Ok(purgadas)
}

fn aplicar_cerrar_por_deteccion(c: &Connection, id: &str, fecha_completada: &str) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.estado == EstadoTarea::Terminada {
        return Ok(());
    }
    let padre = tarea.tarea_padre_id.clone();
    tarea.estado = EstadoTarea::Terminada;
    tarea.fecha_completada = Some(fecha_completada.to_string());
    tarea.modificada = false;
    db_tareas::actualizar(c, &tarea)?;
    recalcular_padres(c, padre)
}

fn aplicar_hacer_checkin(c: &Connection, payload: &PayloadHacerCheckin) -> Resultado<()> {
    let mut tarea = db_tareas::obtener(c, &payload.commit.tarea_id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if let Some(tipo) = payload.commit.tipo {
        validar_tipo_creacion(tipo)?;
    }
    commits::guardar(c, &payload.commit)?;
    let visto = tarea
        .ultimo_changeset_visto
        .unwrap_or(0)
        .max(payload.changeset);
    let mut cambio = false;
    if tarea.ultimo_changeset_visto != Some(visto) {
        tarea.ultimo_changeset_visto = Some(visto);
        cambio = true;
    }
    if !payload.commit.fecha.trim().is_empty() {
        tarea.actividad_desde = Some(payload.commit.fecha.clone());
        cambio = true;
    }
    if cambio {
        db_tareas::actualizar(c, &tarea)?;
    }
    Ok(())
}

fn aplicar_crear_en_espera(c: &Connection, tarea: &Tarea) -> Resultado<()> {
    if tarea.tipo != TipoTarea::SinDefinir {
        return Err(Error::Validacion(
            "La subtarea En espera debe ser de tipo 'Sin definir'".into(),
        ));
    }
    let padre_id = tarea
        .tarea_padre_id
        .as_deref()
        .ok_or_else(|| Error::Validacion("La subtarea En espera requiere tarea_padre_id".into()))?;
    let padre = db_tareas::obtener(c, padre_id)?;
    if padre.eliminada || padre.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea padre {} ya está cerrada o en la Papelera",
            padre.id
        )));
    }
    if db_tareas::hijas(c, padre_id)?
        .into_iter()
        .any(|h| !h.eliminada && h.tipo == TipoTarea::SinDefinir)
    {
        return Ok(());
    }
    db_tareas::insertar(c, tarea)?;
    recalcular_padres(c, Some(padre.id))
}

fn aplicar_quitar_en_espera(c: &Connection, id: &str) -> Resultado<()> {
    let Some(tarea) = db_tareas::buscar(c, id)? else {
        return Ok(());
    };
    if tarea.tipo != TipoTarea::SinDefinir {
        return Err(Error::Validacion(format!(
            "La tarea {} ya fue convertida en subtarea y no se puede quitar como En espera",
            tarea.id
        )));
    }
    let padre_id = tarea.tarea_padre_id.clone();
    db_tareas::borrar_fila(c, &tarea.id)?;
    recalcular_padres(c, padre_id)
}

fn aplicar_crear_rama_desde_espera(
    c: &Connection,
    payload: &PayloadCrearRamaDesdeEspera,
    ahora: &str,
) -> Resultado<()> {
    validar_tipo_creacion(payload.tipo)?;
    let titulo = payload.titulo.trim();
    if titulo.is_empty() {
        return Err(Error::Validacion(
            "El título de la subtarea es obligatorio".into(),
        ));
    }
    let mut tarea = db_tareas::obtener(c, &payload.id)?;
    if tarea.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            tarea.id
        )));
    }
    if tarea.tipo != TipoTarea::SinDefinir {
        return Err(Error::Validacion(format!(
            "La tarea {} no está En espera",
            tarea.id
        )));
    }
    let rama = validar_ruta_rama_tarea(&payload.rama)?;
    let padre = tarea.tarea_padre_id.clone();

    tarea.titulo = titulo.to_string();
    tarea.detalles = payload.detalles.trim().to_string();
    tarea.tipo = payload.tipo;
    tarea.estado = EstadoTarea::EnProduccion;
    tarea.pendiente_aprobacion = false;
    tarea.asignado_a = Some(normalizar_email(&payload.asignado_a));
    tarea.modo_trabajo = Some(MODO_RAMA.to_string());
    tarea.rama_asociada = Some(rama);
    tarea.actividad_desde = Some(ahora.to_string());
    tarea.ultimo_changeset_visto = Some(payload.ultimo_changeset_visto);
    db_tareas::actualizar(c, &tarea)?;
    recalcular_padres(c, padre)
}

fn aplicar_traer_cambios(c: &Connection, payload: &PayloadTraerCambios) -> Resultado<()> {
    let mut destino = db_tareas::obtener(c, &payload.prestamo.tarea_id)?;
    if destino.eliminada {
        return Err(Error::Validacion(format!(
            "La tarea {} está en la Papelera",
            destino.id
        )));
    }
    if destino.estado == EstadoTarea::Terminada {
        return Err(Error::Validacion(format!(
            "La tarea {} ya está terminada",
            destino.id
        )));
    }
    let _origen = db_tareas::obtener(c, &payload.prestamo.origen_id)?;
    commits::insertar_prestamo(c, &payload.prestamo)?;
    if let Some(cs) = payload.ultimo_changeset_visto {
        let visto = destino.ultimo_changeset_visto.unwrap_or(0).max(cs);
        if destino.ultimo_changeset_visto != Some(visto) {
            destino.ultimo_changeset_visto = Some(visto);
            db_tareas::actualizar(c, &destino)?;
        }
    }
    Ok(())
}

fn aplicar_devolver_cambios(c: &Connection, payload: &PayloadDevolverCambios) -> Resultado<()> {
    commits::quitar_prestamo(
        c,
        &payload.prestamo_id,
        &payload.quitado_por,
        &payload.quitado_en,
    )
}

fn aplicar_eliminar_cambios_ajenos(
    c: &Connection,
    payload: &PayloadEliminarCambiosAjenos,
) -> Resultado<()> {
    let prestamo = commits::obtener_prestamo(c, &payload.prestamo_id)?;
    commits::limpiar_tocados(c, &prestamo.id, payload.changeset_limpio)?;
    if let Some(cs) = payload.changeset_limpio {
        let mut tarea = db_tareas::obtener(c, &prestamo.tarea_id)?;
        let visto = tarea.ultimo_changeset_visto.unwrap_or(0).max(cs);
        if tarea.ultimo_changeset_visto != Some(visto) {
            tarea.ultimo_changeset_visto = Some(visto);
            db_tareas::actualizar(c, &tarea)?;
        }
    }
    Ok(())
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
            emitir_si_falta(c, &n)?;
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
    emitir_si_falta(c, &n)?;
    Ok(aplicado_entrante)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::notificaciones;
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
            let notifs: Vec<_> = notificaciones::de_usuario(e.db_b.conexion(), correo)
                .unwrap()
                .into_iter()
                .filter(|n| n.tipo == TipoNotificacion::Advertencia)
                .collect();
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
                    .into_iter()
                    .filter(|n| n.tipo == TipoNotificacion::Advertencia)
                    .count(),
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

    struct EntornoRealPrueba {
        repo_cm: String,
        compartida: PathBuf,
        estado_a: crate::estado::AppState,
        db_b: Db,
        _ws_a_dir: tempfile::TempDir,
        ws_ruta: PathBuf,
        _datos_a: tempfile::TempDir,
        _maquina_b: tempfile::TempDir,
        local_b: PathBuf,
        ops_a: PathBuf,
        ops_b: PathBuf,
        publica: String,
        llave_admin: LlavePrivada,
        admin_fila: usuarios::Usuario,
        ramas_creadas: Vec<String>,
    }

    impl EntornoRealPrueba {
        fn nuevo(etiqueta: &str) -> Self {
            use crate::cm::Comando;
            use crate::comandos::acceso::entrar_en;
            use crate::comandos::datos::carpeta_abierta;
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
            let compartida = nube.join(format!("BranchBoard-prueba-{etiqueta}-{sufijo}"));
            std::fs::create_dir_all(&compartida).unwrap();

            let estado_a = AppState::default();
            let ws_a_dir = tempfile::tempdir().unwrap();
            let datos_a = tempfile::tempdir().unwrap();
            let maquina_b = tempfile::tempdir().unwrap();
            let ws_ruta = ws_a_dir.path().to_path_buf();

            Comando::cm()
                .args(["workspace", "create"])
                .arg(format!("bb-prueba-{etiqueta}-{sufijo}"))
                .arg(ws_ruta.to_string_lossy().as_ref())
                .arg(format!("rep:{repo_cm}"))
                .exigir_exito(&estado_a)
                .unwrap();

            if let Err(err) = entrar_en(&estado_a, datos_a.path(), &ws_ruta) {
                let _ = Comando::cm()
                    .args(["workspace", "delete"])
                    .arg(ws_ruta.to_string_lossy().as_ref())
                    .ejecutar(&estado_a);
                let _ = std::fs::remove_dir_all(&compartida);
                panic!("entrar_en falló: {err}");
            }

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
            let llave_admin = cargar_llave_admin(&local_a, None).unwrap();
            fijar_llave_publica_admin(&local_b, &publica).unwrap();
            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            Self {
                repo_cm,
                compartida,
                estado_a,
                db_b,
                _ws_a_dir: ws_a_dir,
                ws_ruta,
                _datos_a: datos_a,
                _maquina_b: maquina_b,
                local_b,
                ops_a,
                ops_b,
                publica,
                llave_admin,
                admin_fila,
                ramas_creadas: Vec::new(),
            }
        }

        fn registrar_rama(&mut self, rama: impl Into<String>) {
            let rama = rama.into();
            if !self.ramas_creadas.contains(&rama) {
                self.ramas_creadas.push(rama);
            }
        }

        fn registrar_ramas_de(&mut self, tareas: &[Tarea]) {
            for t in tareas {
                if let Some(ref r) = t.rama_asociada {
                    self.registrar_rama(r.clone());
                }
            }
        }

        fn crear_usuario(&self, email: &str, rol: Rol) -> usuarios::Usuario {
            let firmante = Firmante {
                email: &self.admin_fila.email,
                rol: Rol::Administrador,
                llave: &self.llave_admin,
            };
            self.estado_a
                .con_db(|db_a| {
                    equipo::crear(
                        db_a.conexion(),
                        &self.ops_a,
                        &self.publica,
                        &firmante,
                        email,
                        rol,
                        &crate::modelo::ahora_iso(),
                        &crate::modelo::hoy_local(),
                    )
                })
                .unwrap()
        }

        fn crear_tarea(&self, email: &str, rol: Rol, titulo: &str) -> Tarea {
            let autor = AutorTarea { email, rol };
            self.estado_a
                .con_db(|db_a| {
                    crear(
                        db_a.conexion(),
                        &self.ops_a,
                        &autor,
                        &DatosNuevaTarea {
                            titulo: titulo.into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                        },
                        &crate::modelo::ahora_iso(),
                        &crate::modelo::hoy_local(),
                    )
                })
                .unwrap()
        }

        fn crear_con_subtareas(
            &self,
            email: &str,
            rol: Rol,
            titulo: &str,
            subs: &[&str],
        ) -> Vec<Tarea> {
            let autor = AutorTarea { email, rol };
            self.estado_a
                .con_db(|db_a| {
                    crear_con_subtareas(
                        db_a.conexion(),
                        &self.ops_a,
                        &autor,
                        &DatosNuevaTareaConSubtareas {
                            titulo: titulo.into(),
                            detalles: String::new(),
                            tipo: TipoTarea::Scripting,
                            tarea_padre_id: None,
                            sprint_id: None,
                            backlog_id: None,
                            con_fecha: false,
                            fecha_limite: None,
                            subtareas: subs
                                .iter()
                                .map(|s| DatosSubtareaNueva {
                                    tipo: TipoTarea::UiInterfaz,
                                    titulo: (*s).into(),
                                    detalles: String::new(),
                                })
                                .collect(),
                        },
                        &crate::modelo::ahora_iso(),
                        &crate::modelo::hoy_local(),
                    )
                })
                .unwrap()
        }

        fn commit_real(&self, archivo: &str, contenido: &str, mensaje: &str) {
            use crate::cm::Comando;
            let ruta = self.ws_ruta.join(archivo);
            std::fs::write(&ruta, contenido).unwrap();
            Comando::cm()
                .arg("add")
                .arg(ruta.to_string_lossy().as_ref())
                .en(&self.ws_ruta)
                .exigir_exito(&self.estado_a)
                .unwrap();
            Comando::cm()
                .args(["checkin", "--all", &format!("-c={mensaje}")])
                .en(&self.ws_ruta)
                .exigir_exito(&self.estado_a)
                .unwrap();
        }

        fn volver_a_dev(&self) {
            let _ = crate::ramas::cambiar_a_rama(&self.estado_a, &self.ws_ruta, "/main/dev");
        }

        fn sincronizar_b(&self) -> crate::eventos::lectura::ResumenSincronizacion {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let ahora = Utc::now() + chrono::Duration::minutes(5);
            let sync = lectura::sincronizar(
                self.db_b.conexion(),
                &self.ops_b,
                &self.local_b,
                ahora,
                |c, e| lectura::aplicar_evento(c, &self.publica, e),
            )
            .unwrap();
            assert_eq!(sync.descartados, 0, "no debe descartar eventos válidos");
            sync
        }
    }

    fn correr_entorno_real<F: FnOnce(&mut EntornoRealPrueba)>(etiqueta: &str, f: F) {
        use crate::cm::Comando;

        let mut env = EntornoRealPrueba::nuevo(etiqueta);
        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut env)));

        env.volver_a_dev();
        let ws_str = env.ws_ruta.to_string_lossy().to_string();
        env.estado_a.cerrar_proyecto();
        let _ = Comando::cm()
            .args(["workspace", "delete"])
            .arg(&ws_str)
            .ejecutar(&env.estado_a);

        // Borra ramas en reversa; si alguna quedó por merges cruzados, limpia changesets primero y reintenta.
        let borrar_ramas = |ramas: &[String]| {
            for rama in ramas.iter().rev() {
                let _ = Comando::cm()
                    .args(["branch", "delete", "--delete-changesets"])
                    .arg(format!("br:{rama}@{}", env.repo_cm))
                    .ejecutar(&env.estado_a);
            }
        };
        borrar_ramas(&env.ramas_creadas);
        for _ in 0..2 {
            for rama in &env.ramas_creadas {
                if let Ok(sal) = Comando::cm()
                    .args(["find", "changeset"])
                    .arg(format!(
                        "where branch='{rama}' on repository '{}'",
                        env.repo_cm
                    ))
                    .args(["--format={changesetid}", "--nototal"])
                    .exigir_exito(&env.estado_a)
                {
                    let ids: Vec<&str> = sal
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect();
                    for cs in ids.into_iter().rev() {
                        let _ = Comando::cm()
                            .args(["changeset", "delete", &format!("cs:{cs}@{}", env.repo_cm)])
                            .ejecutar(&env.estado_a);
                    }
                }
            }
        }
        borrar_ramas(&env.ramas_creadas);

        let _ = std::fs::remove_dir_all(&env.compartida);
        drop(env);

        if let Err(err) = resultado {
            std::panic::resume_unwind(err);
        }
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_asignar_y_soltar_tarea() {
        use crate::cm::lectura::rama_actual_workspace;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::{asignar_en, cambiar_estado_en, desasignar_en};

        correr_entorno_real("98", |env| {
            env.crear_usuario(DEV, Rol::Desarrollador);
            let admin_email = env.admin_fila.email.clone();

            let grupo =
                env.crear_con_subtareas(&admin_email, Rol::Administrador, "Padre 98", &["Sub 98"]);
            let padre_id = grupo[0].id.clone();
            let sub_id = grupo[1].id.clone();
            let t2 = env.crear_tarea(&admin_email, Rol::Administrador, "Con commit 98");
            let t3 = env.crear_tarea(&admin_email, Rol::Administrador, "Terminada 98");

            // DEV no puede asignar; Admin no puede asignar tarea con subtareas ni terminada.
            env.estado_a.fijar_usuario(Some(DEV));
            assert!(asignar_en(&env.estado_a, &sub_id, DEV).is_err());
            env.estado_a.fijar_usuario(Some(&admin_email));
            assert!(asignar_en(&env.estado_a, &padre_id, DEV).is_err());

            cambiar_estado_en(&env.estado_a, &t3.id, EstadoTarea::EnProduccion).unwrap();
            cambiar_estado_en(&env.estado_a, &t3.id, EstadoTarea::EnRevision).unwrap();
            cambiar_estado_en(&env.estado_a, &t3.id, EstadoTarea::Terminada).unwrap();
            assert!(asignar_en(&env.estado_a, &t3.id, DEV).is_err());

            let ws_inicial = rama_actual_workspace(&env.estado_a, &env.ws_ruta).unwrap();

            // 1) Admin asigna subtarea a DEV: crea ramas en Unity VC sin cambiar el workspace local.
            let res = asignar_en(&env.estado_a, &sub_id, DEV).unwrap();
            assert_eq!(res.len(), 2);
            env.registrar_ramas_de(&res);
            let rama_sub = res[1]
                .rama_asociada
                .clone()
                .expect("subtarea debe tener rama");
            assert_eq!(res[1].asignado_a.as_deref(), Some(DEV));
            let ws_despues = rama_actual_workspace(&env.estado_a, &env.ws_ruta).unwrap();
            assert_eq!(
                ws_despues.rama, ws_inicial.rama,
                "asignar no debe tocar workspace"
            );

            // Máquina B sincroniza asignación y notificación al DEV.
            env.sincronizar_b();
            let sub_b = db_tareas::obtener(env.db_b.conexion(), &sub_id).unwrap();
            assert_eq!(sub_b.asignado_a.as_deref(), Some(DEV));
            assert_eq!(
                notificaciones::de_usuario(env.db_b.conexion(), DEV)
                    .unwrap()
                    .iter()
                    .filter(|n| n.tipo == TipoNotificacion::TareaAsignada)
                    .count(),
                1
            );

            // 2) Reasignar cuando ya tiene rama conserva la rama y cambia el responsable.
            let res2 = asignar_en(&env.estado_a, &sub_id, &admin_email).unwrap();
            let sub2 = res2.iter().find(|t| t.id == sub_id).unwrap();
            assert_eq!(sub2.rama_asociada.as_deref(), Some(rama_sub.as_str()));
            assert_eq!(sub2.asignado_a.as_deref(), Some(admin_email.as_str()));

            // 3) Soltar la subtarea sin check-ins la deja sin responsable y en 'por hacer'.
            let soltada = desasignar_en(&env.estado_a, &sub_id).unwrap();
            assert_eq!(soltada.asignado_a, None);
            assert_eq!(soltada.estado, EstadoTarea::PorHacer);

            // 4) Asignar t2, hacer check-in real y verificar que desasignar rechaza si hay commits.
            let res_t2 = asignar_en(&env.estado_a, &t2.id, &admin_email).unwrap();
            env.registrar_ramas_de(&res_t2);
            let rama_t2 = res_t2[0].rama_asociada.clone().unwrap();
            crate::ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_t2).unwrap();
            env.commit_real("commit-98.txt", "avance 98", "Check-in real 98");
            revisar_actualizaciones_en(&env.estado_a, Some(&t2.id)).unwrap();
            assert!(
                desasignar_en(&env.estado_a, &t2.id).is_err(),
                "desasignar debe rechazar si ya hay check-ins"
            );

            env.volver_a_dev();
            env.sincronizar_b();
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &sub_id)
                    .unwrap()
                    .asignado_a,
                None
            );
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &t2.id)
                    .unwrap()
                    .asignado_a
                    .as_deref(),
                Some(admin_email.as_str())
            );
        });
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_cambiar_estado() {
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::{aprobar_tareas_en, cambiar_estado_en, tomar_en};

        correr_entorno_real("99", |env| {
            env.crear_usuario(DEV, Rol::Desarrollador);
            env.crear_usuario(TESTER, Rol::Tester);
            let admin_email = env.admin_fila.email.clone();

            // DEV crea padre + 2 subtareas (quedan pendiente_aprobacion = true).
            let grupo = env.crear_con_subtareas(
                DEV,
                Rol::Desarrollador,
                "Padre 99",
                &["Sub 99 A", "Sub 99 B"],
            );
            let padre_id = grupo[0].id.clone();
            let sub1_id = grupo[1].id.clone();
            let sub2_id = grupo[2].id.clone();
            let origen = env.crear_tarea(&admin_email, Rol::Administrador, "Origen prestamo 99");

            // Pendiente de aprobación rechaza pasar a EnRevision.
            assert!(cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::EnRevision).is_err());

            // Admin aprueba las 3 y toma sub1 y sub2 en Unity VC.
            aprobar_tareas_en(
                &env.estado_a,
                &[padre_id.clone(), sub1_id.clone(), sub2_id.clone()],
            )
            .unwrap();
            let t1 = tomar_en(&env.estado_a, &sub1_id).unwrap();
            env.registrar_ramas_de(&t1);
            let t2 = tomar_en(&env.estado_a, &sub2_id).unwrap();
            env.registrar_ramas_de(&t2);

            // Check-in real en sub2 para comprobar bloqueo de regreso a PorHacer.
            env.commit_real("commit-99.txt", "codigo 99", "Commit real 99");
            revisar_actualizaciones_en(&env.estado_a, Some(&sub2_id)).unwrap();
            env.volver_a_dev();

            // Pasar ambas subtareas a EnProduccion recalcula automáticamente al padre a EnProduccion.
            cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::EnProduccion).unwrap();
            cambiar_estado_en(&env.estado_a, &sub2_id, EstadoTarea::EnProduccion).unwrap();
            assert_eq!(
                env.estado_a
                    .con_db(|db| db_tareas::obtener(db.conexion(), &padre_id))
                    .unwrap()
                    .estado,
                EstadoTarea::EnProduccion
            );
            assert!(cambiar_estado_en(&env.estado_a, &padre_id, EstadoTarea::EnRevision).is_err());

            // sub1 -> EnRevision genera notificación TareaARevision.
            cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::EnRevision).unwrap();

            // Préstamo vigente sobre sub2 avisa al mandarla a EnRevision.
            env.estado_a
                .con_db(|db| {
                    commits::insertar_prestamo(
                        db.conexion(),
                        &commits::CambioPrestado {
                            id: "P-99".into(),
                            tarea_id: sub2_id.clone(),
                            origen_id: origen.id.clone(),
                            hash: None,
                            autor: admin_email.clone(),
                            fecha: crate::modelo::hoy_local(),
                            archivos: vec!["Assets/P99.cs".into()],
                            archivos_tocados: vec![],
                            quitado_en: None,
                            quitado_por: None,
                            changeset_merge: None,
                            changeset_limpio: None,
                        },
                    )
                })
                .unwrap();
            let rev2 = cambiar_estado_en(&env.estado_a, &sub2_id, EstadoTarea::EnRevision).unwrap();
            assert!(!rev2.aviso_prestamos.is_empty());

            // Desarrollador y Tester no pueden mandar a Terminada.
            env.estado_a.fijar_usuario(Some(DEV));
            assert!(cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::Terminada).is_err());
            env.estado_a.fijar_usuario(Some(TESTER));
            assert!(cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::Terminada).is_err());

            // Admin termina sub1; sub2 con préstamo vigente bloquea Terminada hasta quitar el préstamo.
            env.estado_a.fijar_usuario(Some(&admin_email));
            cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::Terminada).unwrap();
            assert!(cambiar_estado_en(&env.estado_a, &sub2_id, EstadoTarea::Terminada).is_err());
            env.estado_a
                .con_db(|db| {
                    commits::quitar_prestamo(
                        db.conexion(),
                        "P-99",
                        &admin_email,
                        &crate::modelo::hoy_local(),
                    )
                })
                .unwrap();
            cambiar_estado_en(&env.estado_a, &sub2_id, EstadoTarea::Terminada).unwrap();
            assert_eq!(
                env.estado_a
                    .con_db(|db| db_tareas::obtener(db.conexion(), &padre_id))
                    .unwrap()
                    .estado,
                EstadoTarea::EnRevision
            );
            cambiar_estado_en(&env.estado_a, &padre_id, EstadoTarea::Terminada).unwrap();
            let padre_term = env
                .estado_a
                .con_db(|db| db_tareas::obtener(db.conexion(), &padre_id))
                .unwrap();
            assert_eq!(padre_term.estado, EstadoTarea::Terminada);
            assert!(padre_term.fecha_completada.is_some());

            // Hacia atrás: sub2 con commit impide devolver el subárbol a PorHacer, pero sí a EnProduccion.
            assert!(cambiar_estado_en(&env.estado_a, &padre_id, EstadoTarea::PorHacer).is_err());
            cambiar_estado_en(&env.estado_a, &padre_id, EstadoTarea::EnProduccion).unwrap();
            assert_eq!(
                env.estado_a
                    .con_db(|db| db_tareas::obtener(db.conexion(), &sub2_id))
                    .unwrap()
                    .estado,
                EstadoTarea::EnProduccion
            );
            // sub1 (sin commits) vuelve a PorHacer; sub2 (con commit) lo rechaza.
            cambiar_estado_en(&env.estado_a, &sub1_id, EstadoTarea::PorHacer).unwrap();
            assert!(cambiar_estado_en(&env.estado_a, &sub2_id, EstadoTarea::PorHacer).is_err());

            // Máquina B sincroniza vía OneDrive el estado final del subárbol y las notificaciones.
            env.sincronizar_b();
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &padre_id)
                    .unwrap()
                    .estado,
                EstadoTarea::EnProduccion
            );
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &sub1_id)
                    .unwrap()
                    .estado,
                EstadoTarea::PorHacer
            );
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &sub2_id)
                    .unwrap()
                    .estado,
                EstadoTarea::EnProduccion
            );
            assert!(
                notificaciones::de_usuario(env.db_b.conexion(), &admin_email)
                    .unwrap()
                    .iter()
                    .any(|n| n.tipo == TipoNotificacion::TareaARevision)
            );
        });
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
                changeset_merge: None,
                changeset_limpio: None,
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

    #[test]
    fn borrar_progreso_y_reabrir_cumplen_reglas_y_notifican() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_admin = AutorTarea {
            email: ADMIN,
            rol: Rol::Administrador,
        };
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
            &datos_base("Tarea para borrar progreso y reabrir"),
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
                        id: 1,
                        name: "/main/dev/T-101-tarea-para-borrar".into(),
                        head_changeset: 5,
                    },
                )])
            },
            "2026-11-15T10:01:00Z",
        )
        .unwrap();
        cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            EstadoTarea::EnProduccion,
            "2026-11-15T10:02:00Z",
        )
        .unwrap();
        commits::guardar(
            c,
            &commits::Commit {
                id: "c1".into(),
                tarea_id: "T-101".into(),
                hash: "10".into(),
                mensaje: "Avance".into(),
                descripcion: None,
                tipo: Some(TipoTarea::Scripting),
                autor: DEV.into(),
                fecha: "2026-11-15T10:03:00Z".into(),
            },
        )
        .unwrap();
        commits::insertar_prestamo(
            c,
            &commits::CambioPrestado {
                id: "p1".into(),
                tarea_id: "T-101".into(),
                origen_id: "T-100".into(),
                hash: None,
                autor: DEV.into(),
                fecha: HOY.into(),
                archivos: vec![],
                archivos_tocados: vec![],
                quitado_en: None,
                quitado_por: None,
                changeset_merge: None,
                changeset_limpio: None,
            },
        )
        .unwrap();

        let lista =
            borrar_progreso(c, &e.ops, &autor_dev, "T-101", "2026-11-15T10:04:00Z").unwrap();
        let t = lista.iter().find(|x| x.id == "T-101").unwrap();
        assert_eq!(t.estado, EstadoTarea::PorHacer);
        assert_eq!(t.asignado_a.as_deref(), Some(DEV));
        assert!(t.rama_asociada.is_some());
        assert!(commits::de_tarea(c, "T-101").unwrap().is_empty());
        assert!(commits::prestamos_de_tarea(c, "T-101").unwrap().is_empty());

        // Motivo vacío o tarea no terminada rechaza reabrir.
        let err = reabrir(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            "   ",
            "2026-11-15T10:05:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("motivo"));
        let err = reabrir(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            "Falta prueba",
            "2026-11-15T10:05:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("terminadas"));

        // Pasar a terminada y reabrir 3 veces con Líder; la 4ª solo Administrador.
        for n in 1..=3u32 {
            let m_fin = n * 2;
            let m_re = n * 2 + 1;
            cambiar_estado(
                c,
                &e.ops,
                &autor_lider,
                "T-101",
                EstadoTarea::Terminada,
                &format!("2026-11-15T10:1{m_fin}:00Z"),
            )
            .unwrap();
            let r = reabrir(
                c,
                &e.ops,
                &autor_lider,
                "T-101",
                &format!("Motivo {n}"),
                &format!("2026-11-15T10:1{m_re}:00Z"),
            )
            .unwrap();
            assert_eq!(r.estado, EstadoTarea::EnRevision);
            assert_eq!(r.reaperturas, n);
        }

        cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            EstadoTarea::Terminada,
            "2026-11-15T10:30:00Z",
        )
        .unwrap();
        let err = reabrir(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            "Motivo 4",
            "2026-11-15T10:31:00Z",
        )
        .unwrap_err();
        assert!(err.to_string().contains("Administrador"));

        let r4 = reabrir(
            c,
            &e.ops,
            &autor_admin,
            "T-101",
            "Motivo 4 admin",
            "2026-11-15T10:32:00Z",
        )
        .unwrap();
        assert_eq!(r4.reaperturas, 4);
        assert_eq!(r4.estado, EstadoTarea::EnRevision);
        assert!(notificaciones::de_usuario(c, LIDER)
            .unwrap()
            .iter()
            .any(|n| n.tipo == TipoNotificacion::TareaReabierta));

        let sync = e.sincronizar_b();
        assert_eq!(sync.descartados, 0);
        let t_b = db_tareas::obtener(e.db_b.conexion(), "T-101").unwrap();
        assert_eq!(t_b.reaperturas, 4);
        assert_eq!(t_b.estado, EstadoTarea::EnRevision);
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_borrar_progreso() {
        use crate::cm::Comando;
        use crate::comandos::tareas::{
            borrar_progreso_en, cambiar_estado_en, reabrir_en, tomar_en,
        };

        correr_entorno_real("100", |env| {
            env.crear_usuario(LIDER, Rol::Lider);
            let admin_email = env.admin_fila.email.clone();

            let t1 = env.crear_tarea(&admin_email, Rol::Administrador, "Borrar progreso 100");
            let t2 = env.crear_tarea(&admin_email, Rol::Administrador, "Reabrir 100");
            let t3 = env.crear_tarea(&admin_email, Rol::Administrador, "Subtractive origen 100");
            let t4 = env.crear_tarea(&admin_email, Rol::Administrador, "Subtractive destino 100");

            // 1) T-101: check-in real + cambio sin check-in + préstamo en SQLite -> borrar_progreso revierte todo.
            let tomadas1 = tomar_en(&env.estado_a, &t1.id).unwrap();
            env.registrar_ramas_de(&tomadas1);
            let rama1 = tomadas1[0].rama_asociada.clone().unwrap();
            env.commit_real("avance-100.txt", "hola", "Avance real 100");
            std::fs::write(env.ws_ruta.join("pendiente-100.txt"), "pendiente").unwrap();
            Comando::cm()
                .arg("add")
                .arg(
                    env.ws_ruta
                        .join("pendiente-100.txt")
                        .to_string_lossy()
                        .as_ref(),
                )
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            env.estado_a
                .con_db(|db| {
                    commits::insertar_prestamo(
                        db.conexion(),
                        &commits::CambioPrestado {
                            id: "P-100".into(),
                            tarea_id: t1.id.clone(),
                            origen_id: t2.id.clone(),
                            hash: None,
                            autor: admin_email.clone(),
                            fecha: crate::modelo::hoy_local(),
                            archivos: vec!["Assets/P100.cs".into()],
                            archivos_tocados: vec![],
                            quitado_en: None,
                            quitado_por: None,
                            changeset_merge: None,
                            changeset_limpio: None,
                        },
                    )
                })
                .unwrap();

            let lista = borrar_progreso_en(&env.estado_a, &t1.id).unwrap();
            let res_t1 = lista.iter().find(|x| x.id == t1.id).unwrap();
            assert_eq!(res_t1.estado, EstadoTarea::PorHacer);
            assert!(res_t1.rama_asociada.is_some());
            assert!(
                crate::ramas::cambios_pendientes(&env.estado_a, &env.ws_ruta)
                    .unwrap()
                    .is_empty()
            );
            let cs_restantes = Comando::cm()
                .args(["find", "changeset"])
                .arg(format!(
                    "where branch='{rama1}' on repository '{}'",
                    env.repo_cm
                ))
                .args(["--format={changesetid}", "--nototal"])
                .exigir_exito(&env.estado_a)
                .unwrap();
            assert!(
                cs_restantes.trim().is_empty(),
                "no deben quedar changesets tras revertir: {cs_restantes}"
            );

            // 2) T-103 + T-104: cuando hay préstamo/cherrypick hacia otra rama, usa `cm merge --subtractive`.
            let tomadas3 = tomar_en(&env.estado_a, &t3.id).unwrap();
            env.registrar_ramas_de(&tomadas3);
            let rama3 = tomadas3[0].rama_asociada.clone().unwrap();
            env.commit_real(
                "subtractive-100.txt",
                "codigo prestado",
                "Commit prestable 100",
            );
            let cs3 = Comando::cm()
                .args(["find", "changeset"])
                .arg(format!(
                    "where branch='{rama3}' on repository '{}'",
                    env.repo_cm
                ))
                .args(["--format={changesetid}", "--nototal"])
                .exigir_exito(&env.estado_a)
                .unwrap()
                .lines()
                .last()
                .unwrap()
                .trim()
                .to_string();
            let tomadas4 = tomar_en(&env.estado_a, &t4.id).unwrap();
            env.registrar_ramas_de(&tomadas4);
            Comando::cm()
                .args(["merge", &format!("cs:{cs3}"), "--cherrypicking", "--merge"])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            Comando::cm()
                .args(["checkin", "--all", "-c=Cherrypick prestamo 100"])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            borrar_progreso_en(&env.estado_a, &t3.id).unwrap();
            assert!(!env.ws_ruta.join("subtractive-100.txt").exists());
            env.volver_a_dev();

            // 3) Reabrir tarea terminada hasta 3 veces como Líder; 4ta rechaza para Líder y acepta para Admin.
            let tomadas2 = tomar_en(&env.estado_a, &t2.id).unwrap();
            env.registrar_ramas_de(&tomadas2);
            env.volver_a_dev();
            cambiar_estado_en(&env.estado_a, &t2.id, EstadoTarea::EnProduccion).unwrap();
            for intento in 1..=3 {
                env.estado_a.fijar_usuario(Some(&admin_email));
                if intento == 1 {
                    cambiar_estado_en(&env.estado_a, &t2.id, EstadoTarea::EnRevision).unwrap();
                }
                cambiar_estado_en(&env.estado_a, &t2.id, EstadoTarea::Terminada).unwrap();
                env.estado_a.fijar_usuario(Some(LIDER));
                let reabierta =
                    reabrir_en(&env.estado_a, &t2.id, &format!("Reapertura {intento}")).unwrap();
                assert_eq!(reabierta.estado, EstadoTarea::EnRevision);
                assert_eq!(reabierta.reaperturas, intento);
            }
            env.estado_a.fijar_usuario(Some(&admin_email));
            cambiar_estado_en(&env.estado_a, &t2.id, EstadoTarea::Terminada).unwrap();
            env.estado_a.fijar_usuario(Some(LIDER));
            assert!(reabrir_en(&env.estado_a, &t2.id, "Cuarta por lider").is_err());

            env.estado_a.fijar_usuario(Some(&admin_email));
            let cuarta = reabrir_en(&env.estado_a, &t2.id, "Cuarta por admin").unwrap();
            assert_eq!(cuarta.reaperturas, 4);
            assert_eq!(cuarta.estado, EstadoTarea::EnRevision);

            // 4) Máquina B sincroniza vía OneDrive el borrado de progreso, las reaperturas y notificaciones.
            env.sincronizar_b();
            let t1_b = db_tareas::obtener(env.db_b.conexion(), &t1.id).unwrap();
            assert_eq!(t1_b.estado, EstadoTarea::PorHacer);
            assert!(commits::de_tarea(env.db_b.conexion(), &t1.id)
                .unwrap()
                .is_empty());
            assert!(commits::prestamos_de_tarea(env.db_b.conexion(), &t1.id)
                .unwrap()
                .is_empty());
            let t2_b = db_tareas::obtener(env.db_b.conexion(), &t2.id).unwrap();
            assert_eq!(t2_b.estado, EstadoTarea::EnRevision);
            assert_eq!(t2_b.reaperturas, 4);
            assert!(notificaciones::de_usuario(env.db_b.conexion(), LIDER)
                .unwrap()
                .iter()
                .any(|n| n.tipo == TipoNotificacion::TareaReabierta));
        });
    }

    #[test]
    fn aprobar_editar_y_advertir_tareas_pendientes() {
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

        for (i, titulo) in ["Pendiente 1", "Pendiente 2", "Pendiente 3"]
            .into_iter()
            .enumerate()
        {
            crear(
                c,
                &e.ops,
                &autor_dev,
                &datos_base(titulo),
                &format!("2026-11-15T10:0{i}:00Z"),
                HOY,
            )
            .unwrap();
        }

        // Desarrollador no puede aprobar, editar_y_aprobar ni advertir.
        assert!(matches!(
            aprobar_tarea(c, &e.ops, &autor_dev, "T-101", "2026-11-15T10:05:00Z").unwrap_err(),
            Error::SinPermiso
        ));
        assert!(matches!(
            advertir_pendiente(
                c,
                &e.ops,
                &autor_dev,
                "T-101",
                "Falta info",
                "2026-11-15T10:05:00Z"
            )
            .unwrap_err(),
            Error::SinPermiso
        ));

        // Advertir deja pendiente_aprobacion = true y crea notificación #12 al creador.
        advertir_pendiente(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            "Detalla mejor la meta",
            "2026-11-15T10:06:00Z",
        )
        .unwrap();
        assert!(db_tareas::obtener(c, "T-101").unwrap().pendiente_aprobacion);
        assert!(notificaciones::de_usuario(c, DEV)
            .unwrap()
            .iter()
            .any(|n| n.tipo == TipoNotificacion::Advertencia));

        // Editar y aprobar cambia campos y aprueba en un solo evento + notificación #11.
        let editada = editar_y_aprobar(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            &CambiosTarea {
                titulo: Some("Pendiente 1 corregida".into()),
                ..Default::default()
            },
            "2026-11-15T10:07:00Z",
        )
        .unwrap();
        assert_eq!(editada.titulo, "Pendiente 1 corregida");
        assert!(!editada.pendiente_aprobacion);
        assert!(notificaciones::de_usuario(c, DEV)
            .unwrap()
            .iter()
            .any(|n| n.tipo == TipoNotificacion::EditadaAprobada));

        // aprobar_tareas aprueba T-102 y T-103 en una operación sin tocar estado.
        let lista = aprobar_tareas(
            c,
            &e.ops,
            &autor_lider,
            &["T-102".into(), "T-103".into()],
            "2026-11-15T10:08:00Z",
        )
        .unwrap();
        assert!(lista.iter().all(|t| !t.pendiente_aprobacion));

        // Máquina B sincroniza y llega al mismo estado y notificaciones.
        let sync = e.sincronizar_b();
        assert_eq!(sync.descartados, 0);
        assert!(
            !db_tareas::obtener(e.db_b.conexion(), "T-101")
                .unwrap()
                .pendiente_aprobacion
        );
        assert!(
            !db_tareas::obtener(e.db_b.conexion(), "T-103")
                .unwrap()
                .pendiente_aprobacion
        );
        assert_eq!(
            notificaciones::de_usuario(e.db_b.conexion(), DEV)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_aprobar_editar_y_advertir() {
        use crate::comandos::tareas::{
            advertir_pendiente_en, aprobar_tarea_en, aprobar_tareas_en, editar_y_aprobar_en,
        };

        correr_entorno_real("101", |env| {
            env.crear_usuario(DEV, Rol::Desarrollador);
            env.crear_usuario(LIDER, Rol::Lider);

            // DEV crea 4 tareas pendientes de aprobación.
            let t1 = env.crear_tarea(DEV, Rol::Desarrollador, "Pendiente 101 A");
            let t2 = env.crear_tarea(DEV, Rol::Desarrollador, "Pendiente 101 B");
            let t3 = env.crear_tarea(DEV, Rol::Desarrollador, "Pendiente 101 C");
            let t4 = env.crear_tarea(DEV, Rol::Desarrollador, "Pendiente 101 D");
            assert!(t1.pendiente_aprobacion && t2.pendiente_aprobacion);

            // DEV no puede aprobar, editar ni advertir.
            env.estado_a.fijar_usuario(Some(DEV));
            assert!(aprobar_tarea_en(&env.estado_a, &t2.id).is_err());
            assert!(aprobar_tareas_en(&env.estado_a, &[t3.id.clone(), t4.id.clone()]).is_err());
            assert!(advertir_pendiente_en(&env.estado_a, &t1.id, "No autorizado").is_err());

            // Líder advierte T1 (sigue pendiente), luego edita+aprueba T1, aprueba T2 y aprueba T3+T4 en lote.
            env.estado_a.fijar_usuario(Some(LIDER));
            advertir_pendiente_en(&env.estado_a, &t1.id, "Detalla mejor la meta").unwrap();
            assert!(
                env.estado_a
                    .con_db(|db| db_tareas::obtener(db.conexion(), &t1.id))
                    .unwrap()
                    .pendiente_aprobacion
            );

            let editada = editar_y_aprobar_en(
                &env.estado_a,
                &t1.id,
                &CambiosTarea {
                    titulo: Some("Pendiente 101 A corregida".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(editada.titulo, "Pendiente 101 A corregida");
            assert!(!editada.pendiente_aprobacion);

            let aprobada2 = aprobar_tarea_en(&env.estado_a, &t2.id).unwrap();
            assert!(!aprobada2.pendiente_aprobacion);

            let lote = aprobar_tareas_en(&env.estado_a, &[t3.id.clone(), t4.id.clone()]).unwrap();
            assert_eq!(lote.len(), 4);
            assert!(lote.iter().all(|t| !t.pendiente_aprobacion));

            // Máquina B sincroniza vía OneDrive: todas aprobadas y el creador DEV recibe las dos notificaciones.
            env.sincronizar_b();
            let t1_b = db_tareas::obtener(env.db_b.conexion(), &t1.id).unwrap();
            assert_eq!(t1_b.titulo, "Pendiente 101 A corregida");
            for tid in [&t1.id, &t2.id, &t3.id, &t4.id] {
                assert!(
                    !db_tareas::obtener(env.db_b.conexion(), tid)
                        .unwrap()
                        .pendiente_aprobacion
                );
            }
            let notis_dev = notificaciones::de_usuario(env.db_b.conexion(), DEV).unwrap();
            assert!(notis_dev
                .iter()
                .any(|n| n.tipo == TipoNotificacion::Advertencia));
            assert!(notis_dev
                .iter()
                .any(|n| n.tipo == TipoNotificacion::EditadaAprobada));
        });
    }

    #[test]
    fn eliminar_y_papelera_con_cascada_notificaciones_y_purga() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };

        // Padre T-101 creado por DEV, con dos subtareas:
        // T-101-1 (activa) y T-101-2 (terminada). Además T-102 independiente.
        crear(c, &e.ops, &autor_dev, &datos_base("Padre"), AHORA, HOY).unwrap();

        let mut sub_activa = datos_base("Sub activa");
        sub_activa.tarea_padre_id = Some("T-101".into());
        crear(
            c,
            &e.ops,
            &autor_dev,
            &sub_activa,
            "2026-11-15T15:00:01Z",
            HOY,
        )
        .unwrap();

        let mut sub_term = datos_base("Sub terminada");
        sub_term.tarea_padre_id = Some("T-101".into());
        crear(
            c,
            &e.ops,
            &autor_lider,
            &sub_term,
            "2026-11-15T15:00:02Z",
            HOY,
        )
        .unwrap();
        cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-101-2",
            EstadoTarea::EnProduccion,
            "2026-11-15T15:00:03Z",
        )
        .unwrap();
        cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-101-2",
            EstadoTarea::Terminada,
            "2026-11-15T15:00:04Z",
        )
        .unwrap();

        crear(
            c,
            &e.ops,
            &autor_dev,
            &datos_base("Otra tarea"),
            "2026-11-15T15:00:05Z",
            HOY,
        )
        .unwrap();

        // Desarrollador no puede eliminar; justificación vacía rechazada.
        assert!(matches!(
            eliminar(
                c,
                &e.ops,
                &autor_dev,
                "T-101",
                "Motivo",
                "2026-11-15T15:00:06Z"
            )
            .unwrap_err(),
            Error::SinPermiso
        ));
        assert!(matches!(
            eliminar(
                c,
                &e.ops,
                &autor_lider,
                "T-101",
                "   ",
                "2026-11-15T15:00:06Z"
            )
            .unwrap_err(),
            Error::Validacion(_)
        ));

        // No se puede purgar definitivamente una tarea que no está en Papelera.
        assert!(matches!(
            eliminar_definitivamente(c, &e.ops, &autor_lider, "T-101", "2026-11-15T15:00:06Z")
                .unwrap_err(),
            Error::Validacion(_)
        ));

        // Eliminar T-102 primero (más antigua), luego T-101 (más reciente).
        eliminar(
            c,
            &e.ops,
            &autor_lider,
            "T-102",
            "Duplicada",
            "2026-11-15T15:00:07Z",
        )
        .unwrap();
        let afectadas = eliminar(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            "Ya no aplica",
            "2026-11-15T15:00:08Z",
        )
        .unwrap();
        let ids_afectadas: Vec<&str> = afectadas.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids_afectadas, vec!["T-101", "T-101-1"]);

        // T-101 y T-101-1 en Papelera sin pendiente_aprobacion; T-101-2 sigue activa/terminada.
        let p = db_tareas::obtener(c, "T-101").unwrap();
        assert!(p.eliminada);
        assert!(!p.pendiente_aprobacion);
        assert_eq!(p.justificacion_eliminacion.as_deref(), Some("Ya no aplica"));
        assert!(db_tareas::obtener(c, "T-101-1").unwrap().eliminada);
        assert!(!db_tareas::obtener(c, "T-101-2").unwrap().eliminada);

        // Papelera ordenada de la más reciente a la más antigua (a igual fecha por id desc).
        let papelera_ids: Vec<String> = listar_papelera(c)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(papelera_ids, vec!["T-101-1", "T-101", "T-102"]);

        // Notificación #13 al creador (DEV) por T-102, T-101 y T-101-1.
        let notis_dev: Vec<_> = notificaciones::de_usuario(c, DEV)
            .unwrap()
            .into_iter()
            .filter(|n| n.tipo == TipoNotificacion::TareaEliminada)
            .collect();
        assert_eq!(notis_dev.len(), 3);

        // Purga definitiva sobre T-101 borra T-101 y T-101-1 en cascada, dejando T-101-2.
        let purgadas =
            eliminar_definitivamente(c, &e.ops, &autor_lider, "T-101", "2026-11-15T15:00:09Z")
                .unwrap();
        assert_eq!(purgadas.len(), 2);
        assert!(db_tareas::buscar(c, "T-101").unwrap().is_none());
        assert!(db_tareas::buscar(c, "T-101-1").unwrap().is_none());
        assert!(db_tareas::buscar(c, "T-101-2").unwrap().is_some());

        // Máquina B sincroniza y llega al mismo estado.
        let sync = e.sincronizar_b();
        let log = std::fs::read_to_string(e.local_b.join("eventos.log")).unwrap_or_default();
        assert_eq!(
            sync.descartados, 0,
            "descartados: {:?}\nlog:\n{}",
            sync, log
        );
        assert!(db_tareas::buscar(e.db_b.conexion(), "T-101")
            .unwrap()
            .is_none());
        assert!(db_tareas::buscar(e.db_b.conexion(), "T-101-1")
            .unwrap()
            .is_none());
        assert!(
            db_tareas::obtener(e.db_b.conexion(), "T-102")
                .unwrap()
                .eliminada
        );
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_eliminar_y_purgar() {
        use crate::cm::Comando;
        use crate::comandos::tareas::{
            aprobar_tareas_en, cambiar_estado_en, eliminar_definitivamente_en, eliminar_tarea_en,
            listar_papelera_en, tomar_en,
        };

        correr_entorno_real("103", |env| {
            env.crear_usuario(DEV, Rol::Desarrollador);
            let admin_email = env.admin_fila.email.clone();

            // DEV crea padre T1 con sub1 (activa) y sub2 (terminada), más t2 independiente.
            let grupo = env.crear_con_subtareas(
                DEV,
                Rol::Desarrollador,
                "Padre 103",
                &["Sub activa 103", "Sub terminada 103"],
            );
            let padre_id = grupo[0].id.clone();
            let sub_act_id = grupo[1].id.clone();
            let sub_term_id = grupo[2].id.clone();
            let t2 = env.crear_tarea(DEV, Rol::Desarrollador, "Independiente 103");

            aprobar_tareas_en(
                &env.estado_a,
                &[
                    padre_id.clone(),
                    sub_act_id.clone(),
                    sub_term_id.clone(),
                    t2.id.clone(),
                ],
            )
            .unwrap();

            let tom1 = tomar_en(&env.estado_a, &sub_act_id).unwrap();
            env.registrar_ramas_de(&tom1);
            let rama_sub_act = tom1.last().unwrap().rama_asociada.clone().unwrap();

            let tom_term = tomar_en(&env.estado_a, &sub_term_id).unwrap();
            env.registrar_ramas_de(&tom_term);
            let rama_sub_term = tom_term.last().unwrap().rama_asociada.clone().unwrap();

            let tom2 = tomar_en(&env.estado_a, &t2.id).unwrap();
            env.registrar_ramas_de(&tom2);
            let rama_t2 = tom2[0].rama_asociada.clone().unwrap();
            env.volver_a_dev();

            // Terminamos sub_term_id para comprobar que la cascada NO elimina subtareas terminadas.
            cambiar_estado_en(&env.estado_a, &sub_term_id, EstadoTarea::EnRevision).unwrap();
            cambiar_estado_en(&env.estado_a, &sub_term_id, EstadoTarea::Terminada).unwrap();

            // 1) Soft-delete primero de t2, 25 ms después del padre -> orden más reciente primero en Papelera.
            eliminar_tarea_en(&env.estado_a, &t2.id, "Descartada primero").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(25));
            let en_papelera =
                eliminar_tarea_en(&env.estado_a, &padre_id, "Padre descartado después").unwrap();
            assert_eq!(en_papelera.len(), 2);
            assert!(en_papelera.iter().all(|t| t.eliminada));
            assert!(
                !env.estado_a
                    .con_db(|db| db_tareas::obtener(db.conexion(), &sub_term_id))
                    .unwrap()
                    .eliminada,
                "la subtarea terminada no debe eliminarse por cascada"
            );

            let papelera = listar_papelera_en(&env.estado_a).unwrap();
            assert_eq!(papelera.len(), 3);
            let pos_padre = papelera.iter().position(|t| t.id == padre_id).unwrap();
            let pos_t2 = papelera.iter().position(|t| t.id == t2.id).unwrap();
            assert!(
                pos_padre < pos_t2,
                "Papelera debe venir ordenada de la más reciente a la más antigua"
            );

            // Máquina B sincroniza Papelera, no-cascada a terminada y notificación TareaEliminada a DEV.
            env.sincronizar_b();
            let pap_b = db_tareas::listar_papelera(env.db_b.conexion()).unwrap();
            assert_eq!(pap_b.len(), 3);
            assert!(
                !db_tareas::obtener(env.db_b.conexion(), &sub_term_id)
                    .unwrap()
                    .eliminada
            );
            let notis_elim = notificaciones::de_usuario(env.db_b.conexion(), DEV)
                .unwrap()
                .into_iter()
                .filter(|n| n.tipo == TipoNotificacion::TareaEliminada)
                .count();
            assert!(
                notis_elim >= 2,
                "DEV debe recibir notificación por sus tareas eliminadas, recibidas: {notis_elim}"
            );

            // Admin también participa si creó tareas; aseguramos estado limpio en A.
            assert_eq!(env.admin_fila.email, admin_email);

            // 2) Purga definitiva: borra filas de Papelera en SQLite y purga ramas en Unity VC.
            let purgadas_p = eliminar_definitivamente_en(&env.estado_a, &padre_id).unwrap();
            assert_eq!(purgadas_p.len(), 2);
            let purgadas_2 = eliminar_definitivamente_en(&env.estado_a, &t2.id).unwrap();
            assert_eq!(purgadas_2.len(), 1);

            for id_gone in [&padre_id, &sub_act_id, &t2.id] {
                assert!(env
                    .estado_a
                    .con_db(|db| db_tareas::buscar(db.conexion(), id_gone))
                    .unwrap()
                    .is_none());
            }
            for rama in [&rama_sub_act, &rama_t2] {
                let corto = rama.rsplit('/').next().unwrap();
                let salida = Comando::cm()
                    .args(["find", "branch"])
                    .arg(format!(
                        "where name='{corto}' on repository '{}'",
                        env.repo_cm
                    ))
                    .args(["--format={name}", "--nototal"])
                    .exigir_exito(&env.estado_a)
                    .unwrap_or_default();
                assert!(
                    !salida.lines().any(|l| l.trim() == rama.as_str()),
                    "la rama {rama} debió purgarse en Unity VC, salida: {salida}"
                );
            }
            // La subtarea terminada y su rama siguen vivas.
            assert!(env
                .estado_a
                .con_db(|db| db_tareas::buscar(db.conexion(), &sub_term_id))
                .unwrap()
                .is_some());
            let corto_term = rama_sub_term.rsplit('/').next().unwrap();
            let vive_term = Comando::cm()
                .args(["find", "branch"])
                .arg(format!(
                    "where name='{corto_term}' on repository '{}'",
                    env.repo_cm
                ))
                .args(["--format={name}", "--nototal"])
                .exigir_exito(&env.estado_a)
                .unwrap_or_default();
            assert!(
                vive_term.lines().any(|l| l.trim() == rama_sub_term.as_str()),
                "la rama terminada {rama_sub_term} debió permanecer en Unity VC, salida: {vive_term}"
            );

            // Máquina B sincroniza la purga definitiva vía OneDrive.
            env.sincronizar_b();
            assert!(db_tareas::buscar(env.db_b.conexion(), &padre_id)
                .unwrap()
                .is_none());
            assert!(db_tareas::buscar(env.db_b.conexion(), &sub_act_id)
                .unwrap()
                .is_none());
            assert!(db_tareas::buscar(env.db_b.conexion(), &t2.id)
                .unwrap()
                .is_none());
            assert!(db_tareas::buscar(env.db_b.conexion(), &sub_term_id)
                .unwrap()
                .is_some());
        });
    }

    #[test]
    fn deteccion_local_y_cierre_por_deteccion_viajan_por_ops() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };

        crear(
            c,
            &e.ops,
            &autor_dev,
            &datos_base("Tarea polling"),
            AHORA,
            HOY,
        )
        .unwrap();
        assert!(actualizar_deteccion_local(c, "T-101", Some(12), true).unwrap());
        // Llamar de nuevo con los mismos datos no vuelve a mutar.
        assert!(!actualizar_deteccion_local(c, "T-101", Some(12), true).unwrap());

        let t = db_tareas::obtener(c, "T-101").unwrap();
        assert_eq!(t.ultimo_changeset_visto, Some(12));
        assert!(t.modificada);

        let cerrada =
            cerrar_por_deteccion(c, &e.ops, &autor_dev, "T-101", "2026-11-15T15:00:01Z", HOY)
                .unwrap();
        assert_eq!(cerrada.estado, EstadoTarea::Terminada);
        assert_eq!(cerrada.fecha_completada.as_deref(), Some(HOY));
        assert!(!cerrada.modificada);

        // Máquina B sincroniza y recibe la terminación automática.
        let sync = e.sincronizar_b();
        assert_eq!(sync.descartados, 0);
        let en_b = db_tareas::obtener(e.db_b.conexion(), "T-101").unwrap();
        assert_eq!(en_b.estado, EstadoTarea::Terminada);
        assert_eq!(en_b.fecha_completada.as_deref(), Some(HOY));
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_polling_unity_vc() {
        use crate::cm::Comando;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::tomar_en;
        use crate::ramas;

        correr_entorno_real("104", |env| {
            let admin_email = env.admin_fila.email.clone();
            let grupo = env.crear_con_subtareas(
                &admin_email,
                Rol::Administrador,
                "Padre 104",
                &["Sub polling 104"],
            );
            let padre_id = grupo[0].id.clone();
            let sub_id = grupo[1].id.clone();

            let tomadas = tomar_en(&env.estado_a, &sub_id).unwrap();
            env.registrar_ramas_de(&tomadas);
            let rama_padre = tomadas[0].rama_asociada.clone().unwrap();
            let rama_sub = tomadas[1].rama_asociada.clone().unwrap();

            // 1) Check-in externo en la rama de la subtarea -> detecta rama actual y check-in nuevo (`modificada`).
            env.commit_real(
                "polling_104.txt",
                "prueba polling 104",
                "Commit externo polling 104",
            );

            let resumen = revisar_actualizaciones_en(&env.estado_a, Some(&sub_id)).unwrap();
            assert_eq!(resumen.rama_actual.as_deref(), Some(rama_sub.as_str()));
            assert_eq!(resumen.tareas_modificadas, vec![sub_id.clone()]);
            assert!(resumen.tareas_cerradas.is_empty());

            let (sub_despues, cs_lista) = env
                .estado_a
                .con_db(|db| {
                    Ok((
                        db_tareas::obtener(db.conexion(), &sub_id)?,
                        commits::de_tarea(db.conexion(), &sub_id)?,
                    ))
                })
                .unwrap();
            assert!(sub_despues.modificada);
            assert!(sub_despues.ultimo_changeset_visto.is_some());
            assert!(cs_lista
                .iter()
                .any(|c| c.mensaje.contains("Commit externo polling 104")));

            // 2) Merge real de la rama de subtarea hacia su rama padre (sin tocar /main/dev)
            //    -> polling detecta rama cerrada, pasa a terminada y emite `tarea_cerrada_por_deteccion`.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_padre).unwrap();
            Comando::cm()
                .args([
                    "merge",
                    &format!("br:{rama_sub}@{}", env.repo_cm),
                    "--merge",
                ])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            Comando::cm()
                .args(["checkin", "--all", "-c=Merge subtarea a padre 104"])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();

            let cierre = revisar_actualizaciones_en(&env.estado_a, Some(&sub_id)).unwrap();
            assert_eq!(cierre.rama_actual.as_deref(), Some(rama_padre.as_str()));
            assert_eq!(cierre.tareas_cerradas, vec![sub_id.clone()]);

            let sub_final = env
                .estado_a
                .con_db(|db| db_tareas::obtener(db.conexion(), &sub_id))
                .unwrap();
            assert_eq!(sub_final.estado, EstadoTarea::Terminada);
            assert!(sub_final.fecha_completada.is_some());
            assert!(!sub_final.modificada);
            let padre_final = env
                .estado_a
                .con_db(|db| db_tareas::obtener(db.conexion(), &padre_id))
                .unwrap();
            assert_eq!(padre_final.estado, EstadoTarea::EnRevision);

            // 3) Máquina B sincroniza vía OneDrive el evento `tarea_cerrada_por_deteccion`.
            env.sincronizar_b();
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &sub_id)
                    .unwrap()
                    .estado,
                EstadoTarea::Terminada
            );
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &padre_id)
                    .unwrap()
                    .estado,
                EstadoTarea::EnRevision
            );
        });
    }

    #[test]
    fn proponer_sumar_aprobar_y_rechazar_ediciones_pendientes() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_dev = AutorTarea {
            email: DEV,
            rol: Rol::Desarrollador,
        };
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };

        let mut d_vacio = datos_base("Original 102");
        d_vacio.detalles = String::new();
        crear(c, &e.ops, &autor_lider, &d_vacio, AHORA, HOY).unwrap();
        crear(c, &e.ops, &autor_lider, &datos_base("En curso"), AHORA, HOY).unwrap();
        crear(
            c,
            &e.ops,
            &autor_lider,
            &datos_base("Aprobar lote A"),
            AHORA,
            HOY,
        )
        .unwrap();
        crear(
            c,
            &e.ops,
            &autor_lider,
            &datos_base("Aprobar lote B"),
            AHORA,
            HOY,
        )
        .unwrap();

        cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-102",
            EstadoTarea::EnProduccion,
            "2026-11-15T15:01:00Z",
        )
        .unwrap();
        assert!(proponer_edicion(
            c,
            &e.ops,
            &autor_dev,
            "T-102",
            "No debe",
            "",
            None,
            "2026-11-15T15:02:00Z",
            HOY
        )
        .is_err());

        // Primera edición sobre T-101 (detalles vacíos originalmente): aplica de inmediato y guarda lo anterior.
        let r1 = proponer_edicion(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            "Editada v1",
            "Nuevos detalles",
            Some("Clarificar alcance"),
            "2026-11-15T15:03:00Z",
            HOY,
        )
        .unwrap();
        assert_eq!(r1.tarea.titulo, "Editada v1");
        assert_eq!(r1.tarea.detalles, "Nuevos detalles");
        assert!(r1.tarea.edicion_pendiente);
        assert_eq!(r1.edicion.campos_anteriores["titulo"], "Original 102");
        assert_eq!(r1.edicion.campos_anteriores["detalles"], "");

        // Segunda edición se suma conservando el valor anterior original ("Original 102" y "").
        let r2 = proponer_edicion(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            "Editada v2",
            "Nuevos detalles",
            None,
            "2026-11-15T15:04:00Z",
            HOY,
        )
        .unwrap();
        assert_eq!(r2.edicion.id, r1.edicion.id);
        assert_eq!(listar_ediciones_pendientes(c).unwrap().len(), 1);
        assert_eq!(r2.edicion.campos_anteriores["titulo"], "Original 102");
        assert_eq!(r2.edicion.campos_nuevos["titulo"], "Editada v2");

        // Desarrollador no puede aprobar ni rechazar.
        assert!(matches!(
            aprobar_modificacion(c, &e.ops, &autor_dev, "T-101", "2026-11-15T15:05:00Z")
                .unwrap_err(),
            Error::SinPermiso
        ));
        assert!(matches!(
            rechazar_modificacion(c, &e.ops, &autor_dev, "T-101", "2026-11-15T15:05:00Z")
                .unwrap_err(),
            Error::SinPermiso
        ));

        // Rechazar T-101 restaura título y detalles vacíos originales, y borra la fila pendiente.
        let revertida =
            rechazar_modificacion(c, &e.ops, &autor_lider, "T-101", "2026-11-15T15:06:00Z")
                .unwrap();
        assert_eq!(revertida.titulo, "Original 102");
        assert_eq!(revertida.detalles, "");
        assert!(!revertida.edicion_pendiente);

        // T-103 y T-104 se proponen y se aprueban en lote.
        proponer_edicion(
            c,
            &e.ops,
            &autor_dev,
            "T-103",
            "Lote A v1",
            "",
            None,
            "2026-11-15T15:07:00Z",
            HOY,
        )
        .unwrap();
        proponer_edicion(
            c,
            &e.ops,
            &autor_dev,
            "T-104",
            "Lote B v1",
            "Con detalle",
            None,
            "2026-11-15T15:08:00Z",
            HOY,
        )
        .unwrap();
        let tras_lote = aprobar_modificaciones(
            c,
            &e.ops,
            &autor_lider,
            &["T-103".into(), "T-104".into()],
            "2026-11-15T15:09:00Z",
        )
        .unwrap();
        assert!(tras_lote.iter().all(|t| !t.edicion_pendiente));
        assert!(listar_ediciones_pendientes(c).unwrap().is_empty());

        // Máquina B sincroniza vía ops/ y converge exactamente con A.
        let sync = e.sincronizar_b();
        assert_eq!(sync.descartados, 0);
        let t1_b = db_tareas::obtener(e.db_b.conexion(), "T-101").unwrap();
        assert_eq!(t1_b.titulo, "Original 102");
        assert_eq!(t1_b.detalles, "");
        assert!(!t1_b.edicion_pendiente);
        assert_eq!(
            db_tareas::obtener(e.db_b.conexion(), "T-103")
                .unwrap()
                .titulo,
            "Lote A v1"
        );
        assert!(listar_ediciones_pendientes(e.db_b.conexion())
            .unwrap()
            .is_empty());
        assert!(notificaciones::de_usuario(e.db_b.conexion(), LIDER)
            .unwrap()
            .iter()
            .any(|n| n.tipo == TipoNotificacion::EdicionPendiente && n.accionable));
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_ediciones_pendientes() {
        use crate::comandos::tareas::{
            aprobar_modificacion_en, aprobar_modificaciones_en, asignar_en, cambiar_estado_en,
            listar_ediciones_pendientes_en, proponer_edicion_en, rechazar_modificacion_en,
        };

        correr_entorno_real("102", |env| {
            env.crear_usuario(DEV, Rol::Desarrollador);
            env.crear_usuario(LIDER, Rol::Lider);
            let admin_email = env.admin_fila.email.clone();

            let t1 = env.crear_tarea(&admin_email, Rol::Administrador, "Original 102");
            let t2 = env.crear_tarea(&admin_email, Rol::Administrador, "En produccion 102");
            let t3 = env.crear_tarea(&admin_email, Rol::Administrador, "Aprobar indiv 102");
            let t4 = env.crear_tarea(&admin_email, Rol::Administrador, "Aprobar lote 102");

            // T2 tomada (en Unity VC) y en producción: no acepta `proponer_edicion`.
            let asg2 = asignar_en(&env.estado_a, &t2.id, DEV).unwrap();
            env.registrar_ramas_de(&asg2);
            cambiar_estado_en(&env.estado_a, &t2.id, EstadoTarea::EnProduccion).unwrap();
            env.estado_a.fijar_usuario(Some(DEV));
            assert!(proponer_edicion_en(&env.estado_a, &t2.id, "No valida", "", None).is_err());

            // 1) Primera edición por DEV sobre T1 (Sin asignar, detalles inicialmente vacíos "").
            let r1 = proponer_edicion_en(
                &env.estado_a,
                &t1.id,
                "Editada 102 v1",
                "Detalles 102",
                Some("Ajuste inicial"),
            )
            .unwrap();
            assert!(r1.tarea.edicion_pendiente);
            assert_eq!(r1.edicion.campos_anteriores["titulo"], "Original 102");
            assert_eq!(r1.edicion.campos_anteriores["detalles"], "");

            // Máquina B sincroniza edición pendiente inmediata y notificación accionable para Líder/Admin.
            env.sincronizar_b();
            let t1_b = db_tareas::obtener(env.db_b.conexion(), &t1.id).unwrap();
            assert_eq!(t1_b.titulo, "Editada 102 v1");
            assert!(t1_b.edicion_pendiente);
            assert!(notificaciones::de_usuario(env.db_b.conexion(), LIDER)
                .unwrap()
                .iter()
                .any(|n| n.tipo == TipoNotificacion::EdicionPendiente && n.accionable));

            // 2) Segunda edición se suma conservando valores anteriores originales.
            std::thread::sleep(std::time::Duration::from_millis(20));
            let r2 = proponer_edicion_en(
                &env.estado_a,
                &t1.id,
                "Editada 102 v2",
                "Detalles 102",
                Some("Segundo ajuste"),
            )
            .unwrap();
            assert_eq!(r2.edicion.id, r1.edicion.id);
            assert_eq!(
                listar_ediciones_pendientes_en(&env.estado_a).unwrap().len(),
                1
            );
            assert_eq!(r2.edicion.campos_anteriores["titulo"], "Original 102");
            assert_eq!(r2.edicion.campos_anteriores["detalles"], "");

            // DEV no puede aprobar ni rechazar modificaciones.
            assert!(aprobar_modificacion_en(&env.estado_a, &t1.id).is_err());
            assert!(rechazar_modificacion_en(&env.estado_a, &t1.id).is_err());

            // T3 (asignada en Por hacer) y T4 (Sin asignar) reciben ediciones pendientes.
            env.estado_a.fijar_usuario(Some(&admin_email));
            let asg3 = asignar_en(&env.estado_a, &t3.id, DEV).unwrap();
            env.registrar_ramas_de(&asg3);
            env.estado_a.fijar_usuario(Some(DEV));
            std::thread::sleep(std::time::Duration::from_millis(20));
            proponer_edicion_en(&env.estado_a, &t3.id, "T3 aprobada", "Det T3", None).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
            proponer_edicion_en(&env.estado_a, &t4.id, "T4 aprobada lote", "", None).unwrap();

            // 3) Líder rechaza T1 (restaura título y detalles vacíos), aprueba T3 individual y T4 en lote.
            env.estado_a.fijar_usuario(Some(LIDER));
            let rev1 = rechazar_modificacion_en(&env.estado_a, &t1.id).unwrap();
            assert_eq!(rev1.titulo, "Original 102");
            assert_eq!(rev1.detalles, "");
            assert!(!rev1.edicion_pendiente);

            let apr3 = aprobar_modificacion_en(&env.estado_a, &t3.id).unwrap();
            assert_eq!(apr3.titulo, "T3 aprobada");
            assert!(!apr3.edicion_pendiente);

            let lista_lote =
                aprobar_modificaciones_en(&env.estado_a, std::slice::from_ref(&t4.id)).unwrap();
            assert!(lista_lote
                .iter()
                .find(|t| t.id == t4.id)
                .map(|t| !t.edicion_pendiente && t.titulo == "T4 aprobada lote")
                .unwrap());
            assert!(listar_ediciones_pendientes_en(&env.estado_a)
                .unwrap()
                .is_empty());

            // 4) Máquina B sincroniza vía OneDrive el rechazo (restaurado) y aprobaciones.
            env.sincronizar_b();
            let t1_final_b = db_tareas::obtener(env.db_b.conexion(), &t1.id).unwrap();
            assert_eq!(t1_final_b.titulo, "Original 102");
            assert_eq!(t1_final_b.detalles, "");
            assert!(!t1_final_b.edicion_pendiente);
            let t3_final_b = db_tareas::obtener(env.db_b.conexion(), &t3.id).unwrap();
            assert_eq!(t3_final_b.titulo, "T3 aprobada");
            assert!(!t3_final_b.edicion_pendiente);
            assert!(ediciones::listar(env.db_b.conexion()).unwrap().is_empty());
        });
    }

    #[test]
    fn checkin_desde_branchboard_y_guardia_de_rama() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };

        crear_con_subtareas(
            c,
            &e.ops,
            &autor_lider,
            &DatosNuevaTareaConSubtareas {
                titulo: "Padre 105".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![DatosSubtareaNueva {
                    tipo: TipoTarea::Audio,
                    titulo: "Sub 105".into(),
                    detalles: String::new(),
                }],
            },
            AHORA,
            HOY,
        )
        .unwrap();

        tomar(
            c,
            &e.ops,
            &autor_lider,
            "T-101-1",
            |_| {
                Ok(vec![
                    (
                        "T-101".into(),
                        Rama {
                            id: 1,
                            name: "/main/dev/T-101-padre-105".into(),
                            head_changeset: 10,
                        },
                    ),
                    (
                        "T-101-1".into(),
                        Rama {
                            id: 2,
                            name: "/main/dev/T-101-padre-105/T-101-1-sub-105".into(),
                            head_changeset: 11,
                        },
                    ),
                ])
            },
            "2026-11-15T15:01:00Z",
        )
        .unwrap();

        // No permite check-in si no es responsable, si es rama padre con subtareas ni tipo SinDefinir.
        assert!(matches!(
            tarea_para_checkin(c, DEV, "/main/dev/T-101-padre-105/T-101-1-sub-105").unwrap_err(),
            Error::SinPermiso
        ));
        assert!(tarea_para_checkin(c, LIDER, "/main/dev/T-101-padre-105").is_err());
        assert!(armar_comentario_checkin("Titulo", "", TipoTarea::SinDefinir).is_err());
        assert!(armar_comentario_checkin("   ", "Desc", TipoTarea::Audio).is_err());
        assert_eq!(
            armar_comentario_checkin("  Audio final ", " Con reverb ", TipoTarea::Audio).unwrap(),
            "Audio final\n\nCon reverb"
        );
        assert_eq!(
            rama_para_cambiar(c, "T-101-1").unwrap(),
            "/main/dev/T-101-padre-105/T-101-1-sub-105"
        );

        let commit = registrar_checkin(
            c,
            &e.ops,
            &autor_lider,
            &DatosCheckin {
                tarea_id: "T-101-1",
                changeset: 25,
                titulo: "Audio final",
                descripcion: "Con reverb",
                tipo: TipoTarea::Audio,
            },
            "2026-11-15T15:02:00Z",
            "2026-11-15T10:02:00",
        )
        .unwrap();
        assert_eq!(commit.id, "C-T-101-1-25");
        assert_eq!(commit.tipo, Some(TipoTarea::Audio));
        assert_eq!(
            db_tareas::obtener(c, "T-101-1")
                .unwrap()
                .ultimo_changeset_visto,
            Some(25)
        );

        // Máquina B sincroniza vía ops/ y conserva tipo y ultimo_changeset_visto.
        let sync = e.sincronizar_b();
        assert_eq!(sync.descartados, 0);
        let cs_b = commits::de_tarea(e.db_b.conexion(), "T-101-1").unwrap();
        assert_eq!(cs_b.len(), 1);
        assert_eq!(cs_b[0].tipo, Some(TipoTarea::Audio));
        assert_eq!(cs_b[0].descripcion.as_deref(), Some("Con reverb"));
        assert_eq!(
            db_tareas::obtener(e.db_b.conexion(), "T-101-1")
                .unwrap()
                .ultimo_changeset_visto,
            Some(25)
        );
    }

    #[test]
    fn traer_y_devolver_cambios_reglas_y_viajan_por_ops() {
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

        for titulo in ["Origen prestamo", "Destino prestamo", "Sin rama"] {
            crear(c, &e.ops, &autor_lider, &datos_base(titulo), AHORA, HOY).unwrap();
        }
        // T-101 = origen; T-102 = destino; T-103 = sin rama.
        let mut con_rama = vec![
            ("T-101", "/main/dev/T-101-origen"),
            ("T-102", "/main/dev/T-102-destino"),
        ];
        for (id, rama) in con_rama.drain(..) {
            tomar(
                c,
                &e.ops,
                &autor_dev,
                id,
                |_| {
                    Ok(vec![(
                        id.into(),
                        Rama {
                            id: 1,
                            name: rama.into(),
                            head_changeset: 5,
                        },
                    )])
                },
                AHORA,
            )
            .unwrap();
        }

        // Origen en producción sin commits y origen en 'por hacer' se rechazan.
        assert!(preparar_traer_cambios(c, &autor_dev, "T-102", "T-101", None).is_err());
        assert!(preparar_traer_cambios(c, &autor_dev, "T-102", "T-102", None).is_err());
        assert!(preparar_traer_cambios(c, &autor_dev, "T-103", "T-101", None).is_err());

        // Origen pasa a producción con commit 12; destino sigue en 'por hacer' (ya tomada) -> permitido.
        cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-101",
            EstadoTarea::EnProduccion,
            AHORA,
        )
        .unwrap();
        assert!(preparar_traer_cambios(c, &autor_dev, "T-102", "T-101", None).is_err());
        commits::guardar(
            c,
            &commits::Commit {
                id: "C-1".into(),
                tarea_id: "T-101".into(),
                hash: "12".into(),
                mensaje: "Base 101".into(),
                descripcion: None,
                tipo: Some(TipoTarea::Scripting),
                autor: DEV.into(),
                fecha: HOY.into(),
            },
        )
        .unwrap();
        assert!(preparar_traer_cambios(c, &autor_dev, "T-102", "T-101", Some("abc")).is_err());

        let (info, cs) =
            preparar_traer_cambios(c, &autor_dev, "T-102", "T-101", Some("cs:12")).unwrap();
        assert_eq!(info.rama_destino, "/main/dev/T-102-destino");
        assert_eq!(info.rama_origen, "/main/dev/T-101-origen");
        assert_eq!(cs, Some(12));

        // En EnRevision solo Líder/Admin pueden traer cambios.
        cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-102",
            EstadoTarea::EnRevision,
            AHORA,
        )
        .unwrap();
        assert!(matches!(
            preparar_traer_cambios(c, &autor_dev, "T-102", "T-101", None).unwrap_err(),
            Error::SinPermiso
        ));
        assert!(preparar_traer_cambios(c, &autor_lider, "T-102", "T-101", None).is_ok());

        // Registrar préstamo viaja por ops/ guardando changeset_merge y archivos.
        let p = registrar_traer_cambios(
            c,
            &e.ops,
            &autor_lider,
            &DatosTraerCambios {
                destino_id: "T-102",
                origen_id: "T-101",
                hash: Some("12"),
                changeset_merge: 30,
                archivos: vec!["Assets/Combate.cs".into()],
            },
            "2026-11-15T16:00:00Z",
        )
        .unwrap();
        assert_eq!(p.changeset_merge, Some(30));
        assert_eq!(p.archivos, vec!["Assets/Combate.cs".to_string()]);
        assert_eq!(
            db_tareas::obtener(c, "T-102")
                .unwrap()
                .ultimo_changeset_visto,
            Some(30)
        );
        // No permite repetir exactamente el mismo préstamo vigente.
        assert!(preparar_traer_cambios(c, &autor_lider, "T-102", "T-101", Some("12")).is_err());

        // Desarrollador no puede devolver en EnRevision; si hay archivos_tocados también bloquea.
        assert!(matches!(
            preparar_devolver_cambios(c, &autor_dev, &p.id).unwrap_err(),
            Error::SinPermiso
        ));
        commits::marcar_tocados(c, &p.id, &["Assets/Combate.cs".into()]).unwrap();
        assert!(preparar_devolver_cambios(c, &autor_lider, &p.id).is_err());
        commits::marcar_tocados(c, &p.id, &[]).unwrap();
        assert!(preparar_devolver_cambios(c, &autor_lider, &p.id).is_ok());

        e.sincronizar_b();
        let pb = listar_cambios_prestados(e.db_b.conexion(), Some("T-102")).unwrap();
        assert_eq!(pb.len(), 1);
        assert_eq!(pb[0].changeset_merge, Some(30));

        let devuelto =
            registrar_devolver_cambios(c, &e.ops, &autor_lider, &p.id, "2026-11-15T16:05:00Z")
                .unwrap();
        assert_eq!(devuelto.quitado_por.as_deref(), Some(LIDER));
        assert!(commits::prestamos_vigentes(c, "T-102").unwrap().is_empty());

        e.sincronizar_b();
        let hist_b = listar_cambios_prestados(e.db_b.conexion(), None).unwrap();
        assert_eq!(hist_b.len(), 1);
        assert!(hist_b[0].quitado_en.is_some());
    }

    #[test]
    fn detectar_y_eliminar_cambios_ajenos_bloquean_terminada_y_viajan_por_ops() {
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

        for titulo in ["Origen 109", "Destino 109"] {
            crear(c, &e.ops, &autor_lider, &datos_base(titulo), AHORA, HOY).unwrap();
        }
        for (id, rama) in [
            ("T-101", "/main/dev/T-101-origen"),
            ("T-102", "/main/dev/T-102-destino"),
        ] {
            tomar(
                c,
                &e.ops,
                &autor_dev,
                id,
                |_| {
                    Ok(vec![(
                        id.into(),
                        Rama {
                            id: 1,
                            name: rama.into(),
                            head_changeset: 5,
                        },
                    )])
                },
                "2026-11-15T16:01:00Z",
            )
            .unwrap();
        }

        let prestamo = registrar_traer_cambios(
            c,
            &e.ops,
            &autor_dev,
            &DatosTraerCambios {
                destino_id: "T-102",
                origen_id: "T-101",
                hash: None,
                changeset_merge: 30,
                archivos: vec!["Assets/Player.cs".into(), "Assets/HUD.png".into()],
            },
            "2026-11-15T16:02:00Z",
        )
        .unwrap();

        // Cruce con pendientes de Windows (`\` / mayúsculas) y check-in <= 30 ignorado.
        assert!(!actualizar_tocados_de_prestamos(
            c,
            "T-102",
            Some(&[]),
            &[(30, vec!["Assets/Player.cs".into()])]
        )
        .unwrap());
        assert!(actualizar_tocados_de_prestamos(
            c,
            "T-102",
            Some(&["assets\\player.cs".into(), "Assets/Propio.cs".into()]),
            &[(31, vec!["Assets/HUD.png".into()])]
        )
        .unwrap());
        let p_tocado = commits::obtener_prestamo(c, &prestamo.id).unwrap();
        assert_eq!(
            p_tocado.archivos_tocados,
            vec!["Assets/HUD.png".to_string(), "Assets/Player.cs".to_string()]
        );

        // Si polling corre desde otra rama (`None`), no borra los tocados detectados.
        assert!(!actualizar_tocados_de_prestamos(c, "T-102", None, &[]).unwrap());
        assert_eq!(
            commits::obtener_prestamo(c, &prestamo.id)
                .unwrap()
                .archivos_tocados
                .len(),
            2
        );

        // Bloquea pasar a Terminada (aunque tuviera que fallar antes incluso que el préstamo) y rechaza sin tocados tras limpiar.
        cambiar_estado(
            c,
            &e.ops,
            &autor_dev,
            "T-102",
            EstadoTarea::EnRevision,
            "2026-11-15T16:03:00Z",
        )
        .unwrap();
        let err_term = cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-102",
            EstadoTarea::Terminada,
            "2026-11-15T16:04:00Z",
        )
        .unwrap_err();
        assert!(
            err_term.to_string().contains("cambios ajenos"),
            "{err_term}"
        );

        // Desarrollador no puede eliminar en EnRevision; Líder sí y viaja por ops/.
        assert!(matches!(
            preparar_eliminar_cambios_ajenos(c, &autor_dev, &prestamo.id).unwrap_err(),
            Error::SinPermiso
        ));
        assert!(preparar_eliminar_cambios_ajenos(c, &autor_lider, &prestamo.id).is_ok());

        assert_eq!(e.sincronizar_b().descartados, 0);
        let limpio = registrar_eliminar_cambios_ajenos(
            c,
            &e.ops,
            &autor_lider,
            &prestamo.id,
            Some(32),
            "2026-11-15T16:10:00Z",
        )
        .unwrap();
        assert!(limpio.archivos_tocados.is_empty());
        assert_eq!(limpio.changeset_limpio, Some(32));
        // Poll posterior con cs 31 (anterior a 32) NO vuelve a marcar tocados.
        assert!(!actualizar_tocados_de_prestamos(
            c,
            "T-102",
            Some(&[]),
            &[(31, vec!["Assets/HUD.png".into()])]
        )
        .unwrap());

        assert_eq!(e.sincronizar_b().descartados, 0);
        let pb = commits::obtener_prestamo(e.db_b.conexion(), &prestamo.id).unwrap();
        assert!(pb.archivos_tocados.is_empty());
        assert_eq!(pb.changeset_limpio, Some(32));
    }

    #[test]
    fn alerta_rama_ajena_ignora_meta_y_no_deshace_cambios() {
        let mut ajena = Tarea::nueva("T-101", "Ajena", TipoTarea::Scripting, ADMIN, HOY);
        ajena.asignado_a = Some(LIDER.into());
        ajena.estado = EstadoTarea::EnProduccion;
        ajena.rama_asociada = Some("/main/dev/T-101-ajena".into());

        let mut propia = Tarea::nueva("T-102", "Mia", TipoTarea::Scripting, ADMIN, HOY);
        propia.asignado_a = Some(DEV.into());
        propia.estado = EstadoTarea::EnProduccion;
        propia.rama_asociada = Some("/main/dev/T-102-mia".into());

        let mut terminada = Tarea::nueva("T-103", "Cerrada", TipoTarea::Scripting, ADMIN, HOY);
        terminada.asignado_a = Some(DEV.into());
        terminada.estado = EstadoTarea::Terminada;
        terminada.rama_asociada = Some("/main/dev/T-103-cerrada".into());

        let todas = vec![ajena, propia, terminada];

        // Solo `.meta` o rama propia asignada NO disparan alerta.
        assert!(evaluar_alerta_rama_ajena(
            &todas,
            DEV,
            "/main/dev/T-101-ajena",
            &["Assets/Player.cs.meta".into()],
            &[]
        )
        .is_none());
        assert!(evaluar_alerta_rama_ajena(
            &todas,
            DEV,
            "/main/dev/T-102-mia",
            &["Assets/Player.cs".into()],
            &[]
        )
        .is_none());

        // En `dev` con cambio real sí alerta y sugiere la rama asignada.
        let en_dev = evaluar_alerta_rama_ajena(
            &todas,
            DEV,
            "/main/dev",
            &["Assets/Enemy.cs".into(), "Assets/Enemy.cs.meta".into()],
            &[],
        )
        .expect("debe alertar en /main/dev");
        assert_eq!(en_dev.tarea_ajena_id, None);
        assert_eq!(en_dev.rama_sugerida.as_deref(), Some("/main/dev/T-102-mia"));
        assert_eq!(en_dev.archivos, vec!["Assets/Enemy.cs".to_string()]);
        assert!(en_dev.mensaje.contains("/main/dev/T-102-mia"));

        // En rama ajena o terminada con check-in reciente propio sí alerta e identifica tarea/dueño.
        let en_ajena = evaluar_alerta_rama_ajena(
            &todas,
            DEV,
            "/main/dev/T-101-ajena",
            &[],
            &["Assets/UI.png".into()],
        )
        .expect("debe alertar en rama ajena");
        assert_eq!(en_ajena.tarea_ajena_id.as_deref(), Some("T-101"));
        assert_eq!(en_ajena.dueno_rama.as_deref(), Some(LIDER));
        assert!(en_ajena.mensaje.contains("T-101"));

        let en_cerrada = evaluar_alerta_rama_ajena(
            &todas,
            DEV,
            "/main/dev/T-103-cerrada",
            &["Assets/Fix.cs".into()],
            &[],
        )
        .expect("rama terminada ya no debe editarse");
        assert_eq!(en_cerrada.tarea_ajena_id.as_deref(), Some("T-103"));
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_hacer_checkin() {
        use crate::cm::Comando;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::{
            asignar_en, cambiar_a_rama_tarea_en, cambios_pendientes_en, hacer_checkin_en, tomar_en,
        };

        correr_entorno_real("105", |env| {
            env.crear_usuario(DEV, Rol::Desarrollador);
            let admin_email = env.admin_fila.email.clone();

            let grupo = env.crear_con_subtareas(
                &admin_email,
                Rol::Administrador,
                "Padre 105",
                &["Sub checkin 105"],
            );
            let sub_id = grupo[1].id.clone();
            let indep = env.crear_tarea(&admin_email, Rol::Administrador, "Otra rama 105");
            let asg_indep = asignar_en(&env.estado_a, &indep.id, &admin_email).unwrap();
            env.registrar_ramas_de(&asg_indep);

            let tomadas = tomar_en(&env.estado_a, &sub_id).unwrap();
            env.registrar_ramas_de(&tomadas);
            let rama_padre = tomadas[0].rama_asociada.clone().unwrap();
            let rama_sub = tomadas[1].rama_asociada.clone().unwrap();

            // Guardia: otro usuario o rama padre con subtareas rechazan `hacer_checkin_en`.
            env.estado_a.fijar_usuario(Some(DEV));
            assert!(hacer_checkin_en(
                &env.estado_a,
                &rama_sub,
                "No soy dueño",
                "",
                TipoTarea::Scripting
            )
            .is_err());
            env.estado_a.fijar_usuario(Some(&admin_email));
            assert!(hacer_checkin_en(
                &env.estado_a,
                &rama_padre,
                "Padre",
                "",
                TipoTarea::Scripting
            )
            .is_err());

            // Guardia de rama incorrecta: al volver a /main/dev, detecta rama incorrecta y bloquea check-in.
            env.volver_a_dev();
            let pend_fuera = cambios_pendientes_en(&env.estado_a, Some(&rama_sub)).unwrap();
            assert_eq!(pend_fuera.rama_actual, "/main/dev");
            assert!(!pend_fuera.en_rama_esperada);
            assert!(pend_fuera.archivos.is_empty());
            let err_rama =
                hacer_checkin_en(&env.estado_a, &rama_sub, "En dev", "", TipoTarea::Scripting)
                    .unwrap_err();
            assert!(err_rama.to_string().contains("cambia de rama"));

            // Ofrecer cambiar de rama: `cambiar_a_rama_tarea_en` devuelve el workspace a `rama_sub`.
            let cambiada = cambiar_a_rama_tarea_en(&env.estado_a, &sub_id).unwrap();
            assert_eq!(cambiada, rama_sub);

            // Sin cambios pendientes rechaza `hacer_checkin_en`; agregando un archivo, `cm status` lo lista.
            assert!(
                hacer_checkin_en(&env.estado_a, &rama_sub, "Vacio", "", TipoTarea::Scripting)
                    .is_err()
            );
            let archivo = env.ws_ruta.join("script-105.txt");
            std::fs::write(&archivo, "contenido 105").unwrap();
            Comando::cm()
                .arg("add")
                .arg(archivo.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();

            let pend = cambios_pendientes_en(&env.estado_a, Some(&rama_sub)).unwrap();
            assert!(pend.en_rama_esperada);
            assert_eq!(pend.rama_actual, rama_sub);
            assert!(pend.archivos.iter().any(|a| a.contains("script-105.txt")));
            assert!(cambiar_a_rama_tarea_en(&env.estado_a, &indep.id).is_err());

            // Check-in real desde BranchBoard con título, descripción y tipo `Audio`.
            let commit = hacer_checkin_en(
                &env.estado_a,
                &rama_sub,
                "Sistema de audio 105",
                "Efectos base de combate",
                TipoTarea::Audio,
            )
            .unwrap();
            let cs: i64 = commit.hash.parse().unwrap();
            assert!(cs > 0);
            assert_eq!(commit.mensaje, "Sistema de audio 105");
            assert_eq!(
                commit.descripcion.as_deref(),
                Some("Efectos base de combate")
            );
            assert_eq!(commit.tipo, Some(TipoTarea::Audio));

            let pend_despues = cambios_pendientes_en(&env.estado_a, Some(&rama_sub)).unwrap();
            assert!(pend_despues.archivos.is_empty());

            // Releer con `revisar_actualizaciones_en` conserva `tipo` y no marca `modificada`.
            let pol = revisar_actualizaciones_en(&env.estado_a, Some(&sub_id)).unwrap();
            assert!(pol.tareas_modificadas.is_empty());
            let cs_a = env
                .estado_a
                .con_db(|db| commits::de_tarea(db.conexion(), &sub_id))
                .unwrap();
            assert_eq!(cs_a.len(), 1);
            assert_eq!(cs_a[0].tipo, Some(TipoTarea::Audio));

            // Máquina B sincroniza vía OneDrive el Commit con su `tipo` y `ultimo_changeset_visto`.
            env.sincronizar_b();
            let cs_b = commits::de_tarea(env.db_b.conexion(), &sub_id).unwrap();
            assert_eq!(cs_b.len(), 1);
            assert_eq!(cs_b[0].hash, commit.hash);
            assert_eq!(cs_b[0].tipo, Some(TipoTarea::Audio));
            assert_eq!(
                cs_b[0].descripcion.as_deref(),
                Some("Efectos base de combate")
            );
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &sub_id)
                    .unwrap()
                    .ultimo_changeset_visto,
                Some(cs)
            );
        });
    }

    #[test]
    fn en_espera_bloquea_revision_y_se_convierte_en_subtarea() {
        let e = Escenario::nuevo();
        let c = e.db_a.conexion();
        let autor_lider = AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        };

        crear_con_subtareas(
            c,
            &e.ops,
            &autor_lider,
            &DatosNuevaTareaConSubtareas {
                titulo: "Padre 106".into(),
                detalles: String::new(),
                tipo: TipoTarea::Scripting,
                tarea_padre_id: None,
                sprint_id: None,
                backlog_id: None,
                con_fecha: false,
                fecha_limite: None,
                subtareas: vec![DatosSubtareaNueva {
                    tipo: TipoTarea::Scripting,
                    titulo: "Sub normal".into(),
                    detalles: String::new(),
                }],
            },
            AHORA,
            HOY,
        )
        .unwrap();
        tomar(
            c,
            &e.ops,
            &autor_lider,
            "T-101-1",
            |_| {
                Ok(vec![
                    (
                        "T-101".into(),
                        Rama {
                            id: 1,
                            name: "/main/dev/T-101-padre-106".into(),
                            head_changeset: 10,
                        },
                    ),
                    (
                        "T-101-1".into(),
                        Rama {
                            id: 2,
                            name: "/main/dev/T-101-padre-106/T-101-1-sub-normal".into(),
                            head_changeset: 11,
                        },
                    ),
                ])
            },
            "2026-11-15T15:01:00Z",
        )
        .unwrap();
        cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-101-1",
            EstadoTarea::EnRevision,
            "2026-11-15T15:02:00Z",
        )
        .unwrap();

        // Crear subtarea "En espera" en T-101: bloquea EnRevision en padre y en sí misma.
        let esp = crear_en_espera(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            2,
            "2026-11-15T15:03:00Z",
            HOY,
        )
        .unwrap();
        assert_eq!(esp.tipo, TipoTarea::SinDefinir);
        assert_eq!(esp.estado, EstadoTarea::EnProduccion);
        assert_eq!(
            db_tareas::obtener(c, "T-101").unwrap().estado,
            EstadoTarea::EnProduccion
        );
        assert!(cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            EstadoTarea::EnRevision,
            "2026-11-15T15:04:00Z"
        )
        .is_err());
        assert!(cambiar_estado(
            c,
            &e.ops,
            &autor_lider,
            &esp.id,
            EstadoTarea::EnRevision,
            "2026-11-15T15:04:00Z"
        )
        .is_err());

        // Convertir "En espera" en subtarea con rama propia.
        let conv = convertir_espera_en_subtarea(
            c,
            &e.ops,
            &autor_lider,
            &DatosRamaDesdeEspera {
                espera_id: &esp.id,
                titulo: "Fix colision",
                detalles: "Ajuste pendiente",
                tipo: TipoTarea::Scripting,
                asignado_a: LIDER,
                rama: "/main/dev/T-101-padre-106/T-101-2-fix-colision",
                ultimo_changeset_visto: 10,
            },
            "2026-11-15T15:05:00Z",
        )
        .unwrap();
        assert_eq!(conv.tipo, TipoTarea::Scripting);
        assert_eq!(conv.estado, EstadoTarea::EnProduccion);
        assert_eq!(
            conv.rama_asociada.as_deref(),
            Some("/main/dev/T-101-padre-106/T-101-2-fix-colision")
        );

        // Segunda En espera -> quitar_en_espera la elimina limpiamente.
        let esp2 = crear_en_espera(
            c,
            &e.ops,
            &autor_lider,
            "T-101",
            1,
            "2026-11-15T15:06:00Z",
            HOY,
        )
        .unwrap();
        quitar_en_espera(c, &e.ops, &autor_lider, &esp2.id, "2026-11-15T15:07:00Z").unwrap();
        assert!(db_tareas::buscar(c, &esp2.id).unwrap().is_none());

        // Máquina B sincroniza vía ops/ y llega al mismo estado.
        let sync = e.sincronizar_b();
        assert_eq!(sync.descartados, 0);
        let conv_b = db_tareas::obtener(e.db_b.conexion(), &conv.id).unwrap();
        assert_eq!(conv_b.tipo, TipoTarea::Scripting);
        assert_eq!(conv_b.titulo, "Fix colision");
        assert!(db_tareas::buscar(e.db_b.conexion(), &esp2.id)
            .unwrap()
            .is_none());
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_subtarea_en_espera() {
        use crate::cm::Comando;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::{
            cambiar_estado_en, cambios_pendientes_en, crear_rama_desde_espera_en, hacer_checkin_en,
            tomar_en,
        };
        use crate::ramas;

        correr_entorno_real("106", |env| {
            let admin_email = env.admin_fila.email.clone();
            let grupo = env.crear_con_subtareas(
                &admin_email,
                Rol::Administrador,
                "Padre 106",
                &["Sub normal 106"],
            );
            let padre_id = grupo[0].id.clone();
            let sub_id = grupo[1].id.clone();

            let tomadas = tomar_en(&env.estado_a, &sub_id).unwrap();
            env.registrar_ramas_de(&tomadas);
            let rama_padre = tomadas[0].rama_asociada.clone().unwrap();

            // Terminamos la subtarea normal para demostrar que solo "En espera" bloquea al padre.
            env.volver_a_dev();
            cambiar_estado_en(&env.estado_a, &sub_id, EstadoTarea::EnProduccion).unwrap();
            cambiar_estado_en(&env.estado_a, &sub_id, EstadoTarea::EnRevision).unwrap();
            cambiar_estado_en(&env.estado_a, &sub_id, EstadoTarea::Terminada).unwrap();

            // 1) Trabajo sin check-in directo en la rama padre -> polling crea "En espera".
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_padre).unwrap();
            let archivo = env.ws_ruta.join("directo-en-padre-106.txt");
            std::fs::write(&archivo, "cambio directo 106").unwrap();
            Comando::cm()
                .arg("add")
                .arg(archivo.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();

            let pol1 = revisar_actualizaciones_en(&env.estado_a, None).unwrap();
            let espera_id = pol1.espera_creada.expect("debe crear subtarea En espera");
            let esp_a = env
                .estado_a
                .con_db(|db| db_tareas::obtener(db.conexion(), &espera_id))
                .unwrap();
            assert_eq!(esp_a.tipo, TipoTarea::SinDefinir);
            assert_eq!(esp_a.titulo, "En espera");
            assert_eq!(esp_a.estado, EstadoTarea::EnProduccion);

            // Bloqueos: en padre no se hace check-in ni pasa a revisión mientras exista En espera.
            assert!(hacer_checkin_en(
                &env.estado_a,
                &rama_padre,
                "Prohibido",
                "",
                TipoTarea::Scripting
            )
            .is_err());
            assert!(cambiar_estado_en(&env.estado_a, &padre_id, EstadoTarea::EnRevision).is_err());
            assert!(cambiar_estado_en(&env.estado_a, &espera_id, EstadoTarea::EnRevision).is_err());

            env.sincronizar_b();
            let esp_b = db_tareas::obtener(env.db_b.conexion(), &espera_id).unwrap();
            assert_eq!(esp_b.tipo, TipoTarea::SinDefinir);

            // 2) Crear rama desde En espera: crea rama hija y lleva el cambio pendiente con `--noinput`.
            let convertida = crear_rama_desde_espera_en(
                &env.estado_a,
                &espera_id,
                "Nueva sub UI 106",
                "Movida desde rama padre",
                TipoTarea::UiInterfaz,
            )
            .unwrap();
            let rama_nueva = convertida.rama_asociada.clone().unwrap();
            env.registrar_rama(&rama_nueva);
            assert_eq!(convertida.tipo, TipoTarea::UiInterfaz);
            assert_eq!(convertida.estado, EstadoTarea::EnProduccion);

            let pend_hija = cambios_pendientes_en(&env.estado_a, Some(&rama_nueva)).unwrap();
            assert!(pend_hija.en_rama_esperada);
            assert!(
                pend_hija
                    .archivos
                    .iter()
                    .any(|a| a.contains("directo-en-padre-106.txt")),
                "archivos pendientes en la hija: {:?}",
                pend_hija.archivos
            );

            // Check-in real en la nueva hija + sync en B.
            hacer_checkin_en(
                &env.estado_a,
                &rama_nueva,
                "Commit movido desde espera",
                "Guardado en su rama",
                TipoTarea::UiInterfaz,
            )
            .unwrap();
            env.sincronizar_b();
            let conv_b = db_tareas::obtener(env.db_b.conexion(), &espera_id).unwrap();
            assert_eq!(conv_b.tipo, TipoTarea::UiInterfaz);
            assert_eq!(conv_b.rama_asociada.as_deref(), Some(rama_nueva.as_str()));

            // 3) Guardia de rama al crear desde espera + retiro automático si la rama padre queda limpia.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_padre).unwrap();
            let tmp_arch = env.ws_ruta.join("temporal-106.txt");
            std::fs::write(&tmp_arch, "temp").unwrap();
            Comando::cm()
                .arg("add")
                .arg(tmp_arch.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();

            let pol2 = revisar_actualizaciones_en(&env.estado_a, Some(&padre_id)).unwrap();
            let esp2_id = pol2.espera_creada.expect("debe crear segunda En espera");
            ramas::descartar_cambios_pendientes(&env.estado_a, &env.ws_ruta).unwrap();
            env.volver_a_dev();
            assert!(crear_rama_desde_espera_en(
                &env.estado_a,
                &esp2_id,
                "Desde dev bloqueado",
                "",
                TipoTarea::Audio
            )
            .is_err());

            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_padre).unwrap();
            let pol3 = revisar_actualizaciones_en(&env.estado_a, Some(&padre_id)).unwrap();
            assert_eq!(pol3.espera_quitada.as_deref(), Some(esp2_id.as_str()));
            env.sincronizar_b();
            assert!(db_tareas::buscar(env.db_b.conexion(), &esp2_id)
                .unwrap()
                .is_none());
        });
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_cambios_prestados() {
        use crate::cm::Comando;
        use crate::comandos::tareas::{
            asignar_en, cambiar_estado_en, devolver_cambios_prestados_en, hacer_checkin_en,
            listar_cambios_prestados_en, tomar_en, traer_cambios_en,
        };
        use crate::ramas;

        correr_entorno_real("108", |env| {
            let admin_email = env.admin_fila.email.clone();
            let origen = env.crear_tarea(&admin_email, Rol::Administrador, "Origen prestamo 108");
            let destino = env.crear_tarea(&admin_email, Rol::Administrador, "Destino prestamo 108");

            // Registrar destino después de origen: la limpieza borra primero destino (quita los merge links).
            let asg_origen = asignar_en(&env.estado_a, &origen.id, &admin_email).unwrap();
            env.registrar_ramas_de(&asg_origen);
            let rama_origen = asg_origen[0].rama_asociada.clone().unwrap();

            let tom_destino = tomar_en(&env.estado_a, &destino.id).unwrap();
            env.registrar_ramas_de(&tom_destino);
            let rama_destino = tom_destino[0].rama_asociada.clone().unwrap();

            // Trabajo en origen: dos commits reales en Unity VC.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_origen).unwrap();
            let arch1 = env.ws_ruta.join("prestamo-uno-108.txt");
            std::fs::write(&arch1, "uno 108").unwrap();
            Comando::cm()
                .arg("add")
                .arg(arch1.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            let commit1 = hacer_checkin_en(
                &env.estado_a,
                &rama_origen,
                "Primer avance 108",
                "",
                TipoTarea::Scripting,
            )
            .unwrap();

            let arch2 = env.ws_ruta.join("prestamo-dos-108.txt");
            let arch3 = env.ws_ruta.join("prestamo-tres-108.txt");
            std::fs::write(&arch2, "dos 108").unwrap();
            std::fs::write(&arch3, "tres 108").unwrap();
            Comando::cm()
                .arg("add")
                .args([
                    arch2.to_string_lossy().as_ref(),
                    arch3.to_string_lossy().as_ref(),
                ])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            hacer_checkin_en(
                &env.estado_a,
                &rama_origen,
                "Segundo avance 108",
                "",
                TipoTarea::Scripting,
            )
            .unwrap();

            // Trabajo propio en destino + pruebas de guardia de rama y "check-in primero".
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_destino).unwrap();
            let propio = env.ws_ruta.join("propio-108.txt");
            std::fs::write(&propio, "propio 108").unwrap();
            Comando::cm()
                .arg("add")
                .arg(propio.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            hacer_checkin_en(
                &env.estado_a,
                &rama_destino,
                "Propio destino 108",
                "",
                TipoTarea::UiInterfaz,
            )
            .unwrap();

            // Mientras origen esté en 'por hacer' se rechaza aunque ya tenga check-ins.
            assert!(
                traer_cambios_en(&env.estado_a, &destino.id, &origen.id, Some(&commit1.hash),)
                    .is_err()
            );

            cambiar_estado_en(&env.estado_a, &origen.id, EstadoTarea::EnProduccion).unwrap();
            cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::EnProduccion).unwrap();

            // Cambio pendiente en EnProduccion debe bloquear traer_cambios hasta hacer check-in o limpiar.
            let sucio = env.ws_ruta.join("sucio-108.txt");
            std::fs::write(&sucio, "sin checkin").unwrap();
            Comando::cm()
                .arg("add")
                .arg(sucio.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            let err_sucio =
                traer_cambios_en(&env.estado_a, &destino.id, &origen.id, Some(&commit1.hash))
                    .unwrap_err();
            assert!(err_sucio.to_string().contains("check-in"), "{err_sucio}");
            ramas::descartar_cambios_pendientes(&env.estado_a, &env.ws_ruta).unwrap();
            let _ = std::fs::remove_file(&sucio);

            // Con el workspace fuera de rama_destino, la guardia de rama también bloquea.
            env.volver_a_dev();
            let err_rama =
                traer_cambios_en(&env.estado_a, &destino.id, &origen.id, Some(&commit1.hash))
                    .unwrap_err();
            assert!(
                err_rama.to_string().contains("cambia de rama"),
                "{err_rama}"
            );

            // 1) Préstamo de un commit puntual (`commit1`) en EnProduccion + devolución sustractiva.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_destino).unwrap();
            let p1 = traer_cambios_en(&env.estado_a, &destino.id, &origen.id, Some(&commit1.hash))
                .unwrap();
            assert!(p1.changeset_merge.is_some());
            assert_eq!(p1.hash.as_deref(), Some(commit1.hash.as_str()));
            assert!(p1
                .archivos
                .iter()
                .any(|a| a.contains("prestamo-uno-108.txt")));
            assert!(!p1
                .archivos
                .iter()
                .any(|a| a.contains("prestamo-dos-108.txt")));
            assert!(env.ws_ruta.join("prestamo-uno-108.txt").exists());
            assert!(!env.ws_ruta.join("prestamo-dos-108.txt").exists());
            assert!(
                traer_cambios_en(&env.estado_a, &destino.id, &origen.id, Some(&commit1.hash),)
                    .is_err()
            );
            assert!(cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::Terminada).is_err());

            env.sincronizar_b();
            let p1_b = listar_cambios_prestados_en(&env.estado_a, Some(&destino.id)).unwrap();
            assert_eq!(p1_b.len(), 1);
            assert_eq!(
                db_tareas::obtener(env.db_b.conexion(), &destino.id)
                    .unwrap()
                    .id,
                destino.id
            );
            assert_eq!(
                commits::obtener_prestamo(env.db_b.conexion(), &p1.id).unwrap(),
                p1
            );

            let p1_dev = devolver_cambios_prestados_en(&env.estado_a, &p1.id).unwrap();
            assert!(p1_dev.quitado_en.is_some());
            assert!(p1_dev.quitado_por.is_some());
            assert!(!env.ws_ruta.join("prestamo-uno-108.txt").exists());
            assert!(env.ws_ruta.join("propio-108.txt").exists());
            env.sincronizar_b();
            assert!(commits::obtener_prestamo(env.db_b.conexion(), &p1.id)
                .unwrap()
                .quitado_en
                .is_some());

            // 2) Con destino en EnRevision y origen en Terminada: traer TODOS los cambios (intervalo) y devolver.
            cambiar_estado_en(&env.estado_a, &origen.id, EstadoTarea::EnRevision).unwrap();
            cambiar_estado_en(&env.estado_a, &origen.id, EstadoTarea::Terminada).unwrap();
            cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::EnRevision).unwrap();

            let p2 = traer_cambios_en(&env.estado_a, &destino.id, &origen.id, None).unwrap();
            assert!(p2.hash.is_none());
            assert!(p2.changeset_merge.is_some());
            for nombre in [
                "prestamo-uno-108.txt",
                "prestamo-dos-108.txt",
                "prestamo-tres-108.txt",
            ] {
                assert!(
                    p2.archivos.iter().any(|a| a.contains(nombre)),
                    "falta {nombre} en {:?}",
                    p2.archivos
                );
                assert!(env.ws_ruta.join(nombre).exists(), "debe existir {nombre}");
            }

            // Sigue bloqueando Terminada hasta devolverlo; tras devolver conserva historial y desbloquea.
            assert!(cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::Terminada).is_err());
            let p2_dev = devolver_cambios_prestados_en(&env.estado_a, &p2.id).unwrap();
            assert!(p2_dev.quitado_en.is_some());
            for nombre in [
                "prestamo-uno-108.txt",
                "prestamo-dos-108.txt",
                "prestamo-tres-108.txt",
            ] {
                assert!(
                    !env.ws_ruta.join(nombre).exists(),
                    "debió revertirse {nombre}"
                );
            }
            assert!(env.ws_ruta.join("propio-108.txt").exists());

            cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::Terminada).unwrap();
            env.sincronizar_b();
            let hist_b = commits::prestamos_de_tarea(env.db_b.conexion(), &destino.id).unwrap();
            assert_eq!(hist_b.len(), 2);
            assert!(hist_b.iter().all(|x| x.quitado_en.is_some()));
        });
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_cambios_ajenos_tocados() {
        use crate::cm::Comando;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::{
            asignar_en, cambiar_estado_en, devolver_cambios_prestados_en,
            eliminar_cambios_ajenos_en, hacer_checkin_en, tomar_en, traer_cambios_en,
        };
        use crate::ramas;

        correr_entorno_real("109", |env| {
            let admin_email = env.admin_fila.email.clone();
            let origen = env.crear_tarea(&admin_email, Rol::Administrador, "Origen ajenos 109");
            let destino = env.crear_tarea(&admin_email, Rol::Administrador, "Destino ajenos 109");

            let asg_origen = asignar_en(&env.estado_a, &origen.id, &admin_email).unwrap();
            env.registrar_ramas_de(&asg_origen);
            let rama_origen = asg_origen[0].rama_asociada.clone().unwrap();

            let tom_destino = tomar_en(&env.estado_a, &destino.id).unwrap();
            env.registrar_ramas_de(&tom_destino);
            let rama_destino = tom_destino[0].rama_asociada.clone().unwrap();

            // Commit en origen con 2 archivos prestables.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_origen).unwrap();
            let a1 = env.ws_ruta.join("ajeno-uno-109.txt");
            let a2 = env.ws_ruta.join("ajeno-dos-109.txt");
            std::fs::write(&a1, "original ajeno 109").unwrap();
            std::fs::write(&a2, "original dos 109").unwrap();
            Comando::cm()
                .arg("add")
                .args([a1.to_string_lossy().as_ref(), a2.to_string_lossy().as_ref()])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            hacer_checkin_en(
                &env.estado_a,
                &rama_origen,
                "Base prestamo 109",
                "",
                TipoTarea::Scripting,
            )
            .unwrap();
            cambiar_estado_en(&env.estado_a, &origen.id, EstadoTarea::EnProduccion).unwrap();

            // Trabajo propio en destino + préstamo de origen.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_destino).unwrap();
            let propio = env.ws_ruta.join("propio-109.txt");
            std::fs::write(&propio, "propio v1").unwrap();
            Comando::cm()
                .arg("add")
                .arg(propio.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            hacer_checkin_en(
                &env.estado_a,
                &rama_destino,
                "Propio inicial 109",
                "",
                TipoTarea::Scripting,
            )
            .unwrap();
            cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::EnProduccion).unwrap();

            let prestamo = traer_cambios_en(&env.estado_a, &destino.id, &origen.id, None).unwrap();

            // 1) Cambio sin check-in en archivo prestado: el polling lo detecta antes de guardarse.
            std::fs::write(&a1, "tocado sin checkin").unwrap();
            let pol1 = revisar_actualizaciones_en(&env.estado_a, Some(&destino.id)).unwrap();
            assert!(pol1.tocados_detectados.contains(&destino.id));
            let tras_local = env
                .estado_a
                .con_db(|db| commits::obtener_prestamo(db.conexion(), &prestamo.id))
                .unwrap();
            assert_eq!(
                tras_local.archivos_tocados,
                vec!["ajeno-uno-109.txt".to_string()]
            );

            // Limpiar el toque sin check-in vuelve el archivo al original.
            let limpio_local = eliminar_cambios_ajenos_en(&env.estado_a, &prestamo.id).unwrap();
            assert!(limpio_local.archivos_tocados.is_empty());
            assert_eq!(std::fs::read_to_string(&a1).unwrap(), "original ajeno 109");

            // 2) Tocar otro archivo prestado junto con uno propio y hacer check-in en Unity VC.
            std::fs::write(&a2, "alterado en checkin 109").unwrap();
            std::fs::write(&propio, "propio v2").unwrap();
            hacer_checkin_en(
                &env.estado_a,
                &rama_destino,
                "Cambio propio mezclando ajeno",
                "",
                TipoTarea::Scripting,
            )
            .unwrap();

            let tras_ci = env
                .estado_a
                .con_db(|db| commits::obtener_prestamo(db.conexion(), &prestamo.id))
                .unwrap();
            assert_eq!(
                tras_ci.archivos_tocados,
                vec!["ajeno-dos-109.txt".to_string()]
            );

            // También debe detectar desde cero vía polling por diff de check-in nuevo.
            env.estado_a
                .con_db(|db| commits::marcar_tocados(db.conexion(), &prestamo.id, &[]))
                .unwrap();
            let pol2 = revisar_actualizaciones_en(&env.estado_a, Some(&destino.id)).unwrap();
            assert!(pol2.tocados_detectados.contains(&destino.id));

            // Bloquea devolución y Terminada hasta limpiar cambios ajenos; valida guardia de rama.
            assert!(devolver_cambios_prestados_en(&env.estado_a, &prestamo.id).is_err());
            cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::EnRevision).unwrap();
            let err_term =
                cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::Terminada).unwrap_err();
            assert!(
                err_term.to_string().contains("cambios ajenos"),
                "{err_term}"
            );

            env.volver_a_dev();
            let err_guard = eliminar_cambios_ajenos_en(&env.estado_a, &prestamo.id).unwrap_err();
            assert!(
                err_guard.to_string().contains("cambia de rama"),
                "{err_guard}"
            );

            // Restaurar en la rama + check-in automático; preserva `propio-109.txt` v2.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_destino).unwrap();
            let limpio_ci = eliminar_cambios_ajenos_en(&env.estado_a, &prestamo.id).unwrap();
            assert!(limpio_ci.archivos_tocados.is_empty());
            assert!(limpio_ci.changeset_limpio.is_some());
            assert_eq!(std::fs::read_to_string(&a2).unwrap(), "original dos 109");
            assert_eq!(std::fs::read_to_string(&propio).unwrap(), "propio v2");

            // Polling posterior NO vuelve a marcar el check-in ya restaurado, y queda registrado en Unity VC.
            let pol3 = revisar_actualizaciones_en(&env.estado_a, Some(&destino.id)).unwrap();
            assert!(!pol3.tocados_detectados.contains(&destino.id));
            let commits_dest = env
                .estado_a
                .con_db(|db| commits::de_tarea(db.conexion(), &destino.id))
                .unwrap();
            assert!(commits_dest
                .iter()
                .any(|c| c.mensaje.contains("Se eliminaron cambios ajenos")));

            // Máquina B recibe la limpieza por ops/, ya se puede devolver y pasar a Terminada.
            env.sincronizar_b();
            let pb = commits::obtener_prestamo(env.db_b.conexion(), &prestamo.id).unwrap();
            assert!(pb.archivos_tocados.is_empty());
            assert!(pb.changeset_limpio.is_some());

            devolver_cambios_prestados_en(&env.estado_a, &prestamo.id).unwrap();
            assert!(!a1.exists());
            assert!(!a2.exists());
            assert_eq!(std::fs::read_to_string(&propio).unwrap(), "propio v2");
            cambiar_estado_en(&env.estado_a, &destino.id, EstadoTarea::Terminada).unwrap();
        });
    }

    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_alerta_rama_ajena() {
        use crate::cm::Comando;
        use crate::comandos::eventos::revisar_actualizaciones_en;
        use crate::comandos::tareas::asignar_en;
        use crate::ramas;

        correr_entorno_real("111", |env| {
            let compa = "compa.ajeno@ejemplo.com";
            env.crear_usuario(compa, Rol::Desarrollador);
            let admin_email = env.admin_fila.email.clone();

            let ajena = env.crear_tarea(&admin_email, Rol::Administrador, "Rama ajena 111");
            let propia = env.crear_tarea(&admin_email, Rol::Administrador, "Rama propia 111");

            let asg_ajena = asignar_en(&env.estado_a, &ajena.id, compa).unwrap();
            env.registrar_ramas_de(&asg_ajena);
            let rama_ajena = asg_ajena[0].rama_asociada.clone().unwrap();

            let asg_propia = asignar_en(&env.estado_a, &propia.id, &admin_email).unwrap();
            env.registrar_ramas_de(&asg_propia);
            let rama_propia = asg_propia[0].rama_asociada.clone().unwrap();

            // 1) Cambios exclusivos .meta en /main/dev NO generan falsa alarma.
            env.volver_a_dev();
            let meta_dev = env.ws_ruta.join("AutoUnity.cs.meta");
            std::fs::write(&meta_dev, "guid: 123").unwrap();
            Comando::cm()
                .arg("add")
                .arg(meta_dev.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            let pol_meta = revisar_actualizaciones_en(&env.estado_a, None).unwrap();
            assert!(pol_meta.alerta_rama_ajena.is_none());

            // 2) Cambio real sin check-in en /main/dev dispara alerta, sugiere rama propia y NO deshace nada.
            let dev_real = env.ws_ruta.join("cambio-en-dev-111.txt");
            std::fs::write(&dev_real, "contenido sin perder 111").unwrap();
            Comando::cm()
                .arg("add")
                .arg(dev_real.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            let pol_dev = revisar_actualizaciones_en(&env.estado_a, None).unwrap();
            let alerta_dev = pol_dev
                .alerta_rama_ajena
                .expect("debe alertar cambio real en /main/dev");
            assert_eq!(alerta_dev.tarea_ajena_id, None);
            assert_eq!(
                alerta_dev.rama_sugerida.as_deref(),
                Some(rama_propia.as_str())
            );
            assert_eq!(
                alerta_dev.archivos,
                vec!["cambio-en-dev-111.txt".to_string()]
            );
            assert_eq!(
                std::fs::read_to_string(&dev_real).unwrap(),
                "contenido sin perder 111"
            );

            // Limpiamos antes de cambiar de rama en el workspace.
            ramas::descartar_cambios_pendientes(&env.estado_a, &env.ws_ruta).unwrap();
            let _ = std::fs::remove_file(&meta_dev);
            let _ = std::fs::remove_file(&dev_real);

            // 3) En rama de otro compañero (`compa`): cambio sin check-in y luego check-in directo.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_ajena).unwrap();
            let arch_ajeno = env.ws_ruta.join("edicion-ajena-111.txt");
            std::fs::write(&arch_ajeno, "editado en rama ajena").unwrap();
            Comando::cm()
                .arg("add")
                .arg(arch_ajeno.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();

            let pol_ajena = revisar_actualizaciones_en(&env.estado_a, None).unwrap();
            let alerta_aj = pol_ajena
                .alerta_rama_ajena
                .expect("debe alertar cambio pendiente en rama ajena");
            assert_eq!(alerta_aj.tarea_ajena_id.as_deref(), Some(ajena.id.as_str()));
            assert_eq!(alerta_aj.dueno_rama.as_deref(), Some(compa));
            assert_eq!(
                alerta_aj.rama_sugerida.as_deref(),
                Some(rama_propia.as_str())
            );
            assert!(arch_ajeno.exists());

            // Simular check-in directo hecho desde Unity VC en la rama ajena.
            Comando::cm()
                .args(["checkin", "--all", "-c=Checkin equivocado 111"])
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            let pol_ci = revisar_actualizaciones_en(&env.estado_a, None).unwrap();
            let alerta_ci = pol_ci
                .alerta_rama_ajena
                .expect("debe alertar check-in propio reciente en rama ajena");
            assert_eq!(alerta_ci.tarea_ajena_id.as_deref(), Some(ajena.id.as_str()));
            assert!(alerta_ci
                .archivos
                .iter()
                .any(|a| a.contains("edicion-ajena-111.txt")));

            // 4) En la rama propia asignada NO dispara alerta aunque tenga cambios reales.
            ramas::cambiar_a_rama(&env.estado_a, &env.ws_ruta, &rama_propia).unwrap();
            let arch_mio = env.ws_ruta.join("mio-111.txt");
            std::fs::write(&arch_mio, "mio OK").unwrap();
            Comando::cm()
                .arg("add")
                .arg(arch_mio.to_string_lossy().as_ref())
                .en(&env.ws_ruta)
                .exigir_exito(&env.estado_a)
                .unwrap();
            let pol_propia = revisar_actualizaciones_en(&env.estado_a, None).unwrap();
            assert!(pol_propia.alerta_rama_ajena.is_none());

            ramas::descartar_cambios_pendientes(&env.estado_a, &env.ws_ruta).unwrap();
            let _ = std::fs::remove_file(&arch_mio);
        });
    }
}
