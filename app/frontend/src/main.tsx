import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
// Fuentes dentro de la app (sin pedirlas a internet: la app de escritorio no tiene permiso de red).
import '@fontsource/inter/400.css';
import '@fontsource/inter/500.css';
import '@fontsource/inter/600.css';
import '@fontsource/inter/700.css';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import './index.css';

// La ventana tiene el pellizco activado (zoomHotkeysEnabled en tauri.conf.json): sin eso WebView2 no
// le pasa el gesto del touchpad a la página y Branches no puede acercar ni alejar. En el resto de la
// app no se hace zoom de toda la ventana: ni pellizco/Ctrl+rueda ni Ctrl + / - / 0. Branches
// intercepta su propio pellizco antes (en captura) y lo aplica al diagrama.
window.addEventListener(
  'wheel',
  (e) => {
    if (e.ctrlKey) e.preventDefault();
  },
  { passive: false }
);
window.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && ['+', '-', '=', '0'].includes(e.key)) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// Comprobación del puente con el backend al arrancar en desarrollo.
if (import.meta.env.DEV) {
  void import('./lib/api').then(({ api, enTauri }) =>
    api.ping().then((r) => console.info(`[BranchBoard] backend (${enTauri() ? 'Tauri' : 'mockup'}):`, r))
  );
}
