import { useState } from 'react';
import './FondoRamas.css';

// Fondo decorativo del acceso: arbolitos de ramas de git que crecen desde los bordes y se van
// bifurcando (más delgadas en cada nivel), con un commit en cada bifurcación. Se generan al azar cada
// vez que se abre la pantalla; una rama que tocaría a otra no se dibuja, así nunca se cruzan.
// Solo decoración (aria-hidden, sin eventos de puntero).

type Segmento = {
  d: string;
  color: string;
  ancho: number;
  nivel: number;
  fin: [number, number];
  punta: boolean;
};
type Linea = [number, number, number, number];

const COLORES = ['#818cf8', '#22d3ee', '#fbbf24', '#f472b6', '#a78bfa', '#2dd4bf', '#fb923c', '#60a5fa', '#fb7185'];
const NIVELES = 6;

const azar = (min: number, max: number) => min + Math.random() * (max - min);
const rad = (g: number) => (g * Math.PI) / 180;

// ¿Se cortan los segmentos p1-p2 y p3-p4? (sin contar que compartan un extremo)
function seCortan([x1, y1, x2, y2]: Linea, [x3, y3, x4, y4]: Linea) {
  const d = (x2 - x1) * (y4 - y3) - (y2 - y1) * (x4 - x3);
  if (Math.abs(d) < 1e-9) return false;
  const t = ((x3 - x1) * (y4 - y3) - (y3 - y1) * (x4 - x3)) / d;
  const u = ((x3 - x1) * (y2 - y1) - (y3 - y1) * (x2 - x1)) / d;
  return t > 0.02 && t < 0.98 && u > 0.02 && u < 0.98;
}

// Distancia mínima entre dos segmentos (0 si se cortan).
function distancia(a: Linea, b: Linea) {
  if (seCortan(a, b)) return 0;
  const aPunto = ([x1, y1, x2, y2]: Linea, px: number, py: number) => {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const l = dx * dx + dy * dy || 1;
    const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / l));
    return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
  };
  return Math.min(aPunto(a, b[0], b[1]), aPunto(a, b[2], b[3]), aPunto(b, a[0], a[1]), aPunto(b, a[2], a[3]));
}

const MARGEN = 14;

function generarArboles(ANCHO: number, ALTO: number): Segmento[] {
  const segmentos: Segmento[] = [];
  const ocupadas: Linea[] = [];
  let color = Math.floor(azar(0, COLORES.length));

  // Curva suave desde (x, y) en el ángulo dado; devuelve el tramo y su forma aproximada en líneas.
  const tramo = (x: number, y: number, angulo: number, largo: number) => {
    const xf = x + largo * Math.cos(rad(angulo));
    const yf = y + largo * Math.sin(rad(angulo));
    const curva = azar(-0.18, 0.18) * largo;
    const cx = (x + xf) / 2 - curva * Math.sin(rad(angulo));
    const cy = (y + yf) / 2 + curva * Math.cos(rad(angulo));
    const puntos: [number, number][] = [];
    for (let k = 0; k <= 6; k++) {
      const t = k / 6;
      puntos.push([(1 - t) ** 2 * x + 2 * (1 - t) * t * cx + t ** 2 * xf, (1 - t) ** 2 * y + 2 * (1 - t) * t * cy + t ** 2 * yf]);
    }
    const lineas: Linea[] = puntos.slice(1).map((p, k) => [puntos[k][0], puntos[k][1], p[0], p[1]]);
    return { d: `M ${x} ${y} Q ${cx} ${cy} ${xf} ${yf}`, xf, yf, lineas };
  };

  const crecer = (x: number, y: number, angulo: number, largo: number, ancho: number, nivel: number, tono: string) => {
    const t = tramo(x, y, angulo, largo);
    // Se descarta la rama si tocaría a otra ya dibujada.
    // Las primeras partes nacen pegadas a la rama madre y a sus hermanas: ahí solo se exige no cortarlas.
    if (
      t.lineas.some((l, k) =>
        ocupadas.some((o) => (k < 2 ? seCortan(l, o) : distancia(l, o) < MARGEN))
      )
    )
      return false;
    ocupadas.push(...t.lineas);
    const seg: Segmento = { d: t.d, color: tono, ancho, nivel, fin: [t.xf, t.yf], punta: true };
    segmentos.push(seg);
    if (nivel >= NIVELES) return true;

    // Se bifurca en dos (a veces tres) ramas más cortas y delgadas, abiertas hacia los lados.
    const hijos = Math.random() < 0.25 && nivel < 3 ? 3 : 2;
    const abertura = azar(22, 38);
    let alguno = false;
    for (let h = 0; h < hijos; h++) {
      const desvio = hijos === 2 ? (h === 0 ? -abertura : abertura) : (h - 1) * abertura * 1.1;
      // Desde el primer nivel cada rama toma su color y lo heredan sus hijas.
      const tonoHijo = nivel === 1 ? COLORES[color++ % COLORES.length] : tono;
      const ok = crecer(t.xf, t.yf, angulo + desvio + azar(-8, 8), largo * azar(0.68, 0.84), Math.max(1.2, ancho * 0.75), nivel + 1, tonoHijo);
      alguno = alguno || ok;
    }
    seg.punta = !alguno;
    return true;
  };

  // Troncos repartidos por los cuatro bordes según el tamaño de la ventana, para que no queden huecos.
  const largo = Math.min(ANCHO, ALTO) * 0.2;
  const troncos: [number, number, number][] = [];
  // Abajo y arriba: uno cada ~380 px, con un poco de desorden.
  const nAbajo = Math.max(2, Math.round(ANCHO / 380));
  for (let k = 0; k < nAbajo; k++) {
    const x = (ANCHO * (k + 0.5)) / nAbajo + azar(-50, 50);
    troncos.push([x, ALTO + 20, -90 + azar(-12, 12)]);
    troncos.push([x + ANCHO / nAbajo / 2, -20, 90 + azar(-12, 12)]);
  }
  // Lados: uno cada ~360 px.
  const nLado = Math.max(1, Math.round(ALTO / 360));
  for (let k = 0; k < nLado; k++) {
    const y = (ALTO * (k + 0.5)) / nLado + azar(-40, 40);
    troncos.push([-20, y, azar(-10, 10)]);
    troncos.push([ANCHO + 20, y + ALTO / nLado / 2 - ALTO / nLado / 4, 180 + azar(-10, 10)]);
  }
  // Se mezclan para que ningún borde se quede siempre con el espacio libre.
  troncos.sort(() => Math.random() - 0.5);
  for (const [x, y, angulo] of troncos) crecer(x, y, angulo, largo * azar(0.9, 1.1), 4.5, 1, '#4ade80');
  return segmentos;
}

// Desfase estable por rama para que los destellos no salgan todos a la vez.
function desfase(i: number) {
  const x = Math.sin(i * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

export function FondoRamas() {
  const [{ ancho, alto, segmentos }] = useState(() => {
    const ancho = Math.max(800, window.innerWidth);
    const alto = Math.max(600, window.innerHeight);
    return { ancho, alto, segmentos: generarArboles(ancho, alto) };
  });
  return (
    <svg className="fondo-ramas" viewBox={`0 0 ${ancho} ${alto}`} preserveAspectRatio="xMidYMid slice" aria-hidden="true">
      {segmentos.map((s, i) => {
        // Crece desde la raíz: cada nivel empieza cuando termina el anterior.
        const inicio = (s.nivel - 1) * 0.55;
        return (
          <g key={i}>
            <path
              className="rama"
              d={s.d}
              stroke={s.color}
              pathLength={1000}
              style={{ strokeWidth: s.ancho, animationDuration: '0.6s', animationDelay: `${inicio}s` }}
            />
            <path
              className="flujo"
              d={s.d}
              stroke="#ffffff"
              pathLength={1000}
              style={{ strokeWidth: s.ancho, animationDelay: `3.2s, ${3.2 + desfase(i) * 9}s` }}
            />
            <circle
              className="nodo"
              cx={s.fin[0]}
              cy={s.fin[1]}
              r={s.punta ? 4 : Math.max(5, s.ancho + 3)}
              fill="#161922"
              stroke={s.color}
              strokeWidth={s.punta ? 2 : 3}
              style={{ animationDelay: `${inicio + 0.5}s, ${desfase(i + 7) * 4}s` }}
            />
          </g>
        );
      })}
    </svg>
  );
}
