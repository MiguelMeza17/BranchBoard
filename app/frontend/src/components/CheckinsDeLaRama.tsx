import { useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useStore, entrantesDe, estadoEntrante } from '../store/useStore';
import { esTareaEnEspera } from '../lib/ui';
import { ramaDeTarea } from './AccionesRama';
import { DetalleCommit } from './HistorialRamas';
import { Modal } from './Modal';
import { Avatar } from './primitives';
import { TIPOS_TAREA } from '../types';
import type { Commit, Tarea, TipoTarea } from '../types';

const fecha = (iso: string) => new Date(iso).toLocaleString();

// Las ventanas de detalle se abren encima de la de la tarea, que tiene transform (animación): sin
// portal quedarían recortadas dentro de ella.
const portal = (n: ReactNode) => createPortal(n, document.body);

// Check-ins de la rama, en los detalles de la tarea. En una rama sin subtareas el check-in no se
// vuelve tarea: queda aquí. En una rama con subtareas también se listan los changesets de sus
// tareas de check-in (caen en esta misma rama), cada uno con el ID de su tarea.
// Click en un check-in: su detalle, con "Abrir en Unity" en ese changeset.
export function CheckinsDeLaRama({ tarea }: { tarea: Tarea }) {
  const tareas = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  const usuarios = useStore((s) => s.usuarios);
  const showToast = useStore((s) => s.showToast);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);
  const [ver, setVer] = useState<number | null>(null);
  const rama = ramaDeTarea(tarea, tareas);
  if (esTareaEnEspera(tarea) || !rama) return null;
  const directos = tarea.rama_asociada
    ? new Set(
        tareas
          .filter((t) => t.tarea_padre_id === tarea.id && t.modo_trabajo === 'checkin' && !t.rama_asociada)
          .map((t) => t.id)
      )
    : new Set<string>();
  const lista: Commit[] = commits
    .filter((c) => c.tarea_id === tarea.id || directos.has(c.tarea_id))
    .sort((a, b) => b.fecha.localeCompare(a.fecha));
  const viendo = ver !== null ? lista[ver] : undefined;
  return (
    <div className="rounded-xl border border-border bg-surface p-3">
      <dt className="mb-2 text-xs font-semibold text-muted">📝 Check-ins de la rama ({lista.length})</dt>
      {lista.length === 0 && <p className="text-[12px] text-faint">Todavía no hay check-ins en esta rama.</p>}
      <ul className="max-h-56 space-y-1 overflow-y-auto">
        {lista.map((c, i) => (
          <li
            key={c.id}
            onClick={() => setVer(i)}
            className="flex cursor-pointer items-center gap-2 rounded-lg border border-border bg-surface-2 px-3 py-1.5 transition hover:bg-surface-3"
          >
            <span className="font-mono text-[11px] text-faint">{c.hash}</span>
            <span className="min-w-0 flex-1 truncate text-[12px] text-text">{c.mensaje}</span>
            {c.tarea_id !== tarea.id && <span className="font-mono text-[10px] text-faint">{c.tarea_id}</span>}
            <span className="text-[11px] text-faint">{fecha(c.fecha)}</span>
            <Avatar email={c.autor} size={18} />
          </li>
        ))}
      </ul>
      {viendo &&
        ver !== null &&
        portal(
          <DetalleCommit
            commit={viendo}
            posicion={lista.length - ver}
            total={lista.length}
            usuario={usuarios.find((u) => u.email === viendo.autor)}
            onCerrar={() => setVer(null)}
            onCopiar={(hash) => {
              navigator.clipboard?.writeText(hash);
              showToast(`Hash ${hash} copiado`, 'info');
            }}
            onAnterior={ver < lista.length - 1 ? () => setVer(ver + 1) : undefined}
            onSiguiente={ver > 0 ? () => setVer(ver - 1) : undefined}
            onAbrirUnity={() => {
              abrirEnUnity(viendo.tarea_id, rama, viendo.hash);
              setVer(null);
            }}
          />
        )}
    </div>
  );
}

// Cambios entrantes (incoming changes): subtareas con rama propia que ya se terminaron, es decir,
// que se mergearon a la rama de esta tarea, pero que el workspace todavía no tiene. Unirlos es
// opcional: desde su detalle (click) o desde "⇣ Traer cambios", donde salen primero en azul.
export function CambiosEntrantes({ tarea }: { tarea: Tarea }) {
  const tareas = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  const recibidos = useStore((s) => s.entrantesRecibidos);
  const recibirEntrante = useStore((s) => s.recibirEntrante);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);
  const [ver, setVer] = useState<string | null>(null);
  const entrantes = entrantesDe(tareas, recibidos, tarea.id);
  // Una tarea terminada ya se mergeó y su rama está cerrada: no tiene workspace al que le falte nada.
  // Solo sale cuando hay algo que unir (terminado o check-in en revisión).
  if (!tarea.rama_asociada || tarea.estado === 'terminada' || entrantes.length === 0) return null;
  const rama = tarea.rama_asociada;
  const viendo = entrantes.find((h) => h.id === ver);
  const suyos = viendo ? commits.filter((c) => c.tarea_id === viendo.id).sort((a, b) => b.fecha.localeCompare(a.fecha)) : [];
  return (
    <div className="rounded-xl border border-border bg-surface p-3">
      <dt className="mb-2 text-xs font-semibold text-muted">⇣ Incoming changes ({entrantes.length})</dt>
      <p className="mb-1.5 text-[11px] text-sky-300">
        Llegaron cambios a esta rama que tu workspace aún no tiene. Unirlos es opcional.
      </p>
      {(
        [
          ['Terminados', entrantes.filter((h) => h.estado === 'terminada')],
          ['En revisión (sin aprobar)', entrantes.filter((h) => h.estado !== 'terminada')],
        ] as const
      ).map(([titulo, grupo]) =>
        grupo.length === 0 ? null : (
          <div key={titulo} className="mb-1.5">
            <div className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-faint">
              {titulo} ({grupo.length})
            </div>
            <ul className="space-y-1">
              {grupo.map((h) => {
                const n = commits.filter((c) => c.tarea_id === h.id).length;
                const revision = h.estado !== 'terminada';
                return (
                  <li
                    key={h.id}
                    onClick={() => setVer(h.id)}
                    className={`cursor-pointer rounded-lg border px-3 py-1.5 transition ${
                      revision
                        ? 'border-amber-500/30 bg-amber-500/10 hover:bg-amber-500/20'
                        : 'border-sky-500/30 bg-sky-500/10 hover:bg-sky-500/20'
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-[11px] text-faint">{h.id}</span>
                      <span className="min-w-0 flex-1 truncate text-[12px] text-text">{h.titulo}</span>
                      {h.asignado_a && <Avatar email={h.asignado_a} size={18} />}
                    </div>
                    <p className="mt-0.5 text-[11px] text-muted">
                      <span className={revision ? 'text-amber-300' : ''}>{estadoEntrante(h)}</span> · {n}{' '}
                      {n === 1 ? 'check-in' : 'check-ins'} ·{' '}
                      {h.rama_asociada ? <>desde <span className="font-mono">{h.rama_asociada}</span></> : 'check-in directo en esta rama'}
                    </p>
                  </li>
                );
              })}
            </ul>
          </div>
        )
      )}
      {viendo &&
        portal(
          <Modal
            open
            onClose={() => setVer(null)}
            title={`⇣ ${viendo.titulo}`}
            subtitle={`Incoming change de ${viendo.id} hacia ${rama}`}
            size="md"
            footer={
              <div className="flex w-full items-center justify-end gap-2">
                <button
                  type="button"
                  className="btn-outline btn-sm"
                  onClick={() => {
                    abrirEnUnity(tarea.id, rama);
                    setVer(null);
                  }}
                >
                  🎮 Abrir en Unity
                </button>
                <button
                  type="button"
                  className="btn-primary btn-sm"
                  onClick={() => {
                    recibirEntrante(tarea.id, viendo.id);
                    setVer(null);
                  }}
                >
                  ⇣ Unir cambios
                </button>
              </div>
            }
          >
            <div className="space-y-3 text-sm">
              {viendo.estado !== 'terminada' && (
                <p className="rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-200">
                  ⚠ Este check-in está en revisión: todavía no está aprobado. Si lo rechazan, se corrige con otro check-in
                  encima, que también te llegará.
                </p>
              )}
              <p className="rounded-lg border border-sky-500/30 bg-sky-500/10 px-3 py-2 text-[12px] text-sky-200">
                {viendo.id} {viendo.estado === 'terminada' ? 'ya se terminó' : 'hizo check-in'} y sus cambios están en <span className="font-mono">{rama}</span>, pero tu workspace todavía no
                tiene esos cambios. Ya son de esta rama: no es un préstamo y no hay que devolverlos. Unirlos es opcional.
              </p>
              <dl className="grid grid-cols-2 gap-3">
                <div>
                  <dt className="text-xs text-faint">Desde</dt>
                  <dd className="font-mono text-text">{viendo.rama_asociada ?? 'check-in directo en esta rama'}</dd>
                </div>
                <div>
                  <dt className="text-xs text-faint">Estado</dt>
                  <dd className="text-text">{estadoEntrante(viendo)}</dd>
                </div>
                <div>
                  <dt className="text-xs text-faint">Trabajada por</dt>
                  <dd className="text-text">{viendo.asignado_a ?? '—'}</dd>
                </div>
                <div>
                  <dt className="text-xs text-faint">Tipo</dt>
                  <dd className="text-text">{viendo.tipo}</dd>
                </div>
              </dl>
              <div>
                <dt className="mb-1 text-xs text-faint">Check-ins que trae ({suyos.length})</dt>
                {suyos.length === 0 ? (
                  <p className="text-[12px] text-faint">Sin check-ins registrados.</p>
                ) : (
                  <ul className="max-h-48 space-y-1 overflow-y-auto">
                    {suyos.map((c) => (
                      <li key={c.id} className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 px-3 py-1.5">
                        <span className="font-mono text-[11px] text-faint">{c.hash}</span>
                        <span className="min-w-0 flex-1 truncate text-[12px] text-text">{c.mensaje}</span>
                        <span className="text-[11px] text-faint">{fecha(c.fecha)}</span>
                        <Avatar email={c.autor} size={18} />
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          </Modal>
        )}
    </div>
  );
}

// Pending changes: cambios sin check-in en el workspace de la rama de la tarea (cm status). En la
// subtarea "En espera" es justamente su razón de ser: avisa que alguien trabaja y aún no guarda.
// Qué pasa al hacer check-in depende de la rama: en una rama con hijos, el check-in se vuelve una
// tarea (la "En espera" se convierte en ella); en una rama sin hijos, queda en sus check-ins.
// Click: su detalle, con "Abrir en Unity" y "Hacer check-in".
export function PendingChanges({ tarea }: { tarea: Tarea }) {
  const tareas = useStore((s) => s.tareas);
  const sinCheckin = useStore((s) => s.cambiosSinCheckin);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);
  const hacerCheckin = useStore((s) => s.hacerCheckin);
  const [abierto, setAbierto] = useState(false);
  const [mensaje, setMensaje] = useState('');
  const [descripcion, setDescripcion] = useState('');
  const [tipo, setTipo] = useState<TipoTarea>(TIPOS_TAREA[0]);
  const rama = ramaDeTarea(tarea, tareas);
  if (!rama || tarea.estado === 'terminada') return null;
  const n = sinCheckin[rama] ?? 0;
  const enEspera = esTareaEnEspera(tarea);
  const padre = tareas.find((t) => t.id === tarea.tarea_padre_id);
  const quien = (enEspera ? padre?.asignado_a : tarea.asignado_a) ?? null;
  const duena = tareas.find((t) => t.rama_asociada === rama);
  const conHijos = !!duena && tareas.some((t) => t.tarea_padre_id === duena.id && !t.eliminada);
  const quePasa = conHijos
    ? `Es una rama con subtareas: al hacer check-in, el check-in se vuelve una tarea${
        enEspera ? ' (esta "En espera" se convierte en ella)' : ''
      } de ${duena!.id}.`
    : 'Al hacer check-in pasan a los check-ins de la rama; no se convierten en tarea.';
  const cerrar = () => {
    setAbierto(false);
    setMensaje('');
    setDescripcion('');
  };
  return (
    <div className="rounded-xl border border-border bg-surface p-3">
      <dt className="mb-2 text-xs font-semibold text-muted">⇡ Pending changes ({n})</dt>
      {n > 0 ? (
        <div
          onClick={() => setAbierto(true)}
          className="cursor-pointer rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 transition hover:bg-amber-500/20"
        >
          <div className="flex items-center gap-2 text-[12px] text-amber-200">
            {quien && <Avatar email={quien} size={20} />}
            <span>
              ✏️ {quien ? <b>{quien.split('@')[0]}</b> : 'Alguien'} está trabajando:{' '}
              {n === 1 ? '1 cambio sin check-in' : `${n} cambios sin check-in`}
            </span>
          </div>
          <p className="mt-1 text-[11px] text-muted">{quePasa}</p>
        </div>
      ) : (
        <p className="text-[12px] text-faint">No hay cambios sin check-in en esta rama.</p>
      )}
      {abierto &&
        n > 0 &&
        portal(
          <Modal
            open
            onClose={cerrar}
            title="⇡ Pending changes"
            subtitle={`${n === 1 ? '1 cambio' : `${n} cambios`} sin check-in en ${rama}`}
            size="md"
            footer={
              <div className="flex w-full items-center justify-end gap-2">
                <button
                  type="button"
                  className="btn-outline btn-sm"
                  onClick={() => {
                    abrirEnUnity(tarea.id, rama);
                    cerrar();
                  }}
                >
                  🎮 Abrir en Unity
                </button>
                <button
                  type="button"
                  className="btn-primary btn-sm"
                  disabled={!mensaje.trim()}
                  onClick={() => {
                    hacerCheckin(rama, { titulo: mensaje.trim(), descripcion: descripcion.trim(), tipo });
                    cerrar();
                  }}
                >
                  ⇡ Hacer check-in
                </button>
              </div>
            }
          >
            <div className="space-y-3 text-sm">
              <dl className="grid grid-cols-2 gap-3">
                <div>
                  <dt className="text-xs text-faint">Rama</dt>
                  <dd className="font-mono text-text">{rama}</dd>
                </div>
                <div>
                  <dt className="text-xs text-faint">Quién trabaja</dt>
                  <dd className="text-text">{quien ?? '—'}</dd>
                </div>
                <div>
                  <dt className="text-xs text-faint">Cambios sin check-in</dt>
                  <dd className="text-text">{n}</dd>
                </div>
                <div>
                  <dt className="text-xs text-faint">Archivos</dt>
                  <dd className="text-faint">En la app real, la lista de `cm status`.</dd>
                </div>
              </dl>
              <p className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-200">{quePasa}</p>
              <label className="block">
                <span className="mb-1 block text-xs text-faint">Título</span>
                <input
                  className="input"
                  value={mensaje}
                  onChange={(e) => setMensaje(e.target.value)}
                  placeholder="Qué cambiaste, en una línea"
                  autoFocus
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-xs text-faint">Descripción</span>
                <textarea
                  className="input min-h-[72px] resize-y"
                  value={descripcion}
                  onChange={(e) => setDescripcion(e.target.value)}
                  placeholder="Detalles del cambio (opcional)"
                />
              </label>
              <label className="block">
                <span className="mb-1 block text-xs text-faint">Tipo</span>
                <select className="input" value={tipo} onChange={(e) => setTipo(e.target.value as TipoTarea)}>
                  {TIPOS_TAREA.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </Modal>
        )}
    </div>
  );
}
