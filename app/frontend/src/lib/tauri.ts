import { invoke } from '@tauri-apps/api/core';

/** `true` cuando la app corre dentro de la ventana de Tauri (no en el navegador). */
export const enTauri = (): boolean => '__TAURI_INTERNALS__' in window;

/** Llama al comando de prueba del backend. En el navegador devuelve `null`. */
export async function ping(): Promise<string | null> {
  if (!enTauri()) return null;
  return invoke<string>('ping');
}
