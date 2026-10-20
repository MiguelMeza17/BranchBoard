import { useState } from 'react';
import { HashRouter, Navigate, Route, Routes } from 'react-router-dom';
import { useStore } from './store/useStore';
import { Layout } from './components/Layout';
import { SplashScreen } from './components/SplashScreen';
import { SesionUnityVCRequerida } from './components/SesionUnityVCRequerida';
import { ScrumBoard } from './pages/ScrumBoard';
import { Resumen } from './pages/Resumen';
import { ListaTareas } from './pages/ListaTareas';
import { Avance } from './pages/Avance';
import { Branches } from './pages/Branches';
import { Revision } from './pages/Revision';
import { Pendientes } from './pages/Pendientes';
import { Papelera } from './pages/Papelera';
import { Categorias } from './pages/Categorias';
import { CategoriaScrum } from './pages/CategoriaScrum';
import { Reportes } from './pages/Reportes';
import { Ajustes } from './pages/Ajustes';
import { PerfilUsuario } from './pages/PerfilUsuario';

export function App() {
  const [showSplash, setShowSplash] = useState(true);
  const cmSesionActiva = useStore((s) => s.cmSesionActiva);

  if (!showSplash && !cmSesionActiva) {
    return <SesionUnityVCRequerida />;
  }

  return (
    <>
      {showSplash && <SplashScreen onDone={() => setShowSplash(false)} />}
      <HashRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<Navigate to="/scrum" replace />} />
            <Route path="/scrum" element={<ScrumBoard />} />
            <Route path="/resumen" element={<Resumen />} />
            <Route path="/lista" element={<ListaTareas />} />
            <Route path="/avance" element={<Avance />} />
            <Route path="/branches" element={<Branches />} />
            <Route path="/revision" element={<Revision />} />
            <Route path="/pendientes" element={<Pendientes />} />
            <Route path="/papelera" element={<Papelera />} />
            <Route path="/categorias" element={<Categorias />} />
            <Route path="/categorias/:tipo" element={<CategoriaScrum />} />
            <Route path="/reportes" element={<Reportes />} />
            <Route path="/ajustes" element={<Ajustes />} />
            <Route path="/perfil/:email" element={<PerfilUsuario />} />
            <Route path="*" element={<Navigate to="/scrum" replace />} />
          </Route>
        </Routes>
      </HashRouter>
    </>
  );
}
