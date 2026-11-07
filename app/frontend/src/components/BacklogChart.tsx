import { Line } from 'react-chartjs-2';
import type { ScriptableContext } from 'chart.js';
import '../lib/charts';
import type { Sprint, Tarea } from '../types';
import { IconoEnLinea } from './Icono';
import { REJILLA, TINTA, TINTA_SUAVE, VERDE, fechaCorta, lineaHoy, listarDias, sumarDias } from './BurndownChart';

// Avance de un backlog (sin fecha de fin): el ritmo se mide con las últimas 4 semanas.
const VENTANA_DIAS = 28;
const MAX_PROYECCION_DIAS = 365;

const degradado = (rgb: string, alfa: number) => (ctx: ScriptableContext<'line'>) => {
  const { chart } = ctx;
  if (!chart.chartArea) return 'transparent';
  const g = chart.ctx.createLinearGradient(0, chart.chartArea.top, 0, chart.chartArea.bottom);
  g.addColorStop(0, `rgba(${rgb},${alfa})`);
  g.addColorStop(1, `rgba(${rgb},0)`);
  return g;
};

const tooltip = {
  backgroundColor: '#1b1f2a',
  borderColor: '#2b313f',
  borderWidth: 1,
  titleColor: '#e6e9ef',
  bodyColor: TINTA,
  padding: 10,
  cornerRadius: 8,
  boxPadding: 4,
  usePointStyle: true,
};

function Dato({ titulo, valor, tono }: { titulo: string; valor: string; tono?: string }) {
  return (
    <div>
      <dt className="text-[11px] uppercase tracking-wide text-faint">{titulo}</dt>
      <dd className={`mt-0.5 text-sm font-semibold tabular-nums ${tono ?? 'text-text'}`}>{valor}</dd>
    </div>
  );
}

function Leyenda({ items }: { items: { nombre: string; estilo: React.CSSProperties }[] }) {
  return (
    <ul className="mt-3 flex flex-wrap items-center gap-5 text-xs text-muted">
      {items.map((l) => (
        <li key={l.nombre} className="flex items-center gap-2">
          <span className="inline-block w-5 rounded-full" style={l.estilo} />
          {l.nombre}
        </li>
      ))}
    </ul>
  );
}

export function BacklogChart({ backlog, tareas }: { backlog: Sprint; tareas: Tarea[] }) {
  const hoy = new Date().toISOString().slice(0, 10);
  const primera = tareas.reduce((min, t) => (t.fecha_creacion < min ? t.fecha_creacion : min), backlog.fecha_inicio);
  const dias = listarDias(primera, hoy < primera ? primera : hoy);
  const idxHoy = dias.length - 1;

  const totales = dias.map((d) => tareas.filter((t) => t.fecha_creacion <= d).length);
  const hechas = dias.map((d) => tareas.filter((t) => t.fecha_completada && t.fecha_completada <= d).length);
  const restantesSerie = dias.map((_, i) => totales[i] - hechas[i]);
  const total = tareas.length;
  const terminadas = hechas[idxHoy] ?? 0;
  const restantes = total - terminadas;

  // Ritmo: tareas terminadas y tareas nuevas en las últimas 4 semanas.
  const desde = sumarDias(hoy, -VENTANA_DIAS);
  const salen = tareas.filter((t) => t.fecha_completada && t.fecha_completada > desde).length;
  const entran = tareas.filter((t) => t.fecha_creacion > desde).length;
  const porSemana = (n: number) => (n * 7) / VENTANA_DIAS;
  const ritmoDia = salen / VENTANA_DIAS;
  const diasFaltan = restantes === 0 ? 0 : ritmoDia > 0 ? Math.min(MAX_PROYECCION_DIAS, Math.ceil(restantes / ritmoDia)) : null;
  const fechaEstimada = diasFaltan !== null ? sumarDias(hoy, diasFaltan) : null;
  const crece = entran > salen;

  if (total === 0) {
    return <p className="text-sm text-muted">Este backlog todavía no tiene tareas.</p>;
  }

  const etiqueta = (x: number) => fechaCorta(sumarDias(primera, x));
  const ejes = (max: number) => ({
    x: {
      type: 'linear' as const,
      min: 0,
      max,
      border: { display: false },
      grid: { display: false },
      ticks: {
        stepSize: Math.max(1, Math.ceil(max / 6)),
        color: TINTA_SUAVE,
        font: { size: 11 },
        callback: (v: string | number) => (Number.isInteger(v) ? etiqueta(Number(v)) : ''),
      },
    },
    y: {
      beginAtZero: true,
      border: { display: false },
      grid: { color: REJILLA },
      ticks: { precision: 0, color: TINTA_SUAVE, font: { size: 11 }, padding: 8, maxTicksLimit: 6 },
    },
  });
  const tituloTooltip = (items: { parsed: { x: number | null } }[]) => etiqueta(Math.round(items[0].parsed.x ?? 0));
  const puntos = (ys: number[]) => ys.map((y, x) => ({ x, y }));

  // Proyección: al ritmo actual, las restantes bajan hasta 0.
  const xFin = diasFaltan !== null && diasFaltan > 0 ? idxHoy + diasFaltan : idxHoy;
  const hayProyeccion = xFin > idxHoy;

  return (
    <section>
      <div className="mb-5 flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
        <div>
          <h3 className="text-sm font-semibold text-text">{backlog.nombre}</h3>
          <div className="mt-1.5 flex items-baseline gap-1.5">
            <span className="text-3xl font-bold tabular-nums text-text">{restantes}</span>
            <span className="text-sm text-muted">
              tareas restantes · {terminadas} de {total} terminadas
            </span>
          </div>
        </div>
        <dl className="flex gap-8 text-right">
          <Dato titulo="Entran" valor={`${porSemana(entran).toFixed(1)} / semana`} />
          <Dato titulo="Salen" valor={`${porSemana(salen).toFixed(1)} / semana`} />
          <Dato
            titulo="Cierre estimado"
            valor={fechaEstimada ? `${fechaCorta(fechaEstimada)} · ${diasFaltan} días` : 'Sin avance todavía'}
          />
        </dl>
      </div>

      <div style={{ height: 320 }}>
        <Line
          data={{
            datasets: [
              {
                label: 'Restantes',
                data: puntos(restantesSerie),
                borderColor: VERDE,
                backgroundColor: degradado('34,197,94', 0.22),
                fill: 'origin',
                borderWidth: 2.5,
                stepped: 'after',
                pointRadius: (c: ScriptableContext<'line'>) => (c.dataIndex === idxHoy ? 5 : 0),
                pointBackgroundColor: VERDE,
                pointBorderColor: '#151922',
                pointBorderWidth: 2,
                pointHoverRadius: 4,
                pointHoverBackgroundColor: VERDE,
              },
              ...(hayProyeccion
                ? [
                    {
                      label: 'Proyección',
                      data: [
                        { x: idxHoy, y: restantes },
                        { x: xFin, y: 0 },
                      ],
                      borderColor: 'rgba(34,197,94,0.55)',
                      borderDash: [2, 4],
                      borderWidth: 2,
                      pointRadius: 0,
                      pointHoverRadius: 0,
                      fill: false,
                    },
                  ]
                : []),
            ],
          }}
          plugins={[lineaHoy(hayProyeccion ? idxHoy : null)]}
          options={{
            maintainAspectRatio: false,
            parsing: false,
            layout: { padding: { top: 20, right: 8 } },
            interaction: { mode: 'nearest', axis: 'x', intersect: false },
            plugins: {
              legend: { display: false },
              tooltip: {
                ...tooltip,
                filter: (item) => item.dataset.label !== 'Proyección',
                displayColors: false,
                callbacks: { title: tituloTooltip, label: (item) => `Restantes: ${item.parsed.y} tareas` },
              },
            },
            scales: ejes(xFin),
          }}
        />
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <Leyenda
          items={[
            { nombre: 'Restantes', estilo: { background: VERDE, height: 3 } },
            ...(hayProyeccion
              ? [{ nombre: 'Proyección', estilo: { backgroundImage: 'repeating-linear-gradient(90deg, rgba(34,197,94,0.75) 0 2px, transparent 2px 6px)', height: 2 } }]
              : []),
          ]}
        />
        <p className={`mt-3 flex items-center gap-1.5 text-xs ${crece ? 'text-orange-300' : 'text-green-300'}`}>
          <IconoEnLinea nombre={crece ? 'alerta' : 'check'} sinColor />
          {crece ? 'Entran más tareas de las que se terminan: el backlog crece.' : 'Se terminan más tareas de las que entran: el backlog baja.'}
        </p>
      </div>
    </section>
  );
}
