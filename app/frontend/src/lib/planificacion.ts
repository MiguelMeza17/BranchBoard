import type { Sprint, Tarea } from '../types';

// Sprints y backlogs comparten la tabla `sprints` (con_fecha true = sprint, false = backlog), pero una
// tarea se relaciona distinto con cada uno: con un sprint por `sprint_id` (quién la trabaja ahora) y con
// un backlog por `backlog_id` (a qué grupo pertenece). Puede tener los dos, uno o ninguno.
export const perteneceA = (t: Tarea, sp: Sprint) => (sp.con_fecha ? t.sprint_id === sp.id : t.backlog_id === sp.id);

// ¿Está en algún sprint o backlog? (lo que suma "General" en Avance)
export const estaPlanificada = (t: Tarea) => t.sprint_id !== null || t.backlog_id !== null;
