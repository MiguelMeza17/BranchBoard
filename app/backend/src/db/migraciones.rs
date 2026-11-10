//! Migraciones del esquema. Cada entrada de `migraciones()` lleva el `.db` una
//! versión más arriba; la versión actual se guarda en `PRAGMA user_version`.
//!
//! Reglas: **nunca** editar una migración ya publicada; los cambios van en una
//! entrada nueva al final (y se sube `VERSION_ESQUEMA`).

use rusqlite::Connection;

use crate::error::{Error, Resultado};
use crate::modelo::{EstadoTarea, Rol, TipoNotificacion, TipoTarea};

/// Versión del esquema a la que queda un `.db` recién abierto.
pub const VERSION_ESQUEMA: u32 = 5;

/// Una migración por versión, en orden. La posición `i` lleva de la versión `i` a la `i + 1`.
fn migraciones() -> Vec<String> {
    vec![
        // 1 — datos internos de la base (clave/valor).
        "CREATE TABLE meta (
            clave TEXT PRIMARY KEY,
            valor TEXT NOT NULL
        ) STRICT;"
            .to_string(),
        // 2 — usuarios y configuración del proyecto.
        format!(
            "CREATE TABLE usuarios (
                email         TEXT PRIMARY KEY,
                nombre        TEXT NOT NULL,
                apellido      TEXT NOT NULL DEFAULT '',
                rol           TEXT NOT NULL CHECK (rol IN ({roles})),
                fecha_alta    TEXT NOT NULL,
                firma         TEXT,
                firmado_en    TEXT,
                firmado_por   TEXT,
                llave_publica TEXT,
                certificado   TEXT,
                CHECK (email = lower(email))
            ) STRICT;
            CREATE TABLE configuracion_proyecto (
                clave               TEXT PRIMARY KEY,
                valor               TEXT NOT NULL,
                actualizado_por     TEXT NOT NULL,
                fecha_actualizacion TEXT NOT NULL
            ) STRICT;",
            roles = Rol::lista_sql()
        ),
        // 3 — tareas y ediciones pendientes.
        format!(
            "CREATE TABLE tareas (
                id                        TEXT PRIMARY KEY,
                titulo                    TEXT NOT NULL,
                detalles                  TEXT NOT NULL DEFAULT '',
                tipo                      TEXT NOT NULL CHECK (tipo IN ({tipos})),
                tarea_padre_id            TEXT,
                archivo_ruta              TEXT,
                estado                    TEXT NOT NULL CHECK (estado IN ({estados})),
                pendiente_aprobacion      INTEGER NOT NULL DEFAULT 0,
                rama_asociada             TEXT,
                modo_trabajo              TEXT CHECK (modo_trabajo IS NULL OR modo_trabajo = 'rama'),
                modificada                INTEGER NOT NULL DEFAULT 0,
                ultimo_changeset_visto    INTEGER,
                creado_por                TEXT NOT NULL,
                fecha_creacion            TEXT NOT NULL,
                asignado_a                TEXT,
                lider_revision_asignado   TEXT,
                sprint_id                 TEXT,
                backlog_id                TEXT,
                con_fecha                 INTEGER NOT NULL DEFAULT 0,
                fecha_limite              TEXT,
                fecha_completada          TEXT,
                eliminada                 INTEGER NOT NULL DEFAULT 0,
                justificacion_eliminacion TEXT,
                fecha_eliminacion         TEXT,
                motivo_rechazo            TEXT,
                reporte_testing_origen_id TEXT,
                tarea_origen_id           TEXT,
                reaperturas               INTEGER NOT NULL DEFAULT 0
            ) STRICT;
            CREATE INDEX tareas_por_padre ON tareas (tarea_padre_id);
            CREATE INDEX tareas_por_responsable ON tareas (asignado_a);
            CREATE INDEX tareas_por_sprint ON tareas (sprint_id);
            CREATE INDEX tareas_por_backlog ON tareas (backlog_id);
            CREATE TABLE ediciones_pendientes (
                id                TEXT PRIMARY KEY,
                tarea_id          TEXT NOT NULL UNIQUE REFERENCES tareas (id) ON DELETE CASCADE,
                campos_nuevos     TEXT NOT NULL,
                campos_anteriores TEXT NOT NULL,
                motivo            TEXT,
                creado_por        TEXT NOT NULL,
                fecha             TEXT NOT NULL
            ) STRICT;",
            tipos = TipoTarea::lista_sql(),
            estados = EstadoTarea::lista_sql()
        ),
        // 4 — sprints, backlogs y la copia de tareas de cada reporte.
        "CREATE TABLE sprints (
            id                TEXT PRIMARY KEY,
            nombre            TEXT NOT NULL,
            descripcion       TEXT NOT NULL DEFAULT '',
            con_fecha         INTEGER NOT NULL,
            fecha_inicio      TEXT NOT NULL,
            duracion_dias     INTEGER,
            fecha_fin         TEXT,
            cerrado           INTEGER NOT NULL DEFAULT 0,
            fecha_cierre_real TEXT,
            creado_por        TEXT NOT NULL,
            CHECK ((con_fecha = 1 AND duracion_dias > 0 AND fecha_fin IS NOT NULL)
                OR (con_fecha = 0 AND duracion_dias IS NULL AND fecha_fin IS NULL))
        ) STRICT;
        CREATE TABLE reportes_sprint (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            sprint_id TEXT NOT NULL REFERENCES sprints (id) ON DELETE CASCADE,
            fecha     TEXT NOT NULL,
            de_cierre INTEGER NOT NULL DEFAULT 0,
            tareas    TEXT NOT NULL
        ) STRICT;
        CREATE INDEX reportes_por_sprint ON reportes_sprint (sprint_id);"
            .to_string(),
        // 5 — commits y préstamos, notificaciones, documentos, chat y las tablas
        // locales por usuario (leídos del chat, chats eliminados, huellas vistas).
        format!(
            "CREATE TABLE commits (
                id          TEXT PRIMARY KEY,
                tarea_id    TEXT NOT NULL REFERENCES tareas (id) ON DELETE CASCADE,
                hash        TEXT NOT NULL,
                mensaje     TEXT NOT NULL,
                descripcion TEXT,
                tipo        TEXT CHECK (tipo IS NULL OR tipo IN ({tipos})),
                autor       TEXT NOT NULL,
                fecha       TEXT NOT NULL,
                UNIQUE (tarea_id, hash)
            ) STRICT;
            CREATE TABLE cambios_prestados (
                id               TEXT PRIMARY KEY,
                tarea_id         TEXT NOT NULL REFERENCES tareas (id) ON DELETE CASCADE,
                origen_id        TEXT NOT NULL,
                hash             TEXT,
                autor            TEXT NOT NULL,
                fecha            TEXT NOT NULL,
                archivos         TEXT NOT NULL,
                archivos_tocados TEXT NOT NULL DEFAULT '[]',
                quitado_en       TEXT,
                quitado_por      TEXT,
                CHECK ((quitado_en IS NULL) = (quitado_por IS NULL))
            ) STRICT;
            CREATE INDEX prestamos_por_tarea ON cambios_prestados (tarea_id);
            CREATE TABLE notificaciones (
                id         TEXT PRIMARY KEY,
                tipo       TEXT NOT NULL CHECK (tipo IN ({tipos_notificacion})),
                para       TEXT NOT NULL,
                titulo     TEXT NOT NULL,
                cuerpo     TEXT NOT NULL DEFAULT '',
                leida      INTEGER NOT NULL DEFAULT 0,
                fecha      TEXT NOT NULL,
                tarea_id   TEXT,
                accionable INTEGER NOT NULL DEFAULT 0
            ) STRICT;
            CREATE INDEX notificaciones_por_destinatario ON notificaciones (para);
            CREATE TABLE documentos (
                id         TEXT PRIMARY KEY,
                nombre     TEXT NOT NULL,
                tamano     INTEGER NOT NULL CHECK (tamano >= 0),
                subido_por TEXT NOT NULL,
                fecha      TEXT NOT NULL,
                pendiente  INTEGER NOT NULL DEFAULT 0,
                ruta       TEXT NOT NULL UNIQUE
            ) STRICT;
            CREATE TABLE grupos_chat (
                id         TEXT PRIMARY KEY,
                nombre     TEXT NOT NULL,
                miembros   TEXT NOT NULL,
                creado_por TEXT NOT NULL,
                fecha      TEXT NOT NULL
            ) STRICT;
            CREATE TABLE mensajes_chat (
                id    TEXT PRIMARY KEY,
                de    TEXT NOT NULL,
                para  TEXT NOT NULL,
                texto TEXT NOT NULL,
                fecha TEXT NOT NULL
            ) STRICT;
            CREATE INDEX mensajes_por_destino ON mensajes_chat (para);
            CREATE INDEX mensajes_por_autor ON mensajes_chat (de);
            CREATE TABLE chat_leidos (
                mensaje_id TEXT NOT NULL REFERENCES mensajes_chat (id) ON DELETE CASCADE,
                usuario    TEXT NOT NULL,
                PRIMARY KEY (mensaje_id, usuario)
            ) STRICT;
            CREATE TABLE chats_eliminados (
                usuario      TEXT NOT NULL,
                conversacion TEXT NOT NULL,
                desde        TEXT NOT NULL,
                PRIMARY KEY (usuario, conversacion)
            ) STRICT;
            CREATE TABLE paneles_vistos (
                usuario TEXT NOT NULL,
                panel   TEXT NOT NULL,
                huella  TEXT NOT NULL,
                PRIMARY KEY (usuario, panel)
            ) STRICT;",
            tipos = TipoTarea::lista_sql(),
            tipos_notificacion = TipoNotificacion::lista_sql()
        ),
    ]
}

pub fn version(conexion: &Connection) -> rusqlite::Result<u32> {
    conexion.pragma_query_value(None, "user_version", |f| f.get(0))
}

/// Aplica las migraciones que le falten al `.db`. Cada una va en su propia
/// transacción: si falla, el `.db` queda en la versión anterior, intacto.
pub fn aplicar(conexion: &mut Connection) -> Resultado<()> {
    let actual = version(conexion)?;
    if actual > VERSION_ESQUEMA {
        return Err(Error::BaseDatos(format!(
            "La base de datos es de una versión más nueva de BranchBoard (esquema {actual}, esta app maneja hasta {VERSION_ESQUEMA}). Actualiza la app."
        )));
    }
    for (i, sql) in migraciones().iter().enumerate().skip(actual as usize) {
        let tx = conexion.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as u32)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_version_coincide_con_la_lista() {
        assert_eq!(migraciones().len() as u32, VERSION_ESQUEMA);
    }

    #[test]
    fn rechaza_un_db_de_una_version_mas_nueva() {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "user_version", VERSION_ESQUEMA + 1)
            .unwrap();
        assert!(aplicar(&mut c).is_err());
    }

    #[test]
    fn aplicar_dos_veces_no_falla() {
        let mut c = Connection::open_in_memory().unwrap();
        aplicar(&mut c).unwrap();
        aplicar(&mut c).unwrap();
        assert_eq!(version(&c).unwrap(), VERSION_ESQUEMA);
    }
}
