// Cliente único del backend. Toda llamada a un comando de Tauri pasa por aquí.
//
// - Dentro de la app de escritorio (Tauri): llama al comando real con `invoke`.
// - En el navegador (`npm run dev`, mockup): no hay backend, así que cada función
//   responde con su versión de mockup (datos en memoria).
//
// Para agregar un comando: una función tipada en `api` que use `llamar`, con el
// nombre del comando de Rust, sus argumentos y qué responde en el mockup.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import type {
  AvanceBacklog,
  AvanceSprint,
  CambioTraido,
  Commit,
  Documento,
  EdicionPendiente,
  EstadoTarea,
  GrupoChat,
  Mensaje,
  Notificacion,
  Rol,
  Sprint,
  Tarea,
  TipoTarea,
} from '../types';

/** `true` cuando la app corre dentro de la ventana de Tauri (no en el navegador). */
export const enTauri = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Códigos de error del backend (`app/backend/src/error.rs`). */
export type CodigoError =
  | 'sin_permiso'
  | 'no_encontrado'
  | 'validacion'
  | 'cm_no_instalado'
  | 'sin_sesion_unity_vc'
  | 'cm'
  | 'servidor_unity_vc'
  | 'token_servidor_invalido'
  | 'base_datos'
  | 'archivos'
  | 'llave_cifrada'
  | 'actualizacion'
  | 'interno';

/** Error que devuelve un comando del backend: `{ codigo, mensaje }`. */
export class ErrorBackend extends Error {
  constructor(
    public readonly codigo: CodigoError,
    public readonly mensaje: string
  ) {
    super(mensaje);
    this.name = 'ErrorBackend';
  }
}

function aErrorBackend(e: unknown): ErrorBackend {
  if (e instanceof ErrorBackend) return e;
  if (e && typeof e === 'object' && 'codigo' in e && 'mensaje' in e) {
    const { codigo, mensaje } = e as { codigo: CodigoError; mensaje: string };
    return new ErrorBackend(codigo, mensaje);
  }
  return new ErrorBackend('interno', typeof e === 'string' ? e : String(e));
}

/** Texto para mostrarle a la persona a partir de cualquier error. */
export const mensajeDeError = (e: unknown): string => aErrorBackend(e).mensaje;

/**
 * Llama a un comando del backend. Fuera de Tauri usa `mockup`.
 * Los errores siempre salen como `ErrorBackend`.
 */
export async function llamar<T>(
  comando: string,
  args: Record<string, unknown> | undefined,
  mockup: () => T | Promise<T>
): Promise<T> {
  try {
    return enTauri() ? await invoke<T>(comando, args) : await mockup();
  } catch (e) {
    throw aErrorBackend(e);
  }
}

// ───────── Tipos de los comandos ─────────

export interface InfoActualizacion {
  version_actual: string;
  /** `null` si la app ya está al día. */
  version_nueva: string | null;
  notas: string | null;
}

export interface ProgresoActualizacion {
  descargado: number;
  total: number | null;
}

/** Versión que muestra el mockup en el navegador. */
const VERSION_MOCKUP = '0.1.0';

/** Resultado de un escaneo de `ops/` (`sincronizar_eventos`). */
export interface ResumenSincronizacion {
  aplicados: number;
  descartados: number;
  /** Escritos por una versión más nueva de la app: quedan pendientes hasta actualizar. */
  desconocidos: number;
  /** Archivos que todavía no se pueden leer (ej. a medio sincronizar): se reintentan. */
  ilegibles: number;
}

/** Resultado de compactar `ops/` (`compactar_eventos`, #94). */
export interface ResumenCompactacion {
  compactados: number;
  /** `snapshot_<fecha>.json` creado en `ops/`, o `null` si no había eventos de más de 30 días. */
  archivo: string | null;
}

/** Alerta en pantalla cuando hay cambios en `main`, `dev` o una rama ajena (#111). */
export interface AlertaRamaAjena {
  rama_actual: string;
  tarea_ajena_id: string | null;
  dueno_rama: string | null;
  rama_sugerida: string | null;
  tarea_sugerida_id: string | null;
  archivos: string[];
  mensaje: string;
}

/** Datos para `crear_tarea` (#95). */
export interface DatosNuevaTarea {
  titulo: string;
  detalles?: string;
  tipo: TipoTarea;
  tarea_padre_id: string | null;
  sprint_id: string | null;
  backlog_id: string | null;
  con_fecha: boolean;
  fecha_limite: string | null;
}

/** Datos para `crear_tarea_con_subtareas` (#95). */
export interface DatosNuevaTareaConSubtareas extends DatosNuevaTarea {
  subtareas: { tipo: TipoTarea; titulo: string; detalles?: string }[];
}

/** Campos editables con `editar_tarea` (#95). */
export interface CambiosEditarTarea {
  titulo?: string;
  detalles?: string;
  tipo?: TipoTarea;
  sprint_id?: string | null;
  backlog_id?: string | null;
  con_fecha?: boolean;
  fecha_limite?: string | null;
}

/**
 * Qué ve la persona al abrir un workspace (`entrar_a_proyecto`, #89):
 * - `dentro`: entra con su rol; `configurado_ahora` = era el dueño del repo y el proyecto se configuró.
 * - `pendiente`: registrada, pero el Administrador todavía no confirma su rol.
 * - `no_registrado`: su correo no está en el proyecto.
 * - `esperar_dueno`: el proyecto no usa BranchBoard todavía y solo su dueño puede configurarlo.
 */
export type EntradaProyecto =
  | {
      estado: 'dentro';
      email: string;
      rol: Rol;
      rol_verificado: boolean;
      proyecto: string;
      configurado_ahora: boolean;
      vistos?: Record<string, string>;
    }
  | { estado: 'pendiente' | 'no_registrado' | 'esperar_dueno'; email: string; proyecto: string };

// ───────── Comandos ─────────

export const api = {
  /** Comando de prueba: `"pong"` si el backend responde. */
  ping: () => llamar<string>('ping', undefined, () => 'pong (mockup)'),

  /** `true` mientras haya una operación larga (`cm`) corriendo. */
  hayOperacionEnCurso: () => llamar<boolean>('hay_operacion_en_curso', undefined, () => false),

  /**
   * Avisa cada vez que se intentó cerrar la ventana con una operación en curso y el backend
   * canceló el cierre. Devuelve la función para dejar de escuchar. En el navegador nunca ocurre.
   */
  alCierreBloqueado: async (avisar: () => void): Promise<() => void> => {
    if (!enTauri()) return () => {};
    return listen('cierre-bloqueado', () => avisar());
  },

  /**
   * Entra al proyecto del workspace de `carpeta`: el backend lee `cm whoami`, el repo del workspace y,
   * si el proyecto no tiene datos, si quien entra es su dueño. En el mockup responde `mockup()`, que
   * simula lo mismo con los datos de ejemplo.
   */
  entrarAProyecto: (carpeta: string, mockup: () => EntradaProyecto) =>
    llamar<EntradaProyecto>('entrar_a_proyecto', { carpeta }, mockup),

  /**
   * Abre el explorador de carpetas del sistema (lo abre el backend) y responde la carpeta elegida, o
   * `null` si se cancela. En el navegador no hay acceso al disco: responde `null`.
   */
  elegirCarpeta: (titulo?: string) => llamar<string | null>('elegir_carpeta', { titulo }, () => null),

  /**
   * Carpeta compartida del proyecto abierto (Drive/OneDrive) en esta máquina, donde viven los eventos de
   * `ops/`; `null` si todavía no se configuró. En el mockup responde `mockup()`.
   */
  carpetaCompartida: (mockup: () => string | null) =>
    llamar<string | null>('carpeta_compartida', undefined, mockup),

  /**
   * Guarda la carpeta compartida del proyecto abierto en esta máquina y responde la ruta completa guardada.
   * El backend exige que sea la de este proyecto (archivo `branchboard-proyecto.json`); si la carpeta no
   * tiene esa marca, solo el Administrador la puede dejar como la del proyecto.
   */
  configurarCarpetaCompartida: (ruta: string, mockup: () => string) =>
    llamar<string>('configurar_carpeta_compartida', { ruta }, mockup),

  /**
   * Escanea `ops/` y aplica los cambios de las demás máquinas (#92). En el mockup no hay otras máquinas:
   * responde que no llegó nada.
   */
  sincronizarEventos: () =>
    llamar<ResumenSincronizacion>('sincronizar_eventos', undefined, () => ({
      aplicados: 0,
      descartados: 0,
      desconocidos: 0,
      ilegibles: 0,
    })),

  /**
   * Compacta en un `snapshot_<fecha>.json` firmado los eventos de `ops/` con más de 30 días y los
   * borra (#94). Solo Administrador; nunca automático.
   */
  compactarEventos: (passphrase?: string) =>
    llamar<ResumenCompactacion>('compactar_eventos', { passphrase }, () => ({
      compactados: 0,
      archivo: null,
    })),

  /** Todas las tareas del proyecto abierto (#95). */
  listarTareas: (mockup: () => Tarea[]) => llamar<Tarea[]>('listar_tareas', undefined, mockup),

  /** Crea una tarea (#95). */
  crearTarea: (datos: DatosNuevaTarea, mockup: () => Tarea) =>
    llamar<Tarea>('crear_tarea', { datos }, mockup),

  /** Crea una tarea y sus subtareas iniciales (#95). */
  crearTareaConSubtareas: (datos: DatosNuevaTareaConSubtareas, mockup: () => Tarea[]) =>
    llamar<Tarea[]>('crear_tarea_con_subtareas', { datos }, mockup),

  /** Edita los campos de una tarea (creador sobre su pendiente, o Líder/Administrador, #95). */
  editarTarea: (id: string, campos: CambiosEditarTarea, mockup: () => Tarea) =>
    llamar<Tarea>('editar_tarea', { id, campos }, mockup),

  /**
   * Toma la tarea `id` (#96): crea en Unity VC las ramas que falten en la cadena (desde la tarea
   * general hasta `id`), cambia el workspace local a ella con `cm switch` y publica el evento en
   * `ops/`. Devuelve las tareas de la cadena actualizadas.
   */
  tomarTarea: (id: string, mockup: () => Tarea[]) => llamar<Tarea[]>('tomar_tarea', { id }, mockup),

  /** Asigna la tarea `id` a `email` (Líder/Admin, #98) sin cambiar el workspace local. */
  asignarTarea: (id: string, email: string, mockup: () => Tarea[]) =>
    llamar<Tarea[]>('asignar_tarea', { id, email }, mockup),

  /** Suelta la tarea `id` (#98) si ni ella ni sus subtareas tienen check-ins. */
  desasignarTarea: (id: string, mockup: () => Tarea) =>
    llamar<Tarea>('desasignar_tarea', { id }, mockup),

  /** Cambia el estado de `id` con recálculo de padres y aviso de préstamos (#99). */
  cambiarEstado: (
    id: string,
    nuevoEstado: EstadoTarea,
    mockup: () => { tareas: Tarea[]; aviso_prestamos: string[] },
  ) =>
    llamar<{ tareas: Tarea[]; aviso_prestamos: string[] }>(
      'cambiar_estado',
      { id, nuevoEstado },
      mockup,
    ),

  /** Borra commits, préstamos y cambios sin check-in y revierte la rama (#100). */
  borrarProgreso: (id: string, mockup: () => Tarea[]) =>
    llamar<Tarea[]>('borrar_progreso', { id }, mockup),

  /** Reabre una tarea terminada a 'en revisión' con motivo obligatorio (#100). */
  reabrirTarea: (id: string, motivo: string, mockup: () => Tarea) =>
    llamar<Tarea>('reabrir_tarea', { id, motivo }, mockup),

  /** Aprueba una tarea pendiente (Líder/Admin, #101). */
  aprobarTarea: (id: string, mockup: () => Tarea) =>
    llamar<Tarea>('aprobar_tarea', { id }, mockup),

  /** Aprueba varias tareas pendientes en una operación (Líder/Admin, #101). */
  aprobarTareas: (ids: string[], mockup: () => Tarea[]) =>
    llamar<Tarea[]>('aprobar_tareas', { ids }, mockup),

  /** Edita y aprueba una tarea pendiente en un solo comando (Líder/Admin, #101). */
  editarYAprobarTarea: (id: string, campos: CambiosEditarTarea, mockup: () => Tarea) =>
    llamar<Tarea>('editar_y_aprobar_tarea', { id, campos }, mockup),

  /** Envía una advertencia al creador de una tarea pendiente sin aprobarla (#101). */
  advertirTareaPendiente: (id: string, mensaje: string, mockup: () => void) =>
    llamar<void>('advertir_tarea_pendiente', { id, mensaje }, mockup),

  /** Lista las ediciones pendientes de aprobación (#102). */
  listarEdicionesPendientes: (mockup: () => EdicionPendiente[]) =>
    llamar<EdicionPendiente[]>('listar_ediciones_pendientes', undefined, mockup),

  /** Propone una edición de título/detalles sobre una tarea en 'por hacer' (#102). */
  proponerEdicion: (
    id: string,
    titulo: string,
    detalles: string,
    motivo?: string | null,
    mockup?: () => { tarea: Tarea; edicion: EdicionPendiente },
  ) =>
    llamar<{ tarea: Tarea; edicion: EdicionPendiente }>(
      'proponer_edicion',
      { id, titulo, detalles, motivo: motivo ?? null },
      mockup ?? (() => ({}) as { tarea: Tarea; edicion: EdicionPendiente }),
    ),

  /** Aprueba una edición pendiente manteniendo los nuevos valores (#102). */
  aprobarModificacion: (id: string, mockup: () => Tarea) =>
    llamar<Tarea>('aprobar_modificacion', { id }, mockup),

  /** Aprueba varias ediciones pendientes en una operación (#102). */
  aprobarModificaciones: (ids: string[], mockup: () => Tarea[]) =>
    llamar<Tarea[]>('aprobar_modificaciones', { ids }, mockup),

  /** Rechaza una edición pendiente y restaura los valores anteriores (#102). */
  rechazarModificacion: (id: string, mockup: () => Tarea) =>
    llamar<Tarea>('rechazar_modificacion', { id }, mockup),

  /** Lista la Papelera ordenada de la eliminación más reciente a la más antigua (#103). */
  listarPapelera: (mockup: () => Tarea[]) =>
    llamar<Tarea[]>('listar_papelera', undefined, mockup),

  /** Manda una tarea (y sus subtareas activas en cascada) a la Papelera (Líder/Admin, #103). */
  eliminarTarea: (id: string, justificacion: string, mockup: () => Tarea[]) =>
    llamar<Tarea[]>('eliminar_tarea', { id, justificacion }, mockup),

  /** Purga definitivamente una tarea de la Papelera y sus subtareas en Papelera (Líder/Admin, #103). */
  eliminarDefinitivamente: (id: string, mockup: () => Tarea[]) =>
    llamar<Tarea[]>('eliminar_definitivamente', { id }, mockup),

  /** Cambia el workspace local de vuelta a `/main/dev` (notificación de doble toma, #97). */
  volverADev: () => llamar<string>('volver_a_dev', undefined, () => '/main/dev'),

  /** Revisa ramas activas y rama del workspace en Unity VC (#104, #106, #109, #111). */
  revisarActualizaciones: (tareaId?: string) =>
    llamar<{
      rama_actual: string | null;
      tareas_modificadas: string[];
      tareas_cerradas: string[];
      espera_creada?: string | null;
      espera_quitada?: string | null;
      tocados_detectados?: string[];
      alerta_rama_ajena?: AlertaRamaAjena | null;
      sprints_actualizados?: string[];
      inactividad_notificada?: string[];
    }>(
      'revisar_actualizaciones',
      { tareaId: tareaId ?? null },
      () => ({
        rama_actual: null,
        tareas_modificadas: [],
        tareas_cerradas: [],
        espera_creada: null,
        espera_quitada: null,
        tocados_detectados: [],
        alerta_rama_ajena: null,
        sprints_actualizados: [],
        inactividad_notificada: [],
      })
    ),

  /** Escucha alertas en tiempo real cuando hay cambios en `dev` o una rama ajena (#111). */
  alAlertaRamaAjena: async (alAlertar: (alerta: AlertaRamaAjena) => void): Promise<() => void> => {
    if (!enTauri()) return () => undefined;
    return listen<AlertaRamaAjena>('alerta-rama-ajena', (e) => alAlertar(e.payload));
  },

  /** Lista cambios sin check-in del workspace y valida la rama esperada (#105). */
  cambiosPendientes: (rama?: string | null) =>
    llamar<{ rama_actual: string; en_rama_esperada: boolean; archivos: string[] }>(
      'cambios_pendientes',
      { rama: rama ?? null },
      () => ({ rama_actual: '/main/dev', en_rama_esperada: true, archivos: [] })
    ),

  /** Cambia el workspace local hacia la rama de `id` (#105). */
  cambiarARamaTarea: (id: string) =>
    llamar<string>('cambiar_a_rama_tarea', { id }, () => '/main/dev'),

  /** Hace check-in en Unity VC desde BranchBoard con título, descripción y tipo (#105). */
  hacerCheckin: (
    rama: string,
    titulo: string,
    descripcion: string,
    tipo: TipoTarea,
    mockup?: () => Commit,
  ) =>
    llamar<Commit>(
      'hacer_checkin',
      { rama, titulo, descripcion, tipo },
      mockup ?? (() => ({}) as Commit),
    ),

  /** Convierte la subtarea "En espera" en subtarea normal llevándose los cambios a su rama nueva (#106). */
  crearRamaDesdeEspera: (
    esperaId: string,
    titulo: string,
    detalles: string,
    tipo: TipoTarea,
    mockup?: () => Tarea,
  ) =>
    llamar<Tarea>(
      'crear_rama_desde_espera',
      { esperaId, titulo, detalles, tipo },
      mockup ?? (() => ({}) as Tarea),
    ),

  /** Lista los préstamos de una tarea o del proyecto completo (#108). */
  listarCambiosPrestados: (tareaId?: string | null, mockup?: () => CambioTraido[]) =>
    llamar<CambioTraido[]>(
      'listar_cambios_prestados',
      { tareaId: tareaId ?? null },
      mockup ?? (() => []),
    ),

  /** Trae cambios prestados de otra tarea (todos o un commit puntual) mediante cherry-pick (#108). */
  traerCambios: (
    destinoId: string,
    origenId: string,
    hash?: string | null,
    mockup?: () => CambioTraido,
  ) =>
    llamar<CambioTraido>(
      'traer_cambios',
      { destinoId, origenId, hash: hash ?? null },
      mockup ?? (() => ({}) as CambioTraido),
    ),

  /** Devuelve un préstamo mediante merge sustractivo, conservando el historial (#108). */
  devolverCambiosPrestados: (prestamoId: string, mockup?: () => CambioTraido) =>
    llamar<CambioTraido>(
      'devolver_cambios_prestados',
      { prestamoId },
      mockup ?? (() => ({}) as CambioTraido),
    ),

  /** Restaura los archivos prestados que se modificaron y hace check-in automático (#109). */
  eliminarCambiosAjenos: (prestamoId: string, mockup?: () => CambioTraido) =>
    llamar<CambioTraido>(
      'eliminar_cambios_ajenos',
      { prestamoId },
      mockup ?? (() => ({}) as CambioTraido),
    ),

  /** Lista todos los sprints y backlogs del proyecto (#113). */
  listarSprints: (mockup?: () => Sprint[]) =>
    llamar<Sprint[]>('listar_sprints', undefined, mockup ?? (() => [])),

  /** Crea un sprint (`conFecha = true`) o backlog (`conFecha = false`), solo Líder/Admin (#113). */
  crearSprint: (
    nombre: string,
    descripcion: string,
    conFecha: boolean,
    duracionDias?: number | null,
    mockup?: () => Sprint,
  ) =>
    llamar<Sprint>(
      'crear_sprint',
      { nombre, descripcion, conFecha, duracionDias: duracionDias ?? null },
      mockup ?? (() => ({}) as Sprint),
    ),

  /** Asigna o quita una tarea de un sprint (#113). */
  asignarTareaASprint: (
    tareaId: string,
    sprintId?: string | null,
    mockup?: () => Tarea,
  ) =>
    llamar<Tarea>(
      'asignar_tarea_a_sprint',
      { tareaId, sprintId: sprintId ?? null },
      mockup ?? (() => ({}) as Tarea),
    ),

  /** Asigna o quita una tarea de un backlog (#113). */
  asignarTareaABacklog: (
    tareaId: string,
    backlogId?: string | null,
    mockup?: () => Tarea,
  ) =>
    llamar<Tarea>(
      'asignar_tarea_a_backlog',
      { tareaId, backlogId: backlogId ?? null },
      mockup ?? (() => ({}) as Tarea),
    ),

  /** Mueve al sprint las tareas libres del backlog (`backlogId = null` usa las sin backlog, #113). */
  asignarBacklogASprint: (
    backlogId: string | null | undefined,
    sprintId: string,
    mockup?: () => Tarea[],
  ) =>
    llamar<Tarea[]>(
      'asignar_backlog_a_sprint',
      { backlogId: backlogId ?? null, sprintId },
      mockup ?? (() => []),
    ),

  /** Cierra manualmente un sprint o backlog y genera su reporte final, solo Administrador (#114). */
  cerrarSprint: (sprintId: string, mockup?: () => Sprint) =>
    llamar<Sprint>('cerrar_sprint', { sprintId }, mockup ?? (() => ({}) as Sprint)),

  /** Datos calculados para la gráfica de Avance de un sprint (#115). */
  datosAvanceSprint: (sprintId: string, mockup?: () => AvanceSprint) =>
    llamar<AvanceSprint>('datos_avance_sprint', { sprintId }, mockup ?? (() => ({}) as AvanceSprint)),

  /** Datos calculados para la gráfica de Avance de un backlog (#115). */
  datosAvanceBacklog: (backlogId: string, mockup?: () => AvanceBacklog) =>
    llamar<AvanceBacklog>(
      'datos_avance_backlog',
      { backlogId },
      mockup ?? (() => ({}) as AvanceBacklog),
    ),

  /** Datos calculados para la vista General de Avance (#115). */
  datosAvanceGeneral: (mockup?: () => AvanceBacklog) =>
    llamar<AvanceBacklog>('datos_avance_general', undefined, mockup ?? (() => ({}) as AvanceBacklog)),

  /** Notificaciones del usuario actual (más reciente primero, #116). */
  obtenerNotificaciones: (mockup?: () => Notificacion[]) =>
    llamar<Notificacion[]>('obtener_notificaciones', undefined, mockup ?? (() => [])),

  /** Marca una notificación como leída en esta máquina (#116). */
  marcarLeida: (notificacionId: string, mockup?: () => void) =>
    llamar<void>('marcar_leida', { notificacionId }, mockup ?? (() => undefined)),

  /** Vuelve a marcar una notificación como no leída en esta máquina (#116). */
  desmarcarLeida: (notificacionId: string, mockup?: () => void) =>
    llamar<void>('desmarcar_leida', { notificacionId }, mockup ?? (() => undefined)),

  /** Marca todas las notificaciones pendientes del usuario actual como leídas (#116). */
  marcarTodasLeidas: (mockup?: () => number) =>
    llamar<number>('marcar_todas_leidas', undefined, mockup ?? (() => 0)),

  /** Envía un mensaje manual (máx. 500 caracteres) a un compañero como notificación (#118). */
  enviarMensaje: (para: string, texto: string, mockup?: () => Notificacion) =>
    llamar<Notificacion>(
      'enviar_mensaje',
      { para, texto },
      mockup ?? (() => ({}) as Notificacion),
    ),

  /** Reporta un problema de base de datos a los Administradores (Líder y Desarrollador, #118). */
  solicitarAccionBd: (motivo: string, mockup?: () => Notificacion[]) =>
    llamar<Notificacion[]>(
      'solicitar_accion_bd',
      { motivo },
      mockup ?? (() => []),
    ),

  /** Estado completo de chat del usuario actual (#119). */
  obtenerChat: (
    mockup?: () => {
      grupos: GrupoChat[];
      mensajes: Mensaje[];
      conversaciones_sin_leer: string[];
      eliminados: Record<string, string>;
    },
  ) =>
    llamar<{
      grupos: GrupoChat[];
      mensajes: Mensaje[];
      conversaciones_sin_leer: string[];
      eliminados: Record<string, string>;
    }>(
      'obtener_chat',
      undefined,
      mockup ?? (() => ({ grupos: [], mensajes: [], conversaciones_sin_leer: [], eliminados: {} })),
    ),

  /** Conversaciones con mensajes sin leer para el badge del menú (#119). */
  conversacionesSinLeer: (mockup?: () => string[]) =>
    llamar<string[]>('conversaciones_sin_leer', undefined, mockup ?? (() => [])),

  /** Envía un mensaje a `grupo`, grupo creado o privado (#119). */
  enviarChat: (para: string, texto: string, mockup?: () => Mensaje) =>
    llamar<Mensaje>('enviar_chat', { para, texto }, mockup ?? (() => ({}) as Mensaje)),

  /** Crea un grupo de chat incluyendo al creador (#119). */
  crearGrupoChat: (nombre: string, miembros: string[], mockup?: () => GrupoChat) =>
    llamar<GrupoChat>(
      'crear_grupo_chat',
      { nombre, miembros },
      mockup ?? (() => ({}) as GrupoChat),
    ),

  /** Marca como leídos los mensajes de una conversación en esta máquina (#119). */
  marcarChatLeido: (conversacion: string, mockup?: () => number) =>
    llamar<number>('marcar_chat_leido', { conversacion }, mockup ?? (() => 0)),

  /** Oculta localmente una conversación dejando visibles futuros mensajes (#119). */
  eliminarChat: (conversacion: string, mockup?: () => void) =>
    llamar<void>('eliminar_chat', { conversacion }, mockup ?? (() => undefined)),

  /** Documentos del proyecto en la carpeta compartida (#120). */
  listarDocumentos: (mockup?: () => Documento[]) =>
    llamar<Documento[]>('listar_documentos', undefined, mockup ?? (() => [])),

  /** Sube archivos locales al proyecto (Líder/Admin publicado; resto pendiente, #120). */
  subirDocumentos: (archivos: string[], mockup?: () => Documento[]) =>
    llamar<Documento[]>('subir_documentos', { archivos }, mockup ?? (() => [])),

  /** Aprueba un documento pendiente (solo Líder o Administrador, #120). */
  aprobarDocumento: (id: string, mockup?: () => Documento) =>
    llamar<Documento>('aprobar_documento', { id }, mockup ?? (() => ({}) as Documento)),

  /** Elimina un documento del proyecto y su archivo compartido (Líder o Administrador, #120). */
  eliminarDocumento: (id: string, mockup?: () => Documento) =>
    llamar<Documento>('eliminar_documento', { id }, mockup ?? (() => ({}) as Documento)),

  /** Abre un documento con la aplicación predeterminada del sistema en esta máquina (#120). */
  abrirDocumento: (id: string, mockup?: () => void) =>
    llamar<void>('abrir_documento', { id }, mockup ?? (() => undefined)),

  /** Guarda la huella vista de un panel para este usuario en esta máquina (#121). */
  marcarVisto: (panel: string, huella: string, mockup?: () => void) =>
    llamar<void>('marcar_visto', { panel, huella }, mockup ?? (() => undefined)),

  /** Devuelve las huellas vistas guardadas `{ panel: huella }` del usuario actual (#121). */
  obtenerVistos: (mockup?: () => Record<string, string>) =>
    llamar<Record<string, string>>('obtener_vistos', undefined, mockup ?? (() => ({}))),

  /** Versión instalada de la app. */
  versionActual: async (): Promise<string> => (enTauri() ? getVersion() : VERSION_MOCKUP),

  /** Revisa si hay una versión nueva. En el mockup siempre responde "al día". */
  buscarActualizacion: () =>
    llamar<InfoActualizacion>('buscar_actualizacion', undefined, () => ({
      version_actual: VERSION_MOCKUP,
      version_nueva: null,
      notas: null,
    })),

  /** Descarga e instala la versión nueva; la app se reinicia sola al terminar. */
  instalarActualizacion: async (alAvanzar: (p: ProgresoActualizacion) => void): Promise<void> => {
    if (!enTauri()) return;
    const dejarDeEscuchar = await listen<ProgresoActualizacion>('actualizacion-progreso', (e) => alAvanzar(e.payload));
    try {
      await llamar<void>('instalar_actualizacion', undefined, () => undefined);
    } finally {
      dejarDeEscuchar();
    }
  },
};
