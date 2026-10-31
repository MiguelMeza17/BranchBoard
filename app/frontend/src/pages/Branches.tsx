import { useEffect, useMemo, useState, type ReactNode } from 'react';
import ReactFlow, {
  Background,
  BaseEdge,
  Controls,
  Handle,
  Position,
  getBezierPath,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeProps,
  type ReactFlowInstance,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { useStore } from '../store/useStore';
import { estadoMeta } from '../lib/ui';
import { PageHeader } from '../components/primitives';
import { HistorialRamas } from '../components/HistorialRamas';
import type { Tarea } from '../types';

// Separación entre los tramos de la línea de tiempo de main.
const MAIN_X = 130;
// Línea de tiempo de dev: nace en un punto y cada merge de una tarea general es un
// recuadro gris sobre la línea.
const PUNTO_X = 110; // separación entre las ramas fusionadas dentro de dev
const PUNTO = 14; // diámetro del punto
const ALTO_RAMA = 44;

// Grosor de las conexiones según el nivel: entre más arriba en el árbol, más cambios
// trae y más gruesa es. main-main > main-dev > dev-tarea > tarea-subtarea > ...
const GROSOR_MAIN = 10;
const GROSOR_MAIN_DEV = 5;
// d = profundidad de la tarea (1 = cuelga de dev): 4px, y cada nivel más abajo la mitad
// (2, 1, 0.5, ...).
const grosorTarea = (d: number) => 4 / 2 ** (d - 1);

// REGLA DE ESCALA (no romper al agregar cosas; ver FRONTEND.md, "Branches"):
// - Flecha (siempre con triángulo) = la de destino es HIJA (un nivel más abajo en la jerarquía).
//   Sale siempre POR ADELANTE (costado derecho) del padre y entra por la izquierda de la hija.
//   main y dev son el tronco; cada rama queda centrada en vertical respecto a sus hijas.
// - Línea por el COSTADO, recta y SIN flecha = CONTINUIDAD: la misma rama en el tiempo
//   (tramos de la línea de main, de dev o del historial de una tarea).
// Nunca usar una línea lateral para unir padre e hija, ni una flecha de abajo para continuidad.
// Separación vertical mínima entre dos ramas (o paneles) que se cruzan en horizontal.
const GAP_VERTICAL = 16;

// Nodo de rama: solo muestra el nombre; el resto de la información aparece al pasar
// el mouse. Además de arriba/abajo tiene conexiones a los lados, para la línea de
// tiempo de main en horizontal. El primer handle de cada tipo es el que usan las
// aristas sin sourceHandle/targetHandle.
type RamaData = { nombre: string; info?: ReactNode };
const handleOculto = { opacity: 0 };

// Recuadro con la información que aparece al pasar el mouse (solo datos, sin textos
// explicativos: esos viven en documentacion/AGENTE.md). Sin info, no se muestra nada.
function InfoAlPasar({ info }: { info?: ReactNode }) {
  if (!info) return null;
  return (
    <div className="pointer-events-none absolute left-1/2 top-full z-50 mt-2 w-max max-w-[280px] -translate-x-1/2 whitespace-normal break-words rounded-md border border-border bg-surface-3 px-3 py-2 text-left font-sans text-xs font-normal text-text opacity-0 shadow-pop transition-opacity duration-150 group-hover/rama:opacity-100">
      {info}
    </div>
  );
}
function RamaNode({ data }: NodeProps<RamaData>) {
  return (
    <div className="group/rama relative flex h-full w-full items-center justify-center">
      <Handle type="target" position={Position.Top} id="arriba" style={handleOculto} />
      <Handle type="target" position={Position.Left} id="izquierda" style={handleOculto} />
      {/* El margen va en el texto, no en el nodo: así los handles quedan en el borde real del
          recuadro y el triángulo de la flecha no queda escondido debajo de él. */}
      <span className="truncate px-3">{data.nombre}</span>
      <InfoAlPasar info={data.info} />
      <Handle type="source" position={Position.Bottom} id="abajo" style={handleOculto} />
      <Handle type="source" position={Position.Right} id="derecha" style={handleOculto} />
      <Handle type="source" position={Position.Top} id="arriba-salida" style={handleOculto} />
    </div>
  );
}
// Punto de merge sobre la línea de dev: sin texto, la información sale al pasar el mouse.
type PuntoData = { info?: ReactNode };
function PuntoNode({ data }: NodeProps<PuntoData>) {
  return (
    <div className="group/rama relative h-full w-full">
      <Handle type="source" position={Position.Bottom} id="abajo" style={handleOculto} />
      <Handle type="source" position={Position.Right} id="derecha" style={handleOculto} />
      <Handle type="target" position={Position.Left} id="izquierda" style={handleOculto} />
      <Handle type="target" position={Position.Top} id="arriba" style={handleOculto} />
      <InfoAlPasar info={data.info} />
    </div>
  );
}
// Contenedor que encierra toda la línea de tiempo de una rama (main, dev o una tarea con
// historial), con su nombre
// sobre el borde superior. Va detrás de todo y no recibe el mouse.
type GrupoData = { nombre: string; color: string };
function GrupoNode({ data }: NodeProps<GrupoData>) {
  return (
    <div className="relative h-full w-full">
      <span
        // whitespace-nowrap: centrado con left-1/2, el navegador le daría solo la mitad del ancho
        // del panel y en un panel angosto el nombre se partiría en dos renglones (se ve cortado).
        className="absolute -top-2.5 left-1/2 -translate-x-1/2 whitespace-nowrap bg-surface px-2 font-mono text-xs font-bold leading-5"
        style={{ color: data.color }}
      >
        {data.nombre}
      </span>
    </div>
  );
}
const nodeTypes = { rama: RamaNode, punto: PuntoNode, grupo: GrupoNode };

// Conexión con flecha propia. Los marcadores de React Flow escalan el triángulo con el
// grosor de la línea (con 5px queda enorme) y la línea llega hasta la punta, así que se
// asoma por delante. Aquí la curva termina en la BASE del triángulo y el triángulo pone
// la punta; su tamaño acompaña al grosor, con un mínimo para las líneas delgadas.
const DIRECCION: Record<Position, [number, number]> = {
  [Position.Top]: [0, -1],
  [Position.Bottom]: [0, 1],
  [Position.Left]: [-1, 0],
  [Position.Right]: [1, 0],
};
function FlechaEdge({ id, sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition, style }: EdgeProps) {
  const grosor = Number(style?.strokeWidth ?? 2);
  const largo = Math.max(8, grosor * 2.2);
  const ancho = Math.max(8, grosor * 2.6);
  // Hacia afuera del nodo destino: la base del triángulo queda "largo" antes del borde.
  const [dx, dy] = DIRECCION[targetPosition];
  const baseX = targetX + dx * largo;
  const baseY = targetY + dy * largo;
  let path: string;
  if ((sourcePosition === Position.Bottom || sourcePosition === Position.Top) && targetPosition === Position.Left) {
    // Sale por abajo (o por arriba) y entra por la izquierda. La curva estándar, si la rama
    // está del otro lado, se va mucho hacia afuera y vuelve. Aquí sale un poco hacia su lado,
    // gira enseguida hacia adelante y entra derecha a la rama.
    const signo = sourcePosition === Position.Bottom ? 1 : -1;
    const dy = (baseY - sourceY) * signo;
    const salida = (dy > 0 ? dy * 0.6 : 10) * signo;
    const tiron = Math.max(30, (baseX - sourceX) * 0.6);
    path = `M ${sourceX},${sourceY} C ${sourceX},${sourceY + salida} ${baseX - tiron},${baseY} ${baseX},${baseY}`;
  } else if (sourcePosition === Position.Right && targetPosition === Position.Left) {
    // Sale por adelante y entra por la izquierda: va RECTA a la altura del padre (por su propia
    // fila, donde no hay otras ramas) y gira hacia la hija solo en el último tramo, que está
    // libre gracias a las columnas parejas por nivel.
    const giro = Math.max(sourceX, baseX - 60);
    path = `M ${sourceX},${sourceY} L ${giro},${sourceY} C ${giro + 30},${sourceY} ${giro + 30},${baseY} ${baseX},${baseY}`;
  } else {
    [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX: baseX, targetY: baseY, targetPosition });
  }
  const [px, py] = [-dy, dx];
  const puntos = [
    `${targetX},${targetY}`,
    `${baseX + (px * ancho) / 2},${baseY + (py * ancho) / 2}`,
    `${baseX - (px * ancho) / 2},${baseY - (py * ancho) / 2}`,
  ].join(' ');
  return (
    <>
      <BaseEdge id={id} path={path} style={{ ...style, strokeLinecap: 'butt' }} />
      <polygon points={puntos} fill={String(style?.stroke ?? '#64748b')} />
    </>
  );
}
const edgeTypes = { flecha: FlechaEdge };

// Ancho aproximado de un recuadro de rama según su nombre (fuente mono de 12px, más el
// margen y el borde), para calcular el contenedor de main sin esperar a medir el DOM.
const anchoRama = (nombre: string) => Math.ceil(nombre.length * 7.3) + 28;
const MARGEN_GRUPO = 16;
// Regla para cualquier panel: el punto donde una rama entra a su panel (ej. donde nace dev,
// al que llega la flecha desde main) queda SOBRE el borde izquierdo, metido SOLAPE_BORDE px.
// Así la flecha llega desde afuera sin cruzar el panel, y el punto se lee como la puerta
// de entrada de esa línea. Ver FRONTEND.md, sección "Branches".
const SOLAPE_BORDE = 2;
// Inicio del contenido del panel de dev: el primer recuadro queda a MARGEN_GRUPO del borde
// (igual que en el panel de main).
// Regla: toda flecha baja y AVANZA (hacia la derecha), nunca retrocede ni baja recta.
// Cada rama que sale de otra empieza ADELANTO px a la derecha de su origen.
const ADELANTO = 80;
// Punto donde nace dev: su centro ADELANTO px a la derecha del centro de main, para que la
// flecha main -> dev avance con una curva abierta (igual que las de las tareas).
// La cabeza de main es lo último que se fusionó en ella (igual que dev): main/dev.
const CABEZA_MAIN = 'main/dev';
const DEV_INICIO_X = 40 + anchoRama(CABEZA_MAIN) / 2 + ADELANTO - PUNTO / 2;
const DEV_CONTENIDO_X = DEV_INICIO_X + PUNTO - SOLAPE_BORDE + MARGEN_GRUPO;

// Línea de información del recuadro que aparece al pasar el mouse.
function InfoFila({ k, v }: { k: string; v: ReactNode }) {
  return (
    <div className="flex gap-2">
      <span className="text-faint">{k}</span>
      <span>{v}</span>
    </div>
  );
}

// Separación mínima entre el borde derecho de una rama (o su panel) y la siguiente de la fila.
const GAP_TAREAS = 50;

// Texto en minúsculas y sin tildes, para que el buscador no dependa de mayúsculas ni acentos.
const normalizar = (s: string) =>
  s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();

// Dónde se ve una tarea en el diagrama: el nodo que la representa y qué historiales deben
// estar abiertos para verlo. Una tarea abierta es su propio recuadro; una terminada es un
// recuadro gris dentro del historial de su padre (o de dev); y una terminada cuyo padre
// también está terminado se ve a través del recuadro de ese padre.
type Ubicacion = { nodo: string; abrir: string[] };

export function Branches() {
  const tareasAll = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  // Historiales cerrados: 'main', 'dev' o el número de una tarea con historial. Click en
  // su recuadro de color lo cierra/abre, para ahorrar espacio.
  const [cerrados, setCerrados] = useState<Set<string>>(new Set());
  // Viaje en el tiempo de dev: click en un recuadro gris de dev (una tarea fusionada) lleva la
  // cabeza de dev hasta ese merge: se oculta lo posterior (merges, dev actual y ramas que
  // nacieron después) y ese recuadro brilla. Otro click vuelve al presente.
  const [cabezaDev, setCabezaDev] = useState<string | null>(null);
  // Segundo click en la misma cabeza: "enfoque". Además de lo de adelante se cierran las ramas
  // de atrás (las que nacieron de recuadros anteriores) junto con sus hijas: dev queda solo con
  // la cabeza elegida y las ramas que nacieron de ella. Tercer click: vuelve al presente.
  const [enfoqueDev, setEnfoqueDev] = useState(false);
  // Lo mismo para cada tarea con historial: número de tarea -> subtarea elegida como cabeza.
  // (main no: solo tiene una instancia de dev.)
  const [cabezas, setCabezas] = useState<Record<string, string>>({});
  const alternarHistorial = (id: string) => {
    if (id === 'dev') {
      // al cerrar/abrir el historial de dev, al presente
      setCabezaDev(null);
      setEnfoqueDev(false);
    }
    setCabezas(({ [id]: _, ...resto }) => resto); // y lo mismo para una tarea
    setCerrados((prev) => {
      const sig = new Set(prev);
      if (sig.has(id)) sig.delete(id);
      else sig.add(id);
      return sig;
    });
  };

  const { nodes, edges, ubicaciones, buscables, conHistorial, duenoHistorial, cabezaId } = useMemo(() => {
    const tareas = tareasAll.filter((t) => !t.eliminada && t.rama_asociada);
    const porId = new Map(tareasAll.map((t) => [t.id, t]));
    const conRama = new Set(tareas.map((t) => t.id));
    const terminada = (t: Tarea) => t.estado === 'terminada';

    // Ancestro más cercano que sí está en el grafo: un padre sin rama (aún sin
    // tomar) o en la Papelera no se dibuja, así que la hija cuelga del siguiente.
    const padreEnGrafo = (t: Tarea): string | null => {
      let cur = t.tarea_padre_id;
      while (cur && !conRama.has(cur)) cur = porId.get(cur)?.tarea_padre_id ?? null;
      return cur;
    };
    // Ancestro abierto más cercano (de él cuelga una rama abierta); null = dev.
    const ancestroAbierto = (t: Tarea): string | null => {
      let cur = padreEnGrafo(t);
      while (cur && terminada(porId.get(cur)!)) cur = padreEnGrafo(porId.get(cur)!);
      return cur;
    };

    const nodes: Node[] = [];
    const edges: Edge[] = [];
    const ubicaciones = new Map<string, Ubicacion>();

    const estiloRamaBase = {
      background: '#161922',
      color: '#e7e9ee',
      borderRadius: 12,
      padding: 0, // el margen lo pone el texto (ver RamaNode)
      fontSize: 12,
      fontWeight: 700,
      fontFamily: 'JetBrains Mono, monospace',
      // Misma altura para todas (las conexiones laterales quedan rectas); el ancho se
      // ajusta al nombre, con un poco de margen.
      boxSizing: 'border-box' as const,
      height: ALTO_RAMA,
      width: 'auto',
      whiteSpace: 'nowrap' as const,
    };
    // Gris continuo = pasado. Fondo sólido (sin opacity) para que las líneas no se vean a
    // través del recuadro; el aspecto de "pasado" lo dan el borde y el texto.
    const estiloPasado = { ...estiloRamaBase, border: '2px solid #475063', color: '#6b7385' };
    const estiloPunto = (color: string) => ({
      width: PUNTO,
      height: PUNTO,
      borderRadius: '50%',
      background: color,
      border: '2px solid #161922',
    });

    // Contenedor alrededor de toda la línea de tiempo de una rama: todo lo de adentro es
    // esa misma rama en distintos momentos. Va detrás de todo (zIndex -1) y no recibe el mouse.
    // soloCabeza: historial cerrado por completo; el panel se pinta igual que la cabeza (borde
    // de su color), para que se lea como una sola pieza.
    const contenedor = (id: string, nombre: string, color: string, x: number, derecha: number, y: number, soloCabeza = false) => {
      // Ancho mínimo: que el nombre del panel quepa sobre el borde con aire a los lados
      // (probado hasta números de 4 cifras, ej. T-9999).
      const minimo = anchoRama(nombre) + 2 * MARGEN_GRUPO;
      const ancho = Math.max(derecha - x + 2 * MARGEN_GRUPO, minimo);
      nodes.unshift({
        id,
        type: 'grupo',
        position: { x: x - MARGEN_GRUPO, y: y - MARGEN_GRUPO },
        data: { nombre, color } satisfies GrupoData,
        selectable: false,
        focusable: false,
        zIndex: -1,
        style: {
          width: ancho,
          height: ALTO_RAMA + 2 * MARGEN_GRUPO,
          border: soloCabeza ? `2px solid ${color}` : '1px solid #3a4252',
          borderRadius: 16,
          background: 'transparent',
          pointerEvents: 'none',
        },
      });
    };
    // Tramo de una línea de tiempo: recto y sin flecha (es la misma rama avanzando).
    const tramo = (source: string, target: string, color: string, grosor: number) =>
      edges.push({
        id: `e-${source}-${target}`,
        source,
        sourceHandle: 'derecha',
        target,
        targetHandle: 'izquierda',
        type: 'straight',
        style: { stroke: color, strokeWidth: grosor },
      });
    const infoTerminada = (t: Tarea) => (
      <div className="space-y-1">
        <div className="font-mono text-[11px]">{t.rama_asociada}</div>
        <InfoFila k="Tarea" v={t.titulo} />
        {t.tarea_origen_id && <InfoFila k="Corrige" v={t.tarea_origen_id} />}
        <InfoFila k="Fecha" v={t.fecha_completada ?? '—'} />
      </div>
    );

    // ===== main =====
    // EJERCICIO VISUAL (datos de ejemplo): historia de main en una sola línea.
    // main (anterior) -> main/dev (el dev que ya se fusionó y es parte de main) -> main (actual).
    const historialMain = !cerrados.has('main');
    const pasadosMain = 1; // estados anteriores a la cabeza
    nodes.push({
      id: 'main',
      type: 'rama',
      position: { x: 40, y: 40 },
      // Sin textos explicativos en pantalla: las explicaciones viven en documentacion/AGENTE.md.
      // Cabeza de main = último merge en main (main/dev), igual que la cabeza de dev es su último
      // merge: no hay un recuadro "main" aparte. Click en ella cierra/abre el historial de main.
      data: { nombre: CABEZA_MAIN, info: 'dev fusionado en main · PR #17' } satisfies RamaData,
      style: { ...estiloRamaBase, border: '2px solid #60a5fa', cursor: 'pointer' },
    });
    if (historialMain) {
      const pasado = (id: string, x: number, nombre: string, info: string) =>
        nodes.push({ id, type: 'rama', position: { x, y: 40 }, data: { nombre, info } satisfies RamaData, style: estiloPasado });
      pasado('main-anterior', 40 - MAIN_X, 'main', 'Antes del PR #17');
      tramo('main-anterior', 'main', '#64748b', GROSOR_MAIN);
    }
    contenedor('grupo-main', 'main', '#60a5fa', historialMain ? 40 - pasadosMain * MAIN_X : 40, 40 + anchoRama(CABEZA_MAIN), 40, !historialMain);
    edges.push({
      id: 'e-main-dev',
      source: 'main',
      // main y dev son líneas de tiempo: su presente está al final, así que sus flechas hacia
      // las hijas salen POR ADELANTE (costado derecho).
      sourceHandle: 'derecha',
      target: 'dev-inicio',
      style: { stroke: '#60a5fa', strokeWidth: GROSOR_MAIN_DEV },
      type: 'flecha',
    });

    // ===== ramas de tareas =====
    // Abiertas: cuelgan de su ancestro abierto más cercano (o de dev).
    // Terminadas con padre abierto: recuadro gris en el historial de ese padre.
    const abiertas = tareas.filter((t) => !terminada(t));
    const hijosAbiertos = new Map<string, Tarea[]>();
    const raices: Tarea[] = [];
    abiertas.forEach((t) => {
      const a = ancestroAbierto(t);
      if (a) hijosAbiertos.set(a, [...(hijosAbiertos.get(a) ?? []), t]);
      else raices.push(t);
    });
    const historialDe = new Map<string, Tarea[]>();
    tareas
      .filter((t) => terminada(t) && padreEnGrafo(t) && !terminada(porId.get(padreEnGrafo(t)!)!))
      .forEach((t) => {
        const p = padreEnGrafo(t)!;
        historialDe.set(p, [...(historialDe.get(p) ?? []), t]);
      });
    const porNumero = (a: Tarea, b: Tarea) => a.id.localeCompare(b.id, undefined, { numeric: true });
    raices.sort(porNumero);

    // Tronco: dev va a la misma altura que main.
    const DEV_Y = 40;

    // ===== dev =====
    // Nace en un punto (sobre el borde de su panel), pasa por un recuadro gris por cada
    // tarea general ya fusionada (en orden) y termina en el último merge: su presente.
    const historialDev = !cerrados.has('dev');
    const merges = tareas
      .filter((t) => !padreEnGrafo(t) && terminada(t))
      .sort((a, b) => (a.fecha_completada ?? '').localeCompare(b.fecha_completada ?? '') || a.id.localeCompare(b.id));
    // Cabeza elegida (viaje en el tiempo): índice del merge hasta donde se ve dev; -1 = presente.
    const idxCabeza = historialDev && cabezaDev ? merges.findIndex((m) => m.id === cabezaDev) : -1;
    const enElPasado = idxCabeza >= 0;
    const enfocado = enElPasado && enfoqueDev;
    nodes.push({
      id: 'dev-inicio',
      type: 'punto',
      position: { x: DEV_INICIO_X, y: DEV_Y + (ALTO_RAMA - PUNTO) / 2 },
      data: {} satisfies PuntoData,
      style: estiloPunto('#2dd4bf'),
    });
    let anterior = 'dev-inicio';
    // Igual que la línea de main: empieza con el dev inicial (gris, pasado), tal como nació
    // de main; después van las tareas fusionadas. Es historial: se oculta al cerrarlo.
    if (historialDev) {
      nodes.push({
        id: 'dev-anterior',
        type: 'rama',
        position: { x: DEV_CONTENIDO_X, y: DEV_Y },
        data: { nombre: 'dev' } satisfies RamaData,
        style: estiloPasado,
      });
      tramo(anterior, 'dev-anterior', '#2dd4bf', GROSOR_MAIN_DEV);
      anterior = 'dev-anterior';
    }
    // La cabeza de dev es su ÚLTIMO merge (no hay un recuadro "dev" aparte): las ramas abiertas
    // trabajan sobre ese último estado. Va con el color de dev (presente), no en gris. Click en
    // ella cierra/abre el historial de dev; con el historial cerrado solo queda ella.
    // Sin ningún merge, la cabeza es un recuadro "dev".
    const ultimo = merges.length ? merges[merges.length - 1] : null;
    const cabezaId = ultimo ? `merge-${ultimo.id}` : 'dev';
    const xDeMerge = (i: number) => DEV_CONTENIDO_X + (historialDev ? i + 1 : 0) * PUNTO_X;
    merges.forEach((t, i) => {
      ubicaciones.set(t.id, { nodo: `merge-${t.id}`, abrir: t === ultimo ? [] : ['dev'] });
      const esCabezaActual = t === ultimo && !enElPasado;
      if (!esCabezaActual && (!historialDev || (enElPasado && i > idxCabeza))) return;
      const id = `merge-${t.id}`;
      nodes.push({
        id,
        type: 'rama',
        position: { x: esCabezaActual && !historialDev ? DEV_CONTENIDO_X : xDeMerge(i), y: DEV_Y },
        data: { nombre: t.id, info: infoTerminada(t) } satisfies RamaData,
        style: esCabezaActual
          ? { ...estiloRamaBase, border: '2px solid #2dd4bf', cursor: 'pointer' }
          : { ...estiloPasado, cursor: 'pointer' },
        className: i === idxCabeza ? 'cabeza-dev' : undefined,
      });
      tramo(anterior, id, '#2dd4bf', GROSOR_MAIN_DEV);
      anterior = id;
    });
    // devX = posición de la cabeza (de donde cuelgan las ramas de adelante).
    const devX = ultimo ? (historialDev ? xDeMerge(merges.length - 1) : DEV_CONTENIDO_X) : DEV_CONTENIDO_X + (historialDev ? 1 : 0) * PUNTO_X;
    const anchoCabeza = anchoRama(ultimo ? ultimo.id : 'dev');
    if (enElPasado) {
      // dev se ve hasta la cabeza elegida (viaje en el tiempo): el panel termina en ella.
      const xCabeza = DEV_CONTENIDO_X + (idxCabeza + 1) * PUNTO_X;
      contenedor('grupo-dev', 'dev', '#2dd4bf', DEV_CONTENIDO_X, xCabeza + anchoRama(merges[idxCabeza].id), DEV_Y);
    } else {
      if (!ultimo) {
        nodes.push({
          id: 'dev',
          type: 'rama',
          position: { x: devX, y: DEV_Y },
          data: { nombre: 'dev' } satisfies RamaData,
          style: { ...estiloRamaBase, border: '2px solid #2dd4bf', cursor: 'pointer' },
        });
        tramo(anterior, 'dev', '#2dd4bf', GROSOR_MAIN_DEV);
      }
      contenedor('grupo-dev', 'dev', '#2dd4bf', DEV_CONTENIDO_X, devX + anchoCabeza, DEV_Y, !historialDev);
    }


    // Historia de una tarea en orden de fusión, y la subtarea elegida como cabeza (-1 = presente).
    const historiaDe = (t: Tarea) =>
      (historialDe.get(t.id) ?? []).sort(
        (a, b) => (a.fecha_completada ?? '').localeCompare(b.fecha_completada ?? '') || porNumero(a, b)
      );
    const cabezaDe = (t: Tarea) =>
      cabezas[t.id] && !cerrados.has(t.id) ? historiaDe(t).findIndex((h) => h.id === cabezas[t.id]) : -1;
    const duenoHistorial = new Map<string, string>(); // nodo hist-* -> tarea dueña de ese historial
    // Subtareas abiertas que se ven de una tarea: en el presente, todas; con una cabeza elegida
    // en su historial (1er click), ninguna: quedan solo las tareas de su historial.
    const hijosVisibles = (t: Tarea): Tarea[] =>
      cabezaDe(t) >= 0 ? [] : (hijosAbiertos.get(t.id) ?? []).sort(porNumero);

    // nivel = profundidad en el árbol (1 = cuelga de dev), para el grosor. La altura sale de
    // filaDe (árbol centrado). salida/llegada = handles por los que sale y entra la flecha.
    const ubicar = (t: Tarea, nivel: number, izquierda: number, origen: string, salida: string, llegada: string) => {
      const y = yDe.get(t.id)!;
      const m = estadoMeta[t.estado];
      const grosor = grosorTarea(nivel);
      const historia = historiaDe(t);
      const idxCab = cabezaDe(t);
      historia.forEach((h) => {
        ubicaciones.set(h.id, { nodo: `hist-${h.id}`, abrir: [t.id] });
        duenoHistorial.set(`hist-${h.id}`, t.id);
      });
      ubicaciones.set(t.id, { nodo: t.id, abrir: [] });

      let x = izquierda; // posición del recuadro de la tarea (su presente)
      let derecha: number;
      if (historia.length) {
        // Igual que dev: punto de entrada sobre el borde del panel, recuadros grises de las
        // subtareas ya fusionadas y la tarea al final. Click en la tarea cierra/abre su historial.
        const abierto = !cerrados.has(t.id);
        const entrada = `entrada-${t.id}`;
        const contenidoX = izquierda + PUNTO - SOLAPE_BORDE + MARGEN_GRUPO;
        nodes.push({
          id: entrada,
          type: 'punto',
          position: { x: izquierda, y: y + (ALTO_RAMA - PUNTO) / 2 },
          data: {} satisfies PuntoData,
          style: estiloPunto(m.dot),
        });
        let ant = entrada;
        (abierto ? historia : []).forEach((h, i) => {
          if (idxCab >= 0 && i > idxCab) return; // viaje en el tiempo: lo posterior no existe aún
          const id = `hist-${h.id}`;
          nodes.push({
            id,
            type: 'rama',
            position: { x: contenidoX + i * PUNTO_X, y },
            data: { nombre: h.id, info: infoTerminada(h) } satisfies RamaData,
            style: { ...estiloPasado, cursor: 'pointer' },
            className: i === idxCab ? 'cabeza-dev' : undefined,
          });
          tramo(ant, id, m.dot, grosor);
          ant = id;
        });
        if (idxCab >= 0) {
          // La cabeza elegida es el final de la línea: sin la tarea actual.
          const xCab = contenidoX + idxCab * PUNTO_X;
          contenedor(`grupo-${t.id}`, t.id, m.dot, contenidoX, xCab + anchoRama(historia[idxCab].id), y);
          edges.push({ id: `e-${origen}-${entrada}`, source: origen, sourceHandle: salida, target: entrada, targetHandle: llegada, animated: t.estado === 'en producción', style: { stroke: m.dot, strokeWidth: grosor }, type: 'flecha' });
          // Las subtareas que ya existían en ese momento cuelgan de la cabeza (por adelante).
          hijosVisibles(t).forEach((h) => ubicar(h, nivel + 1, colDe.get(h.id)!, `hist-${historia[idxCab].id}`, 'derecha', 'izquierda'));
          return;
        }
        x = contenidoX + (abierto ? historia.length : 0) * PUNTO_X;
        tramo(ant, t.id, m.dot, grosor);
        contenedor(`grupo-${t.id}`, t.id, m.dot, contenidoX, x + anchoRama(t.id), y, !abierto);
        derecha = x + anchoRama(t.id) + MARGEN_GRUPO;
        edges.push({ id: `e-${origen}-${entrada}`, source: origen, sourceHandle: salida, target: entrada, targetHandle: llegada, animated: t.estado === 'en producción', style: { stroke: m.dot, strokeWidth: grosor }, type: 'flecha' });
      } else {
        derecha = x + anchoRama(t.id);
        edges.push({ id: `e-${origen}-${t.id}`, source: origen, sourceHandle: salida, target: t.id, targetHandle: llegada, animated: t.estado === 'en producción', style: { stroke: m.dot, strokeWidth: grosor }, type: 'flecha' });
      }

      nodes.push({
        id: t.id,
        type: 'rama',
        position: { x, y },
        data: {
          // En el recuadro solo el número de la tarea: es corto, nunca se corta y su forma
          // (T-101 → T-101-1) ya refleja el árbol. El nombre completo sale al pasar el mouse.
          nombre: t.id,
          info: (
            <div className="space-y-1">
              <div className="font-mono text-[11px] font-semibold">{t.rama_asociada}</div>
              <InfoFila k="Tarea" v={t.titulo} />
              {t.tarea_origen_id && <InfoFila k="Corrige" v={t.tarea_origen_id} />}
              <InfoFila k="Estado" v={<span style={{ color: m.dot }}>● {m.label}</span>} />
            </div>
          ),
        } satisfies RamaData,
        style: { ...estiloRamaBase, fontWeight: 600, border: `2px solid ${m.dot}`, cursor: historia.length ? 'pointer' : undefined },
      });

      // Subtareas: columna adelante de la tarea (la tarea queda centrada entre ellas). REGLA DE
      // ESCALA: son hijas, así que la flecha SALE POR ADELANTE del padre (costado derecho) y
      // entra por la izquierda de la hija.
      hijosVisibles(t).forEach((h) => ubicar(h, nivel + 1, colDe.get(h.id)!, t.id, 'derecha', 'izquierda'));
    };
    // Ramas de dev: una columna adelante de dev (dev centrado entre ellas, ver DEV_Y), en vez de
    // alargarse hacia la derecha. La columna empieza ADELANTO después del borde derecho de dev,
    // para que las curvas tengan espacio. Flechas por adelante, con triángulo hacia la hija.
    const columnaX = devX + anchoCabeza + ADELANTO;
    const xMerge = (i: number) => DEV_CONTENIDO_X + (i + 1) * PUNTO_X; // +1: el primero es el dev inicial

    // --- Árbol compacto (que parezca árbol y no escoba) ---
    // Cada rama ocupa solo su alto real (recuadro simple o panel con historial), y los subárboles
    // se encajan lo más cerca posible: solo se separan donde de verdad se tocarían (cajas que se
    // cruzan en horizontal). Cada rama queda centrada entre sus hijas, y dev entre sus ramas.
    // Las x no dependen de la altura (hijas = ADELANTO después del borde derecho del padre), así
    // que primero se mide en horizontal y después se empaca en vertical.
    type Caja = { id: string; x0: number; x1: number; y: number; alto: number };
    const medir = (t: Tarea, izquierda: number) => {
      const n = (historialDe.get(t.id) ?? []).length;
      if (!n) return { derecha: izquierda + anchoRama(t.id), alto: ALTO_RAMA };
      const contenidoX = izquierda + PUNTO - SOLAPE_BORDE + MARGEN_GRUPO;
      const idxCab = cabezaDe(t);
      if (idxCab >= 0) {
        const xCab = contenidoX + idxCab * PUNTO_X;
        return { derecha: xCab + anchoRama(historiaDe(t)[idxCab].id) + MARGEN_GRUPO, alto: ALTO_RAMA + 2 * MARGEN_GRUPO + 10 };
      }
      const x = contenidoX + (cerrados.has(t.id) ? 0 : n) * PUNTO_X;
      // panel: margen arriba y abajo, más el nombre que asoma por el borde superior
      return { derecha: x + anchoRama(t.id) + MARGEN_GRUPO, alto: ALTO_RAMA + 2 * MARGEN_GRUPO + 10 };
    };
    const seCruzan = (a: Caja, b: Caja) => a.x0 < b.x1 + 12 && b.x0 < a.x1 + 12;
    // Cuánto bajar "nuevo" para que no toque nada de "puestas" (separación mínima GAP_VERTICAL).
    const bajada = (puestas: Caja[], nuevo: Caja[]) => {
      let d = -Infinity;
      puestas.forEach((a) =>
        nuevo.forEach((b) => {
          if (seCruzan(a, b)) d = Math.max(d, a.y + (a.alto + b.alto) / 2 + GAP_VERTICAL - b.y);
        })
      );
      if (d === -Infinity) {
        // nada se cruza: simplemente debajo de todo
        const fondo = Math.max(...puestas.map((a) => a.y + a.alto / 2));
        const tope = Math.min(...nuevo.map((b) => b.y - b.alto / 2));
        d = fondo - tope + GAP_VERTICAL;
      }
      return d;
    };
    // Empaca el subárbol de t; devuelve sus cajas con la de t primero y en y = 0.
    const empacar = (t: Tarea, izquierda: number): Caja[] => {
      const m = medir(t, izquierda);
      const cajas: Caja[] = [];
      const yHijas: number[] = [];
      hijosVisibles(t).forEach((h) => {
        const sub = empacar(h, colDe.get(h.id)!);
        const d = cajas.length ? bajada(cajas, sub) : 0;
        sub.forEach((c) => cajas.push({ ...c, y: c.y + d }));
        yHijas.push(d);
      });
      const yPropia = yHijas.length ? (yHijas[0] + yHijas[yHijas.length - 1]) / 2 : 0;
      return [
        { id: t.id, x0: izquierda, x1: m.derecha, y: 0, alto: m.alto },
        ...cajas.map((c) => ({ ...c, y: c.y - yPropia })),
      ];
    };
    // EJERCICIO VISUAL (se puede quitar): ramas que nacieron de un estado ANTERIOR de dev.
    // De dónde nació cada rama: el último merge a dev anterior a su primer commit. Si después
    // de ese merge hubo otros, la rama cuelga de ese recuadro gris y no del dev actual. Sin
    // commits, o con el historial de dev cerrado, cuelga del dev actual como siempre.
    const primerCommit = new Map<string, string>();
    commits.forEach((c) => {
      const f = primerCommit.get(c.tarea_id);
      if (!f || c.fecha < f) primerCommit.set(c.tarea_id, c.fecha);
    });
    // Momento en que nació cada rama. Con commits: su primer commit. Sin commits (mientras no
    // haya backend que lo registre): un momento FIJO que no depende de los merges que lleguen
    // después, así una rama sigue colgando del dev del que nació aunque después se fusione otra.
    // - Hasta MAX_ADELANTE "recientes" (~1 de cada 4, las de número más bajo): FECHA_RECIENTE.
    // - El resto: justo después de un merge anterior fijo según su número.
    const MAX_ADELANTE = 5;
    const FECHA_RECIENTE = '2026-10-29'; // fecha de referencia de los datos de prueba
    const hashDe = (id: string) => [...id].reduce((acc, ch) => (acc * 31 + ch.charCodeAt(0)) % 9973, 7);
    const mergesBase = merges.filter((m) => (m.fecha_completada ?? '') < FECHA_RECIENTE);
    const recientes = new Set(
      tareas
        .filter(
          (t) =>
            !padreEnGrafo(t) &&
            !primerCommit.has(t.id) &&
            hashDe(t.id) % 4 === 0 &&
            !(terminada(t) && (t.fecha_completada ?? '') < FECHA_RECIENTE)
        )
        .sort(porNumero)
        .slice(0, MAX_ADELANTE)
        .map((t) => t.id)
    );
    const nacimiento = (t: Tarea): string => {
      const f = primerCommit.get(t.id);
      if (f) return f;
      if (recientes.has(t.id) || mergesBase.length < 2) return FECHA_RECIENTE;
      return `${mergesBase[hashDe(t.id) % (mergesBase.length - 1)].fecha_completada}T12:00`;
    };
    // Recuadro de dev del que nació: el último merge anterior a su nacimiento. Si es el último
    // merge de todos (la cabeza actual), cuelga del presente (null).
    const nacioEnSiempre = (t: Tarea): number | null => {
      if (!merges.length) return null;
      const f = nacimiento(t);
      let i = -1;
      merges.forEach((m, k) => {
        if ((m.fecha_completada ?? '') < f) i = k;
      });
      return i >= 0 && i < merges.length - 1 ? i : null;
    };
    const nacioEn = (t: Tarea) => (historialDev ? nacioEnSiempre(t) : null);
    const origen = new Map(raices.map((r) => [r.id, nacioEn(r)]));
    // Lado FIJO de cada rama de dev (arriba o abajo de la línea de dev): se decide una sola vez,
    // alternando en el orden de colocación con el historial abierto, y se respeta en cualquier
    // modo (historial cerrado, viaje en el tiempo...). Así una rama que está abajo nunca salta
    // arriba al cambiar de vista.
    const ordenLado = [...raices].sort((a, b) => {
      const oa = nacioEnSiempre(a) ?? Infinity;
      const ob = nacioEnSiempre(b) ?? Infinity;
      return ob - oa || porNumero(a, b);
    });
    const haciaArriba = new Set(ordenLado.filter((_, k) => k % 2 === 1).map((r) => r.id));
    // En el pasado (cabeza elegida) solo existen las ramas que nacieron hasta esa cabeza.
    // Con enfoque, solo las que nacieron justo de la cabeza (las de atrás se cierran con sus hijas).
    const existe = (r: Tarea) =>
      !enElPasado ||
      (origen.get(r.id) !== null && (enfocado ? origen.get(r.id)! === idxCabeza : origen.get(r.id)! <= idxCabeza));
    // Orden de colocación: primero (más cerca de la línea de dev) las que nacen de recuadros más
    // a la DERECHA; las de recuadros anteriores van más lejos y su flecha baja por la izquierda
    // de las cercanas, sin cruzarlas.
    const raicesPasado = raices
      .filter((r) => origen.get(r.id) !== null && existe(r))
      .sort((a, b) => origen.get(b.id)! - origen.get(a.id)! || porNumero(a, b));
    const raicesActual = enElPasado ? [] : raices.filter((r) => origen.get(r.id) === null);

    // Columnas parejas por nivel (para que ninguna línea cruce otra rama, creciendo a lo LARGO):
    // todas las ramas de un mismo nivel empiezan en la misma columna, y la del nivel siguiente
    // empieza ADELANTO después de la rama MÁS ANCHA del anterior. Así ninguna rama sobresale en
    // la zona donde giran las flechas (ver FlechaEdge: van rectas y giran al final).
    const colDe = new Map<string, number>();
    const asignarColumnas = (bosqueRaices: Tarea[], x0: number) => {
      let nivel = bosqueRaices;
      let x = x0;
      while (nivel.length) {
        nivel.forEach((t) => colDe.set(t.id, x));
        x += Math.max(...nivel.map((t) => medir(t, 0).derecha)) + ADELANTO;
        nivel = nivel.flatMap((t) => hijosVisibles(t));
      }
    };

    asignarColumnas(raicesActual, columnaX);
    raicesPasado.forEach((r) => asignarColumnas([r], xMerge(origen.get(r.id)!) + ADELANTO));

    // Colocación: cada rama de su lado (arriba/abajo) de la fila de dev, lo más cerca posible de
    // ella sin chocar con nada. La fila de dev, desde su cabeza hacia adelante, queda libre: por
    // ahí pasan las flechas hacia la columna de adelante.
    const altoPanel = ALTO_RAMA + 2 * MARGEN_GRUPO + 10;
    const ocupadas: Caja[] = [
      // los paneles de main y dev completos (con su nombre arriba), como barrera
      { id: 'panel-main', x0: -Infinity, x1: 40 + anchoRama(CABEZA_MAIN) + MARGEN_GRUPO, y: DEV_Y, alto: altoPanel },
      { id: 'panel-dev', x0: DEV_INICIO_X, x1: devX + anchoCabeza + MARGEN_GRUPO, y: DEV_Y, alto: altoPanel },
      { id: 'fila-dev', x0: devX, x1: Infinity, y: DEV_Y, alto: altoPanel },
    ];
    const yDe = new Map<string, number>();
    // Cuánto subir "nuevo" para que quede encima de todo lo que toca (espejo de bajada).
    const subida = (puestas: Caja[], nuevo: Caja[]) => {
      let d = Infinity;
      puestas.forEach((a) =>
        nuevo.forEach((b) => {
          if (seCruzan(a, b)) d = Math.min(d, a.y - (a.alto + b.alto) / 2 - GAP_VERTICAL - b.y);
        })
      );
      return d === Infinity ? 0 : d;
    };
    const colocar = (r: Tarea, izquierda: number) => {
      const sub = empacar(r, izquierda);
      const d = haciaArriba.has(r.id) ? subida(ocupadas, sub) : bajada(ocupadas, sub);
      sub.forEach((c) => {
        ocupadas.push({ ...c, y: c.y + d });
        yDe.set(c.id, c.y + d);
      });
    };
    // Primero la columna de adelante (cuelga de la cabeza de dev), después las de devs antiguos.
    raicesActual.forEach((r) => colocar(r, columnaX));
    raicesPasado.forEach((r) => colocar(r, xMerge(origen.get(r.id)!) + ADELANTO));

    raicesActual.forEach((t) => ubicar(t, 1, columnaX, cabezaId, 'derecha', 'izquierda'));
    // La flecha sale por abajo o por arriba del recuadro gris (según el lado al que va la rama):
    // por su costado derecho pasa la línea de dev.
    raicesPasado.forEach((t) =>
      ubicar(
        t,
        1,
        xMerge(origen.get(t.id)!) + ADELANTO,
        `merge-${merges[origen.get(t.id)!].id}`,
        haciaArriba.has(t.id) ? 'arriba-salida' : 'abajo',
        'izquierda'
      )
    );

    // Terminadas cuyo padre también está terminado: se ven a través del recuadro del padre.
    const ubicacionDe = (t: Tarea): Ubicacion | undefined => {
      const u = ubicaciones.get(t.id);
      if (u) return u;
      const p = padreEnGrafo(t);
      return p ? ubicacionDe(porId.get(p)!) : undefined;
    };
    tareas.forEach((t) => {
      const u = ubicacionDe(t);
      if (u) ubicaciones.set(t.id, u);
    });

    const conHistorial = new Set(['main', ...historialDe.keys()]);
    return { nodes, edges, ubicaciones, buscables: tareas, conHistorial, duenoHistorial, cabezaId };
  }, [tareasAll, commits, cerrados, cabezaDev, enfoqueDev, cabezas]);

  // ===== vistas: Árbol (diagrama) e Historial (ramas apiladas con su historia y commits) =====
  const [vista, setVista] = useState<'arbol' | 'historial'>('arbol');
  const [abiertaHistorial, setAbiertaHistorial] = useState<string | null>(null);

  // ===== buscador (sirve en las dos vistas) =====
  const [busqueda, setBusqueda] = useState('');
  const [rf, setRf] = useState<ReactFlowInstance | null>(null);
  const [irA, setIrA] = useState<string | null>(null);
  const [resaltado, setResaltado] = useState<string | null>(null);
  const resultados = useMemo(() => {
    const q = normalizar(busqueda.trim());
    if (!q) return [];
    return buscables
      .filter((t) => normalizar(`${t.id} ${t.titulo} ${t.rama_asociada ?? ''}`).includes(q))
      .sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true }))
      .slice(0, 8);
  }, [busqueda, buscables]);

  const irATarea = (t: Tarea) => {
    if (vista === 'historial') {
      setAbiertaHistorial(t.id);
      setBusqueda('');
      requestAnimationFrame(() =>
        document.getElementById(`historial-${t.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' })
      );
      return;
    }
    const u = ubicaciones.get(t.id);
    if (!u) return;
    setCabezaDev(null); // el buscador siempre trabaja sobre el presente
    setEnfoqueDev(false);
    setCabezas({});
    // Si está dentro de un historial cerrado, se abre primero.
    if (u.abrir.some((id) => cerrados.has(id))) {
      setCerrados((prev) => {
        const sig = new Set(prev);
        u.abrir.forEach((id) => sig.delete(id));
        return sig;
      });
    }
    setIrA(u.nodo);
    setBusqueda('');
  };

  // Cuando el nodo ya está dibujado: centra la vista en él y lo resalta un momento.
  useEffect(() => {
    if (!irA || !rf) return;
    const n = nodes.find((x) => x.id === irA);
    if (!n) return;
    const ancho = anchoRama((n.data as RamaData).nombre ?? '');
    rf.setCenter(n.position.x + ancho / 2, n.position.y + ALTO_RAMA / 2, { zoom: 1.3, duration: 600 });
    setResaltado(irA);
    setIrA(null);
    const timer = setTimeout(() => setResaltado(null), 1800);
    return () => clearTimeout(timer);
  }, [irA, rf, nodes]);

  const nodosVista = useMemo(
    () => (resaltado ? nodes.map((n) => (n.id === resaltado ? { ...n, className: 'resaltado' } : n)) : nodes),
    [nodes, resaltado]
  );

  return (
    // Ocupa todo el alto disponible: pantalla menos la barra superior (57px) y el margen de la
    // página (48px). El panel se estira y la leyenda queda abajo.
    <div className="flex flex-col" style={{ height: 'calc(100vh - 105px)' }}>
      <PageHeader title="Branches" />
      <div className="card relative min-h-[420px] flex-1 overflow-hidden">
        {/* Pestañas: Árbol / Historial */}
        <div className="absolute left-3 top-3 z-10 flex rounded-lg border border-border bg-surface-2 p-0.5 text-xs">
          {(
            [
              ['arbol', 'Árbol'],
              ['historial', 'Historial'],
            ] as const
          ).map(([id, etiqueta]) => (
            <button
              key={id}
              type="button"
              onClick={() => setVista(id)}
              className={`rounded-md px-3 py-1.5 font-medium transition ${
                vista === id ? 'bg-surface-3 text-text shadow-pop' : 'text-muted hover:text-text'
              }`}
            >
              {etiqueta}
            </button>
          ))}
        </div>

        {vista === 'historial' ? (
          <HistorialRamas busqueda={busqueda} abierta={abiertaHistorial} onAbrir={setAbiertaHistorial} />
        ) : (
        <ReactFlow
          nodes={nodosVista}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onInit={setRf}
          fitView
          proOptions={{ hideAttribution: true }}
          nodesConnectable={false}
          // Los nodos no se arrastran (su posición no se guarda), así que arrastrar con
          // click izquierdo mueve la vista desde cualquier punto, también sobre un nodo.
          nodesDraggable={false}
          onNodeClick={(_, n) => {
            if (conHistorial.has(n.id)) alternarHistorial(n.id);
            if (n.id === cabezaId && !cabezaDev) {
              alternarHistorial('dev'); // la cabeza de dev (último merge) cierra/abre su historial
              return;
            }
            if (n.id.startsWith('merge-')) {
              const id = n.id.slice('merge-'.length);
              if (id !== cabezaDev) {
                setCabezaDev(id); // 1er click (o en otro recuadro): cabeza, se apaga lo de adelante
                setEnfoqueDev(false);
              } else if (!enfoqueDev) {
                setEnfoqueDev(true); // 2º click: se cierran también las ramas de atrás con sus hijas
              } else {
                setCabezaDev(null); // 3er click: vuelve al presente
                setEnfoqueDev(false);
              }
            }
            const dueno = duenoHistorial.get(n.id);
            if (dueno) {
              const id = n.id.slice('hist-'.length);
              // Tareas: 1er click, solo las tareas de su historial hasta esa cabeza; 2º, como antes.
              if (cabezas[dueno] !== id) setCabezas((c) => ({ ...c, [dueno]: id }));
              else setCabezas(({ [dueno]: _, ...resto }) => resto);
            }
          }}
          panOnDrag
          // Dos dedos en el trackpad (o la rueda) mueven la vista; el pellizco hace zoom.
          panOnScroll
          zoomOnPinch
        >
          <Background color="#2b313f" gap={20} />
          <Controls showInteractive={false} />
        </ReactFlow>
        )}

        {/* Buscador: por número, título o nombre de rama; abiertas y terminadas. */}
        <div className="absolute right-3 top-3 z-10 w-72">
          <input
            className="input"
            placeholder="Buscar tarea…"
            value={busqueda}
            onChange={(e) => setBusqueda(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && resultados[0]) irATarea(resultados[0]);
              if (e.key === 'Escape') setBusqueda('');
            }}
          />
          {busqueda.trim() && vista === 'arbol' && (
            <div className="mt-1 max-h-72 overflow-y-auto rounded-lg border border-border bg-surface-2 py-1 shadow-pop">
              {resultados.length === 0 ? (
                <div className="px-3 py-2 text-xs text-faint">Sin resultados</div>
              ) : (
                resultados.map((t) => {
                  const m = estadoMeta[t.estado];
                  return (
                    <button
                      key={t.id}
                      type="button"
                      onClick={() => irATarea(t)}
                      className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition hover:bg-surface-3"
                    >
                      <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: m.dot }} />
                      <span className="shrink-0 font-mono font-semibold text-text">{t.id}</span>
                      <span className="truncate text-muted">{t.titulo}</span>
                    </button>
                  );
                })
              )}
            </div>
          )}
        </div>
      </div>
      {/* Leyenda: solo lo que aparece en el árbol. "Por hacer" no sale (esas tareas aún no tienen
          rama) y "Terminada" en el árbol es una rama ya fusionada: recuadro gris. */}
      <div className="mt-3 flex shrink-0 flex-wrap gap-4 text-xs text-muted">
        <span className="flex items-center gap-1.5">
          <span className="h-3 w-4 rounded border-2" style={{ borderColor: '#475063', background: '#161922' }} />
          Fusionada
        </span>
        {(['en producción', 'en revisión'] as const).map((e) => (
          <span key={e} className="flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-full" style={{ background: estadoMeta[e].dot }} />
            {estadoMeta[e].label}
          </span>
        ))}
      </div>
    </div>
  );
}
