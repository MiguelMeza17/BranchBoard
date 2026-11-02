import { useEffect, useRef, useState } from 'react';
import { TIPOS_TAREA, type TipoTarea } from '../types';
import { tipoMeta } from '../lib/ui';
import { Modal } from './Modal';

interface SubtareaEntry {
  uid: string;
  tipo: TipoTarea;
  titulo: string;
  detalles: string;
  guardada: boolean;
}

export interface NuevaTareaData {
  titulo: string;
  detalles: string;
  tipo: TipoTarea;
  sprint_id: string | null;
  con_fecha: boolean;
  fecha_limite: string | null;
  subtareas: { tipo: TipoTarea; titulo: string; detalles: string }[];
}

export function CrearTareaModal({
  onClose,
  onCreate,
  rolUsuario,
  sprints,
  tipoFijo,
  modoForzado,
  sprintInicial,
}: {
  onClose: () => void;
  onCreate: (data: NuevaTareaData) => void;
  rolUsuario: string;
  sprints: { id: string; nombre: string }[];
  /** Si se da, se salta la elección general/específica y el tipo queda fijo (ej. crear desde una categoría). */
  tipoFijo?: TipoTarea;
  /** Si se da, se salta la elección y el modo queda fijo en ese valor (ej. el botón de la sección
   * "Tareas generales" solo debe crear generales; el de "Tareas individuales" o dentro de una
   * carpeta solo debe crear específicas — una tarea general no puede contener otra general). */
  modoForzado?: 'general' | 'especifica';
  /** Sprint elegido de entrada (ej. al crear desde el tablero de un sprint). */
  sprintInicial?: string;
}) {
  const modoInicial = tipoFijo ? 'especifica' : modoForzado ?? 'elegir';
  const saltaElegir = modoInicial !== 'elegir';
  const [modo, setModo] = useState<'elegir' | 'general' | 'especifica'>(modoInicial);

  const [titulo, setTitulo] = useState('');
  const [detalles, setDetalles] = useState('');
  const [tipo, setTipo] = useState<TipoTarea>(tipoFijo ?? 'Scripting');
  const [subtareas, setSubtareas] = useState<SubtareaEntry[]>([]);
  const subUidRef = useRef(0);
  const [conFecha, setConFecha] = useState(false);
  const [fecha, setFecha] = useState('');
  const [sprintId, setSprintId] = useState<string>(sprintInicial ?? '');
  const [avisoLeido, setAvisoLeido] = useState(false);

  const necesitaAviso = rolUsuario === 'Desarrollador' || rolUsuario === 'Tester';
  const [mostrarAviso, setMostrarAviso] = useState(false);
  const [mostrarErrorMinimas, setMostrarErrorMinimas] = useState(false);

  // Subtareas: se agregan de a una (con su tipo), se editan y se "guardan" — al
  // guardarse se colapsan a solo su ícono (el nombre completo queda en el title/tooltip).
  // Como ahora son una lista y no un Set por tipo, una misma categoría puede repetirse.
  const agregarSubtarea = (tp: TipoTarea) => {
    const uid = `s${subUidRef.current++}`;
    setSubtareas((prev) => [...prev, { uid, tipo: tp, titulo: '', detalles: '', guardada: false }]);
  };
  const editarSubtarea = (uid: string, campos: Partial<Pick<SubtareaEntry, 'titulo' | 'detalles'>>) =>
    setSubtareas((prev) => prev.map((s) => (s.uid === uid ? { ...s, ...campos } : s)));
  const guardarSubtarea = (uid: string) =>
    setSubtareas((prev) => prev.map((s) => (s.uid === uid && s.titulo.trim() ? { ...s, guardada: true } : s)));
  const reabrirSubtarea = (uid: string) =>
    setSubtareas((prev) => prev.map((s) => (s.uid === uid ? { ...s, guardada: false } : s)));
  const quitarSubtarea = (uid: string) => setSubtareas((prev) => prev.filter((s) => s.uid !== uid));

  const MIN_SUBTAREAS_GENERAL = 2;
  const esGeneral = modo === 'general';
  // Con un solo hijo no es "general" — eso es una tarea específica con una subtarea suelta.
  // No bloquea el botón (eso se avisa recién al intentar crear, no mientras se arma la tarea).
  const faltanMinimas = esGeneral && subtareas.length < MIN_SUBTAREAS_GENERAL;
  const faltanPorGuardar = esGeneral && subtareas.some((s) => !s.guardada);
  const puedeCrear = !!titulo.trim() && !faltanPorGuardar;

  // En cuanto se resuelve (agregó otra subtarea), se oculta el aviso de error.
  useEffect(() => {
    if (!faltanMinimas) setMostrarErrorMinimas(false);
  }, [faltanMinimas]);

  function crear() {
    onCreate({
      titulo: titulo.trim(),
      detalles: detalles.trim(),
      tipo,
      sprint_id: sprintId || null,
      con_fecha: conFecha,
      fecha_limite: conFecha && fecha ? fecha : null,
      subtareas: esGeneral
        ? subtareas.map((s) => ({ tipo: s.tipo, titulo: s.titulo.trim(), detalles: s.detalles.trim() }))
        : [],
    });
  }

  function submit() {
    if (!puedeCrear) return;
    if (faltanMinimas) {
      setMostrarErrorMinimas(true);
      return;
    }
    // Dev/Tester: mostrar aviso de "pendiente de aprobación" antes de guardar
    if (necesitaAviso && !avisoLeido) {
      setMostrarAviso(true);
      return;
    }
    crear();
  }

  return (
    <>
      <Modal
        open={!mostrarAviso}
        onClose={onClose}
        title="Nueva tarea"
        subtitle={
          modo === 'elegir'
            ? '¿Qué tipo de tarea quieres crear?'
            : 'La tarea es informativa hasta que alguien la tome — ahí se crea la rama.'
        }
        size={esGeneral ? 'lg' : 'md'}
        footer={
          modo === 'elegir' ? (
            <button className="btn-ghost" onClick={onClose}>Cancelar</button>
          ) : (
            <>
              {!saltaElegir && (
                <button className="btn-ghost" onClick={() => setModo('elegir')}>← Atrás</button>
              )}
              <button className="btn-primary" onClick={submit} disabled={!puedeCrear}>
                Crear tarea
              </button>
            </>
          )
        }
      >
        {modo === 'elegir' && (
          <div className="grid grid-cols-2 gap-3">
            <button
              type="button"
              onClick={() => setModo('general')}
              className="card flex flex-col items-start gap-2 p-4 text-left transition-all hover:border-border-strong hover:shadow-pop"
            >
              <span className="text-2xl">📦</span>
              <span className="font-semibold text-text">Tarea general</span>
              <span className="text-xs text-muted">Se reparte en subtareas de distintos tipos.</span>
            </button>
            <button
              type="button"
              onClick={() => setModo('especifica')}
              className="card flex flex-col items-start gap-2 p-4 text-left transition-all hover:border-border-strong hover:shadow-pop"
            >
              <span className="text-2xl">📄</span>
              <span className="font-semibold text-text">Tarea específica</span>
              <span className="text-xs text-muted">Una sola tarea, de un solo tipo/disciplina.</span>
            </button>
          </div>
        )}

        {modo !== 'elegir' && (
          <div className="space-y-4">
            <div>
              <label className="label">Título {esGeneral && <span className="text-faint">(tarea general)</span>}</label>
              <input className="input" value={titulo} onChange={(e) => setTitulo(e.target.value)} placeholder="Ej. Sistema de inventario" />
            </div>
            <div>
              <label className="label">
                Detalles <span className="text-faint">(opcional)</span>
              </label>
              <textarea
                className="input min-h-[140px] resize-y"
                value={detalles}
                onChange={(e) => setDetalles(e.target.value)}
                placeholder="Qué hay que hacer a detalle: los pasos, dónde está cada cosa, lo que haga falta explicar…"
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="label">Tipo</label>
                {tipoFijo ? (
                  <div className="input flex items-center gap-1.5 bg-surface-3 text-muted">
                    <span>{tipoMeta[tipoFijo].emoji}</span>
                    {tipoFijo}
                  </div>
                ) : (
                  <select className="input" value={tipo} onChange={(e) => setTipo(e.target.value as TipoTarea)}>
                    {TIPOS_TAREA.map((tp) => (
                      <option key={tp} value={tp}>{tp}</option>
                    ))}
                  </select>
                )}
              </div>
              <div>
                <label className="label">Sprint</label>
                {/* Sin ningún sprint creado no hay nada que elegir: la tarea queda sin sprint (en el backlog). */}
                <select
                  className="input disabled:cursor-not-allowed disabled:opacity-60"
                  value={sprintId}
                  disabled={sprints.length === 0}
                  onChange={(e) => setSprintId(e.target.value)}
                >
                  <option value="">{sprints.length === 0 ? 'No hay sprints todavía' : 'Sin sprint (backlog)'}</option>
                  {sprints.map((sp) => (
                    <option key={sp.id} value={sp.id}>{sp.nombre}</option>
                  ))}
                </select>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <label className="flex cursor-pointer items-center gap-2 text-sm text-muted">
                <input type="checkbox" checked={conFecha} onChange={(e) => setConFecha(e.target.checked)} className="accent-primary" />
                Con fecha límite
              </label>
              {conFecha && (
                <input type="date" className="input max-w-[180px]" value={fecha} onChange={(e) => setFecha(e.target.value)} />
              )}
            </div>

            {esGeneral && (
              <div className="space-y-3 border-t border-border pt-4">
                <label className="label">
                  Subtareas — toca un tipo para agregar una (puedes repetir el mismo tipo varias veces)
                </label>
                <div className="flex flex-wrap gap-1.5">
                  {TIPOS_TAREA.map((tp) => {
                    const m = tipoMeta[tp];
                    return (
                      <button
                        key={tp}
                        type="button"
                        onClick={() => agregarSubtarea(tp)}
                        title={`Agregar subtarea de ${tp}`}
                        className="chip border border-border bg-surface-2 text-muted transition hover:border-border-strong hover:text-text"
                      >
                        <span>{m.emoji}</span>
                        {tp}
                      </button>
                    );
                  })}
                </div>

                {subtareas.length > 0 && (
                  <div className="flex flex-wrap items-start gap-2 pt-1">
                    {subtareas.map((st) => {
                      const m = tipoMeta[st.tipo];
                      if (st.guardada) {
                        return (
                          <button
                            key={st.uid}
                            type="button"
                            title={st.titulo}
                            onClick={() => reabrirSubtarea(st.uid)}
                            className="grid h-10 w-10 shrink-0 place-items-center rounded-full border text-lg transition hover:scale-110"
                            style={{ borderColor: `${m.color}70`, background: `${m.color}18`, color: m.color }}
                          >
                            {m.emoji}
                          </button>
                        );
                      }
                      return (
                        <div
                          key={st.uid}
                          className="w-full rounded-xl border p-3"
                          style={{ borderColor: `${m.color}70`, background: `${m.color}0d` }}
                        >
                          <div className="mb-2 flex items-center justify-between">
                            <span className="flex items-center gap-1.5 text-sm font-medium" style={{ color: m.color }}>
                              <span>{m.emoji}</span>
                              {st.tipo}
                            </span>
                            <button
                              type="button"
                              className="text-xs text-faint transition hover:text-danger"
                              onClick={() => quitarSubtarea(st.uid)}
                            >
                              Quitar
                            </button>
                          </div>
                          <div className="space-y-2">
                            <input
                              className="input"
                              placeholder="Nombre de la subtarea"
                              value={st.titulo}
                              onChange={(e) => editarSubtarea(st.uid, { titulo: e.target.value })}
                              autoFocus
                            />
                            <textarea
                              className="input min-h-[70px] resize-y"
                              placeholder="Detalles (opcional): pasos, dónde está cada cosa…"
                              value={st.detalles}
                              onChange={(e) => editarSubtarea(st.uid, { detalles: e.target.value })}
                            />
                            <div className="flex justify-end">
                              <button
                                type="button"
                                className="btn-outline btn-sm"
                                disabled={!st.titulo.trim()}
                                onClick={() => guardarSubtarea(st.uid)}
                              >
                                Guardar
                              </button>
                            </div>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}

                {faltanPorGuardar && (
                  <p className="text-xs text-faint">
                    Guarda (o quita) las subtareas que quedaron abiertas antes de crear la tarea.
                  </p>
                )}
                {mostrarErrorMinimas && faltanMinimas && (
                  <p className="text-xs text-danger">
                    Una tarea general necesita al menos {MIN_SUBTAREAS_GENERAL} subtareas.
                  </p>
                )}
              </div>
            )}
          </div>
        )}
      </Modal>

      {/* Aviso obligatorio para Desarrollador/Tester */}
      <Modal
        open={mostrarAviso}
        onClose={() => setMostrarAviso(false)}
        title="⏳ Aviso antes de guardar"
        size="sm"
        footer={
          <>
            <button className="btn-ghost" onClick={() => setMostrarAviso(false)}>Volver</button>
            <button
              className="btn-primary"
              onClick={() => {
                setAvisoLeido(true);
                setMostrarAviso(false);
                crear();
              }}
            >
              Entiendo, continuar
            </button>
          </>
        }
      >
        <p className="text-sm text-muted">
          Como <strong className="text-text">{rolUsuario}</strong>, esta tarea quedará{' '}
          <strong className="text-amber-300">pendiente de aprobación</strong> de un Líder o
          Administrador antes de poder mandarse a revisión.
        </p>
        <p className="mt-2 text-sm text-muted">
          Pero ya puedes <strong className="text-text">tomarla y trabajarla</strong> desde ahora —
          se creará su rama sin esperar la aprobación.
        </p>
      </Modal>
    </>
  );
}
