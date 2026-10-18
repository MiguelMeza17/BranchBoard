import { useEffect, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { puedeAprobarPendientes } from '../lib/permisos';
import { PageHeader, Card, EmptyState, TipoBadge, Avatar } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { Modal } from '../components/Modal';
import type { EdicionPendiente, Tarea } from '../types';

export function Pendientes() {
  const location = useLocation();
  const [resaltada, setResaltada] = useState<string | null>(null);
  const user = useStore((s) => s.currentUser());
  const tareas = useStore((s) => s.tareas);
  const edicionesPendientes = useStore((s) => s.edicionesPendientes);
  const aprobarTarea = useStore((s) => s.aprobarTarea);
  const advertirTarea = useStore((s) => s.advertirTarea);
  const eliminarTarea = useStore((s) => s.eliminarTarea);
  const aprobarEdicion = useStore((s) => s.aprobarEdicion);
  const rechazarEdicion = useStore((s) => s.rechazarEdicion);
  const editarTarea = useStore((s) => s.editarTarea);
  const editarYAprobarTarea = useStore((s) => s.editarYAprobarTarea);

  const [revisando, setRevisando] = useState<Tarea | null>(null);
  const [advertir, setAdvertir] = useState<Tarea | null>(null);
  const [mensaje, setMensaje] = useState('');
  const [eliminar, setEliminar] = useState<Tarea | null>(null);
  const [aprobarConfirm, setAprobarConfirm] = useState<Tarea | null>(null);
  const [editarAprobar, setEditarAprobar] = useState<Tarea | null>(null);
  const [revisandoEdicion, setRevisandoEdicion] = useState<EdicionPendiente | null>(null);
  const [aprobarEdicionConfirm, setAprobarEdicionConfirm] = useState<EdicionPendiente | null>(null);
  const [rechazarEdicionConfirm, setRechazarEdicionConfirm] = useState<EdicionPendiente | null>(null);

  const puedeActuar = puedeAprobarPendientes(user.rol);
  const nuevas = tareas.filter((t) => t.pendiente_aprobacion && !t.eliminada);

  // Llegada desde una notificación: hace scroll hasta la tarea y la resalta un momento.
  useEffect(() => {
    const focusTareaId = (location.state as { focusTareaId?: string } | null)?.focusTareaId;
    if (!focusTareaId) return;
    const el = document.getElementById(`pendiente-${focusTareaId}`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    setResaltada(focusTareaId);
    const t = setTimeout(() => setResaltada(null), 2500);
    return () => clearTimeout(t);
  }, [location.state]);

  return (
    <div>
      <PageHeader title="Pendientes" desc="Tareas nuevas y ediciones esperando aprobación de un Líder o Administrador." />

      <div className="card p-5">
      {/* Tareas nuevas */}
      <section className="mb-8">
        <div className="mb-3 flex items-center gap-2">
          <span className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300">Nueva</span>
          <h2 className="text-sm font-semibold text-text">Tareas nuevas pendientes</h2>
          <span className="chip border border-border bg-surface-2 text-muted">{nuevas.length}</span>
        </div>
        {nuevas.length === 0 ? (
          <EmptyState icon="📌" title="No hay tareas pendientes de aprobación" />
        ) : (
          <div className="space-y-2">
            {nuevas.map((t) => {
              const esCreador = t.creado_por === user.email;
              return (
                <Card
                  key={t.id}
                  id={`pendiente-${t.id}`}
                  className={`flex flex-wrap items-center gap-4 transition-shadow ${
                    resaltada === t.id ? 'ring-2 ring-primary' : ''
                  }`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-[11px] text-faint">{t.id}</span>
                      <span className="font-medium text-text">{t.titulo}</span>
                      {t.tarea_padre_id && <span className="text-xs text-faint">↳ subtarea de {t.tarea_padre_id}</span>}
                    </div>
                    <div className="mt-1.5 flex flex-wrap items-center gap-2">
                      <TipoBadge tipo={t.tipo} />
                      <span className="text-xs text-muted">creada por</span>
                      <Avatar email={t.creado_por} size={20} />
                    </div>
                  </div>
                  {puedeActuar ? (
                    <div className="flex flex-wrap items-center gap-2">
                      <button className="btn-primary btn-sm" onClick={() => setRevisando(t)}>Revisar</button>
                    </div>
                  ) : esCreador ? (
                    <button className="btn-outline btn-sm" onClick={() => setEditarAprobar(t)}>Editar</button>
                  ) : (
                    <span className="text-xs text-faint">Esperando Líder/Administrador</span>
                  )}
                </Card>
              );
            })}
          </div>
        )}
      </section>

      {/* Ediciones pendientes */}
      <section>
        <div className="mb-3 flex items-center gap-2">
          <span className="chip border border-warn/40 bg-yellow-500/15 text-yellow-300">Edición</span>
          <h2 className="text-sm font-semibold text-text">Ediciones pendientes</h2>
          <span className="chip border border-border bg-surface-2 text-muted">{edicionesPendientes.length}</span>
        </div>
        {edicionesPendientes.length === 0 ? (
          <EmptyState icon="✏️" title="No hay ediciones esperando confirmación" />
        ) : (
          <div className="space-y-2">
            {edicionesPendientes.map((ep) => {
              const t = tareas.find((x) => x.id === ep.tarea_id);
              return (
                <Card
                  key={ep.id}
                  id={`pendiente-${ep.tarea_id}`}
                  className={`flex flex-wrap items-center gap-4 transition-shadow ${
                    resaltada === ep.tarea_id ? 'ring-2 ring-primary' : ''
                  }`}
                >
                  <div className="min-w-0 flex-1">
                    <span className="font-medium text-text">{t?.titulo}</span>
                    {ep.motivo && <p className="mt-1 text-xs text-muted">Motivo: {ep.motivo}</p>}
                  </div>
                  {puedeActuar ? (
                    <button className="btn-primary btn-sm" onClick={() => setRevisandoEdicion(ep)}>Revisar</button>
                  ) : (
                    <span className="text-xs text-faint">Esperando Líder/Admin</span>
                  )}
                </Card>
              );
            })}
          </div>
        )}
      </section>
      </div>

      {/* Advertir */}
      <Modal
        open={!!advertir}
        onClose={() => { setAdvertir(null); setMensaje(''); }}
        title="Advertir sobre la tarea"
        subtitle="La tarea no cambia — solo se le notifica al creador."
        size="sm"
        footer={
          <>
            <button className="btn-ghost" onClick={() => { setAdvertir(null); setMensaje(''); }}>Cancelar</button>
            <button className="btn-primary" disabled={mensaje.trim().length < 3} onClick={() => { if (advertir) advertirTarea(advertir.id, mensaje); setAdvertir(null); setMensaje(''); }}>
              Enviar advertencia
            </button>
          </>
        }
      >
        <label className="label">Mensaje para el creador (obligatorio)</label>
        <textarea
          className="input min-h-[90px] resize-y"
          value={mensaje}
          onChange={(e) => setMensaje(e.target.value)}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && mensaje.trim().length >= 3) {
              e.preventDefault();
              if (advertir) advertirTarea(advertir.id, mensaje);
              setAdvertir(null);
              setMensaje('');
            }
          }}
          placeholder="Explica qué está mal para que pueda corregirlo…"
          autoFocus
        />
      </Modal>

      {/* Rechazar (=eliminar, doble candado) */}
      <ConfirmLock
        open={!!eliminar}
        onClose={() => setEliminar(null)}
        onConfirm={(justif) => { if (eliminar) eliminarTarea(eliminar.id, justif ?? ''); setEliminar(null); }}
        title="Rechazar tarea"
        tone="danger"
        palabra="rechazar"
        confirmLabel="Rechazar y enviar a Papelera"
        requiereJustificacion
        justificacionLabel="Justificación del rechazo"
        description={`"${eliminar?.titulo}" saldrá de Pendientes y del Tablero Scrum, y pasará a la Papelera.`}
      />

      {/* Aprobar (doble candado) */}
      <ConfirmLock
        open={!!aprobarConfirm}
        onClose={() => setAprobarConfirm(null)}
        onConfirm={() => { if (aprobarConfirm) aprobarTarea(aprobarConfirm.id); setAprobarConfirm(null); }}
        title="Aprobar tarea"
        tone="primary"
        palabra="aprobar"
        confirmLabel="Aprobar tarea"
        description={`"${aprobarConfirm?.titulo}" quedará aprobada — ya podrá mandarse a revisión.`}
      />

      {/* Revisar (ver detalle y elegir acción) */}
      {revisando && (
        <RevisarPendienteModal
          tarea={revisando}
          onClose={() => setRevisando(null)}
          onAprobar={() => { setAprobarConfirm(revisando); setRevisando(null); }}
          onEditar={() => { setEditarAprobar(revisando); setRevisando(null); }}
          onAdvertir={() => { setAdvertir(revisando); setRevisando(null); }}
          onRechazar={() => { setEliminar(revisando); setRevisando(null); }}
        />
      )}

      {/* Revisar edición pendiente */}
      {revisandoEdicion && (
        <RevisarEdicionModal
          edicion={revisandoEdicion}
          tarea={tareas.find((x) => x.id === revisandoEdicion.tarea_id)}
          onClose={() => setRevisandoEdicion(null)}
          onAprobar={() => { setAprobarEdicionConfirm(revisandoEdicion); setRevisandoEdicion(null); }}
          onRechazar={() => { setRechazarEdicionConfirm(revisandoEdicion); setRevisandoEdicion(null); }}
        />
      )}

      {/* Aprobar edición (doble candado) */}
      <ConfirmLock
        open={!!aprobarEdicionConfirm}
        onClose={() => setAprobarEdicionConfirm(null)}
        onConfirm={() => { if (aprobarEdicionConfirm) aprobarEdicion(aprobarEdicionConfirm.tarea_id); setAprobarEdicionConfirm(null); }}
        title="Aprobar edición"
        tone="primary"
        palabra="aprobar"
        confirmLabel="Aprobar edición"
        description="Los valores nuevos quedarán definitivos y desaparece la sombra amarilla de la tarea."
      />

      {/* Rechazar edición (doble candado) */}
      <ConfirmLock
        open={!!rechazarEdicionConfirm}
        onClose={() => setRechazarEdicionConfirm(null)}
        onConfirm={() => { if (rechazarEdicionConfirm) rechazarEdicion(rechazarEdicionConfirm.tarea_id); setRechazarEdicionConfirm(null); }}
        title="Rechazar edición"
        tone="danger"
        palabra="rechazar"
        confirmLabel="Rechazar edición"
        description="Se revierten los campos a sus valores anteriores y la edición pendiente desaparece."
      />

      {/* Editar y aprobar */}
      {editarAprobar && (
        <EditarAprobarModal
          tarea={editarAprobar}
          esCreador={editarAprobar.creado_por === user.email && !puedeActuar}
          onClose={() => setEditarAprobar(null)}
          onSave={(campos, aprobar) => {
            if (aprobar) editarYAprobarTarea(editarAprobar.id, campos);
            else editarTarea(editarAprobar.id, campos, false);
            setEditarAprobar(null);
          }}
        />
      )}
    </div>
  );
}

function RevisarEdicionModal({
  edicion,
  tarea,
  onClose,
  onAprobar,
  onRechazar,
}: {
  edicion: EdicionPendiente;
  tarea: Tarea | undefined;
  onClose: () => void;
  onAprobar: () => void;
  onRechazar: () => void;
}) {
  const campos = Object.keys(edicion.campos_nuevos);
  const descripcionNueva = edicion.campos_nuevos.descripcion ?? tarea?.descripcion ?? '';
  return (
    <Modal
      open
      onClose={onClose}
      title={edicion.campos_nuevos.titulo ?? tarea?.titulo ?? edicion.tarea_id}
      subtitle={`${edicion.tarea_id}${tarea ? ` · ${tarea.tipo}` : ''}`}
      footer={
        <>
          <button className="btn-danger btn-sm" onClick={onRechazar}>Rechazar</button>
          <button className="btn-primary btn-sm" onClick={onAprobar}>Aprobar</button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted">Editada por</span>
          <Avatar email={edicion.creado_por} size={20} />
        </div>
        {edicion.motivo && <p className="text-sm text-muted">Motivo: {edicion.motivo}</p>}
        <p className="text-sm text-text">{descripcionNueva || 'Sin descripción.'}</p>
        <div className="space-y-1.5 border-t border-border pt-3">
          {campos.map((c) => (
            <div key={c} className="flex items-start gap-2 text-xs">
              <span className="w-20 shrink-0 text-faint">{c}</span>
              <span className="rounded bg-danger/15 px-1.5 py-0.5 text-red-300 line-through">{edicion.campos_anteriores[c]}</span>
              <span className="text-faint">→</span>
              <span className="rounded bg-green-500/15 px-1.5 py-0.5 text-green-300">{edicion.campos_nuevos[c]}</span>
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
}

function RevisarPendienteModal({
  tarea,
  onClose,
  onAprobar,
  onEditar,
  onAdvertir,
  onRechazar,
}: {
  tarea: Tarea;
  onClose: () => void;
  onAprobar: () => void;
  onEditar: () => void;
  onAdvertir: () => void;
  onRechazar: () => void;
}) {
  return (
    <Modal
      open
      onClose={onClose}
      title={tarea.titulo}
      subtitle={`${tarea.id} · ${tarea.tipo}`}
      footer={
        <>
          <button className="btn-outline btn-sm" onClick={onAdvertir}>Advertir</button>
          <button className="btn-outline btn-sm" onClick={onEditar}>Editar</button>
          <button className="btn-danger btn-sm" onClick={onRechazar}>Rechazar</button>
          <button className="btn-primary btn-sm" onClick={onAprobar}>Aprobar</button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted">Creada por</span>
          <Avatar email={tarea.creado_por} size={20} />
        </div>
        <p className="text-sm text-text">{tarea.descripcion || 'Sin descripción.'}</p>
      </div>
    </Modal>
  );
}

function EditarAprobarModal({
  tarea,
  esCreador,
  onClose,
  onSave,
}: {
  tarea: Tarea;
  esCreador: boolean;
  onClose: () => void;
  onSave: (campos: { titulo: string; descripcion: string }, aprobar: boolean) => void;
}) {
  const [titulo, setTitulo] = useState(tarea.titulo);
  const [descripcion, setDescripcion] = useState(tarea.descripcion);
  return (
    <Modal
      open
      onClose={onClose}
      title={esCreador ? 'Editar tarea' : 'Editar y aprobar'}
      subtitle={esCreador ? 'Es tu tarea y sigue pendiente — la editas sin aprobación.' : 'Corriges los campos y apruebas en un solo paso.'}
      footer={
        <>
          <button className="btn-ghost" onClick={onClose}>Cancelar</button>
          <button className="btn-primary" onClick={() => onSave({ titulo, descripcion }, !esCreador)}>
            {esCreador ? 'Guardar' : 'Editar y aprobar'}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="label">Título</label>
          <input
            className="input"
            value={titulo}
            onChange={(e) => setTitulo(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && titulo.trim()) {
                e.preventDefault();
                onSave({ titulo, descripcion }, !esCreador);
              }
            }}
            autoFocus
          />
        </div>
        <div>
          <label className="label">Descripción</label>
          <textarea
            className="input min-h-[90px] resize-y"
            value={descripcion}
            onChange={(e) => setDescripcion(e.target.value)}
            onKeyDown={(e) => {
              if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && titulo.trim()) {
                e.preventDefault();
                onSave({ titulo, descripcion }, !esCreador);
              }
            }}
          />
        </div>
      </div>
    </Modal>
  );
}
