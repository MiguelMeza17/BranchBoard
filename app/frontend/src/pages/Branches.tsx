import { useMemo } from 'react';
import ReactFlow, { Background, Controls, MarkerType, type Edge, type Node } from 'reactflow';
import 'reactflow/dist/style.css';
import { useStore } from '../store/useStore';
import { estadoMeta } from '../lib/ui';
import { PageHeader } from '../components/primitives';
import type { Tarea } from '../types';

export function Branches() {
  const tareasAll = useStore((s) => s.tareas);

  const { nodes, edges } = useMemo(() => {
    const tareas = tareasAll.filter((t) => !t.eliminada && t.rama_asociada);
    const porId = new Map(tareasAll.map((t) => [t.id, t]));
    const conRama = new Set(tareas.map((t) => t.id));

    // Ancestro más cercano que sí está en el grafo: un padre sin rama (aún sin
    // tomar) o en la Papelera no se dibuja, así que la hija cuelga del siguiente.
    const padreEnGrafo = (t: Tarea): string | null => {
      let cur = t.tarea_padre_id;
      while (cur && !conRama.has(cur)) cur = porId.get(cur)?.tarea_padre_id ?? null;
      return cur;
    };

    // profundidad de cada tarea según la cadena de padres dibujados
    const depthOf = (t: Tarea): number => {
      let d = 1; // dev = 0
      let cur = padreEnGrafo(t);
      while (cur) {
        d++;
        cur = padreEnGrafo(porId.get(cur)!);
      }
      return d;
    };

    const nodes: Node[] = [];
    const edges: Edge[] = [];

    // raíz dev
    nodes.push({
      id: 'dev',
      position: { x: 40, y: 40 },
      data: { label: '🌳 dev' },
      style: {
        background: '#161922',
        color: '#e7e9ee',
        border: '2px solid #2dd4bf',
        borderRadius: 12,
        padding: '10px 16px',
        fontWeight: 700,
        fontFamily: 'JetBrains Mono, monospace',
      },
      draggable: true,
    });

    const perLevel: Record<number, number> = {};
    tareas.forEach((t) => {
      const d = depthOf(t);
      perLevel[d] = (perLevel[d] ?? 0) + 1;
      const idx = perLevel[d] - 1;
      const m = estadoMeta[t.estado];
      nodes.push({
        id: t.id,
        position: { x: 40 + idx * 250, y: 40 + d * 130 },
        data: {
          label: (
            <div style={{ textAlign: 'left' }}>
              <div style={{ fontSize: 11, fontFamily: 'JetBrains Mono, monospace', opacity: 0.8 }}>
                {t.rama_asociada}
              </div>
              <div style={{ fontSize: 12, fontWeight: 600, marginTop: 2 }}>{t.titulo}</div>
              <div style={{ fontSize: 10, marginTop: 3, color: m.dot }}>● {m.label}</div>
            </div>
          ),
        },
        style: {
          background: '#1d212c',
          color: '#e7e9ee',
          border: `2px solid ${m.dot}`,
          borderRadius: 12,
          padding: '8px 12px',
          width: 210,
        },
        draggable: true,
      });
      const padre = padreEnGrafo(t) ?? 'dev';
      edges.push({
        id: `e-${padre}-${t.id}`,
        source: padre,
        target: t.id,
        animated: t.estado === 'en producción',
        style: { stroke: m.dot, strokeWidth: 2 },
        markerEnd: { type: MarkerType.ArrowClosed, color: m.dot },
      });
    });

    return { nodes, edges };
  }, [tareasAll]);

  return (
    <div>
      <PageHeader
        title="Branches"
        desc="Árbol de ascendencia de ramas — dev como raíz, ramas de tareas y sus hijas, coloreadas por estado."
      />
      <div className="card overflow-hidden" style={{ height: 560 }}>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          fitView
          proOptions={{ hideAttribution: true }}
          nodesConnectable={false}
        >
          <Background color="#2b313f" gap={20} />
          <Controls showInteractive={false} />
        </ReactFlow>
      </div>
      <div className="mt-3 flex flex-wrap gap-3 text-xs text-muted">
        {Object.values(estadoMeta).map((m) => (
          <span key={m.label} className="flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-full" style={{ background: m.dot }} />
            {m.label}
          </span>
        ))}
      </div>
    </div>
  );
}
