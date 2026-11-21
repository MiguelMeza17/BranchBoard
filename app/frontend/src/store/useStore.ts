import { create } from 'zustand';
import type {
  CambioTraido,
  Usuario,
  Tarea,
  Sprint,
  Notificacion,
  EdicionPendiente,
  Backup,
  Documento,
  Mensaje,
  GrupoChat,
  Commit,
  EstadoTarea,
  Rol,
  TipoTarea,
  WorkspaceUnityVC,
} from '../types';
import { CHAT_GRUPO } from '../types';
import * as ejemplo from '../data/fixtures';
import { api, enTauri, ErrorBackend, mensajeDeError, type AlertaRamaAjena, type CambiosEntrantes, type ConflictoMerge, type ProyectoServidor, type ResumenPolling, type UsuarioVisible } from '../lib/api';
import { esTareaEnEspera, ramaDeTarea } from '../lib/ui';
import type { Cuenta } from '../lib/cuentas';
import { esLiderOAdmin, puedeTomarTarea } from '../lib/permisos';

// Datos de ejemplo del mockup (el proyecto «VirusTF» inventado, sus usuarios, tareas, chat…): solo en el
// navegador. En la app de escritorio el store arranca vacío y cada pantalla lee del backend, para que
// lo de ejemplo nunca se mezcle con el proyecto real.
const fx: typeof ejemplo = enTauri()
  ? {
      ...ejemplo,
      workspaces: [],
      proyectosServidor: [],
      usuarios: [],
      sprints: [],
      tareas: [],
      cambiosSinCheckin: {},
      entrantesSinRecibir: [],
      cambiosTraidos: [],
      commits: [],
      edicionesPendientes: [],
      gruposChat: [],
      mensajes: [],
      documentos: [],
      backups: [],
      notificaciones: [],
    }
  : ejemplo;
/** Quien aún no entró a ningún proyecto (app de escritorio antes de `entrar_a_proyecto`): sin permisos. */
const USUARIO_SIN_SESION: Usuario = {
  email: '',
  nombre: '',
  apellido: '',
  rol: 'Colaborador externo',
  fecha_alta: '',
};

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
/** Parte del título en el nombre de la rama, igual que el backend (`ramas.rs`). */
function tituloParaRama(titulo: string): string {
  const texto = titulo
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  if (texto.length <= 30) return texto;
  const corto = texto.slice(0, 30);
  const i = corto.lastIndexOf('-');
  return i > 0 ? corto.slice(0, i) : corto;
}

/** Normaliza lo que devuelve `cm status --header`: `/main`/`/main/dev` -> `null`, `cs:<n>` -> `<rama> @ <n>`. */
function resolverRamaWorkspace(ramaRaw: string | null, tareas: Tarea[], commits: Commit[]): string | null {
  if (!ramaRaw) return null;
  const limpia = ramaRaw.trim().replace(/\/+$/, '');
  const lower = limpia.toLowerCase();
  if (!limpia || lower === '/main/dev' || lower === '/main') return null;
  const matchCs = /^cs:(\d+)$/i.exec(limpia);
  if (matchCs) {
    const cs = matchCs[1];
    const commit = commits.find((c) => c.hash === cs);
    const tarea = commit ? tareas.find((t) => t.id === commit.tarea_id) : undefined;
    const rama = tarea ? ramaDeTarea(tarea, tareas) : null;
    return rama ? `${rama} @ ${cs}` : limpia;
  }
  return limpia;
}

/**
 * Ruta completa de la rama de una tarea, como la crea el backend (#83): una tarea
 * general cuelga de `/main/dev`; una subtarea, de la rama de su tarea padre.
 */
function rutaRama(tareas: Tarea[], t: Tarea): string {
  const padre = t.tarea_padre_id ? tareas.find((p) => p.id === t.tarea_padre_id) : undefined;
  const base = padre ? (padre.rama_asociada ?? rutaRama(tareas, padre)) : '/main/dev';
  const parte = tituloParaRama(t.titulo);
  return `${base}/${parte ? `${t.id}-${parte}` : t.id}`;
}

/**
 * Al tomar o asignar una subtarea, sus ancestros sin rama reciben la suya
 * (decidido 2026-11-14, igual que `crear_ramas_de_cadena` en el backend): la rama
 * de una subtarea vive dentro de la de su padre. Solo se les pone la rama, no
 * un responsable.
 */
function conRamasDeAncestros(tareas: Tarea[], id: string): Tarea[] {
  const cadena: Tarea[] = [];
  let actual = tareas.find((t) => t.id === id);
  while (actual?.tarea_padre_id) {
    const padre = tareas.find((t) => t.id === actual?.tarea_padre_id);
    if (!padre) break;
    cadena.unshift(padre);
    actual = padre;
  }
  let lista = tareas;
  for (const p of cadena) {
    if (p.rama_asociada) continue;
    const rama = rutaRama(lista, p);
    lista = lista.map((t) => (t.id === p.id ? { ...t, rama_asociada: rama } : t));
  }
  return lista;
}

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
/** Alerta de rama ajena (#111) que la persona ya cerró: no se vuelve a mostrar igual. */
let alertaRamaAjenaCerrada: string | null = null;
const claveAlertaRamaAjena = (a: AlertaRamaAjena) => `${a.rama_actual}|${a.archivos.join(',')}`;

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
    : 'En revisión, sin aprobar';
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
  backlog_id: string | null;
  con_fecha: boolean;
  fecha_limite: string | null;
}

interface State {
  // sesión simulada
  currentEmail: string;
  vpnOn: boolean;
  vpnInstalado: boolean;
  vpnCargando: boolean;
  vpnInstalando: boolean;
  cmInstalado: boolean;
  cmInstalando: boolean;
  // Simula el resultado de `cm whoami` al arrancar — en false, la app muestra la
  // pantalla de "no hay sesión de Unity VC" en vez de los tableros (ver FRONTEND.md,
  // "Autenticación / acceso al proyecto"). Toggle de demo en el Topbar.
  cmSesionActiva: boolean;
  /**
   * "Continuar sin Unity VC": deja pasar a la pantalla de acceso (y a un proyecto local) sin sesión de
   * Unity VC. Ver "Proyectos sin Unity VC" en FRONTEND.md.
   */
  modoSinUnityVc: boolean;
  /** Cuenta de BranchBoard con la que se inició sesión en "Bienvenido" (lib/cuentas.ts). */
  cuentaBranchBoard: Cuenta | null;
  /** Tras iniciar sesión: se entra a BranchBoard sin proyecto, sin rol y sin datos ("No estás en ningún proyecto"). */
  entrarSinProyecto: (cuenta: Cuenta) => void;
  /** Cerrar sesión de la cuenta de BranchBoard: sale del proyecto y vuelve a "Bienvenido". La sesión de Unity VC no se toca. */
  cerrarSesionBranchBoard: () => void;
  setModoSinUnityVc: (activo: boolean) => void;
  /** Carpeta del proyecto local abierto (sin Unity VC), o `null` si el proyecto es de Unity VC. */
  proyectoLocal: string | null;
  sesionUnityVcCaducada: boolean;
  marcarSesionUnityVcCaducada: () => void;
  // Si ya se pasó por las pantallas de acceso al proyecto (Pantalla inicial →
  // workspaces → validación del .db). En el mockup nunca bloquea: se pueden saltar.
  accesoCompletado: boolean;
  /** Proyecto (workspace de Unity VC) en el que se entró. */
  proyectoActual: string;
  /** Workspace de Unity VC abierto (varios workspaces pueden ser del mismo proyecto). */
  workspaceActual: string | null;
  /** "Cambiar de proyecto": vuelve al acceso directo a la lista de workspaces (sin la pantalla inicial). */
  accesoIrAWorkspaces: boolean;
  cambiarDeProyecto: () => void;
  /**
   * Cerrar sesión: BranchBoard no tiene login propio, la identidad es la sesión de Unity VC
   * (`cm whoami`). Cerrarla deja la app en "No se detectó una sesión de Unity VC" y, al volver a
   * iniciarla, se pasa otra vez por el acceso al proyecto.
   */
  cerrarSesion: () => Promise<void> | void;
  cargarSesionUnityVc: () => Promise<void>;
  reconectarUnityVc: (org?: string) => Promise<boolean>;
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
  /** Repositorios del servidor de Unity VC (`cm repo list`, #125). */
  proyectosServidor: ProyectoServidor[];
  /** Proyecto del servidor que se eligió para clonar (lleva al acceso, paso "Clonar"). */
  proyectoAClonar: string | null;
  proyectoRepoAClonar: string | null;
  irAClonar: (nombre: string, repo?: string | null) => void;
  cargarWorkspacesYProyectos: () => Promise<void>;
  /** `workspace`: el workspace de Unity VC por el que se entró, si su nombre no es el del proyecto. */
  entrarAProyecto: (nombre: string, workspace?: string, carpetaLocal?: string) => void;
  /**
   * Sube cada vez que llegan cambios de otras máquinas (`sincronizar_eventos`). Las pantallas que leen
   * del backend lo ponen como dependencia para volver a pedir sus datos solas.
   */
  versionDatos: number;
  subirVersionDatos: () => void;
  /** Recarga la lista de tareas desde el backend (`listar_tareas`, #95). */
  cargarTareas: () => Promise<void>;
  /** Recarga la lista de sprints y sus reportes desde el backend (`listar_sprints`, #113, #114). */
  cargarSprints: () => Promise<void>;
  /** Recarga los check-ins cacheados en SQLite (`listar_commits`, #112). */
  cargarCommits: () => Promise<void>;
  /** Consulta la rama real del workspace en Unity VC (`cambios_pendientes`, #112). */
  refrescarRamaWorkspace: () => Promise<void>;
  /** Aplica al store un resumen de polling emitido por Unity VC (`unity-vc-actualizado`, #104, #112). */
  aplicarResumenPolling: (resumen: ResumenPolling) => Promise<void>;

  // datos
  usuarios: Usuario[];
  usuariosVisibles: UsuarioVisible[];
  usuariosCargando: boolean;
  equipoOcupado: boolean;
  errorUsuarios: string | null;
  revisionEquipo: number;
  cargarUsuarios: () => Promise<boolean>;
  tareas: Tarea[];
  sprints: Sprint[];
  notificaciones: Notificacion[];
  edicionesPendientes: EdicionPendiente[];
  backups: Backup[];
  /** Chat entre usuarios. */
  mensajes: Mensaje[];
  /** Grupos de chat creados por el equipo (el general no está aquí: siempre existe). */
  gruposChat: GrupoChat[];
  crearGrupoChat: (nombre: string, miembros: string[]) => string;
  /** Conversaciones que eliminaste: id → desde cuándo. Solo para ti; si llega un mensaje nuevo, vuelve a aparecer. */
  chatsEliminados: Record<string, string>;
  eliminarChat: (conversacion: string) => void;
  enviarChat: (para: string, texto: string) => void;
  /** Marca como leída la conversación: con esa persona, o el grupo (CHAT_GRUPO). */
  marcarChatLeido: (conversacion: string) => void;
  /** Documentos del proyecto (apartado con el nombre del proyecto). */
  documentos: Documento[];
  /** Mockup del navegador: los archivos quedan en memoria. */
  subirDocumentos: (archivos: File[]) => void;
  /**
   * App de escritorio: sube los archivos por su ruta en el equipo (`subir_documentos`, #120); el backend
   * los sube a Storage cuando el proyecto está en el servidor. Las rutas salen del explorador de archivos o de soltarlos en la ventana.
   */
  subirDocumentosRutas: (rutas: string[]) => Promise<void>;
  /** Trae los documentos del proyecto desde el backend (#120); en el navegador no cambia nada. */
  cargarDocumentos: () => Promise<void>;
  eliminarDocumento: (id: string) => void;
  /** Visto bueno de un Líder/Administrador a un documento pendiente. */
  aprobarDocumento: (id: string) => void;
  abrirDocumento: (id: string) => void;
  /** Tamaño de la base de datos actual (la que va arriba como "Actual" en Copias de seguridad). */
  tamanoActual: string;
  /**
   * Puntos de aviso del menú: huella de los datos de cada panel la última vez que lo abriste. Si la
   * huella actual es distinta, hay algo nuevo y se enciende el punto; al entrar al panel se apaga.
   */
  vistos: Record<string, string>;
  marcarVisto: (ruta: string, huella: string) => void;
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
   * Une al workspace TODOS los incoming changes de la rama de `destinoId` (en Unity VC, `cm update .`,
   * #110): Unity VC no deja unir una sola subtarea, así que llegan todas juntas. No es obligatorio ni
   * automático. No es un préstamo: el cambio ya es de esta rama. Devuelve null si falló.
   */
  unirEntrantes: (destinoId: string) => Promise<CambiosEntrantes | null>;
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
  cargarEstadoWarp: () => Promise<void>;
  toggleVpn: () => Promise<void>;
  instalarWarp: () => Promise<void>;
  cargarEstadoCm: () => Promise<void>;
  marcarCmNoInstalado: () => void;
  instalarCm: () => Promise<void>;
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
  conflictoMerge: (ConflictoMerge & { tareaId: string }) | null;
  autorizandoMerge: boolean;
  cerrarConflictoMerge: () => void;
  /** "Resolver en Unity VC" (#207): `true` si se abrió Unity VC en la rama destino. */
  resolverConflictoEnUnityVc: () => Promise<boolean>;
  autorizarMerge: (id: string) => Promise<void>;
  /**
   * Tarea roja (#225), solo Líder/Administrador: pasa los check-ins directos de `main`/`dev` a la rama
   * de la tarea y los revierte de ahí. `true` si se trasladaron.
   */
  trasladarCambiosDirectos: (id: string) => Promise<boolean>;
  trasladandoCambiosDirectos: boolean;
  /** Alerta de #111 (cambios en `main`, `dev` o la rama de otra persona en tu workspace). */
  alertaRamaAjena: AlertaRamaAjena | null;
  mostrarAlertaRamaAjena: (alerta: AlertaRamaAjena) => void;
  cerrarAlertaRamaAjena: () => void;
  /** "terminada" -> "en revisión" con motivo obligatorio; respeta LIMITE_REAPERTURAS. */
  reabrirTarea: (id: string, motivo: string) => void;
  rechazarRevision: (id: string, motivo: string) => Promise<boolean>;
  asignarRevision: (id: string, liderEmail: string) => Promise<boolean>;
  autoasignarRevision: (id: string, email: string) => void;

  // notificaciones
  marcarLeida: (id: string) => void;
  desmarcarLeida: (id: string) => void;
  marcarTodasLeidas: (email: string) => void;

  // usuarios / ajustes
  /** Crea el usuario con su correo y rol (la identidad es `cm whoami`, sin contraseña). Devuelve false si ya existe. */
  crearUsuario: (correo: string, rol: Rol) => Promise<boolean>;
  /** Antes de restaurar, el estado actual se guarda como una copia más, tal cual estaba. */
  restaurarBackup: (id: string) => void;
  eliminarUsuario: (email: string) => Promise<boolean>;
  /** Cambia el rol (con el proyecto en el servidor lo hace cumplir el servidor, #241). */
  cambiarRol: (email: string, rol: Rol) => Promise<boolean>;
  /** Mensaje libre a un compañero: le llega como notificación. */
  enviarMensaje: (para: string, texto: string) => void;
  /** Cambia el nombre del proyecto en BranchBoard (el repositorio de Unity VC conserva el suyo). */
  renombrarProyecto: (nombre: string) => boolean;

  // sprints
  crearSprint: (nombre: string, descripcion: string, con_fecha: boolean, duracion_dias: number | null) => void;
  cerrarSprint: (id: string) => void;
  /** Mete o saca una tarea de un sprint/backlog (null = sin sprint). Desde Planificación. */
  asignarTareaASprint: (tareaId: string, sprintId: string | null) => void;
  asignarTareaABacklog: (tareaId: string, backlogId: string | null) => void;
  // El sprint toma de una vez todas las tareas de un backlog (o las sin backlog, con null) que no estén
  // en otro sprint.
  asignarBacklogASprint: (backlogId: string | null, sprintId: string) => void;
}

// `true` si quitar a `email` (rol `null`) o darle `rol` deja al proyecto sin Administrador (#241).
function quedariaSinAdministrador(usuarios: Usuario[], email: string, rol: Rol | null): boolean {
  const admins = usuarios.filter((u) => u.rol === 'Administrador' && !u.invitado);
  const esAdmin = admins.some((u) => u.email === email);
  return esAdmin && rol !== 'Administrador' && admins.length === 1;
}

export const useStore = create<State>((set, get) => {
  let lecturaEquipo = 0;
  const contextoEquipo = () => {
    const s = get();
    return `${s.revisionEquipo}\n${s.proyectoActual}\n${s.currentEmail}`;
  };
  const cambiarEquipo = async (operacion: () => Promise<unknown>, mensaje: string): Promise<boolean> => {
    if (get().equipoOcupado) return false;
    const contexto = contextoEquipo();
    set({ equipoOcupado: true, errorUsuarios: null });
    try {
      await operacion();
      if (contextoEquipo() !== contexto) return false;
      const actualizado = await get().cargarUsuarios();
      if (contextoEquipo() !== contexto) return false;
      get().showToast(actualizado ? mensaje : `${mensaje}. No se pudo actualizar la lista; pulsa Actualizar.`, actualizado ? 'ok' : 'info');
      return true;
    } catch (e) {
      if (contextoEquipo() === contexto) {
        set({ errorUsuarios: mensajeDeError(e) });
        get().showToast(mensajeDeError(e), 'danger');
      }
      return false;
    } finally {
      if (contextoEquipo() === contexto) set({ equipoOcupado: false });
    }
  };
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
  currentEmail: fx.usuarios[0]?.email ?? '',
  vpnOn: true,
  vpnInstalado: true,
  vpnCargando: false,
  vpnInstalando: false,
  cmInstalado: true,
  cmInstalando: false,
  cmSesionActiva: true,
  modoSinUnityVc: false,
  cuentaBranchBoard: null,
  entrarSinProyecto: (cuenta) =>
    set({
      cuentaBranchBoard: cuenta,
      revisionEquipo: get().revisionEquipo + 1,
      usuariosVisibles: [], usuariosCargando: false, equipoOcupado: false, errorUsuarios: null,
      currentEmail: cuenta.correo,
      usuarios: [],
      tareas: [],
      proyectoActual: '',
      workspaceActual: null,
      proyectoLocal: null,
      accesoCompletado: true,
      accesoIrAWorkspaces: false,
    }),
  cerrarSesionBranchBoard: () => {
    // Borra la sesión de Supabase del llavero (y avisa al servidor si hay red, #236).
    void api.cerrarSesionBranchBoard().catch(() => undefined);
    set({
      cuentaBranchBoard: null,
      currentEmail: '',
      revisionEquipo: get().revisionEquipo + 1,
      usuariosVisibles: [], usuariosCargando: false, equipoOcupado: false, errorUsuarios: null,
      usuarios: [],
      tareas: [],
      proyectoActual: '',
      workspaceActual: null,
      proyectoLocal: null,
      proyectoAClonar: null,
      proyectoRepoAClonar: null,
      accesoIrAWorkspaces: false,
      accesoCompletado: false,
      // Sin sesión de Unity VC, esto deja ver el login de BranchBoard en vez de la pantalla de Unity VC.
      modoSinUnityVc: true,
    });
  },
  setModoSinUnityVc: (activo) => set({ modoSinUnityVc: activo }),
  proyectoLocal: null,
  sesionUnityVcCaducada: false,
  // Que caduque Unity VC no saca a nadie del proyecto (#124): sus tareas siguen funcionando y solo
  // espera lo que necesita Unity VC (ramas, check-in, merge) hasta que pulse "Reconectar" en el aviso.
  marcarSesionUnityVcCaducada: () =>
    set({ cmSesionActiva: false, sesionUnityVC: null, sesionUnityVcCaducada: true }),
  accesoCompletado: false,
  proyectoActual: enTauri() ? '' : fx.PROYECTO_NOMBRE,
  workspaceActual: enTauri() ? null : fx.PROYECTO_NOMBRE,
  accesoIrAWorkspaces: false,
  sesionUnityVC: null,
  workspaces: [...fx.workspaces],
  proyectosServidor: fx.proyectosServidor.map((p) => ({ ...p, org: 'MIGUELMEZA' })),
  proyectoAClonar: null,
  proyectoRepoAClonar: null,
  versionDatos: 0,
  subirVersionDatos: () => set((s) => ({ versionDatos: s.versionDatos + 1 })),
  cargarTareas: async () => {
    try {
      const lista = await api.listarTareas(() => get().tareas);
      let ediciones = get().edicionesPendientes;
      if (enTauri()) {
        try {
          ediciones = await api.listarEdicionesPendientes(() => get().edicionesPendientes);
        } catch {
          // Si no está listo o falla, mantener lo existente
        }
      }
      set({ tareas: normalizar(lista), edicionesPendientes: ediciones });
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
    }
  },
  cargarSprints: async () => {
    try {
      const lista = await api.listarSprints(() => get().sprints);
      set({ sprints: lista });
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
    }
  },
  cargarCommits: async () => {
    try {
      const lista = await api.listarCommits(null, () => get().commits);
      set((s) => ({
        commits: lista,
        tareas: normalizarJerarquiaEstados(s.tareas, new Set(lista.map((c) => c.tarea_id))),
      }));
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
    }
  },
  refrescarRamaWorkspace: async () => {
    if (!enTauri()) return;
    try {
      const res = await api.cambiosPendientes(null);
      set((s) => ({
        ramaWorkspace: resolverRamaWorkspace(res.rama_actual, s.tareas, s.commits),
      }));
    } catch {
      // Si no hay workspace local configurado aún (p. ej. rol no técnico), queda en null.
    }
  },
  aplicarResumenPolling: async (resumen) => {
    if (!enTauri()) return;
    if (resumen.rama_actual !== undefined) {
      set((s) => ({
        ramaWorkspace: resolverRamaWorkspace(resumen.rama_actual, s.tareas, s.commits),
      }));
    }
    const hayCambiosDatos =
      resumen.tareas_modificadas.length > 0 ||
      resumen.tareas_cerradas.length > 0 ||
      !!resumen.espera_creada ||
      !!resumen.espera_quitada ||
      (resumen.tocados_detectados?.length ?? 0) > 0 ||
      (resumen.cambios_directos?.length ?? 0) > 0;
    if (hayCambiosDatos) {
      await get().cargarCommits();
      await get().cargarTareas();
      get().subirVersionDatos();
    }
  },

  usuarios: [...fx.usuarios],
  usuariosVisibles: [],
  usuariosCargando: false,
  equipoOcupado: false,
  errorUsuarios: null,
  revisionEquipo: 0,
  cargarUsuarios: async () => {
    const contexto = contextoEquipo();
    const lectura = ++lecturaEquipo;
    set({ usuariosCargando: true, errorUsuarios: null });
    try {
      const filas = await api.listarUsuarios(() => get().usuarios.map((u) => ({ ...u, propio: u.email === get().currentEmail })));
      if (contextoEquipo() !== contexto || lectura !== lecturaEquipo) return false;
      if (enTauri()) {
        const usuarios: Usuario[] = filas.flatMap((u) => {
          const correo = u.email ?? (u.propio ? get().currentEmail : null);
          return correo ? [{ ...u, email: correo }] : [];
        });
        set({ usuarios, usuariosVisibles: filas });
      } else set({ usuariosVisibles: filas });
      return true;
    } catch (e) {
      if (contextoEquipo() === contexto && lectura === lecturaEquipo) {
        set({ errorUsuarios: mensajeDeError(e) });
        if (e instanceof ErrorBackend && e.codigo === 'sin_permiso') set({ usuarios: [], usuariosVisibles: [] });
      }
      return false;
    } finally {
      if (contextoEquipo() === contexto && lectura === lecturaEquipo) set({ usuariosCargando: false });
    }
  },
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
  mensajes: [...fx.mensajes],
  gruposChat: [...fx.gruposChat],
  chatsEliminados: {},
  eliminarChat: (conversacion) => {
    get().marcarChatLeido(conversacion);
    set((s) => ({ chatsEliminados: { ...s.chatsEliminados, [conversacion]: new Date().toISOString() } }));
    get().showToast('Chat eliminado');
  },
  crearGrupoChat: (nombre, miembros) => {
    const s = get();
    const g: GrupoChat = {
      id: `GR-${Date.now()}`,
      nombre: nombre.trim(),
      miembros: Array.from(new Set([s.currentEmail, ...miembros])),
      creado_por: s.currentEmail,
      fecha: today(),
    };
    set({ gruposChat: [...s.gruposChat, g] });
    s.showToast(`Grupo "${g.nombre}" creado`);
    return g.id;
  },
  enviarChat: (para, texto) => {
    const s = get();
    const m: Mensaje = {
      id: `M-${Date.now()}`,
      de: s.currentEmail,
      para,
      texto: texto.trim(),
      fecha: new Date().toISOString(),
      leidoPor: [s.currentEmail],
    };
    set({ mensajes: [...s.mensajes, m] });
  },
  marcarChatLeido: (conversacion) => {
    const s = get();
    const yo = s.currentEmail;
    const esGrupo = conversacion === CHAT_GRUPO || s.gruposChat.some((g) => g.id === conversacion);
    const esDeLaConversacion = (m: Mensaje) =>
      esGrupo ? m.para === conversacion : m.de === conversacion && m.para === yo;
    if (!s.mensajes.some((m) => esDeLaConversacion(m) && !m.leidoPor.includes(yo))) return;
    set({
      mensajes: s.mensajes.map((m) =>
        esDeLaConversacion(m) && !m.leidoPor.includes(yo) ? { ...m, leidoPor: [...m.leidoPor, yo] } : m
      ),
    });
  },
  documentos: [...fx.documentos],
  subirDocumentos: (archivos) => {
    const s = get();
    const nuevos: Documento[] = archivos.map((f, i) => ({
      id: `D-${Date.now()}-${i}`,
      nombre: f.name,
      tamano: f.size,
      subido_por: s.currentEmail,
      fecha: today(),
      // Líder/Administrador lo publican directo; los demás esperan su visto bueno en Pendientes.
      pendiente: !esLiderOAdmin(s.currentUser().rol),
      url: URL.createObjectURL(f),
    }));
    set({ documentos: [...nuevos, ...s.documentos] });
    const que = nuevos.length === 1 ? nuevos[0].nombre : `${nuevos.length} documentos`;
    s.showToast(nuevos[0].pendiente ? `${que}: pendiente del visto bueno de un Líder o Administrador` : `${que} subido`);
  },
  subirDocumentosRutas: async (rutas) => {
    if (!rutas.length) return;
    try {
      const nuevos = await api.subirDocumentos(rutas);
      await get().cargarDocumentos();
      if (!nuevos.length) return;
      const que = nuevos.length === 1 ? nuevos[0].nombre : `${nuevos.length} documentos`;
      get().showToast(
        nuevos.some((d) => d.pendiente) ? `${que}: pendiente del visto bueno de un Líder o Administrador` : `${que} subido`
      );
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
    }
  },
  cargarDocumentos: async () => {
    if (!enTauri()) return;
    try {
      set({ documentos: await api.listarDocumentos() });
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
    }
  },
  eliminarDocumento: (id) => {
    if (enTauri()) {
      void (async () => {
        try {
          await api.eliminarDocumento(id);
          await get().cargarDocumentos();
          get().showToast('Documento eliminado', 'danger');
        } catch (e) {
          get().showToast(mensajeDeError(e), 'danger');
        }
      })();
      return;
    }
    const d = get().documentos.find((x) => x.id === id);
    if (d?.url) URL.revokeObjectURL(d.url);
    set((s) => ({ documentos: s.documentos.filter((x) => x.id !== id) }));
    get().showToast('Documento eliminado', 'danger');
  },
  aprobarDocumento: (id) => {
    if (enTauri()) {
      void (async () => {
        try {
          await api.aprobarDocumento(id);
          await get().cargarDocumentos();
          get().showToast('Documento aprobado');
        } catch (e) {
          get().showToast(mensajeDeError(e), 'danger');
        }
      })();
      return;
    }
    set((s) => ({ documentos: s.documentos.map((d) => (d.id === id ? { ...d, pendiente: false } : d)) }));
    get().showToast('Documento aprobado');
  },
  abrirDocumento: (id) => {
    const d = get().documentos.find((x) => x.id === id);
    if (!d) return;
    if (enTauri()) {
      void api.abrirDocumento(id).catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    // Los subidos en esta sesión tienen su archivo; los de ejemplo no (en la app real se abren desde la carpeta del proyecto).
    if (d.url) window.open(d.url, '_blank', 'noopener');
    else get().showToast(`Abriendo ${d.nombre}…`, 'info');
  },
  tamanoActual: '2.4 MB',
  vistos: {},
  marcarVisto: (ruta, huella) =>
    set((s) => (s.vistos[ruta] === huella ? s : { vistos: { ...s.vistos, [ruta]: huella } })),

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
  unirEntrantes: async (destinoId) => {
    const s = get();
    const hijos = entrantesDe(s.tareas, s.entrantesRecibidos, destinoId).map((h) => h.id);
    const destino = s.tareas.find((t) => t.id === destinoId);
    try {
      const res = await api.unirCambiosEntrantes(destinoId, () => ({
        rama: destino?.rama_asociada ?? '',
        rama_actual: destino?.rama_asociada ?? '',
        en_rama: true,
        changeset_workspace: null,
        cabeza: null,
        changesets: [],
      }));
      if (!enTauri()) set((st) => ({ entrantesRecibidos: [...st.entrantesRecibidos, ...hijos] }));
      get().showToast(`Tu workspace ya tiene todos los cambios entrantes de ${destinoId}`, 'ok');
      get().subirVersionDatos();
      return res;
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
      return null;
    }
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
    const parte = tituloParaRama(titulo);
    const rama = `${padre.rama_asociada}/${parte ? `${esperaId}-${parte}` : esperaId}`;
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
    if (enTauri()) {
      void api
        .abrirEnUnity(tareaId, rama, hash ?? null)
        .then((res) => {
          const cs = res.changeset ?? (hash ? Number(hash) || null : null);
          const limpia = resolverRamaWorkspace(res.rama_actual, get().tareas, get().commits);
          const base = limpia && !/^cs:\d+$/i.test(limpia) ? limpia : rama;
          const visible = base && cs ? `${base} @ ${cs}` : limpia;
          set({ ramaWorkspace: visible });
          get().showToast(`Abriendo Unity en ${visible ?? destino} (${tareaId})…`, 'info');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    const antes = get().ramaWorkspace;
    set({ ramaWorkspace: destino });
    get().showToast(
      antes === destino
        ? `Abriendo Unity en ${destino} (${tareaId})…`
        : `Workspace cambiado a ${destino} — abriendo Unity ahí (${tareaId})…`,
      'info'
    );
  },

  setEmail: (email) => set({ currentEmail: email, revisionEquipo: get().revisionEquipo + 1, usuariosVisibles: [], usuariosCargando: false, equipoOcupado: false, errorUsuarios: null }),
  setRolActual: (rol) =>
    set((s) => ({
      usuarios: s.usuarios.map((u) => (u.email === s.currentEmail ? { ...u, rol } : u)),
    })),
  cargarEstadoWarp: async () => {
    if (!enTauri()) return;
    try {
      const estado = await api.estadoWarp();
      set({ vpnInstalado: estado.instalado, vpnOn: estado.conectado });
    } catch {
      // Si la consulta falla al arrancar, no bloquea el resto de la pantalla inicial.
    }
  },
  toggleVpn: async () => {
    if (!enTauri()) {
      set((s) => ({ vpnOn: !s.vpnOn }));
      return;
    }
    if (get().vpnCargando || get().vpnInstalando) return;
    set({ vpnCargando: true });
    try {
      const estado = await api.alternarWarp();
      set({ vpnInstalado: estado.instalado, vpnOn: estado.conectado, vpnCargando: false });
    } catch (e) {
      set({ vpnCargando: false });
      get().showToast(mensajeDeError(e), 'danger');
      void get().cargarEstadoWarp();
    }
  },
  instalarWarp: async () => {
    if (!enTauri()) {
      get().showToast('En la app de escritorio se instalará Cloudflare WARP con winget.', 'info');
      return;
    }
    if (get().vpnInstalando) return;
    set({ vpnInstalando: true });
    try {
      const res = await api.instalarWarp();
      set({
        vpnInstalado: res.estado.instalado,
        vpnOn: res.estado.conectado,
        vpnInstalando: false,
      });
      get().showToast(res.mensaje, res.instalado_ahora ? 'ok' : 'info');
    } catch (e) {
      set({ vpnInstalando: false });
      get().showToast(mensajeDeError(e), 'danger');
      void get().cargarEstadoWarp();
    }
  },
  cargarEstadoCm: async () => {
    if (!enTauri()) return;
    try {
      const estado = await api.estadoCm();
      set({ cmInstalado: estado.instalado });
    } catch {
      // No bloquea la pantalla inicial si la consulta falla.
    }
  },
  marcarCmNoInstalado: () => set({ cmInstalado: false }),
  instalarCm: async () => {
    if (!enTauri()) {
      get().showToast('En la app de escritorio se instalará Unity Version Control con winget.', 'info');
      return;
    }
    if (get().cmInstalando) return;
    set({ cmInstalando: true });
    try {
      const res = await api.instalarCm();
      set({ cmInstalado: res.estado.instalado, cmInstalando: false });
      get().showToast(res.mensaje, res.instalado_ahora ? 'ok' : 'info');
    } catch (e) {
      set({ cmInstalando: false });
      get().showToast(mensajeDeError(e), 'danger');
      void get().cargarEstadoCm();
    }
  },
  toggleCmSesion: () => set((s) => ({ cmSesionActiva: !s.cmSesionActiva })),
  setAccesoCompletado: (v) =>
    set({ accesoCompletado: v, accesoIrAWorkspaces: false, proyectoAClonar: null, proyectoRepoAClonar: null }),
  // Desde un proyecto local se vuelve a las opciones sin Unity VC (no a la lista de workspaces).
  cambiarDeProyecto: () => set((s) => ({ accesoCompletado: false, accesoIrAWorkspaces: !s.proyectoLocal })),
  cerrarSesion: async () => {
    if (!enTauri()) {
      set({ cmSesionActiva: false, accesoCompletado: false, accesoIrAWorkspaces: false, sesionUnityVC: null });
      return;
    }
    try {
      await api.cerrarSesionUnityVc();
      set({ cmSesionActiva: false, accesoCompletado: false, accesoIrAWorkspaces: false, sesionUnityVC: null });
      get().showToast('Sesión de Unity VC cerrada', 'info');
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
    }
  },
  cargarSesionUnityVc: async () => {
    if (!enTauri()) return;
    if (get().sesionUnityVcCaducada) return;
    try {
      const s = await api.sesionUnityVc();
      // Una consulta local iniciada antes del error no debe ocultar el aviso.
      if (get().sesionUnityVcCaducada) return;
      if (s.activa && s.email) {
        set({ cmInstalado: true, cmSesionActiva: true, sesionUnityVC: s.email, currentEmail: s.email });
        void get().cargarWorkspacesYProyectos();
      } else {
        set({ cmSesionActiva: false, sesionUnityVC: null, accesoCompletado: false });
      }
    } catch (e) {
      if (e instanceof ErrorBackend && e.codigo === 'cm_no_instalado') {
        set({ cmInstalado: false, cmSesionActiva: false, sesionUnityVC: null, accesoCompletado: false });
      } else if (e instanceof ErrorBackend && e.codigo === 'sin_sesion_unity_vc') {
        set({ cmSesionActiva: false, sesionUnityVC: null, accesoCompletado: false });
      }
    }
  },
  reconectarUnityVc: async (org) => {
    if (!enTauri()) {
      get().showToast('Conexión con Unity VC verificada (mockup).', 'ok');
      return true;
    }
    try {
      if (get().sesionUnityVcCaducada) {
        // Abrir el SSO solo tras pulsar Reconectar. Un `whoami` local puede
        // seguir devolviendo el correo aunque la sesión remota haya vencido.
        const sesion = await api.iniciarSesionUnityVc(true);
        if (!sesion.activa || !sesion.email) {
          throw new ErrorBackend('sin_sesion_unity_vc', 'Completa el inicio de sesión de Unity VC y vuelve a intentar.');
        }
      }
      const res = await api.reconectarUnityVc(org);
      set({
        cmInstalado: true,
        cmSesionActiva: true,
        sesionUnityVC: res.email,
        currentEmail: res.email,
        sesionUnityVcCaducada: false,
      });
      get().showToast(res.mensaje, 'ok');
      return true;
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
      return false;
    }
  },
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
      documentos: [],
      mensajes: [],
      gruposChat: [],
      chatsEliminados: {},
      ramaWorkspace: null,
      proyectoActual: proyecto,
      workspaceActual: proyecto,
      proyectoLocal: `D:\\BranchBoard\\${proyecto}`,
      modoSinUnityVc: true,
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
  cargarWorkspacesYProyectos: async () => {
    if (!enTauri()) return;
    const [locales, servidor] = await Promise.allSettled([
      api.listarWorkspacesLocales(),
      api.listarProyectosServidor(),
    ]);
    set((s) => ({
      workspaces: locales.status === 'fulfilled' ? locales.value : s.workspaces,
      proyectosServidor: servidor.status === 'fulfilled' ? servidor.value : s.proyectosServidor,
    }));
  },
  irAClonar: (nombre, repo = null) =>
    set({
      accesoCompletado: false,
      accesoIrAWorkspaces: false,
      proyectoAClonar: nombre,
      proyectoRepoAClonar: repo,
    }),
  entrarAProyecto: (nombre, workspace, carpetaLocal) => {
    set({
      revisionEquipo: get().revisionEquipo + 1,
      usuariosVisibles: [], usuariosCargando: false, equipoOcupado: false, errorUsuarios: null,
      ...(enTauri() ? { usuarios: get().usuarios.filter((u) => u.email === get().currentEmail) } : {}),
      proyectoLocal: carpetaLocal ?? null,
      ...(carpetaLocal ? { modoSinUnityVc: true } : {}),
      proyectoActual: nombre,
      workspaceActual: workspace ?? nombre,
      accesoCompletado: true,
      accesoIrAWorkspaces: false,
      proyectoAClonar: null,
      proyectoRepoAClonar: null,
      ...(enTauri()
        ? {
            tareas: [],
            commits: [],
            cambiosTraidos: [],
            notificaciones: [],
            edicionesPendientes: [],
            ramaWorkspace: null,
          }
        : {}),
    });
    if (enTauri()) {
      void get()
        .cargarCommits()
        .then(() => Promise.all([get().cargarTareas(), get().refrescarRamaWorkspace()]));
    }
  },
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
    const email = s.currentEmail.toLowerCase();
    return s.usuarios.find((u) => u.email.toLowerCase() === email) ?? (enTauri() ? USUARIO_SIN_SESION : s.usuarios[0] ?? USUARIO_SIN_SESION);
  },

  puedeAvanzar: (id, estado) => {
    if (estado !== 'en revisión' && estado !== 'terminada') return true;
    // Los cambios traídos de otras tareas son prestados. A revisión pueden ir (puede que hagan falta
    // para revisarla), con un aviso; a terminada (merge) no: hay que devolverlos antes. Para todos
    // los roles. Cuenta los de la tarea y los de sus subtareas (un check-in trabaja sobre la rama
    // del padre).
    const tareas = get().tareas;
    // Tarea roja (#225): a revisión puede ir; al merge no hasta trasladar sus cambios.
    const directos = tareas.find((t) => t.id === id)?.cambios_directos;
    if (estado === 'terminada' && directos && !directos.trasladado_en) {
      get().showToast(
        `Los cambios de ${id} siguen en ${directos.rama}: trasládalos a una rama antes de autorizar el merge`,
        'danger'
      );
      return false;
    }
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
    // Lo mismo que validará el backend: el Colaborador externo solo toma tareas individuales.
    const tomada = get().tareas.find((x) => x.id === id);
    const quien = get().usuarios.find((u) => u.email === email);
    if (tomada && quien && !puedeTomarTarea(quien.rol, tomada, get().tareas)) return;
    if (enTauri()) {
      void api
        .tomarTarea(id, () => [])
        .then((actualizadas) => {
          const porId = new Map(actualizadas.map((x) => [x.id, x]));
          const final = actualizadas.find((x) => x.id === id);
          set((st) => ({
            tareas: normalizar(st.tareas.map((t) => porId.get(t.id) ?? t)),
            ramaWorkspace: final?.rama_asociada ?? st.ramaWorkspace,
          }));
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    set((s) => {
      const conPadres = conRamasDeAncestros(s.tareas, id);
      return {
        tareas: conPadres.map((t) => {
          if (t.id !== id) return t;
          return {
            ...t,
            asignado_a: email,
            modo_trabajo: 'rama',
            rama_asociada: t.rama_asociada ?? rutaRama(conPadres, t),
          };
        }),
      };
    });
  },

  // En una tarea general, sus subtareas también vuelven a "por hacer"; si no, al siguiente
  // cambio el padre se volvería a derivar a otro estado (y heredaría un responsable).
  asignarTarea: (id, email) => {
    const t = get().tareas.find((x) => x.id === id);
    if (!t || t.asignado_a === email) return;
    const destino = get().usuarios.find((u) => u.email === email);
    if (destino && !puedeTomarTarea(destino.rol, t, get().tareas)) return;
    set((s) => {
      const conPadres = conRamasDeAncestros(s.tareas, id);
      return {
        tareas: conPadres.map((x) =>
          x.id === id
            ? {
                ...x,
                asignado_a: email,
                modo_trabajo: 'rama',
                rama_asociada: x.rama_asociada ?? rutaRama(conPadres, x),
              }
            : x
        ),
      };
    });
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
      backlog_id: data.backlog_id,
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
    if (enTauri()) {
      void api
        .crearTarea(data, () => nueva)
        .then((creada) => {
          set((st) => ({ tareas: normalizar([creada, ...st.tareas.filter((t) => t.id !== creada.id)]) }));
          get().showToast(
            creada.pendiente_aprobacion ? 'Tarea creada — quedó pendiente de aprobación' : 'Tarea creada'
          );
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return nueva.id;
    }
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
    if (enTauri()) {
      void api
        .crearTareaConSubtareas(data, () => [])
        .then((creadas) => {
          const ids = new Set(creadas.map((t) => t.id));
          set((st) => ({ tareas: normalizar([...creadas, ...st.tareas.filter((t) => !ids.has(t.id))]) }));
          const pendiente = creadas[0]?.pendiente_aprobacion ?? false;
          get().showToast(pendiente ? 'Tarea creada — quedó pendiente de aprobación' : 'Tarea creada');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    const { subtareas, ...raiz } = data;
    const padreId = get().crearTarea(raiz);
    subtareas.forEach((st) => {
      get().crearTarea({
        titulo: st.titulo,
        detalles: st.detalles ?? '',
        tipo: st.tipo,
        tarea_padre_id: padreId,
        sprint_id: raiz.sprint_id,
        backlog_id: raiz.backlog_id,
        con_fecha: false,
        fecha_limite: null,
      });
    });
  },

  editarTarea: (id, campos, marcarPendiente) => {
    const actualizarLocal = (actualizada?: Tarea) =>
      set((s) => ({
        tareas: s.tareas.map((t) =>
          t.id === id
            ? actualizada
              ? { ...actualizada, edicion_pendiente: marcarPendiente || actualizada.edicion_pendiente }
              : { ...t, ...campos, edicion_pendiente: marcarPendiente || t.edicion_pendiente }
            : t
        ),
      }));
    if (enTauri()) {
      void api
        .editarTarea(
          id,
          {
            titulo: campos.titulo,
            detalles: campos.detalles,
            tipo: campos.tipo,
            sprint_id: campos.sprint_id,
            backlog_id: campos.backlog_id,
            con_fecha: campos.con_fecha,
            fecha_limite: campos.fecha_limite,
          },
          () => get().tareas.find((t) => t.id === id)!
        )
        .then((actualizada) => actualizarLocal(actualizada))
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    actualizarLocal();
  },

  // "editar_y_aprobar_tarea" (BACKEND.md #1): un solo comando — corrige campos y aprueba a la vez.
  // Notificación #11: avisa al creador que su tarea pendiente fue editada y aprobada.
  editarYAprobarTarea: (id, campos) => {
    const tarea = get().tareas.find((t) => t.id === id);
    if (enTauri()) {
      void api
        .editarYAprobarTarea(id, campos, () => ({ ...(tarea as Tarea), ...campos, pendiente_aprobacion: false }))
        .then((actualizada) => {
          set((s) => ({
            tareas: s.tareas.map((t) => (t.id === id ? actualizada : t)),
          }));
          get().showToast('Tarea editada y aprobada');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
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
    if (enTauri()) {
      void api
        .eliminarTarea(id, justificacion, () => [])
        .then((afectadas) => {
          const mapa = new Map(afectadas.map((t) => [t.id, t]));
          set((s) => ({
            tareas: s.tareas.map((t) => mapa.get(t.id) ?? t),
          }));
          get().showToast('Tarea enviada a la Papelera', 'danger');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
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
    if (enTauri()) {
      void api
        .eliminarDefinitivamente(id, () => [])
        .then((purgadas) => {
          const borradas = new Set(purgadas.map((t) => t.id));
          set((s) => ({ tareas: s.tareas.filter((t) => !borradas.has(t.id)) }));
          get().showToast('Tarea eliminada definitivamente', 'danger');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    set((s) => ({ tareas: s.tareas.filter((t) => t.id !== id) }));
    get().showToast('Tarea eliminada definitivamente', 'danger');
  },

  aprobarTarea: (id) => {
    const tarea = get().tareas.find((t) => t.id === id);
    if (enTauri()) {
      void api
        .aprobarTarea(id, () => ({ ...(tarea as Tarea), pendiente_aprobacion: false }))
        .then((actualizada) => {
          set((s) => ({
            tareas: s.tareas.map((t) => (t.id === id ? actualizada : t)),
          }));
          get().showToast('Tarea aprobada');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === id ? { ...t, pendiente_aprobacion: false } : t)),
    }));
    get().showToast('Tarea aprobada');
  },

  aprobarTareas: (ids) => {
    const set_ = new Set(ids);
    if (enTauri()) {
      void api
        .aprobarTareas(ids, () =>
          get().tareas.map((t) => (set_.has(t.id) ? { ...t, pendiente_aprobacion: false } : t)),
        )
        .then((lista) => {
          set({ tareas: normalizar(lista) });
          get().showToast(`${ids.length} tareas nuevas aprobadas`);
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    set((s) => ({
      tareas: s.tareas.map((t) => (set_.has(t.id) ? { ...t, pendiente_aprobacion: false } : t)),
    }));
    get().showToast(`${ids.length} tareas nuevas aprobadas`);
  },

  aprobarEdiciones: (tareaIds) => {
    const set_ = new Set(tareaIds);
    if (enTauri()) {
      void api
        .aprobarModificaciones(tareaIds, () =>
          get().tareas.map((t) => (set_.has(t.id) ? { ...t, edicion_pendiente: false } : t)),
        )
        .then((lista) => {
          set((s) => ({
            tareas: normalizar(lista),
            edicionesPendientes: s.edicionesPendientes.filter((e) => !set_.has(e.tarea_id)),
          }));
          get().showToast(`${tareaIds.length} ediciones aprobadas`);
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    set((s) => ({
      tareas: s.tareas.map((t) => (set_.has(t.id) ? { ...t, edicion_pendiente: false } : t)),
      edicionesPendientes: s.edicionesPendientes.filter((e) => !set_.has(e.tarea_id)),
    }));
    get().showToast(`${tareaIds.length} ediciones aprobadas`);
  },

  // Notificación #12 (BACKEND.md): advertencia sobre tarea pendiente — no toca la tarea, solo avisa.
  advertirTarea: (id, mensaje) => {
    const tarea = get().tareas.find((t) => t.id === id);
    if (enTauri()) {
      void api
        .advertirTareaPendiente(id, mensaje, () => undefined)
        .then(() => get().showToast('Advertencia enviada al creador', 'info'))
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
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
    const tarea = get().tareas.find((t) => t.id === tareaId);
    if (enTauri()) {
      void api
        .aprobarModificacion(tareaId, () => ({ ...(tarea as Tarea), edicion_pendiente: false }))
        .then((actualizada) => {
          set((s) => ({
            tareas: s.tareas.map((t) => (t.id === tareaId ? actualizada : t)),
            edicionesPendientes: s.edicionesPendientes.filter((e) => e.tarea_id !== tareaId),
          }));
          get().showToast('Edición aprobada');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    set((s) => ({
      tareas: s.tareas.map((t) => (t.id === tareaId ? { ...t, edicion_pendiente: false } : t)),
      edicionesPendientes: s.edicionesPendientes.filter((e) => e.tarea_id !== tareaId),
    }));
    get().showToast('Edición aprobada');
  },

  rechazarEdicion: (tareaId) => {
    const s = get();
    const ep = s.edicionesPendientes.find((e) => e.tarea_id === tareaId);
    if (enTauri()) {
      void api
        .rechazarModificacion(tareaId, () => {
          const rev: Partial<Tarea> = { edicion_pendiente: false };
          if (ep) {
            if (ep.campos_anteriores.titulo !== undefined) rev.titulo = ep.campos_anteriores.titulo;
            if (ep.campos_anteriores.detalles !== undefined) rev.detalles = ep.campos_anteriores.detalles;
          }
          return { ...(s.tareas.find((t) => t.id === tareaId) as Tarea), ...rev };
        })
        .then((restaurada) => {
          set((prev) => ({
            tareas: prev.tareas.map((t) => (t.id === tareaId ? restaurada : t)),
            edicionesPendientes: prev.edicionesPendientes.filter((e) => e.tarea_id !== tareaId),
          }));
          get().showToast('Edición rechazada — se revirtieron los valores', 'danger');
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
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
  conflictoMerge: null,
  autorizandoMerge: false,
  cerrarConflictoMerge: () => set({ conflictoMerge: null }),
  resolverConflictoEnUnityVc: async () => {
    const conflicto = get().conflictoMerge;
    if (!conflicto) return false;
    try {
      const { rama_actual } = await api.abrirResolucionConflicto(conflicto.destino);
      set({ conflictoMerge: null });
      const despues = get().tareas.find((t) => t.id === conflicto.tareaId)?.tarea_padre_id
        ? 'BranchBoard la cerrará solo al detectar el merge.'
        : 'Después vuelve a autorizar la tarea en BranchBoard.';
      get().showToast(
        enTauri()
          ? `Tu workspace quedó en ${rama_actual}. En Unity VC: clic derecho en ${conflicto.origen} → "Merge from this branch", resuelve los archivos y haz check-in. ${despues}`
          : 'En la app de escritorio esto abre Unity VC en la rama destino para resolver el conflicto.',
        'info',
      );
      return true;
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
      return false;
    }
  },
  trasladandoCambiosDirectos: false,
  trasladarCambiosDirectos: async (id) => {
    if (get().trasladandoCambiosDirectos) return false;
    const s = get();
    const tarea = s.tareas.find((t) => t.id === id);
    const directos = tarea?.cambios_directos;
    if (!tarea || !directos || directos.trasladado_en) return false;
    if (!esLiderOAdmin(s.currentUser().rol)) {
      s.showToast('Solo un Líder o el Administrador puede trasladar estos cambios', 'danger');
      return false;
    }
    set({ trasladandoCambiosDirectos: true });
    try {
      const rama = `/main/dev/${tarea.id}-cambios-directos-en-${directos.rama === '/main' ? 'main' : 'dev'}`;
      const actualizada = await api.trasladarCambiosDirectos(id, () => ({
        ...tarea,
        rama_asociada: rama,
        modo_trabajo: 'rama',
        cambios_directos: {
          ...directos,
          rama_traslado: rama,
          changesets_reversion: directos.changesets.map((_, i) => 900 + i).reverse(),
          trasladado_en: new Date().toISOString(),
          trasladado_por: s.currentEmail,
        },
      }));
      set((st) => ({ tareas: st.tareas.map((t) => (t.id === id ? actualizada : t)) }));
      get().showToast(`Cambios trasladados a ${actualizada.rama_asociada ?? 'su rama'}`, 'ok');
      get().subirVersionDatos();
      return true;
    } catch (e) {
      get().showToast(mensajeDeError(e), 'danger');
      return false;
    } finally {
      set({ trasladandoCambiosDirectos: false });
    }
  },
  alertaRamaAjena: null,
  mostrarAlertaRamaAjena: (alerta) => {
    // El polling la repite cada 15 s mientras dure: solo se muestra si es otra y no se cerró ya.
    const clave = claveAlertaRamaAjena(alerta);
    const actual = get().alertaRamaAjena;
    if (clave === alertaRamaAjenaCerrada || (actual && claveAlertaRamaAjena(actual) === clave)) return;
    set({ alertaRamaAjena: alerta });
  },
  cerrarAlertaRamaAjena: () => {
    const actual = get().alertaRamaAjena;
    if (actual) alertaRamaAjenaCerrada = claveAlertaRamaAjena(actual);
    set({ alertaRamaAjena: null });
  },
  autorizarMerge: async (id) => {
    if (enTauri()) {
      if (get().autorizandoMerge) return;
      set({ autorizandoMerge: true, conflictoMerge: null });
      try {
        const resultado = await api.autorizarMerge(id, () => ({ tareas: get().tareas, conflicto: null }));
        set({ tareas: resultado.tareas });
        if (resultado.conflicto) {
          set({ conflictoMerge: { ...resultado.conflicto, tareaId: id } });
        } else {
          get().showToast('Merge autorizado — tarea terminada');
          get().subirVersionDatos();
        }
      } catch (e) {
        get().showToast(mensajeDeError(e), 'danger');
      } finally {
        set({ autorizandoMerge: false });
      }
      return;
    }
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

  rechazarRevision: async (id, motivo) => {
    if (enTauri()) {
      try {
        const tareas = await api.rechazarRevision(id, motivo, () => get().tareas);
        set({ tareas, conflictoMerge: null });
        get().subirVersionDatos();
        get().showToast('Revisión rechazada — vuelve a producción', 'info');
        return true;
      } catch (e) {
        get().showToast(mensajeDeError(e), 'danger');
        return false;
      }
    }
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
    return true;
  },

  // Notificación #6 (BACKEND.md): revisión asignada a un Líder específico.
  asignarRevision: async (id, liderEmail) => {
    if (enTauri()) {
      try {
        const tarea = await api.asignarRevision(id, liderEmail, () => get().tareas.find((t) => t.id === id)!);
        set((s) => ({ tareas: s.tareas.map((t) => t.id === id ? tarea : t) }));
        get().subirVersionDatos();
        get().showToast('Revisión asignada');
        return true;
      } catch (e) {
        get().showToast(mensajeDeError(e), 'danger');
        return false;
      }
    }
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
    return true;
  },

  autoasignarRevision: (id, email) => {
    if (enTauri()) {
      void get().asignarRevision(id, email);
      return;
    }
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

  crearUsuario: async (correo, rol) => {
    const s = get();
    const c = correo.trim().toLowerCase();
    if (enTauri()) return cambiarEquipo(() => api.crearUsuario(c, rol), `${c} agregado al equipo o invitado`);
    if (s.usuarios.some((u) => u.email.toLowerCase() === c)) {
      s.showToast('Ya existe un usuario con ese correo', 'danger');
      return false;
    }
    // El nombre sale del correo (ana.lopez@… → Ana Lopez) hasta que la persona lo cambie.
    const [nombre = '', apellido = ''] = c
      .split('@')[0]
      .split(/[._-]+/)
      .map((p) => p.charAt(0).toUpperCase() + p.slice(1));
    // Con el servidor (#241): si la persona ya tiene cuenta entra de una vez; si no, queda invitada y
    // entra al registrarse con ese correo. El mockup no sabe quién tiene cuenta: la muestra invitada.
    const nuevo: Usuario = {
      email: c,
      nombre,
      apellido,
      rol,
      fecha_alta: today(),
      invitado: true,
    };
    set({ usuarios: [...s.usuarios, nuevo] });
    s.showToast(`${c} invitado como ${rol}: entra al registrarse con ese correo`);
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

  eliminarUsuario: async (email) => {
    if (enTauri()) return cambiarEquipo(() => api.eliminarUsuario(email), 'Usuario o invitación quitados del proyecto');
    const s = get();
    const quien = s.usuarios.find((u) => u.email === email);
    if (quien && quedariaSinAdministrador(s.usuarios, email, null)) {
      s.showToast('El proyecto tiene que tener al menos un Administrador', 'danger');
      return false;
    }
    set({ usuarios: s.usuarios.filter((u) => u.email !== email) });
    s.showToast(quien?.invitado ? 'Invitación quitada' : 'Usuario eliminado', 'danger');
    return true;
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
      workspaceActual: s.workspaceActual === anterior ? nuevo : s.workspaceActual,
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

  cambiarRol: async (email, rol) => {
    if (enTauri()) return cambiarEquipo(() => api.cambiarRol(email, rol), `Rol cambiado a ${rol}`);
    const s = get();
    // El servidor nunca deja degradar al último Administrador (#241).
    if (quedariaSinAdministrador(s.usuarios, email, rol)) {
      s.showToast('El proyecto tiene que tener al menos un Administrador', 'danger');
      return false;
    }
    set({ usuarios: s.usuarios.map((u) => (u.email === email ? { ...u, rol } : u)) });
    s.showToast(`Rol cambiado a ${rol}`);
    return true;
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

  // Un sprint no toma tareas de otro sprint, ni un backlog tareas de otro backlog: primero hay que quitarla.
  asignarTareaASprint: (tareaId, sprintId) => {
    const t = get().tareas.find((x) => x.id === tareaId);
    if (!t || (sprintId && t.sprint_id && t.sprint_id !== sprintId)) return;
    set((s) => ({ tareas: s.tareas.map((x) => (x.id === tareaId ? { ...x, sprint_id: sprintId } : x)) }));
  },
  asignarTareaABacklog: (tareaId, backlogId) => {
    const t = get().tareas.find((x) => x.id === tareaId);
    if (!t || (backlogId && t.backlog_id && t.backlog_id !== backlogId)) return;
    set((s) => ({ tareas: s.tareas.map((x) => (x.id === tareaId ? { ...x, backlog_id: backlogId } : x)) }));
  },
  asignarBacklogASprint: (backlogId, sprintId) => {
    const libres = get().tareas.filter((t) => t.backlog_id === backlogId && !t.sprint_id && !t.eliminada);
    if (libres.length === 0) return;
    const ids = new Set(libres.map((t) => t.id));
    set((s) => ({ tareas: s.tareas.map((t) => (ids.has(t.id) ? { ...t, sprint_id: sprintId } : t)) }));
    const sp = get().sprints.find((x) => x.id === sprintId);
    get().showToast(`${libres.length} ${libres.length === 1 ? 'tarea pasó' : 'tareas pasaron'} a ${sp?.nombre ?? 'el sprint'}`);
  },

  cerrarSprint: (id) => {
    // Solo el Administrador cierra sprints y backlogs.
    if (get().currentUser().rol !== 'Administrador') {
      get().showToast('Solo el Administrador puede cerrar un sprint', 'danger');
      return;
    }
    if (enTauri()) {
      void api
        .cerrarSprint(id, () => get().sprints.find((s) => s.id === id)!)
        .then(async (cerrado) => {
          await get().cargarSprints();
          await get().cargarTareas();
          get().showToast(`${cerrado.con_fecha ? 'Sprint' : 'Backlog'} cerrado — se generó su reporte`);
        })
        .catch((e) => get().showToast(mensajeDeError(e), 'danger'));
      return;
    }
    // Al cerrar un sprint se guarda la copia de sus tareas para el reporte, y las que no se terminaron
    // quedan libres otra vez (siguen en su backlog, si tienen).
    set((s) => ({
      sprints: s.sprints.map((sp) =>
        sp.id === id
          ? {
              ...sp,
              cerrado: true,
              fecha_cierre_real: today(),
              reporte: sp.con_fecha ? s.tareas.filter((t) => t.sprint_id === id && !t.eliminada).map((t) => ({ ...t })) : undefined,
            }
          : sp
      ),
      tareas: s.tareas.map((t) => (t.sprint_id === id && t.estado !== 'terminada' ? { ...t, sprint_id: null } : t)),
    }));
    get().showToast('Sprint cerrado — se generó su reporte');
  },
  };
});
