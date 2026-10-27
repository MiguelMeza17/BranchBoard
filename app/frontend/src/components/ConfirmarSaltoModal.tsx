import type { EstadoTarea, Tarea } from '../types';
import { estadoMeta } from '../lib/ui';
import type { EstadoVisual } from './TaskCard';
import { Modal } from './Modal';

// Orden de las columnas del tablero, para nombrar los estados que se salta la tarea.
const ORDEN: EstadoVisual[] = ['sin asignar', 'por hacer', 'en producción', 'en revisión', 'terminada'];

const labelDe = (e: EstadoVisual) => (e === 'sin asignar' ? 'Sin asignar' : estadoMeta[e].label);

const columnaDe = (t: Tarea): EstadoVisual =>
  t.estado === 'por hacer' && !t.asignado_a ? 'sin asignar' : t.estado;

function estadosSaltados(tarea: Tarea, to: EstadoTarea): EstadoVisual[] {
  const i = ORDEN.indexOf(columnaDe(tarea));
  const j = ORDEN.indexOf(to);
  return j > i + 1 ? ORDEN.slice(i + 1, j) : [];
}

/** Confirma forzar una tarea "Sin asignar" a una columna distinta de "Por hacer". */
export function ConfirmarSaltoModal({
  tarea,
  to,
  onClose,
  onConfirm,
}: {
  tarea: Tarea | null;
  to: EstadoTarea | null;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const saltados = tarea && to ? estadosSaltados(tarea, to) : [];
  const sinDueno = !!tarea && tarea.estado === 'por hacer' && !tarea.asignado_a;

  return (
    <Modal
      open={!!tarea && !!to}
      onClose={onClose}
      title="Saltar estados"
      size="sm"
      footer={
        <>
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn-primary" onClick={onConfirm}>
            Continuar
          </button>
        </>
      }
    >
      {tarea && to && (
        <div className="space-y-3 text-sm text-muted">
          <p>
            Vas a mover <span className="font-semibold text-text">"{tarea.titulo}"</span> de{' '}
            <span className="font-semibold text-text">{labelDe(columnaDe(tarea))}</span> a{' '}
            <span className="font-semibold text-text">{labelDe(to)}</span>, saltándote{' '}
            {saltados.map((e, k) => (
              <span key={e}>
                {k > 0 && (k === saltados.length - 1 ? ' y ' : ', ')}
                <span className="font-semibold text-text">{labelDe(e)}</span>
              </span>
            ))}
            . Una tarea sin asignar normalmente solo pasa a Por hacer.
          </p>
          {sinDueno && (
            <p>Al continuar, la tarea se te asigna y se crea su rama, igual que con "Tomar tarea".</p>
          )}
        </div>
      )}
    </Modal>
  );
}
