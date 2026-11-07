import type { Rol, Tarea, Usuario } from '../types';

// Central permission helpers — mirror of SEGURIDAD.md matrix.
// The real backend must re-check these; here they only drive what the UI shows.

export const esLiderOAdmin = (rol: Rol) => rol === 'Administrador' || rol === 'Líder';

export const puedeEliminarProyecto = (rol: Rol) => rol === 'Administrador';
export const puedeRestaurarBackup = (rol: Rol) => rol === 'Administrador';
// Roles a los que `actor` puede cambiar a `objetivo` (vacío = no puede cambiarlo). El Administrador
// cambia a cualquiera menos a otro Administrador (y no da el rol Administrador); el Líder solo mueve
// entre Desarrollador, Tester y Colaborador externo.
const ROLES_BASE: Rol[] = ['Desarrollador', 'Tester', 'Colaborador externo'];
export function rolesAsignablesA(actor: Rol, objetivo: Rol): Rol[] {
  if (actor === 'Administrador') return objetivo === 'Administrador' ? [] : ['Líder', ...ROLES_BASE];
  if (actor === 'Líder') return ROLES_BASE.includes(objetivo) ? ROLES_BASE : [];
  return [];
}
export const puedeAsignarRevision = (rol: Rol) => rol === 'Administrador';
export const puedeAutoasignarseRevision = (rol: Rol) => esLiderOAdmin(rol);
export const puedeAprobarPendientes = (rol: Rol) => esLiderOAdmin(rol);
export const puedeEliminarTareas = (rol: Rol) => esLiderOAdmin(rol);
export const puedeEliminarDefinitivo = (rol: Rol) => esLiderOAdmin(rol);
export const puedeAutorizarRevisionGrande = (rol: Rol) => esLiderOAdmin(rol);
export const puedeCrearTareaDirecta = (rol: Rol) => esLiderOAdmin(rol);
export const puedeMergearDirecto = (rol: Rol) => esLiderOAdmin(rol);
export const puedeCrearUsuarios = (rol: Rol) => esLiderOAdmin(rol);

// Ver todas las tareas en tableros principales (Scrum/Lista/Resumen/Avance).
// Desarrollador solo ve las suyas; Admin/Líder/Tester ven todo.
export const veTodasLasTareas = (rol: Rol) => rol !== 'Desarrollador';

export function tareaVisibleEnTableroPrincipal(tarea: Tarea, user: Usuario): boolean {
  if (veTodasLasTareas(user.rol)) return true;
  // Desarrollador: solo las que creó o le fueron asignadas
  return tarea.creado_por === user.email || tarea.asignado_a === user.email;
}

// ¿Puede eliminar a este usuario?
export function puedeEliminarUsuario(actor: Rol, objetivo: Rol): boolean {
  if (actor === 'Administrador') return true;
  if (actor === 'Líder') return objetivo === 'Desarrollador' || objetivo === 'Colaborador externo';
  return false;
}

export function puedeConfirmarUsuario(rol: Rol): boolean {
  return rol === 'Administrador';
}
