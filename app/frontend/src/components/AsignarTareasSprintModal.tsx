import { useMemo, useState } from 'react';
import { Modal } from './Modal';
import { TipoBadge, EstadoBadge } from './primitives';
import { useStore } from '../store/useStore';
import type { Sprint } from '../types';

// "Asignar tareas" de un sprint o backlog (Planificación): todas las tareas del proyecto con un
// buscador; marcar una la mete en este sprint, desmarcarla la deja sin sprint. Si ya estaba en otro
// sprint, se avisa en la fila y al marcarla se mueve a este. Los cambios se aplican al momento.
export function AsignarTareasSprintModal({ sprint, onClose }: { sprint: Sprint | null; onClose: () => void }) {
  const tareas = useStore((s) => s.tareas);
  const sprints = useStore((s) => s.sprints);
  const asignar = useStore((s) => s.asignarTareaASprint);
  const [busqueda, setBusqueda] = useState('');
  const [soloSinSprint, setSoloSinSprint] = useState(false);

  const lista = useMemo(() => {
    const q = busqueda.trim().toLowerCase();
    return tareas
      .filter((t) => !t.eliminada)
      .filter((t) => (q ? `${t.id} ${t.titulo}`.toLowerCase().includes(q) : true))
      .filter((t) => (soloSinSprint ? !t.sprint_id || t.sprint_id === sprint?.id : true))
      .sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true }));
  }, [tareas, busqueda, soloSinSprint, sprint?.id]);

  if (!sprint) return null;
  const enEste = tareas.filter((t) => t.sprint_id === sprint.id && !t.eliminada).length;

  return (
    <Modal
      open
      onClose={onClose}
      title={`Asignar tareas a ${sprint.nombre}`}
      subtitle={`${enEste} ${enEste === 1 ? 'tarea' : 'tareas'} en ${sprint.con_fecha ? 'este sprint' : 'este backlog'}`}
      size="lg"
      footer={
        <div className="flex w-full justify-end">
          <button type="button" className="btn-primary btn-sm" onClick={onClose}>
            Listo
          </button>
        </div>
      }
    >
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <input
          className="input max-w-xs"
          placeholder="Buscar por título o ID…"
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
          autoFocus
        />
        <label className="flex cursor-pointer items-center gap-2 text-sm text-muted">
          <input
            type="checkbox"
            className="accent-primary"
            checked={soloSinSprint}
            onChange={(e) => setSoloSinSprint(e.target.checked)}
          />
          Solo las que no están en otro sprint
        </label>
      </div>
      {lista.length === 0 ? (
        <p className="py-6 text-center text-sm text-faint">No hay tareas que coincidan.</p>
      ) : (
        <div className="max-h-[420px] space-y-1 overflow-y-auto">
          {lista.map((t) => {
            const marcada = t.sprint_id === sprint.id;
            const otro = !marcada && t.sprint_id ? sprints.find((s) => s.id === t.sprint_id) : undefined;
            return (
              <label
                key={t.id}
                className={`flex cursor-pointer items-center gap-3 rounded-lg border px-3 py-2 text-sm transition ${
                  marcada ? 'border-primary/50 bg-primary-soft' : 'border-border bg-surface-2 hover:border-border-strong'
                }`}
              >
                <input
                  type="checkbox"
                  className="accent-primary"
                  checked={marcada}
                  onChange={(e) => asignar(t.id, e.target.checked ? sprint.id : null)}
                />
                <span className="w-16 shrink-0 font-mono text-[11px] text-faint">{t.id}</span>
                <span className="min-w-0 flex-1 truncate text-text">{t.titulo}</span>
                {otro && <span className="shrink-0 text-[11px] text-amber-300">en {otro.nombre}</span>}
                <TipoBadge tipo={t.tipo} />
                <EstadoBadge estado={t.estado} />
              </label>
            );
          })}
        </div>
      )}
    </Modal>
  );
}
