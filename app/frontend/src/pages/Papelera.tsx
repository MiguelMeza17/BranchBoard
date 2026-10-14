import { useState } from 'react';
import { useStore } from '../store/useStore';
import { puedeEliminarDefinitivo } from '../lib/permisos';
import { PageHeader, Card, EmptyState, TipoBadge, ReadOnlyTag } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import type { Tarea } from '../types';

export function Papelera() {
  const user = useStore((s) => s.currentUser());
  const tareas = useStore((s) => s.tareas);
  const eliminarDefinitivo = useStore((s) => s.eliminarDefinitivo);
  const [target, setTarget] = useState<Tarea | null>(null);

  const eliminadas = tareas.filter((t) => t.eliminada);
  const puedeBorrar = puedeEliminarDefinitivo(user.rol);

  return (
    <div>
      <PageHeader
        title="Papelera"
        desc="Tareas eliminadas (soft-delete). No hay restauración — la eliminación definitiva es exclusiva de Líder/Administrador."
        actions={!puedeBorrar ? <ReadOnlyTag /> : undefined}
      />

      <div className="card p-5">
        {eliminadas.length === 0 ? (
          <EmptyState icon="🗑️" title="La papelera está vacía" desc="No hay tareas eliminadas por ahora." />
        ) : (
          <div className="space-y-2">
            {eliminadas.map((t) => (
              <Card key={t.id} className="flex flex-wrap items-center gap-4">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-[11px] text-faint">{t.id}</span>
                    <span className="font-medium text-text line-through decoration-faint">{t.titulo}</span>
                  </div>
                  <div className="mt-1.5 flex flex-wrap items-center gap-2">
                    <TipoBadge tipo={t.tipo} />
                    <span className="text-xs text-faint">Eliminada: {t.fecha_eliminacion}</span>
                  </div>
                  {t.justificacion_eliminacion && (
                    <p className="mt-2 text-xs text-muted">Motivo: {t.justificacion_eliminacion}</p>
                  )}
                </div>
                {puedeBorrar ? (
                  <button className="btn-danger btn-sm" onClick={() => setTarget(t)}>Eliminar definitivamente</button>
                ) : (
                  <ReadOnlyTag />
                )}
              </Card>
            ))}
          </div>
        )}
      </div>

      <ConfirmLock
        open={!!target}
        onClose={() => setTarget(null)}
        onConfirm={() => { if (target) eliminarDefinitivo(target.id); setTarget(null); }}
        title="Eliminar definitivamente"
        tone="danger"
        palabra="eliminar"
        confirmLabel="Borrar para siempre"
        description={`"${target?.titulo}" se borrará por completo y su rama se purgará en Unity Version Control. Esta acción no se puede deshacer.`}
      />
    </div>
  );
}
