import { Line } from 'react-chartjs-2';
import '../lib/charts';
import type { Sprint, Tarea } from '../types';
import { IconoEnLinea } from './Icono';

const MAX_PROYECCION_DIAS = 90;

function listarDias(inicio: string, fin: string): string[] {
  const dias: string[] = [];
  const cur = new Date(`${inicio}T00:00:00`);
  const end = new Date(`${fin}T00:00:00`);
  while (cur <= end) {
    dias.push(cur.toISOString().slice(0, 10));
    cur.setDate(cur.getDate() + 1);
  }
  return dias;
}

function sumarDias(dia: string, n: number): string {
  const d = new Date(`${dia}T00:00:00`);
  d.setDate(d.getDate() + n);
  return d.toISOString().slice(0, 10);
}

interface Serie {
  dias: string[];
  burndown: number[];
  ideal: number[];
  diasExtendidos: string[];
  tendencia: (number | null)[] | null;
  xZero: number | null;
  nPuntos: number;
  avance: number;
}

function calcularSerie(sprint: Sprint, tareas: Tarea[]): Serie {
  const dias = sprint.fecha_fin ? listarDias(sprint.fecha_inicio, sprint.fecha_fin) : [];
  const total = tareas.length;

  const burndown = dias.map(
    (dia) => total - tareas.filter((t) => t.fecha_completada && t.fecha_completada <= dia).length
  );

  const ideal = dias.map((_, i) => (dias.length > 1 ? total - (total * i) / (dias.length - 1) : total));

  // Regresión lineal (mínimos cuadrados) sobre los días ya transcurridos, para proyectar
  // en qué día llegaría a 0 al ritmo actual.
  const hoy = new Date().toISOString().slice(0, 10);
  const nPuntos = dias.filter((d) => d <= hoy).length;

  let regresion: { m: number; b: number } | null = null;
  if (nPuntos >= 2) {
    const xs = Array.from({ length: nPuntos }, (_, i) => i);
    const ys = burndown.slice(0, nPuntos);
    const sumX = xs.reduce((a, x) => a + x, 0);
    const sumY = ys.reduce((a, y) => a + y, 0);
    const sumXY = xs.reduce((acc, x, i) => acc + x * ys[i], 0);
    const sumX2 = xs.reduce((acc, x) => acc + x * x, 0);
    const denom = nPuntos * sumX2 - sumX * sumX;
    if (denom !== 0) {
      const m = (nPuntos * sumXY - sumX * sumY) / denom;
      const b = (sumY - m * sumX) / nPuntos;
      regresion = { m, b };
    }
  }

  const xZero = regresion && regresion.m < 0 ? -regresion.b / regresion.m : null;

  // Si la recta cruza y=0 después del último día, extendemos el eje hasta ahí.
  let diasExtendidos = dias;
  if (xZero !== null && dias.length > 0) {
    const extra = Math.min(MAX_PROYECCION_DIAS, Math.max(0, Math.ceil(xZero) - (dias.length - 1)));
    if (extra > 0) {
      diasExtendidos = [...dias];
      for (let i = 1; i <= extra; i++) diasExtendidos.push(sumarDias(dias[dias.length - 1], i));
    }
  }

  // Solo dos puntos (inicio y el cruce en y=0) para que Chart.js los una con una recta uniforme.
  let tendencia: (number | null)[] | null = null;
  if (regresion && xZero !== null) {
    const idxZero = Math.min(diasExtendidos.length - 1, Math.round(xZero));
    const arr: (number | null)[] = new Array(diasExtendidos.length).fill(null);
    arr[0] = regresion.b;
    arr[idxZero] = 0;
    tendencia = arr;
  }

  // % de tareas completadas hasta el último día real transcurrido — para comparar el ritmo entre sprints.
  const avance = total > 0 && nPuntos > 0 ? (total - burndown[nPuntos - 1]) / total : 0;

  return { dias, burndown, ideal, diasExtendidos, tendencia, xZero, nPuntos, avance };
}

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
  const principal = calcularSerie(sprint, tareas);
  const comparacion = comparar ? calcularSerie(comparar.sprint, comparar.tareas) : null;

  if (principal.dias.length === 0) {
    return (
      <>
        <h3 className="mb-2 text-sm font-semibold text-text">Burndown del sprint</h3>
        <p className="text-sm text-muted">Este sprint no tiene fechas — no se puede graficar el burndown.</p>
      </>
    );
  }

  const ultimoIndiceSprint = principal.dias.length - 1;
  let veredicto: { texto: string; nivel: 'bien' | 'mejora' | 'mal' } | null = null;
  if (principal.xZero !== null) {
    const fechaProyectada =
      principal.diasExtendidos[Math.min(principal.diasExtendidos.length - 1, Math.round(principal.xZero))];
    const aTiempo = principal.xZero <= ultimoIndiceSprint;
    veredicto =
      contexto === 'reporte'
        ? {
            nivel: aTiempo ? 'bien' : 'mal',
            texto: aTiempo
              ? `En ${sprint.nombre}, el progreso fue adecuado — las tareas van al día (proyección de cierre: ${fechaProyectada}, dentro del plazo).`
              : `En ${sprint.nombre}, el progreso fue bajo y las tareas siguen pendientes — al ritmo mostrado se proyectan terminar hasta el ${fechaProyectada}, después de la fecha límite (${sprint.fecha_fin}).`,
          }
        : {
            nivel: aTiempo ? 'bien' : 'mal',
            texto: aTiempo
              ? `Al ritmo actual, las tareas se terminarían el ${fechaProyectada} — dentro del sprint.`
              : `Al ritmo actual, las tareas se terminarían hasta el ${fechaProyectada} — después de la fecha límite (${sprint.fecha_fin}). Toca aumentar el rendimiento.`,
          };
  } else if (principal.nPuntos >= 2) {
    veredicto =
      contexto === 'reporte'
        ? { nivel: 'mal', texto: `En ${sprint.nombre}, el progreso fue nulo — las tareas siguen pendientes sin avance.` }
        : { nivel: 'mal', texto: 'Con el ritmo actual no hay progreso neto — no se proyecta terminar las tareas.' };
  }

  // Si no fue bien pero mejoró bastante respecto al sprint comparado, no lo marcamos igual de mal (naranja, no rojo).
  if (veredicto && veredicto.nivel === 'mal' && comparacion && principal.avance - comparacion.avance >= 0.15) {
    const pctAntes = Math.round(comparacion.avance * 100);
    const pctAhora = Math.round(principal.avance * 100);
    veredicto = {
      nivel: 'mejora',
      texto:
        contexto === 'reporte'
          ? `En ${sprint.nombre}, el progreso no estuvo tan mal — mejoró bastante respecto a ${comparar!.sprint.nombre} (de ${pctAntes}% a ${pctAhora}% completado), aunque todavía no alcanza para terminar a tiempo.`
          : `Vas mejor que en el sprint anterior (de ${pctAntes}% a ${pctAhora}% completado), pero aún toca acelerar para terminar a tiempo.`,
    };
  }

  // Eje X en "día del sprint" (relativo) para poder superponer sprints de fechas distintas.
  const maxDias = Math.max(principal.diasExtendidos.length, comparacion?.diasExtendidos.length ?? 0);
  const labels = Array.from({ length: maxDias }, (_, i) => `Día ${i + 1}`);

  const data = {
    labels,
    datasets: [
      {
        label: 'Tareas restantes',
        data: principal.burndown,
        borderColor: '#22c55e',
        backgroundColor: 'rgba(34,197,94,0.12)',
        fill: true,
        tension: 0.6,
        pointRadius: 3,
        pointBackgroundColor: '#22c55e',
      },
      {
        label: 'Ideal',
        data: principal.ideal,
        borderColor: '#6b7385',
        borderDash: [6, 6],
        fill: false,
        tension: 0,
        pointRadius: 0,
      },
      ...(principal.tendencia
        ? [
            {
              label: 'Tendencia (regresión lineal)',
              data: principal.tendencia,
              spanGaps: true,
              borderColor: '#f59e0b',
              borderDash: [2, 3],
              fill: false,
              tension: 0,
              pointRadius: 0,
              borderWidth: 2,
            },
          ]
        : []),
      ...(comparacion
        ? [
            {
              label: `Tareas restantes (${comparar!.sprint.nombre})`,
              data: comparacion.burndown,
              borderColor: 'rgba(154,162,177,0.9)',
              backgroundColor: 'transparent',
              fill: false,
              tension: 0.6,
              pointRadius: 0,
            },
            {
              label: `Ideal (${comparar!.sprint.nombre})`,
              data: comparacion.ideal,
              borderColor: 'rgba(154,162,177,0.5)',
              borderDash: [6, 6],
              fill: false,
              tension: 0,
              pointRadius: 0,
            },
            ...(comparacion.tendencia
              ? [
                  {
                    label: `Tendencia (${comparar!.sprint.nombre})`,
                    data: comparacion.tendencia,
                    spanGaps: true,
                    borderColor: 'rgba(154,162,177,0.5)',
                    borderDash: [2, 3],
                    fill: false,
                    tension: 0,
                    pointRadius: 0,
                  },
                ]
              : []),
          ]
        : []),
    ],
  };

  return (
    <>
      <h3 className="mb-4 text-sm font-semibold text-text">Burndown del sprint</h3>
      <div style={{ height: 380 }}>
        <Line
          data={data}
          options={{
            maintainAspectRatio: false,
            interaction: { mode: 'index', intersect: false },
            plugins: {
              legend: { position: 'bottom', labels: { usePointStyle: true, pointStyle: 'circle', padding: 16 } },
            },
            scales: {
              y: {
                beginAtZero: true,
                title: { display: true, text: 'Tareas restantes' },
                grid: { color: 'rgba(43,49,63,0.5)' },
              },
              x: { grid: { display: false } },
            },
          }}
        />
      </div>
      {veredicto && (
        <div
          className={`mt-4 rounded-lg border px-3 py-2 text-sm ${
            veredicto.nivel === 'bien'
              ? 'border-green-500/40 bg-green-500/10 text-green-300'
              : veredicto.nivel === 'mejora'
                ? 'border-orange-500/40 bg-orange-500/10 text-orange-300'
                : 'border-red-500/40 bg-red-500/10 text-red-300'
          }`}
        >
          <IconoEnLinea nombre={veredicto.nivel === 'bien' ? 'check' : veredicto.nivel === 'mejora' ? 'avance' : 'alerta'} sinColor />
          {veredicto.texto}
        </div>
      )}
    </>
  );
}
