import type { EstadoTarea, TipoTarea } from '../types';

// Column / state visual language for the Scrum board and badges.
export const estadoMeta: Record<
  EstadoTarea,
  { label: string; dot: string; text: string; bg: string; border: string; accent: string }
> = {
  'por hacer': {
    label: 'Por hacer',
    dot: '#64748b',
    text: 'text-slate-300',
    bg: 'bg-slate-500/10',
    border: 'border-slate-500/30',
    accent: '#64748b',
  },
  'en producción': {
    label: 'En producción',
    dot: '#f59e0b',
    text: 'text-amber-300',
    bg: 'bg-amber-500/10',
    border: 'border-amber-500/30',
    accent: '#f59e0b',
  },
  'en revisión': {
    label: 'En revisión',
    dot: '#a855f7',
    text: 'text-purple-300',
    bg: 'bg-purple-500/10',
    border: 'border-purple-500/30',
    accent: '#a855f7',
  },
  terminada: {
    label: 'Terminada',
    dot: '#22c55e',
    text: 'text-green-300',
    bg: 'bg-green-500/10',
    border: 'border-green-500/30',
    accent: '#22c55e',
  },
};

// 9 task-type categorical palette (distinct hues, dark-friendly).
export const tipoMeta: Record<TipoTarea, { color: string; icono: string }> = {
  Scripting: { color: '#60a5fa', icono: 'codigo' },
  'Modelado 3D': { color: '#f472b6', icono: 'cubo' },
  'Modelado 2D': { color: '#c084fc', icono: 'capas' },
  Animación: { color: '#fb923c', icono: 'animacion' },
  Imagen: { color: '#34d399', icono: 'pincel' },
  'UI/Interfaz': { color: '#22d3ee', icono: 'pieza' },
  Audio: { color: '#facc15', icono: 'altavoz' },
  Música: { color: '#a3e635', icono: 'musica' },
  Video: { color: '#f87171', icono: 'video' },
  Testing: { color: '#fb7185', icono: 'bicho' },
  Corrección: { color: '#f59e0b', icono: 'herramienta' },
  Ajustes: { color: '#cbd5e1', icono: 'ajustes' },
  'Sin definir': { color: '#6b7385', icono: 'pregunta' },
};

export function iniciales(nombre: string, apellido?: string): string {
  const a = nombre?.[0] ?? '?';
  const b = apellido?.[0] ?? '';
  return (a + b).toUpperCase();
}

// Deterministic avatar color from an email.
export function avatarColor(email: string): string {
  const palette = ['#6366f1', '#2dd4bf', '#f472b6', '#f59e0b', '#22c55e', '#a855f7', '#60a5fa'];
  let h = 0;
  for (let i = 0; i < email.length; i++) h = (h * 31 + email.charCodeAt(i)) >>> 0;
  return palette[h % palette.length];
}

// Palabras de confirmación ("asignar", "autorizar", "eliminar", "traer"...): no distinguen
// mayúsculas/minúsculas ni tildes, y se ignoran los espacios al inicio y al final.
export function coincidePalabra(texto: string, palabra: string): boolean {
  const n = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').trim().toLowerCase();
  return n(texto) === n(palabra);
}

// Subtarea comodín "En espera": cambios sin check-in hechos directamente en la rama de una tarea con
// subtareas. No se hace check-in ahí: se crea una rama (subtarea) con nombre, descripción y tipo, y
// los cambios pasan a ella. Es la única con tipo "Sin definir" (ver types.ts).
export function esTareaEnEspera(t: { tipo: string }): boolean {
  return t.tipo === 'Sin definir';
}
export function palabraCambios(n: number): string {
  return n === 1 ? 'commit' : 'commits';
}
