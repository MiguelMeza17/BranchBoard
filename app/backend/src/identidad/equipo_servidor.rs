//! Pantalla Usuarios con el proyecto en el servidor (#241). Reemplaza a [`super::equipo`] (eventos
//! firmados por `ops/`) cuando el proyecto tiene `proyecto_servidor_id`:
//!
//! - Los roles viven en `miembros` y solo los cambian las funciones del servidor, que revisan el rol de
//!   quien pide (#234): agregar (o invitar, si no tiene cuenta), cambiar rol (un Administrador puede nombrar
//!   otros Administradores; nunca se quita ni degrada al último) y quitar.
//! - La tabla local `usuarios` es una **copia** de `miembros` ([`miembros::filas_locales`]), sin firmas: se
//!   rehace después de cada cambio y al listar. Sin red se lista la última copia.
//! - Ya no hay "confirmar usuario": no hay firma que confirmar (y desde #243 no hay firmas en ningún lado).
//!
//! La red y la base van por separado: [`EquipoRemoto`] habla con el servidor y `guardar` escribe la copia.

use crate::db::usuarios::{nombre_desde_email, Usuario};
use crate::error::{Error, Resultado};
use crate::identidad::equipo::UsuarioVisible;
use crate::modelo::{normalizar_email, Rol};
use crate::supabase::miembros::{self, Equipo, Invitacion};

/// Lo que se le pide al servidor (así las pruebas sin red usan uno en memoria).
pub trait EquipoRemoto {
    fn leer(&self) -> Resultado<Equipo>;
    /// `Some(id)` si la persona ya tenía cuenta; `None` si quedó invitada.
    fn agregar(&self, correo: &str, rol: Rol) -> Resultado<Option<String>>;
    fn cambiar_rol(&self, usuario_id: &str, rol: Rol) -> Resultado<()>;
    fn quitar(&self, usuario_id: &str) -> Resultado<()>;
    fn quitar_invitacion(&self, correo: &str) -> Resultado<()>;
}

/// [`EquipoRemoto`] contra Supabase con la sesión del cliente.
pub struct EnSupabase<'a, L: crate::servidor::pat::Llavero> {
    pub cliente: &'a crate::supabase::ClienteSupabase<L>,
    pub proyecto: &'a str,
}

impl<L: crate::servidor::pat::Llavero> EquipoRemoto for EnSupabase<'_, L> {
    fn leer(&self) -> Resultado<Equipo> {
        miembros::leer_equipo(self.cliente, self.proyecto)
    }
    fn agregar(&self, correo: &str, rol: Rol) -> Resultado<Option<String>> {
        miembros::agregar(self.cliente, self.proyecto, correo, rol)
    }
    fn cambiar_rol(&self, usuario_id: &str, rol: Rol) -> Resultado<()> {
        miembros::cambiar_rol(self.cliente, self.proyecto, usuario_id, rol)
    }
    fn quitar(&self, usuario_id: &str) -> Resultado<()> {
        miembros::quitar(self.cliente, self.proyecto, usuario_id)
    }
    fn quitar_invitacion(&self, correo: &str) -> Resultado<()> {
        miembros::quitar_invitacion(self.cliente, self.proyecto, correo)
    }
}

/// Lee el equipo del servidor y rehace la copia local.
pub fn refrescar(
    remoto: &dyn EquipoRemoto,
    guardar: impl FnOnce(&[Usuario]) -> Resultado<()>,
) -> Resultado<Equipo> {
    let equipo = remoto.leer()?;
    guardar(&miembros::filas_locales(&equipo))?;
    Ok(equipo)
}

fn visible_invitacion(i: &Invitacion) -> UsuarioVisible {
    let email = normalizar_email(&i.correo);
    let (nombre, apellido) = nombre_desde_email(&email);
    UsuarioVisible {
        propio: false,
        email: Some(email),
        nombre,
        apellido,
        rol: i.rol,
        fecha_alta: i.invitado_en.chars().take(10).collect(),
        invitado: true,
    }
}

/// `listar_usuarios`: refresca la copia (sin red, usa la última) y lista. Líder y Administrador ven
/// también a los invitados (las demás no reciben invitaciones del servidor: llevan correos).
pub fn listar(
    remoto: &dyn EquipoRemoto,
    guardar: impl FnOnce(&[Usuario]) -> Resultado<()>,
    listar_local: impl FnOnce() -> Resultado<Vec<UsuarioVisible>>,
    rol_de_quien_pide: Rol,
) -> Resultado<Vec<UsuarioVisible>> {
    let equipo = match refrescar(remoto, guardar) {
        Ok(e) => Some(e),
        Err(Error::ServidorSinConexion) => None,
        Err(e) => return Err(e),
    };
    let mut filas = listar_local()?;
    let rol_actual = filas
        .iter()
        .find(|u| u.propio)
        .map(|u| u.rol)
        .unwrap_or(rol_de_quien_pide);
    if rol_actual.es_lider_o_admin() {
        if let Some(e) = equipo {
            filas.extend(e.invitaciones.iter().map(visible_invitacion));
        }
    }
    Ok(filas)
}

/// `crear_usuario`: agrega por correo; si no tiene cuenta queda invitado (sin firma ni rol efectivo
/// hasta que se registre con ese correo).
pub fn crear(
    remoto: &dyn EquipoRemoto,
    guardar: impl FnOnce(&[Usuario]) -> Resultado<()>,
    correo: &str,
    rol: Rol,
    hoy: &str,
) -> Resultado<Usuario> {
    let email = normalizar_email(correo);
    remoto.agregar(&email, rol)?;
    let copia = refrescar(remoto, guardar).ok().and_then(|e| {
        miembros::filas_locales(&e)
            .into_iter()
            .find(|u| u.email == email)
    });
    Ok(copia.unwrap_or_else(|| {
        let (nombre, apellido) = nombre_desde_email(&email);
        Usuario {
            email: email.clone(),
            nombre,
            apellido,
            rol,
            fecha_alta: hoy.to_string(),
        }
    }))
}

/// `cambiar_rol`. Con el servidor un Administrador puede nombrar a otro Administrador; el servidor no deja
/// degradar al último. A un invitado no se le cambia: se quita la invitación y se vuelve a invitar.
pub fn cambiar_rol(
    remoto: &dyn EquipoRemoto,
    guardar: impl FnOnce(&[Usuario]) -> Resultado<()>,
    correo: &str,
    rol: Rol,
) -> Resultado<()> {
    let equipo = remoto.leer()?;
    if equipo.esta_invitado(correo) {
        return Err(Error::Validacion(
            "Esa persona todavía no tiene cuenta: quita la invitación y vuelve a invitarla con otro rol"
                .into(),
        ));
    }
    let id = equipo
        .id_de(correo)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {}", normalizar_email(correo))))?;
    remoto.cambiar_rol(id, rol)?;
    refrescar(remoto, guardar).map(|_| ())
}

/// `eliminar_usuario`: lo quita del proyecto (pierde el acceso de inmediato, también a los documentos de
/// Storage, #242) o cancela su invitación.
pub fn eliminar(
    remoto: &dyn EquipoRemoto,
    guardar: impl FnOnce(&[Usuario]) -> Resultado<()>,
    correo: &str,
) -> Resultado<()> {
    let equipo = remoto.leer()?;
    if equipo.esta_invitado(correo) {
        remoto.quitar_invitacion(correo)?;
    } else {
        let id = equipo.id_de(correo).ok_or_else(|| {
            Error::NoEncontrado(format!("el usuario {}", normalizar_email(correo)))
        })?;
        remoto.quitar(id)?;
    }
    refrescar(remoto, guardar)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::db::{usuarios, Db};
    use crate::identidad::equipo;
    use crate::identidad::permisos;
    use crate::supabase::miembros::{Miembro, Perfil};

    const ANA: &str = "ana@virustf.dev";
    const BETO: &str = "beto@virustf.dev";

    /// Servidor en memoria con las reglas de `puede_asignar` y el último Administrador (#234).
    struct Falso {
        equipo: RefCell<Equipo>,
        yo: Rol,
        sin_red: bool,
    }

    impl Falso {
        fn nuevo(yo: Rol) -> Self {
            let m = |id: &str, correo: &str, rol| Miembro {
                rol,
                agregado_en: "2026-11-21T10:00:00+00:00".into(),
                usuario: Perfil {
                    id: id.into(),
                    correo: correo.into(),
                    nombre: String::new(),
                    apellido: String::new(),
                },
            };
            Falso {
                equipo: RefCell::new(Equipo {
                    miembros: vec![
                        m("id-ana", ANA, Rol::Administrador),
                        m("id-beto", BETO, Rol::Desarrollador),
                    ],
                    invitaciones: vec![],
                }),
                yo,
                sin_red: false,
            }
        }
        fn red(&self) -> Resultado<()> {
            if self.sin_red {
                Err(Error::ServidorSinConexion)
            } else {
                Ok(())
            }
        }
    }

    impl EquipoRemoto for Falso {
        fn leer(&self) -> Resultado<Equipo> {
            self.red()?;
            let mut e = self.equipo.borrow().clone();
            if !self.yo.es_lider_o_admin() {
                e.invitaciones.clear();
            }
            Ok(e)
        }
        fn agregar(&self, correo: &str, rol: Rol) -> Resultado<Option<String>> {
            self.red()?;
            if !self.yo.es_lider_o_admin() || (self.yo == Rol::Lider && rol.es_lider_o_admin()) {
                return Err(Error::SinPermiso);
            }
            self.equipo.borrow_mut().invitaciones.push(Invitacion {
                correo: correo.into(),
                rol,
                invitado_en: "2026-11-22T00:00:00+00:00".into(),
            });
            Ok(None)
        }
        fn cambiar_rol(&self, usuario_id: &str, rol: Rol) -> Resultado<()> {
            self.red()?;
            if self.yo != Rol::Administrador {
                return Err(Error::SinPermiso);
            }
            let mut e = self.equipo.borrow_mut();
            let admins = e
                .miembros
                .iter()
                .filter(|m| m.rol == Rol::Administrador)
                .count();
            let m = e
                .miembros
                .iter_mut()
                .find(|m| m.usuario.id == usuario_id)
                .unwrap();
            if m.rol == Rol::Administrador && rol != Rol::Administrador && admins == 1 {
                return Err(Error::Validacion(
                    "No se puede dejar el proyecto sin Administrador".into(),
                ));
            }
            m.rol = rol;
            Ok(())
        }
        fn quitar(&self, usuario_id: &str) -> Resultado<()> {
            self.red()?;
            self.equipo
                .borrow_mut()
                .miembros
                .retain(|m| m.usuario.id != usuario_id);
            Ok(())
        }
        fn quitar_invitacion(&self, correo: &str) -> Resultado<()> {
            self.red()?;
            self.equipo
                .borrow_mut()
                .invitaciones
                .retain(|i| i.correo != correo);
            Ok(())
        }
    }

    fn base() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::abrir(dir.path()).unwrap();
        (dir, db)
    }

    #[test]
    fn la_copia_local_da_los_roles() {
        let (_d, db) = base();
        let c = db.conexion();
        let remoto = Falso::nuevo(Rol::Administrador);
        refrescar(&remoto, |f| usuarios::reemplazar_todos(c, f)).unwrap();
        // Los roles de la copia valen tal cual: los hace cumplir el servidor.
        let ana = permisos::usuario_actual(c, ANA).unwrap();
        assert_eq!(ana.rol, Rol::Administrador);
        assert_eq!(
            crate::tareas::rol_de(c, BETO).unwrap(),
            Some(Rol::Desarrollador)
        );
    }

    #[test]
    fn invitar_listar_cambiar_y_quitar() {
        let (_d, db) = base();
        let c = db.conexion();
        let remoto = Falso::nuevo(Rol::Administrador);
        let guardar = |f: &[Usuario]| usuarios::reemplazar_todos(c, f);

        // Invitar a alguien sin cuenta: vuelve la fila con el rol pedido y aparece como invitado.
        let nuevo = crear(
            &remoto,
            guardar,
            "Nuevo@VirusTF.dev",
            Rol::Tester,
            "2026-11-22",
        )
        .unwrap();
        assert_eq!(
            (nuevo.email.as_str(), nuevo.rol),
            ("nuevo@virustf.dev", Rol::Tester)
        );
        let lista = listar(
            &remoto,
            guardar,
            || equipo::listar(c, Rol::Administrador),
            Rol::Administrador,
        )
        .unwrap();
        assert_eq!(lista.len(), 3);
        let inv = lista.iter().find(|u| u.invitado).unwrap();
        assert_eq!(inv.email.as_deref(), Some("nuevo@virustf.dev"));
        assert_eq!(inv.rol, Rol::Tester);

        // Una copia previa de Administrador no permite mostrar invitaciones tras actualizar el rol.
        let actualizada = listar(
            &remoto,
            guardar,
            || equipo::listar_para(c, Rol::Desarrollador, Some(BETO)),
            Rol::Administrador,
        )
        .unwrap();
        assert_eq!(actualizada.len(), 2);
        assert!(actualizada.iter().all(|u| u.email.is_none() && !u.invitado));
        assert_eq!(actualizada.iter().filter(|u| u.propio).count(), 1);

        // Un Desarrollador no ve invitados ni correos.
        let dev = Falso::nuevo(Rol::Desarrollador);
        let lista = listar(
            &dev,
            guardar,
            || equipo::listar(c, Rol::Desarrollador),
            Rol::Desarrollador,
        )
        .unwrap();
        assert_eq!(lista.len(), 2);
        assert!(lista.iter().all(|u| u.email.is_none() && !u.invitado));

        // A un invitado no se le cambia el rol; se quita y listo.
        assert!(matches!(
            cambiar_rol(&remoto, guardar, "nuevo@virustf.dev", Rol::Lider),
            Err(Error::Validacion(_))
        ));
        eliminar(&remoto, guardar, "nuevo@virustf.dev").unwrap();
        assert!(remoto.equipo.borrow().invitaciones.is_empty());

        // Administrador nombra a otro Administrador; después ya se puede degradar al primero.
        cambiar_rol(&remoto, guardar, BETO, Rol::Administrador).unwrap();
        assert_eq!(
            usuarios::buscar(c, BETO).unwrap().unwrap().rol,
            Rol::Administrador
        );
        cambiar_rol(&remoto, guardar, ANA, Rol::Lider).unwrap();
        // Y al último no.
        assert!(matches!(
            cambiar_rol(&remoto, guardar, BETO, Rol::Tester),
            Err(Error::Validacion(_))
        ));

        // Quitar: la copia local lo pierde.
        eliminar(&remoto, guardar, ANA).unwrap();
        assert!(usuarios::buscar(c, ANA).unwrap().is_none());
        assert!(matches!(
            eliminar(&remoto, guardar, "nadie@virustf.dev"),
            Err(Error::NoEncontrado(_))
        ));
    }

    #[test]
    fn sin_red_se_lista_la_ultima_copia_y_los_cambios_no_se_hacen() {
        let (_d, db) = base();
        let c = db.conexion();
        let guardar = |f: &[Usuario]| usuarios::reemplazar_todos(c, f);
        refrescar(&Falso::nuevo(Rol::Administrador), guardar).unwrap();
        let mut sin_red = Falso::nuevo(Rol::Administrador);
        sin_red.sin_red = true;
        let lista = listar(
            &sin_red,
            guardar,
            || equipo::listar(c, Rol::Administrador),
            Rol::Administrador,
        )
        .unwrap();
        assert_eq!(lista.len(), 2);
        assert!(matches!(
            crear(
                &sin_red,
                guardar,
                "x@virustf.dev",
                Rol::Tester,
                "2026-11-22"
            ),
            Err(Error::ServidorSinConexion)
        ));
        assert!(matches!(
            cambiar_rol(&sin_red, guardar, BETO, Rol::Tester),
            Err(Error::ServidorSinConexion)
        ));
    }

    #[test]
    fn el_servidor_decide_los_permisos() {
        let (_d, db) = base();
        let c = db.conexion();
        let lider = Falso::nuevo(Rol::Lider);
        let guardar = |f: &[Usuario]| usuarios::reemplazar_todos(c, f);
        assert!(matches!(
            crear(&lider, guardar, "x@virustf.dev", Rol::Lider, "2026-11-22"),
            Err(Error::SinPermiso)
        ));
    }

    #[derive(Default)]
    struct LlaveroMemoria(RefCell<std::collections::HashMap<String, String>>);

    impl crate::servidor::pat::Llavero for LlaveroMemoria {
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

    /// **Prueba en el entorno real** (#241) contra **branchboard-pruebas**: A crea un proyecto, agrega a B
    /// como Desarrollador e invita a un correo sin cuenta. B **no** puede cambiar roles ni agregar (lo
    /// rechaza el servidor, no la app) ni ve invitaciones. El invitado se registra y entra solo con su
    /// rol. A nombra Administrador a B y se degrada; B, ya último Administrador, no se puede degradar; B
    /// quita al invitado y a A, y A deja de ver el proyecto.
    ///
    /// Registra sus propias cuentas (`usuarios-a|b|invitado-<hora>@prueba.branchboard`, clave aleatoria que
    /// no se imprime) y crea el proyecto `bb-prueba-241-<hora>`. La API pública no borra cuentas ni
    /// proyectos: al terminar se borran con SQL ("Usuarios y roles por el servidor (#241)" en BACKEND.md).
    #[test]
    #[ignore = "crea cuentas y un proyecto en branchboard-pruebas: correr a mano con --ignored y borrarlos (BACKEND.md)"]
    fn entorno_real_usuarios_por_el_servidor() {
        use crate::supabase::entorno::PRUEBAS;
        use crate::supabase::ClienteSupabase;
        use rand::{distributions::Alphanumeric, Rng};

        let hora = chrono::Utc::now().format("%H%M%S%3f").to_string();
        let clave: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let correo = |quien: &str| format!("usuarios-{quien}-{hora}@prueba.branchboard");
        let (ua, ub, invitado) = (correo("a"), correo("b"), correo("invitado"));
        let registrar = |correo: &str, nombre: &str| {
            let c = ClienteSupabase::new(&PRUEBAS, LlaveroMemoria::default());
            c.registrar(nombre, "Prueba", correo, &clave).unwrap();
            c
        };
        let (ca, cb) = (registrar(&ua, "Ana"), registrar(&ub, "Beto"));
        let p = ca
            .llamar(
                "POST",
                "/rest/v1/rpc/crear_proyecto",
                Some(&serde_json::json!({ "p_nombre": format!("bb-prueba-241-{hora}") })),
            )
            .unwrap();
        let p = p.as_str().unwrap().to_string();
        let a = EnSupabase {
            cliente: &ca,
            proyecto: &p,
        };
        let b = EnSupabase {
            cliente: &cb,
            proyecto: &p,
        };
        let (_da, db_a) = base();
        let (_db, db_b) = base();
        let (la, lb) = (db_a.conexion(), db_b.conexion());
        let guardar_a = |f: &[Usuario]| usuarios::reemplazar_todos(la, f);
        let guardar_b = |f: &[Usuario]| usuarios::reemplazar_todos(lb, f);

        // A agrega a B (tiene cuenta) e invita a alguien sin cuenta.
        let creado = crear(&a, guardar_a, &ub, Rol::Desarrollador, "2026-11-21").unwrap();
        assert_eq!(creado.rol, Rol::Desarrollador);
        crear(&a, guardar_a, &invitado, Rol::Tester, "2026-11-21").unwrap();
        let lista = listar(
            &a,
            guardar_a,
            || equipo::listar(la, Rol::Administrador),
            Rol::Administrador,
        )
        .unwrap();
        assert_eq!(lista.iter().filter(|u| !u.invitado).count(), 2);
        assert_eq!(lista.iter().filter(|u| u.invitado).count(), 1);
        assert_eq!(
            permisos::usuario_actual(la, &ua).unwrap().rol,
            Rol::Administrador
        );
        // Invitarlo otra vez se rechaza.
        let e = crear(&a, guardar_a, &invitado, Rol::Tester, "2026-11-21").unwrap_err();
        println!("A invita otra vez -> {e}");
        assert!(matches!(e, Error::Validacion(_)));

        // B (Desarrollador): el servidor le rechaza cambiar roles y agregar; no ve invitaciones.
        let lista_b = listar(
            &b,
            guardar_b,
            || equipo::listar(lb, Rol::Desarrollador),
            Rol::Desarrollador,
        )
        .unwrap();
        assert_eq!(lista_b.len(), 2);
        assert!(lista_b.iter().all(|u| !u.invitado && u.email.is_none()));
        let e = cambiar_rol(&b, guardar_b, &ua, Rol::Tester).unwrap_err();
        println!("B cambia el rol de A -> {e:?}");
        assert!(matches!(e, Error::SinPermiso));
        let e = cambiar_rol(&b, guardar_b, &ub, Rol::Administrador).unwrap_err();
        println!("B se da Administrador -> {e:?}");
        assert!(matches!(e, Error::SinPermiso));
        let e = crear(&b, guardar_b, &correo("otro"), Rol::Tester, "2026-11-21").unwrap_err();
        println!("B agrega a alguien -> {e:?}");
        assert!(matches!(e, Error::SinPermiso));
        assert_eq!(
            usuarios::buscar(lb, &ub).unwrap().unwrap().rol,
            Rol::Desarrollador
        );

        // El invitado se registra con ese correo: entra solo, con el rol de la invitación.
        let ci = registrar(&invitado, "Iván");
        let equipo_a = a.leer().unwrap();
        assert!(equipo_a.invitaciones.is_empty(), "la invitación se usó");
        let m = equipo_a
            .miembros
            .iter()
            .find(|m| m.usuario.correo == invitado)
            .expect("el invitado ya es miembro");
        assert_eq!(m.rol, Rol::Tester);
        let suyos = ci
            .llamar("GET", "/rest/v1/proyectos?select=id", None)
            .unwrap();
        assert_eq!(suyos, serde_json::json!([{ "id": p }]));

        // A nombra Administrador a B y se degrada; B, último Administrador, no se puede degradar.
        cambiar_rol(&a, guardar_a, &ub, Rol::Administrador).unwrap();
        cambiar_rol(&a, guardar_a, &ua, Rol::Lider).unwrap();
        let e = cambiar_rol(&b, guardar_b, &ub, Rol::Tester).unwrap_err();
        println!("B (último Administrador) se degrada -> {e:?}");
        assert!(matches!(e, Error::Validacion(_)));

        // B quita al que fue invitado y a A; A ya no ve el proyecto.
        eliminar(&b, guardar_b, &invitado).unwrap();
        eliminar(&b, guardar_b, &ua).unwrap();
        assert!(usuarios::buscar(lb, &ua).unwrap().is_none());
        let equipo_a = a.leer().unwrap();
        assert!(equipo_a.miembros.is_empty() && equipo_a.invitaciones.is_empty());

        for c in [&ca, &cb, &ci] {
            let _ = c.cerrar_sesion();
        }
    }
}
