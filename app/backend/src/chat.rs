//! Lógica de dominio y sincronización por `ops/` para el chat del equipo (#119).
//!
//! - `enviar_chat` y `crear_grupo_chat` publican eventos en `ops/` para replicarse
//!   entre máquinas (quien envía queda contado como lector inicial).
//! - `marcar_chat_leido` y `eliminar_chat` son locales por usuario en SQLite.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::db::chat::{self as db_chat, GrupoChat, Mensaje, GRUPO_GENERAL};
use crate::db::usuarios;
use crate::error::{Error, Resultado};
use crate::eventos::{self, Evento};
use crate::modelo::normalizar_email;
use crate::tareas::rol_de;

/// Tipos de evento de chat manejados desde `ops/`.
pub const TIPOS: &[&str] = &["enviar_chat", "crear_grupo_chat"];

/// Estado completo de chat visible para un usuario (#119).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EstadoChat {
    pub grupos: Vec<GrupoChat>,
    pub mensajes: Vec<Mensaje>,
    pub conversaciones_sin_leer: Vec<String>,
    pub eliminados: BTreeMap<String, String>,
}

fn normalizar_conversacion(conversacion: &str) -> Resultado<String> {
    let limpia = conversacion.trim();
    if limpia.is_empty() {
        return Err(Error::Validacion("Indica la conversación".into()));
    }
    if limpia.eq_ignore_ascii_case(GRUPO_GENERAL) {
        return Ok(GRUPO_GENERAL.to_string());
    }
    if limpia.contains('@') {
        return Ok(normalizar_email(limpia));
    }
    Ok(limpia.to_string())
}

fn exigir_acceso_conversacion(
    c: &Connection,
    yo: &str,
    conv: &str,
    para_enviar: bool,
) -> Resultado<()> {
    if conv == GRUPO_GENERAL {
        return Ok(());
    }
    if conv.contains('@') {
        if conv == yo {
            return Err(Error::Validacion(
                "No puedes abrir una conversación privada contigo mismo".into(),
            ));
        }
        if para_enviar && usuarios::buscar(c, conv)?.is_none() {
            return Err(Error::NoEncontrado(format!("el usuario {conv}")));
        }
        return Ok(());
    }
    let grupo = db_chat::buscar_grupo(c, conv)?
        .ok_or_else(|| Error::NoEncontrado(format!("el grupo de chat {conv}")))?;
    if !grupo.miembros.iter().any(|m| m == yo) {
        return Err(Error::SinPermiso);
    }
    Ok(())
}

/// Envía un mensaje al grupo general (`"grupo"`), a un grupo creado o en privado a un
/// compañero, y lo publica en `ops/` (#119). Quien envía ya lo cuenta como leído.
pub fn enviar(
    c: &Connection,
    emisor: &str,
    para: &str,
    texto: &str,
    ahora: &str,
) -> Resultado<Mensaje> {
    let de = normalizar_email(emisor);
    if usuarios::buscar(c, &de)?.is_none() {
        return Err(Error::NoEncontrado(format!("el usuario {de}")));
    }
    let destino = normalizar_conversacion(para)?;
    exigir_acceso_conversacion(c, &de, &destino, true)?;

    let texto_limpio = texto.trim();
    if texto_limpio.is_empty() {
        return Err(Error::Validacion(
            "Escribe un mensaje antes de enviarlo".into(),
        ));
    }

    let msg = Mensaje {
        id: format!("M-{}", eventos::uuid()),
        de: de.clone(),
        para: destino,
        texto: texto_limpio.to_string(),
        fecha: ahora.to_string(),
        leido_por: vec![de.clone()],
    };
    let mut ev = Evento::nuevo(
        "enviar_chat",
        None,
        &de,
        serde_json::to_value(&msg)
            .map_err(|e| Error::Interno(format!("no se pudo serializar enviar_chat: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| aplicar_enviar_chat(tx, &de, &msg))?;
    Ok(msg)
}

/// Crea un grupo de chat (cualquier rol, siempre incluyendo al creador) y lo replica por `ops/` (#119).
pub fn crear_grupo(
    c: &Connection,
    creador: &str,
    nombre: &str,
    miembros: &[String],
    ahora: &str,
) -> Resultado<GrupoChat> {
    let creador_norm = normalizar_email(creador);
    if usuarios::buscar(c, &creador_norm)?.is_none() {
        return Err(Error::NoEncontrado(format!("el usuario {creador_norm}")));
    }
    let nombre_limpio = nombre.trim();
    if nombre_limpio.is_empty() {
        return Err(Error::Validacion("El grupo necesita un nombre".into()));
    }

    let mut unicos = BTreeSet::new();
    unicos.insert(creador_norm.clone());
    for raw in miembros {
        let email = normalizar_email(raw);
        if email.is_empty() {
            continue;
        }
        if usuarios::buscar(c, &email)?.is_none() {
            return Err(Error::NoEncontrado(format!("el usuario {email}")));
        }
        unicos.insert(email);
    }

    let grupo = GrupoChat {
        id: format!("GR-{}", eventos::uuid()),
        nombre: nombre_limpio.to_string(),
        miembros: unicos.into_iter().collect(),
        creado_por: creador_norm.clone(),
        fecha: ahora.to_string(),
    };
    let mut ev = Evento::nuevo(
        "crear_grupo_chat",
        None,
        &creador_norm,
        serde_json::to_value(&grupo)
            .map_err(|e| Error::Interno(format!("no se pudo serializar crear_grupo_chat: {e}")))?,
    );
    ev.timestamp = ahora.to_string();
    eventos::publicar(c, &ev, |tx| {
        aplicar_crear_grupo_chat(tx, &creador_norm, &grupo)
    })?;
    Ok(grupo)
}

fn aplicar_enviar_chat(c: &Connection, emisor: &str, raw: &Mensaje) -> Resultado<()> {
    let de = normalizar_email(&raw.de);
    if de != normalizar_email(emisor) {
        return Err(Error::SinPermiso);
    }
    if db_chat::buscar_mensaje(c, &raw.id)?.is_some() {
        return Ok(());
    }
    let destino = normalizar_conversacion(&raw.para)?;
    exigir_acceso_conversacion(c, &de, &destino, false)?;

    let msg = Mensaje {
        id: raw.id.clone(),
        de: de.clone(),
        para: destino,
        texto: raw.texto.trim().to_string(),
        fecha: raw.fecha.clone(),
        leido_por: vec![de],
    };
    db_chat::guardar_mensaje(c, &msg)
}

fn aplicar_crear_grupo_chat(c: &Connection, creador: &str, raw: &GrupoChat) -> Resultado<()> {
    let creador_norm = normalizar_email(&raw.creado_por);
    if creador_norm != normalizar_email(creador) {
        return Err(Error::SinPermiso);
    }
    if db_chat::buscar_grupo(c, &raw.id)?.is_some() {
        return Ok(());
    }
    let mut miembros: BTreeSet<String> = raw.miembros.iter().map(|m| normalizar_email(m)).collect();
    miembros.insert(creador_norm.clone());

    let grupo = GrupoChat {
        id: raw.id.clone(),
        nombre: raw.nombre.trim().to_string(),
        miembros: miembros.into_iter().filter(|m| !m.is_empty()).collect(),
        creado_por: creador_norm,
        fecha: raw.fecha.clone(),
    };
    db_chat::crear_grupo(c, &grupo)
}

/// Reproduce un evento de chat recibido desde `ops/` exigiendo un emisor verificado.
pub fn aplicar_evento(c: &Connection, evento: &Evento) -> Resultado<()> {
    if rol_de(c, &evento.usuario)?.is_none() {
        return Err(Error::SinPermiso);
    }
    match evento.tipo.as_str() {
        "enviar_chat" => {
            let msg: Mensaje = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("enviar_chat inválido: {e}")))?;
            aplicar_enviar_chat(c, &evento.usuario, &msg)
        }
        "crear_grupo_chat" => {
            let grupo: GrupoChat = serde_json::from_value(evento.payload.clone())
                .map_err(|e| Error::Validacion(format!("crear_grupo_chat inválido: {e}")))?;
            aplicar_crear_grupo_chat(c, &evento.usuario, &grupo)
        }
        otro => Err(Error::Validacion(format!(
            "tipo de evento de chat no soportado: {otro}"
        ))),
    }
}

/// Estado completo del chat visible para `yo` en esta máquina (#119).
pub fn obtener_estado(c: &Connection, yo: &str) -> Resultado<EstadoChat> {
    let yo_norm = normalizar_email(yo);
    let grupos = db_chat::grupos_de(c, &yo_norm)?;
    let mensajes = db_chat::listar_visibles(c, &yo_norm, &grupos)?;
    let conversaciones_sin_leer = db_chat::conversaciones_sin_leer(c, &yo_norm)?;
    let eliminados = db_chat::eliminados_de(c, &yo_norm)?;
    Ok(EstadoChat {
        grupos,
        mensajes,
        conversaciones_sin_leer,
        eliminados,
    })
}

/// Lista los ids/correos de las conversaciones con mensajes sin leer de `yo` (#119).
pub fn conversaciones_sin_leer(c: &Connection, yo: &str) -> Resultado<Vec<String>> {
    db_chat::conversaciones_sin_leer(c, &normalizar_email(yo))
}

/// Marca como leídos todos los mensajes visibles de `conversacion` para `yo` (local #119).
pub fn marcar_leido(c: &Connection, yo: &str, conversacion: &str) -> Resultado<usize> {
    let yo_norm = normalizar_email(yo);
    let conv = normalizar_conversacion(conversacion)?;
    exigir_acceso_conversacion(c, &yo_norm, &conv, false)?;
    db_chat::marcar_conversacion_leida(c, &yo_norm, &conv)
}

/// Oculta localmente una conversación para `yo` hasta el momento `ahora` (local #119).
pub fn eliminar(c: &Connection, yo: &str, conversacion: &str, ahora: &str) -> Resultado<()> {
    let yo_norm = normalizar_email(yo);
    let conv = normalizar_conversacion(conversacion)?;
    exigir_acceso_conversacion(c, &yo_norm, &conv, false)?;
    db_chat::marcar_conversacion_leida(c, &yo_norm, &conv)?;
    db_chat::eliminar_conversacion(c, &yo_norm, &conv, ahora)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use crate::db::Db;
    use crate::eventos::lectura::{self};
    use crate::modelo::Rol;

    const ANA: &str = "ana@estudio.com";
    const ZOE: &str = "zoe@estudio.com";
    const LEO: &str = "leo@estudio.com";
    const INTRUSO: &str = "intruso@fuera.com";
    const AHORA: &str = "2026-11-16T10:00:00.000Z";
    const HOY: &str = "2026-11-16";

    struct Escenario {
        _tmp: TempDir,
        db_a: Db,
        db_b: Db,
        local_b: PathBuf,
    }

    fn setup() -> Escenario {
        let tmp = TempDir::new().unwrap();
        let compartida = tmp.path().join("compartida");
        std::fs::create_dir_all(&compartida).unwrap();
        let local_a = tmp
            .path()
            .join("datos_a")
            .join("proyectos")
            .join("estudio-chat");
        let local_b = tmp
            .path()
            .join("datos_b")
            .join("proyectos")
            .join("estudio-chat");

        let db_a = Db::abrir(&local_a).unwrap();
        let db_b = Db::abrir(&local_b).unwrap();

        for (correo, rol) in [
            (ANA, Rol::Administrador),
            (ZOE, Rol::Desarrollador),
            (LEO, Rol::Tester),
        ] {
            let fila = usuarios::Usuario {
                email: correo.into(),
                nombre: correo.split('@').next().unwrap().into(),
                apellido: "Test".into(),
                rol,
                fecha_alta: HOY.into(),
            };
            usuarios::insertar(db_a.conexion(), &fila).unwrap();
            usuarios::insertar(db_b.conexion(), &fila).unwrap();
        }

        Escenario {
            _tmp: tmp,
            db_a,
            db_b,
            local_b,
        }
    }

    #[test]
    fn chat_grupos_privados_contador_y_eliminacion_local_viajan_por_ops() {
        let e = setup();

        // Validaciones al crear grupo y enviar mensaje.
        assert!(crear_grupo(e.db_a.conexion(), ANA, "  ", &[ZOE.into()], AHORA).is_err());
        assert!(crear_grupo(
            e.db_a.conexion(),
            ANA,
            "Arte",
            &[INTRUSO.into()],
            AHORA
        )
        .is_err());
        assert!(enviar(e.db_a.conexion(), ANA, ANA, "Hola yo", AHORA).is_err());
        assert!(enviar(e.db_a.conexion(), ANA, INTRUSO, "Hola", AHORA).is_err());
        assert!(enviar(e.db_a.conexion(), ANA, GRUPO_GENERAL, "  ", AHORA).is_err());

        // Grupo creado por Ana con Zoe (Leo queda fuera).
        let grupo = crear_grupo(
            e.db_a.conexion(),
            ANA,
            "Arte 3D",
            &["ZOE@estudio.com".into(), ANA.into()],
            "2026-11-16T10:01:00.000Z",
        )
        .unwrap();
        assert_eq!(grupo.miembros, vec![ANA.to_string(), ZOE.to_string()]);
        assert_eq!(
            enviar(
                e.db_a.conexion(),
                LEO,
                &grupo.id,
                "No soy miembro",
                "2026-11-16T10:01:05.000Z",
            )
            .unwrap_err()
            .codigo(),
            "sin_permiso"
        );
        assert_eq!(
            marcar_leido(e.db_a.conexion(), LEO, &grupo.id)
                .unwrap_err()
                .codigo(),
            "sin_permiso"
        );

        // 2 mensajes en "grupo", 1 en grupo Arte 3D y 1 privado Ana -> Zoe.
        enviar(
            e.db_a.conexion(),
            ANA,
            GRUPO_GENERAL,
            "Buenos días equipo",
            "2026-11-16T10:01:10.000Z",
        )
        .unwrap();
        enviar(
            e.db_a.conexion(),
            ANA,
            GRUPO_GENERAL,
            "Revisen backlog",
            "2026-11-16T10:01:20.000Z",
        )
        .unwrap();
        enviar(
            e.db_a.conexion(),
            ANA,
            &grupo.id,
            "Modelos listos",
            "2026-11-16T10:01:30.000Z",
        )
        .unwrap();
        enviar(
            e.db_a.conexion(),
            ANA,
            "ZOE@estudio.com",
            "¿Puedes ver mi rama?",
            "2026-11-16T10:01:40.000Z",
        )
        .unwrap();

        // Quien envía ya los cuenta como leídos.
        assert!(conversaciones_sin_leer(e.db_a.conexion(), ANA)
            .unwrap()
            .is_empty());

        // Intruso no registrado se descarta al aplicar eventos.
        let falsa = Mensaje {
            id: "M-falso".into(),
            de: INTRUSO.into(),
            para: GRUPO_GENERAL.into(),
            texto: "Spam".into(),
            fecha: "2026-11-16T10:02:00.000Z".into(),
            leido_por: vec![],
        };
        assert_eq!(
            aplicar_evento(
                e.db_a.conexion(),
                &Evento::nuevo(
                    "enviar_chat",
                    None,
                    INTRUSO,
                    serde_json::to_value(&falsa).unwrap(),
                ),
            )
            .unwrap_err()
            .codigo(),
            "sin_permiso"
        );

        // B sincroniza vía ops/: cuenta conversaciones (no mensajes) por usuario.
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T10:05:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        let sync = crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora)
        .unwrap();
        assert_eq!(sync.aplicados, 5);

        assert_eq!(
            conversaciones_sin_leer(e.db_b.conexion(), ZOE).unwrap(),
            vec![grupo.id.clone(), ANA.to_string(), GRUPO_GENERAL.to_string()]
        );
        assert_eq!(
            conversaciones_sin_leer(e.db_b.conexion(), LEO).unwrap(),
            vec![GRUPO_GENERAL.to_string()]
        );

        // Zoe marca leído solo en máquina B; en máquina A sigue sin leer para Zoe.
        assert_eq!(
            marcar_leido(e.db_b.conexion(), ZOE, GRUPO_GENERAL).unwrap(),
            2
        );
        assert_eq!(
            conversaciones_sin_leer(e.db_b.conexion(), ZOE).unwrap(),
            vec![grupo.id.clone(), ANA.to_string()]
        );
        assert_eq!(
            conversaciones_sin_leer(e.db_a.conexion(), ZOE)
                .unwrap()
                .len(),
            3
        );

        // Zoe elimina el privado con Ana en B: oculta lo previo y revive solo con un mensaje nuevo.
        eliminar(e.db_b.conexion(), ZOE, ANA, "2026-11-16T10:05:00.000Z").unwrap();
        let estado_zoe = obtener_estado(e.db_b.conexion(), ZOE).unwrap();
        assert!(!estado_zoe
            .mensajes
            .iter()
            .any(|m| (m.de == ANA && m.para == ZOE) || (m.de == ZOE && m.para == ANA)));
        assert_eq!(estado_zoe.conversaciones_sin_leer, vec![grupo.id.clone()]);

        // Nuevo mensaje de Ana tras la eliminación revive el chat solo con ese mensaje.
        let m_nuevo = enviar(
            e.db_a.conexion(),
            ANA,
            ZOE,
            "Seguimos después de comida",
            "2026-11-16T10:06:00.000Z",
        )
        .unwrap();
        let ahora2 = DateTime::parse_from_rfc3339("2026-11-16T10:07:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        crate::eventos::servidor_de_prueba::entregar(e.db_b.conexion(), ahora2)
        .unwrap();
        let revivida: Vec<_> = obtener_estado(e.db_b.conexion(), ZOE)
            .unwrap()
            .mensajes
            .into_iter()
            .filter(|m| m.de == ANA && m.para == ZOE)
            .collect();
        assert_eq!(revivida.len(), 1);
        assert_eq!(revivida[0].id, m_nuevo.id);
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #119): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// ejerce todos los comandos Tauri del chat (`crear_grupo_chat_en`, `enviar_chat_en`,
    /// `obtener_chat_en`, `conversaciones_sin_leer_en`, `marcar_chat_leido_en`,
    /// `eliminar_chat_en`):
    /// 1. Crea Desarrollador y Tester; crea un grupo excluyendo al Tester y valida rechazo
    ///    de lectura/envío para no miembros.
    /// 2. Envía mensajes al grupo general `"grupo"`, al grupo creado y en privado, más un
    ///    evento falsificado en OneDrive para verificar descarte.
    /// 3. Sincroniza en máquina B vía OneDrive y valida contador por conversaciones,
    ///    aislamiento local de `marcar_chat_leido_en` / `eliminar_chat_en` y resurgimiento
    ///    del chat eliminado al llegar un mensaje nuevo.
    ///
    /// Al terminar borra workspace y subcarpeta en OneDrive.
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_chat() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::chat::{
            conversaciones_sin_leer_en, crear_grupo_chat_en, eliminar_chat_en, enviar_chat_en,
            marcar_chat_leido_en, obtener_chat_en,
        };
        use crate::comandos::datos::carpeta_abierta;
        use crate::estado::AppState;

        use crate::modelo::hoy_local;

        const TESTER: &str = "tester119@estudio.com";

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-119-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-119-{sufijo}"))
            .arg(&ruta_ws)
            .arg(format!("rep:{repo}@{org}@unity"))
            .exigir_exito(&estado_a)
            .unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entrar_en(&estado_a, datos_a.path(), ws_a.path()).unwrap();
            let local_a = carpeta_abierta(&estado_a).unwrap();
            let local_b = maquina_b
                .path()
                .join("proyectos")
                .join(local_a.file_name().unwrap());


            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let hoy_real = hoy_local();
            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));
            let ahora_pausa = || {
                pausa();
                crate::modelo::ahora_iso()
            };

            estado_a
                .con_db(|db_a| {
                    for (correo, rol) in [(ZOE, Rol::Desarrollador), (TESTER, Rol::Tester)] {
                        Ok::<_, crate::error::Error>({
                            let u = crate::identidad::equipo::alta_de_prueba(
                                db_a.conexion(),
                                correo,
                                rol,
                                &hoy_real,
                            );
                            crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                            u
                        })?;
                    }
                    Ok(())
                })
                .unwrap();

            // 1. Admin crea grupo con Zoe y envía a general, grupo y privado con Zoe.
            pausa();
            let grupo = crear_grupo_chat_en(&estado_a, "Programación", &[ZOE.to_string()]).unwrap();
            assert!(grupo.miembros.contains(&admin_fila.email));
            assert!(grupo.miembros.contains(&ZOE.to_string()));

            pausa();
            enviar_chat_en(&estado_a, GRUPO_GENERAL, "Mensaje general 1").unwrap();
            pausa();
            enviar_chat_en(&estado_a, GRUPO_GENERAL, "Mensaje general 2").unwrap();
            pausa();
            enviar_chat_en(&estado_a, &grupo.id, "Mensaje en Programación").unwrap();
            pausa();
            enviar_chat_en(&estado_a, ZOE, "Hola Zoe en privado").unwrap();

            // Admin ya contó todo lo suyo como leído.
            assert!(conversaciones_sin_leer_en(&estado_a).unwrap().is_empty());

            // Tester no pertenece a `grupo` -> bloqueado al enviar/marcar; en cambio sí responde en "grupo".
            estado_a.fijar_usuario(Some(TESTER));
            pausa();
            assert_eq!(
                enviar_chat_en(&estado_a, &grupo.id, "Intento ajeno")
                    .unwrap_err()
                    .codigo(),
                "sin_permiso"
            );
            assert_eq!(
                conversaciones_sin_leer_en(&estado_a).unwrap(),
                vec![GRUPO_GENERAL.to_string()]
            );
            assert_eq!(marcar_chat_leido_en(&estado_a, GRUPO_GENERAL).unwrap(), 2);
            assert!(conversaciones_sin_leer_en(&estado_a).unwrap().is_empty());

            // Eliminar chat localmente oculta en A, mientras Zoe sigue viendo el historial.
            estado_a.fijar_usuario(Some(ZOE));
            pausa();
            let mut esperadas_zoe = vec![
                grupo.id.clone(),
                admin_fila.email.clone(),
                GRUPO_GENERAL.to_string(),
            ];
            esperadas_zoe.sort();
            assert_eq!(
                conversaciones_sin_leer_en(&estado_a).unwrap(),
                esperadas_zoe
            );
            pausa();
            eliminar_chat_en(&estado_a, &admin_fila.email).unwrap();
            assert_eq!(
                conversaciones_sin_leer_en(&estado_a).unwrap(),
                vec![grupo.id.clone(), GRUPO_GENERAL.to_string()]
            );

            // Admin manda otro mensaje privado -> revive el chat eliminado de Zoe en A.
            estado_a.fijar_usuario(Some(&admin_fila.email));
            pausa();
            let revive = enviar_chat_en(&estado_a, ZOE, "Reactivando hilo").unwrap();

            // Inyectamos un evento falsificado de intruso en OneDrive.
            let mut ev_falso = Evento::nuevo(
                "enviar_chat",
                None,
                INTRUSO,
                serde_json::to_value(&Mensaje {
                    id: "M-intruso-real".into(),
                    de: INTRUSO.into(),
                    para: GRUPO_GENERAL.into(),
                    texto: "Falso".into(),
                    fecha: ahora_pausa(),
                    leido_por: vec![],
                })
                .unwrap(),
            );
            ev_falso.timestamp = crate::modelo::ahora_iso();
            crate::eventos::servidor_de_prueba::agregar(&ev_falso);

            std::thread::sleep(std::time::Duration::from_secs(2));

            // 2. Máquina B sincroniza desde OneDrive.
            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b = crate::eventos::servidor_de_prueba::entregar(db_b.conexion(), ahora)
            .unwrap();
            println!("B sincroniza chat desde OneDrive: {sync_b:?}");
            assert_eq!(sync_b.descartados, 1);

            let estado_zoe_b = obtener_estado(db_b.conexion(), ZOE).unwrap();
            assert_eq!(estado_zoe_b.grupos.len(), 1);
            assert_eq!(estado_zoe_b.conversaciones_sin_leer, esperadas_zoe);

            // En A (donde Zoe había eliminado antes de `revive`), Zoe solo ve `revive` en el privado.
            estado_a.fijar_usuario(Some(ZOE));
            let priv_zoe_a: Vec<_> = obtener_chat_en(&estado_a)
                .unwrap()
                .mensajes
                .into_iter()
                .filter(|m| m.de == admin_fila.email && m.para == ZOE)
                .collect();
            assert_eq!(priv_zoe_a.len(), 1);
            assert_eq!(priv_zoe_a[0].id, revive.id);
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
