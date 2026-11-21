//! Archivos del proyecto en Supabase Storage (#242): documentos e imagen del proyecto.
//!
//! Un solo bucket privado, `proyectos`, con una carpeta por proyecto (migración
//! `20261121020000_storage_documentos.sql`):
//!
//! - `<proyecto_id>/documentos/D-<uuid>[.ext]`
//! - `<proyecto_id>/proyecto/icono-<uuid>.png`
//!
//! Quién puede qué lo deciden las reglas de Storage (las mismas que `identidad/permisos.rs`). Aquí solo se
//! arma la ruta (sin `/` ni `..` en el nombre), se revisa el límite de 50 MB **antes** de subir y se
//! traducen los errores. Usa la sesión de [`ClienteSupabase`]: si el token resulta vencido, renueva y
//! reintenta una vez. Los tokens nunca salen en errores ni en `Debug`.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use super::cliente::{ClienteSupabase, Sesion};
use super::entorno::ProyectoSupabase;
use crate::error::{Error, Resultado};
use crate::servidor::pat::Llavero;

/// Bucket de los archivos de todos los proyectos.
pub const BUCKET: &str = "proyectos";
/// Máximo por archivo del plan gratis (el bucket tiene el mismo límite).
pub const LIMITE_POR_ARCHIVO: u64 = 50 * 1024 * 1024;
/// Subir 50 MB por una conexión lenta tarda: más margen que una llamada normal.
const TIEMPO_POR_ARCHIVO: Duration = Duration::from_secs(300);

/// Carpeta de un proyecto dentro del bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carpeta {
    Documentos,
    Proyecto,
}

impl Carpeta {
    fn nombre(self) -> &'static str {
        match self {
            Carpeta::Documentos => "documentos",
            Carpeta::Proyecto => "proyecto",
        }
    }
}

/// `<proyecto_id>/<carpeta>/<archivo>`, validando que el id sea un uuid y que el archivo sea un nombre
/// simple (ni vacío, ni `.`/`..`, ni con separadores de ruta o caracteres raros).
pub fn ruta_objeto(proyecto_id: &str, carpeta: Carpeta, archivo: &str) -> Resultado<String> {
    let id = proyecto_id.trim().to_ascii_lowercase();
    if !es_uuid(&id) {
        return Err(Error::Validacion(
            "El id del proyecto en el servidor no es válido".into(),
        ));
    }
    let nombre_valido = !archivo.is_empty()
        && archivo != "."
        && archivo != ".."
        && archivo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !nombre_valido {
        return Err(Error::Validacion(
            "El nombre interno del archivo no es válido".into(),
        ));
    }
    Ok(format!("{id}/{}/{archivo}", carpeta.nombre()))
}

/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` en hexadecimal (como lo revisa la regla de Storage).
fn es_uuid(texto: &str) -> bool {
    texto.len() == 36
        && texto.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Rechaza, antes de subir nada, un archivo de más de 50 MB (decisión de #242: sin partir archivos).
pub fn revisar_tamano(nombre_visible: &str, tamano: u64) -> Resultado<()> {
    if tamano > LIMITE_POR_ARCHIVO {
        return Err(Error::Validacion(format!(
            "\"{nombre_visible}\" pesa más de {} MB, el máximo por archivo del servidor",
            LIMITE_POR_ARCHIVO / (1024 * 1024)
        )));
    }
    Ok(())
}

/// Tipo MIME por extensión, solo para que el servidor lo guarde; el contenido no se interpreta.
fn tipo_contenido(ruta: &str) -> &'static str {
    let ext = ruta.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Errores de la API de Storage. Responde casi siempre HTTP 400 con el código de verdad en el cuerpo
/// (`{"statusCode":"…","error":"…","message":"…","code":"…"}`); respuestas reales de branchboard-pruebas
/// (2026-11-21) en las pruebas de abajo.
fn error_storage(status: u16, cuerpo: &str) -> Error {
    // Proyecto pausado (#246): mismo código en Storage que en el resto de Supabase.
    if status == super::cliente::HTTP_PROYECTO_PAUSADO {
        return Error::ServidorPausado;
    }
    let json: Value = serde_json::from_str(cuerpo).unwrap_or(Value::Null);
    let texto = |c: &str| json.get(c).and_then(Value::as_str).unwrap_or_default();
    let mensaje = texto("message");
    let m = mensaje.to_ascii_lowercase();
    match texto("code") {
        // Con un token que no es suyo (firma falsa) Storage no da 401: no ve el bucket (que sí existe).
        // Igual que un token vencido, se trata como sesión a renovar.
        "NoSuchBucket" | "InvalidJWT" => return Error::SinSesionServidor,
        "AccessDenied" => return Error::SinPermiso,
        "NoSuchKey" => return Error::NoEncontrado("el archivo en el servidor".into()),
        "KeyAlreadyExists" => {
            return Error::Validacion("Ese archivo ya existe en el servidor".into())
        }
        "EntityTooLarge" => return demasiado_grande(),
        _ => {}
    }
    if status == 401 || m.contains("jwt") || (m.contains("exp") && m.contains("claim")) {
        return Error::SinSesionServidor;
    }
    if status == 403 || m.contains("row-level security") {
        return Error::SinPermiso;
    }
    if status == 413 || texto("statusCode") == "413" || m.contains("maximum allowed size") {
        return demasiado_grande();
    }
    let detalle = if mensaje.is_empty() {
        "sin detalle"
    } else {
        mensaje
    };
    Error::Servidor(format!("HTTP {status}: {detalle}"))
}

fn demasiado_grande() -> Error {
    Error::Validacion(format!(
        "El archivo pesa más de {} MB, el máximo por archivo del servidor",
        LIMITE_POR_ARCHIVO / (1024 * 1024)
    ))
}

/// Acceso a Storage con la sesión del cliente.
pub struct Storage<'a, L: Llavero> {
    cliente: &'a ClienteSupabase<L>,
    proyecto: &'static ProyectoSupabase,
    agente: ureq::Agent,
}

impl<'a, L: Llavero> Storage<'a, L> {
    pub fn new(cliente: &'a ClienteSupabase<L>, proyecto: &'static ProyectoSupabase) -> Self {
        Self {
            cliente,
            proyecto,
            agente: ureq::AgentBuilder::new()
                .timeout(TIEMPO_POR_ARCHIVO)
                .build(),
        }
    }

    fn url(&self, ruta: &str) -> String {
        format!("{}/storage/v1/object/{BUCKET}/{ruta}", self.proyecto.url)
    }

    /// Hace la petición con la sesión vigente; si da "sin sesión" (token vencido), renueva y reintenta.
    fn con_sesion<T>(&self, hacer: impl Fn(&Sesion) -> Resultado<T>) -> Resultado<T> {
        let sesion = self.cliente.sesion_vigente()?;
        match hacer(&sesion) {
            Err(Error::SinSesionServidor) => {
                let renovada = self.cliente.renovar(&sesion)?;
                // Con la sesión recién renovada ya no puede ser el token: es otra cosa del servidor.
                hacer(&renovada).map_err(|e| match e {
                    Error::SinSesionServidor => {
                        Error::Servidor("Storage no reconoce la sesión ni el bucket".into())
                    }
                    otro => otro,
                })
            }
            otro => otro,
        }
    }

    fn peticion(&self, metodo: &str, ruta: &str, sesion: &Sesion) -> ureq::Request {
        self.agente
            .request(metodo, &self.url(ruta))
            .set("apikey", self.proyecto.clave_publica)
            .set("Authorization", &format!("Bearer {}", sesion.access_token))
    }

    /// Sube `bytes` a `ruta` (de [`ruta_objeto`]). Nunca sobrescribe: si ya existe, es un error.
    pub fn subir(&self, ruta: &str, bytes: &[u8]) -> Resultado<()> {
        revisar_tamano(ruta, bytes.len() as u64)?;
        self.con_sesion(|s| {
            let respuesta = self
                .peticion("POST", ruta, s)
                .set("Content-Type", tipo_contenido(ruta))
                .set("x-upsert", "false")
                .send_bytes(bytes);
            leer(respuesta).map(|_| ())
        })
    }

    /// Baja el archivo de `ruta`.
    pub fn bajar(&self, ruta: &str) -> Resultado<Vec<u8>> {
        // Storage puede servir desde la CDN un archivo recién borrado o un permiso anterior.
        // Cada descarga debe comprobar el estado y los permisos actuales en el origen.
        self.con_sesion(|s| {
            leer(
                self.peticion("GET", ruta, s)
                    .query("cacheNonce", &crate::eventos::uuid())
                    .call(),
            )
        })
    }

    /// Borra el archivo de `ruta`. Storage responde 200 aunque las reglas no dejen borrar (no hay fila
    /// visible que borrar), así que se comprueba en la respuesta que de verdad se borró.
    pub fn borrar(&self, ruta: &str) -> Resultado<()> {
        let prefijo = format!("{}/storage/v1/object/{BUCKET}", self.proyecto.url);
        self.con_sesion(|s| {
            let respuesta = self
                .agente
                .request("DELETE", &prefijo)
                .set("apikey", self.proyecto.clave_publica)
                .set("Authorization", &format!("Bearer {}", s.access_token))
                .send_json(serde_json::json!({ "prefixes": [ruta] }));
            let cuerpo = leer(respuesta)?;
            let borrados: Value = serde_json::from_slice(&cuerpo)
                .map_err(|_| Error::Servidor("respuesta que la app no entiende".into()))?;
            if borrados.as_array().is_some_and(|a| !a.is_empty()) {
                Ok(())
            } else {
                Err(Error::SinPermiso)
            }
        })
    }
}

/// El cuerpo de la respuesta, o el error de Storage traducido.
fn leer(respuesta: Result<ureq::Response, ureq::Error>) -> Resultado<Vec<u8>> {
    match respuesta {
        Ok(r) => {
            let mut bytes = Vec::new();
            r.into_reader()
                .take(LIMITE_POR_ARCHIVO + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::ServidorSinConexion)?;
            Ok(bytes)
        }
        Err(ureq::Error::Status(status, r)) => {
            let cuerpo = r.into_string().unwrap_or_default();
            Err(error_storage(status, &cuerpo))
        }
        Err(ureq::Error::Transport(_)) => Err(Error::ServidorSinConexion),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn la_ruta_va_dentro_de_la_carpeta_del_proyecto() {
        assert_eq!(
            ruta_objeto(P, Carpeta::Documentos, "D-abc.pdf").unwrap(),
            format!("{P}/documentos/D-abc.pdf")
        );
        assert_eq!(
            ruta_objeto(&P.to_uppercase(), Carpeta::Proyecto, "icono-1.png").unwrap(),
            format!("{P}/proyecto/icono-1.png")
        );
    }

    #[test]
    fn la_ruta_rechaza_ids_y_nombres_raros() {
        for id in ["", "no-es-uuid", "../x", &format!("{P}/..")] {
            assert!(ruta_objeto(id, Carpeta::Documentos, "D-a").is_err(), "{id}");
        }
        for nombre in [
            "", ".", "..", "a/b", "a\\b", "../D-a", "D a.pdf", "D-ñ.pdf", "%2e%2e",
        ] {
            assert!(
                ruta_objeto(P, Carpeta::Documentos, nombre).is_err(),
                "{nombre}"
            );
        }
    }

    #[test]
    fn mas_de_50_mb_se_rechaza_antes_de_subir() {
        assert!(revisar_tamano("a.fbx", LIMITE_POR_ARCHIVO).is_ok());
        let e = revisar_tamano("modelo.fbx", LIMITE_POR_ARCHIVO + 1).unwrap_err();
        assert!(
            matches!(&e, Error::Validacion(m) if m.contains("modelo.fbx") && m.contains("50 MB")),
            "{e:?}"
        );
    }

    #[test]
    fn con_el_proyecto_pausado_storage_avisa_igual() {
        assert!(matches!(error_storage(540, ""), Error::ServidorPausado));
    }

    #[test]
    fn errores_de_storage() {
        // Respuestas reales de branchboard-pruebas (2026-11-21), todas con HTTP 400.
        let rls = r#"{"statusCode":"403","error":"Unauthorized","message":"new row violates row-level security policy","code":"AccessDenied"}"#;
        assert!(matches!(error_storage(400, rls), Error::SinPermiso));
        let repetido = r#"{"statusCode":"409","error":"Duplicate","message":"The resource already exists","code":"KeyAlreadyExists"}"#;
        assert!(matches!(error_storage(400, repetido), Error::Validacion(_)));
        let no_esta = r#"{"statusCode":"404","error":"not_found","message":"Object not found","code":"NoSuchKey"}"#;
        assert!(matches!(
            error_storage(400, no_esta),
            Error::NoEncontrado(_)
        ));
        let firma_falsa = r#"{"statusCode":"404","error":"Bucket not found","message":"Bucket not found","code":"NoSuchBucket"}"#;
        assert!(matches!(
            error_storage(400, firma_falsa),
            Error::SinSesionServidor
        ));
        // De la documentación de Storage (no se pudieron provocar sin esperar una hora o subir 50 MB).
        let vencido = r#"{"statusCode":"403","error":"Unauthorized","message":"\"exp\" claim timestamp check failed"}"#;
        assert!(matches!(
            error_storage(400, vencido),
            Error::SinSesionServidor
        ));
        let grande = r#"{"statusCode":"413","error":"Payload too large","message":"The object exceeded the maximum allowed size","code":"EntityTooLarge"}"#;
        assert!(matches!(error_storage(400, grande), Error::Validacion(m) if m.contains("50 MB")));
        assert!(
            matches!(error_storage(500, "no es json"), Error::Servidor(m) if m == "HTTP 500: sin detalle")
        );
    }

    #[test]
    fn tipo_de_contenido_por_extension() {
        assert_eq!(tipo_contenido("p/proyecto/icono-1.png"), "image/png");
        assert_eq!(tipo_contenido("p/documentos/D-1.PDF"), "application/pdf");
        assert_eq!(
            tipo_contenido("p/documentos/D-1"),
            "application/octet-stream"
        );
        assert_eq!(
            tipo_contenido("p/documentos/D-1.fbx"),
            "application/octet-stream"
        );
    }

    /// Llavero en memoria que la prueba también puede tocar (para dañar el token guardado).
    #[derive(Default, Clone)]
    struct LlaveroMemoria(
        std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>>,
    );

    impl Llavero for LlaveroMemoria {
        fn leer(&self, cuenta: &str) -> Resultado<Option<String>> {
            Ok(self.0.borrow().get(cuenta).cloned())
        }
        fn guardar(&self, cuenta: &str, valor: &str) -> Resultado<()> {
            self.0.borrow_mut().insert(cuenta.into(), valor.into());
            Ok(())
        }
        fn borrar(&self, cuenta: &str) -> Resultado<()> {
            self.0.borrow_mut().remove(cuenta);
            Ok(())
        }
    }

    /// **Prueba en el entorno real** (#242) contra **branchboard-pruebas**, por la API HTTP de Storage:
    /// A (Administrador) crea un proyecto y agrega a B (Desarrollador); C no es miembro. Comprueba que B
    /// sube un documento pero no la imagen ni borra; que C no ve ni sube nada; que A baja, cambia la imagen
    /// y borra; que nada se sobrescribe; y que un token de acceso dañado se renueva y la llamada se repite.
    ///
    /// Necesita tres cuentas confirmadas `storage-a|b|c@prueba.branchboard` con la clave de
    /// `BB_PRUEBA_SUPABASE_CLAVE` (cómo crearlas y borrarlas, en "Storage (#242)" de BACKEND.md). Borra sus
    /// archivos; el proyecto `bb-prueba-242` se borra con las cuentas.
    #[test]
    #[ignore = "usa branchboard-pruebas de verdad: correr a mano con BB_PRUEBA_SUPABASE_CLAVE"]
    fn entorno_real_storage_documentos_e_imagen() {
        use crate::supabase::entorno::PRUEBAS;

        let clave =
            std::env::var("BB_PRUEBA_SUPABASE_CLAVE").expect("falta BB_PRUEBA_SUPABASE_CLAVE");
        let llavero_a = LlaveroMemoria::default();
        let entrar = |quien: &str, llavero: LlaveroMemoria| {
            let c = ClienteSupabase::new(&PRUEBAS, llavero);
            c.iniciar_sesion(&format!("storage-{quien}@prueba.branchboard"), &clave)
                .unwrap();
            c
        };
        let a = entrar("a", llavero_a.clone());
        let b = entrar("b", LlaveroMemoria::default());
        let c = entrar("c", LlaveroMemoria::default());
        let (sa, sb, sc) = (
            Storage::new(&a, &PRUEBAS),
            Storage::new(&b, &PRUEBAS),
            Storage::new(&c, &PRUEBAS),
        );

        let p = a
            .llamar(
                "POST",
                "/rest/v1/rpc/crear_proyecto",
                Some(&serde_json::json!({ "p_nombre": "bb-prueba-242" })),
            )
            .unwrap();
        let p = p.as_str().unwrap().to_string();
        a.llamar(
            "POST",
            "/rest/v1/rpc/agregar_miembro",
            Some(&serde_json::json!({
                "p_proyecto": p, "p_correo": "storage-b@prueba.branchboard", "p_rol": "Desarrollador"
            })),
        )
        .unwrap();

        let sufijo = chrono::Utc::now().format("%H%M%S").to_string();
        let doc = ruta_objeto(&p, Carpeta::Documentos, &format!("D-{sufijo}.txt")).unwrap();
        let icono = ruta_objeto(&p, Carpeta::Proyecto, &format!("icono-{sufijo}.png")).unwrap();
        let icono_b = ruta_objeto(&p, Carpeta::Proyecto, &format!("icono-b{sufijo}.png")).unwrap();

        let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // B (Desarrollador) sube un documento; no cambia la imagen ni borra.
            sb.subir(&doc, b"GDD de prueba").unwrap();
            let e = sb.subir(&icono_b, b"png").unwrap_err();
            println!("B sube la imagen -> {e:?}");
            assert!(matches!(e, Error::SinPermiso));
            let e = sb.borrar(&doc).unwrap_err();
            println!("B borra un documento -> {e:?}");
            assert!(matches!(e, Error::SinPermiso));

            // C (de fuera) ni lo ve ni sube.
            let e = sc.bajar(&doc).unwrap_err();
            println!("C baja un documento de P -> {e:?}");
            assert!(matches!(e, Error::NoEncontrado(_) | Error::SinPermiso));
            let e = sc
                .subir(
                    &ruta_objeto(&p, Carpeta::Documentos, "D-intruso.txt").unwrap(),
                    b"x",
                )
                .unwrap_err();
            println!("C sube a P -> {e:?}");
            assert!(matches!(e, Error::SinPermiso));

            // A baja lo de B y pone la imagen; la misma ruta no se sobrescribe.
            assert_eq!(sa.bajar(&doc).unwrap(), b"GDD de prueba");
            sa.subir(&icono, b"\x89PNG imagen").unwrap();
            let e = sa.subir(&icono, b"otra").unwrap_err();
            println!("A sube otra vez la misma ruta -> {e:?}");
            assert!(matches!(e, Error::Validacion(_)));
            assert_eq!(sb.bajar(&icono).unwrap(), b"\x89PNG imagen");

            // Token de acceso dañado (como uno revocado): Storage lo rechaza, se renueva y se repite.
            let mut danada = a.sesion_guardada().unwrap().unwrap();
            let (sin_firma, _) = danada.access_token.rsplit_once('.').unwrap();
            danada.access_token = format!("{sin_firma}.firmaFalsa");
            // La cuenta del llavero es `supabase-<proyecto>` (cliente.rs).
            llavero_a
                .guardar(
                    &format!("supabase-{}", PRUEBAS.nombre),
                    &serde_json::to_string(&danada).unwrap(),
                )
                .unwrap();
            assert_eq!(sa.bajar(&doc).unwrap(), b"GDD de prueba");
            assert!(!a
                .sesion_guardada()
                .unwrap()
                .unwrap()
                .access_token
                .ends_with(".firmaFalsa"));

            // N3 (#244): retirar a B revoca también una descarga que antes pudo hacer.
            assert_eq!(sb.bajar(&doc).unwrap(), b"GDD de prueba");
            a.llamar(
                "POST",
                "/rest/v1/rpc/quitar_miembro",
                Some(&serde_json::json!({
                    "p_proyecto": p,
                    "p_usuario": b.sesion_guardada().unwrap().unwrap().usuario_id,
                })),
            )
            .unwrap();
            assert!(matches!(
                sb.bajar(&doc),
                Err(Error::NoEncontrado(_) | Error::SinPermiso)
            ));
            assert!(matches!(sb.subir(&icono_b, b"png"), Err(Error::SinPermiso)));

            // A borra; ya no está.
            sa.borrar(&doc).unwrap();
            sa.borrar(&icono).unwrap();
            let e = sa.bajar(&doc).unwrap_err();
            println!("A baja un documento borrado -> {e:?}");
            assert!(matches!(e, Error::NoEncontrado(_)));
        }));
        let _ = sa.borrar(&doc);
        let _ = sa.borrar(&icono);
        for c in [&a, &b, &c] {
            let _ = c.cerrar_sesion();
        }
        if let Err(e) = resultado {
            std::panic::resume_unwind(e);
        }
    }
}
