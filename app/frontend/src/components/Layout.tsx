import { useState } from 'react';
import { Outlet } from 'react-router-dom';
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
      <Sidebar colapsado={colapsado} onToggle={toggleSidebar} />
      <div className="flex min-w-0 flex-1 flex-col">
        <Topbar />
        <main className="flex-1 overflow-y-auto">
          {/* Con el menú contraído, el contenido gana el ancho que liberó el menú (240px). */}
          <div className={`mx-auto px-6 py-6 ${colapsado ? 'max-w-[1640px]' : 'max-w-[1400px]'}`}>
            <Outlet />
          </div>
        </main>
      </div>
      <Toast />
    </div>
  );
}
