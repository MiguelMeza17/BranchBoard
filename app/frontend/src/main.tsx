import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// Comprobación del puente con el backend (solo dentro de Tauri).
if (import.meta.env.DEV) {
  void import('./lib/tauri').then(({ ping }) =>
    ping().then((r) => r && console.info('[BranchBoard] backend:', r))
  );
}
