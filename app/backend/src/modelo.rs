//! Tipos del dominio compartidos por todo el backend. Los textos son exactamente
//! los que usa el frontend (`app/frontend/src/types.ts`) y los que se guardan en SQLite.

/// Declara un enum cuyo valor viaja como texto (JSON y SQLite).
macro_rules! enum_texto {
    ($(#[$doc:meta])* $nombre:ident { $($variante:ident => $texto:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum $nombre {
            $(#[serde(rename = $texto)] $variante),+
        }

        impl $nombre {
            /// Todos los valores, en el orden en que se declaran.
            pub const TODOS: &'static [$nombre] = &[$($nombre::$variante),+];

            pub fn como_texto(self) -> &'static str {
                match self { $($nombre::$variante => $texto),+ }
            }

            pub fn desde_texto(texto: &str) -> Option<Self> {
                match texto { $($texto => Some($nombre::$variante),)+ _ => None }
            }

            /// Lista para un `CHECK (columna IN (...))` de SQLite.
            pub fn lista_sql() -> String {
                [$($texto),+].iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(", ")
            }
        }

        impl std::fmt::Display for $nombre {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.como_texto())
            }
        }

        impl rusqlite::types::ToSql for $nombre {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.como_texto()))
            }
        }

        impl rusqlite::types::FromSql for $nombre {
            fn column_result(v: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                let texto = v.as_str()?;
                Self::desde_texto(texto).ok_or_else(|| {
                    rusqlite::types::FromSqlError::Other(
                        format!("valor desconocido para {}: {texto}", stringify!($nombre)).into(),
                    )
                })
            }
        }
    };
}
#[allow(unused_imports)]
pub(crate) use enum_texto;

enum_texto! {
    /// Rol de una persona en el proyecto (ver SEGURIDAD.md).
    Rol {
        Administrador => "Administrador",
        Lider => "Líder",
        Desarrollador => "Desarrollador",
        Tester => "Tester",
        ColaboradorExterno => "Colaborador externo",
    }
}

impl Rol {
    pub fn es_lider_o_admin(self) -> bool {
        matches!(self, Rol::Administrador | Rol::Lider)
    }
}

/// Fecha y hora actual en UTC, formato ISO 8601 (`2026-11-09T20:51:55Z`).
pub fn ahora_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Fecha local de hoy (`AAAA-MM-DD`), la que usa el equipo en tareas y sprints.
pub fn hoy_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Los correos se comparan sin distinguir mayúsculas: se guardan en minúsculas.
pub fn normalizar_email(email: &str) -> String {
    email.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rol_viaja_con_el_texto_del_frontend() {
        assert_eq!(serde_json::to_string(&Rol::Lider).unwrap(), "\"Líder\"");
        assert_eq!(
            serde_json::from_str::<Rol>("\"Colaborador externo\"").unwrap(),
            Rol::ColaboradorExterno
        );
        assert_eq!(Rol::desde_texto("Jefe"), None);
        assert_eq!(Rol::TODOS.len(), 5);
    }

    #[test]
    fn email_se_normaliza() {
        assert_eq!(
            normalizar_email("  Ana.Lopez@VirusTF.dev "),
            "ana.lopez@virustf.dev"
        );
    }
}
