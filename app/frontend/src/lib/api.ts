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
  | 'base_datos'
  | 'archivos'
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

// ───────── Comandos ─────────

export const api = {
  /** Comando de prueba: `"pong"` si el backend responde. */
  ping: () => llamar<string>('ping', undefined, () => 'pong (mockup)'),

  /** `true` mientras haya una operación larga (`cm`) corriendo. */
  hayOperacionEnCurso: () => llamar<boolean>('hay_operacion_en_curso', undefined, () => false),

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
