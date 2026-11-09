import type {
  Usuario,
  Tarea,
  Sprint,
  Notificacion,
  EdicionPendiente,
  Backup,
  Documento,
  Mensaje,
  GrupoChat,
  Commit,
  WorkspaceUnityVC, CambioTraido } from '../types';
import { CHAT_GRUPO } from '../types';

export const PROYECTO_NOMBRE = 'VirusTF';

// ---- Workspaces de Unity VC en esta máquina (simula `cm workspace list`) ----
export const workspaces: WorkspaceUnityVC[] = [
  {
    nombre: 'VirusTF',
    repo: 'VirusTF@MIGUELMEZA',
    ruta: 'D:\\Unity\\VirusTF',
    tiene_db_branchboard: true,
  },
  // Proyecto ficticio que ya usa BranchBoard: para probar "Cambiar de proyecto" (en el mockup se ven
  // los mismos datos de VirusTF; solo cambia el nombre del proyecto).
  {
    nombre: 'Neon-Runner',
    repo: 'Neon-Runner@MIGUELMEZA',
    ruta: 'D:\\Unity\\Neon-Runner',
    tiene_db_branchboard: true,
  },
  {
    nombre: 'Prototipo-Plataformas',
    repo: 'Prototipo-Plataformas@MIGUELMEZA',
    ruta: 'D:\\Unity\\Prototipo-Plataformas',
    tiene_db_branchboard: false,
  },
];

// ---- Proyectos (repositorios) del servidor de Unity VC a los que tienes acceso (simula `cm repo list`) ----
// Los que no tienen workspace en esta máquina salen como "sin clonar": al elegirlos se clonan.
export const proyectosServidor: { nombre: string; repo: string; tiene_db_branchboard: boolean }[] = [
  { nombre: 'VirusTF', repo: 'VirusTF@MIGUELMEZA', tiene_db_branchboard: true },
  { nombre: 'Neon-Runner', repo: 'Neon-Runner@MIGUELMEZA', tiene_db_branchboard: true },
  { nombre: 'Prototipo-Plataformas', repo: 'Prototipo-Plataformas@MIGUELMEZA', tiene_db_branchboard: false },
  { nombre: 'Arena-Battle', repo: 'Arena-Battle@MIGUELMEZA', tiene_db_branchboard: true },
  { nombre: 'GameJam-Octubre', repo: 'GameJam-Octubre@MIGUELMEZA', tiene_db_branchboard: false },
];

// Correo de prueba que `cm whoami` puede devolver sin estar en la tabla de usuarios.
export const CORREO_NO_REGISTRADO = 'ana.torres@estudiante.dev';

// ---- Usuarios (equipo semilla VirusTF + tester + un pendiente de confirmar) ----
export const usuarios: Usuario[] = [
  {
    email: 'miguel.meza@virustf.dev',
    nombre: 'Miguel',
    apellido: 'Meza',
    rol: 'Administrador',
    fecha_alta: '2026-06-01',
    firma: 'sig_admin_ok',
    firmado_en: '2026-06-01T10:00:00',
  },
  {
    email: 'joan@virustf.dev',
    nombre: 'Cristóbal',
    apellido: 'Arteaga',
    rol: 'Líder',
    fecha_alta: '2026-06-02',
    firma: 'sig_lider_ok',
    firmado_en: '2026-06-02T09:00:00',
  },
  {
    email: 'sebas@virustf.dev',
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
    firma: 'sig_dev2_ok',
    firmado_en: '2026-09-20T10:00:00',
  },
  {
    email: 'colab.arte@freelance.dev',
    nombre: 'Marta',
    apellido: 'Freelance',
    rol: 'Colaborador externo',
    fecha_alta: '2026-09-05',
    firma: 'sig_colab_ok',
    firmado_en: '2026-09-05T10:00:00',
  },
];

// Un sprint en curso de ejemplo (con tareas de prueba T-301…T-310, ver abajo), para ver Planificación,
// el selector del Scrum y el burndown de Reportes. El resto de sprints y backlogs los crea el equipo.
export const sprints: Sprint[] = [
  // Sprint ya cerrado: su reporte (copia de las tareas al cierre) se arma al final de este archivo.
  {
    id: 'sp0',
    nombre: 'Sprint 0',
    descripcion: 'Base jugable: movimiento, cámara y primer enemigo.',
    con_fecha: true,
    fecha_inicio: '2026-10-19',
    duracion_dias: 13,
    fecha_fin: '2026-11-01',
    cerrado: true,
    fecha_cierre_real: '2026-11-01',
    creado_por: 'miguel.meza@virustf.dev',
  },
  {
    id: 'sp1',
    nombre: 'Sprint 1',
    descripcion: '',
    con_fecha: true,
    fecha_inicio: '2026-11-02',
    duracion_dias: 14,
    fecha_fin: '2026-11-16',
    cerrado: false,
    fecha_cierre_real: null,
    creado_por: 'miguel.meza@virustf.dev',
  },
  // Backlogs de prueba (sin fecha): "Etapa 1", ya terminado (T-401…T-435), y "Etapa 2", en curso. "General" no es un backlog: en Avance
  // suma las tareas de todos los sprints y backlogs.
  {
    id: 'sp2',
    nombre: 'Sprint 2',
    descripcion: 'Bosque: primeras salas y enemigos.',
    con_fecha: true,
    fecha_inicio: '2026-11-16',
    duracion_dias: 14,
    fecha_fin: '2026-11-30',
    cerrado: false,
    fecha_cierre_real: null,
    creado_por: 'miguel.meza@virustf.dev',
  },
  {
    id: 'bl2',
    nombre: 'Etapa 1',
    descripcion: 'Primera etapa del juego.',
    con_fecha: false,
    fecha_inicio: '2026-10-06',
    duracion_dias: null,
    fecha_fin: null,
    cerrado: false,
    fecha_cierre_real: null,
    creado_por: 'miguel.meza@virustf.dev',
  },
  {
    id: 'bl3',
    nombre: 'Etapa 2',
    descripcion: 'Segunda etapa: el bosque.',
    con_fecha: false,
    fecha_inicio: '2026-11-03',
    duracion_dias: null,
    fecha_fin: null,
    cerrado: false,
    fecha_cierre_real: null,
    creado_por: 'joan@virustf.dev',
  },
];

const ADMIN = 'miguel.meza@virustf.dev';
const LIDER = 'joan@virustf.dev';
const DEV = 'sebas@virustf.dev';
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
    sprint_id: null,
    backlog_id: null,
    fecha_creacion: '2026-08-15',
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
  // "En espera" de T-101-1 (ver BACKEND.md): cambios sin check-in hechos directamente en la rama de
  // T-101-1, que tiene subtareas. Ahí no se hace check-in: al crear la rama (nombre, detalles y
  // tipo) esta "En espera" se convierte en una subtarea con su propia rama y los cambios pasan a ella.
  t({
    id: 'T-101-1-2',
    titulo: 'En espera',
    detalles: 'Cambios sin check-in hechos directamente en la rama de T-101-1. Crea su rama para guardarlos.',
    tipo: 'Sin definir',
    estado: 'en producción',
    creado_por: LIDER,
    tarea_padre_id: 'T-101-1',
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
  }),
  // Subtareas de T-101 en revisión, cada una en su propia rama (asignadas a distintas personas
  // para ver el panel "Abrir en Unity").
  t({
    id: 'T-101-3',
    titulo: 'Partículas del golpe',
    detalles: 'Efecto de partículas al conectar un golpe del combo.',
    tipo: 'Animación',
    estado: 'en revisión',
    creado_por: LIDER,
    asignado_a: ADMIN,
    tarea_padre_id: 'T-101',
    rama_asociada: 'task/T-101-3-particulas',
    modo_trabajo: 'rama',
  }),
  t({
    id: 'T-101-5',
    titulo: 'Rastro del arma al girar',
    detalles: 'Trail visual del arma durante el giro del combo.',
    tipo: 'Animación',
    estado: 'en revisión',
    creado_por: LIDER,
    asignado_a: DEV,
    tarea_padre_id: 'T-101',
    rama_asociada: 'task/T-101-5-rastro-arma',
    modo_trabajo: 'rama',
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
    rama_asociada: 'task/T-101-6-sfx-critico',
    modo_trabajo: 'rama',
  }),
  // Libre, para poder tomarla.
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
    // Eliminada de ejemplo, para ver en la Papelera que el motivo es distinto de los detalles.
    eliminada: true,
    fecha_eliminacion: '2026-10-31',
    justificacion_eliminacion: 'Se decidió reemplazar la cinemática pre-renderizada por una escena en tiempo real dentro del motor; esta tarea ya no aplica.',
    con_fecha: false,
  }),
  // --- Modelado 2D ---
  t({
    id: 'T-111',
    titulo: 'Concept art del jefe final',
    detalles: 'Bocetos y arte 2D del boss.',
    tipo: 'Modelado 2D',
    estado: 'por hacer',
    creado_por: ADMIN,
    con_fecha: false,
  }),

  // --- Correcciones de combate y enemigos (ya terminadas) ---
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
  }),

  // --- Sprint 1 en curso (2026-11-02 → 2026-11-16): tareas de prueba en varios estados ---
  t({ id: 'T-301', titulo: 'Menú de pausa', detalles: 'Pausar, reanudar, opciones y salir al menú.', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, asignado_a: DEV, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-301-menu-pausa', modo_trabajo: 'rama', fecha_completada: '2026-11-03' }),
  t({ id: 'T-302', titulo: 'Guardado automático por checkpoint', detalles: 'Guardar posición, vida y objetos al tocar un checkpoint.', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, asignado_a: DEV, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-302-checkpoint', modo_trabajo: 'rama', fecha_completada: '2026-11-04' }),
  t({ id: 'T-303', titulo: 'Sonido de pasos por superficie', detalles: 'Pasos distintos en piedra, pasto y metal.', tipo: 'Audio', estado: 'terminada', creado_por: ADMIN, asignado_a: LIDER, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-303-pasos', modo_trabajo: 'rama', fecha_completada: '2026-11-06' }),
  t({ id: 'T-304', titulo: 'Enemigo volador', detalles: 'Patrulla en el aire y se lanza en picada hacia el jugador.', tipo: 'Scripting', estado: 'en revisión', creado_por: LIDER, asignado_a: DEV, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-304-enemigo-volador', modo_trabajo: 'rama' }),
  t({ id: 'T-305', titulo: 'Modelo 3D del enemigo volador', tipo: 'Modelado 3D', estado: 'en producción', creado_por: LIDER, asignado_a: ADMIN, sprint_id: 'sp1', backlog_id: 'bl3', fecha_creacion: '2026-11-03', rama_asociada: 'task/T-305-modelo-volador', modo_trabajo: 'rama', con_fecha: true, fecha_limite: '2026-11-12' }),
  t({ id: 'T-306', titulo: 'Pantalla de game over', detalles: 'Reintentar desde el último checkpoint o volver al menú.', tipo: 'UI/Interfaz', estado: 'en producción', creado_por: ADMIN, asignado_a: DEV, sprint_id: 'sp1', backlog_id: 'bl3', fecha_creacion: '2026-11-03', rama_asociada: 'task/T-306-game-over', modo_trabajo: 'rama' }),
  t({ id: 'T-307', titulo: 'Animación de muerte del jugador', tipo: 'Animación', estado: 'por hacer', creado_por: LIDER, asignado_a: LIDER, sprint_id: 'sp1', backlog_id: 'bl3', fecha_creacion: '2026-11-04', rama_asociada: 'task/T-307-anim-muerte', modo_trabajo: 'rama' }),
  t({ id: 'T-308', titulo: 'Ícono de checkpoint en el HUD', tipo: 'Imagen', estado: 'por hacer', creado_por: LIDER, sprint_id: 'sp1', fecha_creacion: '2026-11-04' }),
  t({ id: 'T-310', titulo: 'Jefe del nivel 1', detalles: 'Patrón de ataques en dos fases, barra de vida propia y escena de entrada.', tipo: 'Scripting', estado: 'en producción', creado_por: LIDER, asignado_a: DEV, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-310-jefe-nivel1', modo_trabajo: 'rama' }),
  t({ id: 'T-310-1', titulo: 'Fase 1: ataques básicos', tipo: 'Scripting', estado: 'terminada', tarea_padre_id: 'T-310', creado_por: LIDER, asignado_a: DEV, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-310-1-fase1', modo_trabajo: 'rama', fecha_completada: '2026-11-05' }),
  t({ id: 'T-310-2', titulo: 'Fase 2: furia', tipo: 'Scripting', estado: 'en producción', tarea_padre_id: 'T-310', creado_por: LIDER, asignado_a: DEV, sprint_id: 'sp1', fecha_creacion: '2026-11-02', rama_asociada: 'task/T-310-2-fase2', modo_trabajo: 'rama' }),
  t({ id: 'T-310-3', titulo: 'Barra de vida del jefe', tipo: 'UI/Interfaz', estado: 'por hacer', tarea_padre_id: 'T-310', creado_por: LIDER, sprint_id: 'sp1', fecha_creacion: '2026-11-03' }),
  t({ id: 'T-309', titulo: 'Música de jefe (intro)', tipo: 'Música', estado: 'por hacer', creado_por: ADMIN, sprint_id: 'sp1', fecha_creacion: '2026-11-05', con_fecha: true, fecha_limite: '2026-11-15' }),

  // --- Backlog de prueba 'Etapa 2': T-305…T-307 están ocupadas por Sprint 1 (se ven en gris en el backlog),
  // T-501…T-503 están libres ---
  t({ id: 'T-311', titulo: 'Optimizar sombras en el bosque', tipo: 'Scripting', estado: 'por hacer', creado_por: LIDER, sprint_id: 'sp2', fecha_creacion: '2026-11-06' }),
  t({ id: 'T-312', titulo: 'Sonido de viento entre árboles', tipo: 'Audio', estado: 'por hacer', creado_por: ADMIN, sprint_id: 'sp2', fecha_creacion: '2026-11-06' }),
  t({ id: 'T-501', titulo: 'Nivel del bosque: diseño de salas', tipo: 'Modelado 3D', estado: 'por hacer', creado_por: LIDER, backlog_id: 'bl3', sprint_id: 'sp2', fecha_creacion: '2026-11-03' }),
  t({ id: 'T-502', titulo: 'Enemigo planta carnívora', tipo: 'Scripting', estado: 'por hacer', creado_por: LIDER, backlog_id: 'bl3', sprint_id: 'sp2', fecha_creacion: '2026-11-04' }),
  t({ id: 'T-503', titulo: 'Música del bosque', tipo: 'Música', estado: 'en producción', creado_por: ADMIN, asignado_a: LIDER, backlog_id: 'bl3', fecha_creacion: '2026-11-04', rama_asociada: 'task/T-503', modo_trabajo: 'rama' }),

  // --- Sprint 0 (cerrado el 1 nov): 8 terminadas; T-609 y T-610 no se terminaron y quedaron libres al cierre ---
  t({ id: 'T-601', titulo: 'Movimiento del jugador', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: DEV, rama_asociada: 'task/T-601', modo_trabajo: 'rama', fecha_completada: '2026-10-21' }),
  t({ id: 'T-602', titulo: 'Cámara en tercera persona', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: DEV, rama_asociada: 'task/T-602', modo_trabajo: 'rama', fecha_completada: '2026-10-22' }),
  t({ id: 'T-603', titulo: 'Modelo base del jugador', tipo: 'Modelado 3D', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: ADMIN, rama_asociada: 'task/T-603', modo_trabajo: 'rama', fecha_completada: '2026-10-23' }),
  t({ id: 'T-604', titulo: 'Animación de correr', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: LIDER, rama_asociada: 'task/T-604', modo_trabajo: 'rama', fecha_completada: '2026-10-25' }),
  t({ id: 'T-605', titulo: 'Enemigo básico: patrulla', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: DEV, rama_asociada: 'task/T-605', modo_trabajo: 'rama', fecha_completada: '2026-10-27' }),
  t({ id: 'T-606', titulo: 'Sonido de pasos', tipo: 'Audio', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: LIDER, rama_asociada: 'task/T-606', modo_trabajo: 'rama', fecha_completada: '2026-10-28' }),
  t({ id: 'T-607', titulo: 'Menú principal', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: ADMIN, rama_asociada: 'task/T-607', modo_trabajo: 'rama', fecha_completada: '2026-10-30' }),
  t({ id: 'T-608', titulo: 'Iluminación de la sala inicial', tipo: 'Modelado 3D', estado: 'terminada', creado_por: LIDER, sprint_id: 'sp0', fecha_creacion: '2026-10-19', asignado_a: ADMIN, rama_asociada: 'task/T-608', modo_trabajo: 'rama', fecha_completada: '2026-10-31' }),
  t({ id: 'T-609', titulo: 'Barra de vida del enemigo', tipo: 'UI/Interfaz', estado: 'en revisión', creado_por: LIDER, fecha_creacion: '2026-10-19', asignado_a: DEV, rama_asociada: 'task/T-609', modo_trabajo: 'rama' }),
  t({ id: 'T-610', titulo: 'Efecto al recibir daño', tipo: 'Animación', estado: 'por hacer', creado_por: LIDER, fecha_creacion: '2026-10-19' }),

  // --- Backlog de prueba 'Etapa 1' (ya terminado: sube y baja hasta 0) ---
  t({ id: 'T-401', titulo: 'Ajustar colisiones del nivel 1', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: DEV, rama_asociada: 'task/T-401', modo_trabajo: 'rama', fecha_completada: '2026-10-07' }),
  t({ id: 'T-402', titulo: 'Texturas del suelo de la cueva', tipo: 'Imagen', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: ADMIN, rama_asociada: 'task/T-402', modo_trabajo: 'rama', fecha_completada: '2026-10-08' }),
  t({ id: 'T-403', titulo: 'Sonido ambiente de la cueva', tipo: 'Audio', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: LIDER, rama_asociada: 'task/T-403', modo_trabajo: 'rama', fecha_completada: '2026-10-09' }),
  t({ id: 'T-404', titulo: 'Iluminación del nivel 2', tipo: 'Modelado 3D', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: DEV, rama_asociada: 'task/T-404', modo_trabajo: 'rama', fecha_completada: '2026-10-10' }),
  t({ id: 'T-405', titulo: 'Tutorial de controles', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: ADMIN, rama_asociada: 'task/T-405', modo_trabajo: 'rama', fecha_completada: '2026-10-11' }),
  t({ id: 'T-406', titulo: 'Optimizar carga de escenas', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: LIDER, rama_asociada: 'task/T-406', modo_trabajo: 'rama', fecha_completada: '2026-10-12' }),
  t({ id: 'T-407', titulo: 'Ícono de la app', tipo: 'Imagen', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: DEV, rama_asociada: 'task/T-407', modo_trabajo: 'rama', fecha_completada: '2026-10-13' }),
  t({ id: 'T-408', titulo: 'Vibración del mando al recibir daño', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: ADMIN, rama_asociada: 'task/T-408', modo_trabajo: 'rama', fecha_completada: '2026-10-15' }),
  t({ id: 'T-409', titulo: 'Créditos finales', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: LIDER, rama_asociada: 'task/T-409', modo_trabajo: 'rama', fecha_completada: '2026-10-16' }),
  t({ id: 'T-410', titulo: 'Rocas decorativas', tipo: 'Modelado 3D', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: DEV, rama_asociada: 'task/T-410', modo_trabajo: 'rama', fecha_completada: '2026-10-17' }),
  t({ id: 'T-411', titulo: 'Música del menú principal', tipo: 'Música', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: ADMIN, rama_asociada: 'task/T-411', modo_trabajo: 'rama', fecha_completada: '2026-10-18' }),
  t({ id: 'T-412', titulo: 'Animación de victoria', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: LIDER, rama_asociada: 'task/T-412', modo_trabajo: 'rama', fecha_completada: '2026-10-19' }),
  t({ id: 'T-413', titulo: 'Subtítulos', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: DEV, rama_asociada: 'task/T-413', modo_trabajo: 'rama', fecha_completada: '2026-10-20' }),
  t({ id: 'T-414', titulo: 'Partículas de polvo al caer', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: ADMIN, rama_asociada: 'task/T-414', modo_trabajo: 'rama', fecha_completada: '2026-10-22' }),
  t({ id: 'T-415', titulo: 'Logros del juego', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: LIDER, rama_asociada: 'task/T-415', modo_trabajo: 'rama', fecha_completada: '2026-10-23' }),
  t({ id: 'T-416', titulo: 'Sonido de cofre', tipo: 'Audio', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-06', asignado_a: DEV, rama_asociada: 'task/T-416', modo_trabajo: 'rama', fecha_completada: '2026-10-24' }),
  t({ id: 'T-417', titulo: 'Menú de opciones de audio', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-14', asignado_a: ADMIN, rama_asociada: 'task/T-417', modo_trabajo: 'rama', fecha_completada: '2026-10-25' }),
  t({ id: 'T-418', titulo: 'Antorchas animadas', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-14', asignado_a: LIDER, rama_asociada: 'task/T-418', modo_trabajo: 'rama', fecha_completada: '2026-10-25' }),
  t({ id: 'T-419', titulo: 'Puertas con llave', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-14', asignado_a: DEV, rama_asociada: 'task/T-419', modo_trabajo: 'rama', fecha_completada: '2026-10-26' }),
  t({ id: 'T-420', titulo: 'Barra de oxígeno bajo el agua', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-21', asignado_a: ADMIN, rama_asociada: 'task/T-420', modo_trabajo: 'rama', fecha_completada: '2026-10-28' }),
  t({ id: 'T-421', titulo: 'Diálogos del herrero', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-21', asignado_a: LIDER, rama_asociada: 'task/T-421', modo_trabajo: 'rama', fecha_completada: '2026-10-28' }),
  t({ id: 'T-422', titulo: 'Cámara al entrar a una sala', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-21', asignado_a: DEV, rama_asociada: 'task/T-422', modo_trabajo: 'rama', fecha_completada: '2026-10-29' }),
  t({ id: 'T-423', titulo: 'Brillo en objetos recogibles', tipo: 'Imagen', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-21', asignado_a: ADMIN, rama_asociada: 'task/T-423', modo_trabajo: 'rama', fecha_completada: '2026-10-30' }),
  t({ id: 'T-424', titulo: 'Sonido de pasos en agua', tipo: 'Audio', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-27', asignado_a: LIDER, rama_asociada: 'task/T-424', modo_trabajo: 'rama', fecha_completada: '2026-10-31' }),
  t({ id: 'T-425', titulo: 'Hojas que caen en el bosque', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-27', asignado_a: DEV, rama_asociada: 'task/T-425', modo_trabajo: 'rama', fecha_completada: '2026-11-02' }),
  t({ id: 'T-426', titulo: 'Cinemática de inicio', tipo: 'Video', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-10-27', asignado_a: ADMIN, rama_asociada: 'task/T-426', modo_trabajo: 'rama', fecha_completada: '2026-11-02' }),
  t({ id: 'T-427', titulo: 'Pantalla de carga con consejos', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-01', asignado_a: LIDER, rama_asociada: 'task/T-427', modo_trabajo: 'rama', fecha_completada: '2026-11-03' }),
  t({ id: 'T-428', titulo: 'Arreglar parpadeo de sombras', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-01', asignado_a: DEV, rama_asociada: 'task/T-428', modo_trabajo: 'rama', fecha_completada: '2026-11-04' }),
  t({ id: 'T-429', titulo: 'Mapa del mundo', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-01', asignado_a: ADMIN, rama_asociada: 'task/T-429', modo_trabajo: 'rama', fecha_completada: '2026-11-05' }),
  t({ id: 'T-430', titulo: 'Efecto de golpe crítico', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-01', asignado_a: LIDER, rama_asociada: 'task/T-430', modo_trabajo: 'rama', fecha_completada: '2026-11-06' }),
  t({ id: 'T-431', titulo: 'Ajustar volumen de efectos', tipo: 'Audio', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-01', asignado_a: DEV, rama_asociada: 'task/T-431', modo_trabajo: 'rama', fecha_completada: '2026-11-03' }),
  t({ id: 'T-432', titulo: 'Reflejos en el agua', tipo: 'Imagen', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-01', asignado_a: ADMIN, rama_asociada: 'task/T-432', modo_trabajo: 'rama', fecha_completada: '2026-11-04' }),
  t({ id: 'T-433', titulo: 'Guardar configuración de controles', tipo: 'Scripting', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-02', asignado_a: DEV, rama_asociada: 'task/T-433', modo_trabajo: 'rama', fecha_completada: '2026-11-05' }),
  t({ id: 'T-434', titulo: 'Arreglar texto cortado en el menú', tipo: 'UI/Interfaz', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-02', asignado_a: LIDER, rama_asociada: 'task/T-434', modo_trabajo: 'rama', fecha_completada: '2026-11-06' }),
  t({ id: 'T-435', titulo: 'Animación de recoger objeto', tipo: 'Animación', estado: 'terminada', creado_por: LIDER, backlog_id: 'bl2', fecha_creacion: '2026-11-03', asignado_a: ADMIN, rama_asociada: 'task/T-435', modo_trabajo: 'rama', fecha_completada: '2026-11-06' }),

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
  // Lo que muestra la subtarea "En espera" de T-101-1: trabajo en curso que aún no tiene check-in.
  'task/T-101-1-arma': 2,
};

// Subtareas con rama propia ya terminadas (mergeadas a la rama de su padre) que el workspace todavía
// no tiene: salen como "Cambios entrantes" en los detalles del padre. Las demás terminadas se dan
// por recibidas.
export const entrantesSinRecibir: string[] = ['T-101-2'];

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

// Chat: fechas relativas a ahora para que la demo siempre se vea reciente.
const haceMin = (m: number) => new Date(Date.now() - m * 60_000).toISOString();
export const gruposChat: GrupoChat[] = [
  { id: 'GR-arte', nombre: 'Arte y sonido', miembros: [ADMIN, DEV, 'colab.arte@freelance.dev'], creado_por: ADMIN, fecha: '2026-10-20' },
];

export const mensajes: Mensaje[] = [
  // Grupo del proyecto
  { id: 'G-1', de: ADMIN, para: CHAT_GRUPO, texto: '¡Buenas, equipo! Esta semana cerramos el nivel 2 🎮', fecha: haceMin(60 * 27), leidoPor: [ADMIN, LIDER, DEV, TESTER] },
  { id: 'G-2', de: LIDER, para: CHAT_GRUPO, texto: 'Yo reviso las ramas de UI hoy en la tarde.', fecha: haceMin(60 * 26), leidoPor: [ADMIN, LIDER, DEV] },
  { id: 'G-3', de: TESTER, para: CHAT_GRUPO, texto: 'Encontré un bug con las colisiones de la puerta, ya lo reporté en Testing.', fecha: haceMin(95), leidoPor: [TESTER, LIDER] },
  { id: 'G-4', de: DEV, para: CHAT_GRUPO, texto: 'Lo miro yo, creo que es el collider del prefab.', fecha: haceMin(90), leidoPor: [DEV] },
  { id: 'G-5', de: 'colab.arte@freelance.dev', para: CHAT_GRUPO, texto: 'Subí el icono de la poción, queda en revisión 🧪', fecha: haceMin(20), leidoPor: ['colab.arte@freelance.dev'] },
  // Grupo "Arte y sonido"
  { id: 'A-1', de: 'colab.arte@freelance.dev', para: 'GR-arte', texto: '¿Los sprites van en 64x64 o 128x128?', fecha: haceMin(60 * 5), leidoPor: ['colab.arte@freelance.dev', ADMIN, DEV] },
  { id: 'A-2', de: DEV, para: 'GR-arte', texto: '64x64 para el inventario, 128 para el HUD.', fecha: haceMin(60 * 5 - 10), leidoPor: [DEV, 'colab.arte@freelance.dev'] },
  // Privados
  { id: 'M-1', de: LIDER, para: ADMIN, texto: '¿Revisaste la rama del menú de pausa?', fecha: haceMin(60 * 26), leidoPor: [LIDER, ADMIN] },
  { id: 'M-2', de: ADMIN, para: LIDER, texto: 'Sí, le dejé dos comentarios. Mañana la mergeo.', fecha: haceMin(60 * 25), leidoPor: [ADMIN, LIDER] },
  { id: 'M-3', de: LIDER, para: ADMIN, texto: 'Perfecto 👌', fecha: haceMin(60 * 25 - 3), leidoPor: [LIDER, ADMIN] },
  { id: 'M-4', de: DEV, para: ADMIN, texto: 'Subí las referencias de sonido, quedaron pendientes.', fecha: haceMin(42), leidoPor: [DEV] },
  { id: 'M-5', de: DEV, para: ADMIN, texto: '¿Les das el visto bueno cuando puedas?', fecha: haceMin(41), leidoPor: [DEV] },
  { id: 'M-6', de: ADMIN, para: TESTER, texto: 'Laura, ¿puedes probar el nivel 2 hoy?', fecha: haceMin(180), leidoPor: [ADMIN] },
];

export const documentos: Documento[] = [
  { id: 'D-1', nombre: 'GDD VirusTF.pdf', tamano: 2_480_000, subido_por: ADMIN, fecha: '2026-09-02', pendiente: false },
  { id: 'D-2', nombre: 'Guía de estilo de arte.pdf', tamano: 5_120_000, subido_por: LIDER, fecha: '2026-09-10', pendiente: false },
  { id: 'D-3', nombre: 'Cronograma.xlsx', tamano: 86_000, subido_por: ADMIN, fecha: '2026-09-15', pendiente: false },
  { id: 'D-4', nombre: 'Referencias de sonido.zip', tamano: 14_300_000, subido_por: DEV, fecha: '2026-10-28', pendiente: true },
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

// Reporte de Sprint 0: copia de sus tareas tal como estaban al cerrarse (T-609 en revisión, T-610 por hacer).
const sprint0 = sprints.find((sp) => sp.id === 'sp0');
if (sprint0) {
  sprint0.reporte = tareas
    .filter((t) => /^T-6(0[1-9]|10)$/.test(t.id))
    .map((t) => ({ ...t, sprint_id: 'sp0' }));
}
