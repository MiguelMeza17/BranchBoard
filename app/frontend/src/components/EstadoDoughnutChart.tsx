import { Doughnut } from 'react-chartjs-2';
import '../lib/charts';
import { ESTADOS } from '../types';
import type { Tarea } from '../types';
import { estadoMeta } from '../lib/ui';

export function EstadoDoughnutChart({ tareas }: { tareas: Tarea[] }) {
  const porEstado = ESTADOS.map((e) => tareas.filter((t) => t.estado === e).length);
  const total = tareas.length;

  const data = {
    labels: ESTADOS.map((e) => estadoMeta[e].label),
    datasets: [
      {
        data: porEstado,
        backgroundColor: ESTADOS.map((e) => estadoMeta[e].dot),
        borderColor: '#161922',
        borderWidth: 3,
        hoverOffset: 6,
      },
    ],
  };

  return (
    <div className="relative mx-auto" style={{ maxWidth: 320 }}>
      <Doughnut
        data={data}
        options={{
          cutout: '68%',
          plugins: {
            legend: { position: 'bottom', labels: { padding: 16, usePointStyle: true, pointStyle: 'circle' } },
          },
        }}
      />
      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center pb-10">
        <span className="text-3xl font-bold text-text">{total}</span>
        <span className="text-xs text-muted">tareas</span>
      </div>
    </div>
  );
}
