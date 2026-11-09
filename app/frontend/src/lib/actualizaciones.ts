import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { enTauri } from './tauri';

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

/** Versión del mockup cuando corre en el navegador (sin Tauri). */
const VERSION_MOCKUP = '0.1.0';

export async function versionActual(): Promise<string> {
  return enTauri() ? getVersion() : VERSION_MOCKUP;
}

/** Revisa si hay una versión nueva. En el navegador siempre responde "al día". */
export async function buscarActualizacion(): Promise<InfoActualizacion> {
  if (!enTauri()) return { version_actual: VERSION_MOCKUP, version_nueva: null, notas: null };
  return invoke<InfoActualizacion>('buscar_actualizacion');
}

/** Descarga e instala la versión nueva; la app se reinicia sola al terminar. */
export async function instalarActualizacion(alAvanzar: (p: ProgresoActualizacion) => void): Promise<void> {
  if (!enTauri()) return;
  const dejarDeEscuchar = await listen<ProgresoActualizacion>('actualizacion-progreso', (e) => alAvanzar(e.payload));
  try {
    await invoke('instalar_actualizacion');
  } finally {
    dejarDeEscuchar();
  }
}

/** Texto del error que manda el backend (`{ codigo, mensaje }`). */
export function mensajeDeError(e: unknown): string {
  if (e && typeof e === 'object' && 'mensaje' in e) return String((e as { mensaje: unknown }).mensaje);
  return String(e);
}
