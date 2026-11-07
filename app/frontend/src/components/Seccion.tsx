import type { ReactNode } from 'react';

// Panel plegable: cerrado solo muestra el nombre; al hacer click se despliega hacia abajo. La flecha
// indica que se puede abrir y gira al desplegarse.
export function Seccion({ title, children, danger }: { title: string; children: ReactNode; danger?: boolean }) {
  return (
    <details className={`card group overflow-hidden ${danger ? 'border-danger/40' : ''}`}>
      <summary
        className={`flex cursor-pointer list-none items-center gap-3 px-5 py-4 transition-colors hover:bg-surface-3/50 [&::-webkit-details-marker]:hidden ${
          danger ? 'text-red-300' : 'text-text'
        }`}
      >
        <svg
          viewBox="0 0 20 20"
          className="h-4 w-4 shrink-0 text-faint transition-transform duration-200 group-open:rotate-90"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M7 4l6 6-6 6" />
        </svg>
        <h2 className="flex-1 text-sm font-semibold">{title}</h2>
      </summary>
      <div className="border-t border-border px-5 pb-5 pt-4">{children}</div>
    </details>
  );
}
