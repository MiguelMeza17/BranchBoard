import { useMemo } from 'react';
import { useStore } from '../store/useStore';
import { TIPOS_TAREA } from '../types';
import { estadoMeta, tipoMeta } from '../lib/ui';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { PageHeader, Card } from '../components/primitives';
import { EstadoDoughnutChart } from '../components/EstadoDoughnutChart';
import { Icono } from '../components/Icono';

export function Resumen() {
  const user = useStore((s) => s.currentUser());
  const tareasAll = useStore((s) => s.tareas);

  const tareas = useMemo(
    () => tareasAll.filter((t) => !t.eliminada && tareaVisibleEnTableroPrincipal(t, user)),
    [tareasAll, user]
  );

  const total = tareas.length;
  const porTipo = TIPOS_TAREA.map((tp) => tareas.filter((t) => t.tipo === tp).length);

  const pendientes = tareas.filter((t) => t.pendiente_aprobacion).length;
  const enRevision = tareas.filter((t) => t.estado === 'en revisión').length;
  const conFecha = tareas.filter((t) => t.con_fecha).length;

  return (
    <div>
      <PageHeader
        title="Resumen"
        desc="Vista general del estado de las tareas del proyecto."
      />

      <div className="card p-3 sm:p-5">
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          <StatTile label="Total de tareas" value={total} tone="#6366f1" />
          <StatTile label="En revisión" value={enRevision} tone={estadoMeta['en revisión'].dot} />
          <StatTile label="Pendientes de aprobación" value={pendientes} tone="#f59e0b" />
          <StatTile label="Con fecha límite" value={conFecha} tone="#2dd4bf" />
        </div>

        <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,420px)_1fr]">
          <Card>
            <h3 className="mb-4 text-sm font-semibold text-text">Tareas por estado</h3>
            <EstadoDoughnutChart tareas={tareas} />
          </Card>

          <Card>
            <h3 className="mb-4 text-sm font-semibold text-text">Distribución por tipo</h3>
            <div className="space-y-3">
              {TIPOS_TAREA.map((tp, i) => {
                const n = porTipo[i];
                const pct = total ? Math.round((n / total) * 100) : 0;
                return (
                  <div key={tp} className="flex items-center gap-3">
                    <span className="flex w-28 shrink-0 items-center gap-1.5 text-xs text-muted">
                      <Icono nombre={tipoMeta[tp].icono} className="h-3.5 w-3.5" color={tipoMeta[tp].color} /> {tp}
                    </span>
                    <div className="h-2.5 flex-1 overflow-hidden rounded-full bg-surface-3">
                      <div
                        className="h-full rounded-full transition-all"
                        style={{ width: `${pct}%`, background: tipoMeta[tp].color }}
                      />
                    </div>
                    <span className="w-8 text-right font-mono text-xs text-faint">{n}</span>
                  </div>
                );
              })}
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}

function StatTile({ label, value, tone }: { label: string; value: number; tone: string }) {
  return (
    <Card className="relative overflow-hidden">
      <span className="absolute left-0 top-0 h-full w-1" style={{ background: tone }} />
      <div className="text-3xl font-bold text-text">{value}</div>
      <div className="mt-1 text-xs text-muted">{label}</div>
    </Card>
  );
}
