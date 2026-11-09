import { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { puedeAutorizarRevisionGrande } from '../lib/permisos';
import { PageHeader, TipoBadge, Avatar, EmptyState, ReadOnlyTag } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { CambiosTraidos, ramaDeTarea } from '../components/AccionesRama';
import { Modal } from '../components/Modal';
import type { Tarea, Usuario } from '../types';
import { coincidePalabra, palabraCambios, tipoMeta } from '../lib/ui';
import { Icono, IconoEnLinea } from '../components/Icono';

export function Revision() {
  const user = useStore((s) => s.currentUser());
  const tareas = useStore((s) => s.tareas);
  const usuarios = useStore((s) => s.usuarios);
  const autorizarMerge = useStore((s) => s.autorizarMerge);
  const commits = useStore((s) => s.commits);
  const puedeAvanzar = useStore((s) => s.puedeAvanzar);
  const rechazarRevision = useStore((s) => s.rechazarRevision);
  const asignarRevision = useStore((s) => s.asignarRevision);
  const showToast = useStore((s) => s.showToast);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);

  const [revisando, setRevisando] = useState<Tarea | null>(null);
  const [preview, setPreview] = useState<Tarea | null>(null);
  const [autorizar, setAutorizar] = useState<Tarea | null>(null);
  const [rechazar, setRechazar] = useState<Tarea | null>(null);
  const [asignando, setAsignando] = useState<Tarea | null>(null);


  // Antes de asignar, tanto Líder como Administrador pueden elegir revisor.
  // Ya asignada, solo Administrador puede reasignar — un Líder ya no puede tocarlo.
  const puedeAsignar = (t: Tarea) =>
    t.lider_revision_asignado ? user.rol === 'Administrador' : user.rol === 'Administrador' || user.rol === 'Líder';

  const enRevision = useMemo(() => tareas.filter((t) => t.estado === 'en revisión' && !t.eliminada), [tareas]);
  const grandes = enRevision.filter((t) => t.tarea_padre_id === null);
  const pequenas = enRevision.filter((t) => t.tarea_padre_id !== null);
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
          <h2 className="flex items-center gap-2 text-sm font-semibold text-text"><Icono nombre="carpeta" className="h-4 w-4" /> Tareas grandes → dev</h2>
          <span className="chip border border-border bg-surface-2 text-muted">{grandes.length}</span>
          {!puedeActuarGrande && <ReadOnlyTag />}
        </div>
        {grandes.length === 0 ? (
          <EmptyState icon="revision" title="Nada esperando merge a dev" />
        ) : (
          <div className="acotado space-y-2 pr-1">
            {grandes.map((t) => (
              <div
                key={t.id}
                // Click en cualquier parte abre sus detalles, igual que en Tareas pequeñas.
                onClick={puedeActuarGrande ? () => setRevisando(t) : undefined}
                className={`card flex flex-wrap items-center gap-4 p-4 ${
                  t.lider_revision_asignado ? 'bg-fuchsia-500/5' : ''
                } ${puedeActuarGrande ? 'cursor-pointer transition hover:border-border-strong' : ''}`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-[11px] text-faint">{t.id}</span>
                    <span className="font-medium text-text">{t.titulo}</span>
                  </div>
                  <div className="mt-1.5 flex flex-wrap items-center gap-2">
                    <TipoBadge tipo={t.tipo} />
                    <span className="font-mono text-[11px] text-accent"><IconoEnLinea nombre="branches" />{t.rama_asociada}</span>
                    {t.asignado_a && <Avatar email={t.asignado_a} size={22} />}
                  </div>
                </div>

                {puedeActuarGrande ? (
                  // Los botones hacen lo suyo sin abrir los detalles de la fila.
                  <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
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
          <h2 className="flex items-center gap-2 text-sm font-semibold text-text"><Icono nombre="carpeta" className="h-4 w-4" /> Tareas pequeñas → tarea padre</h2>
          <span className="chip border border-border bg-surface-2 text-muted">
            {pequenas.length}
          </span>
        </div>
        {pequenas.length === 0 ? (
          <EmptyState icon="pieza" title="No hay subtareas esperando revisión" />
        ) : (
          <div className="acotado space-y-2 pr-1">
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
        description={`La tarea "${rechazar?.titulo}" volverá a "en producción" con el motivo indicado.`}
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
          {padre && <button className="btn-outline btn-sm" onClick={onPreRevisar}><Icono nombre="ojo" className="h-3.5 w-3.5" /> Pre-revisar</button>}
          {/* Siempre: no hay mejor forma de revisar que ver el trabajo en Unity. */}
          <button className="btn-outline btn-sm" onClick={onAbrirUnity} disabled={!rama} title={rama ?? 'Sin rama'}>
            <Icono nombre="unity" className="h-3.5 w-3.5" /> Abrir en Unity
          </button>
          <button className="btn-danger btn-sm" onClick={onRechazar}>Rechazar</button>
          <button className="btn-primary btn-sm" onClick={onAutorizar}><Icono nombre="check" className="h-3.5 w-3.5" sinColor /> Autorizar</button>
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
            <span className="font-mono text-[11px] text-accent"><IconoEnLinea nombre="branches" />{tarea.rama_asociada}</span>
          )}
        </div>
        {padre && <p className="text-xs text-muted">↳ subtarea de {padre.titulo}</p>}
        {tarea.archivo_ruta && <p className="font-mono text-xs text-faint">{tarea.archivo_ruta}</p>}
        <CambiosTraidos tarea={tarea} />
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
            <div className="flex justify-center"><Icono nombre="cubo" className="h-12 w-12" /></div>
            <p className="mt-3 text-sm text-muted">Visor de malla 3D (glTF)</p>
            <p className="text-xs text-faint">12.4k polígonos · normales visibles{tarea.tipo === 'Animación' ? ' · animación en loop' : ''}</p>
            <button className="btn-outline btn-sm mt-3"><Icono nombre="unity" className="h-3.5 w-3.5" /> Abrir en Unity</button>
          </div>
        </div>
      );
    case 'Audio':
    case 'Música':
      return (
        <div className={box} style={{ height: 200 }}>
          <div className="w-full max-w-md px-6">
            <div className="flex justify-center"><Icono nombre="altavoz" className="h-10 w-10" /></div>
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
          <div className="text-center"><div className="flex justify-center"><Icono nombre="video" className="h-12 w-12" /></div><p className="mt-2 text-sm text-muted">Reproductor de video embebido</p></div>
        </div>
      );
    default:
      return (
        <div className={box} style={{ height: 300 }}>
          <div>
            <div className="flex justify-center"><Icono nombre={tipoMeta[tarea.tipo].icono} className="h-12 w-12" color={tipoMeta[tarea.tipo].color} /></div>
            <p className="mt-3 text-sm text-muted">Visor de imagen</p>
            <p className="text-xs text-faint">{tarea.archivo_ruta}</p>
          </div>
        </div>
      );
  }
}
