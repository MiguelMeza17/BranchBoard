import { Modal } from './Modal';
import { AccionesRama, CambiosTraidos, RamaDeLaTarea } from './AccionesRama';
import { CambiosEntrantes, CheckinsDeLaRama, PendingChanges } from './CheckinsDeLaRama';
import { estadoMeta } from '../lib/ui';
import type { Tarea } from '../types';

// Ventana de detalles de una tarea: la misma en el Tablero Scrum, el tablero de categoría y la Lista
// de tareas. Arriba su rama y las secciones de la rama; al pie, los botones de la rama
// (Abrir en Unity, Traer cambios, Editar).
export function DetalleTareaModal({ tarea, onClose }: { tarea: Tarea | null; onClose: () => void }) {
  return (
    <Modal
      open={!!tarea}
      onClose={onClose}
      title={tarea?.titulo ?? ''}
      subtitle={tarea ? `${tarea.id} · ${tarea.tipo}` : ''}
      size="md"
      footer={tarea ? <AccionesRama tarea={tarea} onCerrar={onClose} /> : undefined}
    >
      {tarea && (
        <div className="space-y-4 text-sm">
          <RamaDeLaTarea tarea={tarea} />
          <PendingChanges tarea={tarea} />
          <CambiosEntrantes tarea={tarea} />
          <CheckinsDeLaRama tarea={tarea} />
          <CambiosTraidos tarea={tarea} />
          <div className="rounded-xl border border-border bg-surface p-3">
            <dt className="mb-2 text-xs font-semibold text-muted">Detalles</dt>
            {tarea.detalles ? (
              <p className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-lg border border-border bg-surface-2 px-3 py-2 leading-relaxed text-text">
                {tarea.detalles}
              </p>
            ) : (
              <p className="text-faint">Sin detalles.</p>
            )}
          </div>
          <dl className="grid grid-cols-2 gap-3 border-t border-border pt-3">
            <div>
              <dt className="text-xs text-faint">Estado</dt>
              <dd className="text-text">{estadoMeta[tarea.estado].label}</dd>
            </div>
            <div>
              <dt className="text-xs text-faint">Rama</dt>
              <dd className="font-mono text-accent">{tarea.rama_asociada ?? '— sin rama —'}</dd>
            </div>
            <div>
              <dt className="text-xs text-faint">Asignado a</dt>
              <dd className="text-text">{tarea.asignado_a ?? 'Sin asignar'}</dd>
            </div>
            <div>
              <dt className="text-xs text-faint">Creado por</dt>
              <dd className="text-text">{tarea.creado_por}</dd>
            </div>
            <div>
              <dt className="text-xs text-faint">Fecha de creación</dt>
              <dd className="text-text">{tarea.fecha_creacion}</dd>
            </div>
            <div>
              <dt className="text-xs text-faint">Fecha límite</dt>
              <dd className="text-text">{tarea.fecha_limite ?? 'Sin fecha'}</dd>
            </div>
            <div>
              <dt className="text-xs text-faint">Pendiente aprobación</dt>
              <dd className="text-text">{tarea.pendiente_aprobacion ? 'Sí' : 'No'}</dd>
            </div>
          </dl>
        </div>
      )}
    </Modal>
  );
}
