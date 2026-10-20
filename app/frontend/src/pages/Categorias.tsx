import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { TIPOS_TAREA, type TipoTarea } from '../types';
import { tipoMeta } from '../lib/ui';
import { PageHeader } from '../components/primitives';

const CARPETAS = [...TIPOS_TAREA];

export function Categorias() {
  const navigate = useNavigate();
  const tareas = useStore((s) => s.tareas).filter((t) => !t.eliminada);

  const [ocultas, setOcultas] = useState<Set<TipoTarea>>(new Set());
  const toggleCarpeta = (id: TipoTarea) =>
    setOcultas((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div>
      <PageHeader
        title="Categorías"
        desc="Carpetas puramente visuales por tipo/disciplina. Abrir una muestra un tablero Scrum solo con esas tareas — nunca son ramas."
      />

      <div className="card p-5">
        {/* Interruptores: no a todos le interesa ver todas las categorías */}
        <div className="mb-4 flex flex-wrap items-center gap-1.5">
          {CARPETAS.map((id) => {
            const activa = !ocultas.has(id);
            const color = tipoMeta[id].color;
            return (
              <button
                key={id}
                onClick={() => toggleCarpeta(id)}
                title={activa ? `Ocultar "${id}"` : `Mostrar "${id}"`}
                className={`chip border transition ${
                  activa
                    ? 'border-border bg-surface-2 text-text'
                    : 'border-border/40 bg-transparent text-faint opacity-60'
                }`}
              >
                <svg
                  width="13"
                  height="13"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ color: activa ? color : '#475569' }}
                >
                  <path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7-11-7-11-7z" />
                  {activa ? <circle cx="12" cy="12" r="3" /> : <line x1="2" y1="2" x2="22" y2="22" />}
                </svg>
                {id}
              </button>
            );
          })}
        </div>

        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
          {CARPETAS.filter((tp) => !ocultas.has(tp)).map((tp) => {
            const n = tareas.filter((t) => t.tipo === tp).length;
            const m = tipoMeta[tp];
            return (
              <button
                key={tp}
                onClick={() => navigate(`/categorias/${encodeURIComponent(tp)}`)}
                className="card group flex flex-col items-start gap-3 p-5 text-left transition-all hover:border-border-strong hover:shadow-pop"
              >
                <div
                  className="grid h-12 w-12 place-items-center rounded-xl text-2xl transition-transform group-hover:scale-105"
                  style={{ background: `${m.color}20`, border: `1px solid ${m.color}55` }}
                >
                  {m.emoji}
                </div>
                <div>
                  <div className="font-semibold text-text">{tp}</div>
                  <div className="text-xs text-muted">{n} tarea{n === 1 ? '' : 's'}</div>
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
