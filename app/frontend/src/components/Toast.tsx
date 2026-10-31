import { useStore } from '../store/useStore';

export function Toast() {
  const toast = useStore((s) => s.toast);
  const clearToast = useStore((s) => s.clearToast);
  const tone =
    toast?.tone === 'danger'
      ? 'border-danger/50 bg-danger/15 text-red-200'
      : toast?.tone === 'info'
      ? 'border-primary/50 bg-primary-soft text-indigo-100'
      : 'border-done/50 bg-green-500/15 text-green-100';
  // La región viva queda montada siempre: los lectores de pantalla solo anuncian
  // los cambios dentro de una región que ya existía. Los avisos "danger" van como alerta.
  return (
    <div
      role="status"
      aria-live="polite"
      aria-atomic="true"
      className="fixed bottom-6 left-1/2 z-[60] -translate-x-1/2"
    >
      {toast && (
        <div
          key={toast.id}
          role={toast.tone === 'danger' ? 'alert' : undefined}
          className={`flex items-center gap-2 rounded-xl border py-2.5 pl-4 pr-2 text-sm font-medium shadow-pop animate-slide-in ${tone}`}
        >
          <span>{toast.msg}</span>
          <button
            type="button"
            onClick={clearToast}
            className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-base leading-none opacity-70 transition hover:bg-white/10 hover:opacity-100"
            aria-label="Cerrar aviso"
          >
            ×
          </button>
        </div>
      )}
    </div>
  );
}
