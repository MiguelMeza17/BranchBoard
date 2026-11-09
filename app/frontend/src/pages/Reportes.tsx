import { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { PageHeader, Card, EmptyState, Avatar, EstadoBadge } from '../components/primitives';
import { BurndownChart } from '../components/BurndownChart';
import { EstadoDoughnutChart } from '../components/EstadoDoughnutChart';

export function Reportes() {
  const sprints = useStore((s) => s.sprints);
  const tareas = useStore((s) => s.tareas);
  const usuarios = useStore((s) => s.usuarios);

  // Solo sprints que ya tienen reporte: cerrados, o que ya pasaron su fecha de fin (el reporte se genera ese
  // día aunque sigan abiertos). Los en curso o por empezar no salen. El más reciente primero.
  const hoy = new Date().toISOString().slice(0, 10);
  const conFecha = sprints
    .filter((s) => s.con_fecha && (s.cerrado || (s.fecha_fin !== null && s.fecha_fin < hoy)))
    .sort((a, b) => b.fecha_inicio.localeCompare(a.fecha_inicio));
  const [sel, setSel] = useState(conFecha[0]?.id ?? '');
  const sprint = conFecha.find((s) => s.id === sel);

  // Las tareas tal como quedaron al cierre (copia guardada); si no hay copia, las de ahora.
  const delSprint = useMemo(
    () => sprint?.reporte ?? tareas.filter((t) => t.sprint_id === sel && !t.eliminada),
    [tareas, sel, sprint]
  );
  const hechas = delSprint.filter((t) => t.estado === 'terminada');
  const enRevision = delSprint.filter((t) => t.estado === 'en revisión');
  const faltaron = delSprint.filter((t) => t.estado === 'por hacer' || t.estado === 'en producción');

  const porPersona = useMemo(() => {
    const map = new Map<string, number>();
    delSprint.forEach((t) => {
      if (t.asignado_a) map.set(t.asignado_a, (map.get(t.asignado_a) ?? 0) + 1);
    });
    return [...map.entries()].sort((a, b) => b[1] - a[1]);
  }, [delSprint]);

  // Se compara con el reporte anterior (el sprint previo con reporte), en gris.
  const sprintAnterior = sprint ? conFecha[conFecha.indexOf(sprint) + 1] : undefined;
  const tareasSprintAnterior = useMemo(
    () => (sprintAnterior ? sprintAnterior.reporte ?? tareas.filter((t) => t.sprint_id === sprintAnterior.id && !t.eliminada) : []),
    [tareas, sprintAnterior]
  );

  return (
    <div>
      <PageHeader
        title="Reportes"
        desc="Reporte automático de cierre de sprint (solo sprints con fecha). Visible para todos los roles."
        actions={
          conFecha.length > 0 ? (
            <select className="input max-w-[260px]" value={sel} onChange={(e) => setSel(e.target.value)}>
              {conFecha.map((s) => (
                <option key={s.id} value={s.id}>{s.nombre}</option>
              ))}
            </select>
          ) : undefined
        }
      />

      {!sprint ? (
        <EmptyState icon="reportes" title="Todavía no hay reportes" desc="Un sprint genera su reporte al cerrarse o al llegar su fecha de fin." />
      ) : (
        <Card>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 className="text-lg font-semibold text-text">{sprint.nombre}</h2>
              <p className="text-sm text-muted">
                {sprint.fecha_inicio} → {sprint.fecha_fin} · {sprint.cerrado ? `Cerrado el ${sprint.fecha_cierre_real}` : 'Reporte de la fecha de fin (sigue abierto)'}
              </p>
              {sprint.descripcion && <p className="mt-1.5 text-sm text-text/80">{sprint.descripcion}</p>}
            </div>
          </div>

          <div className="mt-6 grid gap-4 sm:grid-cols-3">
            <MiniStat label="Tareas hechas" value={hechas.length} tone="#22c55e" />
            <MiniStat label="Se quedaron en revisión" value={enRevision.length} tone="#a855f7" />
            <MiniStat label="No llegaron a terminar" value={faltaron.length} tone="#f59e0b" />
          </div>

          <div className="mt-6 border-t border-border pt-6">
            <BurndownChart
              sprint={sprint}
              tareas={delSprint}
              contexto="reporte"
              comparar={sprintAnterior ? { sprint: sprintAnterior, tareas: tareasSprintAnterior } : undefined}
            />
          </div>

          <div className="mt-6 border-t border-border pt-6">
            <h3 className="mb-4 text-sm font-semibold text-text">Tareas por estado</h3>
            <EstadoDoughnutChart tareas={delSprint} />
          </div>

          <div className="mt-6 border-t border-border pt-6">
            <h3 className="mb-3 text-sm font-semibold text-text">Análisis de tareas del sprint</h3>
            <div className="acotado space-y-2 pr-1">
              {delSprint.map((t) => (
                <div key={t.id} className="flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-surface-2 px-3 py-2">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-[11px] text-faint">{t.id}</span>
                    <span className="text-sm text-text">{t.titulo}</span>
                  </div>
                  <EstadoBadge estado={t.estado} />
                </div>
              ))}
              {delSprint.length === 0 && <p className="text-sm text-muted">Sin tareas asociadas a este sprint.</p>}
            </div>
          </div>

          <div className="mt-6 border-t border-border pt-6">
            <h3 className="mb-3 text-sm font-semibold text-text">Desglose de actividad por persona</h3>
            <div className="acotado space-y-2 pr-1">
              {porPersona.map(([email, n]) => {
                const u = usuarios.find((x) => x.email === email);
                return (
                  <div key={email} className="flex items-center gap-3">
                    <Avatar email={email} size={28} />
                    <span className="flex-1 text-sm text-text">
                      {u ? `${u.nombre} ${u.apellido}` : email}
                      {u && <span className="ml-2 text-xs text-faint">{u.rol}</span>}
                    </span>
                    <span className="chip border border-border bg-surface-2 text-muted">{n} tarea{n === 1 ? '' : 's'}</span>
                  </div>
                );
              })}
              {porPersona.length === 0 && <p className="text-sm text-muted">Nadie tomó tareas de este sprint todavía.</p>}
            </div>
          </div>
        </Card>
      )}
    </div>
  );
}

function MiniStat({ label, value, tone }: { label: string; value: number; tone: string }) {
  return (
    <Card className="relative overflow-hidden">
      <span className="absolute left-0 top-0 h-full w-1" style={{ background: tone }} />
      <div className="text-2xl font-bold text-text">{value}</div>
      <div className="mt-0.5 text-xs text-muted">{label}</div>
    </Card>
  );
}
