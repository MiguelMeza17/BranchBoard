//! La única forma de correr `cm` (u otro binario) desde el backend.
//!
//! - Cada argumento viaja por separado al proceso, **nunca** como un string de
//!   shell: un título con `; rm -rf x` llega a `cm` como texto literal (ver
//!   "Seguridad al invocar cm" en BACKEND.md).
//! - Tiempo máximo: si el proceso no termina, se mata y se devuelve error.
//! - Mientras corre cuenta como operación en curso en `AppState`, así la app no
//!   se deja cerrar a la mitad (ver "Bloquear el cierre" en BACKEND.md).

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::{Error, Resultado};
use crate::estado::AppState;

/// Tiempo máximo por defecto para un comando. Los que pueden tardar más
/// (clonar, merge grande) lo suben con [`Comando::tiempo_maximo`].
pub const TIEMPO_MAXIMO: Duration = Duration::from_secs(120);
const INTERVALO_ESPERA: Duration = Duration::from_millis(25);

/// Lo que devolvió el proceso.
#[derive(Debug, Clone, PartialEq)]
pub struct Salida {
    /// `None` si lo mató una señal.
    pub codigo: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Salida {
    pub fn exito(&self) -> bool {
        self.codigo == Some(0)
    }
}

/// Un comando por ejecutar: programa, argumentos separados, carpeta y tiempo máximo.
#[derive(Debug, Clone)]
pub struct Comando {
    programa: String,
    argumentos: Vec<String>,
    carpeta: Option<PathBuf>,
    tiempo_maximo: Duration,
}

impl Comando {
    pub fn nuevo(programa: &str) -> Self {
        Comando {
            programa: programa.to_string(),
            argumentos: Vec::new(),
            carpeta: None,
            tiempo_maximo: TIEMPO_MAXIMO,
        }
    }

    /// `cm` de Unity Version Control.
    pub fn cm() -> Self {
        Self::nuevo("cm")
    }

    /// Agrega un argumento tal cual, sin interpretarlo.
    pub fn arg(mut self, argumento: impl Into<String>) -> Self {
        self.argumentos.push(argumento.into());
        self
    }

    pub fn args<I, S>(mut self, argumentos: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.argumentos
            .extend(argumentos.into_iter().map(Into::into));
        self
    }

    /// Carpeta donde corre (el workspace, normalmente).
    pub fn en(mut self, carpeta: &Path) -> Self {
        self.carpeta = Some(carpeta.to_path_buf());
        self
    }

    pub fn tiempo_maximo(mut self, tiempo: Duration) -> Self {
        self.tiempo_maximo = tiempo;
        self
    }

    /// Solo para mensajes: el comando como se leería, nunca se ejecuta así.
    pub fn descripcion(&self) -> String {
        let mut partes = vec![self.programa.clone()];
        partes.extend(self.argumentos.iter().cloned());
        partes.join(" ")
    }

    /// Corre el proceso hasta que termine (o se pase del tiempo máximo) y
    /// devuelve su salida, también cuando el código no es 0.
    pub fn ejecutar(&self, estado: &AppState) -> Resultado<Salida> {
        let _operacion = estado.iniciar_operacion();

        let mut comando = Command::new(&self.programa);
        comando
            .args(&self.argumentos)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(carpeta) = &self.carpeta {
            comando.current_dir(carpeta);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            comando.creation_flags(CREATE_NO_WINDOW);
        }

        let mut hijo = comando
            .spawn()
            .map_err(|e| error_de_arranque(&self.programa, &e))?;
        let stdout = leer_en_hilo(hijo.stdout.take());
        let stderr = leer_en_hilo(hijo.stderr.take());

        let codigo = self.esperar(&mut hijo)?;
        Ok(Salida {
            codigo,
            stdout: recoger(stdout),
            stderr: recoger(stderr),
        })
    }

    /// Como [`ejecutar`](Self::ejecutar), pero exige código 0 y devuelve el `stdout`;
    /// si falla, el error lleva lo que dijo el proceso.
    pub fn exigir_exito(&self, estado: &AppState) -> Resultado<String> {
        let salida = self.ejecutar(estado)?;
        if salida.exito() {
            return Ok(salida.stdout);
        }
        let detalle = match (salida.stderr.trim(), salida.stdout.trim()) {
            ("", "") => match salida.codigo {
                Some(c) => format!("terminó con código {c}"),
                None => "fue interrumpido".to_string(),
            },
            ("", solo_stdout) => solo_stdout.to_string(),
            (stderr, _) => stderr.to_string(),
        };
        Err(Error::Cm(format!("`{}`: {detalle}", self.descripcion())))
    }

    fn esperar(&self, hijo: &mut Child) -> Resultado<Option<i32>> {
        let inicio = Instant::now();
        loop {
            if let Some(fin) = hijo.try_wait().map_err(error_interno)? {
                return Ok(fin.code());
            }
            if inicio.elapsed() >= self.tiempo_maximo {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return Err(Error::Cm(format!(
                    "`{}` tardó más de {} s y se canceló",
                    self.descripcion(),
                    self.tiempo_maximo.as_secs()
                )));
            }
            thread::sleep(INTERVALO_ESPERA);
        }
    }
}

fn error_interno(e: io::Error) -> Error {
    Error::Interno(e.to_string())
}

/// El proceso no llegó a arrancar: `cm` no instalado, o cualquier otro fallo.
fn error_de_arranque(programa: &str, e: &io::Error) -> Error {
    match (programa, e.kind()) {
        ("cm", io::ErrorKind::NotFound) => Error::CmNoInstalado,
        (_, io::ErrorKind::NotFound) => {
            Error::Interno(format!("No se encontró el programa `{programa}`"))
        }
        _ => Error::Interno(format!("No se pudo ejecutar `{programa}`: {e}")),
    }
}

/// Lee un flujo del hijo en su propio hilo, para que stdout y stderr no se
/// bloqueen entre sí cuando el proceso escribe mucho en uno de los dos.
fn leer_en_hilo(flujo: Option<impl Read + Send + 'static>) -> Option<JoinHandle<Vec<u8>>> {
    flujo.map(|mut f| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = f.read_to_end(&mut bytes);
            bytes
        })
    })
}

fn recoger(hilo: Option<JoinHandle<Vec<u8>>>) -> String {
    let bytes = hilo.and_then(|h| h.join().ok()).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Imprime sus argumentos tal cual (cada sistema con lo que tiene a mano).
    fn eco(texto: &str) -> Comando {
        if cfg!(windows) {
            Comando::nuevo("cmd").args(["/C", "echo", texto])
        } else {
            Comando::nuevo("/bin/echo").arg(texto)
        }
    }

    /// Tarda unos segundos en terminar.
    fn lento() -> Comando {
        if cfg!(windows) {
            Comando::nuevo("cmd").args(["/C", "ping", "-n", "4", "127.0.0.1"])
        } else {
            Comando::nuevo("sleep").arg("3")
        }
    }

    /// Termina con código 3 sin decir nada.
    fn falla() -> Comando {
        if cfg!(windows) {
            Comando::nuevo("cmd").args(["/C", "exit", "3"])
        } else {
            Comando::nuevo("sh").args(["-c", "exit 3"])
        }
    }

    #[test]
    fn un_titulo_con_comandos_de_shell_viaja_como_texto() {
        let estado = AppState::default();
        // ASCII a propósito: `cmd` imprime en la página de códigos OEM, no en UTF-8.
        let titulo = "Menu principal; rm -rf carpeta && echo hola";
        let salida = eco(titulo).ejecutar(&estado).unwrap();
        assert!(salida.exito());
        let impreso = salida.stdout.trim().trim_matches('"');
        assert_eq!(
            impreso, titulo,
            "llega entero, sin que nadie interprete `;` ni `&&`"
        );
        assert_eq!(
            salida.stdout.trim().lines().count(),
            1,
            "no se ejecutó un segundo comando"
        );
        assert!(!estado.hay_operacion_en_curso());
    }

    #[test]
    fn mientras_corre_cuenta_como_operacion_en_curso() {
        let estado = Arc::new(AppState::default());
        let en_hilo = Arc::clone(&estado);
        let hilo = thread::spawn(move || lento().ejecutar(&en_hilo).map(|s| s.exito()));
        thread::sleep(Duration::from_millis(500));
        assert!(estado.hay_operacion_en_curso());
        assert!(hilo.join().unwrap().unwrap());
        assert!(!estado.hay_operacion_en_curso());
    }

    #[test]
    fn se_cancela_al_pasar_el_tiempo_maximo() {
        let estado = AppState::default();
        let inicio = Instant::now();
        let error = lento()
            .tiempo_maximo(Duration::from_millis(300))
            .ejecutar(&estado)
            .unwrap_err();
        assert_eq!(error.codigo(), "cm");
        assert!(error.to_string().contains("se canceló"));
        assert!(
            inicio.elapsed() < Duration::from_secs(2),
            "no esperó a que terminara solo"
        );
        assert!(!estado.hay_operacion_en_curso());
    }

    #[test]
    fn exigir_exito_cuenta_lo_que_dijo_el_proceso() {
        let estado = AppState::default();
        assert_eq!(eco("hola").exigir_exito(&estado).unwrap().trim(), "hola");
        let error = falla().exigir_exito(&estado).unwrap_err();
        assert_eq!(error.codigo(), "cm");
        assert!(error.to_string().contains("código 3"), "{error}");
    }

    #[test]
    fn programa_inexistente_y_cm_no_instalado() {
        let estado = AppState::default();
        let error = Comando::nuevo("programa-que-no-existe-bb")
            .ejecutar(&estado)
            .unwrap_err();
        assert_eq!(error.codigo(), "interno");
        assert!(error.to_string().contains("programa-que-no-existe-bb"));

        let no_esta = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(
            error_de_arranque("cm", &no_esta).codigo(),
            "cm_no_instalado"
        );
        assert_eq!(error_de_arranque("otro", &no_esta).codigo(), "interno");
        let sin_permiso = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(error_de_arranque("cm", &sin_permiso).codigo(), "interno");
    }

    #[test]
    fn descripcion_y_constructores() {
        let comando = Comando::cm()
            .arg("log")
            .args(["br:/main/task-1", "--xml"])
            .en(Path::new("C:/ws"))
            .tiempo_maximo(Duration::from_secs(5));
        assert_eq!(comando.descripcion(), "cm log br:/main/task-1 --xml");
        assert_eq!(comando.tiempo_maximo, Duration::from_secs(5));
        assert_eq!(comando.carpeta.as_deref(), Some(Path::new("C:/ws")));
    }
}
