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
  | 'Corrección'
  | 'Ajustes'
  // Solo para la subtarea "En espera" (trabajo sin check-in en la rama de su padre); al llegar el
  // check-in, el bot decide su tipo. No está en
  // TIPOS_TAREA, así que no aparece en Categorías ni se puede elegir al crear una tarea.
  | 'Sin definir';

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
  'Ajustes',
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
  // Detalles: texto opcional — qué hay que hacer, los pasos, dónde está cada cosa. Vacío si no hay
  // (la interfaz muestra "Sin detalles."). Reemplaza a la antigua "descripción".
  detalles: string;
  tipo: TipoTarea;
  tarea_padre_id: string | null;
  archivo_ruta: string | null;
  estado: EstadoTarea;
  pendiente_aprobacion: boolean;
  rama_asociada: string | null;
  // 'rama' cuando ya se tomó (se creó su rama). Ya no existen tareas de check-in directo: toda
  // tarea se trabaja en su propia rama y los check-ins son guardados dentro de ella.
  modo_trabajo: 'rama' | null;
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
  /** Título del check-in (primera línea del comentario en Unity VC). */
  mensaje: string;
  autor: string;
  fecha: string;
  /** Descripción del check-in (resto del comentario). Los check-ins antiguos del mockup no la tienen. */
  descripcion?: string;
  /** Tipo del check-in (Scripting, Modelado 3D…). Los check-ins antiguos del mockup no lo tienen. */
  tipo?: TipoTarea;
}

/**
 * Cambios que una tarea trajo de otra con "Traer cambios" (toda la tarea o un solo commit).
 * Son un préstamo temporal: se usan sin modificarlos y se quitan antes de mandar la tarea a
 * revisión, para que no lleguen al merge (evita conflictos). Queda el registro como historial.
 */
export interface CambioTraido {
  id: string;
  /** Tarea que recibió los cambios. */
  tarea_id: string;
  /** Tarea de donde salieron los cambios. */
  origen_id: string;
  /** Commit puntual; null = todos los cambios de la tarea origen. */
  hash: string | null;
  autor: string;
  fecha: string;
  /** Archivos que llegaron con el préstamo (cm diff del merge). */
  archivos: string[];
  /**
   * Archivos prestados que se modificaron después en esta rama: el backend lo detecta solo, cruzando
   * cada check-in (y los cambios sin check-in) contra `archivos`. Si hay alguno, "se tocaron cambios
   * ajenos" y esas modificaciones se eliminan (el archivo vuelve a como llegó en el préstamo).
   */
  archivos_tocados: string[];
  /** Cuándo se quitaron de la rama; null = todavía están en ella. */
  quitado_en: string | null;
  quitado_por: string | null;
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
