import { useStore, progresoDe } from '../store/useStore';
import { ConfirmLock } from './ConfirmLock';

// "En producción" -> "Por hacer" con progreso: se puede, pero borra lo hecho. Pide escribir
// "eliminar". Lo usan el Tablero Scrum y el tablero de cada categoría, para todos los roles.
export function BorrarProgresoModal({ tareaId, onClose }: { tareaId: string | null; onClose: () => void }) {
  const tareas = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  const traidos = useStore((s) => s.cambiosTraidos);
  const borrarProgreso = useStore((s) => s.borrarProgreso);
  const conProgreso = tareaId ? progresoDe(tareas, commits, tareaId) : [];
  const ids = new Set(conProgreso);
  const nCommits = commits.filter((c) => ids.has(c.tarea_id)).length;
  const nPrestados = tareaId ? traidos.filter((c) => c.tarea_id === tareaId && !c.quitado_en).length : 0;
  return (
    <ConfirmLock
      open={!!tareaId}
      onClose={onClose}
      onConfirm={() => {
        if (tareaId) borrarProgreso(tareaId);
        onClose();
      }}
      title="Volver a Por hacer"
      tone="danger"
      palabra="eliminar"
      confirmLabel="Eliminar progreso y mover"
      description={
        <div className="space-y-2">
          <p>
            Si mueves <span className="font-mono">{tareaId}</span> a <b>Por hacer</b>, se eliminará tu progreso sobre
            esta tarea. No se puede deshacer.
          </p>
          <ul className="list-disc space-y-0.5 pl-5 text-muted">
            <li>
              {nCommits} {nCommits === 1 ? 'commit' : 'commits'}
              {conProgreso.length > 1 || (conProgreso[0] && conProgreso[0] !== tareaId)
                ? ` (en ${conProgreso.join(', ')})`
                : ''}
            </li>
            {nPrestados > 0 && <li>{nPrestados} cambios prestados sin devolver</li>}
            <li>Los cambios sin check-in de su rama</li>
          </ul>
          <p className="text-muted">La tarea sigue tomada por la misma persona, con su rama, pero vacía.</p>
        </div>
      }
    />
  );
}
