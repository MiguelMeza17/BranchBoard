//! Gestión del equipo desde la pantalla Usuarios (issue #90). Ver "Comandos de
//! usuarios y ajustes" en BACKEND.md.
//!
//! Toda fila que se crea o cambia nace **firmada por quien la escribe** (el
//! Administrador, o un Líder certificado). Antes de guardar se comprueba que la
//! fila quede verificada; si no (ej. el Líder todavía no tiene certificado), no
//! se guarda nada.
//!
//! Por ahora escribe directo en el `.db` local. Cuando existan los eventos de
//! `ops/` (issues #91 a #93), estos cambios viajan como eventos administrativos
//! firmados.

use rusqlite::Connection;
use serde::Serialize;

use crate::db::usuarios::{self, nombre_desde_email, Usuario};
use crate::error::{Error, Resultado};
use crate::identidad::firmas::{self, verificar_usuarios, Verificacion, Verificador};
use crate::identidad::llaves::LlavePrivada;
use crate::modelo::{normalizar_email, Rol};

/// Roles que un Líder puede asignar.
const ROLES_BASE: &[Rol] = &[Rol::Desarrollador, Rol::Tester, Rol::ColaboradorExterno];
/// Roles que el Administrador puede asignar (nunca otro Administrador).
const ROLES_ADMIN: &[Rol] = &[
    Rol::Lider,
    Rol::Desarrollador,
    Rol::Tester,
    Rol::ColaboradorExterno,
];

/// Quien firma un cambio: su correo, su rol verificado y su llave privada.
pub struct Firmante<'a> {
    pub email: &'a str,
    pub rol: Rol,
    pub llave: &'a LlavePrivada,
}

/// Roles que `actor` puede darle a una persona con rol `actual` (`None` = alta
/// nueva). Vacío = no puede cambiarla. Igual que `rolesAsignablesA` del frontend.
pub fn roles_asignables(actor: Rol, actual: Option<Rol>) -> &'static [Rol] {
    match (actor, actual) {
        (Rol::Administrador, Some(Rol::Administrador)) => &[],
        (Rol::Administrador, _) => ROLES_ADMIN,
        (Rol::Lider, None) => ROLES_BASE,
        (Rol::Lider, Some(rol)) if ROLES_BASE.contains(&rol) => ROLES_BASE,
        _ => &[],
    }
}

/// `true` si `actor` puede eliminar a alguien con rol `objetivo`. Nadie elimina
/// al Administrador: es el ancla de las firmas del proyecto.
pub fn puede_eliminar(actor: Rol, objetivo: Rol) -> bool {
    match actor {
        Rol::Administrador => objetivo != Rol::Administrador,
        Rol::Lider => matches!(objetivo, Rol::Desarrollador | Rol::ColaboradorExterno),
        _ => false,
    }
}

/// Una fila de la pantalla Usuarios. Sin correo para quien no es Líder ni
/// Administrador.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsuarioVisible {
    pub email: Option<String>,
    pub nombre: String,
    pub apellido: String,
    /// El rol guardado en la fila.
    pub rol: Rol,
    /// El que se le aplica: Desarrollador si su firma no coincide, `None` si
    /// está pendiente de confirmar.
    pub rol_efectivo: Option<Rol>,
    pub verificacion: Verificacion,
    pub fecha_alta: String,
}

/// `listar_usuarios`: todos los roles; solo Líder y Administrador ven correos.
pub fn listar(
    c: &Connection,
    publica_admin: &str,
    rol_de_quien_pide: Rol,
) -> Resultado<Vec<UsuarioVisible>> {
    let ve_correos = rol_de_quien_pide.es_lider_o_admin();
    Ok(verificar_usuarios(publica_admin, usuarios::listar(c)?)
        .into_iter()
        .map(|v| UsuarioVisible {
            email: ve_correos.then(|| v.usuario.email.clone()),
            nombre: v.usuario.nombre,
            apellido: v.usuario.apellido,
            rol: v.usuario.rol,
            rol_efectivo: v.rol_efectivo,
            verificacion: v.verificacion,
            fecha_alta: v.usuario.fecha_alta,
        })
        .collect())
}

/// Firma `email|rol|firmado_en` con la llave de quien firma.
fn firmar(firmante: &Firmante<'_>, email: &str, rol: Rol, ahora: &str) -> Resultado<String> {
    match firmante.rol {
        Rol::Administrador => firmas::firmar_rol(firmante.llave, email, rol, ahora),
        Rol::Lider => firmas::firmar_rol_como_lider(firmante.llave, email, rol, ahora),
        _ => Err(Error::SinPermiso),
    }
}

/// Después de escribir, la fila de `email` tiene que quedar verificada; si no,
/// la llave de quien firma no es la que el proyecto reconoce.
fn exigir_verificada(c: &Connection, publica_admin: &str, email: &str) -> Resultado<()> {
    let tabla = usuarios::listar(c)?;
    let fila = tabla
        .iter()
        .find(|u| u.email == email)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    if Verificador::nuevo(publica_admin, &tabla).verificar_firma(fila) == Verificacion::Verificado {
        Ok(())
    } else {
        Err(Error::Validacion(
            "Tu llave de firma no está certificada por el Administrador de este proyecto".into(),
        ))
    }
}

/// La fila de `email` y el rol que cuenta para las reglas: el verificado, o el
/// guardado si está pendiente de confirmar.
fn buscar_con_rol(c: &Connection, publica_admin: &str, email: &str) -> Resultado<(Usuario, Rol)> {
    let tabla = usuarios::listar(c)?;
    let verificador = Verificador::nuevo(publica_admin, &tabla);
    let fila = tabla
        .into_iter()
        .find(|u| u.email == email)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    let v = verificador.verificar_usuario(fila);
    let rol = v.rol_efectivo.unwrap_or(v.usuario.rol);
    Ok((v.usuario, rol))
}

/// `crear_usuario`: alta sin contraseña; nombre y apellido salen del correo.
pub fn crear(
    c: &Connection,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    rol: Rol,
    ahora: &str,
    hoy: &str,
) -> Resultado<Usuario> {
    if !roles_asignables(firmante.rol, None).contains(&rol) {
        return Err(Error::Validacion(format!(
            "No puedes crear usuarios con el rol {rol}"
        )));
    }
    let email = normalizar_email(email);
    let (nombre, apellido) = nombre_desde_email(&email);
    let nuevo = Usuario {
        email: email.clone(),
        nombre,
        apellido,
        rol,
        fecha_alta: hoy.to_string(),
        firma: Some(firmar(firmante, &email, rol, ahora)?),
        firmado_en: Some(ahora.to_string()),
        firmado_por: Some(normalizar_email(firmante.email)),
        llave_publica: None,
        certificado: None,
    };
    let tx = c.unchecked_transaction()?;
    usuarios::insertar(&tx, &nuevo)?;
    exigir_verificada(&tx, publica_admin, &email)?;
    tx.commit()?;
    Ok(nuevo)
}

/// `cambiar_rol`. Al quitarle el rol a un Líder **no** se borran su llave
/// pública ni su certificado: así lo que firmó antes sigue verificándose.
pub fn cambiar_rol(
    c: &Connection,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    rol_nuevo: Rol,
    ahora: &str,
) -> Resultado<()> {
    let email = normalizar_email(email);
    if email == normalizar_email(firmante.email) {
        return Err(Error::Validacion("No puedes cambiar tu propio rol".into()));
    }
    let (fila, actual) = buscar_con_rol(c, publica_admin, &email)?;
    if !roles_asignables(firmante.rol, Some(actual)).contains(&rol_nuevo) {
        return Err(Error::SinPermiso);
    }
    if fila.rol == rol_nuevo && fila.firma.is_some() && actual == fila.rol {
        return Err(Error::Validacion(format!(
            "{email} ya tiene el rol {rol_nuevo}"
        )));
    }
    let firma = firmar(firmante, &email, rol_nuevo, ahora)?;
    let tx = c.unchecked_transaction()?;
    usuarios::actualizar_rol(
        &tx,
        &email,
        rol_nuevo,
        Some(&firma),
        Some(ahora),
        Some(firmante.email),
    )?;
    exigir_verificada(&tx, publica_admin, &email)?;
    tx.commit()?;
    Ok(())
}

/// `confirmar_usuario` (solo Administrador): firma una fila **sin firma**. Una
/// fila con firma que no coincide no se confirma (podría ser un rol escrito a
/// mano): para esas, `cambiar_rol`.
pub fn confirmar(
    c: &Connection,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    ahora: &str,
) -> Resultado<()> {
    if firmante.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }
    let email = normalizar_email(email);
    let tabla = usuarios::listar(c)?;
    let fila = tabla
        .iter()
        .find(|u| u.email == email)
        .ok_or_else(|| Error::NoEncontrado(format!("el usuario {email}")))?;
    if Verificador::nuevo(publica_admin, &tabla).verificar_firma(fila) != Verificacion::Pendiente {
        return Err(Error::Validacion(format!(
            "{email} no está pendiente de confirmar"
        )));
    }
    let firma = firmar(firmante, &email, fila.rol, ahora)?;
    let tx = c.unchecked_transaction()?;
    usuarios::actualizar_rol(
        &tx,
        &email,
        fila.rol,
        Some(&firma),
        Some(ahora),
        Some(firmante.email),
    )?;
    exigir_verificada(&tx, publica_admin, &email)?;
    tx.commit()?;
    Ok(())
}

/// `eliminar_usuario`. Si se elimina a un Líder, el Administrador vuelve a
/// firmar las filas que ese Líder firmó (sin su fila ya no se podrían
/// verificar). Devuelve cuántas filas volvió a firmar.
///
/// No le quita el acceso a la carpeta de Drive/OneDrive: eso lo tiene que hacer
/// el equipo a mano (la interfaz lo recuerda al confirmar).
pub fn eliminar(
    c: &Connection,
    publica_admin: &str,
    firmante: &Firmante<'_>,
    email: &str,
    ahora: &str,
) -> Resultado<usize> {
    let email = normalizar_email(email);
    if email == normalizar_email(firmante.email) {
        return Err(Error::Validacion("No puedes eliminarte a ti mismo".into()));
    }
    let (_, rol) = buscar_con_rol(c, publica_admin, &email)?;
    if !puede_eliminar(firmante.rol, rol) {
        return Err(Error::SinPermiso);
    }

    // Filas que hoy valen por la firma de esta persona como Líder.
    let tabla = usuarios::listar(c)?;
    let verificador = Verificador::nuevo(publica_admin, &tabla);
    let firmadas_por_el: Vec<&Usuario> = tabla
        .iter()
        .filter(|u| u.email != email)
        .filter(|u| {
            u.firmado_por.as_deref().map(normalizar_email).as_deref() == Some(email.as_str())
        })
        .filter(|u| verificador.verificar_firma(u) == Verificacion::Verificado)
        .collect();
    if !firmadas_por_el.is_empty() && firmante.rol != Rol::Administrador {
        return Err(Error::SinPermiso);
    }

    let tx = c.unchecked_transaction()?;
    for fila in &firmadas_por_el {
        let firma = firmar(firmante, &fila.email, fila.rol, ahora)?;
        usuarios::actualizar_rol(
            &tx,
            &fila.email,
            fila.rol,
            Some(&firma),
            Some(ahora),
            Some(firmante.email),
        )?;
    }
    usuarios::eliminar(&tx, &email)?;
    for fila in &firmadas_por_el {
        exigir_verificada(&tx, publica_admin, &fila.email)?;
    }
    tx.commit()?;
    Ok(firmadas_por_el.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::identidad::llaves;

    const ANTES: &str = "2026-11-01T09:00:00Z";
    const AHORA: &str = "2026-11-10T10:00:00Z";
    const HOY: &str = "2026-11-10";
    const ADMIN: &str = "admin@virustf.dev";
    const LIDER: &str = "lider@virustf.dev";

    struct Proyecto {
        _dirs: (tempfile::TempDir, tempfile::TempDir),
        db: Db,
        publica: String,
        admin: LlavePrivada,
        lider: LlavePrivada,
    }

    impl Proyecto {
        fn c(&self) -> &Connection {
            self.db.conexion()
        }
        fn como_admin(&self) -> Firmante<'_> {
            Firmante {
                email: ADMIN,
                rol: Rol::Administrador,
                llave: &self.admin,
            }
        }
        fn como_lider(&self) -> Firmante<'_> {
            Firmante {
                email: LIDER,
                rol: Rol::Lider,
                llave: &self.lider,
            }
        }
        fn rol(&self, email: &str) -> (Rol, Option<Rol>, Verificacion) {
            let lista = listar(self.c(), &self.publica, Rol::Administrador).unwrap();
            let u = lista
                .into_iter()
                .find(|u| u.email.as_deref() == Some(email))
                .unwrap();
            (u.rol, u.rol_efectivo, u.verificacion)
        }
    }

    /// Administrador y un Líder certificado, los dos firmados por el Administrador.
    fn proyecto() -> Proyecto {
        let dir_admin = tempfile::tempdir().unwrap();
        let dir_lider = tempfile::tempdir().unwrap();
        let publica = llaves::generar_llaves_admin(dir_admin.path(), None).unwrap();
        let admin = llaves::cargar_llave_admin(dir_admin.path(), None).unwrap();
        let publica_lider = llaves::generar_llaves_lider(dir_lider.path(), None).unwrap();
        let lider = llaves::cargar_llave_lider(dir_lider.path(), None).unwrap();
        let db = Db::en_memoria().unwrap();
        let fila = |email: &str, rol: Rol| Usuario {
            email: email.into(),
            nombre: "N".into(),
            apellido: "A".into(),
            rol,
            fecha_alta: HOY.into(),
            firma: Some(firmas::firmar_rol(&admin, email, rol, ANTES).unwrap()),
            firmado_en: Some(ANTES.into()),
            firmado_por: Some(ADMIN.into()),
            llave_publica: None,
            certificado: None,
        };
        usuarios::insertar(db.conexion(), &fila(ADMIN, Rol::Administrador)).unwrap();
        usuarios::insertar(
            db.conexion(),
            &Usuario {
                llave_publica: Some(publica_lider.clone()),
                certificado: Some(
                    firmas::certificar_lider(&admin, LIDER, &publica_lider, ANTES).unwrap(),
                ),
                ..fila(LIDER, Rol::Lider)
            },
        )
        .unwrap();
        Proyecto {
            _dirs: (dir_admin, dir_lider),
            db,
            publica,
            admin,
            lider,
        }
    }

    #[test]
    fn roles_asignables_como_en_el_frontend() {
        use Rol::*;
        assert_eq!(roles_asignables(Administrador, None), ROLES_ADMIN);
        assert!(roles_asignables(Administrador, Some(Administrador)).is_empty());
        assert_eq!(roles_asignables(Lider, None), ROLES_BASE);
        assert_eq!(roles_asignables(Lider, Some(Tester)), ROLES_BASE);
        assert!(roles_asignables(Lider, Some(Lider)).is_empty());
        assert!(roles_asignables(Desarrollador, None).is_empty());
        assert!(puede_eliminar(Administrador, Lider));
        assert!(!puede_eliminar(Administrador, Administrador));
        assert!(puede_eliminar(Lider, ColaboradorExterno));
        assert!(!puede_eliminar(Lider, Tester));
        assert!(!puede_eliminar(Tester, Desarrollador));
    }

    #[test]
    fn crear_nace_firmado_por_quien_lo_crea() {
        let p = proyecto();
        let u = crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "Ana.Lopez@VirusTF.dev",
            Rol::Lider,
            AHORA,
            HOY,
        )
        .unwrap();
        assert_eq!(
            (u.email.as_str(), u.nombre.as_str(), u.apellido.as_str()),
            ("ana.lopez@virustf.dev", "Ana", "Lopez")
        );
        assert_eq!(
            p.rol("ana.lopez@virustf.dev"),
            (Rol::Lider, Some(Rol::Lider), Verificacion::Verificado)
        );

        crear(
            p.c(),
            &p.publica,
            &p.como_lider(),
            "beto@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();
        assert_eq!(p.rol("beto@virustf.dev").2, Verificacion::Verificado);
    }

    #[test]
    fn crear_rechaza_correo_repetido_y_rol_no_permitido() {
        let p = proyecto();
        crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();
        let repetido = crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "ANA@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        );
        assert_eq!(repetido.unwrap_err().codigo(), "validacion");
        for rol in [Rol::Lider, Rol::Administrador] {
            assert!(crear(
                p.c(),
                &p.publica,
                &p.como_lider(),
                "x@virustf.dev",
                rol,
                AHORA,
                HOY
            )
            .is_err());
        }
        assert!(crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "y@virustf.dev",
            Rol::Administrador,
            AHORA,
            HOY
        )
        .is_err());
        assert_eq!(
            usuarios::listar(p.c()).unwrap().len(),
            3,
            "no quedó nada a medias"
        );
    }

    #[test]
    fn un_lider_sin_certificado_no_guarda_nada() {
        let p = proyecto();
        let dir = tempfile::tempdir().unwrap();
        llaves::generar_llaves_lider(dir.path(), None).unwrap();
        let otra = llaves::cargar_llave_lider(dir.path(), None).unwrap();
        let impostor = Firmante {
            email: LIDER,
            rol: Rol::Lider,
            llave: &otra,
        };
        let error = crear(
            p.c(),
            &p.publica,
            &impostor,
            "x@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap_err();
        assert_eq!(error.codigo(), "validacion");
        assert!(usuarios::buscar(p.c(), "x@virustf.dev").unwrap().is_none());
    }

    #[test]
    fn cambiar_rol_respeta_quien_cambia_a_quien() {
        let p = proyecto();
        crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();

        cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_lider(),
            "ana@virustf.dev",
            Rol::Desarrollador,
            AHORA,
        )
        .unwrap();
        assert_eq!(p.rol("ana@virustf.dev").1, Some(Rol::Desarrollador));
        // El Líder no da el rol de Líder, ni toca a otro Líder ni al Administrador.
        assert!(cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_lider(),
            "ana@virustf.dev",
            Rol::Lider,
            AHORA
        )
        .is_err());
        assert!(cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_lider(),
            ADMIN,
            Rol::Tester,
            AHORA
        )
        .is_err());
        // Nadie cambia su propio rol ni le da a otro el de Administrador.
        assert!(cambiar_rol(p.c(), &p.publica, &p.como_admin(), ADMIN, Rol::Lider, AHORA).is_err());
        assert!(cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Administrador,
            AHORA
        )
        .is_err());
        // Mismo rol que ya tiene.
        assert!(cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "ana@virustf.dev",
            Rol::Desarrollador,
            AHORA
        )
        .is_err());
        assert!(cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "nadie@virustf.dev",
            Rol::Tester,
            AHORA
        )
        .is_err());
    }

    #[test]
    fn quitarle_el_rol_a_un_lider_conserva_lo_que_firmo() {
        let p = proyecto();
        crear(
            p.c(),
            &p.publica,
            &p.como_lider(),
            "beto@virustf.dev",
            Rol::Tester,
            "2026-11-05T10:00:00Z",
            HOY,
        )
        .unwrap();
        cambiar_rol(
            p.c(),
            &p.publica,
            &p.como_admin(),
            LIDER,
            Rol::Desarrollador,
            AHORA,
        )
        .unwrap();
        let fila = usuarios::buscar(p.c(), LIDER).unwrap().unwrap();
        assert!(fila.llave_publica.is_some() && fila.certificado.is_some());
        assert_eq!(p.rol("beto@virustf.dev").2, Verificacion::Verificado);
    }

    #[test]
    fn confirmar_solo_filas_sin_firma() {
        let p = proyecto();
        let sin_firma = Usuario {
            firma: None,
            firmado_en: None,
            firmado_por: None,
            ..crear(
                p.c(),
                &p.publica,
                &p.como_admin(),
                "ana@virustf.dev",
                Rol::Tester,
                AHORA,
                HOY,
            )
            .unwrap()
        };
        usuarios::eliminar(p.c(), "ana@virustf.dev").unwrap();
        usuarios::insertar(p.c(), &sin_firma).unwrap();
        assert_eq!(p.rol("ana@virustf.dev").2, Verificacion::Pendiente);

        assert_eq!(
            confirmar(p.c(), &p.publica, &p.como_lider(), "ana@virustf.dev", AHORA)
                .unwrap_err()
                .codigo(),
            "sin_permiso"
        );
        confirmar(p.c(), &p.publica, &p.como_admin(), "ana@virustf.dev", AHORA).unwrap();
        assert_eq!(
            p.rol("ana@virustf.dev"),
            (Rol::Tester, Some(Rol::Tester), Verificacion::Verificado)
        );
        assert!(confirmar(p.c(), &p.publica, &p.como_admin(), "ana@virustf.dev", AHORA).is_err());

        // Rol escrito a mano sobre una fila firmada: no se confirma.
        usuarios::actualizar_rol(
            p.c(),
            "ana@virustf.dev",
            Rol::Lider,
            Some("x"),
            Some(AHORA),
            None,
        )
        .unwrap();
        assert!(confirmar(p.c(), &p.publica, &p.como_admin(), "ana@virustf.dev", AHORA).is_err());
    }

    #[test]
    fn eliminar_respeta_quien_elimina_a_quien() {
        let p = proyecto();
        crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "dev@virustf.dev",
            Rol::Desarrollador,
            AHORA,
            HOY,
        )
        .unwrap();
        crear(
            p.c(),
            &p.publica,
            &p.como_admin(),
            "qa@virustf.dev",
            Rol::Tester,
            AHORA,
            HOY,
        )
        .unwrap();

        assert!(eliminar(p.c(), &p.publica, &p.como_lider(), "qa@virustf.dev", AHORA).is_err());
        assert!(eliminar(p.c(), &p.publica, &p.como_lider(), ADMIN, AHORA).is_err());
        assert!(eliminar(p.c(), &p.publica, &p.como_admin(), ADMIN, AHORA).is_err());
        assert_eq!(
            eliminar(p.c(), &p.publica, &p.como_lider(), "dev@virustf.dev", AHORA).unwrap(),
            0
        );
        assert!(usuarios::buscar(p.c(), "dev@virustf.dev")
            .unwrap()
            .is_none());
        eliminar(p.c(), &p.publica, &p.como_admin(), "qa@virustf.dev", AHORA).unwrap();
        assert!(eliminar(p.c(), &p.publica, &p.como_admin(), "qa@virustf.dev", AHORA).is_err());
    }

    #[test]
    fn eliminar_a_un_lider_vuelve_a_firmar_lo_suyo() {
        let p = proyecto();
        crear(
            p.c(),
            &p.publica,
            &p.como_lider(),
            "beto@virustf.dev",
            Rol::Tester,
            "2026-11-05T10:00:00Z",
            HOY,
        )
        .unwrap();
        crear(
            p.c(),
            &p.publica,
            &p.como_lider(),
            "caro@virustf.dev",
            Rol::Desarrollador,
            "2026-11-05T10:00:00Z",
            HOY,
        )
        .unwrap();

        assert_eq!(
            eliminar(p.c(), &p.publica, &p.como_admin(), LIDER, AHORA).unwrap(),
            2
        );
        assert!(usuarios::buscar(p.c(), LIDER).unwrap().is_none());
        for email in ["beto@virustf.dev", "caro@virustf.dev"] {
            assert_eq!(p.rol(email).2, Verificacion::Verificado, "{email}");
            let fila = usuarios::buscar(p.c(), email).unwrap().unwrap();
            assert_eq!(fila.firmado_por.as_deref(), Some(ADMIN));
        }
    }

    #[test]
    fn listar_esconde_los_correos_a_quien_no_es_lider_ni_admin() {
        let p = proyecto();
        for (rol, ve) in [
            (Rol::Administrador, true),
            (Rol::Lider, true),
            (Rol::Tester, false),
            (Rol::Desarrollador, false),
            (Rol::ColaboradorExterno, false),
        ] {
            let lista = listar(p.c(), &p.publica, rol).unwrap();
            assert_eq!(lista.len(), 2);
            assert!(lista.iter().all(|u| u.email.is_some() == ve), "{rol}");
        }
        let json = serde_json::to_value(listar(p.c(), &p.publica, Rol::Tester).unwrap()).unwrap();
        assert!(json[0]["email"].is_null());
        assert_eq!(json[0]["verificacion"], "verificado");
    }
}
