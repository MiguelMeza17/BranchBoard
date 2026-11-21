//! Cliente del servidor de BranchBoard en Supabase (#235).
//!
//! Toda la red la hace el proceso Rust (SEGURIDAD.md): la webview no habla con internet y la CSP sigue
//! siendo `connect-src ipc: http://ipc.localhost`. Este cliente cubre Auth (iniciar, renovar y cerrar
//! sesión) y las llamadas autenticadas a la API de datos (PostgREST) y a las funciones (`/rest/v1/rpc/…`).
//!
//! La sesión (token de acceso y de renovación) se guarda en el **llavero del sistema**, nunca en SQLite ni
//! en archivos, y nunca se imprime. Sin internet, la sesión guardada se conserva: la app sigue trabajando
//! sobre SQLite y los cambios esperan (#238).
//!
//! Respuestas reales de Supabase comprobadas en branchboard-pruebas (2026-11-21):
//! - contraseña incorrecta → `400 {"error_code":"invalid_credentials"}`
//! - token de renovación inválido → `400 {"error_code":"validation_failed"}`
//! - token de acceso falso en la API de datos → `401 {"code":"PGRST301"}`
//! - función sin permiso → `42501`; regla de negocio de nuestras funciones → `P0001` con el mensaje.

use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::entorno::ProyectoSupabase;
use crate::error::{Error, Resultado};
use crate::servidor::pat::Llavero;

const TIEMPO_POR_PETICION: Duration = Duration::from_secs(20);
/// Código propio de Supabase para "project paused" (docs: Troubleshooting → HTTP status codes, 54X).
pub(crate) const HTTP_PROYECTO_PAUSADO: u16 = 540;
/// La sesión se renueva si le queda menos que esto (segundos).
const MARGEN_RENOVAR: i64 = 60;

/// Sesión de una cuenta en el servidor. `Debug` no muestra los tokens.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sesion {
    pub access_token: String,
    pub refresh_token: String,
    /// Segundos Unix en que vence el token de acceso.
    pub expira_en: i64,
    /// Id de la cuenta en Supabase Auth (`auth.uid()` en las reglas).
    pub usuario_id: String,
    pub correo: String,
    /// Nombre y apellido con que se registró (metadatos de Auth, #236).
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub apellido: String,
}

impl std::fmt::Debug for Sesion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sesion")
            .field("access_token", &"<oculto>")
            .field("refresh_token", &"<oculto>")
            .field("expira_en", &self.expira_en)
            .field("usuario_id", &self.usuario_id)
            .field("correo", &self.correo)
            .field("nombre", &self.nombre)
            .field("apellido", &self.apellido)
            .finish()
    }
}

impl Sesion {
    fn necesita_renovar(&self, ahora: i64) -> bool {
        self.expira_en - ahora < MARGEN_RENOVAR
    }
}

/// Respuesta de `/auth/v1/token`.
#[derive(Deserialize)]
struct RespuestaToken {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    expires_at: Option<i64>,
    user: UsuarioAuth,
}

#[derive(Deserialize)]
struct UsuarioAuth {
    id: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    user_metadata: Metadatos,
}

/// Lo que se manda como `data` al registrarse; el trigger de `perfiles` lo copia (#233).
#[derive(Deserialize, Default)]
struct Metadatos {
    #[serde(default)]
    nombre: String,
    #[serde(default)]
    apellido: String,
}

/// Respuesta de la Edge Function `login-unity` (#237).
#[derive(Deserialize)]
struct RespuestaLoginUnity {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[serde(default)]
    expires_at: Option<i64>,
    usuario: UsuarioLoginUnity,
}

#[derive(Deserialize)]
struct UsuarioLoginUnity {
    id: String,
    correo: String,
}

fn sesion_desde(r: RespuestaToken, ahora: i64) -> Sesion {
    Sesion {
        expira_en: r.expires_at.unwrap_or(ahora + r.expires_in),
        access_token: r.access_token,
        refresh_token: r.refresh_token,
        usuario_id: r.user.id,
        correo: r.user.email.unwrap_or_default().trim().to_lowercase(),
        nombre: r.user.user_metadata.nombre,
        apellido: r.user.user_metadata.apellido,
    }
}

pub struct ClienteSupabase<L: Llavero> {
    proyecto: &'static ProyectoSupabase,
    llavero: L,
    agente: ureq::Agent,
}

impl<L: Llavero> ClienteSupabase<L> {
    pub fn new(proyecto: &'static ProyectoSupabase, llavero: L) -> Self {
        Self {
            proyecto,
            llavero,
            agente: ureq::AgentBuilder::new()
                .timeout(TIEMPO_POR_PETICION)
                .build(),
        }
    }

    /// Cuenta del llavero: una por proyecto, para que pruebas y producción no se pisen.
    fn cuenta_llavero(&self) -> String {
        format!("supabase-{}", self.proyecto.nombre)
    }

    fn url(&self, ruta: &str) -> String {
        format!("{}{}", self.proyecto.url, ruta)
    }

    /// La sesión guardada, sin tocar la red (sirve para arrancar sin internet). Si lo guardado está dañado
    /// se borra y se trata como "sin sesión".
    pub fn sesion_guardada(&self) -> Resultado<Option<Sesion>> {
        let Some(texto) = self.llavero.leer_largo(&self.cuenta_llavero())? else {
            return Ok(None);
        };
        match serde_json::from_str(&texto) {
            Ok(sesion) => Ok(Some(sesion)),
            Err(_) => {
                self.llavero.borrar(&self.cuenta_llavero())?;
                Ok(None)
            }
        }
    }

    fn guardar(&self, sesion: &Sesion) -> Resultado<()> {
        let texto = serde_json::to_string(sesion).map_err(|e| Error::Interno(e.to_string()))?;
        self.llavero.guardar_largo(&self.cuenta_llavero(), &texto)
    }

    /// Inicia sesión con correo y contraseña y guarda la sesión en el llavero.
    pub fn iniciar_sesion(&self, correo: &str, clave: &str) -> Resultado<Sesion> {
        let respuesta = self
            .agente
            .post(&self.url("/auth/v1/token?grant_type=password"))
            .set("apikey", self.proyecto.clave_publica)
            .send_json(
                serde_json::json!({ "email": correo.trim().to_lowercase(), "password": clave }),
            );
        let token: RespuestaToken = leer_json(respuesta, error_auth)?;
        let sesion = sesion_desde(token, Utc::now().timestamp());
        self.guardar(&sesion)?;
        Ok(sesion)
    }

    /// Crea la cuenta y deja la sesión iniciada. Sin confirmación de correo (decidido en #236), el
    /// servidor devuelve la sesión en la misma respuesta.
    pub fn registrar(
        &self,
        nombre: &str,
        apellido: &str,
        correo: &str,
        clave: &str,
    ) -> Resultado<Sesion> {
        let respuesta = self
            .agente
            .post(&self.url("/auth/v1/signup"))
            .set("apikey", self.proyecto.clave_publica)
            .send_json(serde_json::json!({
                "email": correo.trim().to_lowercase(),
                "password": clave,
                "data": { "nombre": nombre.trim(), "apellido": apellido.trim() }
            }));
        let valor: Value = leer_json(respuesta, error_auth)?;
        // Si el proyecto volviera a pedir confirmar el correo, la respuesta no trae sesión.
        if valor.get("access_token").is_none() {
            return Err(Error::Validacion(
                "Confirma tu correo antes de iniciar sesión.".into(),
            ));
        }
        let token: RespuestaToken = serde_json::from_value(valor)
            .map_err(|_| Error::Servidor("respuesta que la app no entiende".into()))?;
        let sesion = sesion_desde(token, Utc::now().timestamp());
        self.guardar(&sesion)?;
        Ok(sesion)
    }

    /// "Iniciar sesión con Unity VC" (#237): la Edge Function `login-unity` le pregunta a Unity VC de quién
    /// es `token_unity` y devuelve la sesión de esa cuenta (la misma si el correo ya tenía cuenta). Se
    /// guarda en el llavero igual que la de correo y contraseña. `servidor` es el `webserver` de `org`.
    pub fn iniciar_sesion_con_unity(
        &self,
        token_unity: &str,
        org: &str,
        servidor: &str,
    ) -> Resultado<Sesion> {
        let respuesta = self
            .agente
            .post(&self.url("/functions/v1/login-unity"))
            .set("apikey", self.proyecto.clave_publica)
            .send_json(
                serde_json::json!({ "token": token_unity, "org": org, "servidor": servidor }),
            );
        let r: RespuestaLoginUnity = leer_json(respuesta, error_funcion)?;
        let sesion = sesion_desde(
            RespuestaToken {
                access_token: r.access_token,
                refresh_token: r.refresh_token,
                expires_in: r.expires_in,
                expires_at: r.expires_at,
                user: UsuarioAuth {
                    id: r.usuario.id,
                    email: Some(r.usuario.correo),
                    user_metadata: Metadatos::default(),
                },
            },
            Utc::now().timestamp(),
        );
        self.guardar(&sesion)?;
        Ok(sesion)
    }

    /// La sesión lista para usar: la guardada, renovada antes si está por vencer.
    pub fn sesion_vigente(&self) -> Resultado<Sesion> {
        self.sesion_vigente_en(Utc::now().timestamp())
    }

    fn sesion_vigente_en(&self, ahora: i64) -> Resultado<Sesion> {
        let sesion = self.sesion_guardada()?.ok_or(Error::SinSesionServidor)?;
        if sesion.necesita_renovar(ahora) {
            self.renovar(&sesion)
        } else {
            Ok(sesion)
        }
    }

    /// Pide un token de acceso nuevo con el de renovación. Si el servidor ya no lo acepta, borra la sesión
    /// (hay que volver a iniciar sesión); sin conexión la conserva.
    pub fn renovar(&self, sesion: &Sesion) -> Resultado<Sesion> {
        let respuesta = self
            .agente
            .post(&self.url("/auth/v1/token?grant_type=refresh_token"))
            .set("apikey", self.proyecto.clave_publica)
            .send_json(serde_json::json!({ "refresh_token": sesion.refresh_token }));
        match leer_json::<RespuestaToken>(respuesta, error_auth) {
            Ok(token) => {
                let nueva = sesion_desde(token, Utc::now().timestamp());
                self.guardar(&nueva)?;
                Ok(nueva)
            }
            Err(Error::ServidorSinConexion) => Err(Error::ServidorSinConexion),
            // Pausado no es "sesión inválida": la sesión sirve cuando lo reactiven (#246).
            Err(Error::ServidorPausado) => Err(Error::ServidorPausado),
            Err(Error::Servidor(m)) if m.starts_with("HTTP 5") => Err(Error::Servidor(m)),
            Err(_) => {
                self.llavero.borrar(&self.cuenta_llavero())?;
                Err(Error::SinSesionServidor)
            }
        }
    }

    /// Cierra la sesión: avisa al servidor si se puede (sin conexión no importa) y la borra del llavero.
    pub fn cerrar_sesion(&self) -> Resultado<()> {
        if let Some(sesion) = self.sesion_guardada()? {
            let _ = self
                .agente
                .post(&self.url("/auth/v1/logout"))
                .set("apikey", self.proyecto.clave_publica)
                .set("Authorization", &format!("Bearer {}", sesion.access_token))
                .call();
        }
        self.llavero.borrar(&self.cuenta_llavero())
    }

    /// Llamada autenticada a la API de datos o a una función (`ruta` empieza por `/rest/v1/`). Devuelve el
    /// JSON de la respuesta (`Null` si no trae cuerpo). Si el token resulta vencido (401), renueva y
    /// reintenta una vez.
    pub fn llamar(&self, metodo: &str, ruta: &str, cuerpo: Option<&Value>) -> Resultado<Value> {
        self.llamar_prefiriendo(metodo, ruta, cuerpo, None)
    }

    /// Sube eventos a la tabla `eventos` del servidor (#238). Los que ya estaban (mismo `id`) se ignoran,
    /// así un reintento después de una respuesta perdida no duplica ni falla.
    pub fn insertar_eventos(&self, filas: &Value) -> Resultado<()> {
        self.llamar_prefiriendo(
            "POST",
            "/rest/v1/eventos?on_conflict=id",
            Some(filas),
            Some("resolution=ignore-duplicates,return=minimal"),
        )
        .map(|_| ())
    }

    /// `POST` a la Edge Function `nombre` con la sesión de BranchBoard (#240, p. ej. `vincular-repo-unity`).
    /// Errores como los de [`error_funcion`]: un `401` es `SesionUnityVcCaducada` (lo usual: el token de
    /// Unity que va en el cuerpo venció); la sesión de BranchBoard ya se renovó antes si estaba por vencer.
    pub fn llamar_funcion(&self, nombre: &str, cuerpo: &Value) -> Resultado<Value> {
        let sesion = self.sesion_vigente()?;
        let respuesta = self
            .agente
            .post(&self.url(&format!("/functions/v1/{nombre}")))
            .set("apikey", self.proyecto.clave_publica)
            .set("Authorization", &format!("Bearer {}", sesion.access_token))
            .send_json(cuerpo.clone());
        leer_json(respuesta, error_funcion)
    }

    /// Como [`Self::llamar`], con la cabecera `Prefer` de PostgREST si se pide.
    fn llamar_prefiriendo(
        &self,
        metodo: &str,
        ruta: &str,
        cuerpo: Option<&Value>,
        prefer: Option<&str>,
    ) -> Resultado<Value> {
        let sesion = self.sesion_vigente()?;
        match self.llamar_con(&sesion, metodo, ruta, cuerpo, prefer) {
            Err(Error::SinSesionServidor) => {
                let renovada = self.renovar(&sesion)?;
                self.llamar_con(&renovada, metodo, ruta, cuerpo, prefer)
            }
            otro => otro,
        }
    }

    fn llamar_con(
        &self,
        sesion: &Sesion,
        metodo: &str,
        ruta: &str,
        cuerpo: Option<&Value>,
        prefer: Option<&str>,
    ) -> Resultado<Value> {
        let mut peticion = self
            .agente
            .request(metodo, &self.url(ruta))
            .set("apikey", self.proyecto.clave_publica)
            .set("Authorization", &format!("Bearer {}", sesion.access_token));
        if let Some(p) = prefer {
            peticion = peticion.set("Prefer", p);
        }
        let respuesta = match cuerpo {
            Some(c) => peticion.send_json(c.clone()),
            None => peticion.call(),
        };
        leer_json(respuesta, error_datos)
    }
}

/// Convierte la respuesta en `T` (o `Null` si no hay cuerpo) y los errores con `mapear(status, cuerpo)`.
fn leer_json<T: serde::de::DeserializeOwned>(
    respuesta: Result<ureq::Response, ureq::Error>,
    mapear: fn(u16, &str) -> Error,
) -> Resultado<T> {
    match respuesta {
        Ok(r) => {
            let texto = r.into_string().map_err(|_| Error::ServidorSinConexion)?;
            let texto = if texto.trim().is_empty() {
                "null"
            } else {
                texto.as_str()
            };
            serde_json::from_str(texto)
                .map_err(|_| Error::Servidor("respuesta que la app no entiende".into()))
        }
        // Código propio de Supabase: el proyecto está pausado (#246). Vale para Auth, datos y funciones.
        Err(ureq::Error::Status(HTTP_PROYECTO_PAUSADO, _)) => Err(Error::ServidorPausado),
        Err(ureq::Error::Status(status, r)) => {
            let cuerpo = r.into_string().unwrap_or_default();
            Err(mapear(status, &cuerpo))
        }
        Err(ureq::Error::Transport(_)) => Err(Error::ServidorSinConexion),
    }
}

fn campo<'a>(json: &'a Value, nombre: &str) -> Option<&'a str> {
    json.get(nombre).and_then(Value::as_str)
}

/// Errores de Auth (`{"code":400,"error_code":"…","msg":"…"}`).
fn error_auth(status: u16, cuerpo: &str) -> Error {
    let json: Value = serde_json::from_str(cuerpo).unwrap_or(Value::Null);
    match campo(&json, "error_code") {
        Some("invalid_credentials") => Error::Validacion("Correo o contraseña incorrectos.".into()),
        Some("email_not_confirmed") => {
            Error::Validacion("Confirma tu correo antes de iniciar sesión.".into())
        }
        Some("user_already_exists") | Some("email_exists") => {
            Error::Validacion("Ya existe una cuenta con ese correo. Inicia sesión.".into())
        }
        Some("weak_password") => {
            Error::Validacion("La contraseña debe tener al menos 8 caracteres.".into())
        }
        Some("email_address_invalid") => {
            Error::Validacion("El correo no es válido (ej. nombre@correo.com).".into())
        }
        Some("signup_disabled") => {
            Error::Validacion("El registro de cuentas nuevas está desactivado.".into())
        }
        Some("over_request_rate_limit") | Some("over_email_send_rate_limit") => {
            Error::Validacion("Demasiados intentos. Espera un momento y vuelve a probar.".into())
        }
        _ if status == 429 => {
            Error::Validacion("Demasiados intentos. Espera un momento y vuelve a probar.".into())
        }
        _ => Error::Servidor(format!(
            "HTTP {status}: {}",
            campo(&json, "msg").unwrap_or("sin detalle")
        )),
    }
}

/// Errores de nuestras Edge Functions (`{"error":"…"}` con el mensaje en español, #237).
fn error_funcion(status: u16, cuerpo: &str) -> Error {
    let json: Value = serde_json::from_str(cuerpo).unwrap_or(Value::Null);
    let mensaje = campo(&json, "error").unwrap_or("sin detalle");
    match status {
        // Unity VC rechazó el token: hay que volver a iniciar sesión con Unity.
        401 => Error::SesionUnityVcCaducada,
        400 | 403 | 404 | 409 => Error::Validacion(mensaje.to_string()),
        _ => Error::Servidor(format!("HTTP {status}: {mensaje}")),
    }
}

/// Errores de la API de datos (`{"code":"…","message":"…"}`).
fn error_datos(status: u16, cuerpo: &str) -> Error {
    let json: Value = serde_json::from_str(cuerpo).unwrap_or(Value::Null);
    let mensaje = campo(&json, "message").unwrap_or("sin detalle");
    match (status, campo(&json, "code")) {
        // Token vencido o inválido: renovar.
        (401, Some(c)) if c.starts_with("PGRST3") => Error::SinSesionServidor,
        (_, Some("42501")) => Error::SinPermiso,
        // Reglas de negocio de nuestras funciones (último Administrador, persona sin cuenta…).
        (_, Some("P0001")) => Error::Validacion(mensaje.to_string()),
        (401, _) => Error::SinSesionServidor,
        _ => Error::Servidor(format!("HTTP {status}: {mensaje}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supabase::entorno::PRUEBAS;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct LlaveroMemoria(RefCell<HashMap<String, String>>);

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

    fn sesion(expira_en: i64) -> Sesion {
        Sesion {
            access_token: "acceso-secreto".into(),
            refresh_token: "renovacion-secreta".into(),
            expira_en,
            usuario_id: "u1".into(),
            correo: "ana@correo.com".into(),
            nombre: "Ana".into(),
            apellido: "Pérez".into(),
        }
    }

    #[test]
    fn la_sesion_toma_el_vencimiento_del_servidor_y_el_correo_en_minusculas() {
        let r: RespuestaToken = serde_json::from_str(
            r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"expires_at":5000,"user":{"id":"u","email":" Ana@Correo.com "}}"#,
        )
        .unwrap();
        let s = sesion_desde(r, 1000);
        assert_eq!(s.expira_en, 5000);
        assert_eq!(s.correo, "ana@correo.com");
        let sin_expires_at: RespuestaToken = serde_json::from_str(
            r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"user":{"id":"u"}}"#,
        )
        .unwrap();
        assert_eq!(sesion_desde(sin_expires_at, 1000).expira_en, 4600);
    }

    #[test]
    fn la_sesion_trae_nombre_y_apellido_del_registro() {
        // Forma real de la respuesta de /auth/v1/signup sin confirmación (branchboard-pruebas, 2026-11-21).
        let r: RespuestaToken = serde_json::from_str(
            r#"{"access_token":"a","token_type":"bearer","expires_in":3600,"expires_at":5000,"refresh_token":"r",
                "user":{"id":"u","email":"ana@correo.com","email_confirmed_at":"2026-11-21T00:35:22Z",
                "user_metadata":{"apellido":"Registro","email_verified":true,"nombre":"Prueba","sub":"u"}}}"#,
        )
        .unwrap();
        let s = sesion_desde(r, 1000);
        assert_eq!(
            (s.nombre.as_str(), s.apellido.as_str()),
            ("Prueba", "Registro")
        );
        // Una sesión guardada antes de #236 (sin nombre) se sigue leyendo.
        let vieja: Sesion = serde_json::from_str(
            r#"{"access_token":"a","refresh_token":"r","expira_en":1,"usuario_id":"u","correo":"c"}"#,
        )
        .unwrap();
        assert_eq!(vieja.nombre, "");
    }

    #[test]
    fn se_renueva_un_minuto_antes_de_vencer() {
        assert!(!sesion(2000).necesita_renovar(1000));
        assert!(sesion(1059).necesita_renovar(1000));
        assert!(sesion(900).necesita_renovar(1000));
    }

    #[test]
    fn debug_no_muestra_los_tokens() {
        let texto = format!("{:?}", sesion(1));
        assert!(
            !texto.contains("secreto") && !texto.contains("secreta"),
            "{texto}"
        );
        assert!(texto.contains("<oculto>"));
    }

    #[test]
    fn la_sesion_se_guarda_en_el_llavero_y_se_lee_sin_red() {
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        assert!(cliente.sesion_guardada().unwrap().is_none());
        cliente.guardar(&sesion(9_999_999_999)).unwrap();
        assert_eq!(
            cliente.sesion_guardada().unwrap(),
            Some(sesion(9_999_999_999))
        );
        // Vigente y lejos de vencer: no toca la red.
        assert_eq!(
            cliente.sesion_vigente_en(1000).unwrap(),
            sesion(9_999_999_999)
        );
        assert_eq!(
            cliente
                .llavero
                .0
                .borrow()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["supabase-branchboard-pruebas"]
        );
    }

    #[test]
    fn sin_sesion_o_con_sesion_danada_pide_iniciar_sesion() {
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        assert!(matches!(
            cliente.sesion_vigente_en(0),
            Err(Error::SinSesionServidor)
        ));
        cliente
            .llavero
            .guardar("supabase-branchboard-pruebas", "{no es json")
            .unwrap();
        assert!(cliente.sesion_guardada().unwrap().is_none());
        assert!(cliente.llavero.0.borrow().is_empty(), "lo dañado se borra");
    }

    #[test]
    fn errores_de_auth() {
        let e = error_auth(
            400,
            r#"{"code":400,"error_code":"invalid_credentials","msg":"Invalid login credentials"}"#,
        );
        assert_eq!(e.to_string(), "Correo o contraseña incorrectos.");
        let e = error_auth(400, r#"{"error_code":"email_not_confirmed"}"#);
        assert_eq!(e.to_string(), "Confirma tu correo antes de iniciar sesión.");
        assert_eq!(error_auth(429, "").codigo(), "validacion");
        let e = error_auth(
            422,
            r#"{"code":422,"error_code":"user_already_exists","msg":"User already registered"}"#,
        );
        assert_eq!(
            e.to_string(),
            "Ya existe una cuenta con ese correo. Inicia sesión."
        );
        let e = error_auth(
            422,
            r#"{"code":422,"error_code":"weak_password","msg":"Password should be at least 8 characters.","weak_password":{"reasons":["length"]}}"#,
        );
        assert_eq!(
            e.to_string(),
            "La contraseña debe tener al menos 8 caracteres."
        );
        assert_eq!(
            error_auth(400, r#"{"error_code":"email_address_invalid"}"#).codigo(),
            "validacion"
        );
        assert_eq!(error_auth(500, r#"{"msg":"boom"}"#).codigo(), "servidor");
    }

    /// Servidor HTTP local que a todo responde `540` (lo que contesta Supabase con el proyecto pausado).
    /// Devuelve un proyecto que apunta a él y cuántas peticiones recibió.
    fn servidor_pausado() -> (
        &'static ProyectoSupabase,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicUsize, Ordering};
        let escucha = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let puerto = escucha.local_addr().unwrap().port();
        let peticiones = std::sync::Arc::new(AtomicUsize::new(0));
        let contador = peticiones.clone();
        std::thread::spawn(move || {
            for conexion in escucha.incoming() {
                let Ok(mut c) = conexion else { continue };
                // Lee la petición entera (cabeceras + cuerpo) antes de responder: si se cierra antes,
                // el cliente lo vería como "sin conexión".
                let mut leido = Vec::new();
                let mut buf = [0u8; 4096];
                while let Ok(n) = c.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    leido.extend_from_slice(&buf[..n]);
                    let texto = String::from_utf8_lossy(&leido).to_lowercase();
                    if let Some(fin) = texto.find("\r\n\r\n") {
                        let largo = texto[..fin]
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if leido.len() >= fin + 4 + largo {
                            break;
                        }
                    }
                }
                contador.fetch_add(1, Ordering::SeqCst);
                let _ = c.write_all(
                    b"HTTP/1.1 540 Project Paused\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        let url: &'static str = Box::leak(format!("http://127.0.0.1:{puerto}").into_boxed_str());
        let proyecto = Box::leak(Box::new(ProyectoSupabase {
            nombre: "branchboard-pausado",
            url,
            clave_publica: "sb_publishable_prueba",
        }));
        (proyecto, peticiones)
    }

    #[test]
    fn con_el_proyecto_pausado_se_avisa_y_la_sesion_no_se_pierde() {
        let (proyecto, peticiones) = servidor_pausado();
        let cliente = ClienteSupabase::new(proyecto, LlaveroMemoria::default());
        // Una sesión guardada por vencer: llamar intenta renovarla primero.
        let vieja = sesion(Utc::now().timestamp() + 10);
        cliente.guardar(&vieja).unwrap();

        assert!(matches!(
            cliente.llamar("GET", "/rest/v1/proyectos", None),
            Err(Error::ServidorPausado)
        ));
        assert!(matches!(
            cliente.renovar(&vieja),
            Err(Error::ServidorPausado)
        ));
        assert_eq!(
            cliente.sesion_guardada().unwrap(),
            Some(vieja.clone()),
            "pausado no es sesión inválida: no se borra"
        );
        assert!(matches!(
            cliente.iniciar_sesion("ana@virustf.dev", "clave"),
            Err(Error::ServidorPausado)
        ));
        assert!(matches!(
            cliente.iniciar_sesion_con_unity("TOKENx", "Org", "https://x.plasticscm.com"),
            Err(Error::ServidorPausado)
        ));

        // Con sesión vigente, subir eventos también lo detecta.
        cliente
            .guardar(&sesion(Utc::now().timestamp() + 3600))
            .unwrap();
        assert!(matches!(
            cliente.insertar_eventos(&serde_json::json!([])),
            Err(Error::ServidorPausado)
        ));
        assert!(peticiones.load(std::sync::atomic::Ordering::SeqCst) >= 5);
    }

    #[test]
    fn el_error_de_pausado_tiene_su_codigo_y_dice_que_hacer() {
        let json = serde_json::to_value(Error::ServidorPausado).unwrap();
        assert_eq!(json["codigo"], "servidor_pausado");
        assert!(json["mensaje"].as_str().unwrap().contains("reactive"));
    }

    #[test]
    fn errores_de_las_edge_functions() {
        assert!(matches!(
            error_funcion(401, r#"{"error":"La sesión de Unity VC no es válida"}"#),
            Error::SesionUnityVcCaducada
        ));
        match error_funcion(409, r#"{"error":"Ese repositorio ya está en BranchBoard"}"#) {
            Error::Validacion(m) => assert_eq!(m, "Ese repositorio ya está en BranchBoard"),
            otro => panic!("{otro:?}"),
        }
        assert!(matches!(
            error_funcion(502, r#"{"error":"No se pudo hablar con Unity VC"}"#),
            Error::Servidor(m) if m.starts_with("HTTP 502")
        ));
    }

    #[test]
    fn errores_de_la_api_de_datos() {
        assert!(matches!(
            error_datos(
                401,
                r#"{"code":"PGRST301","message":"Expected 3 parts in JWT; got 1"}"#
            ),
            Error::SinSesionServidor
        ));
        assert!(matches!(
            error_datos(401, r#"{"code":"PGRST303","message":"JWT expired"}"#),
            Error::SinSesionServidor
        ));
        assert!(matches!(
            error_datos(403, r#"{"code":"42501","message":"permission denied"}"#),
            Error::SinPermiso
        ));
        let e = error_datos(
            400,
            r#"{"code":"P0001","message":"El proyecto tiene que tener al menos un Administrador"}"#,
        );
        assert_eq!(
            e.to_string(),
            "El proyecto tiene que tener al menos un Administrador"
        );
        assert_eq!(error_datos(500, "no json").codigo(), "servidor");
    }

    // ------------------------------------------------------------------ contra branchboard-pruebas

    #[test]
    #[ignore = "toca el proyecto de pruebas de Supabase: correr a mano con --ignored"]
    fn entorno_real_contrasena_incorrecta_y_renovacion_invalida() {
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        let e = cliente
            .iniciar_sesion("nadie@prueba.branchboard", "clave-incorrecta-123")
            .unwrap_err();
        assert_eq!(e.to_string(), "Correo o contraseña incorrectos.");
        assert!(cliente.sesion_guardada().unwrap().is_none());

        // Un token de renovación que el servidor no acepta: se borra la sesión y se pide iniciar sesión.
        cliente.guardar(&sesion(0)).unwrap();
        assert!(matches!(
            cliente.sesion_vigente(),
            Err(Error::SinSesionServidor)
        ));
        assert!(cliente.sesion_guardada().unwrap().is_none());
    }

    #[test]
    #[ignore = "necesita red (resuelve un nombre que no existe): correr a mano con --ignored"]
    fn entorno_real_sin_conexion_conserva_la_sesion() {
        static INEXISTENTE: ProyectoSupabase = ProyectoSupabase {
            nombre: "inexistente",
            url: "https://no-existe-branchboard.supabase.co",
            clave_publica: "sb_publishable_x",
        };
        let cliente = ClienteSupabase::new(&INEXISTENTE, LlaveroMemoria::default());
        cliente.guardar(&sesion(0)).unwrap();
        assert!(matches!(
            cliente.sesion_vigente(),
            Err(Error::ServidorSinConexion)
        ));
        assert!(
            cliente.sesion_guardada().unwrap().is_some(),
            "sin red, la sesión se conserva"
        );
    }

    /// Correo y clave para una cuenta nueva de branchboard-pruebas. La clave es aleatoria y no se imprime.
    /// Las cuentas `ciclo-…@prueba.branchboard` se borran después con SQL (ver BACKEND.md).
    fn cuenta_temporal() -> (String, String) {
        use rand::{distributions::Alphanumeric, Rng};
        let clave: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let correo = format!(
            "ciclo-{}@prueba.branchboard",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        (correo, clave)
    }

    /// Registro sin confirmación de correo (#236): la cuenta queda con la sesión iniciada, su perfil con
    /// nombre y apellido, un segundo registro con el mismo correo se rechaza y se puede volver a entrar.
    #[test]
    #[ignore = "crea una cuenta en branchboard-pruebas: correr a mano con --ignored y borrarla (BACKEND.md)"]
    fn entorno_real_registrar_y_volver_a_entrar() {
        let (correo, clave) = cuenta_temporal();
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());

        let corta = cliente
            .registrar("Prueba", "Registro", &correo, "corta")
            .unwrap_err();
        assert_eq!(
            corta.to_string(),
            "La contraseña debe tener al menos 8 caracteres."
        );

        let sesion = cliente
            .registrar(" Prueba ", "Registro", &correo, &clave)
            .unwrap();
        assert_eq!(sesion.correo, correo);
        assert_eq!(
            (sesion.nombre.as_str(), sesion.apellido.as_str()),
            ("Prueba", "Registro")
        );
        assert_eq!(cliente.sesion_guardada().unwrap(), Some(sesion.clone()));
        // El trigger de #233 creó su perfil con los datos del registro.
        let perfil = cliente
            .llamar(
                "GET",
                "/rest/v1/perfiles?select=correo,nombre,apellido",
                None,
            )
            .unwrap();
        assert_eq!(
            perfil,
            serde_json::json!([{ "correo": correo, "nombre": "Prueba", "apellido": "Registro" }])
        );

        let repetida = cliente
            .registrar("Otra", "Persona", &correo.to_uppercase(), &clave)
            .unwrap_err();
        assert_eq!(
            repetida.to_string(),
            "Ya existe una cuenta con ese correo. Inicia sesión."
        );

        cliente.cerrar_sesion().unwrap();
        assert!(cliente.sesion_guardada().unwrap().is_none());
        let otra_vez = cliente.iniciar_sesion(&correo, &clave).unwrap();
        assert_eq!(otra_vez.usuario_id, sesion.usuario_id);
        assert_eq!(otra_vez.nombre, "Prueba");
        cliente.cerrar_sesion().unwrap();
    }

    /// La sesión real (con su JWT) cabe en el llavero de Windows y se lee igual. Usa una cuenta de llavero
    /// propia (`supabase-branchboard-pruebas-test`) para no pisar la sesión de `npm run tauri dev`.
    #[test]
    #[ignore = "crea una cuenta en branchboard-pruebas y usa el llavero del sistema: correr a mano con --ignored"]
    fn entorno_real_la_sesion_cabe_en_el_llavero_del_sistema() {
        use crate::servidor::LlaveroSistema;
        static PRUEBAS_TEST: ProyectoSupabase = ProyectoSupabase {
            nombre: "branchboard-pruebas-test",
            url: PRUEBAS.url,
            clave_publica: PRUEBAS.clave_publica,
        };
        let (correo, clave) = cuenta_temporal();
        let cliente = ClienteSupabase::new(&PRUEBAS_TEST, LlaveroSistema);
        let sesion = cliente
            .registrar("Prueba", "Llavero", &correo, &clave)
            .unwrap();
        println!(
            "sesión guardada: {} bytes",
            serde_json::to_string(&sesion).unwrap().len()
        );
        assert_eq!(cliente.sesion_guardada().unwrap(), Some(sesion));
        cliente.cerrar_sesion().unwrap();
        assert!(cliente.sesion_guardada().unwrap().is_none());
    }

    /// Ciclo completo con una cuenta nueva de branchboard-pruebas: iniciar sesión, llamar a la API,
    /// forzar la renovación, volver a llamar y cerrar.
    #[test]
    #[ignore = "crea una cuenta en branchboard-pruebas: correr a mano con --ignored y borrarla (BACKEND.md)"]
    fn entorno_real_iniciar_llamar_renovar_y_cerrar() {
        let (correo, clave) = cuenta_temporal();
        let cliente = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
        cliente
            .registrar("Prueba", "Ciclo", &correo, &clave)
            .unwrap();
        cliente.cerrar_sesion().unwrap();

        let primera = cliente.iniciar_sesion(&correo, &clave).unwrap();
        assert_eq!(primera.correo, correo.to_lowercase());
        assert!(primera.expira_en > Utc::now().timestamp());

        // Una cuenta sin proyectos ve una lista vacía (las reglas de #234 la dejan leer, pero no hay nada suyo).
        let proyectos = cliente
            .llamar("GET", "/rest/v1/proyectos?select=id", None)
            .unwrap();
        assert_eq!(proyectos, serde_json::json!([]));
        // Las funciones del servidor responden con sus reglas: sin ser miembro no se cambia ningún rol.
        let e = cliente
            .llamar(
                "POST",
                "/rest/v1/rpc/cambiar_rol",
                Some(&serde_json::json!({
                    "p_proyecto": "00000000-0000-4000-8000-000000000000",
                    "p_usuario": primera.usuario_id,
                    "p_rol": "Administrador"
                })),
            )
            .unwrap_err();
        assert!(matches!(e, Error::SinPermiso), "{e:?}");

        let renovada = cliente.renovar(&primera).unwrap();
        assert_ne!(renovada.refresh_token, primera.refresh_token);
        assert_eq!(cliente.sesion_guardada().unwrap(), Some(renovada.clone()));
        assert_eq!(
            cliente
                .llamar("GET", "/rest/v1/proyectos?select=id", None)
                .unwrap(),
            serde_json::json!([])
        );

        cliente.cerrar_sesion().unwrap();
        assert!(cliente.sesion_guardada().unwrap().is_none());
        // Tras cerrar, el token de renovación ya no sirve.
        assert!(matches!(
            cliente.renovar(&renovada),
            Err(Error::SinSesionServidor)
        ));
    }
}
