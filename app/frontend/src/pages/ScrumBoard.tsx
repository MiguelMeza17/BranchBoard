import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
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
import { useStore, LIMITE_REAPERTURAS, puedeReabrir, errorCheckinDoble } from '../store/useStore';
import { type EstadoTarea, type Tarea } from '../types';
import { estadoMeta, tipoMeta } from '../lib/ui';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { TaskCard, type EstadoVisual } from '../components/TaskCard';
import { PageHeader, EstadoBadge, Avatar } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { ConfirmarSaltoModal } from '../components/ConfirmarSaltoModal';
import { Modal } from '../components/Modal';
import { AccionesRama, CambiosTraidos, RamaDeLaTarea } from '../components/AccionesRama';
import { CrearTareaModal } from '../components/CrearTareaModal';

// Columnas del tablero: "Sin asignar" no es un estado real de la tarea — es un
// recorte puramente visual de "por hacer" (sin dueño vs ya tomada por alguien).
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
  childCount,
  onOpenFolder,
  onOpenDetalle,
  tipoResumen,
  onOpenTipo,
  estadoResumen,
  footer,
  ocultarResponsable,
  disabled,
}: {
  tarea: Tarea;
  childCount: number;
  onOpenFolder?: () => void;
  onOpenDetalle?: () => void;
  tipoResumen?: { tipo: Tarea['tipo']; count: number }[];
  onOpenTipo?: (tipo: Tarea['tipo']) => void;
  estadoResumen?: { estado: EstadoVisual; count: number }[];
  footer?: React.ReactNode;
  ocultarResponsable?: boolean;
  disabled?: boolean;
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: tarea.id, disabled });
  return (
    <div ref={setNodeRef} className={isDragging ? 'opacity-40' : ''}>
      <TaskCard
        tarea={tarea}
        childCount={childCount}
        onOpenFolder={onOpenFolder}
        onOpenDetalle={onOpenDetalle}
        tipoResumen={tipoResumen}
        onOpenTipo={onOpenTipo}
        estadoResumen={estadoResumen}
        footer={footer}
        dragHandleProps={{ ...attributes, ...listeners }}
        ocultarResponsable={ocultarResponsable}
      />
    </div>
  );
}

function Column({
  col,
  droppableId,
  children,
  count,
}: {
  col: (typeof COLUMNAS)[number];
  droppableId: string;
  children: React.ReactNode;
  count: number;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: droppableId });
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

export function ScrumBoard() {
  const navigate = useNavigate();
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

  const [focusId, setFocusId] = useState<string | null>(null); // navegación tipo carpeta
  const [activeId, setActiveId] = useState<string | null>(null);
  const [pendingMove, setPendingMove] = useState<{ id: string; to: EstadoTarea } | null>(null);
  const [salto, setSalto] = useState<{ id: string; to: EstadoTarea } | null>(null);
  const [reabrirId, setReabrirId] = useState<string | null>(null);
  const [tomarId, setTomarId] = useState<string | null>(null);
  const [mostrarTomarModal, setMostrarTomarModal] = useState(false);
  const [modoTomar, setModoTomar] = useState<'rama' | 'checkin'>('rama');
  const [showCrear, setShowCrear] = useState(false);
  const [modoForzadoCrear, setModoForzadoCrear] = useState<'general' | 'especifica'>('especifica');
  const [detalle, setDetalle] = useState<Tarea | null>(null);
  const [ocultas, setOcultas] = useState<Set<ColVisual>>(new Set());
  const [soloMias, setSoloMias] = useState(false);
  const [bloqueado, setBloqueado] = useState(false); // bloquea la UI mientras la tarjeta termina de acomodarse

  // El arrastre empieza al mover 12px con el botón presionado (antes 6): un click con un pequeño
  // movimiento sin querer ya no se toma como arrastre y sí abre los detalles.
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 12 } }));

  // Reinicia la elección rama/check-in cada vez que se abre el modal de "Tomar tarea".
  useEffect(() => {
    if (mostrarTomarModal) setModoTomar('rama');
  }, [mostrarTomarModal]);

  const tareaTomar = tomarId ? tareasAll.find((t) => t.id === tomarId) : null;
  const esSubtareaTomar = !!tareaTomar?.tarea_padre_id;
  const usuariosStore = useStore((s) => s.usuarios);
  // Solo un check-in a la vez por tarea padre: si ya hay uno en curso, este error explica por qué
  // no se puede y la tarea se toma con rama propia.
  const errorCheckin = esSubtareaTomar && tomarId ? errorCheckinDoble(tareasAll, usuariosStore, tomarId) : null;
  const modoEfectivo = errorCheckin ? 'rama' : modoTomar;

  const visibles = useMemo(
    () => tareasAll.filter((t) => !t.eliminada && tareaVisibleEnTableroPrincipal(t, user)),
    [tareasAll, user]
  );

  const childrenCount = (id: string) => visibles.filter((t) => t.tarea_padre_id === id).length;

  // Recorre todo el subárbol (hijas, nietas, etc.) de una tarea.
  const descendientes = (id: string): Tarea[] => {
    const directos = visibles.filter((t) => t.tarea_padre_id === id);
    return directos.flatMap((d) => [d, ...descendientes(d.id)]);
  };

  // Resumen de tipos que contiene por dentro una tarea con subtareas (para los íconos-botón).
  const tipoResumenDe = (id: string) => {
    const desc = descendientes(id);
    const orden: string[] = [];
    const conteo = new Map<string, number>();
    const primeraDeCadaTipo = new Map<string, Tarea>();
    for (const d of desc) {
      if (!conteo.has(d.tipo)) {
        orden.push(d.tipo);
        primeraDeCadaTipo.set(d.tipo, d);
      }
      conteo.set(d.tipo, (conteo.get(d.tipo) ?? 0) + 1);
    }
    return {
      resumen: orden.map((tipo) => ({ tipo: tipo as Tarea['tipo'], count: conteo.get(tipo)! })),
      primeraDeCadaTipo,
    };
  };

  // Cuántas subtareas directas (las que se ven al abrir la carpeta) hay en cada estado,
  // en el orden de las columnas.
  const estadoResumenDe = (id: string) => {
    const desc = visibles.filter((t) => t.tarea_padre_id === id);
    return COLUMNAS.map(({ id: col }) => ({
      estado: (col === 'sin-asignar' ? 'sin asignar' : col) as EstadoVisual,
      count: desc.filter((d) =>
        col === 'sin-asignar'
          ? d.estado === 'por hacer' && !d.asignado_a
          : d.estado === col && (col !== 'por hacer' || !!d.asignado_a)
      ).length,
    })).filter((r) => r.count > 0);
  };

  // Ícono-botón: abre esa categoría (tablero Scrum de ese tipo) y resalta la subtarea puntual.
  const abrirTipoEspecifico = (id: string, tipo: Tarea['tipo']) => {
    const { primeraDeCadaTipo } = tipoResumenDe(id);
    const match = primeraDeCadaTipo.get(tipo as string);
    navigate(`/categorias/${encodeURIComponent(tipo)}`, {
      state: match ? { focusTareaId: match.id } : undefined,
    });
  };

  // Navegación tipo explorador de carpetas — solo las subtareas van en las columnas;
  // la tarea padre se muestra aparte (franja de contexto), nunca mezclada como otra tarjeta más.
  const nivelActual = useMemo(() => {
    if (!focusId) return visibles.filter((t) => t.tarea_padre_id === null);
    return visibles.filter((t) => t.tarea_padre_id === focusId);
  }, [visibles, focusId]);

  const tareaEnFoco = useMemo(
    () => (focusId ? tareasAll.find((t) => t.id === focusId) ?? null : null),
    [focusId, tareasAll]
  );

  const breadcrumb = useMemo(() => {
    const crumbs: Tarea[] = [];
    let cur = focusId;
    while (cur) {
      const t = tareasAll.find((x) => x.id === cur);
      if (!t) break;
      crumbs.unshift(t);
      cur = t.tarea_padre_id;
    }
    return crumbs;
  }, [focusId, tareasAll]);

  // "Todas las tareas" vs "Mis tareas" (asignadas a mí) dentro del nivel actual de la carpeta.
  const nivelFiltrado = useMemo(
    () => (soloMias ? nivelActual.filter((t) => t.asignado_a === user.email) : nivelActual),
    [nivelActual, soloMias, user.email]
  );

  // Al nivel raíz (sin carpeta abierta), el tablero se divide en dos:
  // "Tareas generales" (tienen subtareas) y "Tareas individuales" (no tienen ninguna).
  const generales = useMemo(
    () => (!focusId ? nivelFiltrado.filter((t) => childrenCount(t.id) > 0) : []),
    [nivelFiltrado, focusId, visibles]
  );
  const individuales = useMemo(
    () => (!focusId ? nivelFiltrado.filter((t) => childrenCount(t.id) === 0) : []),
    [nivelFiltrado, focusId, visibles]
  );

  // "Sin asignar" y "Por hacer" recortan el mismo estado real ('por hacer') según si tiene dueño.
  // Mientras el modal "Tomar tarea" está abierto (tomarId), esa tarjeta se muestra ya en
  // "Por hacer" de forma optimista — si se cancela, el dato real no cambió y vuelve sola.
  const porColumnaVisualDe = (items: Tarea[], col: ColVisual): Tarea[] => {
    if (col === 'sin-asignar')
      return items.filter((t) => t.estado === 'por hacer' && !t.asignado_a && t.id !== tomarId);
    if (col === 'por hacer')
      return items.filter((t) => (t.estado === 'por hacer' && !!t.asignado_a) || t.id === tomarId);
    return items.filter((t) => t.estado === col);
  };

  const toggleColumna = (id: ColVisual) =>
    setOcultas((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  function onDragStart(e: DragStartEvent) {
    setActiveId(String(e.active.id));
  }

  function onDragEnd(e: DragEndEvent) {
    setActiveId(null);
    const id = String(e.active.id);
    if (!e.over) return;
    const overId = String(e.over.id);
    const toCol = overId.slice(overId.indexOf(':') + 1) as ColVisual;
    if (!COLUMNAS.some((c) => c.id === toCol)) return;
    const tarea = tareasAll.find((t) => t.id === id);
    if (!tarea) return;

    // "Sin asignar" es visual: soltar aquí libera la tarea, no es un estado real.
    if (toCol === 'sin-asignar') {
      if (tarea.estado === 'por hacer' && !tarea.asignado_a) return; // ya estaba ahí
      desasignarTarea(id);
      return;
    }

    // "Por hacer" también es el mismo estado real que "Sin asignar" — soltar aquí desde
    // "Sin asignar" mueve la tarjeta a "Por hacer" de inmediato (se anima el acomodo) y
    // bloquea la interfaz; recién al terminar la animación aparece el modal de
    // confirmación, el mismo que usa el botón "Tomar tarea". Si se cancela, como el dato
    // real nunca cambió, la tarjeta vuelve sola a "Sin asignar".
    if (toCol === 'por hacer' && tarea.estado === 'por hacer' && !tarea.asignado_a) {
      // Solo una tarjeta "en vuelo" a la vez: si otra ya espera su confirmación,
      // sobrescribir tomarId haría que esa primera regresara sola a "Sin asignar".
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

    // Desde "Sin asignar" lo normal es pasar solo a "Por hacer" (ya tratado arriba). Forzarla a
    // otra columna pide confirmar el salto; al confirmarlo sigue el flujo normal (avanzarA).
    if (tarea.estado === 'por hacer' && !tarea.asignado_a) {
      if (to === 'terminada' && (user.rol === 'Desarrollador' || user.rol === 'Tester')) {
        showToast('No puedes mergear directo a dev — manda la tarea a revisión', 'danger');
        return;
      }
      setSalto({ id, to });
      return;
    }

    avanzarA(id, to);
  }

  // Flujo al soltar en una columna real. Lee la tarea del store y no del render porque, tras
  // confirmar un salto desde "Sin asignar", se acaba de tomar en ese mismo click.
  function avanzarA(id: string, to: EstadoTarea) {
    const tarea = useStore.getState().tareas.find((t) => t.id === id);
    if (!tarea) return;

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

    // Desarrollador/Tester no pueden mandar a "terminada" directo — solo vía revisión
    if (to === 'terminada' && (user.rol === 'Desarrollador' || user.rol === 'Tester')) {
      showToast('No puedes mergear directo a dev — manda la tarea a revisión', 'danger');
      return;
    }

    // Líder/Administrador: mergear a dev pide el mismo candado "autorizar" que la pantalla Revisión.
    if (to === 'terminada') {
      if (puedeAvanzar(id, to)) setPendingMove({ id, to });
      return;
    }

    // Mandar a revisión: doble candado ("mandar a revisar") para Desarrollador/Tester
    if (to === 'en revisión' && (user.rol === 'Desarrollador' || user.rol === 'Tester')) {
      if (tarea.pendiente_aprobacion) {
        showToast('Esta tarea está pendiente de aprobación — no se puede mandar a revisión', 'danger');
        return;
      }
      if (puedeAvanzar(id, to)) setPendingMove({ id, to });
      return;
    }

    moverTarea(id, to);
  }

  const activeTask = activeId ? tareasAll.find((t) => t.id === activeId) : null;
  const esDueno = (t: Tarea) => t.asignado_a === user.email;
  const puedeAutorizarPropia = user.rol === 'Administrador' || user.rol === 'Líder';

  function renderCard(t: Tarea) {
    const nHijos = childrenCount(t.id);
    const footer = (
      <div className="flex flex-wrap items-center gap-1.5">
        <button className="btn-ghost btn-sm" onClick={() => setDetalle(t)}>
          Ver detalle
        </button>
        {/* Tomar tarea (crea rama) */}
        {!t.asignado_a && t.estado === 'por hacer' && (
          <button
            className="btn-outline btn-sm"
            onClick={() => { setTomarId(t.id); setMostrarTomarModal(true); }}
          >
            🙋 Tomar tarea
          </button>
        )}
        {/* Líder/Admin: autorizar su propia tarea directamente */}
        {t.estado === 'en producción' && esDueno(t) && puedeAutorizarPropia && (
          <button
            className="btn-primary btn-sm"
            onClick={() => puedeAvanzar(t.id, 'terminada') && setPendingMove({ id: t.id, to: 'terminada' })}
          >
            ✓ Autorizar (merge a dev)
          </button>
        )}
      </div>
    );
    return (
      <DraggableCard
        key={t.id}
        tarea={t}
        childCount={nHijos}
        onOpenFolder={() => setFocusId(t.id)}
        onOpenDetalle={nHijos === 0 ? () => setDetalle(t) : undefined}
        tipoResumen={nHijos > 0 ? tipoResumenDe(t.id).resumen : undefined}
        onOpenTipo={(tipo) => abrirTipoEspecifico(t.id, tipo)}
        estadoResumen={nHijos > 0 ? estadoResumenDe(t.id) : undefined}
        footer={footer}
        ocultarResponsable={t.estado === 'por hacer'}
        disabled={!!tomarId}
      />
    );
  }

  function renderTablero(items: Tarea[], grupoKey: string, modoForzadoAqui: 'general' | 'especifica') {
    return (
      <div className="flex gap-4 overflow-x-auto pb-2">
        {COLUMNAS.filter((col) => !ocultas.has(col.id)).map((col) => {
          const colItems = porColumnaVisualDe(items, col.id);
          return (
            <Column
              key={`${grupoKey}-${col.id}`}
              col={col}
              count={colItems.length}
              droppableId={`${grupoKey}:${col.id}`}
            >
              {col.id === 'sin-asignar' && (
                <button
                  type="button"
                  onClick={() => {
                    setModoForzadoCrear(modoForzadoAqui);
                    setShowCrear(true);
                  }}
                  className="flex w-full items-center justify-center gap-1.5 rounded-xl border border-dashed border-border-strong/50 bg-surface-2/40 py-2.5 text-sm font-medium text-muted transition hover:border-primary hover:text-primary"
                >
                  + Nueva tarea
                </button>
              )}
              {colItems.map((t) => renderCard(t))}
              {colItems.length === 0 && col.id !== 'sin-asignar' && (
                <div className="grid h-24 place-items-center rounded-lg border border-dashed border-border/60 text-xs text-faint">
                  Suelta tarjetas aquí
                </div>
              )}
            </Column>
          );
        })}
      </div>
    );
  }

  return (
    <div>
      {bloqueado && <div className="fixed inset-0 z-[60] cursor-wait" />}
      {tareaEnFoco ? (
        <PageHeader
          title={tareaEnFoco.titulo}
          desc={`${tareaEnFoco.id} · viendo sus subtareas`}
          actions={
            <div className="flex flex-wrap items-center gap-2">
              <div className="flex flex-wrap items-center gap-1.5">
                {tipoResumenDe(tareaEnFoco.id).resumen.map(({ tipo, count }) => {
                  const m = tipoMeta[tipo];
                  return (
                    <button
                      key={tipo}
                      onClick={() => abrirTipoEspecifico(tareaEnFoco.id, tipo)}
                      className="chip border transition hover:brightness-125"
                      style={{ color: m.color, borderColor: `${m.color}55`, background: `${m.color}18` }}
                      title={`Ir a la subtarea de ${tipo}`}
                    >
                      <span>{m.emoji}</span>
                      {count > 1 && <span className="font-semibold">{count}</span>}
                    </button>
                  );
                })}
              </div>
              <EstadoBadge estado={tareaEnFoco.estado} />
              {tareaEnFoco.asignado_a && <Avatar email={tareaEnFoco.asignado_a} size={22} />}
              <button className="btn-outline btn-sm" onClick={() => setFocusId(tareaEnFoco.tarea_padre_id)}>
                ← Volver
              </button>
            </div>
          }
        />
      ) : (
        <PageHeader
          title="Tablero Scrum"
          desc="Arrastra las tarjetas para cambiar su estado."
          actions={
            <span className="chip border border-border bg-surface-2 text-muted">
              {visibles.filter((t) => t.tarea_padre_id === null).length} tareas raíz
            </span>
          }
        />
      )}

      <div className="card p-5">
        {/* Breadcrumb navegación tipo carpeta + interruptores de columnas visibles */}
        <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 text-sm">
            {/* Siempre con fondo, para que se lea como botón; al pasar el mouse se aclara más. */}
            <button
              onClick={() => setFocusId(null)}
              className={`rounded-md border border-border bg-surface-3 px-2 py-1 transition hover:border-border-strong hover:bg-border-strong hover:text-text ${
                !focusId ? 'font-semibold text-text' : 'text-muted'
              }`}
            >
              🗂️ Tareas
            </button>
            <div className="flex items-center gap-0.5 rounded-lg border border-border bg-surface-2 p-0.5">
              <button
                onClick={() => setSoloMias(false)}
                className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                  !soloMias ? 'bg-surface-3 text-text' : 'text-faint hover:text-muted'
                }`}
              >
                Todas las tareas
              </button>
              <button
                onClick={() => setSoloMias(true)}
                className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                  soloMias ? 'bg-surface-3 text-text' : 'text-faint hover:text-muted'
                }`}
              >
                Mis tareas
              </button>
            </div>
            {breadcrumb.map((c) => (
              <span key={c.id} className="flex items-center gap-1.5">
                <span className="text-faint">/</span>
                <button
                  onClick={() => setFocusId(c.id)}
                  className="rounded-md px-2 py-1 text-muted transition hover:bg-surface-3 hover:text-text"
                >
                  {c.titulo}
                </button>
              </span>
            ))}
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
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
        </div>

        <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd}>
          {focusId ? (
            renderTablero(nivelFiltrado, 'focus', 'especifica')
          ) : (
            <div className="space-y-6">
              <div className="rounded-2xl border border-border border-l-4 border-l-primary bg-surface-2 p-4">
                <h3 className="mb-3 flex items-center gap-2 text-base font-semibold text-text">
                  📦 Tareas generales
                  <span className="rounded-full bg-primary-soft px-2 py-0.5 text-xs font-normal text-indigo-200">
                    {generales.length}
                  </span>
                </h3>
                {renderTablero(generales, 'generales', 'general')}
              </div>
              <div className="rounded-2xl border border-border border-l-4 border-l-accent bg-surface-2 p-4">
                <h3 className="mb-3 flex items-center gap-2 text-base font-semibold text-text">
                  👤 Tareas individuales
                  <span className="rounded-full bg-accent-soft px-2 py-0.5 text-xs font-normal text-teal-200">
                    {individuales.length}
                  </span>
                </h3>
                {renderTablero(individuales, 'individuales', 'especifica')}
              </div>
            </div>
          )}

          <DragOverlay>
            {activeTask ? (
              <div className="w-[300px] rotate-2">
                <TaskCard
                  tarea={activeTask}
                  childCount={childrenCount(activeTask.id)}
                  tipoResumen={
                    childrenCount(activeTask.id) > 0 ? tipoResumenDe(activeTask.id).resumen : undefined
                  }
                  estadoResumen={childrenCount(activeTask.id) > 0 ? estadoResumenDe(activeTask.id) : undefined}
                />
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
        tone={pendingMove?.to === 'terminada' ? 'primary' : 'primary'}
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
          if (t && t.estado === 'por hacer' && !t.asignado_a) tomarTarea(salto.id, user.email, 'rama');
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

      {/* Tomar tarea (crea rama, o — solo para subtareas — check-in directo sobre la rama padre) */}
      <ConfirmLock
        open={mostrarTomarModal}
        onClose={() => { setMostrarTomarModal(false); setTomarId(null); }}
        onConfirm={() => {
          if (tomarId) {
            tomarTarea(tomarId, user.email, esSubtareaTomar ? modoEfectivo : 'rama');
            showToast(
              esSubtareaTomar && modoEfectivo === 'checkin'
                ? 'Check-in habilitado sobre la rama de la tarea padre — ¡listo para trabajar!'
                : 'Rama creada y workspace cambiado — ¡listo para trabajar!'
            );
          }
          setMostrarTomarModal(false);
          setTomarId(null);
        }}
        title="Tomar tarea"
        tone="primary"
        palabra="asignar"
        confirmLabel={esSubtareaTomar && modoEfectivo === 'checkin' ? 'Tomar con check-in' : 'Tomar y crear rama'}
        description={
          <div className="space-y-3">
            <p>
              {esSubtareaTomar && modoEfectivo === 'checkin'
                ? 'El check-in se hará directo sobre la rama de la tarea padre — no se crea una rama nueva ni se cambia el workspace.'
                : 'Al tomar la tarea, el backend crea su rama en Unity Version Control y hace el auto-switch de tu workspace. Podrás abrirla en Unity desde la pantalla de éxito.'}
            </p>
            {esSubtareaTomar && (
              <div className="flex items-center gap-0.5 rounded-lg border border-border bg-surface-2 p-0.5">
                <button
                  type="button"
                  onClick={() => setModoTomar('rama')}
                  className={`flex-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition ${
                    modoEfectivo === 'rama' ? 'bg-surface-3 text-text' : 'text-faint hover:text-muted'
                  }`}
                >
                  🌿 Rama propia
                </button>
                <button
                  type="button"
                  onClick={() => setModoTomar('checkin')}
                  disabled={!!errorCheckin}
                  title={errorCheckin ?? undefined}
                  className={`flex-1 rounded-md px-2.5 py-1.5 text-xs font-medium transition disabled:cursor-not-allowed disabled:opacity-40 ${
                    modoEfectivo === 'checkin' ? 'bg-surface-3 text-text' : 'text-faint hover:text-muted'
                  }`}
                >
                  ✅ Check-in directo
                </button>
              </div>
            )}
            {errorCheckin && (
              <p className="rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-red-200">{errorCheckin}</p>
            )}
          </div>
        }
      />

      {/* Nueva tarea — en la raíz crea una tarea raíz; dentro de una carpeta, una subtarea de ella */}
      {showCrear && (
        <CrearTareaModal
          onClose={() => setShowCrear(false)}
          onCreate={(data) => {
            crearTareaConSubtareas({ ...data, tarea_padre_id: focusId });
            setShowCrear(false);
          }}
          rolUsuario={user.rol}
          sprints={sprints}
          modoForzado={modoForzadoCrear}
        />
      )}

      {/* Detalle de tarea */}
      <Modal
        open={!!detalle}
        onClose={() => setDetalle(null)}
        title={detalle?.titulo ?? ''}
        subtitle={detalle ? `${detalle.id} · ${detalle.tipo}` : ''}
        size="md"
        // Abrir en Unity (directo al workspace de esta tarea) y Traer cambios de otras ramas/check-ins.
        footer={detalle ? <AccionesRama tarea={detalle} onCerrar={() => setDetalle(null)} /> : undefined}
      >
        {detalle && (
          <div className="space-y-4 text-sm">
            {/* Arriba: su rama o "Sin rama"; abajo, en el pie, solo los botones. */}
            <RamaDeLaTarea tarea={detalle} />
            <CambiosTraidos tarea={detalle} />
            <div>
              <dt className="mb-1 text-xs text-faint">Detalles</dt>
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
                <dt className="text-xs text-faint">Estado</dt>
                <dd className="text-text">{estadoMeta[detalle.estado].label}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Rama</dt>
                {detalle.modo_trabajo === 'checkin' ? (
                  <dd className="text-accent">
                    ✅ Check-in directo
                    {(() => {
                      const padre = tareasAll.find((x) => x.id === detalle.tarea_padre_id);
                      return padre?.rama_asociada ? (
                        <span className="text-muted"> — sobre <span className="font-mono">{padre.rama_asociada}</span></span>
                      ) : null;
                    })()}
                  </dd>
                ) : (
                  <dd className="font-mono text-accent">{detalle.rama_asociada ?? '— sin rama —'}</dd>
                )}
              </div>
              <div>
                <dt className="text-xs text-faint">Asignado a</dt>
                <dd className="text-text">{detalle.asignado_a ?? 'Sin asignar'}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Creado por</dt>
                <dd className="text-text">{detalle.creado_por}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Fecha límite</dt>
                <dd className="text-text">{detalle.fecha_limite ?? 'Sin fecha'}</dd>
              </div>
              <div>
                <dt className="text-xs text-faint">Pendiente aprobación</dt>
                <dd className="text-text">{detalle.pendiente_aprobacion ? 'Sí' : 'No'}</dd>
              </div>
            </dl>
          </div>
        )}
      </Modal>
    </div>
  );
}
