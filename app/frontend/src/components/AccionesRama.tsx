import { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import { useStore } from '../store/useStore';
import { coincidePalabra, esCheckinTarea, esTareaEnEspera, estadoMeta, palabraCambios } from '../lib/ui';
import { Modal } from './Modal';
import { EditarTareaModal } from './EditarTareaModal';
import { DetalleCommit } from './HistorialRamas';
import type { Commit } from '../types';
import type { Tarea } from '../types';
import { Avatar } from './primitives';

// Rama de la tarea, arriba en la ventana de detalles: la suya, la de su padre (check-in) o "Sin rama".
export function RamaDeLaTarea({ tarea }: { tarea: Tarea }) {
  const tareas = useStore((s) => s.tareas);
  const rama = ramaDeTarea(tarea, tareas);
  const checkin = tarea.modo_trabajo === 'checkin' && !tarea.rama_asociada;
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-faint">Rama</span>
      {rama ? (
        <span className="rounded-md border border-border bg-surface-3 px-2 py-0.5 font-mono text-text">
          {rama}
          {checkin && <span className="ml-1.5 font-sans text-faint">(check-in sobre la rama de su padre)</span>}
        </span>
      ) : (
        <span className="rounded-md border border-border bg-surface-2 px-2 py-0.5 text-faint">Sin rama — nadie la ha tomado todavía</span>
      )}
    </div>
  );
}

// Debajo de la rama, en los detalles: de qué tareas trajo cambios esta tarea ("Traer cambios").
// Son un préstamo: se usan sin modificarlos y se quitan antes de revisión/merge (si no, no deja
// avanzar). Los ya quitados quedan como historial.
export function CambiosTraidos({ tarea }: { tarea: Tarea }) {
  const tareas = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  const quitar = useStore((s) => s.quitarCambiosTraidos);
  const eliminarAjenos = useStore((s) => s.eliminarCambiosAjenos);
  const traidos = useStore((s) => s.cambiosTraidos).filter((c) => c.tarea_id === tarea.id);
  if (traidos.length === 0) return null;
  const aviso =
    tarea.estado === 'en revisión'
      ? 'Úsalos sin modificarlos y devuélvelos antes de mandar la tarea a terminada.'
      : 'Úsalos sin modificarlos. Si no hacen falta para revisarla, devuélvelos antes de mandarla a revisión; si no, el revisor los devolverá antes de marcarla como terminada.';
  const activos = traidos.filter((c) => !c.quitado_en).length;
  // Detección automática (backend): archivos prestados que se modificaron en esta rama.
  const tocados = traidos.filter((c) => !c.quitado_en && c.archivos_tocados.length > 0);
  const aceptarTocados = () => tocados.forEach((c) => eliminarAjenos(c.id));
  return (
    <div>
      <dt className="mb-1 text-xs text-faint">🤝 Cambios prestados</dt>
      {/* Aviso automático: sale solo y se cierra con Aceptar, que elimina lo modificado en lo prestado. */}
      {tocados.length > 0 &&
        createPortal(
        <Modal
          open
          onClose={aceptarTocados}
          title="⛔ Se tocaron cambios ajenos en esta tarea"
          subtitle={tarea.id}
          size="sm"
          footer={
            <div className="flex w-full justify-end">
              <button type="button" className="btn-primary btn-sm" onClick={aceptarTocados}>
                Aceptar
              </button>
            </div>
          }
        >
          <div className="space-y-2 text-sm">
            <p className="text-text">Se modificaron archivos que llegaron prestados de otra tarea:</p>
            <ul className="space-y-0.5 text-[12px]">
              {tocados.flatMap((c) =>
                c.archivos_tocados.map((a) => (
                  <li key={c.id + a}>
                    <span className="font-mono text-red-300">{a}</span>{' '}
                    <span className="text-faint">(prestado de {c.origen_id})</span>
                  </li>
                ))
              )}
            </ul>
            <p className="text-muted">
              Esas modificaciones no le corresponden a esta tarea y se van a eliminar: los archivos vuelven a como llegaron en
              el préstamo.
            </p>
          </div>
        </Modal>,
          document.body
        )}
      {activos > 0 && (
        <p className="mb-1.5 text-[11px] text-amber-300">
          ⚠️ {aviso}
        </p>
      )}
      <ul className="space-y-1.5">
        {[...traidos].reverse().map((c) => {
          const origen = tareas.find((t) => t.id === c.origen_id);
          const commit = c.hash ? commits.find((x) => x.hash === c.hash) : undefined;
          const est = origen ? estadoMeta[origen.estado] : null;
          return (
            <li
              key={c.id}
              className={`rounded-lg border border-border px-3 py-2 ${c.quitado_en ? 'bg-surface opacity-60' : 'bg-surface-2'}`}
            >
              <div className="flex items-center gap-2">
                <span className="font-mono text-[11px] text-faint">{c.origen_id}</span>
                <span className="min-w-0 flex-1 truncate text-text">{origen?.titulo ?? 'Tarea eliminada'}</span>
                <Avatar email={c.autor} size={20} />
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted">
                <span>
                  {c.hash ? (
                    <>
                      Commit <span className="font-mono text-text">{c.hash}</span>
                      {commit && <> — {commit.mensaje}</>}
                    </>
                  ) : (
                    'Todos sus cambios'
                  )}
                </span>
                <span className="text-faint">· {new Date(c.fecha).toLocaleString()}</span>
                {est && (
                  <span className="flex items-center gap-1">
                    <span className="h-1.5 w-1.5 rounded-full" style={{ background: est.dot }} />
                    {est.label}
                  </span>
                )}
              </div>
              {c.quitado_en ? (
                <p className="mt-1 text-[11px] text-faint">
                  ↩ Devuelto el {new Date(c.quitado_en).toLocaleString()}
                  {c.quitado_por && <> por {c.quitado_por.split('@')[0]}</>}
                </p>
              ) : (
                <div className="mt-1.5 flex justify-end">
                  <button type="button" className="btn-outline btn-sm" onClick={() => quitar(c.id)}>
                    ↩ Devolver
                  </button>
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

// Acciones de la rama en la ventana de detalles de una tarea (solo frontend, simulado):
// - "Abrir en Unity": cambia el workspace a la rama de la tarea (un check-in usa la de su padre) y
//   abre Unity ahí.
// - "Traer cambios": trae a esta rama los cambios de otra rama o check-in del proyecto que esté
//   terminada, en revisión o en producción con commits. Las en revisión avisan que todavía no están
//   aprobadas; las en producción, que es trabajo en curso que todavía puede cambiar.

/** Rama donde se trabaja una tarea: la suya, o la del padre si es un check-in directo. */
export function ramaDeTarea(t: Tarea, tareas: Tarea[]): string | null {
  if (t.rama_asociada) return t.rama_asociada;
  if (t.modo_trabajo === 'checkin' && t.tarea_padre_id) {
    return tareas.find((p) => p.id === t.tarea_padre_id)?.rama_asociada ?? null;
  }
  return null;
}

export function AccionesRama({ tarea, onCerrar }: { tarea: Tarea; onCerrar: () => void }) {
  const tareas = useStore((s) => s.tareas);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);
  const [traer, setTraer] = useState(false);
  const [avisoCheckin, setAvisoCheckin] = useState(false);
  const sinCheckinPorRama = useStore((s) => s.cambiosSinCheckin);
  const rol = useStore((s) => s.currentUser().rol);
  const [editar, setEditar] = useState(false);
  const rama = ramaDeTarea(tarea, tareas);
  const enEspera = esTareaEnEspera(tarea);
  // Traer cambios: en "Por hacer" (ya tomada, con rama) o "En producción" cualquier rol; en "En
  // revisión" solo Líder/Administrador (son quienes la mandan a terminada). En producción, si la
  // rama tiene cambios sin check-in, primero hay que hacer el check-in: si no, lo propio y lo
  // prestado quedan mezclados en el workspace.
  const esLider = rol === 'Líder' || rol === 'Administrador';
  const estadoPermite =
    (tarea.estado === 'por hacer' && !!tarea.asignado_a) ||
    tarea.estado === 'en producción' ||
    (tarea.estado === 'en revisión' && esLider);
  const sinCheckin = rama ? sinCheckinPorRama[rama] ?? 0 : 0;
  const motivoNoTraer = enEspera
    ? 'No se puede traer cambios: esta tarea todavía tiene cambios sin guardar (sin check-in)'
    : rama && !estadoPermite
      ? tarea.estado === 'en revisión'
        ? 'En revisión solo el Líder o el Administrador pueden traer cambios'
        : 'Solo se pueden traer cambios a una tarea en "Por hacer", "En producción" o (Líder/Administrador) "En revisión"'
      : undefined;

  return (
    <div className="flex w-full flex-wrap items-center justify-end gap-2">
      {/* Solo se edita en "Sin asignar" / "Por hacer": desde que alguien la toma ya tiene rama en Unity VC. */}
      <button
        type="button"
        className="btn-ghost btn-sm mr-auto"
        onClick={() => setEditar(true)}
        disabled={tarea.estado !== 'por hacer'}
        title={
          tarea.estado !== 'por hacer'
            ? 'Solo se puede editar en "Sin asignar" o "Por hacer": desde que alguien la toma ya tiene rama en Unity VC'
            : undefined
        }
      >
        ✏️ Editar tarea
      </button>
      {/* "En espera" tiene cambios sin guardar (aún no hay check-in): traerle cambios encima no se permite. */}
      <button
        type="button"
        className="btn-outline btn-sm"
        disabled={!rama || !!motivoNoTraer}
        onClick={() => (tarea.estado === 'en producción' && sinCheckin > 0 ? setAvisoCheckin(true) : setTraer(true))}
        title={motivoNoTraer}
      >
        ⇣ Traer cambios
      </button>
      <button
        type="button"
        className="btn-primary btn-sm"
        disabled={!rama}
        onClick={() => {
          if (!rama) return;
          abrirEnUnity(tarea.id, rama);
          onCerrar();
        }}
      >
        🎮 Abrir en Unity
      </button>
      {editar && <EditarTareaModal tarea={tarea} onClose={() => setEditar(false)} />}
      {avisoCheckin && rama && (
        <Modal
          open
          onClose={() => setAvisoCheckin(false)}
          title="Primero haz check-in"
          subtitle={`${rama} (${tarea.id})`}
          size="sm"
          footer={
            <div className="flex w-full justify-end gap-2">
              <button type="button" className="btn-ghost btn-sm" onClick={() => setAvisoCheckin(false)}>
                Cerrar
              </button>
              <button
                type="button"
                className="btn-primary btn-sm"
                onClick={() => {
                  abrirEnUnity(tarea.id, rama);
                  setAvisoCheckin(false);
                  onCerrar();
                }}
              >
                🎮 Abrir en Unity
              </button>
            </div>
          }
        >
          <p className="text-sm leading-relaxed text-text">
            Esta rama tiene <span className="font-semibold text-amber-300">{sinCheckin} {sinCheckin === 1 ? 'cambio' : 'cambios'} sin check-in</span>.
            Haz el check-in en Unity antes de traer cambios: así tu trabajo queda guardado aparte y lo que traigas
            no se mezcla con él.
          </p>
        </Modal>
      )}
      {traer && rama && <TraerCambiosModal tarea={tarea} rama={rama} onClose={() => setTraer(false)} />}
    </div>
  );
}

function TraerCambiosModal({ tarea, rama, onClose }: { tarea: Tarea; rama: string; onClose: () => void }) {
  const tareas = useStore((s) => s.tareas);
  const traerCambios = useStore((s) => s.traerCambios);
  const commits = useStore((s) => s.commits);
  const [busqueda, setBusqueda] = useState('');
  // Lo elegido: toda una tarea ("Traer todo") o un solo commit de ella ("Traer este").
  const [eleccion, setEleccion] = useState<{ tarea: Tarea; hash?: string } | null>(null);
  const elegida = eleccion?.tarea ?? null;
  const [desplegada, setDesplegada] = useState<string | null>(null);
  // Traer cambios es delicado: hay que escribir "traer" para confirmar.
  const [frase, setFrase] = useState('');
  const fraseOk = coincidePalabra(frase, 'traer');
  // Detalle de un commit (click en cualquier parte de su fila): la lista de su tarea y cuál se ve.
  const [verCommit, setVerCommit] = useState<{ lista: Commit[]; idx: number } | null>(null);
  const usuarios = useStore((s) => s.usuarios);
  const showToast = useStore((s) => s.showToast);

  // Ramas o check-ins del proyecto: terminados, en revisión o en producción con commits (no la misma tarea).
  const opciones = useMemo(() => {
    const q = busqueda.trim().toLowerCase();
    return tareas
      .filter(
        (t) =>
          t.id !== tarea.id &&
          !t.eliminada &&
          (t.estado === 'terminada' ||
            t.estado === 'en revisión' ||
            (t.estado === 'en producción' && commits.some((c) => c.tarea_id === t.id))) &&
          (t.rama_asociada || t.modo_trabajo === 'checkin') &&
          (!q || `${t.id} ${t.titulo}`.toLowerCase().includes(q))
      )
      .sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true }));
  }, [tareas, tarea.id, busqueda, commits]);
  const terminadas = opciones.filter((t) => t.estado === 'terminada');
  const enRevision = opciones.filter((t) => t.estado === 'en revisión');
  const enProduccion = opciones.filter((t) => t.estado === 'en producción');
  const esCheckin = (t: Tarea) => t.modo_trabajo === 'checkin' && !t.rama_asociada;

  // Cada tarea se despliega para ver sus commits: arriba "Traer todo" y en cada commit "Traer este".
  // "Traer todo" solo aparece desplegada, para que no parezca la única opción.
  const fila = (t: Tarea) => {
    const m = estadoMeta[t.estado];
    const suyos = commits.filter((c) => c.tarea_id === t.id).sort((a, b) => b.fecha.localeCompare(a.fecha));
    const abierta = desplegada === t.id;
    const todoElegido = eleccion?.tarea.id === t.id && !eleccion.hash;
    return (
      <div
        key={t.id}
        className={`rounded-lg border ${eleccion?.tarea.id === t.id ? 'border-primary/60' : 'border-border'} bg-surface-2`}
      >
        {/* Click en cualquier parte de la tarea despliega/oculta sus commits. */}
        <div
          onClick={() => suyos.length && setDesplegada(abierta ? null : t.id)}
          className={`flex items-center gap-3 rounded-lg px-3 py-2 text-sm ${suyos.length ? 'cursor-pointer transition hover:bg-surface-3' : ''}`}
        >
          <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: m.dot }} />
          <span className="shrink-0 font-mono font-semibold text-text">{t.id}</span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-text">{t.titulo}</span>
            <span className="block truncate font-mono text-[11px] text-faint">
              {esCheckin(t) ? `check-in sobre ${ramaDeTarea(t, tareas) ?? '—'}` : t.rama_asociada}
            </span>
          </span>
          {t.estado === 'en revisión' && <span className="shrink-0 text-[11px] text-amber-300">⚠ sin aprobar</span>}
          {t.estado === 'en producción' && <span className="shrink-0 text-[11px] text-red-300">⚠ en curso</span>}
          <span className={`shrink-0 px-2 py-1 text-[11px] text-faint ${suyos.length ? '' : 'opacity-50'}`}>
            {suyos.length} {palabraCambios(suyos.length, esCheckinTarea(t))} {suyos.length ? (abierta ? '▾' : '▸') : ''}
          </span>
        </div>
        {abierta && (
          <div className="space-y-0.5 border-t border-border px-2 py-1.5">
            <div className={`flex items-center justify-between gap-3 rounded-md px-2 py-1.5 text-xs ${todoElegido ? 'bg-primary-soft' : ''}`}>
              <span className="text-muted">
                Todos sus cambios ({suyos.length} {palabraCambios(suyos.length, esCheckinTarea(t))})
              </span>
              <button
                type="button"
                onClick={() => setEleccion({ tarea: t })}
                className={`shrink-0 rounded-md border px-2 py-0.5 text-[11px] transition ${
                  todoElegido ? 'border-primary/60 bg-primary text-white' : 'border-border text-muted hover:border-border-strong hover:text-text'
                }`}
              >
                Traer todo
              </button>
            </div>
            {suyos.map((c) => {
              const esteElegido = eleccion?.tarea.id === t.id && eleccion.hash === c.hash;
              return (
                <div
                  key={c.id}
                  // Click en cualquier parte del commit abre su detalle; "Traer este" solo lo elige.
                  onClick={() => setVerCommit({ lista: suyos, idx: suyos.indexOf(c) })}
                  className={`flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 text-xs transition hover:bg-surface-3 ${esteElegido ? 'bg-primary-soft' : ''}`}
                >
                  <span className="shrink-0 rounded border border-border bg-surface px-1.5 py-0.5 font-mono text-[11px] text-muted">
                    {c.hash}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-text">{c.mensaje}</span>
                  <span className="shrink-0 font-mono text-[11px] text-faint">{c.fecha.slice(0, 10)}</span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setEleccion({ tarea: t, hash: c.hash });
                    }}
                    className={`shrink-0 rounded-md border px-2 py-0.5 text-[11px] transition ${
                      esteElegido ? 'border-primary/60 bg-primary text-white' : 'border-border text-muted hover:border-border-strong hover:text-text'
                    }`}
                  >
                    Traer este
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
    );
  };

  return (
    <Modal
      open
      onClose={onClose}
      title="Traer cambios"
      subtitle={`A ${rama} (${tarea.id})`}
      size="lg"
      footer={
        <div className="flex w-full flex-wrap items-center justify-end gap-2">
          <label className="mr-auto flex items-center gap-2 text-xs text-muted">
            Escribe <span className="font-mono font-semibold text-text">traer</span> para confirmar
            <input
              className={`input h-8 w-28 py-1 text-xs ${frase && !fraseOk ? 'border-danger' : ''}`}
              value={frase}
              onChange={(e) => setFrase(e.target.value)}
              placeholder="traer"
              disabled={!eleccion}
            />
          </label>
          <button type="button" className="btn-ghost btn-sm" onClick={onClose}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn-primary btn-sm"
            disabled={!eleccion || !fraseOk}
            onClick={() => {
              if (!eleccion) return;
              traerCambios(tarea.id, rama, eleccion.tarea.id, eleccion.hash);
              onClose();
            }}
          >
            {!eleccion
              ? 'Elige qué traer'
              : eleccion.hash
                ? `Traer commit ${eleccion.hash} de ${eleccion.tarea.id}`
                : `Traer todo de ${eleccion.tarea.id}`}
          </button>
        </div>
      }
    >
      {verCommit && verCommit.lista[verCommit.idx] && (
        <DetalleCommit
          commit={verCommit.lista[verCommit.idx]}
          posicion={verCommit.lista.length - verCommit.idx}
          total={verCommit.lista.length}
          usuario={usuarios.find((u) => u.email === verCommit.lista[verCommit.idx].autor)}
          onCerrar={() => setVerCommit(null)}
          onCopiar={(hash) => {
            navigator.clipboard?.writeText(hash);
            showToast(`Hash ${hash} copiado`, 'info');
          }}
          onAnterior={verCommit.idx < verCommit.lista.length - 1 ? () => setVerCommit({ ...verCommit, idx: verCommit.idx + 1 }) : undefined}
          onSiguiente={verCommit.idx > 0 ? () => setVerCommit({ ...verCommit, idx: verCommit.idx - 1 }) : undefined}
        />
      )}
      <input className="input mb-3" placeholder="Buscar rama o check-in…" value={busqueda} onChange={(e) => setBusqueda(e.target.value)} />

      {elegida?.estado === 'en revisión' && (
        <div className="mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs text-amber-200">
          ⚠ {elegida.id} está en revisión: este cambio aún no ha sido aprobado. Traerlo puede ser riesgoso — si lo
          rechazan o lo corrigen después, tu rama quedará con una versión sin aprobar.
        </div>
      )}
      {elegida?.estado === 'en producción' && (
        <div className="mb-3 rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-red-200">
          ⚠ {elegida.id} está en producción: es trabajo en curso, todavía no se ha mandado a revisión ni se ha
          aprobado. Traerlo es más riesgoso — puede cambiar o quedar a medias, y tu rama quedaría con esa versión.
        </div>
      )}

      <div className="max-h-[380px] space-y-3 overflow-y-auto">
        <div>
          <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">Terminadas</div>
          {terminadas.length ? <div className="space-y-1.5">{terminadas.map(fila)}</div> : <div className="text-xs text-faint">Ninguna.</div>}
        </div>
        <div>
          <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">En revisión (sin aprobar)</div>
          {enRevision.length ? <div className="space-y-1.5">{enRevision.map(fila)}</div> : <div className="text-xs text-faint">Ninguna.</div>}
        </div>
        <div>
          <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">En producción (en curso, con commits)</div>
          {enProduccion.length ? <div className="space-y-1.5">{enProduccion.map(fila)}</div> : <div className="text-xs text-faint">Ninguna.</div>}
        </div>
      </div>
    </Modal>
  );
}
