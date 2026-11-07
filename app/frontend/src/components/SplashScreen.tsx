import { useEffect, useState } from 'react';
import './SplashScreen.css';

// Animación de bienvenida: dibuja un grafo de ramas (git) que forma una B — el tronco, una rama abajo
// que se une al nodo del medio y otra arriba que llega al nodo final — y termina en el wordmark. Los
// puntos solo van donde las ramas se conectan con el tronco y aparecen cuando cada rama llega.
// Puramente decorativo: no bloquea nada real, solo tapa el primer render.
export function SplashScreen({ onDone }: { onDone: () => void }) {
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    const t1 = setTimeout(() => setExiting(true), 2400);
    const t2 = setTimeout(() => onDone(), 2850);
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
    };
  }, [onDone]);

  return (
    <div className={`splash-overlay ${exiting ? 'splash-exit' : ''}`}>
      <div className="flex flex-col items-center gap-5">
        <svg className="splash-graph" viewBox="0 0 110 138" fill="none">
          <defs>
            <radialGradient id="splash-halo-amarillo">
              <stop offset="0%" stopColor="#fde047" stopOpacity="0.95" />
              <stop offset="45%" stopColor="#facc15" stopOpacity="0.45" />
              <stop offset="100%" stopColor="#facc15" stopOpacity="0" />
            </radialGradient>
          </defs>
          <path className="splash-path splash-path--trunk" pathLength={1} d="M28,124 L28,17" />
          {/* Panza de abajo de la B: sale del nodo inicial y se une al del medio. */}
          <path
            className="splash-path splash-path--branch-low"
            pathLength={1}
            d="M28,124 C28,112 70,112 70,101 C70,90 28,90 28,78"
          />
          {/* Panza de arriba: sale del nodo del medio y llega al final. */}
          <path
            className="splash-path splash-path--branch"
            pathLength={1}
            d="M28,78 C28,62 76,62 76,47.5 C76,33 28,33 28,17"
          />
          <g className="splash-node splash-node--1">
            <circle cx="28" cy="124" r="6" fill="#6b7385" />
          </g>
          <g className="splash-node splash-node--2">
            <circle cx="28" cy="78" r="6" fill="#6366f1" />
          </g>
          <g className="splash-node splash-node--4">
            {/* Halo del último nodo: late detrás del punto. */}
            <circle className="splash-halo" cx="28" cy="17" r="20" fill="url(#splash-halo-amarillo)" />
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
