import { Line } from 'react-chartjs-2';
import type { Chart, Plugin, ScriptableContext } from 'chart.js';
import '../lib/charts';
import type { Sprint, Tarea } from '../types';
import { IconoEnLinea } from './Icono';

const MAX_PROYECCION_DIAS = 90;

// Colores del gráfico (mismos que el resto de la app: verde = avance, gris = referencia).
export const VERDE = '#22c55e';
export const GRIS_IDEAL = '#5b6375';
const GRIS_COMPARAR = 'rgba(154,162,177,0.75)';
export const TINTA = '#9aa2b1';
export const TINTA_SUAVE = '#6b7385';
export const REJILLA = 'rgba(43,49,63,0.55)';

const MESES = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sep', 'oct', 'nov', 'dic'];
export const fechaCorta = (dia: string) => {
  const [, m, d] = dia.split('-').map(Number);
  return `${d} ${MESES[m - 1]}`;
};

export function listarDias(inicio: string, fin: string): string[] {
  const dias: string[] = [];
  const cur = new Date(`${inicio}T00:00:00`);
  const end = new Date(`${fin}T00:00:00`);
  while (cur <= end) {
    dias.push(cur.toISOString().slice(0, 10));
    cur.setDate(cur.getDate() + 1);
  }
  return dias;
}

export function sumarDias(dia: string, n: number): string {
  const d = new Date(`${dia}T00:00:00`);
  d.setDate(d.getDate() + n);
  return d.toISOString().slice(0, 10);
}

interface Serie {
  dias: string[];
  total: number;
  burndown: number[];
  // Recta de mínimos cuadrados sobre los días ya transcurridos (y = m·x + b).
  regresion: { m: number; b: number } | null;
  xZero: number | null;
  nPuntos: number;
  avance: number;
}

function calcularSerie(sprint: Sprint, tareas: Tarea[]): Serie {
  const dias = sprint.fecha_fin ? listarDias(sprint.fecha_inicio, sprint.fecha_fin) : [];
  const total = tareas.length;
  const burndown = dias.map((dia) => total - tareas.filter((t) => t.fecha_completada && t.fecha_completada <= dia).length);

  const hoy = new Date().toISOString().slice(0, 10);
  // Días con datos reales: hasta hoy, o hasta el cierre si ya se cerró.
  const tope = sprint.cerrado && sprint.fecha_cierre_real ? sprint.fecha_cierre_real : hoy;
  const nPuntos = dias.filter((d) => d <= tope).length;

  let regresion: Serie['regresion'] = null;
  if (nPuntos >= 2) {
    const ys = burndown.slice(0, nPuntos);
    const xs = ys.map((_, i) => i);
    const sumX = xs.reduce((a, x) => a + x, 0);
    const sumY = ys.reduce((a, y) => a + y, 0);
    const sumXY = xs.reduce((a, x, i) => a + x * ys[i], 0);
    const sumX2 = xs.reduce((a, x) => a + x * x, 0);
    const denom = nPuntos * sumX2 - sumX * sumX;
    if (denom !== 0) {
      const m = (nPuntos * sumXY - sumX * sumY) / denom;
      regresion = { m, b: (sumY - m * sumX) / nPuntos };
    }
  }
  const xZero = regresion && regresion.m < 0 ? Math.min(-regresion.b / regresion.m, dias.length - 1 + MAX_PROYECCION_DIAS) : null;
  const avance = total > 0 && nPuntos > 0 ? (total - burndown[nPuntos - 1]) / total : 0;

  return { dias, total, burndown, regresion, xZero, nPuntos, avance };
}

// Línea vertical "Hoy" (solo con el sprint en curso).
export const lineaHoy = (x: number | null): Plugin<'line'> => ({
  id: 'lineaHoy',
  afterDatasetsDraw(chart: Chart) {
    if (x === null) return;
    const { ctx, chartArea, scales } = chart;
    const px = scales.x.getPixelForValue(x);
    ctx.save();
    ctx.strokeStyle = 'rgba(154,162,177,0.45)';
    ctx.lineWidth = 1;
    ctx.setLineDash([3, 4]);
    ctx.beginPath();
    ctx.moveTo(px, chartArea.top);
    ctx.lineTo(px, chartArea.bottom);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = TINTA;
    ctx.font = "600 11px 'Inter', 'Segoe UI', system-ui, sans-serif";
    ctx.textAlign = 'center';
    ctx.fillText('Hoy', px, chartArea.top - 8);
    ctx.restore();
  },
});

export function BurndownChart({
  sprint,
  tareas,
  contexto = 'avance',
  comparar,
}: {
  sprint: Sprint;
  tareas: Tarea[];
  contexto?: 'avance' | 'reporte';
  comparar?: { sprint: Sprint; tareas: Tarea[] };
}) {
  const s = calcularSerie(sprint, tareas);
  const comp = comparar ? calcularSerie(comparar.sprint, comparar.tareas) : null;

  if (s.dias.length === 0) {
    return (
      <>
        <h3 className="mb-2 text-sm font-semibold text-text">{sprint.nombre}</h3>
        <p className="text-sm text-muted">Este sprint no tiene fechas — no se puede graficar el burndown.</p>
      </>
    );
  }

  const ultimo = s.dias.length - 1;
  const idxHoy = s.nPuntos - 1;
  const enCurso = !sprint.cerrado && s.nPuntos > 0 && s.nPuntos < s.dias.length;
  // Sprint que todavía no empieza: no hay línea real, solo la ideal.
  const porEmpezar = !sprint.cerrado && s.nPuntos === 0;
  const restantes = s.nPuntos > 0 ? s.burndown[idxHoy] : s.total;
  // El día en que se terminaría es el primero en que la recta ya llegó a 0 (redondeo hacia arriba).
  const diaFin = s.xZero !== null ? Math.ceil(s.xZero - 1e-9) : null;
  const fechaProyectada = diaFin !== null ? sumarDias(s.dias[0], diaFin) : null;
  const aTiempo = diaFin !== null && diaFin <= ultimo;

  let veredicto: { texto: string; nivel: 'bien' | 'mejora' | 'mal' } | null = null;
  if (contexto === 'reporte') {
    // Reporte: cómo terminó, sin proyecciones.
    const ultimoCierre = tareas.reduce((m, t) => (t.fecha_completada && t.fecha_completada > m ? t.fecha_completada : m), '');
    veredicto =
      restantes === 0
        ? { nivel: 'bien', texto: `${sprint.nombre} terminó sus ${s.total} tareas${ultimoCierre ? ` el ${fechaCorta(ultimoCierre)}` : ''}.` }
        : { nivel: 'mal', texto: `${sprint.nombre} cerró con ${restantes} de ${s.total} tareas sin terminar.` };
  } else if (fechaProyectada) {
    const f = fechaCorta(fechaProyectada);
    const limite = fechaCorta(s.dias[ultimo]);
    veredicto = {
      nivel: aTiempo ? 'bien' : 'mal',
      texto: aTiempo
        ? `Al ritmo actual se termina el ${f}, dentro del sprint.`
        : `Al ritmo actual se terminaría el ${f}, después del límite (${limite}).`,
    };
  } else if (s.nPuntos >= 2) {
    veredicto = { nivel: 'mal', texto: 'Sin avance neto: al ritmo actual no se proyecta terminar.' };
  }
  if (veredicto?.nivel === 'mal' && comp && s.avance - comp.avance >= 0.15) {
    veredicto = {
      nivel: 'mejora',
      texto: `Mejor que ${comparar!.sprint.nombre} (de ${Math.round(comp.avance * 100)}% a ${Math.round(s.avance * 100)}% completado), aunque todavía no alcanza para terminar a tiempo.`,
    };
  }

  // Eje X numérico (día del sprint, desde 0) para que la proyección cruce el 0 donde de verdad cae.
  // El eje solo se alarga por la proyección mientras el sprint está en curso.
  const maxX = Math.max(ultimo, enCurso ? diaFin ?? 0 : 0, comp ? comp.dias.length - 1 : 0);
  const puntos = (ys: number[], hasta: number) => ys.slice(0, hasta).map((y, x) => ({ x, y }));

  const relleno = (ctx: ScriptableContext<'line'>) => {
    const { chart } = ctx;
    if (!chart.chartArea) return 'transparent';
    const g = chart.ctx.createLinearGradient(0, chart.chartArea.top, 0, chart.chartArea.bottom);
    g.addColorStop(0, 'rgba(34,197,94,0.22)');
    g.addColorStop(1, 'rgba(34,197,94,0)');
    return g;
  };

  const hayProyeccion = !!s.regresion && s.xZero !== null && enCurso;
  const datasets = [
    // Sprint por empezar: sin serie real (no hay progreso), solo la ideal.
    ...(porEmpezar ? [] : [{
      label: 'Restantes',
      data: puntos(s.burndown, Math.max(1, s.nPuntos)),
      borderColor: VERDE,
      backgroundColor: relleno,
      fill: 'origin' as const,
      borderWidth: 2.5,
      cubicInterpolationMode: 'monotone' as const,
      pointRadius: (c: ScriptableContext<'line'>) => (c.dataIndex === idxHoy ? 5 : 0),
      pointBackgroundColor: VERDE,
      pointBorderColor: '#151922',
      pointBorderWidth: 2,
      pointHoverRadius: 5,
      order: 1,
    }]),
    {
      label: 'Ideal',
      data: [
        { x: 0, y: s.total },
        { x: ultimo, y: 0 },
      ],
      borderColor: GRIS_IDEAL,
      borderDash: [5, 5],
      borderWidth: 1.5,
      pointRadius: 0,
      pointHoverRadius: 0,
      fill: false,
      order: 3,
    },
    ...(hayProyeccion
      ? [
          {
            label: 'Proyección',
            data: [
              { x: idxHoy, y: restantes },
              { x: s.xZero as number, y: 0 },
            ],
            borderColor: 'rgba(34,197,94,0.55)',
            borderDash: [2, 4],
            borderWidth: 2,
            pointRadius: 0,
            pointHoverRadius: 0,
            fill: false,
            order: 2,
          },
        ]
      : []),
    ...(comp
      ? [
          {
            label: comparar!.sprint.nombre,
            data: puntos(comp.burndown, Math.max(1, comp.nPuntos)),
            borderColor: GRIS_COMPARAR,
            borderWidth: 1.5,
            cubicInterpolationMode: 'monotone' as const,
            pointRadius: 0,
            fill: false,
            order: 4,
          },
        ]
      : []),
  ];

  const leyenda = [
    ...(porEmpezar ? [] : [{ nombre: 'Restantes', estilo: { background: VERDE, height: 3 } }]),
    { nombre: 'Ideal', estilo: { backgroundImage: `repeating-linear-gradient(90deg, ${GRIS_IDEAL} 0 5px, transparent 5px 10px)`, height: 2 } },
    ...(hayProyeccion
      ? [{ nombre: 'Proyección', estilo: { backgroundImage: 'repeating-linear-gradient(90deg, rgba(34,197,94,0.75) 0 2px, transparent 2px 6px)', height: 2 } }]
      : []),
    ...(comp ? [{ nombre: comparar!.sprint.nombre, estilo: { background: GRIS_COMPARAR, height: 2 } }] : []),
  ];

  const etiquetaX = (x: number) => (comp ? `Día ${x + 1}` : fechaCorta(sumarDias(s.dias[0], x)));

  return (
    <>
      <div className="mb-5 flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
        <div>
          <h3 className="text-sm font-semibold text-text">{sprint.nombre}</h3>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-3xl font-bold tabular-nums text-text">{restantes}</span>
            <span className="text-sm text-muted">de {s.total} tareas restantes</span>
          </div>
        </div>
        <dl className="flex gap-8 text-right">
          <div>
            <dt className="text-[11px] uppercase tracking-wide text-faint">{porEmpezar ? 'Empieza' : enCurso ? 'Día' : 'Duración'}</dt>
            <dd className="mt-0.5 text-sm font-semibold tabular-nums text-text">
              {porEmpezar ? fechaCorta(s.dias[0]) : enCurso ? `${s.nPuntos} de ${s.dias.length}` : `${sprint.duracion_dias ?? s.dias.length} días`}
            </dd>
          </div>
          <div>
            <dt className="text-[11px] uppercase tracking-wide text-faint">Completado</dt>
            <dd className="mt-0.5 text-sm font-semibold tabular-nums text-text">{Math.round(s.avance * 100)}%</dd>
          </div>
          {fechaProyectada && enCurso && (
            <div>
              <dt className="text-[11px] uppercase tracking-wide text-faint">Cierre estimado</dt>
              <dd className={`mt-0.5 text-sm font-semibold tabular-nums ${aTiempo ? 'text-green-300' : 'text-red-300'}`}>
                {fechaCorta(fechaProyectada)}
              </dd>
            </div>
          )}
        </dl>
      </div>

      <div style={{ height: 320 }}>
        <Line
          key={sprint.id}
          data={{ datasets }}
          plugins={[lineaHoy(enCurso ? idxHoy : null)]}
          options={{
            maintainAspectRatio: false,
            parsing: false,
            layout: { padding: { top: 20, right: 8 } },
            interaction: { mode: 'nearest', axis: 'x', intersect: false },
            plugins: {
              legend: { display: false },
              tooltip: {
                filter: (item) => item.dataset.label === 'Restantes' || item.dataset.label === comparar?.sprint.nombre,
                backgroundColor: '#1b1f2a',
                borderColor: '#2b313f',
                borderWidth: 1,
                titleColor: '#e6e9ef',
                bodyColor: TINTA,
                padding: 10,
                cornerRadius: 8,
                displayColors: false,
                callbacks: {
                  title: (items) => {
                    const x = Math.round(items[0].parsed.x ?? 0);
                    return comp ? `Día ${x + 1}` : `${fechaCorta(sumarDias(s.dias[0], x))} · día ${x + 1}`;
                  },
                  label: (item) => `${item.dataset.label}: ${item.parsed.y} tareas`,
                },
              },
            },
            scales: {
              x: {
                type: 'linear',
                min: 0,
                max: maxX,
                border: { display: false },
                grid: { display: false },
                ticks: {
                  stepSize: Math.max(1, Math.ceil(maxX / 7)),
                  color: TINTA_SUAVE,
                  font: { size: 11 },
                  callback: (v) => (Number.isInteger(v) ? etiquetaX(Number(v)) : ''),
                },
              },
              y: {
                beginAtZero: true,
                suggestedMax: s.total,
                border: { display: false },
                grid: { color: REJILLA },
                ticks: { precision: 0, color: TINTA_SUAVE, font: { size: 11 }, padding: 8, maxTicksLimit: 6 },
              },
            },
          }}
        />
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <ul className="flex flex-wrap items-center gap-5 text-xs text-muted">
          {leyenda.map((l) => (
            <li key={l.nombre} className="flex items-center gap-2">
              <span className="inline-block w-5 rounded-full" style={l.estilo} />
              {l.nombre}
            </li>
          ))}
        </ul>
        {veredicto && (
          <p
            className={`flex items-center gap-1.5 text-xs ${
              veredicto.nivel === 'bien' ? 'text-green-300' : veredicto.nivel === 'mejora' ? 'text-orange-300' : 'text-red-300'
            }`}
          >
            <IconoEnLinea
              nombre={veredicto.nivel === 'bien' ? 'check' : veredicto.nivel === 'mejora' ? 'avance' : 'alerta'}
              sinColor
            />
            {veredicto.texto}
          </p>
        )}
      </div>
    </>
  );
}
