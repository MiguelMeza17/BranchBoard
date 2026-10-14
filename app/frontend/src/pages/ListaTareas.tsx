import { useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { ESTADOS, type EstadoTarea, type TipoTarea } from '../types';
import { tareaVisibleEnTableroPrincipal } from '../lib/permisos';
import { PageHeader, EstadoBadge, TipoBadge, Avatar, EmptyState } from '../components/primitives';
import { CrearTareaModal } from '../components/CrearTareaModal';

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

  const tareas = useMemo(() => {
    return tareasAll
      .filter((t) => !t.eliminada && tareaVisibleEnTableroPrincipal(t, user))
      .filter((t) => (filtroTipo ? t.tipo === filtroTipo : true))
      .filter((t) => (filtroEstado ? t.estado === filtroEstado : true))
      .filter((t) =>
        busqueda ? t.titulo.toLowerCase().includes(busqueda.toLowerCase()) || t.id.toLowerCase().includes(busqueda.toLowerCase()) : true
      );
  }, [tareasAll, user, filtroTipo, filtroEstado, busqueda]);

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
      </div>

      {tareas.length === 0 ? (
        <EmptyState icon="📋" title="No hay tareas" desc="Crea la primera tarea del proyecto." />
      ) : (
        <div className="card overflow-hidden">
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
            <tbody>
              {tareas.map((t) => (
                <tr key={t.id} className="border-b border-border/50 transition-colors hover:bg-surface-2">
                  <td className="px-4 py-3 font-mono text-xs text-faint">{t.id}</td>
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-text">{t.titulo}</span>
                      {t.pendiente_aprobacion && (
                        <span className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300">⏳ Pendiente</span>
                      )}
                      {t.edicion_pendiente && (
                        <span className="chip border border-warn/40 bg-yellow-500/15 text-yellow-300">✏️ Edición</span>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3"><TipoBadge tipo={t.tipo} /></td>
                  <td className="px-4 py-3"><EstadoBadge estado={t.estado} /></td>
                  <td className="px-4 py-3">
                    {t.asignado_a ? <Avatar email={t.asignado_a} size={24} /> : <span className="text-faint">—</span>}
                  </td>
                  <td className="px-4 py-3 text-muted">{t.con_fecha ? t.fecha_limite : 'Sin fecha'}</td>
                </tr>
              ))}
            </tbody>
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
