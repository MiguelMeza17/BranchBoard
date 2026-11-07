import { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { PageHeader, Card, EmptyState } from '../components/primitives';
import { BurndownChart } from '../components/BurndownChart';
import { BacklogChart } from '../components/BacklogChart';

// El backlog "General" tiene su propio botón junto al selector (no va dentro de la lista).
const ID_GENERAL = 'general';

export function Avance() {
  const user = useStore((s) => s.currentUser());
  const sprints = useStore((s) => s.sprints);
  const tareas = useStore((s) => s.tareas);
  const [sprintId, setSprintId] = useState(sprints[0]?.id ?? '');

  const sprint = sprints.find((sp) => sp.id === sprintId);
  const delSprint = useMemo(
    () =>
      tareas.filter(
        (t) => t.sprint_id === sprintId && !t.eliminada && tareaVisibleEnTableroPrincipal(t, user)
      ),
    [tareas, sprintId, user]
  );

  return (
    <div>
      <PageHeader
        title="Avance"
        desc="¿Vamos bien o mal respecto al tiempo? Curva de tareas restantes del sprint o del backlog."
        actions={
          sprints.length > 0 ? (
            <div className="flex items-center gap-2">
            {sprints.some((sp) => sp.id === ID_GENERAL) && (
              <button
                type="button"
                onClick={() => setSprintId(ID_GENERAL)}
                aria-pressed={sprintId === ID_GENERAL}
                className={`h-[38px] rounded-lg border px-3.5 text-sm font-medium transition ${
                  sprintId === ID_GENERAL
                    ? 'border-primary bg-primary-soft text-text'
                    : 'border-border bg-surface-2 text-muted hover:border-border-strong hover:text-text'
                }`}
              >
                General
              </button>
            )}
            <select
              className={`input max-w-[240px] ${sprintId === ID_GENERAL ? 'text-muted' : ''}`}
              value={sprintId === ID_GENERAL ? '' : sprintId}
              onChange={(e) => setSprintId(e.target.value)}
            >
              {sprintId === ID_GENERAL && (
                <option value="" disabled hidden>
                  Sprint o backlog
                </option>
              )}
              {[
                { titulo: 'Sprints', lista: sprints.filter((sp) => sp.con_fecha) },
                { titulo: 'Backlogs', lista: sprints.filter((sp) => !sp.con_fecha && sp.id !== ID_GENERAL) },
              ]
                .filter((g) => g.lista.length > 0)
                .map((g) => (
                  <optgroup key={g.titulo} label={g.titulo}>
                    {g.lista.map((sp) => (
                      <option key={sp.id} value={sp.id}>{sp.nombre}</option>
                    ))}
                  </optgroup>
                ))}
            </select>
            </div>
          ) : undefined
        }
      />

      {sprint ? (
        <Card>
          {sprint.con_fecha ? (
            <BurndownChart sprint={sprint} tareas={delSprint} />
          ) : (
            <BacklogChart backlog={sprint} tareas={delSprint} />
          )}
        </Card>
      ) : (
        <EmptyState icon="avance" title="Todavía no hay sprints" desc="Crea un sprint para ver aquí su curva de avance." />
      )}
    </div>
  );
}
