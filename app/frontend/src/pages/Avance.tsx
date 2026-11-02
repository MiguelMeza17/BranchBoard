import { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { PageHeader, Card } from '../components/primitives';
import { BurndownChart } from '../components/BurndownChart';

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
        desc="¿Vamos bien o mal respecto al tiempo? Curva de tareas restantes del sprint."
        actions={
          <select className="input max-w-[240px]" value={sprintId} onChange={(e) => setSprintId(e.target.value)}>
            {sprints.map((sp) => (
              <option key={sp.id} value={sp.id}>{sp.nombre}</option>
            ))}
          </select>
        }
      />

      <Card>{sprint && <BurndownChart sprint={sprint} tareas={delSprint} />}</Card>
    </div>
  );
}
