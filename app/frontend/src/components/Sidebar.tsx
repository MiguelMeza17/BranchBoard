import { NavLink } from 'react-router-dom';
import { useStore } from '../store/useStore';

interface NavItem {
  to: string;
  icon: string;
  label: string;
}

const NAV: NavItem[] = [
  { to: '/scrum', icon: '🗂️', label: 'Tareas (Scrum)' },
  { to: '/lista', icon: '📋', label: 'Lista de tareas' },
  { to: '/categorias', icon: '🗃️', label: 'Categorías' },
  { to: '/revision', icon: '🔀', label: 'Revisión' },
  { to: '/resumen', icon: '📊', label: 'Resumen' },
  { to: '/avance', icon: '📈', label: 'Avance' },
  { to: '/pendientes', icon: '📌', label: 'Pendientes' },
  { to: '/reportes', icon: '📄', label: 'Reportes' },
  { to: '/branches', icon: '🌳', label: 'Branches' },
  { to: '/papelera', icon: '🗑️', label: 'Papelera' },
  { to: '/ajustes', icon: '⚙️', label: 'Ajustes' },
];

// Clases literales (no armadas con template) para que Tailwind las genere.
const PUNTO_ROJO = { solido: 'bg-red-500', halo: 'bg-red-500/25' };
const PUNTO_AZUL = { solido: 'bg-primary', halo: 'bg-primary/25' };

interface SidebarProps {
  colapsado: boolean;
  onToggle: () => void;
}

export function Sidebar({ colapsado, onToggle }: SidebarProps) {
  const tareas = useStore((s) => s.tareas);
  const edicionesPendientes = useStore((s) => s.edicionesPendientes);
  const sprints = useStore((s) => s.sprints);

  const totalPendientes =
    tareas.filter((t) => t.pendiente_aprobacion && !t.eliminada).length + edicionesPendientes.length;
  const totalPapelera = tareas.filter((t) => t.eliminada).length;
  const totalRevision = tareas.filter((t) => t.estado === 'en revisión' && !t.eliminada).length;

  const hayReporteActivo = sprints.some((s) => s.con_fecha);
  const hayCambioBranches = tareas.some((t) => !t.eliminada && t.rama_asociada);
  const hayTareasActivas = tareas.some((t) => !t.eliminada);
  const rutasConPuntoAzul = new Set(['/scrum', '/resumen', '/lista', '/avance', '/categorias']);

  const badgeByPath: Record<string, number> = {
    '/pendientes': totalPendientes,
    '/papelera': totalPapelera,
    '/revision': totalRevision,
  };

  const colorBadgePorRuta: Record<string, string> = {
    '/pendientes': 'bg-amber-500/20 text-amber-300',
    '/papelera': 'bg-red-500/20 text-red-300',
    '/revision': 'bg-fuchsia-500/20 text-fuchsia-300',
  };

  const puntoDeRuta = (to: string) => {
    if (to === '/reportes' && hayReporteActivo) return PUNTO_ROJO;
    if (to === '/branches' && hayCambioBranches) return PUNTO_AZUL;
    if (rutasConPuntoAzul.has(to) && hayTareasActivas) return PUNTO_AZUL;
    return null;
  };

  const etiquetaToggle = colapsado ? 'Expandir menú' : 'Contraer menú';

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-border bg-surface transition-[width] duration-200 ${
        colapsado ? 'w-16' : 'w-60'
      }`}
    >
      <div className={`flex items-center gap-2.5 pt-5 ${colapsado ? 'justify-center px-2' : 'px-5'}`}>
        <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-primary text-lg shadow-sm">
          {/* placeholder: aquí va el icono/imagen definitiva */}
        </div>
        {!colapsado && (
          <div className="min-w-0 flex-1 leading-tight">
            <div className="text-sm font-bold text-text">BranchBoard</div>
            <div className="font-mono text-[11px] text-faint">VirusTF</div>
          </div>
        )}
      </div>

      {/* El botón va siempre debajo de la cabecera y a la misma distancia del borde
          (18px = centrado en la franja contraída de 64px), así no se mueve al alternar. */}
      <div className="px-[18px] pb-3 pt-2">
        <button
          onClick={onToggle}
          className="grid h-7 w-7 shrink-0 place-items-center rounded-lg text-muted transition-colors hover:bg-surface-3 hover:text-text"
          title={etiquetaToggle}
          aria-label={etiquetaToggle}
          aria-expanded={!colapsado}
        >
          <svg viewBox="0 0 20 20" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <path d="M3 5h14M3 10h14M3 15h14" />
          </svg>
        </button>
      </div>

      {/* Contraído: no se muestra ningún botón de navegación, solo la franja con el logo y el toggle. */}
      <nav className={`flex-1 space-y-0.5 overflow-y-auto px-3 pb-4 ${colapsado ? 'hidden' : ''}`}>
        {NAV.map((item) => {
          const badge = badgeByPath[item.to];
          const punto = puntoDeRuta(item.to);

          return (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) => `nav-link ${isActive ? 'nav-link-active' : ''}`}
            >
              <span className="text-base leading-none">{item.icon}</span>
              <span className="truncate">{item.label}</span>
              {punto && (
                <span className="relative ml-auto grid h-5 w-5 shrink-0 place-items-center">
                  <span className={`absolute h-2.5 w-2.5 rounded-full blur-[2px] ${punto.halo}`} />
                  <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${punto.solido}`} />
                </span>
              )}
              {badge !== undefined && (
                <span
                  className={`ml-auto grid h-5 min-w-5 place-items-center rounded-full px-1 text-[10px] font-bold ${
                    badge > 0 ? colorBadgePorRuta[item.to] ?? 'bg-primary text-white' : 'bg-surface-3 text-faint'
                  }`}
                >
                  {badge}
                </span>
              )}
            </NavLink>
          );
        })}
      </nav>

      {!colapsado && (
        <div className="border-t border-border px-4 py-3 text-[11px] text-faint">
          Mockup UI · datos en memoria
        </div>
      )}
    </aside>
  );
}
