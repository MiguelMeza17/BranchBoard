import { useState } from 'react';
import { useStore } from '../store/useStore';
import { puedeEliminarDefinitivo } from '../lib/permisos';
import { PageHeader, Card, EmptyState, TipoBadge, ReadOnlyTag } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { Modal } from '../components/Modal';
import { estadoMeta } from '../lib/ui';
import type { Tarea } from '../types';

export function Papelera() {
  const user = useStore((s) => s.currentUser());
  const tareas = useStore((s) => s.tareas);
  const eliminarDefinitivo = useStore((s) => s.eliminarDefinitivo);
  const [target, setTarget] = useState<Tarea | null>(null);
  // Click en una tarea de la Papelera: sus detalles (solo lectura, está eliminada).
  const [detalle, setDetalle] = useState<Tarea | null>(null);
  const usuarios = useStore((s) => s.usuarios);
  const nombreDe = (email: string | null) => {
    if (!email) return '—';
    const u = usuarios.find((x) => x.email === email);
    return u ? `${u.nombre} ${u.apellido}` : email;
  };

  // La más reciente primero; a igual fecha, por ID.
  const eliminadas = tareas
    .filter((t) => t.eliminada)
    .sort(
      (a, b) =>
        (b.fecha_eliminacion ?? '').localeCompare(a.fecha_eliminacion ?? '') ||
        a.id.localeCompare(b.id, undefined, { numeric: true })
    );
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
          <>
            <p className="mb-3 text-xs text-muted">
              {eliminadas.length} tarea{eliminadas.length === 1 ? '' : 's'} en la Papelera · más recientes primero
            </p>
            <div className="space-y-2">
              {eliminadas.map((t) => (
                <Card
                  key={t.id}
                  onClick={() => setDetalle(t)}
                  className="flex cursor-pointer flex-wrap items-center gap-4 transition hover:border-border-strong"
                >
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
                    <button
                      className="btn-danger btn-sm"
                      onClick={(e) => {
                        e.stopPropagation(); // el botón no abre los detalles
                        setTarget(t);
                      }}
                    >
                      Eliminar definitivamente
                    </button>
                  ) : (
                    <ReadOnlyTag />
                  )}
                </Card>
              ))}
            </div>
          </>
        )}
      </div>

      {/* Detalles de una tarea eliminada (solo lectura) */}
      <Modal
        open={!!detalle}
        onClose={() => setDetalle(null)}
        title={detalle?.titulo ?? ''}
        subtitle={detalle ? `${detalle.id} · ${detalle.tipo} · en la Papelera` : ''}
        size="md"
      >
        {detalle && (
          <div className="space-y-4 text-sm">
            {/* Arriba: por qué se eliminó (es distinto de los detalles de la tarea). */}
            <div className="rounded-lg border border-danger/40 bg-danger/10 px-3 py-2.5">
              <div className="mb-1 flex items-center justify-between gap-2 text-xs">
                <span className="font-semibold text-red-200">Por qué se eliminó</span>
                <span className="text-red-200/70">{detalle.fecha_eliminacion ?? '—'}</span>
              </div>
              <p className="whitespace-pre-wrap leading-relaxed text-red-100">
                {detalle.justificacion_eliminacion || 'Sin motivo registrado.'}
              </p>
            </div>
            <div>
              <dt className="mb-1 text-xs text-faint">Detalles de la tarea</dt>
              {detalle.detalles ? (
                <p className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-lg border border-border bg-surface-2 px-3 py-2 leading-relaxed text-text">
                  {detalle.detalles}
                </p>
              ) : (
                <p className="text-faint">Sin detalles.</p>
              )}
            </div>
            <dl className="grid grid-cols-2 gap-3 border-t border-border pt-3">
              <div>
                <dt className="text-xs text-faint">Estado al eliminarla</dt>
                <dd className="text-text">{estadoMeta[detalle.estado].label}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Tipo</dt>
                <dd className="text-text">{detalle.tipo}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Rama</dt>
                <dd className="font-mono text-text">{detalle.rama_asociada ?? 'Sin rama'}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Tarea padre</dt>
                <dd className="text-text">{detalle.tarea_padre_id ?? '—'}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Responsable</dt>
                <dd className="text-text">{nombreDe(detalle.asignado_a)}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Creada por</dt>
                <dd className="text-text">{nombreDe(detalle.creado_por)}</dd>
              </div>
            </dl>
          </div>
        )}
      </Modal>

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
