import type {
  Usuario,
  Tarea,
  Sprint,
  Notificacion,
  SolicitudIngreso,
  ColaboradorExterno,
  EdicionPendiente,
  Backup,
  Commit,
  WorkspaceUnityVC, CambioTraido } from '../types';

export const PROYECTO_NOMBRE = 'VirusTF';

// ---- Workspaces de Unity VC en esta máquina (simula `cm workspace list`) ----
export const workspaces: WorkspaceUnityVC[] = [
  {
    nombre: 'VirusTF',
    repo: 'VirusTF@MIGUELMEZA',
    ruta: 'D:\\Unity\\VirusTF',
    tiene_db_branchboard: true,
  },
  {
    nombre: 'Prototipo-Plataformas',
    repo: 'Prototipo-Plataformas@MIGUELMEZA',
    ruta: 'D:\\Unity\\Prototipo-Plataformas',
    tiene_db_branchboard: false,
  },
];

// Correo de prueba que `cm whoami` puede devolver sin estar en la tabla de usuarios.
export const CORREO_NO_REGISTRADO = 'ana.torres@estudiante.dev';

// ---- Usuarios (equipo semilla VirusTF + tester + un pendiente de confirmar) ----
export const usuarios: Usuario[] = [
  {
    email: 'miguellmezza123@gmail.com',
    nombre: 'Miguel',
    apellido: 'Meza',
    rol: 'Administrador',
    fecha_alta: '2026-06-01',
    firma: 'sig_admin_ok',
    firmado_en: '2026-06-01T10:00:00',
  },
  {
    email: 'arteagacristobal957@gmail.com',
    nombre: 'Cristóbal',
    apellido: 'Arteaga',
    rol: 'Líder',
    fecha_alta: '2026-06-02',
    firma: 'sig_lider_ok',
    firmado_en: '2026-06-02T09:00:00',
  },
  {
    email: 'johns.telagt@uqvirtual.edu.co',
    nombre: 'Sebastián',
    apellido: 'Johns',
    rol: 'Desarrollador',
    fecha_alta: '2026-06-03',
    firma: 'sig_dev_ok',
    firmado_en: '2026-06-03T09:00:00',
  },
  {
    email: 'laura.qa@virustf.dev',
    nombre: 'Laura',
    apellido: 'Quintero',
    rol: 'Tester',
    fecha_alta: '2026-07-10',
    firma: 'sig_tester_ok',
    firmado_en: '2026-07-10T09:00:00',
  },
  {
    email: 'nuevo.dev@virustf.dev',
    nombre: 'Diego',
    apellido: 'Rivas',
    rol: 'Desarrollador',
    fecha_alta: '2026-09-20',
    firma: null, // pendiente de confirmar por Administrador
    firmado_en: null,
  },
];

export const sprints: Sprint[] = [
  {
    id: 'sp1',
    nombre: 'Sprint 1 — Combate y enemigos',
    descripcion: 'Sistema de combate cuerpo a cuerpo del jugador, IA de persecución de los enemigos rastreadores y el HUD de vida/energía.',
    con_fecha: true,
    fecha_inicio: '2026-09-15',
    duracion_dias: 14,
    fecha_fin: '2026-09-29',
    cerrado: false,
    fecha_cierre_real: null,
    creado_por: 'arteagacristobal957@gmail.com',
  },
  {
    id: 'sp2',
    nombre: 'Sprint 2 — Correcciones de combate y enemigos',
    descripcion: 'Corregir los bugs de combate e IA reportados por Testing durante el Sprint 1, y pulir detalles menores antes de avanzar a nuevas mecánicas.',
    con_fecha: true,
    fecha_inicio: '2026-08-18',
    duracion_dias: 14,
    fecha_fin: '2026-08-31',
    cerrado: true,
    fecha_cierre_real: '2026-08-31',
    creado_por: 'arteagacristobal957@gmail.com',
  },
  {
    id: 'sp3',
    nombre: 'Backlog de arte (sin fecha)',
    descripcion: 'Tareas de arte 2D/3D y video sin sprint ni fecha asignada todavía.',
    con_fecha: false,
    fecha_inicio: '2026-08-20',
    duracion_dias: null,
    fecha_fin: null,
    cerrado: false,
    fecha_cierre_real: null,
    creado_por: 'miguellmezza123@gmail.com',
  },
];

const ADMIN = 'miguellmezza123@gmail.com';
const LIDER = 'arteagacristobal957@gmail.com';
const DEV = 'johns.telagt@uqvirtual.edu.co';
const TESTER = 'laura.qa@virustf.dev';

function t(p: Partial<Tarea> & Pick<Tarea, 'id' | 'titulo' | 'tipo' | 'estado' | 'creado_por'>): Tarea {
  return {
    detalles: '',
    tarea_padre_id: null,
    archivo_ruta: null,
    pendiente_aprobacion: false,
    rama_asociada: null,
    modificada: false,
    asignado_a: null,
    lider_revision_asignado: null,
    sprint_id: 'sp1',
    con_fecha: false,
    fecha_limite: null,
    fecha_completada: null,
    eliminada: false,
    justificacion_eliminacion: null,
    fecha_eliminacion: null,
    motivo_rechazo: null,
    reporte_testing_origen_id: null,
    tarea_origen_id: null,
    edicion_pendiente: false,
    modo_trabajo: null,
    ...p,
  };
}

export const tareas: Tarea[] = [
  // --- Tareas generales (Scripting) con jerarquía ---
  t({
    id: 'T-101',
    titulo: 'Sistema de combate del jugador',
    detalles: 'Ataque melee, combos y detección de golpes contra enemigos.',
    tipo: 'Scripting',
    estado: 'en producción',
    creado_por: LIDER,
    asignado_a: DEV,
    rama_asociada: 'task/T-101-combate-jugador',
    con_fecha: true,
    fecha_limite: '2026-09-28',
    modificada: true,
  }),
  t({
    id: 'T-101-1',
    titulo: 'Modelo 3D del arma principal',
    detalles: 'Espada de energía con material emisivo.',
    tipo: 'Modelado 3D',
    estado: 'en revisión',
    creado_por: DEV,
    tarea_padre_id: 'T-101',
    asignado_a: DEV,
    rama_asociada: 'task/T-101-1-arma',
    archivo_ruta: 'Assets/Models/energy_sword.fbx',
  }),
  t({
    id: 'T-101-1-1',
    titulo: 'Textura del filo del arma',
    detalles: 'Mapa de emisión para el filo.',
    tipo: 'Imagen',
    estado: 'por hacer',
    creado_por: DEV,
    tarea_padre_id: 'T-101-1',
  }),
  // Subtarea comodín "En espera" de T-101-1 (ver BACKEND.md, "Subtarea En espera"): representa el
  // próximo check-in que llegue a su rama; al llegar, se convierte en el check-in real (título del
  // commit, tipo decidido por el bot, en revisión) y aparece una nueva. Tipo "Sin definir" hasta entonces.
  t({
    id: 'T-101-1-2',
    titulo: 'En espera',
    detalles: 'Esperando el próximo check-in de T-101-1. Al llegar, se convierte en el check-in real.',
    tipo: 'Sin definir',
    estado: 'en producción',
    creado_por: LIDER,
    tarea_padre_id: 'T-101-1',
    modo_trabajo: 'checkin',
  }),
  t({
    id: 'T-101-2',
    titulo: 'SFX de impacto de golpe',
    detalles: 'Sonido corto para el impacto del combo.',
    tipo: 'Audio',
    estado: 'terminada',
    creado_por: LIDER,
    tarea_padre_id: 'T-101',
    asignado_a: LIDER,
    rama_asociada: 'task/T-101-2-sfx',
    fecha_completada: '2026-09-22',
    sprint_id: 'sp1',
  }),
  // Subtarea trabajada con check-in directo sobre la rama de su tarea grande (sin rama propia):
  // sus cambios son commits en task/T-101-combate-jugador. Asignada a Miguel para ver el panel
  // "Abrir en Unity".
  t({
    id: 'T-101-3',
    titulo: 'Partículas del golpe',
    detalles: 'Efecto de partículas al conectar un golpe del combo.',
    tipo: 'Animación',
    estado: 'en revisión',
    creado_por: LIDER,
    asignado_a: ADMIN,
    tarea_padre_id: 'T-101',
    modo_trabajo: 'checkin',
  }),
  // Más check-in de T-101 en revisión, hechos uno después del otro (el turno se libera al pasar a
  // revisión), para ver cómo se revisan juntos o por separado.
  t({
    id: 'T-101-5',
    titulo: 'Rastro del arma al girar',
    detalles: 'Trail visual del arma durante el giro del combo.',
    tipo: 'Animación',
    estado: 'en revisión',
    creado_por: LIDER,
    asignado_a: DEV,
    tarea_padre_id: 'T-101',
    modo_trabajo: 'checkin',
  }),
  t({
    id: 'T-101-6',
    titulo: 'Sonido de impacto crítico',
    detalles: 'Variante de sonido para los golpes críticos.',
    tipo: 'Audio',
    estado: 'en revisión',
    creado_por: LIDER,
    asignado_a: LIDER,
    tarea_padre_id: 'T-101',
    modo_trabajo: 'checkin',
  }),
  // Libre, para probar la regla de un solo check-in por tarea padre: T-101 ya tiene el de T-101-3.
  t({
    id: 'T-101-4',
    titulo: 'Sacudida de cámara al golpear',
    detalles: 'Pequeña sacudida de cámara al conectar el golpe final del combo.',
    tipo: 'Scripting',
    estado: 'por hacer',
    creado_por: LIDER,
    tarea_padre_id: 'T-101',
  }),

  // --- IA de enemigos ---
  t({
    id: 'T-102',
    titulo: 'IA de enemigo rastreador',
    detalles: 'Máquina de estados: patrulla, persecución y ataque.',
    tipo: 'Scripting',
    estado: 'por hacer',
    creado_por: LIDER,
    con_fecha: true,
    fecha_limite: '2026-09-29',
  }),
  t({
    id: 'T-102-1',
    titulo: 'Animación de caminar del enemigo',
    detalles: 'Loop de caminata sobre el rig del rastreador.',
    tipo: 'Animación',
    estado: 'en producción',
    creado_por: DEV,
    tarea_padre_id: 'T-102',
    asignado_a: ADMIN, // asignada a Miguel para ver el panel "Abrir en Unity"
    rama_asociada: 'task/T-102-1-anim-walk',
  }),

  // --- UI / HUD ---
  t({
    id: 'T-103',
    titulo: 'HUD de vida y energía',
    detalles: 'Barra de vida, barra de energía y contador de combos.',
    tipo: 'UI/Interfaz',
    estado: 'en revisión',
    creado_por: LIDER,
    asignado_a: DEV,
    rama_asociada: 'task/T-103-hud',
  }),
  t({
    id: 'T-104',
    titulo: 'Música del nivel 1 (loop extendido)',
    detalles: 'Tema principal en loop para el primer nivel, versión extendida a 2 minutos.',
    tipo: 'Música',
    estado: 'terminada',
    creado_por: ADMIN,
    asignado_a: ADMIN,
    rama_asociada: 'task/T-104-musica-nivel1',
    fecha_completada: '2026-09-20',
    sprint_id: 'sp1',
    edicion_pendiente: true,
  }),

  // --- Tarea pendiente de aprobación (creada por Desarrollador) ---
  t({
    id: 'T-105',
    titulo: 'Sistema de partículas de daño',
    detalles: 'Efecto de chispas al recibir daño.',
    tipo: 'Scripting',
    estado: 'por hacer',
    creado_por: DEV,
    pendiente_aprobacion: true,
    con_fecha: false,
  }),
  // --- Otra pendiente creada por Tester ---
  t({
    id: 'T-106',
    titulo: 'Pantalla de game over',
    detalles: 'Mockup de la pantalla de derrota.',
    tipo: 'UI/Interfaz',
    estado: 'por hacer',
    creado_por: TESTER,
    pendiente_aprobacion: true,
  }),

  // --- Tarea con edición pendiente (sombra amarilla) ---
  t({
    id: 'T-107',
    titulo: 'Salto doble del jugador',
    detalles: 'Permite un segundo salto en el aire con reducción de gravedad.',
    tipo: 'Scripting',
    estado: 'en producción',
    creado_por: DEV,
    asignado_a: ADMIN, // asignada a Miguel para ver el panel "Abrir en Unity"
    rama_asociada: 'task/T-107-doble-salto',
    edicion_pendiente: true,
  }),

  // --- Tarea de corrección nacida de un reporte de Testing ---
  t({
    id: 'T-108',
    titulo: '[Corrección] Colisión atraviesa el suelo al caer rápido',
    detalles: 'Generada desde reporte de Testing RT-01.',
    tipo: 'Scripting',
    estado: 'por hacer',
    creado_por: TESTER,
    reporte_testing_origen_id: 'RT-01',
    sprint_id: 'sp1',
  }),

  // --- Subtarea simple de colaborador externo, en revisión (tarea pequeña) ---
  t({
    id: 'T-109',
    titulo: 'Icono de poción de vida',
    detalles: 'Sprite 64x64 para el inventario.',
    tipo: 'Imagen',
    estado: 'en revisión',
    creado_por: 'colab.arte@freelance.dev',
    tarea_padre_id: 'T-103',
    asignado_a: 'colab.arte@freelance.dev',
    rama_asociada: 'task/T-103-1-icono-pocion',
    archivo_ruta: 'Assets/UI/potion_icon.png',
  }),

  // --- Video ---
  t({
    id: 'T-110',
    titulo: 'Cinemática de introducción',
    detalles: 'Video pre-renderizado de apertura.',
    tipo: 'Video',
    estado: 'por hacer',
    creado_por: LIDER,
    sprint_id: 'sp3',
    // Eliminada de ejemplo, para ver en la Papelera que el motivo es distinto de los detalles.
    eliminada: true,
    fecha_eliminacion: '2026-10-31',
    justificacion_eliminacion: 'Se decidió reemplazar la cinemática pre-renderizada por una escena en tiempo real dentro del motor; esta tarea ya no aplica.',
    con_fecha: false,
  }),
  // --- Modelado 2D backlog ---
  t({
    id: 'T-111',
    titulo: 'Concept art del jefe final',
    detalles: 'Bocetos y arte 2D del boss.',
    tipo: 'Modelado 2D',
    estado: 'por hacer',
    creado_por: ADMIN,
    sprint_id: 'sp3',
    con_fecha: false,
  }),

  // --- Sprint 2 — Correcciones de combate y enemigos (cerrado, ligado al Sprint 1) ---
  t({
    id: 'T-211',
    titulo: '[Corrección] Hitbox del combo se desalinea al girar',
    detalles: 'El collider de golpe no rota junto con el jugador durante el combo.',
    tipo: 'Scripting',
    estado: 'terminada',
    creado_por: TESTER,
    asignado_a: DEV,
    // Sale de dev (la rama de la original ya está mergeada); solo referencia.
    tarea_origen_id: 'T-101',
    rama_asociada: 'task/T-211-fix-hitbox-combo',
    fecha_completada: '2026-08-20',
    sprint_id: 'sp2',
  }),
  t({
    id: 'T-212',
    titulo: '[Corrección] IA se atasca contra paredes al perseguir',
    detalles: 'El enemigo rastreador no recalcula ruta al chocar con obstáculos.',
    tipo: 'Scripting',
    estado: 'terminada',
    creado_por: TESTER,
    asignado_a: DEV,
    // Sale de dev (la rama de la original ya está mergeada); solo referencia.
    tarea_origen_id: 'T-102',
    rama_asociada: 'task/T-212-fix-ia-pathing',
    fecha_completada: '2026-08-23',
    sprint_id: 'sp2',
  }),
  t({
    id: 'T-213',
    titulo: 'Balanceo de daño del combo final',
    detalles: 'Ajuste de daño del último golpe del combo tras pruebas de Testing.',
    tipo: 'Scripting',
    estado: 'terminada',
    creado_por: LIDER,
    asignado_a: DEV,
    // Sale de dev (la rama de la original ya está mergeada); solo referencia.
    tarea_origen_id: 'T-101',
    rama_asociada: 'task/T-213-balance-combo',
    fecha_completada: '2026-08-26',
    sprint_id: 'sp2',
  }),
  t({
    id: 'T-214',
    titulo: '[Corrección] Salto doble no cancela el dash',
    detalles: 'El segundo salto no interrumpe el dash en curso, se acumulan velocidades.',
    tipo: 'Scripting',
    estado: 'terminada',
    creado_por: TESTER,
    asignado_a: DEV,
    // Sale de dev (la rama de la original ya está mergeada); solo referencia.
    tarea_origen_id: 'T-107',
    rama_asociada: 'task/T-214-fix-salto-dash',
    fecha_completada: '2026-08-29',
    sprint_id: 'sp2',
  }),
  t({
    id: 'T-215',
    titulo: 'Pulido de animación de muerte del enemigo',
    detalles: 'Transición más natural entre el golpe final y la caída del enemigo.',
    tipo: 'Animación',
    estado: 'en producción',
    creado_por: LIDER,
    asignado_a: ADMIN, // asignada a Miguel para ver el panel "Abrir en Unity"
    // Sale de dev (la rama de la original ya está mergeada); solo referencia.
    tarea_origen_id: 'T-102',
    rama_asociada: 'task/T-215-anim-muerte-enemigo',
    sprint_id: 'sp2',
  }),
  t({
    id: 'T-216',
    titulo: '[Corrección] Sonido de impacto se corta en combos rápidos',
    detalles: 'El SFX de golpe se interrumpe si los golpes del combo van muy seguidos.',
    tipo: 'Audio',
    estado: 'por hacer',
    creado_por: TESTER,
    // Sale de dev (la rama de la original ya está mergeada); solo referencia.
    tarea_origen_id: 'T-101',
    sprint_id: 'sp2',
  }),

  // --- Tareas eliminadas (papelera) ---
  t({
    id: 'T-090',
    titulo: 'Sistema de crafteo',
    detalles: 'Descartado por cambio de alcance.',
    tipo: 'Scripting',
    estado: 'por hacer',
    creado_por: DEV,
    eliminada: true,
    justificacion_eliminacion: 'Fuera del alcance del MVP, se pospone a una versión futura.',
    fecha_eliminacion: '2026-09-18',
  }),
  t({
    id: 'T-091',
    titulo: 'Minimapa duplicado',
    detalles: 'Rechazada desde Pendientes por estar duplicada con T-103.',
    tipo: 'UI/Interfaz',
    estado: 'por hacer',
    creado_por: TESTER,
    eliminada: true,
    justificacion_eliminacion: 'Duplicada con el HUD (T-103).',
    fecha_eliminacion: '2026-09-21',
    motivo_rechazo: 'Ya cubierta por T-103',
  }),
];

// Commits (checkins) acumulados por rama — una rama puede tener varios.
// Cambios sin check-in en el workspace de cada rama (lo que Unity VC llama "pending changes"). En la
// app real los detecta la escucha de Unity VC; aquí se simulan. T-107 tiene 3 para ver el aviso de
// "Traer cambios" en producción.
export const cambiosSinCheckin: Record<string, number> = {
  'task/T-107-doble-salto': 3,
};

// "Traer cambios" ya hechos. T-103 (en revisión) tomó prestados cambios de T-107 y después modificó
// uno de esos archivos: sirve para ver la alerta "se tocaron cambios ajenos". En la app real la
// detección es automática (cruce de los check-ins de la rama con los archivos prestados).
export const cambiosTraidos: CambioTraido[] = [
  {
    id: 'CT-1',
    tarea_id: 'T-103',
    origen_id: 'T-107',
    hash: null,
    autor: DEV,
    fecha: '2026-10-30T15:20:00',
    archivos: ['Assets/Scripts/Player/PlayerJump.cs', 'Assets/Prefabs/Player.prefab'],
    archivos_tocados: ['Assets/Scripts/Player/PlayerJump.cs'],
    quitado_en: null,
    quitado_por: null,
  },
];

export const commits: Commit[] = [
  { id: 'C-1', tarea_id: 'T-101', hash: 'a3f91c2', mensaje: 'Detección de golpes contra hitbox de enemigos', autor: DEV, fecha: '2026-09-16T11:20:00' },
  { id: 'C-2', tarea_id: 'T-101', hash: 'b02de7a', mensaje: 'Combo de 3 golpes con ventana de cancelación', autor: DEV, fecha: '2026-09-19T15:05:00' },
  { id: 'C-3', tarea_id: 'T-101', hash: 'c7118f4', mensaje: 'Ajuste de daño base del combo final', autor: DEV, fecha: '2026-09-23T09:40:00' },

  { id: 'C-4', tarea_id: 'T-101-1', hash: 'd44a0b1', mensaje: 'Malla base de la espada de energía', autor: DEV, fecha: '2026-09-17T10:00:00' },
  { id: 'C-5', tarea_id: 'T-101-1', hash: 'e91c3f2', mensaje: 'Material emisivo del filo', autor: DEV, fecha: '2026-09-18T14:30:00' },

  { id: 'C-6', tarea_id: 'T-101-2', hash: 'f12a8d9', mensaje: 'SFX de impacto importado y asignado al combo', autor: LIDER, fecha: '2026-09-21T16:15:00' },
  // Check-in de T-101-3 directo sobre la rama de T-101 (no tiene rama propia).
  { id: 'C-19', tarea_id: 'T-101-3', hash: 'a3e9b27', mensaje: 'Partículas al conectar el golpe final del combo', autor: ADMIN, fecha: '2026-09-26T15:40:00' },
  { id: 'C-20', tarea_id: 'T-101-5', hash: 'b7d4e10', mensaje: 'Trail del arma con fade al terminar el giro', autor: DEV, fecha: '2026-09-27T11:05:00' },
  { id: 'C-21', tarea_id: 'T-101-5', hash: 'c2f8a93', mensaje: 'Ajuste de ancho del trail', autor: DEV, fecha: '2026-09-27T16:20:00' },
  { id: 'C-22', tarea_id: 'T-101-6', hash: 'd5a1c64', mensaje: 'Sonido de crítico importado y asignado', autor: LIDER, fecha: '2026-09-28T10:30:00' },

  { id: 'C-7', tarea_id: 'T-102-1', hash: '0a7b3e6', mensaje: 'Blend tree de caminata del enemigo', autor: DEV, fecha: '2026-09-20T12:00:00' },

  { id: 'C-8', tarea_id: 'T-103', hash: '18c4f20', mensaje: 'Barras de vida y energía con binding al estado del jugador', autor: DEV, fecha: '2026-09-18T09:30:00' },
  { id: 'C-9', tarea_id: 'T-103', hash: '2d9a1e7', mensaje: 'Animación de daño en la barra de vida', autor: DEV, fecha: '2026-09-24T11:10:00' },

  { id: 'C-10', tarea_id: 'T-104', hash: '39b6c85', mensaje: 'Loop extendido mezclado y normalizado', autor: LIDER, fecha: '2026-09-19T17:45:00' },

  { id: 'C-11', tarea_id: 'T-107', hash: '4e1f0a3', mensaje: 'Segundo salto habilitado tras el primero', autor: DEV, fecha: '2026-09-22T10:20:00' },
  { id: 'C-12', tarea_id: 'T-107', hash: '5c8d2b6', mensaje: 'Altura del doble salto ajustada', autor: DEV, fecha: '2026-09-25T13:00:00' },

  { id: 'C-13', tarea_id: 'T-109', hash: '6f3a7c1', mensaje: 'Icono de poción de vida en 4 variantes', autor: DEV, fecha: '2026-09-23T08:50:00' },

  { id: 'C-14', tarea_id: 'T-211', hash: '7a9e4d8', mensaje: 'Fix: hitbox del combo alineada al girar', autor: DEV, fecha: '2026-09-27T15:30:00' },

  { id: 'C-15', tarea_id: 'T-212', hash: '8b0f5c2', mensaje: 'Fix: pathing recalcula al atascarse contra paredes', autor: DEV, fecha: '2026-09-27T16:10:00' },

  { id: 'C-16', tarea_id: 'T-213', hash: '9c1a6d3', mensaje: 'Balanceo de daño del combo final (-15%)', autor: DEV, fecha: '2026-09-26T14:00:00' },

  { id: 'C-17', tarea_id: 'T-214', hash: 'ad2b7e4', mensaje: 'Fix: dash cancela correctamente el doble salto', autor: DEV, fecha: '2026-09-28T09:15:00' },

  { id: 'C-18', tarea_id: 'T-215', hash: 'be3c8f5', mensaje: 'Pulido de curva de animación de muerte', autor: DEV, fecha: '2026-09-28T17:00:00' },
];

export const edicionesPendientes: EdicionPendiente[] = [
  {
    id: 'EP-1',
    tarea_id: 'T-107',
    campos_nuevos: {
      titulo: 'Salto doble del jugador',
      detalles: 'Permite un segundo salto en el aire con reducción de gravedad.',
      fecha_limite: '2026-09-30',
    },
    campos_anteriores: {
      titulo: 'Salto doble',
      detalles: 'Segundo salto en el aire.',
      fecha_limite: '(sin fecha)',
    },
    motivo: 'Aclaré el comportamiento de gravedad y agregué fecha límite.',
    creado_por: DEV,
    fecha: '2026-09-26',
  },
  {
    id: 'EP-2',
    tarea_id: 'T-104',
    campos_nuevos: {
      titulo: 'Música del nivel 1 (loop extendido)',
      detalles: 'Tema principal en loop para el primer nivel, versión extendida a 2 minutos.',
    },
    campos_anteriores: {
      titulo: 'Música del nivel 1',
      detalles: 'Tema principal en loop para el primer nivel.',
    },
    motivo: 'El loop original se sentía muy corto en pruebas de playtesting.',
    creado_por: ADMIN,
    fecha: '2026-09-27',
  },
];

export const solicitudes: SolicitudIngreso[] = [
  {
    id: 'S-1',
    nombre: 'Ana',
    apellido: 'López',
    correo: 'ana.lopez@estudiante.edu',
    id_organizacion: 'virustf-org',
    estado: 'pendiente',
    fecha_solicitud: '2026-09-24',
    fecha_resolucion: null,
    registrada_por: LIDER,
  },
  {
    id: 'S-2',
    nombre: 'Carlos',
    apellido: 'Mena',
    correo: 'carlos.mena@estudiante.edu',
    id_organizacion: 'virustf-org',
    estado: 'aceptada',
    fecha_solicitud: '2026-08-30',
    fecha_resolucion: '2026-09-01',
    registrada_por: ADMIN,
  },
];

export const colaboradores: ColaboradorExterno[] = [
  {
    email: 'colab.arte@freelance.dev',
    nombre: 'Marta Freelance',
    agregado_por: LIDER,
    fecha_alta: '2026-09-05',
  },
];

export const backups: Backup[] = [
  { id: 'B-1', fecha: '2026-09-27 03:00', tamano: '2.4 MB' },
  { id: 'B-2', fecha: '2026-09-26 03:00', tamano: '2.3 MB' },
  { id: 'B-3', fecha: '2026-09-25 03:00', tamano: '2.3 MB' },
];

export const notificaciones: Notificacion[] = [
  // Aprobación de tareas nuevas (BACKEND.md #1): va a TODO Líder/Administrador, no solo a uno.
  {
    id: 'N-1',
    tipo: 'aprobacion_tarea',
    para: LIDER,
    titulo: 'Tarea espera tu aprobación',
    cuerpo: 'Sebastián creó "Sistema de partículas de daño" (T-105) y espera tu aprobación.',
    leida: false,
    fecha: '2026-09-26',
    tarea_id: 'T-105',
    accionable: true,
  },
  {
    id: 'N-1b',
    tipo: 'aprobacion_tarea',
    para: ADMIN,
    titulo: 'Tarea espera tu aprobación',
    cuerpo: 'Sebastián creó "Sistema de partículas de daño" (T-105) y espera tu aprobación.',
    leida: false,
    fecha: '2026-09-26',
    tarea_id: 'T-105',
    accionable: true,
  },
  {
    id: 'N-6',
    tipo: 'aprobacion_tarea',
    para: LIDER,
    titulo: 'Tarea espera tu aprobación',
    cuerpo: 'Laura creó "Pantalla de game over" (T-106) y espera tu aprobación.',
    leida: false,
    fecha: '2026-09-27',
    tarea_id: 'T-106',
    accionable: true,
  },
  {
    id: 'N-6b',
    tipo: 'aprobacion_tarea',
    para: ADMIN,
    titulo: 'Tarea espera tu aprobación',
    cuerpo: 'Laura creó "Pantalla de game over" (T-106) y espera tu aprobación.',
    leida: false,
    fecha: '2026-09-27',
    tarea_id: 'T-106',
    accionable: true,
  },
  {
    id: 'N-2',
    tipo: 'edicion_pendiente',
    para: LIDER,
    titulo: 'Edición pendiente de revisión',
    cuerpo: 'Sebastián editó "Salto doble del jugador" (T-107). Revisa la edición pendiente.',
    leida: false,
    fecha: '2026-09-26',
    tarea_id: 'T-107',
    accionable: true,
  },
  {
    id: 'N-2b',
    tipo: 'edicion_pendiente',
    para: ADMIN,
    titulo: 'Edición pendiente de revisión',
    cuerpo: 'Sebastián editó "Salto doble del jugador" (T-107). Revisa la edición pendiente.',
    leida: false,
    fecha: '2026-09-26',
    tarea_id: 'T-107',
    accionable: true,
  },
  {
    id: 'N-3',
    tipo: 'tarea_a_revision',
    para: LIDER,
    titulo: 'Una tarea pasó a revisión',
    cuerpo: 'El "HUD de vida y energía" (T-103) pasó a revisión.',
    leida: true,
    fecha: '2026-09-25',
    tarea_id: 'T-103',
    accionable: false,
  },
  {
    id: 'N-3b',
    tipo: 'tarea_a_revision',
    para: ADMIN,
    titulo: 'Una tarea pasó a revisión',
    cuerpo: 'El "HUD de vida y energía" (T-103) pasó a revisión.',
    leida: true,
    fecha: '2026-09-25',
    tarea_id: 'T-103',
    accionable: false,
  },
  {
    id: 'N-4',
    tipo: 'tarea_asignada',
    para: DEV,
    titulo: 'Se te asignó una tarea',
    cuerpo: 'Se te asignó "Sistema de combate del jugador" (T-101).',
    leida: true,
    fecha: '2026-09-16',
    tarea_id: 'T-101',
    accionable: false,
  },
  {
    id: 'N-5',
    tipo: 'inactividad',
    para: DEV,
    titulo: 'Recordatorio de inactividad',
    cuerpo: 'Llevas 3 días sin actividad en "Salto doble del jugador" (T-107).',
    leida: false,
    fecha: '2026-09-27',
    tarea_id: 'T-107',
    accionable: false,
  },
];
