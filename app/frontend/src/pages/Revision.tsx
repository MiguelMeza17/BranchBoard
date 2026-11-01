import { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { puedeAutorizarRevisionGrande } from '../lib/permisos';
import { PageHeader, TipoBadge, Avatar, EmptyState, ReadOnlyTag } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { ramaDeTarea } from '../components/AccionesRama';
import { Modal } from '../components/Modal';
import type { Tarea, Usuario } from '../types';
import { coincidePalabra, palabraCambios, tipoMeta } from '../lib/ui';

export function Revision() {
  const user = useStore((s) => s.currentUser());
  const tareas = useStore((s) => s.tareas);
  const usuarios = useStore((s) => s.usuarios);
  const autorizarMerge = useStore((s) => s.autorizarMerge);
  const autorizarCheckins = useStore((s) => s.autorizarCheckins);
  const commits = useStore((s) => s.commits);
  const puedeAvanzar = useStore((s) => s.puedeAvanzar);
  const rechazarRevision = useStore((s) => s.rechazarRevision);
  const rechazarCheckins = useStore((s) => s.rechazarCheckins);
  const asignarRevision = useStore((s) => s.asignarRevision);
  const showToast = useStore((s) => s.showToast);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);

  const [revisando, setRevisando] = useState<Tarea | null>(null);
  const [preview, setPreview] = useState<Tarea | null>(null);
  const [autorizar, setAutorizar] = useState<Tarea | null>(null);
  const [rechazar, setRechazar] = useState<Tarea | null>(null);
  const [asignando, setAsignando] = useState<Tarea | null>(null);
  const [aprobarJuntos, setAprobarJuntos] = useState<Tarea[] | null>(null);
  const [rechazarJuntos, setRechazarJuntos] = useState<Tarea[] | null>(null);
  // "Revisar los N juntos": ventana con los check-in del grupo y la opción de autorizarlos o rechazarlos todos.
  const [revisarJuntos, setRevisarJuntos] = useState<Tarea[] | null>(null);

  // Antes de asignar, tanto Líder como Administrador pueden elegir revisor.
  // Ya asignada, solo Administrador puede reasignar — un Líder ya no puede tocarlo.
  const puedeAsignar = (t: Tarea) =>
    t.lider_revision_asignado ? user.rol === 'Administrador' : user.rol === 'Administrador' || user.rol === 'Líder';

  const enRevision = useMemo(() => tareas.filter((t) => t.estado === 'en revisión' && !t.eliminada), [tareas]);
  const grandes = enRevision.filter((t) => t.tarea_padre_id === null);
  // Check-ins (subtareas trabajadas con check-in directo sobre la rama de su padre): se agrupan por
  // tarea padre, porque se pueden revisar juntos o uno por uno. El resto de subtareas, como siempre.
  const esCheckin = (t: Tarea) => t.modo_trabajo === 'checkin' && !t.rama_asociada;
  const pequenas = enRevision.filter((t) => t.tarea_padre_id !== null && !esCheckin(t));
  const checkinsPorPadre = useMemo(() => {
    const m = new Map<string, Tarea[]>();
    enRevision
      .filter((t) => t.tarea_padre_id !== null && esCheckin(t))
      .forEach((t) => m.set(t.tarea_padre_id!, [...(m.get(t.tarea_padre_id!) ?? []), t]));
    return [...m.entries()].map(([padreId, lista]) => ({
      padre: tareas.find((x) => x.id === padreId),
      lista: lista.sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true })),
    }));
  }, [enRevision, tareas]);
  const commitsDe = (id: string) => commits.filter((c) => c.tarea_id === id).sort((a, b) => b.fecha.localeCompare(a.fecha));

  const lideres = usuarios.filter((u) => u.rol === 'Líder' || u.rol === 'Administrador');
  const puedeActuarGrande = puedeAutorizarRevisionGrande(user.rol);

  // En "tareas pequeñas" autoriza quien está a cargo de la tarea padre
  const esDuenoPadre = (t: Tarea) => {
    const padre = tareas.find((x) => x.id === t.tarea_padre_id);
    return padre?.asignado_a === user.email;
  };

  const nombreDe = (email: string) => {
    const u = usuarios.find((x) => x.email === email);
    return u ? `${u.nombre} ${u.apellido}` : email.split('@')[0];
  };

  return (
    <div>
      <PageHeader title="Revisión" desc="Estado tipo pull-request de las ramas esperando merge." />

      <div className="card p-5">
      {/* Tareas grandes */}
      <section className="mb-8">
        <div className="mb-3 flex items-center gap-2">
          <h2 className="text-sm font-semibold text-text">📁 Tareas grandes → dev</h2>
          <span className="chip border border-border bg-surface-2 text-muted">{grandes.length}</span>
          {!puedeActuarGrande && <ReadOnlyTag />}
        </div>
        {grandes.length === 0 ? (
          <EmptyState icon="🔀" title="Nada esperando merge a dev" />
        ) : (
          <div className="space-y-2">
            {grandes.map((t) => (
              <div
                key={t.id}
                className={`card flex flex-wrap items-center gap-4 p-4 ${
                  t.lider_revision_asignado ? 'bg-fuchsia-500/5' : ''
                }`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-[11px] text-faint">{t.id}</span>
                    <span className="font-medium text-text">{t.titulo}</span>
                  </div>
                  <div className="mt-1.5 flex flex-wrap items-center gap-2">
                    <TipoBadge tipo={t.tipo} />
                    <span className="font-mono text-[11px] text-accent">🌿 {t.rama_asociada}</span>
                    {t.asignado_a && <Avatar email={t.asignado_a} size={22} />}
                  </div>
                </div>

                {puedeActuarGrande ? (
                  <div className="flex items-center gap-2">
                    {puedeAsignar(t) && (
                      <button className="btn-outline btn-sm" onClick={() => setAsignando(t)}>
                        {t.lider_revision_asignado ? `${nombreDe(t.lider_revision_asignado)} ▾` : 'Asignar a un líder'}
                      </button>
                    )}
                    <button className="btn-primary btn-sm" onClick={() => setRevisando(t)}>Revisar</button>
                  </div>
                ) : (
                  <span className="text-xs text-faint">Autoriza Líder/Administrador</span>
                )}
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Tareas pequeñas */}
      <section>
        <div className="mb-3 flex items-center gap-2">
          <h2 className="text-sm font-semibold text-text">📁 Tareas pequeñas → tarea padre</h2>
          <span className="chip border border-border bg-surface-2 text-muted">
            {pequenas.length + checkinsPorPadre.reduce((n, g) => n + g.lista.length, 0)}
          </span>
        </div>
        {/* Check-ins agrupados por tarea padre (se revisan juntos o uno por uno) */}
        {checkinsPorPadre.length > 0 && (
          <div className="mb-3">
          <div className="space-y-3">
            {checkinsPorPadre.map(({ padre, lista }) => {
              const puedeActuar =
                (padre && padre.asignado_a === user.email) || user.rol === 'Administrador' || user.rol === 'Líder';
              return (
                <div key={padre?.id ?? lista[0].id} className="card p-4">
                  <div className="mb-2 flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-[11px] text-faint">{padre?.id}</span>
                        <span className="font-medium text-text">{padre?.titulo ?? '—'}</span>
                        <span className="chip border border-border bg-surface-2 text-muted">
                          {lista.length} check-in{lista.length === 1 ? '' : 's'}
                        </span>
                      </div>
                      <p className="mt-1 text-xs text-muted">
                        Sus check-ins ya están en la rama{' '}
                        <span className="font-mono text-text">{padre?.rama_asociada ?? '—'}</span> (se hicieron directo
                        ahí, sin rama propia). Aprobar un check-in lo da por terminado; se pueden revisar uno por uno o
                        todos juntos.
                      </p>
                    </div>
                  </div>
                  <div className="space-y-1.5">
                    {lista.map((t) => {
                      const suyos = commitsDe(t.id);
                      return (
                        <div
                          key={t.id}
                          // Click en cualquier parte abre sus detalles (como las tareas sin hijas en el Scrum).
                          onClick={puedeActuar ? () => setRevisando(t) : undefined}
                          className={`flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface-2 px-3 py-2 ${
                            puedeActuar ? 'cursor-pointer transition hover:border-border-strong hover:bg-surface-3' : ''
                          }`}
                        >
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-2">
                              <span className="font-mono text-[11px] text-faint">{t.id}</span>
                              <span className="text-sm text-text">{t.titulo}</span>
                              <TipoBadge tipo={t.tipo} />
                            </div>
                            <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-faint">
                              {t.asignado_a && <span>{nombreDe(t.asignado_a)}</span>}
                              <span>
                                · {suyos.length} {palabraCambios(suyos.length, true)}
                              </span>
                              {suyos[0] && (
                                <span className="truncate">
                                  · último: <span className="font-mono">{suyos[0].hash}</span> {suyos[0].mensaje}
                                </span>
                              )}
                            </div>
                          </div>
                          {puedeActuar ? (
                            <button
                              className="btn-outline btn-sm"
                              onClick={(e) => {
                                e.stopPropagation();
                                setRevisando(t);
                              }}
                            >
                              Revisar este
                            </button>
                          ) : (
                            <ReadOnlyTag />
                          )}
                        </div>
                      );
                    })}
                  </div>
                  {puedeActuar && lista.length > 1 && (
                    <div className="mt-3 flex justify-end">
                      <button className="btn-primary btn-sm" onClick={() => setRevisarJuntos(lista)}>
                        Revisar los {lista.length} juntos
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          </div>
        )}
        {pequenas.length === 0 && checkinsPorPadre.length > 0 ? null : pequenas.length === 0 ? (
          <EmptyState icon="🧩" title="No hay subtareas esperando revisión" />
        ) : (
          <div className="space-y-2">
            {pequenas.map((t) => {
              const padre = tareas.find((x) => x.id === t.tarea_padre_id);
              const puedeActuar = esDuenoPadre(t) || user.rol === 'Administrador' || user.rol === 'Líder';
              return (
                <div
                  key={t.id}
                  // Click en cualquier parte abre sus detalles (no tiene hijas).
                  onClick={puedeActuar ? () => setRevisando(t) : undefined}
                  className={`card flex flex-wrap items-center gap-4 p-4 ${
                    t.lider_revision_asignado ? 'bg-fuchsia-500/5' : ''
                  } ${puedeActuar ? 'cursor-pointer transition hover:border-border-strong' : ''}`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-[11px] text-faint">{t.id}</span>
                      <span className="font-medium text-text">{t.titulo}</span>
                    </div>
                    <div className="mt-1.5 flex flex-wrap items-center gap-2">
                      <TipoBadge tipo={t.tipo} />
                      <span className="text-xs text-muted">↳ padre: {padre?.titulo ?? '—'}</span>
                      {t.archivo_ruta && <span className="font-mono text-[11px] text-faint">{t.archivo_ruta}</span>}
                    </div>
                  </div>
                  {/* Los botones hacen lo suyo sin abrir los detalles de la fila. */}
                  <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
                    {puedeAsignar(t) && (
                      <button className="btn-outline btn-sm" onClick={() => setAsignando(t)}>
                        {t.lider_revision_asignado ? `${nombreDe(t.lider_revision_asignado)} ▾` : 'Asignar a un líder'}
                      </button>
                    )}
                    {puedeActuar ? (
                      <button className="btn-primary btn-sm" onClick={() => setRevisando(t)}>Revisar</button>
                    ) : (
                      <ReadOnlyTag />
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
      </div>

      {/* Revisar (ver detalle y elegir acción) */}
      {revisando && (
        <RevisarEnRevisionModal
          tarea={revisando}
          padre={revisando.tarea_padre_id ? tareas.find((x) => x.id === revisando.tarea_padre_id) : undefined}
          onClose={() => setRevisando(null)}
          onAutorizar={() => { if (puedeAvanzar(revisando.id, 'terminada')) { setAutorizar(revisando); setRevisando(null); } }}
          onRechazar={() => { setRechazar(revisando); setRevisando(null); }}
          onPreRevisar={() => setPreview(revisando)}
          // Revisar viendo el trabajo en Unity: cambia el workspace a la rama de la tarea (un check-in usa
          // la de su padre) y abre Unity ahí.
          rama={ramaDeTarea(revisando, tareas)}
          onAbrirUnity={() => {
            const rama = ramaDeTarea(revisando, tareas);
            if (!rama) return;
            abrirEnUnity(revisando.id, rama);
            setRevisando(null);
          }}
        />
      )}

      {/* Asignar a un líder (candado: escribir "asignar") */}
      {asignando && (
        <AsignarLiderModal
          tarea={asignando}
          lideres={lideres}
          onClose={() => setAsignando(null)}
          onConfirm={(email) => { asignarRevision(asignando.id, email); setAsignando(null); }}
        />
      )}

      {/* Visor de pre-revisión (tareas pequeñas) */}
      <Modal open={!!preview} onClose={() => setPreview(null)} title="Pre-revisión" subtitle={preview ? `${preview.id} · ${preview.tipo}` : ''} size="lg">
        {preview && <PrevisualizadorTipo tarea={preview} />}
      </Modal>

      {/* Revisar juntos los check-in de una tarea padre */}
      <Modal
        open={!!revisarJuntos}
        onClose={() => setRevisarJuntos(null)}
        title={`Revisar ${revisarJuntos?.length ?? 0} check-ins juntos`}
        subtitle={
          revisarJuntos?.[0]?.tarea_padre_id
            ? `Tarea padre ${revisarJuntos[0].tarea_padre_id} · sus check-ins ya están en su rama`
            : ''
        }
        footer={
          <div className="flex w-full justify-end gap-2">
            <button
              className="btn-danger btn-sm"
              onClick={() => {
                setRechazarJuntos(revisarJuntos);
                setRevisarJuntos(null);
              }}
            >
              Rechazar los {revisarJuntos?.length ?? 0}
            </button>
            <button
              className="btn-primary btn-sm"
              onClick={() => {
                setAprobarJuntos(revisarJuntos);
                setRevisarJuntos(null);
              }}
            >
              Autorizar los {revisarJuntos?.length ?? 0}
            </button>
          </div>
        }
      >
        <div className="space-y-1.5">
          {revisarJuntos?.map((t) => {
            const suyos = commitsDe(t.id);
            return (
              <div key={t.id} className="rounded-lg border border-border bg-surface-2 px-3 py-2">
                <div className="flex items-center gap-2">
                  <span className="font-mono text-[11px] text-faint">{t.id}</span>
                  <span className="text-sm text-text">{t.titulo}</span>
                  <TipoBadge tipo={t.tipo} />
                </div>
                <div className="mt-1 text-[11px] text-faint">
                  {t.asignado_a ? nombreDe(t.asignado_a) : '—'} · {suyos.length} {palabraCambios(suyos.length, true)}
                </div>
                {/* Detalles del check-in (qué había que hacer), antes de sus check-ins. */}
                <div className="mt-2">
                  <div className="mb-0.5 text-[11px] text-faint">Detalles</div>
                  {t.detalles ? (
                    <p className="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-md border border-border bg-surface px-2.5 py-1.5 text-xs leading-relaxed text-text">
                      {t.detalles}
                    </p>
                  ) : (
                    <p className="text-xs text-faint">Sin detalles.</p>
                  )}
                </div>
                {suyos.length > 0 && (
                  <div className="mt-1.5 space-y-0.5">
                    {suyos.map((c) => (
                      <div key={c.id} className="flex items-baseline gap-2 text-xs">
                        <span className="shrink-0 font-mono text-faint">{c.hash}</span>
                        <span className="truncate text-muted">{c.mensaje}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </Modal>

      {/* Rechazar juntos los check-in (doble candado + motivo) */}
      <ConfirmLock
        open={!!rechazarJuntos}
        onClose={() => setRechazarJuntos(null)}
        onConfirm={(motivo) => {
          if (rechazarJuntos) rechazarCheckins(rechazarJuntos.map((t) => t.id), motivo ?? '');
          setRechazarJuntos(null);
        }}
        title="Rechazar check-ins juntos"
        tone="danger"
        palabra="rechazar"
        confirmLabel={`Rechazar ${rechazarJuntos?.length ?? 0} check-ins`}
        requiereJustificacion
        justificacionLabel="Motivo del rechazo (le llega a cada responsable)"
        description={`Los check-in ${rechazarJuntos?.map((t) => t.id).join(', ') ?? ''} volverán a "en producción" con el motivo indicado. Sus cambios ya están en la rama de la tarea padre: rechazarlos no los quita; cada responsable debe corregirlo con un nuevo check-in.`}
      />

      {/* Aprobar juntos los check-in de una tarea padre (doble candado) */}
      <ConfirmLock
        open={!!aprobarJuntos}
        onClose={() => setAprobarJuntos(null)}
        onConfirm={() => {
          if (aprobarJuntos) autorizarCheckins(aprobarJuntos.map((t) => t.id));
          setAprobarJuntos(null);
        }}
        title="Autorizar check-ins juntos"
        tone="primary"
        palabra="autorizar"
        confirmLabel={`Autorizar ${aprobarJuntos?.length ?? 0} check-ins`}
        description={`Vas a aprobar juntos los check-in ${aprobarJuntos?.map((t) => t.id).join(', ') ?? ''}. Sus check-ins ya están en la rama de su tarea padre; quedarán terminados.`}
      />

      {/* Autorizar (doble candado) */}
      <ConfirmLock
        open={!!autorizar}
        onClose={() => setAutorizar(null)}
        onConfirm={() => { if (autorizar) autorizarMerge(autorizar.id); setAutorizar(null); }}
        title="Autorizar merge"
        tone="primary"
        palabra="autorizar"
        confirmLabel="Autorizar y mergear"
        description={`Vas a autorizar el merge de "${autorizar?.titulo}". Esta acción ejecuta el merge en Unity Version Control.`}
      />

      {/* Rechazar (con justificación + candado) */}
      <ConfirmLock
        open={!!rechazar}
        onClose={() => setRechazar(null)}
        onConfirm={(motivo) => { if (rechazar) rechazarRevision(rechazar.id, motivo ?? ''); setRechazar(null); }}
        title="Rechazar revisión"
        tone="danger"
        palabra="rechazar"
        confirmLabel="Rechazar"
        requiereJustificacion
        justificacionLabel="Motivo del rechazo (le llega al responsable)"
        description={`La tarea "${rechazar?.titulo}" volverá a "en producción" con el motivo indicado.${
          rechazar && rechazar.modo_trabajo === 'checkin' && !rechazar.rama_asociada
            ? ' Sus cambios ya están en la rama de la tarea padre: rechazar no los quita; el responsable debe corregirlo con un nuevo check-in.'
            : ''
        }`}
      />

    </div>
  );
}

function RevisarEnRevisionModal({
  tarea,
  padre,
  onClose,
  onAutorizar,
  onRechazar,
  onPreRevisar,
  onAbrirUnity,
  rama,
}: {
  tarea: Tarea;
  padre?: Tarea;
  onClose: () => void;
  onAutorizar: () => void;
  onRechazar: () => void;
  onPreRevisar: () => void;
  onAbrirUnity: () => void;
  rama: string | null;
}) {
  return (
    <Modal
      open
      onClose={onClose}
      title={tarea.titulo}
      subtitle={`${tarea.id} · ${tarea.tipo}`}
      footer={
        <>
          {padre && <button className="btn-outline btn-sm" onClick={onPreRevisar}>👁️ Pre-revisar</button>}
          {/* Siempre: no hay mejor forma de revisar que ver el trabajo en Unity. */}
          <button className="btn-outline btn-sm" onClick={onAbrirUnity} disabled={!rama} title={rama ?? 'Sin rama'}>
            🎮 Abrir en Unity
          </button>
          <button className="btn-danger btn-sm" onClick={onRechazar}>Rechazar</button>
          <button className="btn-primary btn-sm" onClick={onAutorizar}>✓ Autorizar</button>
        </>
      }
    >
      <div className="space-y-3">
        {/* Detalles primero; abajo, lo demás (responsable, rama, padre, archivo). */}
        <div>
          <div className="mb-1 text-xs text-faint">Detalles</div>
          {tarea.detalles ? (
            <p className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm leading-relaxed text-text">
              {tarea.detalles}
            </p>
          ) : (
            <p className="text-sm text-faint">Sin detalles.</p>
          )}
        </div>
        <div className="border-t border-border pt-3" />
        <div className="flex items-center gap-2">
          {tarea.asignado_a && <Avatar email={tarea.asignado_a} size={20} />}
          {tarea.rama_asociada && (
            <span className="font-mono text-[11px] text-accent">🌿 {tarea.rama_asociada}</span>
          )}
        </div>
        {padre && <p className="text-xs text-muted">↳ subtarea de {padre.titulo}</p>}
        {tarea.archivo_ruta && <p className="font-mono text-xs text-faint">{tarea.archivo_ruta}</p>}
        {tarea.lider_revision_asignado && (
          <span className="chip border border-fuchsia-500/30 bg-fuchsia-500/10 text-fuchsia-300">
            Revisa: {tarea.lider_revision_asignado.split('@')[0]}
          </span>
        )}
      </div>
    </Modal>
  );
}

function AsignarLiderModal({
  tarea,
  lideres,
  onClose,
  onConfirm,
}: {
  tarea: Tarea;
  lideres: Usuario[];
  onClose: () => void;
  onConfirm: (email: string) => void;
}) {
  const [email, setEmail] = useState(tarea.lider_revision_asignado ?? '');
  const [texto, setTexto] = useState('');
  const palabraOk = coincidePalabra(texto, 'asignar');
  const enabled = !!email && palabraOk;

  return (
    <Modal
      open
      onClose={onClose}
      title="Asignar a un líder"
      size="sm"
      footer={
        <>
          <button className="btn-ghost" onClick={onClose}>Cancelar</button>
          <button className="btn-primary" disabled={!enabled} onClick={() => onConfirm(email)}>
            Asignar
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="text-sm text-muted">
          Vas a asignar la revisión de <span className="text-text">"{tarea.titulo}"</span> a un Líder o Administrador.
        </div>
        <div>
          <label className="label">Líder o Administrador</label>
          <select className="input" value={email} onChange={(e) => setEmail(e.target.value)}>
            <option value="">Selecciona…</option>
            {lideres.map((l) => (
              <option key={l.email} value={l.email}>{l.nombre} {l.apellido}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="label">
            Escribe <span className="font-mono font-semibold text-text">asignar</span> para confirmar
          </label>
          <input
            className={`input font-mono ${texto && !palabraOk ? 'border-danger focus:border-danger focus:ring-danger/30' : ''}`}
            value={texto}
            onChange={(e) => setTexto(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && enabled) {
                e.preventDefault();
                onConfirm(email);
              }
            }}
            placeholder="asignar"
            autoComplete="off"
            spellCheck={false}
          />
        </div>
      </div>
    </Modal>
  );
}

function PrevisualizadorTipo({ tarea }: { tarea: Tarea }) {
  const c = tipoMeta[tarea.tipo].color;
  const box = 'grid place-items-center rounded-xl border border-border bg-surface-2 text-center';
  switch (tarea.tipo) {
    case 'Modelado 3D':
    case 'Animación':
      return (
        <div className={box} style={{ height: 320 }}>
          <div>
            <div className="text-5xl">🧊</div>
            <p className="mt-3 text-sm text-muted">Visor de malla 3D (glTF)</p>
            <p className="text-xs text-faint">12.4k polígonos · normales visibles{tarea.tipo === 'Animación' ? ' · animación en loop' : ''}</p>
            <button className="btn-outline btn-sm mt-3">🎮 Abrir en Unity</button>
          </div>
        </div>
      );
    case 'Audio':
    case 'Música':
      return (
        <div className={box} style={{ height: 200 }}>
          <div className="w-full max-w-md px-6">
            <div className="text-4xl">🔊</div>
            <div className="mt-4 flex items-center gap-3">
              <button className="grid h-10 w-10 place-items-center rounded-full bg-primary text-white">▶</button>
              <div className="h-1.5 flex-1 rounded-full bg-surface-3"><div className="h-full w-1/3 rounded-full" style={{ background: c }} /></div>
              <span className="font-mono text-xs text-faint">0:42</span>
            </div>
          </div>
        </div>
      );
    case 'Video':
      return (
        <div className="grid place-items-center rounded-xl border border-border bg-black" style={{ height: 320 }}>
          <div className="text-center"><div className="text-5xl">🎬</div><p className="mt-2 text-sm text-muted">Reproductor de video embebido</p></div>
        </div>
      );
    default:
      return (
        <div className={box} style={{ height: 300 }}>
          <div>
            <div className="text-5xl">{tipoMeta[tarea.tipo].emoji}</div>
            <p className="mt-3 text-sm text-muted">Visor de imagen</p>
            <p className="text-xs text-faint">{tarea.archivo_ruta}</p>
          </div>
        </div>
      );
  }
}
