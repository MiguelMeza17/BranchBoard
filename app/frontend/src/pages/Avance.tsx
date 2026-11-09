import { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import type { Sprint } from '../types';
import { estaPlanificada, perteneceA } from '../lib/planificacion';
import { PageHeader, Card, EmptyState } from '../components/primitives';
import { BurndownChart } from '../components/BurndownChart';
import { BacklogChart } from '../components/BacklogChart';

// "General" no es un backlog guardado: es el avance de todo el proyecto (la suma de las tareas de todos
// los sprints y backlogs). Como no tiene fecha de fin se grafica como un backlog. Va en su propio botón.
const ID_GENERAL = 'general';

export function Avance() {
  const user = useStore((s) => s.currentUser());
  const sprints = useStore((s) => s.sprints);
  const tareas = useStore((s) => s.tareas);
  // Al entrar se muestra "General".
  const [sprintId, setSprintId] = useState(ID_GENERAL);
  const esGeneral = sprintId === ID_GENERAL;

  const general: Sprint = useMemo(
    () => ({
      id: ID_GENERAL,
      nombre: 'General',
      descripcion: '',
      con_fecha: false,
      fecha_inicio: sprints.reduce((min, sp) => (sp.fecha_inicio < min ? sp.fecha_inicio : min), '9999-12-31'),
      duracion_dias: null,
      fecha_fin: null,
      cerrado: false,
      fecha_cierre_real: null,
      creado_por: '',
    }),
    [sprints]
  );
  const sprint = esGeneral ? general : sprints.find((sp) => sp.id === sprintId);
  const delSprint = useMemo(
    () =>
      tareas.filter(
        (t) =>
          (esGeneral ? estaPlanificada(t) : !!sprint && perteneceA(t, sprint)) &&
          !t.eliminada &&
          tareaVisibleEnTableroPrincipal(t, user)
      ),
    [tareas, sprint, esGeneral, user]
  );

  return (
    <div>
      <PageHeader
        title="Avance"
        desc="¿Vamos bien o mal respecto al tiempo? Curva de tareas restantes del sprint o del backlog."
        actions={
          sprints.length > 0 ? (
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => setSprintId(ID_GENERAL)}
                aria-pressed={esGeneral}
                className={`h-[38px] rounded-lg border px-3.5 text-sm font-medium transition ${
                  esGeneral
                    ? 'border-primary bg-primary-soft text-text'
                    : 'border-border bg-surface-2 text-muted hover:border-border-strong hover:text-text'
                }`}
              >
                General
              </button>
            <select
              className={`input max-w-[240px] ${esGeneral ? 'text-muted' : ''}`}
              value={esGeneral ? '' : sprintId}
              onChange={(e) => setSprintId(e.target.value)}
            >
              {esGeneral && (
                <option value="" disabled hidden>
                  Sprint o backlog
                </option>
              )}
              {[
                { titulo: 'Sprints', lista: sprints.filter((sp) => sp.con_fecha) },
                { titulo: 'Backlogs', lista: sprints.filter((sp) => !sp.con_fecha) },
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
            // Un sprint cerrado se ve como quedó al cierre (la copia de su reporte).
            sprint.cerrado && sprint.reporte ? (
              <BurndownChart sprint={sprint} tareas={sprint.reporte} contexto="reporte" />
            ) : (
              <BurndownChart sprint={sprint} tareas={delSprint} />
            )
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
