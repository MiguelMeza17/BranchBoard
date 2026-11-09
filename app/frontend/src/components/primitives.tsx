import type { ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import type { EstadoTarea, TipoTarea } from '../types';
import { estadoMeta, tipoMeta, iniciales, avatarColor } from '../lib/ui';
import { useStore } from '../store/useStore';
import { Icono, IconoEnLinea } from './Icono';

export function EstadoBadge({ estado }: { estado: EstadoTarea }) {
  const m = estadoMeta[estado];
  return (
    <span className={`chip whitespace-nowrap ${m.bg} ${m.text} border ${m.border}`}>
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: m.dot }} />
      {m.label}
    </span>
  );
}

export function TipoBadge({ tipo }: { tipo: TipoTarea }) {
  const m = tipoMeta[tipo];
  return (
    <span
      className="chip whitespace-nowrap border"
      style={{ color: m.color, borderColor: `${m.color}55`, background: `${m.color}18` }}
    >
      <Icono nombre={m.icono} className="h-3.5 w-3.5" sinColor />
      {tipo}
    </span>
  );
}

export function Avatar({
  email,
  size = 28,
  sinTooltip = false,
}: {
  email: string;
  size?: number;
  /** Oculta el tooltip con el nombre (ej. en la barra superior, donde el nombre ya está escrito al lado). */
  sinTooltip?: boolean;
}) {
  const usuarios = useStore((s) => s.usuarios);
  const navigate = useNavigate();
  const u = usuarios.find((x) => x.email === email);
  const nombre = u ? `${u.nombre} ${u.apellido}` : email;
  const label = u ? iniciales(u.nombre, u.apellido) : email.slice(0, 2).toUpperCase();
  // Todos los roles pueden ver el perfil básico de cualquier persona del equipo.
  const destino = `/perfil/${encodeURIComponent(email)}`;
  return (
    <span className="group/avatar relative inline-flex">
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          navigate(destino);
        }}
        className="inline-flex shrink-0 items-center justify-center rounded-full font-semibold text-white ring-1 ring-black/20 transition hover:ring-2 hover:ring-primary/70"
        style={{ width: size, height: size, background: avatarColor(email), fontSize: size * 0.38 }}
        aria-label={`Ver perfil de ${nombre}`}
      >
        {label}
      </button>
      {!sinTooltip && (
        <span className="pointer-events-none absolute -top-1.5 left-1/2 z-20 -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-md border border-border bg-surface-3 px-2 py-1 text-xs font-medium text-text opacity-0 shadow-pop transition-opacity duration-150 group-hover/avatar:opacity-100">
          {nombre}
        </span>
      )}
    </span>
  );
}

export function PageHeader({
  title,
  desc,
  actions,
}: {
  title: string;
  desc?: string;
  actions?: ReactNode;
}) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-2xl font-bold tracking-tight text-text">{title}</h1>
        {desc && <p className="mt-1 max-w-2xl text-sm text-muted">{desc}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  );
}

export function EmptyState({
  icon = 'bandeja',
  title,
  desc,
  action,
}: {
  icon?: string;
  title: string;
  desc?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border py-16 text-center">
      <div className="mb-3 grid h-14 w-14 place-items-center rounded-2xl bg-surface-2">
        <Icono nombre={icon} className="h-7 w-7" />
      </div>
      <h3 className="text-base font-semibold text-text">{title}</h3>
      {desc && <p className="mt-1 max-w-sm text-sm text-muted">{desc}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function ReadOnlyTag() {
  return (
    <span className="chip border border-border bg-surface-2 text-faint"><IconoEnLinea nombre="ojo" sinColor />Solo lectura</span>
  );
}

export function Card({
  children,
  className = '',
  id,
  onClick,
}: {
  children: ReactNode;
  className?: string;
  id?: string;
  onClick?: () => void;
}) {
  return (
    <div id={id} onClick={onClick} className={`card p-5 ${className}`}>
      {children}
    </div>
  );
}
