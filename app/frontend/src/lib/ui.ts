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
export const tipoMeta: Record<TipoTarea, { color: string; emoji: string }> = {
  Scripting: { color: '#60a5fa', emoji: '💻' },
  'Modelado 3D': { color: '#f472b6', emoji: '🧊' },
  'Modelado 2D': { color: '#c084fc', emoji: '🖼️' },
  Animación: { color: '#fb923c', emoji: '🎭' },
  Imagen: { color: '#34d399', emoji: '🎨' },
  'UI/Interfaz': { color: '#22d3ee', emoji: '🧩' },
  Audio: { color: '#facc15', emoji: '🔊' },
  Música: { color: '#a3e635', emoji: '🎵' },
  Video: { color: '#f87171', emoji: '🎬' },
  Testing: { color: '#fb7185', emoji: '🐞' },
  Corrección: { color: '#f59e0b', emoji: '🔧' },
  Ajustes: { color: '#cbd5e1', emoji: '⚙️' },
  'Sin definir': { color: '#6b7385', emoji: '❔' },
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

// Un check-in ES un commit (en Unity VC, hacer check-in es confirmar un cambio). Para una tarea
// que se trabaja con check-in directo (sin rama propia) se dice "check-in", no "commit".
export function esCheckinTarea(t: { modo_trabajo: string | null; rama_asociada: string | null }): boolean {
  return t.modo_trabajo === 'checkin' && !t.rama_asociada;
}
// Subtarea comodín "En espera": representa el próximo check-in, cuyos cambios todavía están sin
// guardar en el workspace. Es la única con tipo "Sin definir" (ver types.ts).
export function esTareaEnEspera(t: { tipo: string }): boolean {
  return t.tipo === 'Sin definir';
}
export function palabraCambios(n: number, checkin: boolean): string {
  return checkin ? (n === 1 ? 'check-in' : 'check-ins') : n === 1 ? 'commit' : 'commits';
}
