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

use crate::db::tareas::{self as db_tareas, Tarea};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::identidad::firmas::{verificar_usuarios, Verificacion};
use crate::modelo::{normalizar_email, EstadoTarea, Rol, TipoTarea};

/// Tipos de evento que maneja este módulo.
pub const TIPOS: &[&str] = &["crear_tarea", "crear_tarea_con_subtareas", "editar_tarea"];

/// Máximo de ancestros que puede tener una tarea en `tareas`:
/// `tarea general (0) -> subtarea (1) -> subtarea de subtarea (2)`, que junto
/// con `/main/dev` suman los 5 niveles máximos de Unity VC (#83).
const MAX_ANCESTROS_TAREA: usize = 2;

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
        otro => Err(Error::Validacion(format!(
            "tipo de evento de tarea no soportado: {otro}"
        ))),
    }
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
}
