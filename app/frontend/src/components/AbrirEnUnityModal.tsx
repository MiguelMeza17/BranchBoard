import { useState } from 'react';
import { useStore } from '../store/useStore';
import { estadoMeta, palabraCambios } from '../lib/ui';
import { Modal } from './Modal';
import { ramaDeTarea } from './AccionesRama';

// Panel "Abrir en Unity" (solo frontend, simulado): elige con qué tarea vas a trabajar. Click en
// una tarea: el workspace cambia a su rama (en el backend real: `cm switch`) y se abre el Editor de
// Unity parado en ella. Cada tarea despliega sus commits: click en uno abre Unity en ese commit
// (changeset) de la rama.
export function AbrirEnUnityModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const user = useStore((s) => s.currentUser());
  const tareas = useStore((s) => s.tareas);
  const ramaWorkspace = useStore((s) => s.ramaWorkspace);
  const abrirEnUnity = useStore((s) => s.abrirEnUnity);
  const commits = useStore((s) => s.commits);
  const [desplegada, setDesplegada] = useState<string | null>(null);
  const [ramaWs, commitWs] = (ramaWorkspace ?? '').split(' @ ');
  const tareaWs = ramaWorkspace ? tareas.find((t) => t.rama_asociada === ramaWs) : undefined;

  // Rama con la que se trabaja una tarea (ver ramaDeTarea).
  const ramaDe = (t: (typeof tareas)[number]) => ramaDeTarea(t, tareas);
  const mias = tareas
    .filter((t) => !t.eliminada && t.asignado_a === user.email && t.estado !== 'terminada' && ramaDe(t))
    .sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true }));

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="¿Qué tarea vas a trabajar en Unity?"
      subtitle={`${user.nombre} ${user.apellido} · ${user.rol}`}
    >
      {/* Dónde está parado el workspace: si es una de tus tareas, se marca en azul en la lista. Aquí arriba
          solo sale si no está en ninguna, o si está en la rama de una tarea que no es tuya. */}
      {!ramaWorkspace ? (
        // Sin rama: nunca se asume dev (trabajar ahí metería cambios directo en la integración).
        <div className="mb-3 rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm font-medium text-amber-200">
          Tu workspace no está en ninguna tarea
        </div>
      ) : (
        !mias.some((t) => ramaDe(t) === ramaWs) && (
          <div className="mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm">
            <span className="text-faint">Tu workspace está en</span>
            {tareaWs ? (
              <span className="text-text">
                <span className="font-mono font-semibold">{tareaWs.id}</span> {tareaWs.titulo}
              </span>
            ) : (
              <span className="font-mono text-text">{ramaWs}</span>
            )}
            {commitWs && <span className="font-mono text-xs text-faint">@ {commitWs}</span>}
          </div>
        )
      )}

      {mias.length === 0 ? (
        <div className="rounded-lg border border-border bg-surface-2 px-4 py-6 text-center text-sm text-muted">
          No tienes tareas abiertas con rama.
        </div>
      ) : (
        <div className="max-h-[360px] space-y-1.5 overflow-y-auto">
          {mias.map((t) => {
            const m = estadoMeta[t.estado];
            const rama = ramaDe(t)!;
            const actual = !!ramaWorkspace && (ramaWorkspace === rama || ramaWorkspace.startsWith(`${rama} @ `));
            const suyos = commits
              .filter((c) => c.tarea_id === t.id)
              .sort((a, b) => b.fecha.localeCompare(a.fecha));
            const abierta = desplegada === t.id;
            return (
              <div
                key={t.id}
                className={`rounded-lg border ${actual ? 'border-primary/50 bg-primary-soft' : 'border-border bg-surface-2'}`}
              >
                <div className="flex items-center">
                  <button
                    type="button"
                    onClick={() => {
                      abrirEnUnity(t.id, rama);
                      onClose();
                    }}
                    className="flex min-w-0 flex-1 items-center gap-3 rounded-l-lg px-3 py-2.5 text-left text-sm transition hover:bg-surface-3"
                  >
                    <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: m.dot }} />
                    <span className="shrink-0 font-mono font-semibold text-text">{t.id}</span>
                    <span className="min-w-0 flex-1">
                      {actual && (
                        <span className="block text-[11px] font-semibold text-primary">
                          Tu workspace está en
                          {commitWs && <span className="ml-1 font-mono font-normal text-faint">@ {commitWs}</span>}
                        </span>
                      )}
                      <span className="block truncate text-text">{t.titulo}</span>
                      <span className="block truncate font-mono text-[11px] text-faint">{rama}</span>
                    </span>
                  </button>
                  {/* Desplegar sus commits */}
                  <button
                    type="button"
                    onClick={() => setDesplegada(abierta ? null : t.id)}
                    disabled={!suyos.length}
                    className="shrink-0 self-stretch rounded-r-lg border-l border-border px-3 text-[11px] text-faint transition enabled:hover:bg-surface-3 enabled:hover:text-text disabled:opacity-50"
                    title={suyos.length ? 'Ver sus commits' : 'Sin commits'}
                  >
                    {suyos.length} {palabraCambios(suyos.length)} {suyos.length ? (abierta ? '▾' : '▸') : ''}
                  </button>
                </div>
                {abierta && (
                  <div className="space-y-0.5 border-t border-border px-2 py-1.5">
                    {suyos.map((c, i) => {
                      const enEste = ramaWorkspace === `${rama} @ ${c.hash}`;
                      return (
                        <button
                          key={c.id}
                          type="button"
                          onClick={() => {
                            abrirEnUnity(t.id, rama, c.hash);
                            onClose();
                          }}
                          className={`flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-left text-xs transition hover:bg-surface-3 ${enEste ? 'bg-surface-3' : ''}`}
                        >
                          <span className="shrink-0 rounded border border-border bg-surface px-1.5 py-0.5 font-mono text-[11px] text-muted">
                            {c.hash}
                          </span>
                          <span className="min-w-0 flex-1 truncate text-text">{c.mensaje}</span>
                          <span className="shrink-0 font-mono text-[11px] text-faint">{c.fecha.slice(0, 10)}</span>
                          {i === 0 && <span className="shrink-0 text-[10px] text-faint">último</span>}
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </Modal>
  );
}
