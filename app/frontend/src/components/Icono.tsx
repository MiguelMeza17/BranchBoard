import type { CSSProperties } from 'react';

// Íconos de línea de toda la app: mismo trazo (1.8) y estilo en todas partes, en vez de emojis.
// Cada uno tiene un color suave por defecto (apagado, para no recargar); `sinColor` hereda el color
// del texto y `color` fuerza uno (p. ej. el color de un tipo de tarea).
const C = (cx: number, cy: number, r: number) => `M${cx} ${cy}m-${r} 0a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 -${2 * r} 0`;

const TRAZOS: Record<string, string[]> = {
  // --- menú ---
  chat: ['M21 12a8.5 8.5 0 0 1-12.3 7.6L3.5 21l1.4-5A8.5 8.5 0 1 1 21 12Z'],
  scrum: ['M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z', 'M9 4v16', 'M15 4v16'],
  lista: ['M9 6h12', 'M9 12h12', 'M9 18h12', 'M4 6h.01', 'M4 12h.01', 'M4 18h.01'],
  planificacion: ['M5 5h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z', 'M16 3v4', 'M8 3v4', 'M3 10h18'],
  categorias: ['M4 4h6v6H4z', 'M14 4h6v6h-6z', 'M4 14h6v6H4z', 'M14 14h6v6h-6z'],
  revision: ['M6 8.5v7', 'M6 8.5c0 3.5 3 4 9.5 4', C(6, 6, 2.5), C(6, 18, 2.5), C(18, 12.5, 2.5)],
  resumen: ['M12 3a9 9 0 1 0 9 9h-9V3Z', 'M15.5 2.8v5.7h5.7a6 6 0 0 0-5.7-5.7Z'],
  avance: ['M3 17l6-6 4 4 8-8', 'M15 7h6v6'],
  pendientes: [C(12, 12, 9), 'M12 7v5l3.5 2'],
  reportes: ['M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z', 'M14 3v6h6', 'M8 13h8', 'M8 17h5'],
  branches: ['M6 7v10', 'M18 9.5v.5a5 5 0 0 1-5 5H8', C(6, 5, 2), C(6, 19, 2), C(18, 7.5, 2)],
  usuarios: [C(9, 8, 3.5), 'M2.5 20a6.5 6.5 0 0 1 13 0', 'M16 4.6a3.5 3.5 0 0 1 0 6.8', 'M18.5 14.6A6.5 6.5 0 0 1 21.5 20'],
  papelera: ['M4 7h16', 'M10 11v6', 'M14 11v6', 'M6 7l1 13h10l1-13', 'M9 7V4h6v3'],
  ajustes: [
    C(12, 12, 3),
    'M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z',
  ],
  // --- acciones y estados ---
  check: ['M5 12.5l4.5 4.5L19 7.5'],
  x: ['M6 6l12 12', 'M18 6L6 18'],
  alerta: ['M12 3.5 2.5 20h19L12 3.5Z', 'M12 10v4.5', 'M12 17.5h.01'],
  prohibido: [C(12, 12, 9), 'M5.6 5.6l12.8 12.8'],
  bandeja: ['M3 13h5l1.5 3h5L16 13h5', 'M5.5 5h13L21 13v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-5l2.5-8Z'],
  recibir: ['M12 3v11', 'M7.5 9.5 12 14l4.5-4.5', 'M3 13h5l1.5 3h5L16 13h5', 'M3 13v5a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5'],
  lapiz: ['M15.5 4.5l4 4L8 20H4v-4L15.5 4.5Z', 'M13.5 6.5l4 4'],
  usuario: [C(12, 8, 4), 'M4 21a8 8 0 0 1 16 0'],
  reloj: [C(12, 12, 9), 'M12 7v5l3.5 2'],
  basedatos: ['M12 3c4.4 0 8 1.3 8 3s-3.6 3-8 3-8-1.3-8-3 3.6-3 8-3Z', 'M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6', 'M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3'],
  caja: ['M3 7.5 12 3l9 4.5v9L12 21l-9-4.5v-9Z', 'M3 7.5l9 4.5 9-4.5', 'M12 12v9'],
  campana: ['M6 9a6 6 0 0 1 12 0c0 6 2.5 7.5 2.5 7.5h-17S6 15 6 9Z', 'M10 20a2 2 0 0 0 4 0'],
  pieza: ['M10 4a2 2 0 1 1 4 0v1h4a1 1 0 0 1 1 1v4h-1a2 2 0 1 0 0 4h1v4a1 1 0 0 1-1 1h-4v-1a2 2 0 1 0-4 0v1H6a1 1 0 0 1-1-1v-4h1a2 2 0 1 0 0-4H5V6a1 1 0 0 1 1-1h4V4Z'],
  carpeta: ['M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z'],
  carpetaAbierta: ['M3 18V7a2 2 0 0 1 2-2h4l2 2.5h6a2 2 0 0 1 2 2V11', 'M3 18l2.6-6.2A1.5 1.5 0 0 1 7 11h13.5a1 1 0 0 1 .9 1.4L19 18.5a2.5 2.5 0 0 1-2.3 1.5H5a2 2 0 0 1-2-2Z'],
  documento: ['M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z', 'M14 3v6h6'],
  chispa: ['M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8L12 3Z', 'M19 15l.7 1.8 1.8.7-1.8.7L19 20l-.7-1.8-1.8-.7 1.8-.7L19 15Z'],
  descargar: ['M12 4v11', 'M7 10.5l5 5 5-5', 'M4 20h16'],
  subir: ['M12 20V9', 'M7 13.5l5-5 5 5', 'M4 4h16'],
  intercambio: ['M7 7h13', 'M16 3l4 4-4 4', 'M17 17H4', 'M8 13l-4 4 4 4'],
  checkin: [C(12, 12, 3.5), 'M12 3v5.5', 'M12 15.5V21'],
  unity: ['M6 9h12a3 3 0 0 1 3 3v3a3 3 0 0 1-5.4 1.8L14.5 15h-5l-1.1 1.8A3 3 0 0 1 3 15v-3a3 3 0 0 1 3-3Z', 'M8 11v3', 'M6.5 12.5h3', 'M15.5 12h.01', 'M17.5 13.5h.01'],
  llave: [C(8, 15, 4), 'M10.8 12.2 20 3', 'M16 7l2.5 2.5', 'M18.5 4.5 21 7'],
  sobre: ['M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Z', 'M3 6l9 7 9-7'],
  puerta: ['M5 21V4a1 1 0 0 1 1-1h9v18', 'M3 21h18', 'M15 3l4 1.5V21', 'M11.5 12h.01'],
  candado: ['M6 11h12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1Z', 'M8 11V7.5a4 4 0 0 1 8 0V11'],
  ojo: ['M2.5 12S6 5 12 5s9.5 7 9.5 7-3.5 7-9.5 7-9.5-7-9.5-7Z', C(12, 12, 3)],
  mano: ['M8 13V5.5a1.5 1.5 0 0 1 3 0V11', 'M11 10.5V4a1.5 1.5 0 0 1 3 0v6.5', 'M14 10.5V5.5a1.5 1.5 0 0 1 3 0V13', 'M17 9.5a1.5 1.5 0 0 1 3 0V14a7 7 0 0 1-7 7h-1.3a6 6 0 0 1-4.6-2.2L4 15.2a1.6 1.6 0 0 1 2.4-2.1L8 14.5'],
  sprint: ['M13 2 4 14h7l-1 8 9-12h-7l1-8Z'],
  buscar: [C(11, 11, 7), 'M20.5 20.5 16 16'],
  mas: ['M12 5v14', 'M5 12h14'],
  apagar: ['M12 3v9', 'M6.3 6.8a8 8 0 1 0 11.4 0'],
  cuadros: ['M4 4h7v7H4z', 'M13 4h7v7h-7z', 'M4 13h7v7H4z', 'M13 13h7v7h-7z'],
  cuadrosPequenos: ['M4 4h4v4H4z', 'M10 4h4v4h-4z', 'M16 4h4v4h-4z', 'M4 10h4v4H4z', 'M10 10h4v4h-4z', 'M16 10h4v4h-4z', 'M4 16h4v4H4z', 'M10 16h4v4h-4z', 'M16 16h4v4h-4z'],
  // --- archivos ---
  pdf: ['M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z', 'M14 3v6h6', 'M8 13h8', 'M8 17h8'],
  hoja: ['M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z', 'M14 3v6h6', 'M8 12h8v6H8z', 'M12 12v6', 'M8 15h8'],
  diapositivas: ['M3 4h18', 'M4 4v11a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1V4', 'M12 16v4', 'M8.5 20h7'],
  imagen: ['M5 4h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z', C(9, 9, 1.8), 'M20 15.5 15 10.5 5 20'],
  comprimido: ['M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6Z', 'M10 3v2', 'M10 7v2', 'M10 11v2', 'M9 15h2v3H9z'],
  clip: ['M20 11.5 12 19.5a5 5 0 0 1-7-7l8-8a3.3 3.3 0 0 1 4.7 4.7l-8 8a1.7 1.7 0 0 1-2.4-2.4L15 7.5'],
  // --- tipos de tarea ---
  codigo: ['M8 8l-4 4 4 4', 'M16 8l4 4-4 4', 'M13.5 5.5l-3 13'],
  cubo: ['M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3Z', 'M4 7.5l8 4.5 8-4.5', 'M12 12v9'],
  capas: ['M12 3 3 8l9 5 9-5-9-5Z', 'M3 13l9 5 9-5'],
  animacion: [C(12, 12, 9), 'M10 8.5v7l6-3.5-6-3.5Z'],
  pincel: ['M18.5 3.5a2.1 2.1 0 0 1 3 3L12 16l-3-3 9.5-9.5Z', 'M9 13c-2.5 0-4 1.5-4 4 0 1.5-1 2.5-2 3 3 0 7-1 7-4.5'],
  altavoz: ['M4 9h3l5-4v14l-5-4H4V9Z', 'M16 9a4 4 0 0 1 0 6', 'M18.5 6.5a7.5 7.5 0 0 1 0 11'],
  musica: ['M9 18V5l11-2v13', C(6.5, 18, 2.5), C(17.5, 16, 2.5)],
  video: ['M4 6h11a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1Z', 'M16 10.5 21 7.5v9l-5-3'],
  bicho: ['M9 7a3 3 0 0 1 6 0', 'M8 9h8a1 1 0 0 1 1 1v4a5 5 0 0 1-10 0v-4a1 1 0 0 1 1-1Z', 'M12 9v10', 'M3 13h4', 'M17 13h4', 'M4 8l3 2', 'M20 8l-3 2', 'M4 19l3-2', 'M20 19l-3-2'],
  herramienta: ['M14.5 6.5a4.5 4.5 0 0 0 5.6 5.6L21 13l-8 8-3-3 .9-.9A4.5 4.5 0 0 1 5.3 11.5L3 9.2 9.2 3l2.3 2.3a4.5 4.5 0 0 1 3 1.2Z'],
  pregunta: [C(12, 12, 9), 'M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .8-1 1.5v.7', 'M12 17h.01'],
};

// Color suave por defecto (apagado) de cada ícono.
const COLOR: Record<string, string> = {
  chat: 'text-emerald-300/80',
  scrum: 'text-yellow-300/80',
  lista: 'text-sky-300/80',
  planificacion: 'text-violet-300/80',
  categorias: 'text-teal-300/80',
  revision: 'text-fuchsia-300/80',
  resumen: 'text-cyan-300/80',
  avance: 'text-lime-300/80',
  pendientes: 'text-amber-300/80',
  reportes: 'text-rose-300/80',
  branches: 'text-green-400/80',
  usuarios: 'text-blue-300/80',
  papelera: 'text-red-300/70',
  ajustes: 'text-slate-300/70',
  check: 'text-emerald-300/90',
  x: 'text-slate-300/70',
  alerta: 'text-amber-300/90',
  prohibido: 'text-red-300/90',
  bandeja: 'text-slate-300/70',
  recibir: 'text-sky-300/80',
  lapiz: 'text-yellow-300/80',
  usuario: 'text-blue-300/80',
  reloj: 'text-amber-300/80',
  basedatos: 'text-cyan-300/80',
  caja: 'text-orange-300/80',
  campana: 'text-yellow-300/80',
  pieza: 'text-cyan-300/80',
  carpeta: 'text-amber-300/80',
  carpetaAbierta: 'text-amber-300/80',
  documento: 'text-sky-300/80',
  chispa: 'text-violet-300/80',
  descargar: 'text-sky-300/80',
  subir: 'text-sky-300/80',
  intercambio: 'text-teal-300/80',
  checkin: 'text-violet-300/80',
  unity: 'text-indigo-300/80',
  llave: 'text-amber-300/80',
  sobre: 'text-sky-300/80',
  puerta: 'text-orange-300/80',
  candado: 'text-amber-300/80',
  ojo: 'text-slate-300/80',
  mano: 'text-yellow-300/80',
  sprint: 'text-yellow-300/80',
  buscar: 'text-slate-300/80',
  mas: 'text-primary',
  apagar: 'text-red-300/80',
  pdf: 'text-red-300/80',
  hoja: 'text-emerald-300/80',
  diapositivas: 'text-orange-300/80',
  imagen: 'text-violet-300/80',
  comprimido: 'text-amber-300/80',
  clip: 'text-slate-300/80',
};

export function Icono({
  nombre,
  className = 'h-4 w-4',
  sinColor = false,
  color,
  title,
}: {
  nombre: string;
  className?: string;
  /** Hereda el color del texto en vez de usar su color suave. */
  sinColor?: boolean;
  /** Color exacto (ej. el de un tipo de tarea). */
  color?: string;
  title?: string;
}) {
  const trazos = TRAZOS[nombre];
  if (!trazos) return null;
  const style: CSSProperties | undefined = color ? { color } : undefined;
  return (
    <svg
      viewBox="0 0 24 24"
      className={`shrink-0 ${className} ${sinColor || color ? '' : COLOR[nombre] ?? ''}`}
      style={style}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden={title ? undefined : true}
      role={title ? 'img' : undefined}
    >
      {title && <title>{title}</title>}
      {trazos.map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}

/** Ícono pequeño en línea con un texto (alineado con la base del texto). */
export function IconoEnLinea({ nombre, sinColor, className = '' }: { nombre: string; sinColor?: boolean; className?: string }) {
  return <Icono nombre={nombre} sinColor={sinColor} className={`mr-1 inline-block h-[1.05em] w-[1.05em] align-[-0.17em] ${className}`} />;
}
