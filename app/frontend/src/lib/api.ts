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
import type { Rol, Tarea, TipoTarea } from '../types';

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
