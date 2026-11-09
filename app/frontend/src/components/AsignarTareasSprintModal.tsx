import { useMemo, useState } from 'react';
import { Modal } from './Modal';
import { TipoBadge, EstadoBadge } from './primitives';
import { useStore } from '../store/useStore';
import type { Sprint, Tarea } from '../types';

// "Asignar tareas" (Planificación). Funciona distinto según sea un sprint o un backlog:
// - Sprint: las tareas agrupadas por su backlog (y "Sin backlog"). Marcar una la mete en el sprint sin
//   sacarla de su backlog; cada grupo (también "Sin backlog") tiene "Agregar todo": sus tareas que no están
//   en otro sprint. Una tarea ocupada por otro sprint sale en gris con "en Sprint X" y no se puede marcar.
// - Backlog: todas las tareas; marcar una la pone en este backlog. Una tarea de otro backlog sale en gris
//   con "en Etapa X" y no se puede marcar (un backlog no tiene tareas de otro). Las que están en un sprint
//   muestran en cuál.
// Los cambios se aplican al momento.
export function AsignarTareasSprintModal({ sprint, onClose }: { sprint: Sprint | null; onClose: () => void }) {
  const tareas = useStore((s) => s.tareas);
  const sprints = useStore((s) => s.sprints);
  const asignarSprint = useStore((s) => s.asignarTareaASprint);
  const asignarBacklog = useStore((s) => s.asignarTareaABacklog);
  const asignarBacklogEntero = useStore((s) => s.asignarBacklogASprint);
  const [busqueda, setBusqueda] = useState('');

  const esSprint = !!sprint?.con_fecha;
  const nombreDe = (id: string | null) => sprints.find((s) => s.id === id)?.nombre;

  const lista = useMemo(() => {
    if (!sprint) return [];
    const q = busqueda.trim().toLowerCase();
    return tareas
      .filter((t) => !t.eliminada)
      .filter((t) => (q ? `${t.id} ${t.titulo}`.toLowerCase().includes(q) : true))
      .sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true }));
  }, [tareas, busqueda, sprint]);

  if (!sprint) return null;
  const enEste = tareas.filter((t) => !t.eliminada && (esSprint ? t.sprint_id === sprint.id : t.backlog_id === sprint.id)).length;

  // En un sprint: grupos por backlog (los terminados al final) y después "Sin backlog".
  const grupos: { id: string | null; nombre: string; tareas: Tarea[] }[] = esSprint
    ? [
        ...sprints
          .filter((b) => !b.con_fecha)
          .map((b) => ({ id: b.id as string | null, nombre: b.nombre, tareas: lista.filter((t) => t.backlog_id === b.id) }))
          .sort((a, b) => Number(a.tareas.every((t) => t.estado === 'terminada')) - Number(b.tareas.every((t) => t.estado === 'terminada'))),
        { id: null, nombre: 'Sin backlog', tareas: lista.filter((t) => !t.backlog_id) },
      ].filter((g) => g.tareas.length > 0)
    : [{ id: null, nombre: '', tareas: lista }];

  const fila = (t: Tarea) => {
    const marcada = esSprint ? t.sprint_id === sprint.id : t.backlog_id === sprint.id;
    const otroSprint = t.sprint_id && t.sprint_id !== sprint.id ? nombreDe(t.sprint_id) : undefined;
    const otroBacklog = !esSprint && t.backlog_id && t.backlog_id !== sprint.id ? nombreDe(t.backlog_id) : undefined;
    // Una tarea de otro sprint (en un sprint) o de otro backlog (en un backlog) no se puede marcar.
    const ocupada = esSprint ? !!otroSprint : !!otroBacklog;
    return (
      <label
        key={t.id}
        className={`flex items-center gap-3 rounded-lg border px-3 py-2 text-sm transition ${
          ocupada
            ? 'cursor-not-allowed border-border/60 bg-surface-2/50 opacity-50'
            : marcada
              ? 'cursor-pointer border-primary/50 bg-primary-soft'
              : 'cursor-pointer border-border bg-surface-2 hover:border-border-strong'
        }`}
      >
        <input
          type="checkbox"
          className="accent-primary"
          checked={marcada}
          disabled={ocupada}
          onChange={(e) =>
            esSprint ? asignarSprint(t.id, e.target.checked ? sprint.id : null) : asignarBacklog(t.id, e.target.checked ? sprint.id : null)
          }
        />
        <span className="w-16 shrink-0 font-mono text-[11px] text-faint">{t.id}</span>
        <span className="min-w-0 flex-1 truncate text-text">{t.titulo}</span>
        {otroBacklog && <span className="chip shrink-0 border border-border bg-surface-3 text-[11px] text-muted">en {otroBacklog}</span>}
        {otroSprint && <span className="chip shrink-0 border border-border bg-surface-3 text-[11px] text-muted">en {otroSprint}</span>}
        <TipoBadge tipo={t.tipo} />
        <EstadoBadge estado={t.estado} />
      </label>
    );
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={`Asignar tareas a ${sprint.nombre}`}
      subtitle={`${enEste} ${enEste === 1 ? 'tarea' : 'tareas'} en ${esSprint ? 'este sprint' : 'este backlog'}`}
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
      </div>
      {lista.length === 0 ? (
        <p className="py-6 text-center text-sm text-faint">No hay tareas que coincidan.</p>
      ) : (
        <div className="acotado space-y-4">
          {grupos.map((g) => {
            // Lo que "Agregar todo" puede tomar: las del grupo que no están en ningún sprint.
            const libres = g.tareas.filter((t) => !t.sprint_id).length;
            const yaEnEste = g.tareas.length > 0 && g.tareas.every((t) => t.sprint_id === sprint.id);
            return (
              <section key={g.id ?? 'sin'}>
                {esSprint && (
                  <div className="mb-1.5 flex items-center justify-between gap-3 px-1">
                    <span className="text-[11px] font-semibold uppercase tracking-wide text-faint">
                      {g.nombre} · {g.tareas.length}
                    </span>
                    <button
                      type="button"
                      className="text-xs font-medium text-primary hover:underline disabled:cursor-not-allowed disabled:text-faint disabled:no-underline"
                      disabled={libres === 0}
                      onClick={() => asignarBacklogEntero(g.id, sprint.id)}
                    >
                      {libres > 0 ? `Agregar todo (${libres})` : yaEnEste ? 'Todo agregado' : 'Agregar todo'}
                    </button>
                  </div>
                )}
                <div className="space-y-1">{g.tareas.map(fila)}</div>
              </section>
            );
          })}
        </div>
      )}
    </Modal>
  );
}
