import { create } from 'zustand';
import type {
  Usuario,
  Tarea,
  Sprint,
  Notificacion,
  SolicitudIngreso,
  ColaboradorExterno,
  EdicionPendiente,
  Backup,
  Commit,
  EstadoTarea,
  Rol,
  TipoTarea,
} from '../types';
import * as fx from '../data/fixtures';

let idSeq = 1000;
const nextId = () => `T-${++idSeq}`;
let notifSeq = 1000;
const nextNotifId = () => `N-${++notifSeq}`;
const today = () => new Date().toISOString().slice(0, 10);

// Las subtareas nunca pasan por "en revisión" — se terminan directo contra la rama de su tarea
// padre (merge liviano, sin el candado de "mergear a dev"). Lo único que se revisa de verdad es
// la rama total: en cuanto todas las hijas terminan, el padre pasa a "en revisión" él mismo —
// ahí sí corre el flujo normal de aprobación (Líder/Admin) antes de llegar a "terminada".
// - todas "por hacer" -> "por hacer"
// - alguna "por hacer" o "en producción" -> "en producción"
// - ninguna "por hacer"/"en producción" (todas terminaron su parte) -> "en revisión"
function estadoDerivadoDeHijas(hijas: Tarea[]): EstadoTarea {
  if (hijas.every((h) => h.estado === 'por hacer')) return 'por hacer';
  if (hijas.some((h) => h.estado === 'por hacer' || h.estado === 'en producción')) return 'en producción';
  return 'en revisión';
}

// Se aplica de abajo hacia arriba por toda la cadena de tareas padre hasta que se estabiliza.
// Además: ninguna tarea que no esté "por hacer" puede quedar sin responsable — si al padre le
// tocó avanzar por sus hijas y no tiene asignado_a, hereda el de alguna de ellas.
// "terminada" es terminal: una vez autorizado el merge de la rama total, no se vuelve a derivar
// aunque sus hijas ya estuvieran todas en ese estado (si no, nunca se podría salir de "en revisión").
function normalizarJerarquiaEstados(tareas: Tarea[]): Tarea[] {
  let resultado = tareas;
  for (let i = 0; i < 10; i++) {
    let cambio = false;
    resultado = resultado.map((t) => {
      if (t.estado === 'terminada') return t;
      const hijas = resultado.filter((h) => h.tarea_padre_id === t.id && !h.eliminada);
      if (hijas.length === 0) return t;
      const derivado = estadoDerivadoDeHijas(hijas);
      const necesitaResponsable = derivado !== 'por hacer' && !t.asignado_a;
      if (derivado === t.estado && !necesitaResponsable) return t;
      cambio = true;
      return {
        ...t,
        estado: derivado,
        asignado_a: necesitaResponsable
          ? hijas.find((h) => h.asignado_a)?.asignado_a ?? t.asignado_a
          : t.asignado_a,
      };
    });
    if (!cambio) break;
  }
  return resultado;
}

export interface NuevaTarea {
  titulo: string;
  descripcion: string;
  tipo: TipoTarea;
  tarea_padre_id: string | null;
  sprint_id: string | null;
  con_fecha: boolean;
  fecha_limite: string | null;
}

interface State {
  // sesión simulada
  currentEmail: string;
  vpnOn: boolean;
  // Simula el resultado de `cm whoami` al arrancar — en false, la app muestra la
  // pantalla de "no hay sesión de Unity VC" en vez de los tableros (ver FRONTEND.md,
  // "Autenticación / acceso al proyecto"). Toggle de demo en el Topbar.
  cmSesionActiva: boolean;

  // datos
  usuarios: Usuario[];
  tareas: Tarea[];
  sprints: Sprint[];
  notificaciones: Notificacion[];
  solicitudes: SolicitudIngreso[];
  colaboradores: ColaboradorExterno[];
  edicionesPendientes: EdicionPendiente[];
  backups: Backup[];
  commits: Commit[];
  correoContacto: string;

  // toasts efímeros
  toast: { id: number; msg: string; tone: 'ok' | 'info' | 'danger' } | null;

  // acciones de sesión
  setEmail: (email: string) => void;
  setRolActual: (rol: Rol) => void; // cambia el rol del usuario activo (simulación)
  toggleVpn: () => void;
  toggleCmSesion: () => void;
  showToast: (msg: string, tone?: 'ok' | 'info' | 'danger') => void;
  clearToast: () => void;

  // getters
  currentUser: () => Usuario;

  // acciones de tareas
  moverTarea: (id: string, estado: EstadoTarea) => void;
  tomarTarea: (id: string, email: string, modo?: 'rama' | 'checkin') => void;
  desasignarTarea: (id: string) => void;
  crearTarea: (data: NuevaTarea) => string;
  crearTareaConSubtareas: (
    data: NuevaTarea & { subtareas: { tipo: TipoTarea; titulo: string; descripcion: string }[] }
  ) => void;
  editarTarea: (id: string, campos: Partial<Tarea>, marcarPendiente: boolean) => void;
  editarYAprobarTarea: (id: string, campos: { titulo: string; descripcion: string }) => void;
  eliminarTarea: (id: string, justificacion: string) => void;
  eliminarDefinitivo: (id: string) => void;

  // pendientes
  aprobarTarea: (id: string) => void;
  advertirTarea: (id: string, mensaje: string) => void;
  aprobarEdicion: (tareaId: string) => void;
  rechazarEdicion: (tareaId: string) => void;

  // revisión
  autorizarMerge: (id: string) => void;
  rechazarRevision: (id: string, motivo: string) => void;
  asignarRevision: (id: string, liderEmail: string) => void;
  autoasignarRevision: (id: string, email: string) => void;

  // notificaciones
  marcarLeida: (id: string) => void;
  desmarcarLeida: (id: string) => void;
  marcarTodasLeidas: (email: string) => void;

  // usuarios / ajustes
  aceptarSolicitud: (id: string, rol: Rol) => void;
  rechazarSolicitud: (id: string) => void;
  confirmarUsuario: (email: string) => void;
  eliminarUsuario: (email: string) => void;
  ascenderALider: (email: string) => void;
  agregarColaborador: (email: string, nombre: string) => void;
  revocarColaborador: (email: string) => void;
  setCorreoContacto: (correo: string) => void;

  // sprints
  crearSprint: (nombre: string, descripcion: string, con_fecha: boolean, duracion_dias: number | null) => void;
  cerrarSprint: (id: string) => void;
}

export const useStore = create<State>((set, get) => {
  const notificar = (n: Omit<Notificacion, 'id' | 'fecha' | 'leida'>) => {
    const notif: Notificacion = { id: nextNotifId(), fecha: today(), leida: false, ...n };
    set((s) => ({ notificaciones: [notif, ...s.notificaciones] }));
  };
  // Destinatarios de las notificaciones que van dirigidas a "todo Líder/Administrador",
  // no a una sola persona (ver documentacion/BACKEND.md#notificaciones).
  const notificarLideres = (n: Omit<Notificacion, 'id' | 'fecha' | 'leida' | 'para'>) => {
    const destinatarios = get().usuarios.filter((u) => u.rol === 'Líder' || u.rol === 'Administrador');
    destinatarios.forEach((u) => notificar({ ...n, para: u.email }));
  };

  return {
  currentEmail: fx.usuarios[0].email,
  vpnOn: true,
  cmSesionActiva: true,

  usuarios: [...fx.usuarios],
  tareas: normalizarJerarquiaEstados([...fx.tareas]),
  sprints: [...fx.sprints],
  notificaciones: [...fx.notificaciones],
  solicitudes: [...fx.solicitudes],
  colaboradores: [...fx.colaboradores],
  edicionesPendientes: [...fx.edicionesPendientes],
  commits: [...fx.commits],
  backups: [...fx.backups],
  correoContacto: 'ingreso@virustf.dev',

  toast: null,

  setEmail: (email) => set({ currentEmail: email }),
  setRolActual: (rol) =>
    set((s) => ({
      usuarios: s.usuarios.map((u) => (u.email === s.currentEmail ? { ...u, rol } : u)),
    })),
  toggleVpn: () => set((s) => ({ vpnOn: !s.vpnOn })),
  toggleCmSesion: () => set((s) => ({ cmSesionActiva: !s.cmSesionActiva })),
  showToast: (msg, tone = 'ok') => {
    const id = Date.now();
    set({ toast: { id, msg, tone } });
    setTimeout(() => {
      if (get().toast?.id === id) set({ toast: null });
    }, 2800);
  },
  clearToast: () => set({ toast: null }),

  currentUser: () => {
    const s = get();
    return s.usuarios.find((u) => u.email === s.currentEmail) ?? s.usuarios[0];
  },

  moverTarea: (id, estado) => {
    const anterior = get().tareas.find((t) => t.id === id);
    set((s) => ({
      tareas: normalizarJerarquiaEstados(
        s.tareas.map((t) =>
          t.id === id
            ? {
                ...t,
                estado,
                fecha_completada: estado === 'terminada' ? today() : t.fecha_completada,
              }
            : t
        )
      ),
    }));
    // Notificación #5 (BACKEND.md): una tarea llega a "en revisión" — avisa a todo Líder/Admin.
    if (anterior && anterior.estado !== 'en revisión' && estado === 'en revisión') {
      notificarLideres({
        tipo: 'tarea_a_revision',
        titulo: 'Una tarea pasó a revisión',
        cuerpo: `"${anterior.titulo}" (${id}) pasó a revisión.`,
        tarea_id: id,
        accionable: false,
      });
    }
  },

  // "checkin" solo tiene sentido para subtareas (tarea_padre_id != null) — se valida en la UI,
  // que no ofrece la opción para tareas raíz. Aquí, por si acaso, cae a "rama" si no se pasa modo.
  tomarTarea: (id, email, modo = 'rama') =>
    set((s) => ({
      tareas: s.tareas.map((t) => {
        if (t.id !== id) return t;
        if (modo === 'checkin' && t.tarea_padre_id) {
          return { ...t, asignado_a: email, modo_trabajo: 'checkin' };
        }
        return {
          ...t,
          asignado_a: email,
          modo_trabajo: 'rama',
          rama_asociada: t.rama_asociada ?? `task/${id.toLowerCase()}-${t.tipo.slice(0, 4).toLowerCase()}`,
        };
      }),
    })),

  desasignarTarea: (id) =>
    set((s) => ({
      tareas: s.tareas.map((t) =>
        t.id === id ? { ...t, asignado_a: null, estado: 'por hacer' } : t
      ),
    })),

  crearTarea: (data) => {
    const s = get();
    const user = s.currentUser();
    const pendiente = user.rol === 'Desarrollador' || user.rol === 'Tester';
    const nueva: Tarea = {
      id: nextId(),
      titulo: data.titulo,
      descripcion: data.descripcion,
      tipo: data.tipo,
      tarea_padre_id: data.tarea_padre_id,
      archivo_ruta: null,
      estado: 'por hacer',
      pendiente_aprobacion: pendiente,
      rama_asociada: null,
      modo_trabajo: null,
      modificada: false,
      creado_por: user.email,
      asignado_a: null,
      lider_revision_asignado: null,
      sprint_id: data.sprint_id,
      con_fecha: data.con_fecha,
      fecha_limite: data.fecha_limite,
      fecha_completada: null,
      eliminada: false,
      justificacion_eliminacion: null,
      fecha_eliminacion: null,
      motivo_rechazo: null,
      reporte_testing_origen_id: null,
      edicion_pendiente: false,
    };
    set({ tareas: [nueva, ...s.tareas] });
    s.showToast(pendiente ? 'Tarea creada — quedó pendiente de aprobación' : 'Tarea creada');
    // Notificación #1 (BACKEND.md): tarea nueva esperando aprobación — avisa a todo Líder/Admin.
    if (pendiente) {
      notificarLideres({
        tipo: 'aprobacion_tarea',
        titulo: 'Tarea espera tu aprobación',
        cuerpo: `${user.nombre} creó "${nueva.titulo}" (${nueva.id}) y espera tu aprobación.`,
        tarea_id: nueva.id,
        accionable: true,
      });
    }
    return nueva.id;
  },

  // Crea la tarea raíz y, si trae subtareas (tarea general), las crea encadenadas a ella.
  crearTareaConSubtareas: (data) => {
    const { subtareas, ...raiz } = data;
    const padreId = get().crearTarea(raiz);
    subtareas.forEach((st) => {
      get().crearTarea({
        titulo: st.titulo,
        descripcion: st.descripcion,
        tipo: st.tipo,
        tarea_padre_id: padreId,
        sprint_id: raiz.sprint_id,
        con_fecha: false,
        fecha_limite: null,
      });
    });
  },

  editarTarea: (id, campos, marcarPendiente) =>
    set((s) => ({
      tareas: s.tareas.map((t) =>
        t.id === id ? { ...t, ...campos, edicion_pendiente: marcarPendiente || t.edicion_pendiente } : t
      ),
    })),

  // "editar_y_aprobar_tarea" (BACKEND.md #1): un solo comando — corrige campos y aprueba a la vez.
  // Notificación #11: avisa al creador que su tarea pendiente fue editada y aprobada.
  editarYAprobarTarea: (id, campos) => {
    const tarea = get().tareas.find((t) => t.id === id);
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === id ? { ...t, ...campos, pendiente_aprobacion: false } : t)),
    }));
    get().showToast('Tarea editada y aprobada');
    if (tarea) {
      notificar({
        tipo: 'editada_aprobada',
        para: tarea.creado_por,
        titulo: 'Tu tarea pendiente fue editada y aprobada',
        cuerpo: `"${campos.titulo}" (${id}) fue editada y aprobada.`,
        tarea_id: id,
        accionable: false,
      });
    }
  },

  eliminarTarea: (id, justificacion) => {
    const tarea = get().tareas.find((t) => t.id === id);
    set((s) => ({
      tareas: s.tareas.map((t) =>
        t.id === id
          ? {
              ...t,
              eliminada: true,
              justificacion_eliminacion: justificacion,
              fecha_eliminacion: today(),
              pendiente_aprobacion: false,
              edicion_pendiente: false,
            }
          : t
      ),
    }));
    get().showToast('Tarea enviada a la Papelera', 'danger');
    // Notificación #13 (BACKEND.md): tu tarea fue eliminada — aplica a cualquier eliminar_tarea.
    if (tarea) {
      notificar({
        tipo: 'tarea_eliminada',
        para: tarea.creado_por,
        titulo: 'Tu tarea fue eliminada',
        cuerpo: `"${tarea.titulo}" (${id}) fue enviada a la Papelera. Motivo: ${justificacion}`,
        tarea_id: id,
        accionable: false,
      });
    }
  },

  eliminarDefinitivo: (id) => {
    set((s) => ({ tareas: s.tareas.filter((t) => t.id !== id) }));
    get().showToast('Tarea eliminada definitivamente', 'danger');
  },

  aprobarTarea: (id) => {
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === id ? { ...t, pendiente_aprobacion: false } : t)),
    }));
    get().showToast('Tarea aprobada');
  },

  // Notificación #12 (BACKEND.md): advertencia sobre tarea pendiente — no toca la tarea, solo avisa.
  advertirTarea: (id, mensaje) => {
    const tarea = get().tareas.find((t) => t.id === id);
    get().showToast('Advertencia enviada al creador', 'info');
    if (tarea) {
      notificar({
        tipo: 'advertencia',
        para: tarea.creado_por,
        titulo: 'Advertencia sobre tu tarea pendiente',
        cuerpo: `Sobre "${tarea.titulo}" (${id}): ${mensaje}`,
        tarea_id: id,
        accionable: false,
      });
    }
  },

  aprobarEdicion: (tareaId) => {
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === tareaId ? { ...t, edicion_pendiente: false } : t)),
      edicionesPendientes: s.edicionesPendientes.filter((e) => e.tarea_id !== tareaId),
    }));
    get().showToast('Edición aprobada');
  },

  rechazarEdicion: (tareaId) => {
    const s = get();
    const ep = s.edicionesPendientes.find((e) => e.tarea_id === tareaId);
    set({
      tareas: s.tareas.map((t) => {
        if (t.id !== tareaId) return t;
        const rev: Partial<Tarea> = { edicion_pendiente: false };
        if (ep) {
          if (ep.campos_anteriores.titulo) rev.titulo = ep.campos_anteriores.titulo;
          if (ep.campos_anteriores.descripcion) rev.descripcion = ep.campos_anteriores.descripcion;
        }
        return { ...t, ...rev };
      }),
      edicionesPendientes: s.edicionesPendientes.filter((e) => e.tarea_id !== tareaId),
    });
    get().showToast('Edición rechazada — se revirtieron los valores', 'danger');
  },

  // Al autorizar la rama total (la tarea general), se valida junto con TODAS sus hijas (recursivo,
  // ya terminaron su parte y están en "en revisión") — terminan todas a la vez, en un solo merge.
  // Para una tarea sin hijas (individual), esto no cambia nada más que a sí misma.
  autorizarMerge: (id) => {
    set((s) => {
      const descendientes = new Set<string>();
      const recolectar = (padreId: string) => {
        s.tareas.forEach((t) => {
          if (t.tarea_padre_id === padreId && !t.eliminada) {
            descendientes.add(t.id);
            recolectar(t.id);
          }
        });
      };
      recolectar(id);
      const hoy = today();
      return {
        tareas: normalizarJerarquiaEstados(
          s.tareas.map((t) =>
            t.id === id || descendientes.has(t.id)
              ? { ...t, estado: 'terminada', fecha_completada: hoy }
              : t
          )
        ),
      };
    });
    get().showToast('Merge autorizado — tarea terminada');
  },

  rechazarRevision: (id, motivo) => {
    const tarea = get().tareas.find((t) => t.id === id);
    set((s) => ({
      tareas: normalizarJerarquiaEstados(
        s.tareas.map((t) => (t.id === id ? { ...t, estado: 'en producción', motivo_rechazo: motivo } : t))
      ),
    }));
    get().showToast('Revisión rechazada — vuelve a producción', 'danger');
    if (tarea?.asignado_a) {
      notificar({
        tipo: 'tarea_rechazada',
        para: tarea.asignado_a,
        titulo: 'Tu tarea fue rechazada en revisión',
        cuerpo: `"${tarea.titulo}" (${id}) volvió a producción. Motivo: ${motivo}`,
        tarea_id: id,
        accionable: false,
      });
    }
  },

  // Notificación #6 (BACKEND.md): revisión asignada a un Líder específico.
  asignarRevision: (id, liderEmail) => {
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === id ? { ...t, lider_revision_asignado: liderEmail } : t)),
    }));
    get().showToast('Revisión asignada');
    const tarea = get().tareas.find((t) => t.id === id);
    notificar({
      tipo: 'revision_asignada',
      para: liderEmail,
      titulo: 'Se te asignó revisar una tarea',
      cuerpo: `Se te asignó revisar "${tarea?.titulo ?? id}" (${id}).`,
      tarea_id: id,
      accionable: false,
    });
  },

  autoasignarRevision: (id, email) => {
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === id ? { ...t, lider_revision_asignado: email } : t)),
    }));
    get().showToast('Te asignaste la revisión');
  },

  marcarLeida: (id) =>
    set((s) => ({
      notificaciones: s.notificaciones.map((n) => (n.id === id ? { ...n, leida: true } : n)),
    })),

  desmarcarLeida: (id) =>
    set((s) => ({
      notificaciones: s.notificaciones.map((n) => (n.id === id ? { ...n, leida: false } : n)),
    })),

  marcarTodasLeidas: (email) =>
    set((s) => ({
      notificaciones: s.notificaciones.map((n) => (n.para === email ? { ...n, leida: true } : n)),
    })),

  aceptarSolicitud: (id, rol) => {
    const s = get();
    const sol = s.solicitudes.find((x) => x.id === id);
    if (!sol) return;
    const nuevoUsuario: Usuario = {
      email: sol.correo,
      nombre: sol.nombre,
      apellido: sol.apellido,
      rol,
      fecha_alta: today(),
      firma: null, // pendiente de confirmar por Administrador
      firmado_en: null,
    };
    set({
      solicitudes: s.solicitudes.map((x) =>
        x.id === id ? { ...x, estado: 'aceptada', fecha_resolucion: today() } : x
      ),
      usuarios: [...s.usuarios, nuevoUsuario],
    });
    s.showToast('Solicitud aceptada — usuario pendiente de confirmar');
  },

  rechazarSolicitud: (id) => {
    set((s) => ({
      solicitudes: s.solicitudes.map((x) =>
        x.id === id ? { ...x, estado: 'rechazada', fecha_resolucion: today() } : x
      ),
    }));
    get().showToast('Solicitud rechazada', 'danger');
  },

  confirmarUsuario: (email) => {
    set((s) => ({
      usuarios: s.usuarios.map((u) =>
        u.email === email ? { ...u, firma: 'sig_ok', firmado_en: new Date().toISOString() } : u
      ),
    }));
    get().showToast('Usuario confirmado');
  },

  eliminarUsuario: (email) => {
    set((s) => ({ usuarios: s.usuarios.filter((u) => u.email !== email) }));
    get().showToast('Usuario eliminado', 'danger');
  },

  ascenderALider: (email) => {
    set((s) => ({
      usuarios: s.usuarios.map((u) => (u.email === email ? { ...u, rol: 'Líder' } : u)),
    }));
    get().showToast('Usuario ascendido a Líder');
  },

  agregarColaborador: (email, nombre) => {
    set((s) => ({
      colaboradores: [
        ...s.colaboradores,
        { email, nombre, agregado_por: s.currentEmail, fecha_alta: today() },
      ],
    }));
    get().showToast('Colaborador agregado');
  },

  revocarColaborador: (email) => {
    set((s) => ({ colaboradores: s.colaboradores.filter((c) => c.email !== email) }));
    get().showToast('Permiso revocado', 'danger');
  },

  setCorreoContacto: (correo) => {
    set({ correoContacto: correo });
    get().showToast('Correo de contacto actualizado');
  },

  crearSprint: (nombre, descripcion, con_fecha, duracion_dias) => {
    const s = get();
    const inicio = today();
    let fin: string | null = null;
    if (con_fecha && duracion_dias) {
      const d = new Date(inicio);
      d.setDate(d.getDate() + duracion_dias);
      fin = d.toISOString().slice(0, 10);
    }
    const nuevo: Sprint = {
      id: `sp${Date.now().toString().slice(-4)}`,
      nombre,
      descripcion,
      con_fecha,
      fecha_inicio: inicio,
      duracion_dias: con_fecha ? duracion_dias : null,
      fecha_fin: fin,
      cerrado: false,
      fecha_cierre_real: null,
      creado_por: s.currentEmail,
    };
    set({ sprints: [...s.sprints, nuevo] });
    s.showToast('Sprint creado');
  },

  cerrarSprint: (id) => {
    set((s) => ({
      sprints: s.sprints.map((sp) =>
        sp.id === id ? { ...sp, cerrado: true, fecha_cierre_real: today() } : sp
      ),
    }));
    get().showToast('Sprint cerrado — se generó su reporte');
  },
  };
});
