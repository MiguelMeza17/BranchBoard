import { useStore } from '../store/useStore';

export function Toast() {
  const toast = useStore((s) => s.toast);
  if (!toast) return null;
  const tone =
    toast.tone === 'danger'
      ? 'border-danger/50 bg-danger/15 text-red-200'
      : toast.tone === 'info'
      ? 'border-primary/50 bg-primary-soft text-indigo-100'
      : 'border-done/50 bg-green-500/15 text-green-100';
  return (
    <div className="fixed bottom-6 left-1/2 z-[60] -translate-x-1/2 animate-slide-in">
      <div className={`flex items-center gap-2 rounded-xl border px-4 py-2.5 text-sm font-medium shadow-pop ${tone}`}>
        <span>{toast.msg}</span>
      </div>
    </div>
  );
}
