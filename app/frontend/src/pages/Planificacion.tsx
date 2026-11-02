import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { PageHeader, EmptyState } from '../components/primitives';
import { CrearSprintModal } from '../components/CrearSprintModal';
import { AsignarTareasSprintModal } from '../components/AsignarTareasSprintModal';
import type { Sprint } from '../types';

// Planificación: sprints (con fecha: empiezan y terminan, generan su reporte) y backlogs (sin fecha:
// agrupan tareas sin plazo). Los dos son "sprints" en el modelo de datos (`con_fecha` true/false).
// Crearlos y asignarles tareas: Líder y Administrador. Cerrarlos: solo el Administrador. Los demás
// roles los ven.
export function Planificacion() {
  const navigate = useNavigate();
  const user = useStore((s) => s.currentUser());
  const sprints = useStore((s) => s.sprints);
  const tareas = useStore((s) => s.tareas);
  const cerrarSprint = useStore((s) => s.cerrarSprint);
  const [crear, setCrear] = useState<'sprint' | 'backlog' | null>(null);
  // Sprint/backlog al que se le están asignando tareas (se guarda el id: siempre la versión actual).
  const [asignandoId, setAsignandoId] = useState<string | null>(null);
  const puedeGestionar = user.rol === 'Líder' || user.rol === 'Administrador';
  const esAdmin = user.rol === 'Administrador';

  const conFecha = sprints.filter((s) => s.con_fecha);
  const sinFecha = sprints.filter((s) => !s.con_fecha);
  const tareasDe = (id: string) => tareas.filter((t) => t.sprint_id === id && !t.eliminada);

  const fila = (sp: Sprint) => {
    const suyas = tareasDe(sp.id);
    const hechas = suyas.filter((t) => t.estado === 'terminada').length;
    return (
      <div
        key={sp.id}
        // Click en cualquier parte: el Tablero Scrum con este sprint elegido (solo sus tareas).
        onClick={() => navigate(`/scrum?sprint=${sp.id}`)}
        className="card flex cursor-pointer flex-wrap items-center gap-4 p-4 transition hover:border-border-strong"
      >
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="font-medium text-text">{sp.nombre}</span>
            {(sp.con_fecha || sp.cerrado) && (
              <span
                className={`chip border ${
                  sp.cerrado ? 'border-border bg-surface-3 text-faint' : 'border-green-500/40 bg-green-500/10 text-green-300'
                }`}
              >
                {sp.cerrado ? 'Cerrado' : 'En curso'}
              </span>
            )}
          </div>
          {sp.descripcion && <p className="mt-1 text-sm text-muted">{sp.descripcion}</p>}
          <p className="mt-1.5 text-[12px] text-faint">
            {sp.con_fecha ? (
              <>
                {sp.fecha_inicio} → {sp.fecha_fin} · {sp.duracion_dias} días ·{' '}
              </>
            ) : null}
            {suyas.length} {suyas.length === 1 ? 'tarea' : 'tareas'}
            {suyas.length > 0 && <> · {hechas} terminadas</>}
          </p>
        </div>
        {puedeGestionar && (
          <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
            {!sp.cerrado && (
              <button className="btn-primary btn-sm" onClick={() => setAsignandoId(sp.id)}>
                Asignar tareas
              </button>
            )}
            {/* Cerrar: solo el Administrador. */}
            {esAdmin && !sp.cerrado && (
              <button className="btn-outline btn-sm" onClick={() => cerrarSprint(sp.id)}>
                {sp.con_fecha ? 'Cerrar sprint' : 'Cerrar backlog'}
              </button>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div>
      <PageHeader
        title="Planificación"
        desc="Sprints (con fecha de inicio y fin) y backlogs (tareas sin fecha). Las tareas se asignan al crearlas o desde aquí, con «Asignar tareas»."
        actions={
          puedeGestionar ? (
            <div className="flex items-center gap-2">
              <button className="btn-outline" onClick={() => setCrear('backlog')}>
                + Nuevo backlog
              </button>
              <button className="btn-primary" onClick={() => setCrear('sprint')}>
                + Nuevo sprint
              </button>
            </div>
          ) : undefined
        }
      />

      <div className="card p-5">
        <section className="mb-8">
          <div className="mb-3 flex items-center gap-2">
            <h2 className="text-sm font-semibold text-text">🏃 Sprints</h2>
            <span className="chip border border-border bg-surface-2 text-muted">{conFecha.length}</span>
          </div>
          {conFecha.length === 0 ? (
            <EmptyState icon="🏃" title="Todavía no hay sprints" desc="Un sprint tiene fecha de inicio y de fin, y al terminar genera su reporte." />
          ) : (
            <div className="space-y-2">{conFecha.map(fila)}</div>
          )}
        </section>

        <section>
          <div className="mb-3 flex items-center gap-2">
            <h2 className="text-sm font-semibold text-text">📦 Backlogs</h2>
            <span className="chip border border-border bg-surface-2 text-muted">{sinFecha.length}</span>
          </div>
          {sinFecha.length === 0 ? (
            <EmptyState icon="📦" title="Todavía no hay backlogs" desc="Un backlog agrupa tareas sin fecha límite, para planificarlas más adelante." />
          ) : (
            <div className="space-y-2">{sinFecha.map(fila)}</div>
          )}
        </section>
      </div>

      <CrearSprintModal tipo={crear} onClose={() => setCrear(null)} />
      <AsignarTareasSprintModal
        sprint={sprints.find((s) => s.id === asignandoId) ?? null}
        onClose={() => setAsignandoId(null)}
      />
    </div>
  );
}

