import { create } from 'zustand';
import type {
  CambioTraido,
  Usuario,
  Tarea,
  Sprint,
  Notificacion,
  EdicionPendiente,
  Backup,
  Commit,
  EstadoTarea,
  Rol,
  TipoTarea,
  WorkspaceUnityVC,
} from '../types';
import * as fx from '../data/fixtures';
import { esTareaEnEspera } from '../lib/ui';
import { esLiderOAdmin } from '../lib/permisos';

let idSeq = 1000;
const nextId = () => `T-${++idSeq}`;
let notifSeq = 1000;
const nextNotifId = () => `N-${++notifSeq}`;
const today = () => new Date().toISOString().slice(0, 10);
// Fecha LOCAL del equipo (AAAA-MM-DD). toISOString() da la fecha en UTC, que en Colombia ya es
// "mañana" desde las 7 p. m.; la de creación tiene que ser la que muestra el reloj del PC.
const hoyLocal = () => {
  const d = new Date();
  const dos = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${dos(d.getMonth() + 1)}-${dos(d.getDate())}`;
};

// Las subtareas nunca pasan por "en revisión" — se terminan directo contra la rama de su tarea
// padre (merge liviano, sin el candado de "mergear a dev"). Lo único que se revisa de verdad es
// la rama total: en cuanto todas las hijas terminan, el padre pasa a "en revisión" él mismo —
// ahí sí corre el flujo normal de aprobación (Líder/Admin) antes de llegar a "terminada".
// - todas "por hacer" -> "por hacer", SALVO que el padre ya tenga progreso (commits): una rama con
//   trabajo nunca vuelve a "por hacer" -> "en producción"
// - alguna "por hacer" o "en producción" -> "en producción"
// - ninguna "por hacer"/"en producción" (todas terminaron su parte) -> "en revisión"
function estadoDerivadoDeHijas(hijas: Tarea[], padreTieneProgreso = false): EstadoTarea {
  if (hijas.every((h) => h.estado === 'por hacer')) return padreTieneProgreso ? 'en producción' : 'por hacer';
  if (hijas.some((h) => h.estado === 'por hacer' || h.estado === 'en producción')) return 'en producción';
  return 'en revisión';
}

// Se aplica de abajo hacia arriba por toda la cadena de tareas padre hasta que se estabiliza.
// Además: ninguna tarea que no esté "por hacer" puede quedar sin responsable — si al padre le
// tocó avanzar por sus hijas y no tiene asignado_a, hereda el de alguna de ellas.
// "terminada" es terminal: una vez autorizado el merge de la rama total, no se vuelve a derivar
// aunque sus hijas ya estuvieran todas en ese estado (si no, nunca se podría salir de "en revisión").
// conProgreso: ids de tareas con commits (progreso = commits).
function normalizarJerarquiaEstados(tareas: Tarea[], conProgreso: Set<string>): Tarea[] {
  let resultado = tareas;
  for (let i = 0; i < 10; i++) {
    let cambio = false;
    resultado = resultado.map((t) => {
      if (t.estado === 'terminada') return t;
      const hijas = resultado.filter((h) => h.tarea_padre_id === t.id && !h.eliminada);
      if (hijas.length === 0) return t;
      const derivado = estadoDerivadoDeHijas(hijas, conProgreso.has(t.id));
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

// Una rama con commits ya tiene progreso: ni ella ni (al mover un padre) ninguna de sus
// subtareas puede volver a "por hacer" / "sin asignar". Si alguna tiene commits, avisa y bloquea.
/** IDs (la tarea y sus subtareas) que ya tienen commits, es decir, progreso. */
export function progresoDe(tareas: Tarea[], commits: { tarea_id: string }[], id: string): string[] {
  const afectadas = [id, ...descendientesDe(tareas, id)];
  return afectadas.filter((x) => commits.some((c) => c.tarea_id === x));
}
function tieneProgreso(s: { tareas: Tarea[]; commits: { tarea_id: string }[]; showToast: (m: string, tono?: 'ok' | 'info' | 'danger') => void }, id: string): boolean {
  const conCommits = progresoDe(s.tareas, s.commits, id);
  if (!conCommits.length) return false;
  s.showToast(
    `No se puede volver a "Por hacer": ${conCommits.join(', ')} ya ${conCommits.length === 1 ? 'tiene' : 'tienen'} commits (hay progreso).`,
    'danger'
  );
  return true;
}


// Reabrir = devolver una tarea "terminada" (ya mergeada a dev) a "en revisión". Para que no se
// intercale de forma indefinida, cada tarea se puede reabrir hasta este número de veces; después
// solo el Administrador puede seguir reabriéndola.
export const LIMITE_REAPERTURAS = 3;

/** Si el usuario con este rol todavía puede reabrir la tarea. */
export function puedeReabrir(t: Tarea, rol: Rol): boolean {
  return rol === 'Administrador' || (t.reaperturas ?? 0) < LIMITE_REAPERTURAS;
}

// Ids de todo el subárbol (hijas, nietas, etc.) no eliminado de una tarea.
function descendientesDe(tareas: Tarea[], id: string): Set<string> {
  const ids = new Set<string>();
  const recolectar = (padreId: string) => {
    tareas.forEach((t) => {
      if (t.tarea_padre_id === padreId && !t.eliminada) {
        ids.add(t.id);
        recolectar(t.id);
      }
    });
  };
  recolectar(id);
  return ids;
}

/**
 * Incoming changes de una tarea: sus subtareas directas ya terminadas (su rama se mergeó a la de
 * esta tarea) que el workspace todavía no unió. Una subtarea en revisión no cuenta: todavía no se
 * ha mergeado aquí.
 */
export function entrantesDe(tareas: Tarea[], recibidos: string[], id: string): Tarea[] {
  return tareas.filter(
    (t) =>
      t.tarea_padre_id === id &&
      !t.eliminada &&
      !recibidos.includes(t.id) &&
      t.estado === 'terminada' &&
      !!t.rama_asociada
  );
}

/** Texto corto del estado de un incoming change: "Terminada el …" o "En revisión, sin aprobar". */
export function estadoEntrante(t: Tarea): string {
  return t.estado === 'terminada'
    ? `Terminada${t.fecha_completada ? ` el ${t.fecha_completada}` : ''}`
    : '⚠ En revisión, sin aprobar';
}

// Subtareas (de cualquier nivel) que impiden que una tarea avance. Para mandarla a revisión no
// puede quedar ninguna en "por hacer"/"en producción". Para el merge (orden de abajo hacia arriba,
// BACKEND.md) todas deben estar ya "terminada", es decir, mergeadas antes a su rama padre.
function subtareasPendientes(tareas: Tarea[], id: string, estado: EstadoTarea): Tarea[] {
  const descendientes = descendientesDe(tareas, id);
  return tareas.filter(
    (t) =>
      descendientes.has(t.id) &&
      (estado === 'terminada' ? t.estado !== 'terminada' : t.estado === 'por hacer' || t.estado === 'en producción')
  );
}

function avisoSubtareasPendientes(pendientes: Tarea[], estado: EstadoTarea): string {
  const lista = pendientes
    .slice(0, 3)
    .map((t) => `${t.id} (${t.estado})`)
    .join(', ');
  const resto = pendientes.length > 3 ? ` y ${pendientes.length - 3} más` : '';
  return estado === 'terminada'
    ? `No se puede mergear: primero hay que mergear sus subtareas a su rama — ${lista}${resto}`
    : `No se puede mandar a revisión: tiene subtareas pendientes — ${lista}${resto}`;
}

export interface NuevaTarea {
  titulo: string;
  detalles?: string;
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
  // Si ya se pasó por las pantallas de acceso al proyecto (Pantalla inicial →
  // workspaces → validación del .db). En el mockup nunca bloquea: se pueden saltar.
  accesoCompletado: boolean;
  /** Proyecto (workspace de Unity VC) en el que se entró. */
  proyectoActual: string;
  /** "Cambiar de proyecto": vuelve al acceso directo a la lista de workspaces (sin la pantalla inicial). */
  accesoIrAWorkspaces: boolean;
  cambiarDeProyecto: () => void;
  /**
   * Cerrar sesión: BranchBoard no tiene login propio, la identidad es la sesión de Unity VC
   * (`cm whoami`). Cerrarla deja la app en "No se detectó una sesión de Unity VC" y, al volver a
   * iniciarla, se pasa otra vez por el acceso al proyecto.
   */
  cerrarSesion: () => void;
  /** Correo con el que se inició sesión en Unity VC desde BranchBoard (null = todavía no). */
  sesionUnityVC: string | null;
  setSesionUnityVC: (correo: string | null) => void;
  /**
   * "Crear un nuevo proyecto" sin Unity VC: proyecto vacío (sin tareas, sprints ni equipo) con quien
   * lo crea como único usuario y Administrador. En el mockup la contraseña no se guarda.
   */
  crearProyectoNuevo: (datos: { proyecto: string; nombre: string; correo: string }) => void;
  /** Workspaces de Unity VC en esta máquina (`cm workspace list`). */
  workspaces: WorkspaceUnityVC[];
  setWorkspaces: (ws: WorkspaceUnityVC[]) => void;
  /** Proyecto del servidor que se eligió para clonar (lleva al acceso, paso "Clonar"). */
  proyectoAClonar: string | null;
  irAClonar: (nombre: string) => void;
  entrarAProyecto: (nombre: string) => void;

  // datos
  usuarios: Usuario[];
  tareas: Tarea[];
  sprints: Sprint[];
  notificaciones: Notificacion[];
  edicionesPendientes: EdicionPendiente[];
  backups: Backup[];
  /** Tamaño de la base de datos actual (la que va arriba como "Actual" en Copias de seguridad). */
  tamanoActual: string;
  /** Hay un usuario nuevo o un cambio de rol que no has visto: punto azul en "Usuarios" del menú. */
  hayNovedadUsuarios: boolean;
  marcarUsuariosVistos: () => void;
  commits: Commit[];
  /** Registro de "Traer cambios": se muestra en los detalles de la tarea que los recibió. */
  cambiosTraidos: CambioTraido[];
  /** Cambios sin check-in por rama (pending changes de Unity VC, simulados). */
  cambiosSinCheckin: Record<string, number>;
  /**
   * Subtareas con rama propia ya terminadas cuyo merge ya está en el workspace (cm update). Las
   * terminadas que no estén aquí salen como "Cambios entrantes" en los detalles de su padre.
   */
  entrantesRecibidos: string[];

  // toasts efímeros
  toast: { id: number; msg: string; tone: 'ok' | 'info' | 'danger' } | null;
  // Rama en la que está parado el workspace local (solo frontend: simula `cm switch`). null = no
  // está en ninguna tarea. Nunca debe quedar en dev: trabajar ahí metería cambios directo en la
  // rama de integración, así que solo se cambia a ramas de tareas.
  ramaWorkspace: string | null;
  // Panel "Abrir en Unity": cambia el workspace a la rama de la tarea (o a un commit concreto de
  // ella) y abre Unity ahí.
  abrirEnUnity: (tareaId: string, rama: string, hash?: string) => void;
  // "Traer cambios" (solo frontend, simulado): trae a la rama de una tarea los cambios de otra rama o
  // check-in terminada o en revisión (en el backend real: merge desde esa rama/changesets).
  traerCambios: (destinoId: string, ramaDestino: string, origenId: string, hash?: string) => void;
  /** Quita de la rama los cambios traídos (en Unity VC, un merge sustractivo). El registro queda como historial. */
  quitarCambiosTraidos: (registroId: string) => void;
  /**
   * "Se tocaron cambios ajenos": elimina lo que se modificó en los archivos prestados (vuelven a
   * como llegaron en el préstamo). El préstamo sigue ahí hasta que se devuelva.
   */
  eliminarCambiosAjenos: (registroId: string) => void;
  /**
   * Recibe en el workspace los incoming changes de una subtarea terminada (ya mergeada a esta rama).
   * No es obligatorio ni automático: se hace desde "Traer cambios". No es un préstamo: el cambio ya
   * es de esta rama.
   */
  recibirEntrante: (destinoId: string, hijoId: string) => void;
  /** Check-in de los pending changes de una rama sin subtareas (simulado): queda en sus check-ins. */
  hacerCheckin: (rama: string, campos: { titulo: string; descripcion: string; tipo: TipoTarea }) => void;
  /**
   * "En espera" (cambios sin check-in hechos directo en la rama de una tarea con subtareas): se
   * convierte en una subtarea con su propia rama (nombre, detalles y tipo) y los cambios pasan a ella.
   */
  crearRamaDesdeEspera: (esperaId: string, campos: { titulo: string; detalles: string; tipo: TipoTarea }) => void;

  // acciones de sesión
  setEmail: (email: string) => void;
  setRolActual: (rol: Rol) => void; // cambia el rol del usuario activo (simulación)
  toggleVpn: () => void;
  toggleCmSesion: () => void;
  setAccesoCompletado: (v: boolean) => void;
  showToast: (msg: string, tone?: 'ok' | 'info' | 'danger') => void;
  clearToast: () => void;

  // getters
  currentUser: () => Usuario;

  // acciones de tareas
  /** Si la tarea no puede pasar a "en revisión"/"terminada" por tener subtareas pendientes, avisa con un toast y devuelve false. */
  puedeAvanzar: (id: string, estado: EstadoTarea) => boolean;
  moverTarea: (id: string, estado: EstadoTarea) => void;
  /**
   * "En producción" -> "Por hacer" con progreso (confirmado escribiendo "eliminar"): borra los
   * commits de la tarea y sus subtareas, sus cambios prestados y sus cambios sin check-in, y la
   * deja en "Por hacer" (sigue tomada, con su rama). Para todos los roles.
   */
  borrarProgreso: (id: string) => void;
  tomarTarea: (id: string, email: string) => void;
  desasignarTarea: (id: string) => void;
  /**
   * Líder/Administrador asigna (o reasigna) una tarea a alguien desde la Lista de tareas. Si todavía
   * no tiene rama, se crea (sin auto-switch: el workspace de quien asigna no cambia).
   */
  asignarTarea: (id: string, email: string) => void;
  crearTarea: (data: NuevaTarea) => string;
  crearTareaConSubtareas: (
    data: NuevaTarea & { subtareas: { tipo: TipoTarea; titulo: string; detalles?: string }[] }
  ) => void;
  editarTarea: (id: string, campos: Partial<Tarea>, marcarPendiente: boolean) => void;
  editarYAprobarTarea: (id: string, campos: { titulo: string; detalles: string }) => void;
  // "Editar tarea" desde los detalles (título y detalles): se aplica marcada como edición pendiente y
  // se envía al Administrador/Líder para aprobarla; si la rechazan, se revierte.
  proponerEdicion: (id: string, campos: { titulo: string; detalles: string }) => void;
  eliminarTarea: (id: string, justificacion: string) => void;
  eliminarDefinitivo: (id: string) => void;

  // pendientes
  aprobarTarea: (id: string) => void;
  // "Marcar todo como revisado" en Pendientes: aprueba de una vez varias tareas nuevas o ediciones.
  aprobarTareas: (ids: string[]) => void;
  aprobarEdiciones: (tareaIds: string[]) => void;
  advertirTarea: (id: string, mensaje: string) => void;
  aprobarEdicion: (tareaId: string) => void;
  rechazarEdicion: (tareaId: string) => void;

  // revisión
  autorizarMerge: (id: string) => void;
  /** "terminada" -> "en revisión" con motivo obligatorio; respeta LIMITE_REAPERTURAS. */
  reabrirTarea: (id: string, motivo: string) => void;
  rechazarRevision: (id: string, motivo: string) => void;
  asignarRevision: (id: string, liderEmail: string) => void;
  autoasignarRevision: (id: string, email: string) => void;

  // notificaciones
  marcarLeida: (id: string) => void;
  desmarcarLeida: (id: string) => void;
  marcarTodasLeidas: (email: string) => void;

  // usuarios / ajustes
  /** Crea el usuario con su correo y rol (la identidad es `cm whoami`, sin contraseña). Devuelve false si ya existe. */
  crearUsuario: (correo: string, rol: Rol) => boolean;
  confirmarUsuario: (email: string) => void;
  /** Antes de restaurar, el estado actual se guarda como una copia más, tal cual estaba. */
  restaurarBackup: (id: string) => void;
  eliminarUsuario: (email: string) => void;
  /** Cambia el rol; lo firma quien lo cambia (Administrador o Líder). */
  cambiarRol: (email: string, rol: Rol) => void;
  /** Mensaje libre a un compañero: le llega como notificación. */
  enviarMensaje: (para: string, texto: string) => void;
  /** Cambia el nombre del proyecto en BranchBoard (el repositorio de Unity VC conserva el suyo). */
  renombrarProyecto: (nombre: string) => boolean;

  // sprints
  crearSprint: (nombre: string, descripcion: string, con_fecha: boolean, duracion_dias: number | null) => void;
  cerrarSprint: (id: string) => void;
  /** Mete o saca una tarea de un sprint/backlog (null = sin sprint). Desde Planificación. */
  asignarTareaASprint: (tareaId: string, sprintId: string | null) => void;
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
  // Normaliza los estados de la jerarquía con los commits actuales (progreso = commits).
  const normalizar = (tareas: Tarea[]) =>
    normalizarJerarquiaEstados(tareas, new Set(get().commits.map((c) => c.tarea_id)));

  return {
  currentEmail: fx.usuarios[0].email,
  vpnOn: true,
  cmSesionActiva: true,
  accesoCompletado: false,
  proyectoActual: fx.PROYECTO_NOMBRE,
  accesoIrAWorkspaces: false,
  sesionUnityVC: null,
  workspaces: [...fx.workspaces],
  proyectoAClonar: null,

  usuarios: [...fx.usuarios],
  tareas: normalizarJerarquiaEstados([...fx.tareas], new Set(fx.commits.map((c) => c.tarea_id))),
  sprints: [...fx.sprints],
  notificaciones: [...fx.notificaciones],
  edicionesPendientes: [...fx.edicionesPendientes],
  commits: [...fx.commits],
  cambiosTraidos: [...fx.cambiosTraidos],
  cambiosSinCheckin: { ...fx.cambiosSinCheckin },
  entrantesRecibidos: fx.tareas
    .filter((t) => t.estado === 'terminada' && !fx.entrantesSinRecibir.includes(t.id))
    .map((t) => t.id),
  backups: [...fx.backups],
  tamanoActual: '2.4 MB',
  hayNovedadUsuarios: true,
  marcarUsuariosVistos: () => set({ hayNovedadUsuarios: false }),

  toast: null,
  ramaWorkspace: null,
  traerCambios: (destinoId, ramaDestino, origenId, hash) => {
    const origen = get().tareas.find((t) => t.id === origenId);
    const aviso =
      origen?.estado === 'en revisión'
        ? ' — ojo: todavía sin aprobar'
        : origen?.estado === 'en producción'
          ? ' — ojo: es trabajo en curso'
          : '';
    const que = hash ? `Commit ${hash} de ${origenId}` : `Todos los cambios de ${origenId}`;
    const destino = get().tareas.find((t) => t.id === destinoId);
    const recuerda =
      destino?.estado === 'en revisión'
        ? '. Recuerda devolverlos antes de mandarla a terminada'
        : '. Si no hacen falta para la revisión, devuélvelos antes de mandarla; si no, el revisor los devolverá antes de marcarla como terminada';
    const registro: CambioTraido = {
      id: `CT-${Date.now()}`,
      tarea_id: destinoId,
      origen_id: origenId,
      hash: hash ?? null,
      autor: get().currentEmail,
      fecha: new Date().toISOString(),
      // En la app real: cm diff del merge. En el mockup no se conocen los archivos.
      archivos: [],
      archivos_tocados: [],
      quitado_en: null,
      quitado_por: null,
    };
    set((s) => ({ cambiosTraidos: [...s.cambiosTraidos, registro] }));
    get().showToast(`${que} traído${hash ? '' : 's'} a ${ramaDestino} (${destinoId})${aviso}${recuerda}`, 'info');
  },
  quitarCambiosTraidos: (registroId) => {
    const r = get().cambiosTraidos.find((c) => c.id === registroId);
    if (!r || r.quitado_en) return;
    set((s) => ({
      cambiosTraidos: s.cambiosTraidos.map((c) =>
        c.id === registroId ? { ...c, quitado_en: new Date().toISOString(), quitado_por: s.currentEmail } : c
      ),
    }));
    const que = r.hash ? `el commit ${r.hash} de ${r.origen_id}` : `los cambios de ${r.origen_id}`;
    get().showToast(`Se devolvió ${que}: ya no está en ${r.tarea_id}`, 'ok');
  },
  eliminarCambiosAjenos: (registroId) => {
    const r = get().cambiosTraidos.find((c) => c.id === registroId);
    if (!r || r.archivos_tocados.length === 0) return;
    set((s) => ({
      cambiosTraidos: s.cambiosTraidos.map((c) => (c.id === registroId ? { ...c, archivos_tocados: [] } : c)),
    }));
    const n = r.archivos_tocados.length;
    get().showToast(
      `Se eliminó lo que se modificó en ${n === 1 ? '1 archivo prestado' : `${n} archivos prestados`} de ${r.origen_id} (${r.tarea_id})`,
      'ok'
    );
  },
  recibirEntrante: (destinoId, hijoId) => {
    if (get().entrantesRecibidos.includes(hijoId)) return;
    set((s) => ({ entrantesRecibidos: [...s.entrantesRecibidos, hijoId] }));
    get().showToast(`Se unieron a ${destinoId} los cambios de ${hijoId}`, 'ok');
  },
  hacerCheckin: (rama, { titulo: mensaje, descripcion, tipo }) => {
    const s = get();
    const duena = s.tareas.find((t) => t.rama_asociada === rama && !t.eliminada);
    if (!duena || !mensaje) return;
    // En una rama con subtareas no se hace check-in: se crea una rama (ver crearRamaDesdeEspera).
    if (s.tareas.some((t) => t.tarea_padre_id === duena.id && !t.eliminada && !esTareaEnEspera(t))) return;
    const user = s.currentUser();
    const hash = Math.random().toString(16).slice(2, 9).padEnd(7, '0');
    const commit: Commit = {
      id: `C-${hash}`,
      tarea_id: duena.id,
      hash,
      mensaje,
      descripcion: descripcion || undefined,
      tipo,
      autor: user.email,
      fecha: new Date().toISOString().slice(0, 19),
    };
    const sinCheckin = { ...s.cambiosSinCheckin };
    delete sinCheckin[rama];
    set({ commits: [...s.commits, commit], cambiosSinCheckin: sinCheckin });
    set({ tareas: normalizar(get().tareas) });
    get().showToast(`Check-in ${hash} en ${rama}`, 'ok');
  },
  crearRamaDesdeEspera: (esperaId, { titulo, detalles, tipo }) => {
    const s = get();
    const espera = s.tareas.find((t) => t.id === esperaId);
    const padre = espera && s.tareas.find((t) => t.id === espera.tarea_padre_id);
    if (!espera || !padre?.rama_asociada || !titulo) return;
    const user = s.currentUser();
    const slug = titulo
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '')
      .slice(0, 24);
    const rama = `task/${esperaId.toLowerCase()}-${slug || 'cambios'}`;
    const n = s.cambiosSinCheckin[padre.rama_asociada] ?? 0;
    const sinCheckin = { ...s.cambiosSinCheckin };
    delete sinCheckin[padre.rama_asociada];
    if (n > 0) sinCheckin[rama] = n;
    set({
      cambiosSinCheckin: sinCheckin,
      tareas: normalizar(
        s.tareas.map((t) =>
          t.id === esperaId
            ? {
                ...t,
                titulo,
                detalles,
                tipo,
                estado: 'en producción',
                rama_asociada: rama,
                modo_trabajo: 'rama',
                asignado_a: t.asignado_a ?? padre.asignado_a ?? user.email,
              }
            : t
        )
      ),
    });
    get().showToast(`Rama ${rama} creada: los cambios sin check-in pasaron a ${esperaId}`, 'ok');
  },
  abrirEnUnity: (tareaId, rama, hash) => {
    const destino = hash ? `${rama} @ ${hash}` : rama;
    const antes = get().ramaWorkspace;
    set({ ramaWorkspace: destino });
    get().showToast(
      antes === destino
        ? `Abriendo Unity en ${destino} (${tareaId})…`
        : `Workspace cambiado a ${destino} — abriendo Unity ahí (${tareaId})…`,
      'info'
    );
  },

  setEmail: (email) => set({ currentEmail: email }),
  setRolActual: (rol) =>
    set((s) => ({
      usuarios: s.usuarios.map((u) => (u.email === s.currentEmail ? { ...u, rol } : u)),
    })),
  toggleVpn: () => set((s) => ({ vpnOn: !s.vpnOn })),
  toggleCmSesion: () => set((s) => ({ cmSesionActiva: !s.cmSesionActiva })),
  setAccesoCompletado: (v) => set({ accesoCompletado: v, accesoIrAWorkspaces: false, proyectoAClonar: null }),
  cambiarDeProyecto: () => set({ accesoCompletado: false, accesoIrAWorkspaces: true }),
  cerrarSesion: () =>
    set({ cmSesionActiva: false, accesoCompletado: false, accesoIrAWorkspaces: false, sesionUnityVC: null }),
  setSesionUnityVC: (correo) => set({ sesionUnityVC: correo }),
  crearProyectoNuevo: ({ proyecto, nombre, correo }) => {
    const [primero, ...resto] = nombre.trim().split(/\s+/);
    const hoy = today();
    set({
      usuarios: [
        {
          email: correo,
          nombre: primero ?? nombre,
          apellido: resto.join(' '),
          rol: 'Administrador',
          fecha_alta: hoy,
          firma: 'firma-local',
          firmado_en: hoy,
        },
      ],
      currentEmail: correo,
      tareas: [],
      sprints: [],
      notificaciones: [],
      edicionesPendientes: [],
      commits: [],
      cambiosTraidos: [],
      cambiosSinCheckin: {},
      entrantesRecibidos: [],
      backups: [],
      ramaWorkspace: null,
      proyectoActual: proyecto,
      accesoCompletado: true,
      accesoIrAWorkspaces: false,
      proyectoAClonar: null,
      // Aparece en "Tus proyectos". No está en Unity VC: no tiene repositorio ni carpeta de workspace.
      workspaces: [
        ...get().workspaces.filter((w) => w.nombre !== proyecto),
        // Carpeta local del proyecto (sin Unity VC): ahí se guardan sus datos de BranchBoard.
        { nombre: proyecto, repo: `${proyecto}@local`, ruta: `D:\\BranchBoard\\${proyecto}`, tiene_db_branchboard: true },
      ],
    });
    get().showToast(`Proyecto ${proyecto} creado — eres su Administrador`);
  },
  setWorkspaces: (ws) => set({ workspaces: ws }),
  irAClonar: (nombre) => set({ accesoCompletado: false, accesoIrAWorkspaces: false, proyectoAClonar: nombre }),
  entrarAProyecto: (nombre) =>
    set({ proyectoActual: nombre, accesoCompletado: true, accesoIrAWorkspaces: false, proyectoAClonar: null }),
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

  puedeAvanzar: (id, estado) => {
    if (estado !== 'en revisión' && estado !== 'terminada') return true;
    // Los cambios traídos de otras tareas son prestados. A revisión pueden ir (puede que hagan falta
    // para revisarla), con un aviso; a terminada (merge) no: hay que devolverlos antes. Para todos
    // los roles. Cuenta los de la tarea y los de sus subtareas (un check-in trabaja sobre la rama
    // del padre).
    const tareas = get().tareas;
    const ids = new Set([id]);
    for (let creció = true; creció; ) {
      creció = false;
      for (const t of tareas) {
        if (t.tarea_padre_id && ids.has(t.tarea_padre_id) && !ids.has(t.id)) {
          ids.add(t.id);
          creció = true;
        }
      }
    }
    const prestados = get().cambiosTraidos.filter((c) => ids.has(c.tarea_id) && !c.quitado_en);
    const origenes = [...new Set(prestados.map((c) => c.origen_id))].join(', ');
    // Al merge (terminada) no pueden llegar: bloquea.
    if (estado === 'terminada' && prestados.length > 0) {
      get().showToast(
        `Antes de mergear, devuelve los cambios prestados de ${origenes} (en los detalles de la tarea)`,
        'danger'
      );
      return false;
    }
    const pendientes = subtareasPendientes(get().tareas, id, estado);
    if (pendientes.length > 0) {
      get().showToast(avisoSubtareasPendientes(pendientes, estado), 'danger');
      return false;
    }
    // A revisión sí pueden ir (a veces hacen falta para revisarla): solo se avisa.
    if (estado === 'en revisión' && prestados.length > 0) {
      get().showToast(
        `Ojo: va con cambios prestados de ${origenes}. Si no hacen falta para revisarla, devuélvelos antes; si no, el revisor los devolverá antes de marcarla como terminada.`,
        'info'
      );
    }
    return true;
  },

  moverTarea: (id, estado) => {
    if (!get().puedeAvanzar(id, estado)) return;
    if (estado === 'por hacer' && tieneProgreso(get(), id)) return;
    const anterior = get().tareas.find((t) => t.id === id);
    // El estado de una tarea con subtareas se deriva de ellas: si solo se cambiara el padre,
    // normalizarJerarquiaEstados lo devolvería a su columna. Por eso el cambio hacia atrás baja a
    // todo su subárbol. Hacia "en revisión"/"terminada" no hace falta: puedeAvanzar ya exigió que
    // sus subtareas hayan llegado, y bajarlo devolvería a revisión subtareas ya terminadas.
    const propaga = estado !== 'en revisión' && estado !== 'terminada';
    set((s) => {
      const descendientes = propaga ? descendientesDe(s.tareas, id) : new Set<string>();
      return {
        tareas: normalizar(
          s.tareas.map((t) =>
            t.id === id || descendientes.has(t.id)
              ? {
                  ...t,
                  estado,
                  fecha_completada: estado === 'terminada' ? today() : t.fecha_completada,
                }
              : t
          )
        ),
      };
    });
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

  borrarProgreso: (id) => {
    const s = get();
    const ids = new Set([id, ...descendientesDe(s.tareas, id)]);
    const ramas = new Set(
      s.tareas.filter((t) => ids.has(t.id) && t.rama_asociada).map((t) => t.rama_asociada as string)
    );
    const nCommits = s.commits.filter((c) => ids.has(c.tarea_id)).length;
    set((st) => ({
      commits: st.commits.filter((c) => !ids.has(c.tarea_id)),
      cambiosTraidos: st.cambiosTraidos.filter((c) => !ids.has(c.tarea_id)),
      cambiosSinCheckin: Object.fromEntries(Object.entries(st.cambiosSinCheckin).filter(([r]) => !ramas.has(r))),
    }));
    get().moverTarea(id, 'por hacer');
    get().showToast(
      `${id} volvió a "Por hacer": se eliminó su progreso (${nCommits} ${nCommits === 1 ? 'commit' : 'commits'})`,
      'info'
    );
  },

  // Tomar una tarea crea su rama (toda tarea se trabaja en su propia rama).
  tomarTarea: (id, email) => {
    set((s) => ({
      tareas: s.tareas.map((t) => {
        if (t.id !== id) return t;
        return {
          ...t,
          asignado_a: email,
          modo_trabajo: 'rama',
          rama_asociada: t.rama_asociada ?? `task/${id.toLowerCase()}-${t.tipo.slice(0, 4).toLowerCase()}`,
        };
      }),
    }));
  },

  // En una tarea general, sus subtareas también vuelven a "por hacer"; si no, al siguiente
  // cambio el padre se volvería a derivar a otro estado (y heredaría un responsable).
  asignarTarea: (id, email) => {
    const t = get().tareas.find((x) => x.id === id);
    if (!t || t.asignado_a === email) return;
    set((s) => ({
      tareas: s.tareas.map((x) =>
        x.id === id
          ? {
              ...x,
              asignado_a: email,
              modo_trabajo: 'rama',
              rama_asociada: x.rama_asociada ?? `task/${id.toLowerCase()}-${x.tipo.slice(0, 4).toLowerCase()}`,
            }
          : x
      ),
    }));
    const u = get().usuarios.find((x) => x.email === email);
    get().showToast(`${id} asignada a ${u ? `${u.nombre} ${u.apellido}` : email}`);
  },
  desasignarTarea: (id) => {
    if (tieneProgreso(get(), id)) return;
    set((s) => {
      const descendientes = descendientesDe(s.tareas, id);
      return {
        tareas: normalizar(
          s.tareas.map((t) => {
            if (t.id === id) return { ...t, asignado_a: null, estado: 'por hacer' };
            if (descendientes.has(t.id)) return { ...t, estado: 'por hacer' };
            return t;
          })
        ),
      };
    });
  },

  crearTarea: (data) => {
    const s = get();
    const user = s.currentUser();
    const pendiente = !esLiderOAdmin(user.rol);
    const nueva: Tarea = {
      id: nextId(),
      titulo: data.titulo,
      detalles: data.detalles ?? '',
      tipo: data.tipo,
      tarea_padre_id: data.tarea_padre_id,
      archivo_ruta: null,
      estado: 'por hacer',
      pendiente_aprobacion: pendiente,
      rama_asociada: null,
      modo_trabajo: null,
      modificada: false,
      creado_por: user.email,
      fecha_creacion: hoyLocal(),
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
      tarea_origen_id: null,
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
        detalles: st.detalles ?? '',
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

  aprobarTareas: (ids) => {
    const set_ = new Set(ids);
    set((s) => ({
      tareas: s.tareas.map((t) => (set_.has(t.id) ? { ...t, pendiente_aprobacion: false } : t)),
    }));
    get().showToast(`${ids.length} tareas nuevas aprobadas`);
  },

  aprobarEdiciones: (tareaIds) => {
    const set_ = new Set(tareaIds);
    set((s) => ({
      tareas: s.tareas.map((t) => (set_.has(t.id) ? { ...t, edicion_pendiente: false } : t)),
      edicionesPendientes: s.edicionesPendientes.filter((e) => !set_.has(e.tarea_id)),
    }));
    get().showToast(`${tareaIds.length} ediciones aprobadas`);
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

  proponerEdicion: (id, campos) => {
    const s = get();
    const tarea = s.tareas.find((t) => t.id === id);
    if (!tarea) return;
    // Solo en "Sin asignar" / "Por hacer": desde que alguien la toma ya tiene rama en Unity VC.
    if (tarea.estado !== 'por hacer') {
      s.showToast('Solo se puede editar una tarea en "Sin asignar" o "Por hacer"', 'danger');
      return;
    }
    const nuevos: Record<string, string> = {};
    const anteriores: Record<string, string> = {};
    (['titulo', 'detalles'] as const).forEach((k) => {
      if (campos[k] !== tarea[k]) {
        nuevos[k] = campos[k];
        anteriores[k] = tarea[k];
      }
    });
    if (!Object.keys(nuevos).length) return;
    // Si ya había una edición pendiente de esta tarea, se suma a ella conservando los valores
    // anteriores originales (los que se restauran si la rechazan).
    const previa = s.edicionesPendientes.find((e) => e.tarea_id === id);
    const edicion: EdicionPendiente = previa
      ? {
          ...previa,
          campos_nuevos: { ...previa.campos_nuevos, ...nuevos },
          campos_anteriores: { ...anteriores, ...previa.campos_anteriores },
        }
      : {
          id: `EP-${Date.now().toString().slice(-5)}`,
          tarea_id: id,
          campos_nuevos: nuevos,
          campos_anteriores: anteriores,
          motivo: null,
          creado_por: s.currentEmail,
          fecha: today(),
        };
    set({
      tareas: s.tareas.map((t) => (t.id === id ? { ...t, ...nuevos, edicion_pendiente: true } : t)),
      edicionesPendientes: [...s.edicionesPendientes.filter((e) => e.tarea_id !== id), edicion],
    });
    get().showToast('Edición enviada — la aprueba un Administrador o Líder', 'info');
    notificarLideres({
      tipo: 'edicion_pendiente',
      titulo: 'Hay una edición de tarea por aprobar',
      cuerpo: `"${campos.titulo}" (${id}) fue editada (${Object.keys(nuevos).join(', ')}).`,
      tarea_id: id,
      accionable: true,
    });
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
          // !== undefined: unos detalles anteriores vacíos también se restauran.
          if (ep.campos_anteriores.titulo !== undefined) rev.titulo = ep.campos_anteriores.titulo;
          if (ep.campos_anteriores.detalles !== undefined) rev.detalles = ep.campos_anteriores.detalles;
        }
        return { ...t, ...rev };
      }),
      edicionesPendientes: s.edicionesPendientes.filter((e) => e.tarea_id !== tareaId),
    });
    get().showToast('Edición rechazada — se revirtieron los valores', 'danger');
  },

  // Orden de merge de abajo hacia arriba (BACKEND.md): una tarea con subtareas solo se mergea
  // cuando todas ya se mergearon a su rama ("terminada"), y entonces termina solo ella.
  autorizarMerge: (id) => {
    if (!get().puedeAvanzar(id, 'terminada')) return;
    set((s) => {
      const hoy = today();
      return {
        tareas: normalizar(
          s.tareas.map((t) => (t.id === id ? { ...t, estado: 'terminada', fecha_completada: hoy } : t))
        ),
      };
    });
    get().showToast('Merge autorizado — tarea terminada');
  },

  // Las subtareas se quedan "terminada": solo se reabre la rama total. normalizarJerarquiaEstados
  // la deja en "en revisión" porque todas sus hijas ya terminaron.
  reabrirTarea: (id, motivo) => {
    const tarea = get().tareas.find((t) => t.id === id);
    if (!tarea || tarea.estado !== 'terminada') return;
    if (!puedeReabrir(tarea, get().currentUser().rol)) {
      get().showToast(
        `Esta tarea ya se reabrió ${LIMITE_REAPERTURAS} veces — solo el Administrador puede reabrirla otra vez`,
        'danger'
      );
      return;
    }
    const reaperturas = (tarea.reaperturas ?? 0) + 1;
    set((s) => ({
      tareas: normalizar(
        s.tareas.map((t) =>
          t.id === id ? { ...t, estado: 'en revisión', fecha_completada: null, reaperturas } : t
        )
      ),
    }));
    get().showToast(`Tarea reabierta — vuelve a revisión (${reaperturas}/${LIMITE_REAPERTURAS})`, 'info');
    notificarLideres({
      tipo: 'tarea_reabierta',
      titulo: 'Una tarea terminada se reabrió',
      cuerpo: `"${tarea.titulo}" (${id}) volvió a revisión (reapertura ${reaperturas}/${LIMITE_REAPERTURAS}). Motivo: ${motivo}`,
      tarea_id: id,
      accionable: false,
    });
  },

  rechazarRevision: (id, motivo) => {
    const tarea = get().tareas.find((t) => t.id === id);
    // Igual que moverTarea: si es una tarea general, sus subtareas también vuelven a producción.
    set((s) => {
      const descendientes = descendientesDe(s.tareas, id);
      return {
        tareas: normalizar(
          s.tareas.map((t) => {
            if (t.id === id) return { ...t, estado: 'en producción', motivo_rechazo: motivo };
            if (descendientes.has(t.id)) return { ...t, estado: 'en producción' };
            return t;
          })
        ),
      };
    });
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

  crearUsuario: (correo, rol) => {
    const s = get();
    const c = correo.trim().toLowerCase();
    if (s.usuarios.some((u) => u.email.toLowerCase() === c)) {
      s.showToast('Ya existe un usuario con ese correo', 'danger');
      return false;
    }
    // El nombre sale del correo (ana.lopez@… → Ana Lopez) hasta que la persona lo cambie.
    const [nombre = '', apellido = ''] = c
      .split('@')[0]
      .split(/[._-]+/)
      .map((p) => p.charAt(0).toUpperCase() + p.slice(1));
    // Lo firma quien lo crea: el Administrador o un Líder (cada Líder tiene su propia llave, certificada
    // por el Administrador). Así la persona entra a trabajar de una vez.
    const firmante = s.currentUser();
    const nuevo: Usuario = {
      email: c,
      nombre,
      apellido,
      rol,
      fecha_alta: today(),
      firma: `firma-${firmante.email}`,
      firmado_en: today(),
    };
    set({ usuarios: [...s.usuarios, nuevo], hayNovedadUsuarios: true });
    s.showToast(`Usuario ${c} creado como ${rol}`);
    return true;
  },

  restaurarBackup: (id) => {
    const s = get();
    const b = s.backups.find((x) => x.id === id);
    if (!b) return;
    const d = new Date();
    const ahora = `${d.toLocaleDateString('en-CA')} ${d.toTimeString().slice(0, 5)}`;
    const guardada: Backup = {
      id: `B-${Date.now()}`,
      fecha: ahora,
      tamano: s.tamanoActual,
      nota: `Guardada antes de restaurar la copia del ${b.fecha}`,
    };
    set({ backups: [guardada, ...s.backups], tamanoActual: b.tamano });
    s.showToast(`Copia del ${b.fecha} restaurada — la anterior quedó guardada`);
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

  renombrarProyecto: (nombre) => {
    const s = get();
    const nuevo = nombre.trim();
    const anterior = s.proyectoActual;
    if (!nuevo || nuevo === anterior) return false;
    if (s.workspaces.some((w) => w.nombre.toLowerCase() === nuevo.toLowerCase() && w.nombre !== anterior)) {
      s.showToast('Ya tienes otro proyecto con ese nombre', 'danger');
      return false;
    }
    set({
      proyectoActual: nuevo,
      workspaces: s.workspaces.map((w) => (w.nombre === anterior ? { ...w, nombre: nuevo } : w)),
    });
    s.showToast(`Proyecto renombrado a ${nuevo}`);
    return true;
  },

  enviarMensaje: (para, texto) => {
    const s = get();
    const yo = s.currentUser();
    const destino = s.usuarios.find((u) => u.email === para);
    notificar({
      tipo: 'mensaje',
      para,
      titulo: `Mensaje de ${yo.nombre} ${yo.apellido}`.trim(),
      cuerpo: texto.trim(),
      tarea_id: null,
      accionable: false,
    });
    s.showToast(`Mensaje enviado a ${destino ? destino.nombre : para}`);
  },

  cambiarRol: (email, rol) => {
    const s = get();
    const firmante = s.currentUser();
    set({
      usuarios: s.usuarios.map((u) =>
        u.email === email
          ? { ...u, rol, firma: `firma-${firmante.email}`, firmado_en: today() }
          : u
      ),
      hayNovedadUsuarios: true,
    });
    s.showToast(`Rol cambiado a ${rol}`);
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

  asignarTareaASprint: (tareaId, sprintId) => {
    set((s) => ({ tareas: s.tareas.map((t) => (t.id === tareaId ? { ...t, sprint_id: sprintId } : t)) }));
  },

  cerrarSprint: (id) => {
    // Solo el Administrador cierra sprints y backlogs.
    if (get().currentUser().rol !== 'Administrador') {
      get().showToast('Solo el Administrador puede cerrar un sprint', 'danger');
      return;
    }
    set((s) => ({
      sprints: s.sprints.map((sp) =>
        sp.id === id ? { ...sp, cerrado: true, fecha_cierre_real: today() } : sp
      ),
    }));
    get().showToast('Sprint cerrado — se generó su reporte');
  },
  };
});
