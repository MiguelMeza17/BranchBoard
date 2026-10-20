import { useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { ESTADOS, type EstadoTarea, type Tarea, type TipoTarea } from '../types';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { PageHeader, EstadoBadge, TipoBadge, Avatar, EmptyState } from '../components/primitives';
import { CrearTareaModal } from '../components/CrearTareaModal';

// Orden dentro de cada nivel del árbol: "Sin asignar" (por hacer sin dueño, igual que la
// columna visual del Tablero Scrum) va primero, luego el resto de estados en orden de flujo;
// a igual estado, por ID ascendente (T-101-2 antes que T-211).
function rangoEstado(t: Tarea): number {
  if (t.estado === 'por hacer' && !t.asignado_a) return 0;
  return ESTADOS.indexOf(t.estado) + 1;
}

function compararTareas(a: Tarea, b: Tarea): number {
  return rangoEstado(a) - rangoEstado(b) || a.id.localeCompare(b.id, undefined, { numeric: true });
}

// Etiqueta de estado de aprobación con una explicación corta en gris al pasar el mouse.
// La ayuda se abre siempre a la derecha, centrada con la etiqueta, para que quede dentro de
// su propia fila y no se salga de la tarjeta en la última.
function EtiquetaConAyuda({ className, ayuda, children }: { className: string; ayuda: string; children: React.ReactNode }) {
  return (
    <span className="group relative">
      <span className={`chip cursor-help border ${className}`}>{children}</span>
      <span className="pointer-events-none absolute left-full top-1/2 z-20 ml-2 hidden w-80 -translate-y-1/2 rounded-md border border-border bg-surface-2 px-2.5 py-1.5 text-xs font-normal text-muted shadow-lg group-hover:block">
        {ayuda}
      </span>
    </span>
  );
}

export function ListaTareas() {
  const user = useStore((s) => s.currentUser());
  const tareasAll = useStore((s) => s.tareas);
  const crearTareaConSubtareas = useStore((s) => s.crearTareaConSubtareas);
  const sprints = useStore((s) => s.sprints);
  const [params, setParams] = useSearchParams();

  const filtroTipo = params.get('tipo') as TipoTarea | null;
  const [filtroEstado, setFiltroEstado] = useState<EstadoTarea | ''>('');
  const [busqueda, setBusqueda] = useState('');
  const [showCrear, setShowCrear] = useState(false);
  const buscadorRef = useRef<HTMLInputElement>(null);

  // Atajos de teclado: "/" enfoca la búsqueda, "n" abre "Nueva tarea" — se ignoran
  // mientras ya se está escribiendo en un campo, o si hay un modal abierto.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      const escribiendo = ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
      if (showCrear) return;
      if (e.key === '/' && !escribiendo) {
        e.preventDefault();
        buscadorRef.current?.focus();
      } else if (e.key === 'n' && !escribiendo && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        setShowCrear(true);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [showCrear]);

  const filtrando = Boolean(filtroTipo || filtroEstado || busqueda);
  // Carpetas abiertas a mano; con un filtro activo se abren solas las que llevan a un
  // resultado, y entonces se recuerda cuáles cerró el usuario.
  const [abiertas, setAbiertas] = useState<Set<string>>(new Set());
  const [cerradasEnFiltro, setCerradasEnFiltro] = useState<Set<string>>(new Set());

  const { raices, hijosDe, coincide } = useMemo(() => {
    const visibles = tareasAll.filter((t) => !t.eliminada && tareaVisibleEnTableroPrincipal(t, user));
    const porId = new Map(visibles.map((t) => [t.id, t]));
    const q = busqueda.toLowerCase();
    const coincide = new Set(
      visibles
        .filter((t) => (filtroTipo ? t.tipo === filtroTipo : true))
        .filter((t) => (filtroEstado ? t.estado === filtroEstado : true))
        .filter((t) => (q ? t.titulo.toLowerCase().includes(q) || t.id.toLowerCase().includes(q) : true))
        .map((t) => t.id)
    );
    // Con filtro se muestran los resultados más la cadena de carpetas que lleva hasta ellos.
    const incluidas = new Set(coincide);
    for (const id of coincide) {
      let padre = porId.get(id)?.tarea_padre_id;
      while (padre && porId.has(padre) && !incluidas.has(padre)) {
        incluidas.add(padre);
        padre = porId.get(padre)!.tarea_padre_id;
      }
    }
    const raices: Tarea[] = [];
    const hijosDe = new Map<string, Tarea[]>();
    for (const t of visibles) {
      if (!incluidas.has(t.id)) continue;
      // Si el padre no es visible para este usuario, la subtarea se muestra como raíz.
      if (t.tarea_padre_id && porId.has(t.tarea_padre_id)) {
        hijosDe.set(t.tarea_padre_id, [...(hijosDe.get(t.tarea_padre_id) ?? []), t]);
      } else {
        raices.push(t);
      }
    }
    raices.sort(compararTareas);
    hijosDe.forEach((hijos) => hijos.sort(compararTareas));
    return { raices, hijosDe, coincide };
  }, [tareasAll, user, filtroTipo, filtroEstado, busqueda]);

  useEffect(() => setCerradasEnFiltro(new Set()), [filtroTipo, filtroEstado, busqueda]);

  const estaAbierta = (id: string) => (filtrando ? !cerradasEnFiltro.has(id) : abiertas.has(id));

  function alternar(id: string) {
    const [actual, guardar] = filtrando ? [cerradasEnFiltro, setCerradasEnFiltro] : [abiertas, setAbiertas];
    const nuevo = new Set(actual);
    if (nuevo.has(id)) nuevo.delete(id);
    else nuevo.add(id);
    guardar(nuevo);
  }

  function expandirTodo(abrir: boolean) {
    const conHijos = new Set(hijosDe.keys());
    if (filtrando) setCerradasEnFiltro(abrir ? new Set() : conHijos);
    else setAbiertas(abrir ? conHijos : new Set());
  }

  function filas(nivel: Tarea[], profundidad: number): JSX.Element[] {
    return nivel.flatMap((t) => {
      const hijos = hijosDe.get(t.id) ?? [];
      const abierta = hijos.length > 0 && estaAbierta(t.id);
      const fila = (
        <tr
          key={t.id}
          className={`border-b border-border/50 transition-colors hover:bg-surface-2 ${hijos.length ? 'cursor-pointer' : ''} ${
            filtrando && !coincide.has(t.id) ? 'opacity-50' : ''
          }`}
          onClick={hijos.length ? () => alternar(t.id) : undefined}
        >
          <td className="whitespace-nowrap px-4 py-3 font-mono text-xs text-faint">
            <div className="flex items-center gap-1.5" style={{ paddingLeft: profundidad * 20 }}>
              <span className="w-3 text-center text-muted">{hijos.length ? (abierta ? '▾' : '▸') : ''}</span>
              <span className="font-sans text-sm">{hijos.length ? (abierta ? '📂' : '📁') : '📄'}</span>
              <span>{t.id}</span>
              {hijos.length > 0 && <span className="font-sans text-[10px] text-faint">({hijos.length})</span>}
            </div>
          </td>
          <td className="px-4 py-3">
            <div className="flex items-center gap-2">
              <span className="font-medium text-text">{t.titulo}</span>
              {t.pendiente_aprobacion && (
                <EtiquetaConAyuda
                  className="border-amber-500/40 bg-amber-500/15 text-amber-300"
                  ayuda="La creó un Desarrollador o Tester y falta que un Líder o Admin la apruebe. Se puede trabajar, pero no enviar a revisión."
                >
                  ⏳ Pendiente
                </EtiquetaConAyuda>
              )}
              {t.edicion_pendiente && (
                <EtiquetaConAyuda
                  className="border-warn/40 bg-yellow-500/15 text-yellow-300"
                  ayuda="Se modificó después de aprobada. Un Líder o Admin debe confirmar o revertir los cambios."
                >
                  ✏️ Edición
                </EtiquetaConAyuda>
              )}
            </div>
          </td>
          <td className="px-4 py-3"><TipoBadge tipo={t.tipo} /></td>
          <td className="px-4 py-3"><EstadoBadge estado={t.estado} /></td>
          <td className="px-4 py-3">
            {t.asignado_a ? <Avatar email={t.asignado_a} size={24} /> : <span className="text-faint">Sin asignar</span>}
          </td>
          <td className="px-4 py-3 text-muted">{t.con_fecha ? t.fecha_limite : 'Sin fecha'}</td>
        </tr>
      );
      return abierta ? [fila, ...filas(hijos, profundidad + 1)] : [fila];
    });
  }

  return (
    <div>
      <PageHeader
        title="Lista de tareas"
        desc="Todas las tareas del proyecto. Aquí se crean — la rama se crea después, al tomarlas."
        actions={
          <button className="btn-primary" onClick={() => setShowCrear(true)}>
            + Nueva tarea
          </button>
        }
      />

      <div className="mb-4 flex flex-wrap items-center gap-2">
        <div className="relative max-w-xs">
          <input
            ref={buscadorRef}
            className="input pr-8"
            placeholder="Buscar por título o ID…"
            value={busqueda}
            onChange={(e) => setBusqueda(e.target.value)}
          />
          {!busqueda && (
            <kbd className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 rounded border border-border bg-surface-3 px-1.5 py-0.5 text-[10px] text-faint">
              /
            </kbd>
          )}
        </div>
        <select
          className="input max-w-[180px]"
          value={filtroEstado}
          onChange={(e) => setFiltroEstado(e.target.value as EstadoTarea | '')}
        >
          <option value="">Todos los estados</option>
          {ESTADOS.map((e) => (
            <option key={e} value={e}>
              {e}
            </option>
          ))}
        </select>
        {filtroTipo && (
          <button
            className="chip border border-primary/40 bg-primary-soft text-indigo-200"
            onClick={() => {
              params.delete('tipo');
              setParams(params);
            }}
          >
            Tipo: {filtroTipo} ✕
          </button>
        )}
        {hijosDe.size > 0 && (
          <div className="ml-auto flex gap-2">
            <button className="btn-ghost text-xs" onClick={() => expandirTodo(true)}>
              Expandir todo
            </button>
            <button className="btn-ghost text-xs" onClick={() => expandirTodo(false)}>
              Colapsar todo
            </button>
          </div>
        )}
      </div>

      {raices.length === 0 ? (
        <EmptyState icon="📋" title="No hay tareas" desc="Crea la primera tarea del proyecto." />
      ) : (
        <div className="card">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left text-xs text-faint">
                <th className="px-4 py-3 font-medium">ID</th>
                <th className="px-4 py-3 font-medium">Título</th>
                <th className="px-4 py-3 font-medium">Tipo</th>
                <th className="px-4 py-3 font-medium">Estado</th>
                <th className="px-4 py-3 font-medium">Asignado</th>
                <th className="px-4 py-3 font-medium">Fecha</th>
              </tr>
            </thead>
            <tbody>{filas(raices, 0)}</tbody>
          </table>
        </div>
      )}

      {showCrear && (
        <CrearTareaModal
          onClose={() => setShowCrear(false)}
          onCreate={(data) => {
            crearTareaConSubtareas({ ...data, tarea_padre_id: null });
            setShowCrear(false);
          }}
          rolUsuario={user.rol}
          sprints={sprints}
        />
      )}
    </div>
  );
}
