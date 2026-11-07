import { useEffect, useMemo, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import { progresoDe, useStore, LIMITE_REAPERTURAS, puedeReabrir } from '../store/useStore';
import { type EstadoTarea, type Tarea, type TipoTarea } from '../types';
import { estadoMeta, tipoMeta } from '../lib/ui';
import { esLiderOAdmin, tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { TaskCard } from '../components/TaskCard';
import { PageHeader } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { BorrarProgresoModal } from '../components/BorrarProgresoModal';
import { ConfirmarSaltoModal } from '../components/ConfirmarSaltoModal';
import { Modal } from '../components/Modal';
import { DetalleTareaModal } from '../components/DetalleTareaModal';
import { CrearTareaModal } from '../components/CrearTareaModal';

// Tablero Scrum de una categoría: distinto del tablero principal — no agrupa por
// jerarquía (generales/individuales), solo muestra, todas al mismo nivel, las tareas
// (raíz o subtareas) cuyo tipo coincide con la categoría elegida.
type ColVisual = 'sin-asignar' | EstadoTarea;

const COLUMNAS: { id: ColVisual; label: string; dot: string; bg: string; border: string; dashed?: boolean }[] = [
  { id: 'sin-asignar', label: 'Sin asignar', dot: '#94a3b8', bg: 'bg-slate-500/5', border: 'border-slate-500/25', dashed: true },
  { id: 'por hacer', ...estadoMeta['por hacer'] },
  { id: 'en producción', ...estadoMeta['en producción'] },
  { id: 'en revisión', ...estadoMeta['en revisión'] },
  { id: 'terminada', ...estadoMeta['terminada'] },
];

function DraggableCard({
  tarea,
  footer,
  onOpenDetalle,
  ocultarResponsable,
  resaltada,
  disabled,
}: {
  tarea: Tarea;
  footer: React.ReactNode;
  onOpenDetalle: () => void;
  ocultarResponsable: boolean;
  resaltada: boolean;
  disabled: boolean;
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: tarea.id, disabled });
  const colorTipo = tipoMeta[tarea.tipo].color;
  return (
    <div
      id={`categoria-tarea-${tarea.id}`}
      ref={setNodeRef}
      className={`rounded-xl transition-shadow duration-500 ${isDragging ? 'opacity-40' : ''}`}
      style={resaltada ? { boxShadow: `0 0 18px 4px ${colorTipo}8c` } : undefined}
    >
      <TaskCard
        tarea={tarea}
        footer={footer}
        onOpenDetalle={onOpenDetalle}
        dragHandleProps={{ ...attributes, ...listeners }}
        ocultarResponsable={ocultarResponsable}
      />
    </div>
  );
}

function Column({ col, children, count }: { col: (typeof COLUMNAS)[number]; children: React.ReactNode; count: number }) {
  const { setNodeRef, isOver } = useDroppable({ id: col.id });
  return (
    <div className="flex min-w-[280px] flex-1 flex-col">
      <div className="mb-3 flex items-center justify-between px-1">
        <div className="flex items-center gap-2">
          <span className="h-2.5 w-2.5 rounded-full" style={{ background: col.dot }} />
          <span className="text-sm font-semibold text-text">{col.label}</span>
          <span className="rounded-full bg-surface-3 px-2 py-0.5 text-xs text-muted">{count}</span>
        </div>
      </div>
      <div
        ref={setNodeRef}
        className={`flex-1 space-y-3 rounded-2xl border p-3 transition-colors ${col.bg} ${
          col.dashed ? 'border-dashed' : ''
        } ${isOver ? 'border-primary ring-2 ring-primary/30' : col.border}`}
        style={{ minHeight: 200 }}
      >
        {children}
      </div>
    </div>
  );
}

export function CategoriaScrum() {
  const { tipo } = useParams<{ tipo: string }>();
  const tipoTarea = tipo as TipoTarea;
  const navigate = useNavigate();
  const location = useLocation();

  const user = useStore((s) => s.currentUser());
  const tareasAll = useStore((s) => s.tareas);
  const moverTarea = useStore((s) => s.moverTarea);
  const tomarTarea = useStore((s) => s.tomarTarea);
  const desasignarTarea = useStore((s) => s.desasignarTarea);
  const autorizarMerge = useStore((s) => s.autorizarMerge);
  const reabrirTarea = useStore((s) => s.reabrirTarea);
  const puedeAvanzar = useStore((s) => s.puedeAvanzar);
  const crearTareaConSubtareas = useStore((s) => s.crearTareaConSubtareas);
  const sprints = useStore((s) => s.sprints);
  const showToast = useStore((s) => s.showToast);

  const [activeId, setActiveId] = useState<string | null>(null);
  const [pendingMove, setPendingMove] = useState<{ id: string; to: EstadoTarea } | null>(null);
  const [salto, setSalto] = useState<{ id: string; to: EstadoTarea } | null>(null);
  const [reabrirId, setReabrirId] = useState<string | null>(null);
  const [borrarProgresoId, setBorrarProgresoId] = useState<string | null>(null);
  const [tomarId, setTomarId] = useState<string | null>(null);
  const [mostrarTomarModal, setMostrarTomarModal] = useState(false);
  const [showCrear, setShowCrear] = useState(false);
  const [detalle, setDetalle] = useState<Tarea | null>(null);
  const [bloqueado, setBloqueado] = useState(false);
  const [ocultas, setOcultas] = useState<Set<ColVisual>>(new Set());
  const [resaltada, setResaltada] = useState<string | null>(null);

  // Llegada desde el ícono de tipo en Scrum: hace scroll hasta la tarea y la resalta un momento.
  useEffect(() => {
    const focusTareaId = (location.state as { focusTareaId?: string } | null)?.focusTareaId;
    if (!focusTareaId) return;
    const el = document.getElementById(`categoria-tarea-${focusTareaId}`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    setResaltada(focusTareaId);
    const t = setTimeout(() => setResaltada(null), 625);
    return () => clearTimeout(t);
  }, [location.state]);


  const toggleColumna = (id: ColVisual) =>
    setOcultas((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  // El arrastre empieza al mover 12px con el botón presionado (antes 6): un click con un pequeño
  // movimiento sin querer ya no se toma como arrastre y sí abre los detalles.
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 12 } }));

  const items = useMemo(
    () => tareasAll.filter((t) => !t.eliminada && t.tipo === tipoTarea && tareaVisibleEnTableroPrincipal(t, user)),
    [tareasAll, user, tipoTarea]
  );

  const porColumnaVisual = (col: ColVisual): Tarea[] => {
    if (col === 'sin-asignar') return items.filter((t) => t.estado === 'por hacer' && !t.asignado_a && t.id !== tomarId);
    if (col === 'por hacer') return items.filter((t) => (t.estado === 'por hacer' && !!t.asignado_a) || t.id === tomarId);
    return items.filter((t) => t.estado === col);
  };

  function onDragStart(e: DragStartEvent) {
    setActiveId(String(e.active.id));
  }

  function onDragEnd(e: DragEndEvent) {
    setActiveId(null);
    const id = String(e.active.id);
    const toCol = e.over ? (String(e.over.id) as ColVisual) : null;
    if (!toCol || !COLUMNAS.some((c) => c.id === toCol)) return;
    const tarea = tareasAll.find((t) => t.id === id);
    if (!tarea) return;

    if (toCol === 'sin-asignar') {
      if (tarea.estado === 'por hacer' && !tarea.asignado_a) return;
      desasignarTarea(id);
      return;
    }

    if (toCol === 'por hacer' && tarea.estado === 'por hacer' && !tarea.asignado_a) {
      // Solo una tarjeta "en vuelo" a la vez (ver ScrumBoard).
      if (tomarId) return;
      setTomarId(id);
      setBloqueado(true);
      setTimeout(() => {
        setBloqueado(false);
        setMostrarTomarModal(true);
      }, 200);
      return;
    }

    const to = toCol;
    if (tarea.estado === to) return;

    // Desde "Sin asignar" solo se pasa a "Por hacer"; forzar otra columna pide confirmación.
    if (tarea.estado === 'por hacer' && !tarea.asignado_a) {
      if (to === 'terminada' && !esLiderOAdmin(user.rol)) {
        showToast('No puedes mergear directo a dev — manda la tarea a revisión', 'danger');
        return;
      }
      setSalto({ id, to });
      return;
    }

    avanzarA(id, to);
  }

  // Lee la tarea del store: tras confirmar un salto desde "Sin asignar" se acaba de tomar.
  function avanzarA(id: string, to: EstadoTarea) {
    const tarea = useStore.getState().tareas.find((t) => t.id === id);
    if (!tarea) return;

    // En producción -> Por hacer con progreso: se permite, pero borra lo hecho (escribir "eliminar").
    if (to === 'por hacer' && tarea.estado === 'en producción') {
      const st = useStore.getState();
      if (progresoDe(st.tareas, st.commits, id).length > 0) {
        setBorrarProgresoId(id);
        return;
      }
    }
    // Reabrir (terminada -> en revisión): candado "reabrir" con motivo, y tope de reaperturas.
    if (tarea.estado === 'terminada' && to === 'en revisión') {
      if (!puedeReabrir(tarea, user.rol)) {
        showToast(
          `Esta tarea ya se reabrió ${LIMITE_REAPERTURAS} veces — solo el Administrador puede reabrirla otra vez`,
          'danger'
        );
        return;
      }
      setReabrirId(id);
      return;
    }

    if (to === 'terminada' && !esLiderOAdmin(user.rol)) {
      showToast('No puedes mergear directo a dev — manda la tarea a revisión', 'danger');
      return;
    }

    // Líder/Administrador: mergear a dev pide el mismo candado "autorizar" que la pantalla Revisión.
    if (to === 'terminada') {
      if (puedeAvanzar(id, to)) setPendingMove({ id, to });
      return;
    }

    if (to === 'en revisión' && !esLiderOAdmin(user.rol)) {
      if (tarea.pendiente_aprobacion) {
        showToast('Esta tarea está pendiente de aprobación — no se puede mandar a revisión', 'danger');
        return;
      }
      if (puedeAvanzar(id, to)) setPendingMove({ id, to });
      return;
    }

    moverTarea(id, to);
  }

  const colorTipo = tipoMeta[tipoTarea].color;
  const activeTask = activeId ? tareasAll.find((t) => t.id === activeId) : null;
  const esDueno = (t: Tarea) => t.asignado_a === user.email;
  const puedeAutorizarPropia = user.rol === 'Administrador' || user.rol === 'Líder';

  return (
    <div>
      {bloqueado && <div className="fixed inset-0 z-[60] cursor-wait" />}
      <PageHeader
        title={`Categoría: ${tipoTarea}`}
        desc="Tablero Scrum solo con las tareas de esta categoría — arrastra para cambiar su estado."
        actions={
          <button className="btn-outline btn-sm" onClick={() => navigate('/categorias')}>
            ← Volver a categorías
          </button>
        }
      />

      <div className="card p-5" style={{ background: `${colorTipo}0d`, border: `1px solid ${colorTipo}55` }}>
        {/* Interruptores de columnas visibles — no a todos le interesan todos los estados */}
        <div className="mb-4 flex flex-wrap items-center gap-1.5">
          {COLUMNAS.map((col) => {
            const activa = !ocultas.has(col.id);
            return (
              <button
                key={col.id}
                onClick={() => toggleColumna(col.id)}
                title={activa ? `Ocultar "${col.label}"` : `Mostrar "${col.label}"`}
                className={`chip border transition ${
                  activa
                    ? 'border-border bg-surface-2 text-text'
                    : 'border-border/40 bg-transparent text-faint opacity-60'
                }`}
              >
                <svg
                  width="13"
                  height="13"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ color: activa ? col.dot : '#475569' }}
                >
                  <path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7-11-7-11-7z" />
                  {activa ? <circle cx="12" cy="12" r="3" /> : <line x1="2" y1="2" x2="22" y2="22" />}
                </svg>
                {col.label}
              </button>
            );
          })}
        </div>

        <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd}>
          <div className="flex gap-4 overflow-x-auto pb-2">
            {COLUMNAS.filter((col) => !ocultas.has(col.id)).map((col) => {
              const colItems = porColumnaVisual(col.id);
              return (
                <Column key={col.id} col={col} count={colItems.length}>
                  {col.id === 'sin-asignar' && (
                    <button
                      type="button"
                      onClick={() => setShowCrear(true)}
                      className="flex w-full items-center justify-center gap-1.5 rounded-xl border border-dashed border-border-strong/50 bg-surface-2/40 py-2.5 text-sm font-medium text-muted transition hover:border-primary hover:text-primary"
                    >
                      + Nueva tarea
                    </button>
                  )}
                  {colItems.map((t) => {
                    const padre = t.tarea_padre_id ? tareasAll.find((x) => x.id === t.tarea_padre_id) : undefined;
                    const footer = (
                      <div>
                        {padre && <p className="mb-1.5 text-xs text-faint">↳ subtarea de {padre.titulo}</p>}
                        <div className="flex flex-wrap items-center gap-1.5">
                          <button className="btn-ghost btn-sm" onClick={() => setDetalle(t)}>
                            Ver detalle
                          </button>
                          {!t.asignado_a && t.estado === 'por hacer' && (
                            <button
                              className="btn-outline btn-sm"
                              onClick={() => { setTomarId(t.id); setMostrarTomarModal(true); }}
                            >
                              🙋 Tomar tarea
                            </button>
                          )}
                          {t.estado === 'en producción' && esDueno(t) && puedeAutorizarPropia && (
                            <button
                              className="btn-primary btn-sm"
                              onClick={() => puedeAvanzar(t.id, 'terminada') && setPendingMove({ id: t.id, to: 'terminada' })}
                            >
                              ✓ Autorizar (merge a dev)
                            </button>
                          )}
                        </div>
                      </div>
                    );
                    return (
                      <DraggableCard
                        key={t.id}
                        tarea={t}
                        footer={footer}
                        onOpenDetalle={() => setDetalle(t)}
                        ocultarResponsable={t.estado === 'por hacer'}
                        resaltada={resaltada === t.id}
                        disabled={!!tomarId}
                      />
                    );
                  })}
                  {colItems.length === 0 && col.id !== 'sin-asignar' && (
                    <div className="grid h-24 place-items-center rounded-lg border border-dashed border-border/60 text-xs text-faint">
                      Suelta tarjetas aquí
                    </div>
                  )}
                </Column>
              );
            })}
          </div>

          <DragOverlay>
            {activeTask ? (
              <div className="w-[300px] rotate-2">
                <TaskCard tarea={activeTask} />
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      </div>

      {/* Confirmación: mandar a revisar / autorizar (doble candado) */}
      <ConfirmLock
        open={!!pendingMove}
        onClose={() => setPendingMove(null)}
        onConfirm={() => {
          if (!pendingMove) return;
          if (pendingMove.to === 'terminada') autorizarMerge(pendingMove.id);
          else moverTarea(pendingMove.id, pendingMove.to);
          setPendingMove(null);
        }}
        title={pendingMove?.to === 'terminada' ? 'Autorizar merge a dev' : 'Mandar a revisión'}
        tone="primary"
        palabra={pendingMove?.to === 'terminada' ? 'autorizar' : 'mandar a revisar'}
        confirmLabel={pendingMove?.to === 'terminada' ? 'Autorizar y mergear' : 'Mandar a revisar'}
        description={
          pendingMove?.to === 'terminada'
            ? 'Vas a mergear esta rama directamente a dev. Es una acción delicada sobre Unity Version Control.'
            : 'La tarea pasará a "en revisión" y aparecerá en la carpeta "Tareas grandes" de Revisión, visible para todo el equipo. Autorizan Líder/Administrador.'
        }
      />

      {/* Salto de estados: se confirma y luego sigue el flujo normal de la columna destino */}
      <ConfirmarSaltoModal
        tarea={salto ? tareasAll.find((t) => t.id === salto.id) ?? null : null}
        to={salto?.to ?? null}
        onClose={() => setSalto(null)}
        onConfirm={() => {
          if (!salto) return;
          const t = tareasAll.find((x) => x.id === salto.id);
          if (t && t.estado === 'por hacer' && !t.asignado_a) tomarTarea(salto.id, user.email);
          setSalto(null);
          avanzarA(salto.id, salto.to);
        }}
      />

      {/* Reabrir una tarea terminada (ya mergeada a dev): candado + motivo obligatorio */}
      <ConfirmLock
        open={!!reabrirId}
        onClose={() => setReabrirId(null)}
        onConfirm={(motivo) => {
          if (reabrirId) reabrirTarea(reabrirId, motivo ?? '');
          setReabrirId(null);
        }}
        title="Reabrir tarea terminada"
        palabra="reabrir"
        confirmLabel="Reabrir"
        requiereJustificacion
        justificacionLabel="Motivo de la reapertura"
        description={
          <>
            La tarea ya se mergeó a dev y volverá a "en revisión". Se avisará a Líderes y Administradores.{' '}
            <span className="font-semibold text-text">
              Reaperturas: {tareasAll.find((t) => t.id === reabrirId)?.reaperturas ?? 0}/{LIMITE_REAPERTURAS}
            </span>
            {' '}— al llegar al tope, solo el Administrador puede reabrirla.
          </>
        }
      />

      <BorrarProgresoModal tareaId={borrarProgresoId} onClose={() => setBorrarProgresoId(null)} />

      {/* Tomar tarea: crea su rama (toda tarea se trabaja en su propia rama) */}
      <ConfirmLock
        open={mostrarTomarModal}
        onClose={() => { setMostrarTomarModal(false); setTomarId(null); }}
        onConfirm={() => {
          if (tomarId) {
            tomarTarea(tomarId, user.email);
            showToast('Rama creada y workspace cambiado — ¡listo para trabajar!');
          }
          setMostrarTomarModal(false);
          setTomarId(null);
        }}
        title="Tomar tarea"
        tone="primary"
        palabra="asignar"
        confirmLabel="Tomar y crear rama"
        description={'Al tomar la tarea, el backend crea su rama en Unity Version Control y hace el auto-switch de tu workspace.'}
      />

      {/* Nueva tarea — individual y ya con el tipo fijo en esta categoría */}
      {showCrear && (
        <CrearTareaModal
          onClose={() => setShowCrear(false)}
          onCreate={(data) => {
            crearTareaConSubtareas({ ...data, tarea_padre_id: null });
            setShowCrear(false);
          }}
          rolUsuario={user.rol}
          sprints={sprints}
          tipoFijo={tipoTarea}
        />
      )}

      {/* Detalle de tarea */}
      <DetalleTareaModal tarea={detalle} onClose={() => setDetalle(null)} />
    </div>
  );
}
