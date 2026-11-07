import { useEffect, useState } from 'react';
import './SplashScreen.css';

// Animación de bienvenida: dibuja un grafo de rama/merge (git) que termina en el
// wordmark. Puramente decorativo — no bloquea nada real, solo tapa el primer render.
export function SplashScreen({ onDone }: { onDone: () => void }) {
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    const t1 = setTimeout(() => setExiting(true), 2300);
    const t2 = setTimeout(() => onDone(), 2750);
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
    };
  }, [onDone]);

  return (
    <div className={`splash-overlay ${exiting ? 'splash-exit' : ''}`}>
      <div className="flex flex-col items-center gap-5">
        <svg className="splash-graph" viewBox="0 0 110 138" fill="none">
          <path className="splash-path splash-path--trunk" d="M28,124 L28,14" />
          <path
            className="splash-path splash-path--branch"
            d="M28,88 C28,70 72,70 72,52 C72,34 28,34 28,17"
          />
          <g className="splash-node splash-node--1">
            <circle cx="28" cy="124" r="6" fill="#6b7385" />
          </g>
          <g className="splash-node splash-node--2">
            <circle cx="28" cy="88" r="6" fill="#6366f1" />
          </g>
          <g className="splash-node splash-node--3">
            <circle cx="72" cy="52" r="6" fill="#2dd4bf" />
          </g>
          <g className="splash-node splash-node--4">
            <circle cx="28" cy="17" r="6" fill="#facc15" />
          </g>
        </svg>
        <div className="flex flex-col items-center gap-1">
          <div className="splash-wordmark flex items-baseline text-3xl font-bold tracking-tight">
            <span className="text-text">Branch</span>
            <span className="text-green-400">Board</span>
          </div>
          <div className="splash-company text-[11px] font-medium tracking-wide text-faint">Piveta</div>
        </div>
        <div className="splash-tagline text-xs font-medium uppercase tracking-[0.2em] text-muted">
          ramas bajo control
        </div>
      </div>
    </div>
  );
}
