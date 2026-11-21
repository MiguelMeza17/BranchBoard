//! Datos para las gráficas de Avance: Sprint (burndown + ideal + proyección),
//! Backlog (restantes por día + ritmo de 4 semanas + cierre estimado) y General
//! (todas las tareas que están en algún sprint o backlog) (issue #115).
//!
//! Todo se calcula en SQLite con `fecha_creacion` y `fecha_completada`, sin `cm`.

use chrono::{Days, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::sprints as db_sprints;
use crate::db::tareas::{self as db_tareas, Tarea};
use crate::error::{Error, Resultado};
use crate::sprints::sumar_dias;

const VENTANA_RITMO_DIAS: u64 = 28;
const MAX_PROYECCION_SPRINT: f64 = 90.0;
const MAX_PROYECCION_BACKLOG: f64 = 365.0;

/// Proyección lineal (`y = pendiente * x + intercepto`) de un sprint con al menos
/// dos días transcurridos (`x = 0` es `fecha_inicio`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProyeccionSprint {
    pub pendiente: f64,
    pub intercepto: f64,
    pub dia_cierre_x: Option<f64>,
    pub fecha_cierre_estimada: Option<String>,
    pub a_tiempo: bool,
    pub tiene_avance: bool,
}

/// Datos de la gráfica de Avance para un sprint (`con_fecha = true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvanceSprint {
    pub sprint_id: String,
    pub nombre: String,
    pub cerrado: bool,
    pub por_empezar: bool,
    pub total_tareas: usize,
    pub terminadas: usize,
    pub restantes_actuales: usize,
    /// Todos los días calendario desde `fecha_inicio` hasta `fecha_fin`.
    pub dias: Vec<String>,
    /// Tareas restantes por cada día transcurrido (vacío si `por_empezar`).
    pub restantes_por_dia: Vec<usize>,
    /// Línea recta ideal de `total_tareas` (día 0) a `0.0` (último día).
    pub linea_ideal: Vec<f64>,
    pub proyeccion: Option<ProyeccionSprint>,
}

/// Ritmo y flujo semanal medidos en las últimas 4 semanas (`28` días).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RitmoBacklog {
    pub entraron_4_semanas: usize,
    pub salieron_4_semanas: usize,
    pub entran_por_semana: f64,
    pub salen_por_semana: f64,
    pub creciendo: bool,
}

/// Datos de la gráfica de Avance para un backlog (`con_fecha = false`) o para la
/// vista `"General"` (todas las tareas en algún sprint o backlog).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvanceBacklog {
    pub id: String,
    pub nombre: String,
    pub es_general: bool,
    pub total_tareas: usize,
    pub terminadas: usize,
    pub restantes_actuales: usize,
    pub terminado: bool,
    pub fecha_ultimo_cierre: Option<String>,
    pub dias: Vec<String>,
    pub restantes_por_dia: Vec<usize>,
    pub ritmo: RitmoBacklog,
    pub dias_faltan_estimados: Option<u32>,
    pub fecha_cierre_estimada: Option<String>,
}

fn parsear_fecha(fecha: &str) -> Resultado<NaiveDate> {
    NaiveDate::parse_from_str(fecha.trim(), "%Y-%m-%d")
        .map_err(|_| Error::Validacion(format!("Fecha no válida: {fecha}")))
}

fn listar_dias(inicio: &str, fin: &str) -> Resultado<Vec<String>> {
    let mut cur = parsear_fecha(inicio)?;
    let end = parsear_fecha(fin)?;
    let mut dias = Vec::new();
    while cur <= end {
        dias.push(cur.format("%Y-%m-%d").to_string());
        let Some(sig) = cur.checked_add_days(Days::new(1)) else {
            break;
        };
        cur = sig;
    }
    Ok(dias)
}

fn restar_dias(fecha: &str, dias: u64) -> Resultado<String> {
    let base = parsear_fecha(fecha)?;
    let res = base
        .checked_sub_days(Days::new(dias))
        .ok_or_else(|| Error::Validacion("Fecha fuera de rango".into()))?;
    Ok(res.format("%Y-%m-%d").to_string())
}

fn calcular_linea_ideal(n_dias: usize, total: usize) -> Vec<f64> {
    if n_dias == 0 {
        return Vec::new();
    }
    let ultimo = n_dias.saturating_sub(1);
    if ultimo == 0 {
        return vec![total as f64];
    }
    let denom = ultimo as f64;
    let total_f = total as f64;
    (0..n_dias)
        .map(|i| total_f * (1.0 - (i as f64) / denom))
        .collect()
}

fn calcular_regresion_sprint(
    dias: &[String],
    restantes: &[usize],
    fecha_inicio: &str,
) -> Resultado<Option<ProyeccionSprint>> {
    let n_puntos = restantes.len();
    if n_puntos < 2 {
        return Ok(None);
    }
    let n = n_puntos as f64;
    let sum_x: f64 = (0..n_puntos).map(|i| i as f64).sum();
    let sum_y: f64 = restantes.iter().map(|&y| y as f64).sum();
    let sum_xy: f64 = restantes
        .iter()
        .enumerate()
        .map(|(i, &y)| (i as f64) * (y as f64))
        .sum();
    let sum_x2: f64 = (0..n_puntos).map(|i| (i as f64) * (i as f64)).sum();
    let denom = n * sum_x2 - sum_x * sum_x;
    if denom.abs() <= f64::EPSILON {
        return Ok(None);
    }
    let pendiente = (n * sum_xy - sum_x * sum_y) / denom;
    let intercepto = (sum_y - pendiente * sum_x) / n;

    let ultimo = dias.len().saturating_sub(1);
    if pendiente < 0.0 {
        let max_x = (ultimo as f64) + MAX_PROYECCION_SPRINT;
        let x_zero = (-intercepto / pendiente).max(0.0).min(max_x);
        let dia_fin = (x_zero - 1e-9).ceil().max(0.0) as u32;
        let fecha_cierre = sumar_dias(fecha_inicio, dia_fin)?;
        let a_tiempo = (dia_fin as usize) <= ultimo;
        Ok(Some(ProyeccionSprint {
            pendiente,
            intercepto,
            dia_cierre_x: Some(x_zero),
            fecha_cierre_estimada: Some(fecha_cierre),
            a_tiempo,
            tiene_avance: true,
        }))
    } else {
        Ok(Some(ProyeccionSprint {
            pendiente,
            intercepto,
            dia_cierre_x: None,
            fecha_cierre_estimada: None,
            a_tiempo: false,
            tiene_avance: false,
        }))
    }
}

/// Calcula los datos de la gráfica de Avance de un sprint (`con_fecha = true`, #115).
/// Si el sprint ya está cerrado usa la copia guardada al cierre (`reportes_sprint`).
pub fn datos_sprint(c: &Connection, sprint_id: &str, hoy: &str) -> Resultado<AvanceSprint> {
    let sprint = db_sprints::obtener(c, sprint_id.trim())?;
    if !sprint.con_fecha {
        return Err(Error::Validacion(format!(
            "{} es un backlog, no un sprint con fecha",
            sprint.id
        )));
    }
    let Some(ref fecha_fin) = sprint.fecha_fin else {
        return Err(Error::Validacion(format!(
            "El sprint {} no tiene fecha de fin",
            sprint.id
        )));
    };

    let tareas: Vec<Tarea> = if sprint.cerrado {
        if let Some(rep) = db_sprints::ultimo_reporte(c, &sprint.id)? {
            rep.tareas
        } else {
            db_tareas::listar(c)?
                .into_iter()
                .filter(|t| !t.eliminada && t.sprint_id.as_deref() == Some(sprint.id.as_str()))
                .collect()
        }
    } else {
        db_tareas::listar(c)?
            .into_iter()
            .filter(|t| !t.eliminada && t.sprint_id.as_deref() == Some(sprint.id.as_str()))
            .collect()
    };

    let dias = listar_dias(&sprint.fecha_inicio, fecha_fin)?;
    let total_tareas = tareas.len();
    let linea_ideal = calcular_linea_ideal(dias.len(), total_tareas);

    let tope = if sprint.cerrado {
        sprint.fecha_cierre_real.as_deref().unwrap_or(hoy)
    } else {
        hoy
    };
    let por_empezar = !sprint.cerrado && tope < sprint.fecha_inicio.as_str();
    let n_puntos = if por_empezar {
        0
    } else {
        dias.iter().filter(|d| d.as_str() <= tope).count()
    };

    let restantes_por_dia: Vec<usize> = dias
        .iter()
        .take(n_puntos)
        .map(|dia| {
            let hechas = tareas
                .iter()
                .filter(|t| {
                    t.fecha_completada
                        .as_deref()
                        .is_some_and(|fc| fc <= dia.as_str())
                })
                .count();
            total_tareas.saturating_sub(hechas)
        })
        .collect();

    let restantes_actuales = restantes_por_dia.last().copied().unwrap_or(total_tareas);
    let terminadas = total_tareas.saturating_sub(restantes_actuales);
    let proyeccion = if por_empezar {
        None
    } else {
        calcular_regresion_sprint(&dias, &restantes_por_dia, &sprint.fecha_inicio)?
    };

    Ok(AvanceSprint {
        sprint_id: sprint.id,
        nombre: sprint.nombre,
        cerrado: sprint.cerrado,
        por_empezar,
        total_tareas,
        terminadas,
        restantes_actuales,
        dias,
        restantes_por_dia,
        linea_ideal,
        proyeccion,
    })
}

fn calcular_serie_backlog(
    id: &str,
    nombre: &str,
    es_general: bool,
    fecha_inicio_base: &str,
    tareas: &[Tarea],
    hoy: &str,
) -> Resultado<AvanceBacklog> {
    let primera = tareas
        .iter()
        .map(|t| t.fecha_creacion.as_str())
        .min()
        .map(|m| {
            if m < fecha_inicio_base {
                m
            } else {
                fecha_inicio_base
            }
        })
        .unwrap_or(fecha_inicio_base);
    let fin = if hoy < primera { primera } else { hoy };
    let dias = listar_dias(primera, fin)?;

    let restantes_por_dia: Vec<usize> = dias
        .iter()
        .map(|d| {
            let creadas = tareas
                .iter()
                .filter(|t| t.fecha_creacion.as_str() <= d.as_str())
                .count();
            let hechas = tareas
                .iter()
                .filter(|t| {
                    t.fecha_completada
                        .as_deref()
                        .is_some_and(|fc| fc <= d.as_str())
                })
                .count();
            creadas.saturating_sub(hechas)
        })
        .collect();

    let total_tareas = tareas.len();
    let terminadas_hoy = tareas
        .iter()
        .filter(|t| t.fecha_completada.as_deref().is_some_and(|fc| fc <= hoy))
        .count();
    let restantes_actuales = total_tareas.saturating_sub(terminadas_hoy);
    let terminado = total_tareas > 0 && restantes_actuales == 0;
    let fecha_ultimo_cierre = tareas
        .iter()
        .filter_map(|t| t.fecha_completada.as_deref())
        .max()
        .map(str::to_string);

    let desde = restar_dias(hoy, VENTANA_RITMO_DIAS)?;
    let salieron_4_semanas = tareas
        .iter()
        .filter(|t| {
            t.fecha_completada
                .as_deref()
                .is_some_and(|fc| fc > desde.as_str() && fc <= hoy)
        })
        .count();
    let entraron_4_semanas = tareas
        .iter()
        .filter(|t| t.fecha_creacion.as_str() > desde.as_str() && t.fecha_creacion.as_str() <= hoy)
        .count();
    let salen_por_semana = (salieron_4_semanas as f64) * 7.0 / (VENTANA_RITMO_DIAS as f64);
    let entran_por_semana = (entraron_4_semanas as f64) * 7.0 / (VENTANA_RITMO_DIAS as f64);
    let creciendo = entraron_4_semanas > salieron_4_semanas;

    let ritmo_diario = (salieron_4_semanas as f64) / (VENTANA_RITMO_DIAS as f64);
    let (dias_faltan_estimados, fecha_cierre_estimada) = if terminado {
        (Some(0), fecha_ultimo_cierre.clone())
    } else if ritmo_diario > 0.0 && restantes_actuales > 0 {
        let dias_faltan = ((restantes_actuales as f64) / ritmo_diario)
            .ceil()
            .min(MAX_PROYECCION_BACKLOG) as u32;
        (Some(dias_faltan), Some(sumar_dias(hoy, dias_faltan)?))
    } else {
        (None, None)
    };

    Ok(AvanceBacklog {
        id: id.to_string(),
        nombre: nombre.to_string(),
        es_general,
        total_tareas,
        terminadas: terminadas_hoy,
        restantes_actuales,
        terminado,
        fecha_ultimo_cierre,
        dias,
        restantes_por_dia,
        ritmo: RitmoBacklog {
            entraron_4_semanas,
            salieron_4_semanas,
            entran_por_semana,
            salen_por_semana,
            creciendo,
        },
        dias_faltan_estimados,
        fecha_cierre_estimada,
    })
}

/// Calcula los datos de la gráfica de Avance de un backlog (`con_fecha = false`, #115).
pub fn datos_backlog(c: &Connection, backlog_id: &str, hoy: &str) -> Resultado<AvanceBacklog> {
    let backlog = db_sprints::obtener(c, backlog_id.trim())?;
    if backlog.con_fecha {
        return Err(Error::Validacion(format!(
            "{} es un sprint con fecha, no un backlog",
            backlog.id
        )));
    }
    let tareas: Vec<Tarea> = db_tareas::listar(c)?
        .into_iter()
        .filter(|t| !t.eliminada && t.backlog_id.as_deref() == Some(backlog.id.as_str()))
        .collect();
    calcular_serie_backlog(
        &backlog.id,
        &backlog.nombre,
        false,
        &backlog.fecha_inicio,
        &tareas,
        hoy,
    )
}

/// Calcula los datos de la vista **General** en Avance: todas las tareas activas que
/// pertenecen a algún sprint o backlog (cada una contada una sola vez, #115).
pub fn datos_general(c: &Connection, hoy: &str) -> Resultado<AvanceBacklog> {
    let sprints = db_sprints::listar(c)?;
    let inicio_min = sprints
        .iter()
        .map(|s| s.fecha_inicio.as_str())
        .min()
        .unwrap_or(hoy);
    let tareas: Vec<Tarea> = db_tareas::listar(c)?
        .into_iter()
        .filter(|t| !t.eliminada && (t.sprint_id.is_some() || t.backlog_id.is_some()))
        .collect();
    calcular_serie_backlog("general", "General", true, inicio_min, &tareas, hoy)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{DateTime, Utc};
    use tempfile::TempDir;

    use super::*;
    use crate::db::{usuarios, Db};
    use crate::eventos::carpeta_ops;
    use crate::eventos::configurar_carpeta_compartida;
    use crate::eventos::lectura::{self, sincronizar};

    use crate::modelo::{EstadoTarea, Rol, TipoTarea};
    use crate::sprints::{
        asignar_tarea_a_backlog, asignar_tarea_a_sprint, cerrar_sprint, crear_sprint,
        DatosNuevoSprint,
    };
    use crate::tareas::{cambiar_estado, crear, AutorTarea, DatosNuevaTarea};

    const ADMIN: &str = "admin@estudio.com";
    const LIDER: &str = "lider@estudio.com";
    const HOY_INICIO: &str = "2026-11-01";

    struct Entorno {
        _tmp: TempDir,
        db_a: Db,
        ops: PathBuf,
        db_b: Db,
        local_b: PathBuf,
    }

    fn setup() -> Entorno {
        let tmp = TempDir::new().unwrap();
        let compartida = tmp.path().join("compartida");
        std::fs::create_dir_all(&compartida).unwrap();
        let local_a = tmp
            .path()
            .join("datos_a")
            .join("proyectos")
            .join("estudio-avance");
        let local_b = tmp
            .path()
            .join("datos_b")
            .join("proyectos")
            .join("estudio-avance");
        configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
        configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
        let ops = carpeta_ops(&local_a).unwrap();

        let db_a = Db::abrir(&local_a).unwrap();
        let db_b = Db::abrir(&local_b).unwrap();

        let admin_fila = crate::db::usuarios::Usuario {
            email: ADMIN.into(),
            nombre: "Admin".into(),
            apellido: "Uno".into(),
            rol: Rol::Administrador,
            fecha_alta: HOY_INICIO.into(),
        };
        usuarios::insertar(db_a.conexion(), &admin_fila).unwrap();
        usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

        let lider_fila = crate::db::usuarios::Usuario {
            email: LIDER.into(),
            nombre: "Lider".into(),
            apellido: "Uno".into(),
            rol: Rol::Lider,
            fecha_alta: HOY_INICIO.into(),
        };
        usuarios::insertar(db_a.conexion(), &lider_fila).unwrap();
        usuarios::insertar(db_b.conexion(), &lider_fila).unwrap();

        Entorno {
            _tmp: tmp,
            db_a,
            ops,
            db_b,
            local_b,
        }
    }

    fn autor_lider() -> AutorTarea<'static> {
        AutorTarea {
            email: LIDER,
            rol: Rol::Lider,
        }
    }

    fn autor_admin() -> AutorTarea<'static> {
        AutorTarea {
            email: ADMIN,
            rol: Rol::Administrador,
        }
    }

    fn mk_tarea(e: &Entorno, titulo: &str, fecha: &str, ts: &str) -> Tarea {
        crear(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
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
            ts,
            fecha,
        )
        .unwrap()
    }

    #[test]
    fn sprint_restantes_ideal_proyeccion_por_empezar_y_cerrado() {
        let e = setup();
        // Sprint de 4 días (nov 10 -> nov 14 => 5 días calendario, ideal 4 -> 0).
        let s1 = crear_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &DatosNuevoSprint {
                nombre: "Sprint Burndown",
                descripcion: "",
                con_fecha: true,
                duracion_dias: Some(4),
            },
            "2026-11-10T09:00:00.000Z",
            "2026-11-10",
        )
        .unwrap();

        let t1 = mk_tarea(&e, "T1", "2026-11-10", "2026-11-10T09:00:01.000Z");
        let t2 = mk_tarea(&e, "T2", "2026-11-10", "2026-11-10T09:00:02.000Z");
        let t3 = mk_tarea(&e, "T3", "2026-11-10", "2026-11-10T09:00:03.000Z");
        let t4 = mk_tarea(&e, "T4", "2026-11-10", "2026-11-10T09:00:04.000Z");
        for (i, t) in [&t1, &t2, &t3, &t4].into_iter().enumerate() {
            asignar_tarea_a_sprint(
                e.db_a.conexion(),
                &e.ops,
                &autor_lider(),
                &t.id,
                Some(&s1.id),
                &format!("2026-11-10T09:01:0{i}.000Z"),
            )
            .unwrap();
        }

        // Antes de fecha_inicio: solo línea ideal (`restantes_por_dia` vacío, `por_empezar = true`).
        let antes = datos_sprint(e.db_a.conexion(), &s1.id, "2026-11-08").unwrap();
        assert!(antes.por_empezar);
        assert_eq!(
            antes.dias,
            vec![
                "2026-11-10",
                "2026-11-11",
                "2026-11-12",
                "2026-11-13",
                "2026-11-14"
            ]
        );
        assert!(antes.restantes_por_dia.is_empty());
        assert_eq!(antes.linea_ideal, vec![4.0, 3.0, 2.0, 1.0, 0.0]);
        assert!(antes.proyeccion.is_none());

        // Completa T1 el nov 10 y T2 el nov 11 => en curso el nov 12: [3, 2, 2], regresión m = -0.5, a tiempo? no (dia 16 > 14) / o en nov 11: [3, 2], m = -1, x=4 => nov 14 exacto!
        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t1.id,
            EstadoTarea::Terminada,
            "2026-11-10T18:00:00.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t2.id,
            EstadoTarea::Terminada,
            "2026-11-11T18:00:00.000Z",
        )
        .unwrap();

        let dia2 = datos_sprint(e.db_a.conexion(), &s1.id, "2026-11-11").unwrap();
        assert!(!dia2.por_empezar);
        assert_eq!(dia2.restantes_por_dia, vec![3, 2]);
        let proy = dia2.proyeccion.expect("2 puntos calculan proyección");
        assert!(proy.tiene_avance);
        assert!((proy.pendiente - (-1.0)).abs() < 1e-9);
        assert_eq!(proy.fecha_cierre_estimada.as_deref(), Some("2026-11-13"));
        assert!(proy.a_tiempo);

        // Sin avance (solo 1 tarea en curso sin completar entre nov 10..12 en un sprint paralelo S-2).
        let s2 = crear_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &DatosNuevoSprint {
                nombre: "Plano",
                descripcion: "",
                con_fecha: true,
                duracion_dias: Some(4),
            },
            "2026-11-10T09:02:00.000Z",
            "2026-11-10",
        )
        .unwrap();
        let tp = mk_tarea(&e, "TP", "2026-11-10", "2026-11-10T09:02:01.000Z");
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &tp.id,
            Some(&s2.id),
            "2026-11-10T09:02:02.000Z",
        )
        .unwrap();
        let sin_avance = datos_sprint(e.db_a.conexion(), &s2.id, "2026-11-12").unwrap();
        assert_eq!(sin_avance.restantes_por_dia, vec![1, 1, 1]);
        assert!(!sin_avance.proyeccion.unwrap().tiene_avance);

        // Al cerrar S-1 el nov 11, liberar t3/t4 NO borra la copia histórica del reporte: la gráfica
        // del sprint cerrado usa `reportes_sprint` (sigue mostrando 4 tareas totales).
        cerrar_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_admin(),
            &s1.id,
            "2026-11-11T20:00:00.000Z",
            "2026-11-11",
        )
        .unwrap();
        let cerrado = datos_sprint(e.db_a.conexion(), &s1.id, "2026-11-20").unwrap();
        assert!(cerrado.cerrado);
        assert_eq!(cerrado.total_tareas, 4);
        assert_eq!(cerrado.restantes_por_dia, vec![3, 2]);
        assert_eq!(cerrado.restantes_actuales, 2);
    }

    #[test]
    fn backlog_y_general_calculan_restantes_ritmo_4_semanas_y_cierre_sin_duplicar() {
        let e = setup();
        let s1 = crear_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &DatosNuevoSprint {
                nombre: "S1",
                descripcion: "",
                con_fecha: true,
                duracion_dias: Some(14),
            },
            "2026-10-01T10:00:00.000Z",
            "2026-10-01",
        )
        .unwrap();
        let b1 = crear_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &DatosNuevoSprint {
                nombre: "Backlog Juego",
                descripcion: "",
                con_fecha: false,
                duracion_dias: None,
            },
            "2026-10-15T10:00:00.000Z",
            "2026-10-15",
        )
        .unwrap();
        let b2 = crear_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &DatosNuevoSprint {
                nombre: "Backlog Terminado",
                descripcion: "",
                con_fecha: false,
                duracion_dias: None,
            },
            "2026-10-15T10:00:01.000Z",
            "2026-10-15",
        )
        .unwrap();

        // Tareas en B-1 + compartida con S-1 (no debe duplicarse en General), más suelta sin planificación.
        let t1 = mk_tarea(
            &e,
            "Hecha antigua",
            "2026-10-10",
            "2026-10-10T10:00:00.000Z",
        ); // fuera ventana 4 sem
        let t2 = mk_tarea(
            &e,
            "Hecha reciente en B1 y S1",
            "2026-11-01",
            "2026-11-01T10:00:00.000Z",
        );
        let t3 = mk_tarea(
            &e,
            "Hecha reciente",
            "2026-11-05",
            "2026-11-05T10:00:00.000Z",
        );
        let t4 = mk_tarea(&e, "Abierta B1", "2026-11-10", "2026-11-10T10:00:00.000Z");
        let t5 = mk_tarea(&e, "Abierta B1 2", "2026-11-12", "2026-11-12T10:00:00.000Z");
        let t_b2 = mk_tarea(&e, "Hecha B2", "2026-10-20", "2026-10-20T10:00:00.000Z");
        let t_suelta = mk_tarea(
            &e,
            "Suelta no entra en General",
            "2026-11-01",
            "2026-11-01T10:01:00.000Z",
        );
        let _ = t_suelta;

        for (i, t) in [&t1, &t2, &t3, &t4, &t5].into_iter().enumerate() {
            asignar_tarea_a_backlog(
                e.db_a.conexion(),
                &e.ops,
                &autor_lider(),
                &t.id,
                Some(&b1.id),
                &format!("2026-11-12T10:01:0{i}.000Z"),
            )
            .unwrap();
        }
        asignar_tarea_a_sprint(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t2.id,
            Some(&s1.id),
            "2026-11-12T10:02:00.000Z",
        )
        .unwrap();
        asignar_tarea_a_backlog(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_b2.id,
            Some(&b2.id),
            "2026-11-12T10:02:01.000Z",
        )
        .unwrap();

        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t1.id,
            EstadoTarea::Terminada,
            "2026-10-12T10:00:00.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t2.id,
            EstadoTarea::Terminada,
            "2026-11-03T10:00:00.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t3.id,
            EstadoTarea::Terminada,
            "2026-11-10T10:00:00.000Z",
        )
        .unwrap();
        cambiar_estado(
            e.db_a.conexion(),
            &e.ops,
            &autor_lider(),
            &t_b2.id,
            EstadoTarea::Terminada,
            "2026-10-25T10:00:00.000Z",
        )
        .unwrap();

        let hoy = "2026-11-16"; // ventana 4 sem: > 2026-10-19 y <= 2026-11-16
        let av_b1 = datos_backlog(e.db_a.conexion(), &b1.id, hoy).unwrap();
        assert!(!av_b1.es_general);
        assert_eq!(av_b1.dias.first().map(String::as_str), Some("2026-10-10")); // primera tarea del backlog
        assert_eq!(av_b1.dias.last().map(String::as_str), Some(hoy));
        assert_eq!(av_b1.total_tareas, 5);
        assert_eq!(av_b1.terminadas, 3);
        assert_eq!(av_b1.restantes_actuales, 2);
        // En ventana (> 2026-10-19): entraron 4 (nov 1, 5, 10, 12); salieron 2 (nov 3, 10; oct 12 queda fuera)
        assert_eq!(av_b1.ritmo.entraron_4_semanas, 4);
        assert_eq!(av_b1.ritmo.salieron_4_semanas, 2);
        assert!((av_b1.ritmo.entran_por_semana - 1.0).abs() < 1e-9);
        assert!((av_b1.ritmo.salen_por_semana - 0.5).abs() < 1e-9);
        assert!(av_b1.ritmo.creciendo);
        // ritmo diario = 2 / 28 = 1/14 => restantes 2 / (1/14) = 28 días => 2026-12-14
        assert_eq!(av_b1.dias_faltan_estimados, Some(28));
        assert_eq!(av_b1.fecha_cierre_estimada.as_deref(), Some("2026-12-14"));

        // B2 terminado: fecha último cierre = 2026-10-25
        let av_b2 = datos_backlog(e.db_a.conexion(), &b2.id, hoy).unwrap();
        assert!(av_b2.terminado);
        assert_eq!(av_b2.fecha_ultimo_cierre.as_deref(), Some("2026-10-25"));
        assert_eq!(av_b2.dias_faltan_estimados, Some(0));

        // General: incluye B1 (5) + B2 (1) sin duplicar `t2` que está en B1 y S1, ni incluir `t_suelta` -> total 6.
        let gen = datos_general(e.db_a.conexion(), hoy).unwrap();
        assert!(gen.es_general);
        assert_eq!(gen.total_tareas, 6);
        assert_eq!(gen.terminadas, 4);
        assert_eq!(gen.restantes_actuales, 2);
        assert_eq!(gen.dias.first().map(String::as_str), Some("2026-10-01"));

        // Sincroniza máquina B y comprueba cálculos idénticos.
        let ahora = DateTime::parse_from_rfc3339("2026-11-16T12:00:00.000Z")
            .unwrap()
            .with_timezone(&Utc);
        sincronizar(e.db_b.conexion(), &e.ops, &e.local_b, ahora, |c, ev| {
            lectura::aplicar_evento(c, ev)
        })
        .unwrap();
        assert_eq!(datos_general(e.db_b.conexion(), hoy).unwrap(), gen);
    }

    /// **Prueba en el entorno real** (CLAUDE.md, issue #115): contra
    /// `BranchBoard-Pruebas/BranchBoard-Pruebas@MiguelM17@unity` + OneDrive real,
    /// ejerce los comandos Tauri `datos_avance_sprint_en`, `datos_avance_backlog_en`
    /// y `datos_avance_general_en` sobre:
    /// 1. Sprint en curso (verifica restantes por día, línea ideal y proyección lineal).
    /// 2. Sprint ya cerrado manualmente (grafica desde la copia de `reportes_sprint`).
    /// 3. Backlog (restantes por día, ritmo de 4 semanas y cierre estimado).
    /// 4. General (todas las tareas en algún sprint o backlog sin duplicar,
    ///    excluyendo las sueltas).
    /// 5. Sincronización en máquina B por OneDrive obteniendo métricas idénticas.
    ///
    /// Al terminar borra el workspace y la subcarpeta de OneDrive.
    #[test]
    #[ignore = "usa Unity VC y OneDrive de verdad: correr a mano con BB_PRUEBA_ORG, BB_PRUEBA_REPO y BB_PRUEBA_CARPETA_COMPARTIDA"]
    fn entorno_real_datos_avance() {
        use crate::cm::Comando;
        use crate::comandos::acceso::entrar_en;
        use crate::comandos::datos::carpeta_abierta;
        use crate::comandos::sprints::{
            asignar_tarea_a_backlog_en, asignar_tarea_a_sprint_en, cerrar_sprint_en,
            crear_sprint_en, datos_avance_backlog_en, datos_avance_general_en,
            datos_avance_sprint_en,
        };
        use crate::comandos::tareas::cambiar_estado_en;
        use crate::estado::AppState;
        use crate::modelo::hoy_local;

        let org = std::env::var("BB_PRUEBA_ORG").expect("falta BB_PRUEBA_ORG");
        let repo = std::env::var("BB_PRUEBA_REPO").expect("falta BB_PRUEBA_REPO");
        let nube = PathBuf::from(
            std::env::var("BB_PRUEBA_CARPETA_COMPARTIDA")
                .expect("falta BB_PRUEBA_CARPETA_COMPARTIDA"),
        );
        let sufijo = chrono::Local::now().format("%H%M%S").to_string();
        let compartida = nube.join(format!("BranchBoard-prueba-115-{sufijo}"));
        std::fs::create_dir_all(&compartida).unwrap();

        let estado_a = AppState::default();
        let ws_a = tempfile::tempdir().unwrap();
        let datos_a = tempfile::tempdir().unwrap();
        let maquina_b = tempfile::tempdir().unwrap();
        let ruta_ws = ws_a.path().to_string_lossy().to_string();

        Comando::cm()
            .args(["workspace", "create"])
            .arg(format!("bb-prueba-115-{sufijo}"))
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

            configurar_carpeta_compartida(&local_a, &compartida.to_string_lossy(), true).unwrap();
            configurar_carpeta_compartida(&local_b, &compartida.to_string_lossy(), false).unwrap();
            let ops_a = carpeta_ops(&local_a).unwrap();
            let ops_b = carpeta_ops(&local_b).unwrap();

            let admin_fila = estado_a
                .con_db(|db| {
                    let todos = usuarios::listar(db.conexion())?;
                    Ok(todos.into_iter().next().unwrap())
                })
                .unwrap();

            let db_b = Db::abrir(&local_b).unwrap();
            usuarios::insertar(db_b.conexion(), &admin_fila).unwrap();

            let hoy_real = hoy_local();
            let ayer = (chrono::Local::now().date_naive() - Days::new(1))
                .format("%Y-%m-%d")
                .to_string();
            let hace_4_dias = (chrono::Local::now().date_naive() - Days::new(4))
                .format("%Y-%m-%d")
                .to_string();
            let pausa = || std::thread::sleep(std::time::Duration::from_millis(5));
            let ahora_pausa = || {
                pausa();
                crate::modelo::ahora_iso()
            };

            estado_a
                .con_db(|db_a| {
                    Ok::<_, crate::error::Error>({
                        let u = crate::identidad::equipo::alta_de_prueba(
                            db_a.conexion(),
                            LIDER,
                            Rol::Lider,
                            &hoy_real,
                        );
                        crate::db::usuarios::insertar(db_b.conexion(), &u).unwrap();
                        u
                    })
                })
                .unwrap();

            // Sprint en curso S-1 (iniciado hace 4 días, duración 10), backlog B-1 y tarea suelta.
            let s1 = estado_a
                .con_db(|db_a| {
                    crear_sprint(
                        db_a.conexion(),
                        &ops_a,
                        &autor_lider(),
                        &DatosNuevoSprint {
                            nombre: "Sprint En Curso",
                            descripcion: "",
                            con_fecha: true,
                            duracion_dias: Some(10),
                        },
                        &ahora_pausa(),
                        &hace_4_dias,
                    )
                })
                .unwrap();
            pausa();
            let b1 = crear_sprint_en(&estado_a, "Backlog Avance", "", false, None).unwrap();
            pausa();

            let (t_s1_ayer, t_s1_hoy, t_b1_pend, t_suelta) = estado_a
                .con_db(|db_a| {
                    let mk = |titulo: &str, f: &str| {
                        crear(
                            db_a.conexion(),
                            &ops_a,
                            &autor_lider(),
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
                            &ahora_pausa(),
                            f,
                        )
                    };
                    let t1 = mk("Hecha ayer", &hace_4_dias)?;
                    let t2 = mk("En curso S1 y B1", &ayer)?;
                    let t3 = mk("Pendiente en B1", &hace_4_dias)?;
                    let t4 = mk("Suelta fuera de General", &hoy_real)?;
                    Ok((t1, t2, t3, t4))
                })
                .unwrap();
            let _ = t_suelta;

            asignar_tarea_a_sprint_en(&estado_a, &t_s1_ayer.id, Some(&s1.id)).unwrap();
            pausa();
            asignar_tarea_a_sprint_en(&estado_a, &t_s1_hoy.id, Some(&s1.id)).unwrap();
            pausa();
            asignar_tarea_a_backlog_en(&estado_a, &t_s1_hoy.id, Some(&b1.id)).unwrap();
            pausa();
            asignar_tarea_a_backlog_en(&estado_a, &t_b1_pend.id, Some(&b1.id)).unwrap();
            pausa();

            // Completa t_s1_ayer -> en S-1 pasa de 2 a 1 restantes a lo largo de 5 días -> tiene avance!
            cambiar_estado_en(&estado_a, &t_s1_ayer.id, EstadoTarea::Terminada).unwrap();
            pausa();

            let av_s1 = datos_avance_sprint_en(&estado_a, &s1.id).unwrap();
            assert_eq!(av_s1.dias.len(), 11);
            assert_eq!(av_s1.linea_ideal.len(), 11);
            assert!((av_s1.linea_ideal[0] - 2.0).abs() < 1e-9);
            assert!((av_s1.linea_ideal[10] - 0.0).abs() < 1e-9);
            assert_eq!(av_s1.restantes_por_dia.len(), 5);
            assert_eq!(av_s1.restantes_actuales, 1);

            let av_b1 = datos_avance_backlog_en(&estado_a, &b1.id).unwrap();
            assert_eq!(av_b1.total_tareas, 2);
            assert_eq!(av_b1.restantes_actuales, 2);
            assert_eq!(av_b1.ritmo.salieron_4_semanas, 0);
            assert_eq!(av_b1.fecha_cierre_estimada, None);

            // General: t_s1_ayer + t_s1_hoy (solo 1 vez aunque esté en S1 y B1) + t_b1_pend = 3 (sin t_suelta).
            let gen_antes = datos_avance_general_en(&estado_a).unwrap();
            assert!(gen_antes.es_general);
            assert_eq!(gen_antes.total_tareas, 3);
            assert_eq!(gen_antes.terminadas, 1);
            assert_eq!(gen_antes.ritmo.salieron_4_semanas, 1);
            assert!(gen_antes.fecha_cierre_estimada.is_some());

            // Cerramos S-1: libera t_s1_hoy, pero la gráfica del sprint cerrado sigue mostrando 2 tareas de su reporte.
            cerrar_sprint_en(&estado_a, &s1.id).unwrap();
            pausa();
            let av_s1_cerrado = datos_avance_sprint_en(&estado_a, &s1.id).unwrap();
            assert!(av_s1_cerrado.cerrado);
            assert_eq!(av_s1_cerrado.total_tareas, 2);

            std::thread::sleep(std::time::Duration::from_secs(2));

            let ahora = Utc::now() + chrono::Duration::minutes(1);
            let sync_b = sincronizar(db_b.conexion(), &ops_b, &local_b, ahora, |c, e| {
                lectura::aplicar_evento(c, e)
            })
            .unwrap();
            println!("B sincroniza datos de avance desde OneDrive: {sync_b:?}");
            assert_eq!(sync_b.descartados, 0);

            assert_eq!(
                datos_sprint(db_b.conexion(), &s1.id, &hoy_real).unwrap(),
                av_s1_cerrado
            );
            assert_eq!(
                datos_backlog(db_b.conexion(), &b1.id, &hoy_real).unwrap(),
                av_b1
            );
            assert_eq!(
                datos_general(db_b.conexion(), &hoy_real).unwrap(),
                datos_avance_general_en(&estado_a).unwrap()
            );
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
