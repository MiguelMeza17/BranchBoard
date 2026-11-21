import React from 'react';
import { createRoot } from 'react-dom/client';
import '../../src/index.css';
import { App } from '../../src/App';
import { useStore } from '../../src/store/useStore';
import { invoke } from '@tauri-apps/api/core';
import type { Usuario } from '../../src/types';

const datos = await invoke<{ correo: string; proyecto: string; usuarios: Usuario[] }>('datos_prueba');
// Solo inicializa la navegación después de abrir el proyecto real en el backend.
// La lista, el store, las suscripciones y la App son los de producción.
useStore.setState({ usuarios: datos.usuarios, currentEmail: datos.correo,
  cuentaBranchBoard: { nombre: 'Cuenta B', apellido: 'Prueba', correo: datos.correo },
  accesoCompletado: true, proyectoActual: datos.proyecto, cmSesionActiva: false,
  modoSinUnityVc: true, workspaceActual: null, proyectoLocal: null,
  tareas: [], sprints: [], commits: [], notificaciones: [],
});
Object.assign(window, { __refresco239: { store: useStore, invoke } });
await useStore.getState().cargarTareas();
location.hash = '#/lista';
createRoot(document.getElementById('root')!).render(<React.StrictMode><App /></React.StrictMode>);
