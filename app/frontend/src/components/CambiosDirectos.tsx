import { useStore } from '../store/useStore';
import { esLiderOAdmin } from '../lib/permisos';
import type { Tarea } from '../types';

/** `2026-11-17T10:00:00Z` → fecha y hora locales cortas. */
function fechaLocal(iso: string): string {
  const f = new Date(iso);
  return Number.isNaN(f.getTime())
    ? iso
    : f.toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });
}

/** `/main/dev` → `dev`, `/main` → `main`. */
export function ramaCorta(rama: string): string {
  return rama.replace(/^\/main\/?/, '') || 'main';
}

/** Tarea roja (#225) que sigue con sus cambios solo en `main`/`dev`. */
export function esTareaRoja(t: Tarea): boolean {
  return !!t.cambios_directos && !t.cambios_directos.trasladado_en;
}

/**
 * Sección de los detalles de una tarea roja (#225): en qué rama están los check-ins directos, de quién y
 * qué archivos tocan, con el botón "Trasladar a una rama" para Líder/Administrador. Ya trasladada, queda
 * como registro de dónde estaban y a qué rama pasaron.
 */
export function CambiosDirectosSeccion({ tarea }: { tarea: Tarea }) {
  const rol = useStore((s) => s.currentUser().rol);
  const trasladar = useStore((s) => s.trasladarCambiosDirectos);
  const trasladando = useStore((s) => s.trasladandoCambiosDirectos);
  const cd = tarea.cambios_directos;
  if (!cd) return null;
  const roja = !cd.trasladado_en;

  return (
    <section
      className={`rounded-xl border p-3 ${
        roja ? 'border-red-500/60 bg-red-500/10' : 'border-border bg-surface'
      }`}
      data-prueba="cambios-directos"
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h3 className={`text-xs font-semibold ${roja ? 'text-red-300' : 'text-muted'}`}>
          {roja ? `⚠ Cambios en ${cd.rama} — trasladar a una rama y quitar de ${ramaCorta(cd.rama)}` : `Estaban en ${cd.rama}`}
        </h3>
        {roja && esLiderOAdmin(rol) && (
          <button
            type="button"
            className="btn-danger btn-sm"
            disabled={trasladando}
            onClick={() => void trasladar(tarea.id)}
          >
            {trasladando ? 'Trasladando…' : 'Trasladar a una rama'}
          </button>
        )}
      </div>
      <ul className="space-y-2">
        {cd.changesets.map((c) => (
          <li key={c.changeset} className="rounded-lg border border-border bg-surface-2 px-3 py-2">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs">
              <span className="font-mono text-text">cs:{c.changeset}</span>
              <span className="text-muted">{c.autor}</span>
              <span className="text-faint">{fechaLocal(c.fecha)}</span>
            </div>
            {c.comentario && <p className="mt-1 text-xs text-text">{c.comentario}</p>}
            {c.archivos.length > 0 && (
              <ul className="mt-1 space-y-0.5">
                {c.archivos.map((a) => (
                  <li key={a} className="truncate font-mono text-[11px] text-muted" title={a}>
                    {a}
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
      {!roja && (
        <dl className="mt-2 grid grid-cols-2 gap-2 text-xs">
          <div>
            <dt className="text-faint">Trasladados a</dt>
            <dd className="font-mono text-accent">{cd.rama_traslado}</dd>
          </div>
          <div>
            <dt className="text-faint">Por</dt>
            <dd className="text-text">
              {cd.trasladado_por} · {cd.trasladado_en ? fechaLocal(cd.trasladado_en) : ''}
            </dd>
          </div>
          {cd.changesets_reversion.length > 0 && (
            <div className="col-span-2">
              <dt className="text-faint">Revertidos en {ramaCorta(cd.rama)}</dt>
              <dd className="font-mono text-text">
                {cd.changesets_reversion.map((n) => `cs:${n}`).join(', ')}
              </dd>
            </div>
          )}
        </dl>
      )}
    </section>
  );
}
