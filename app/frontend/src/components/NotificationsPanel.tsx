import { useLayoutEffect, useState, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { useNavigate } from 'react-router-dom';
import { useStore } from '../store/useStore';

const tipoIcono: Record<string, string> = {
  aprobacion_tarea: '✅',
  tarea_rechazada: '⛔',
  tarea_asignada: '📥',
  edicion_pendiente: '✏️',
  tarea_a_revision: '🔀',
  revision_asignada: '👤',
  inactividad: '⏰',
  accion_bd: '🗄️',
  mover_cambios: '📦',
  editada_aprobada: '✅',
  advertencia: '⚠️',
  tarea_eliminada: '🗑️',
};

export function NotificationsPanel({
  onClose,
  anchorRef,
  panelRef,
}: {
  onClose: () => void;
  /** Botón/campana sobre el que se ancla el panel — se posiciona con un portal para
   * no quedar atrapado dentro del contexto de apilamiento del header (backdrop-blur). */
  anchorRef: RefObject<HTMLElement>;
  /** Ref al nodo raíz del panel — el que detecta "click afuera" en Topbar la necesita
   * porque, al vivir en un portal, ya no cuelga del DOM del botón que lo abre. */
  panelRef: RefObject<HTMLDivElement>;
}) {
  const navigate = useNavigate();
  const [pos, setPos] = useState<{ top: number; right: number } | null>(null);

  useLayoutEffect(() => {
    const r = anchorRef.current?.getBoundingClientRect();
    if (r) setPos({ top: r.bottom + 8, right: window.innerWidth - r.right });
  }, [anchorRef]);
  const email = useStore((s) => s.currentEmail);
  const notifs = useStore((s) => s.notificaciones).filter((n) => n.para === email);
  const tareas = useStore((s) => s.tareas);
  const marcarLeida = useStore((s) => s.marcarLeida);
  const desmarcarLeida = useStore((s) => s.desmarcarLeida);
  const marcarTodasLeidas = useStore((s) => s.marcarTodasLeidas);
  const [abiertaId, setAbiertaId] = useState<string | null>(null);
  const [tab, setTab] = useState<'sinAprobar' | 'todas'>('sinAprobar');

  // Una notificación de aprobación queda "resuelta" (ya se aprobó) cuando la tarea
  // relacionada ya no está pendiente — a partir de ahí solo queda como historial en "Todas".
  const estaAprobada = (n: (typeof notifs)[number]) => {
    if (n.tipo === 'aprobacion_tarea') {
      const t = tareas.find((x) => x.id === n.tarea_id);
      return !t || !t.pendiente_aprobacion;
    }
    if (n.tipo === 'edicion_pendiente') {
      const t = tareas.find((x) => x.id === n.tarea_id);
      return !t || !t.edicion_pendiente;
    }
    return false;
  };

  const visibles = tab === 'todas' ? notifs : notifs.filter((n) => !estaAprobada(n));

  if (!pos) return null;

  return createPortal(
    <div
      ref={panelRef}
      style={{ top: pos.top, right: pos.right }}
      className="fixed z-50 w-[360px] card animate-scale-in overflow-hidden"
    >
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <span className="text-sm font-semibold">Notificaciones</span>
        <button className="text-xs text-primary hover:underline" onClick={() => marcarTodasLeidas(email)}>
          Marcar todas leídas
        </button>
      </div>
      <div className="flex items-center gap-1 border-b border-border px-3 py-2">
        <button
          onClick={() => setTab('todas')}
          className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
            tab === 'todas' ? 'bg-surface-3 text-text' : 'text-faint hover:text-muted'
          }`}
        >
          Todas
        </button>
        <button
          onClick={() => setTab('sinAprobar')}
          className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
            tab === 'sinAprobar' ? 'bg-surface-3 text-text' : 'text-faint hover:text-muted'
          }`}
        >
          Sin aprobar
        </button>
      </div>
      <div className="max-h-[420px] overflow-y-auto">
        {visibles.length === 0 && (
          <div className="px-4 py-10 text-center text-sm text-muted">
            {tab === 'sinAprobar' ? 'No tienes notificaciones sin aprobar.' : 'No tienes notificaciones.'}
          </div>
        )}
        {visibles.map((n) => {
          const abierta = abiertaId === n.id;
          const vaAPendientes = n.accionable && n.tarea_id && !estaAprobada(n);
          return (
            <div
              key={n.id}
              onClick={() => {
                if (vaAPendientes) {
                  marcarLeida(n.id);
                  onClose();
                  navigate('/pendientes', { state: { focusTareaId: n.tarea_id } });
                  return;
                }
                setAbiertaId(abierta ? null : n.id);
              }}
              className={`flex cursor-pointer gap-3 border-b border-border/60 px-4 py-3 transition-colors hover:bg-surface-2 ${
                n.leida ? 'opacity-60' : ''
              }`}
            >
              <span className="mt-0.5 text-base leading-none">{tipoIcono[n.tipo] ?? '🔔'}</span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-medium text-text">{n.titulo}</p>
                  {!n.leida && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />}
                </div>
                <p className="mt-0.5 text-xs text-muted">{n.cuerpo}</p>
                {abierta && (
                  <div className="mt-2 flex items-center gap-2">
                    <span className="text-[11px] text-faint">{n.fecha}</span>
                    {n.accionable && n.tarea_id && estaAprobada(n) && (
                      <span className="chip border border-done/40 bg-green-500/10 text-green-300">
                        ✓ Aprobada
                      </span>
                    )}
                    {!estaAprobada(n) && (
                      <button
                        className="text-[11px] text-muted hover:text-text"
                        onClick={(e) => {
                          e.stopPropagation();
                          if (n.leida) desmarcarLeida(n.id);
                          else marcarLeida(n.id);
                        }}
                      >
                        {n.leida ? 'Marcar no leída' : 'Marcar leída'}
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>,
    document.body
  );
}
