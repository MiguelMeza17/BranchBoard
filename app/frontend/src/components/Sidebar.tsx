import { useEffect, useRef, useState } from 'react';
import { NavLink } from 'react-router-dom';
import { Modal } from './Modal';
import { SelectorCarpeta } from './SelectorCarpeta';
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

  // Menú del proyecto (click en BranchBoard): se cierra al hacer click fuera.
  const cambiarDeProyecto = useStore((s) => s.cambiarDeProyecto);
  const entrarAProyecto = useStore((s) => s.entrarAProyecto);
  const cerrarSesion = useStore((s) => s.cerrarSesion);
  const workspaces = useStore((s) => s.workspaces);
  const [confirmarCerrarSesion, setConfirmarCerrarSesion] = useState(false);
  // "Nuevo proyecto": traer uno local de Unity VC que aún no usa BranchBoard, o crear uno nuevo.
  const [nuevoProyecto, setNuevoProyecto] = useState(false);
  const sinBranchBoard = workspaces.filter((w) => !w.tiene_db_branchboard);
  const setWorkspaces = useStore((s) => s.setWorkspaces);
  const setRolActual = useStore((s) => s.setRolActual);
  const [nombreNuevo, setNombreNuevo] = useState('');
  const [carpetaNueva, setCarpetaNueva] = useState('D:\\Unity');
  const cerrarNuevo = () => {
    setNuevoProyecto(false);
    setNombreNuevo('');
  };
  const proyectoActual = useStore((s) => s.proyectoActual);
  const showToast = useStore((s) => s.showToast);
  const [menuProyecto, setMenuProyecto] = useState(false);
  const [confirmarSalir, setConfirmarSalir] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menuProyecto) return;
    const fuera = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenuProyecto(false);
    };
    document.addEventListener('mousedown', fuera);
    return () => document.removeEventListener('mousedown', fuera);
  }, [menuProyecto]);
  const salir = () => {
    setConfirmarSalir(false);
    // En la app de escritorio (Tauri) se cierra la ventana. En el navegador, el mockup solo puede
    // cerrar la pestaña si la abrió un script; si no, avisa.
    window.close();
    showToast('En la app de escritorio, aquí se cierra BranchBoard', 'info');
  };

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-border bg-surface transition-[width] duration-200 ${
        colapsado ? 'w-16' : 'w-60'
      }`}
    >
      {/* Cabecera: click en el logo/nombre abre el menú del proyecto (cambiar de proyecto o salir). */}
      <div ref={menuRef} className={`relative pt-5 ${colapsado ? 'px-2' : 'px-3'}`}>
        <button
          type="button"
          onClick={() => setMenuProyecto((v) => !v)}
          title="Tus proyectos o salir"
          aria-expanded={menuProyecto}
          className={`flex w-full items-center gap-2.5 rounded-xl py-1 text-left transition hover:bg-surface-3 ${
            colapsado ? 'justify-center px-0' : 'px-2'
          }`}
        >
          <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-primary text-lg shadow-sm">
            {/* placeholder: aquí va el icono/imagen definitiva */}
          </div>
          {!colapsado && (
            <>
              <div className="min-w-0 flex-1 leading-tight">
                <div className="text-sm font-bold text-text">BranchBoard</div>
                <div className="truncate font-mono text-[11px] text-faint">{proyectoActual}</div>
              </div>
              <span className="text-xs text-faint">▾</span>
            </>
          )}
        </button>
        {menuProyecto && (
          <div
            className={`absolute top-full z-30 mt-1 w-64 overflow-hidden rounded-xl border border-border bg-surface-2 py-1 shadow-pop ${
              colapsado ? 'left-2' : 'left-3'
            }`}
          >
            {/* Tus proyectos: solo los que ya usan BranchBoard en este equipo. Los que hay que clonar o
                configurar por primera vez van en "Nuevo proyecto". */}
            <div className="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-faint">Tus proyectos</div>
            {workspaces
              .filter((ws) => ws.tiene_db_branchboard)
              .map((ws) => {
                const actual = ws.nombre === proyectoActual;
                return (
                  <button
                    key={ws.repo}
                    type="button"
                    disabled={actual}
                    className={`flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition ${
                      actual ? 'cursor-default bg-primary-soft text-text' : 'text-text hover:bg-surface-3'
                    }`}
                    onClick={() => {
                      setMenuProyecto(false);
                      entrarAProyecto(ws.nombre);
                      showToast(`Cambiaste al proyecto ${ws.nombre}`);
                    }}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{ws.nombre}</span>
                      <span className="block truncate font-mono text-[10px] text-faint">{ws.ruta}</span>
                    </span>
                    {actual && <span className="shrink-0 text-[10px] text-primary">actual</span>}
                  </button>
                );
              })}
            <button
              type="button"
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-primary transition hover:bg-surface-3"
              onClick={() => {
                setMenuProyecto(false);
                setNuevoProyecto(true);
              }}
            >
              ➕ Nuevo proyecto
            </button>
            <div className="my-1 border-t border-border" />
            <button
              type="button"
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-text transition hover:bg-surface-3"
              onClick={() => {
                setMenuProyecto(false);
                setConfirmarCerrarSesion(true);
              }}
            >
              Cerrar sesión
            </button>
            <button
              type="button"
              className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-red-300 transition hover:bg-red-500/10"
              onClick={() => {
                setMenuProyecto(false);
                setConfirmarSalir(true);
              }}
            >
              ⏻ Salir de la app
            </button>
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

      {/* Contraído: solo los íconos (centrados, con el nombre al pasar el mouse); el número o el punto
          de aviso van como una marca pequeña en la esquina del ícono. */}
      <nav className={`flex-1 space-y-0.5 overflow-y-auto pb-4 ${colapsado ? 'px-2' : 'px-3'}`}>
        {NAV.map((item) => {
          const badge = badgeByPath[item.to];
          const punto = puntoDeRuta(item.to);

          return (
            <NavLink
              key={item.to}
              to={item.to}
              title={colapsado ? item.label + (badge ? ` (${badge})` : '') : undefined}
              aria-label={item.label}
              className={({ isActive }) =>
                `nav-link ${isActive ? 'nav-link-active' : ''} ${colapsado ? 'relative justify-center px-0' : ''}`
              }
            >
              <span className="text-base leading-none">{item.icon}</span>
              {colapsado ? (
                <>
                  {badge !== undefined && badge > 0 ? (
                    <span
                      className={`absolute right-1 top-0.5 grid h-4 min-w-4 place-items-center rounded-full px-0.5 text-[9px] font-bold ${
                        colorBadgePorRuta[item.to] ?? 'bg-primary text-white'
                      }`}
                    >
                      {badge}
                    </span>
                  ) : (
                    punto && <span className={`absolute right-2 top-1.5 h-1.5 w-1.5 rounded-full ${punto.solido}`} />
                  )}
                </>
              ) : (
                <>
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
                </>
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
      <Modal
        open={nuevoProyecto}
        onClose={cerrarNuevo}
        title="Nuevo proyecto"
        subtitle="Trae un proyecto de Unity VC que ya tienes en este equipo, o crea uno nuevo"
        size="md"
      >
        <div className="space-y-5 text-sm">
          <div>
            <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">
              Traer un proyecto local de Unity VC
            </div>
            {sinBranchBoard.length === 0 ? (
              <p className="text-[12px] text-faint">No tienes proyectos de Unity VC en este equipo que no estén ya en BranchBoard.</p>
            ) : (
              <div className="space-y-1.5">
                {sinBranchBoard.map((ws) => (
                  <button
                    key={ws.repo}
                    type="button"
                    className="flex w-full items-center gap-3 rounded-lg border border-border bg-surface-2 px-3 py-2 text-left transition hover:border-primary"
                    onClick={() => {
                      cerrarNuevo();
                      // Va al acceso, donde se configura BranchBoard en ese proyecto.
                      cambiarDeProyecto();
                    }}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium text-text">{ws.nombre}</span>
                      <span className="block truncate font-mono text-[10px] text-faint">{ws.ruta}</span>
                    </span>
                    <span className="shrink-0 text-[11px] text-primary">Traer</span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="border-t border-border pt-4">
            <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">
              Crear un proyecto nuevo
            </div>
            <p className="mb-2 text-[12px] text-muted">
              Para un proyecto que todavía no está en Unity VC: se crea su repositorio y su workspace, y quedas como
              Administrador.
            </p>
            <div className="space-y-2">
              <input
                className="input"
                placeholder="Nombre del proyecto"
                value={nombreNuevo}
                onChange={(e) => setNombreNuevo(e.target.value)}
              />
              <SelectorCarpeta value={carpetaNueva} onChange={setCarpetaNueva} />
              <button
                type="button"
                className="btn-primary btn-sm w-full"
                disabled={!nombreNuevo.trim() || !carpetaNueva.trim() || workspaces.some((w) => w.nombre === nombreNuevo.trim())}
                onClick={() => {
                  const nombre = nombreNuevo.trim();
                  setWorkspaces([
                    ...workspaces,
                    {
                      nombre,
                      repo: `${nombre}@MIGUELMEZA`,
                      ruta: `${carpetaNueva.trim().replace(/\\$/, '')}\\${nombre}`,
                      tiene_db_branchboard: true,
                    },
                  ]);
                  setRolActual('Administrador');
                  entrarAProyecto(nombre);
                  showToast(`Proyecto ${nombre} creado — eres su Administrador`);
                  cerrarNuevo();
                }}
              >
                Crear proyecto
              </button>
            </div>
          </div>
        </div>
      </Modal>
      <Modal
        open={confirmarCerrarSesion}
        onClose={() => setConfirmarCerrarSesion(false)}
        title="¿Cerrar sesión?"
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={() => setConfirmarCerrarSesion(false)}>
              Cancelar
            </button>
            <button
              type="button"
              className="btn-danger btn-sm"
              onClick={() => {
                setConfirmarCerrarSesion(false);
                cerrarSesion();
              }}
            >
              Cerrar sesión
            </button>
          </div>
        }
      >
        <p className="text-sm text-muted">
          BranchBoard usa tu sesión de Unity Version Control para saber quién eres. Al cerrarla, tendrás que volver a
          iniciar sesión en Unity VC para entrar.
        </p>
      </Modal>
      <Modal
        open={confirmarSalir}
        onClose={() => setConfirmarSalir(false)}
        title="¿Salir de BranchBoard?"
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={() => setConfirmarSalir(false)}>
              Cancelar
            </button>
            <button type="button" className="btn-danger btn-sm" onClick={salir}>
              Salir
            </button>
          </div>
        }
      >
        <p className="text-sm text-muted">Se cierra la app.</p>
      </Modal>
    </aside>
  );
}
