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

export function Sidebar() {
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

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-surface">
      <div className="flex items-center gap-2.5 px-5 py-5">
        <div className="grid h-9 w-9 place-items-center rounded-xl bg-primary text-lg shadow-sm">
          {/* placeholder: aquí va el icono/imagen definitiva */}
        </div>
        <div className="leading-tight">
          <div className="text-sm font-bold text-text">BranchBoard</div>
          <div className="font-mono text-[11px] text-faint">VirusTF</div>
        </div>
      </div>

      <nav className="flex-1 space-y-0.5 overflow-y-auto px-3 pb-4">
        {NAV.map((item) => {
          const badge = badgeByPath[item.to];
          return (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) => `nav-link ${isActive ? 'nav-link-active' : ''}`}
            >
              <span className="text-base leading-none">{item.icon}</span>
              <span className="truncate">{item.label}</span>
              {item.to === '/reportes' && hayReporteActivo && (
                <span className="relative ml-auto grid h-5 w-5 shrink-0 place-items-center">
                  <span className="absolute h-2.5 w-2.5 rounded-full bg-red-500/25 blur-[2px]" />
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-red-500" />
                </span>
              )}
              {item.to === '/branches' && hayCambioBranches && (
                <span className="relative ml-auto grid h-5 w-5 shrink-0 place-items-center">
                  <span className="absolute h-2.5 w-2.5 rounded-full bg-primary/25 blur-[2px]" />
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
                </span>
              )}
              {rutasConPuntoAzul.has(item.to) && hayTareasActivas && (
                <span className="relative ml-auto grid h-5 w-5 shrink-0 place-items-center">
                  <span className="absolute h-2.5 w-2.5 rounded-full bg-primary/25 blur-[2px]" />
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
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

      <div className="border-t border-border px-4 py-3 text-[11px] text-faint">
        Mockup UI · datos en memoria
      </div>
    </aside>
  );
}
