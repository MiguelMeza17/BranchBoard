import { useEffect, useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import { PANTALLA_ANGOSTA, useMediaQuery } from '../lib/useMediaQuery';
import { Sidebar } from './Sidebar';
import { Topbar } from './Topbar';
import { Toast } from './Toast';

const CLAVE_SIDEBAR = 'branchboard.sidebarColapsado';

function leerColapsado(): boolean {
  try {
    return localStorage.getItem(CLAVE_SIDEBAR) === '1';
  } catch {
    return false;
  }
}

export function Layout() {
  const [colapsado, setColapsado] = useState(leerColapsado);
  // En pantalla angosta (celular, tablet en vertical) el menú es un cajón que se abre con el botón
  // de la barra superior y se cierra al elegir una página o tocar fuera.
  const angosta = useMediaQuery(PANTALLA_ANGOSTA);
  const [menuAbierto, setMenuAbierto] = useState(false);
  const { pathname } = useLocation();
  useEffect(() => setMenuAbierto(false), [pathname, angosta]);

  const toggleSidebar = () => {
    setColapsado((c) => {
      try {
        localStorage.setItem(CLAVE_SIDEBAR, c ? '0' : '1');
      } catch {
        // sin almacenamiento: el estado solo dura la sesión
      }
      return !c;
    });
  };

  return (
    <div className="flex h-screen overflow-hidden bg-bg">
      {angosta && menuAbierto && (
        <div
          className="fixed inset-0 z-30 bg-black/60 animate-fade-in"
          onClick={() => setMenuAbierto(false)}
          aria-hidden="true"
        />
      )}
      <Sidebar
        colapsado={!angosta && colapsado}
        onToggle={toggleSidebar}
        cajon={angosta}
        abierto={menuAbierto}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <Topbar onMenu={angosta ? () => setMenuAbierto((v) => !v) : undefined} />
        <main className="flex-1 overflow-y-auto">
          {/* Con el menú contraído, el contenido gana el ancho que liberó el menú (240px). */}
          <div className={`mx-auto px-3 py-4 sm:px-6 sm:py-6 ${colapsado ? 'max-w-[1640px]' : 'max-w-[1400px]'}`}>
            <Outlet />
          </div>
        </main>
      </div>
      <Toast />
    </div>
  );
}
