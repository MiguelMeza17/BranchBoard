// Domain model for BranchBoard — shapes derived from documentacion/BACKEND.md.
// This is a UI mockup: everything lives in memory, no persistence.

export type Rol = 'Administrador' | 'Líder' | 'Desarrollador' | 'Tester';

export const ROLES: Rol[] = ['Administrador', 'Líder', 'Desarrollador', 'Tester'];

export type TipoTarea =
  | 'Scripting'
  | 'Modelado 3D'
  | 'Modelado 2D'
  | 'Animación'
  | 'Imagen'
  | 'UI/Interfaz'
  | 'Audio'
  | 'Música'
  | 'Video'
  | 'Testing'
  | 'Corrección';

export const TIPOS_TAREA: TipoTarea[] = [
  'Scripting',
  'Modelado 3D',
  'Modelado 2D',
  'Animación',
  'Imagen',
  'UI/Interfaz',
  'Audio',
  'Música',
  'Video',
  'Testing',
  'Corrección',
];

export type EstadoTarea = 'por hacer' | 'en producción' | 'en revisión' | 'terminada';

export const ESTADOS: EstadoTarea[] = ['por hacer', 'en producción', 'en revisión', 'terminada'];

export interface Usuario {
  email: string;
  nombre: string;
  apellido: string;
  rol: Rol;
  fecha_alta: string;
  firma: string | null; // null = pendiente de confirmar
  firmado_en: string | null;
}

// Una línea de `cm workspace list`: workspace de Unity VC ya clonado en esta máquina.
export interface WorkspaceUnityVC {
  nombre: string;
  repo: string;
  ruta: string;
  tiene_db_branchboard: boolean; // false = proyecto que nunca se configuró con BranchBoard
}

export interface Sprint {
  id: string;
  nombre: string;
  descripcion: string;
  con_fecha: boolean;
  fecha_inicio: string;
  duracion_dias: number | null;
  fecha_fin: string | null;
  cerrado: boolean;
  fecha_cierre_real: string | null;
  creado_por: string;
}

export interface Tarea {
  id: string;
  titulo: string;
  descripcion: string;
  tipo: TipoTarea;
  tarea_padre_id: string | null;
  archivo_ruta: string | null;
  estado: EstadoTarea;
  pendiente_aprobacion: boolean;
  rama_asociada: string | null;
  // Solo relevante en subtareas (tarea_padre_id != null): si al tomarla se creó su propia
  // rama, o si el trabajo se hace con check-in directo sobre la rama de la tarea padre.
  modo_trabajo: 'rama' | 'checkin' | null;
  modificada: boolean; // badge "hay cambios nuevos"
  creado_por: string;
  asignado_a: string | null;
  lider_revision_asignado: string | null;
  sprint_id: string | null;
  con_fecha: boolean;
  fecha_limite: string | null;
  fecha_completada: string | null;
  eliminada: boolean;
  justificacion_eliminacion: string | null;
  fecha_eliminacion: string | null;
  motivo_rechazo: string | null;
  reporte_testing_origen_id: string | null;
  // Tarea ya terminada (mergeada a dev) que esta corrige o a la que da seguimiento. Es solo
  // una referencia, no jerarquía: la corrección es una tarea general nueva que sale de dev,
  // con su propio número (no T-101-n), porque la rama de la original ya está cerrada.
  tarea_origen_id: string | null;
  // Veces que volvió de "terminada" a "en revisión" (ver LIMITE_REAPERTURAS en useStore).
  reaperturas?: number;
  // UI-only: edición pendiente (sombra amarilla) — derivada de ediciones_pendientes
  edicion_pendiente: boolean;
}

// Un checkin sobre la rama de una tarea genera un commit — una rama acumula
// varios a lo largo del tiempo (ver documentacion/BACKEND.md, "Mover cambios").
export interface Commit {
  id: string;
  tarea_id: string;
  hash: string;
  mensaje: string;
  autor: string;
  fecha: string;
}

export interface EdicionPendiente {
  id: string;
  tarea_id: string;
  campos_nuevos: Record<string, string>;
  campos_anteriores: Record<string, string>;
  motivo: string | null;
  creado_por: string;
  fecha: string;
}

export type TipoNotificacion =
  | 'aprobacion_tarea'
  | 'tarea_rechazada'
  | 'tarea_asignada'
  | 'edicion_pendiente'
  | 'tarea_a_revision'
  | 'tarea_reabierta'
  | 'revision_asignada'
  | 'inactividad'
  | 'accion_bd'
  | 'mover_cambios'
  | 'editada_aprobada'
  | 'advertencia'
  | 'tarea_eliminada';

export interface Notificacion {
  id: string;
  tipo: TipoNotificacion;
  para: string; // email destinatario
  titulo: string;
  cuerpo: string;
  leida: boolean;
  fecha: string;
  tarea_id: string | null;
  accionable: boolean; // ofrece botón de acción directa
}

export interface SolicitudIngreso {
  id: string;
  nombre: string;
  apellido: string;
  correo: string;
  id_organizacion: string;
  estado: 'pendiente' | 'aceptada' | 'rechazada';
  fecha_solicitud: string;
  fecha_resolucion: string | null;
  registrada_por: string;
}

export interface ColaboradorExterno {
  email: string;
  nombre: string;
  agregado_por: string;
  fecha_alta: string;
}

export interface Backup {
  id: string;
  fecha: string;
  tamano: string;
}
