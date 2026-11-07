import { useNavigate } from 'react-router-dom';
import type { EstadoTarea, Tarea, TipoTarea } from '../types';
import { TipoBadge, Avatar } from './primitives';
import { tipoMeta, estadoMeta } from '../lib/ui';
import { LIMITE_REAPERTURAS, useStore } from '../store/useStore';
import { Icono } from './Icono';

/** "Sin asignar" es el recorte visual de 'por hacer' sin dueño, igual que la columna del tablero. */
export type EstadoVisual = 'sin asignar' | EstadoTarea;

const estadoVisualMeta = (e: EstadoVisual) =>
  e === 'sin asignar' ? { label: 'Sin asignar', dot: '#94a3b8' } : estadoMeta[e];

interface Props {
  tarea: Tarea;
  childCount?: number;
  onOpenFolder?: () => void;
  /** Click en cualquier parte "neutra" de la tarjeta (todo excepto categoría, dueño, footer) — igual que "Ver detalle". */
  onOpenDetalle?: () => void;
  /** Resumen de los tipos que contiene por dentro (recursivo), solo si tiene subtareas. */
  tipoResumen?: { tipo: TipoTarea; count: number }[];
  onOpenTipo?: (tipo: TipoTarea) => void;
  /** Cantidad de subtareas directas por estado, solo si tiene subtareas. */
  estadoResumen?: { estado: EstadoVisual; count: number }[];
  footer?: React.ReactNode;
  dragHandleProps?: Record<string, unknown>;
  compact?: boolean;
  /** Oculta el avatar del responsable en la tarjeta (ej. columna "Por hacer") — sigue visible en "Ver detalle". */
  ocultarResponsable?: boolean;
}

export function TaskCard({
  tarea,
  childCount = 0,
  onOpenFolder,
  onOpenDetalle,
  tipoResumen,
  onOpenTipo,
  estadoResumen,
  footer,
  dragHandleProps,
  compact,
  ocultarResponsable,
}: Props) {
  const navigate = useNavigate();
  const accent = tipoMeta[tarea.tipo].color;
  const tiposHijas = childCount > 0 ? tipoResumen ?? [] : [];
  const estado = estadoMeta[tarea.estado];
  // Pending changes de su rama (En espera: la de su padre). Se ve en la tarjeta sin abrirla.
  const pendientes = useStore((s) => {
    const rama =
      tarea.rama_asociada ??
      (tarea.tipo === 'Sin definir' ? s.tareas.find((t) => t.id === tarea.tarea_padre_id)?.rama_asociada : null);
    return rama ? s.cambiosSinCheckin[rama] ?? 0 : 0;
  });
  return (
    <div
      className={`group relative cursor-grab rounded-xl border bg-surface-2 shadow-card transition-all duration-200 active:cursor-grabbing hover:border-border-strong hover:shadow-pop ${
        tarea.edicion_pendiente ? 'border-warn/70 shadow-edit-pending' : 'border-border'
      } ${childCount > 0 || onOpenDetalle ? 'cursor-pointer' : ''}`}
      onClick={(e) => {
        // Con subtareas: el click abre su Scrum interno (carpeta), como siempre.
        if (childCount > 0) {
          onOpenFolder?.();
          return;
        }
        // Sin subtareas: click en cualquier punto abre los detalles, también en
        // espacios vacíos. Solo si cayó en un control de adentro (botón, enlace…), manda ese control.
        const control = (e.target as HTMLElement).closest('button, a, input, select, textarea, label');
        if (control && control !== e.currentTarget && e.currentTarget.contains(control)) return;
        onOpenDetalle?.();
      }}
      {...dragHandleProps}
    >
      {/* type accent stripe: un solo color, o un color por cada tipo de subtarea que contiene */}
      {tiposHijas.length > 0 ? (
        <div className="absolute left-0 top-3 bottom-3 w-1 flex flex-col">
          {tiposHijas.map(({ tipo }) => (
            <span
              key={tipo}
              className="flex-1 first:rounded-t-full last:rounded-b-full"
              style={{ background: tipoMeta[tipo].color, boxShadow: `-3px 0 6px 0 ${tipoMeta[tipo].color}99` }}
            />
          ))}
        </div>
      ) : (
        <span
          className="absolute left-0 top-3 bottom-3 w-1 rounded-full"
          style={{ background: accent, boxShadow: `-3px 0 6px 0 ${accent}99` }}
        />
      )}
      <div className={`pl-4 pr-3 ${compact ? 'py-2.5' : 'py-3'}`}>
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 flex-1">
            <div className="mb-1.5 flex flex-wrap items-center gap-1.5" onClick={childCount > 0 ? (e) => e.stopPropagation() : undefined}>
              <span className="font-mono text-[11px] text-faint">{tarea.id}</span>
              {tarea.pendiente_aprobacion && (
                <button
                  className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300 transition hover:brightness-125"
                  title="Pendiente de aprobación — ir a Pendientes"
                  onClick={() => navigate('/pendientes', { state: { focusTareaId: tarea.id } })}
                >
                  <Icono nombre="reloj" className="h-3.5 w-3.5" sinColor />
                </button>
              )}
              {tarea.edicion_pendiente && (
                <span className="chip border border-warn/40 bg-yellow-500/15 text-yellow-300">
                  <Icono nombre="lapiz" className="h-3.5 w-3.5" sinColor /> Edición pendiente
                </span>
              )}
              {pendientes > 0 && tarea.estado !== 'terminada' && (
                <span
                  className="chip border border-amber-500/40 bg-amber-500/10 text-amber-300"
                  title="Cambios sin check-in en la rama (pending changes)"
                >
                  ⇡ {pendientes} sin check-in
                </span>
              )}
              {(tarea.reaperturas ?? 0) > 0 && (
                <span
                  className={`chip border ${
                    (tarea.reaperturas ?? 0) >= LIMITE_REAPERTURAS
                      ? 'border-red-500/40 bg-red-500/15 text-red-300'
                      : 'border-border bg-surface-3 text-muted'
                  }`}
                  title="Veces que volvió de terminada a revisión — al llegar al tope solo el Administrador puede reabrirla"
                >
                  ↩️ Reabierta {tarea.reaperturas}/{LIMITE_REAPERTURAS}
                </span>
              )}
            </div>
            <h4
              className={`leading-snug text-text ${
                childCount > 0 ? 'text-base font-bold' : 'text-sm font-semibold'
              }`}
            >
              {tarea.titulo}
            </h4>
            {tarea.rama_asociada && (
              <div className="mt-1 flex items-center gap-1.5">
                <span className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: estado.dot }} />
                <span className="text-[11px] text-muted">{estado.label}</span>
              </div>
            )}
            {childCount > 0 && estadoResumen && estadoResumen.length > 0 && (
              <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
                {estadoResumen.map(({ estado: e, count }) => {
                  const m = estadoVisualMeta(e);
                  return (
                    <span
                      key={e}
                      className="chip border"
                      style={{ color: m.dot, borderColor: `${m.dot}55`, background: `${m.dot}18` }}
                      title={`${count} ${m.label.toLowerCase()}`}
                    >
                      <span className="h-1.5 w-1.5 rounded-full" style={{ background: m.dot }} />
                      <span className="font-semibold">{count}</span>
                      <span>{m.label}</span>
                    </span>
                  );
                })}
              </div>
            )}
          </div>
          {tarea.asignado_a && !ocultarResponsable && (
            <span onClick={childCount > 0 ? (e) => e.stopPropagation() : undefined}>
              <Avatar email={tarea.asignado_a} size={24} />
            </span>
          )}
        </div>

        <div className="mt-2.5 flex flex-wrap items-center gap-1.5" onClick={childCount > 0 ? (e) => e.stopPropagation() : undefined}>
          {childCount > 0 ? (
            tipoResumen?.map(({ tipo, count }) => {
              const m = tipoMeta[tipo];
              return (
                <button
                  key={tipo}
                  onClick={() => onOpenTipo?.(tipo)}
                  className="chip border transition hover:brightness-125"
                  style={{ color: m.color, borderColor: `${m.color}55`, background: `${m.color}18` }}
                  title={`Ir a la subtarea de ${tipo}`}
                >
                  <Icono nombre={m.icono} className="h-3.5 w-3.5" sinColor />
                  <span className="font-semibold">{count}</span>
                </button>
              );
            })
          ) : (
            <TipoBadge tipo={tarea.tipo} />
          )}
          {tarea.estado === 'en revisión' && tarea.lider_revision_asignado && (
            <span className="chip border border-fuchsia-500/30 bg-fuchsia-500/10 text-fuchsia-300">
              Revisa: {tarea.lider_revision_asignado.split('@')[0]}
            </span>
          )}
        </div>

        {footer && (
          <div className="mt-3 border-t border-border/60 pt-2.5" onClick={childCount > 0 ? (e) => e.stopPropagation() : undefined}>
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
