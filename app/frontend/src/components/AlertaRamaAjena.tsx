import { useStore } from '../store/useStore';

/**
 * Alerta inmediata de #111: el workspace de esta persona tiene cambios en `main`, `dev` o la rama de
 * otra persona. La manda el backend (evento `alerta-rama-ajena`); no deshace nada, solo avisa.
 */
export function AlertaRamaAjena() {
  const alerta = useStore((s) => s.alertaRamaAjena);
  const cerrar = useStore((s) => s.cerrarAlertaRamaAjena);
  if (!alerta) return null;
  const visibles = alerta.archivos.slice(0, 5);
  const resto = alerta.archivos.length - visibles.length;
  return (
    <div
      role="alert"
      data-prueba="alerta-rama-ajena"
      className="mb-4 rounded-xl border border-red-500/60 bg-red-500/10 px-4 py-3 text-sm"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="font-semibold text-red-300">⚠ {alerta.mensaje}</p>
          <p className="mt-1 font-mono text-xs text-muted">{alerta.rama_actual}</p>
          <ul className="mt-1 space-y-0.5">
            {visibles.map((a) => (
              <li key={a} className="truncate font-mono text-[11px] text-muted" title={a}>
                {a}
              </li>
            ))}
            {resto > 0 && <li className="text-[11px] text-faint">+{resto}</li>}
          </ul>
        </div>
        <button type="button" className="btn-ghost btn-sm shrink-0" onClick={cerrar}>
          Cerrar
        </button>
      </div>
    </div>
  );
}
