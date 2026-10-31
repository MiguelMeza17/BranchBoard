import { useMemo, type ReactNode } from 'react';
import { useStore } from '../store/useStore';
import { avatarColor, estadoMeta, iniciales } from '../lib/ui';
import type { Commit, Tarea, Usuario } from '../types';

// Vista "Historial" de Branches: las ramas apiladas en una lista (en orden de árbol), cada una
// con su historia (de dónde salió, qué se fusionó en ella, en qué se fusionó) y sus commits.
// Solo datos: las explicaciones viven en documentacion/AGENTE.md.

const normalizar = (s: string) =>
  s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();
const porNumero = (a: Tarea, b: Tarea) => a.id.localeCompare(b.id, undefined, { numeric: true });

type Fila = { tarea: Tarea; nivel: number };

export function HistorialRamas({ busqueda, abierta, onAbrir }: {
  busqueda: string;
  abierta: string | null;
  onAbrir: (id: string | null) => void;
}) {
  const tareasAll = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  const usuarios = useStore((s) => s.usuarios);

  const { filas, mergesDev, commitsDe, hijasFusionadas, porId } = useMemo(() => {
    const tareas = tareasAll.filter((t) => !t.eliminada && t.rama_asociada);
    const porId = new Map(tareasAll.map((t) => [t.id, t]));
    const conRama = new Set(tareas.map((t) => t.id));
    const padreEnGrafo = (t: Tarea): string | null => {
      let cur = t.tarea_padre_id;
      while (cur && !conRama.has(cur)) cur = porId.get(cur)?.tarea_padre_id ?? null;
      return cur;
    };
    const hijas = new Map<string | null, Tarea[]>();
    tareas.forEach((t) => {
      const p = padreEnGrafo(t);
      hijas.set(p, [...(hijas.get(p) ?? []), t]);
    });
    // Orden de árbol: cada rama seguida de sus subtareas (con sangría por nivel).
    const filas: Fila[] = [];
    const recorrer = (padre: string | null, nivel: number) =>
      (hijas.get(padre) ?? []).sort(porNumero).forEach((t) => {
        filas.push({ tarea: t, nivel });
        recorrer(t.id, nivel + 1);
      });
    recorrer(null, 0);

    const porFecha = (a: Tarea, b: Tarea) => (a.fecha_completada ?? '').localeCompare(b.fecha_completada ?? '');
    const mergesDev = (hijas.get(null) ?? []).filter((t) => t.estado === 'terminada').sort(porFecha);
    const hijasFusionadas = (id: string) => (hijas.get(id) ?? []).filter((t) => t.estado === 'terminada').sort(porFecha);
    const commitsDe = new Map<string, typeof commits>();
    commits.forEach((c) => commitsDe.set(c.tarea_id, [...(commitsDe.get(c.tarea_id) ?? []), c]));
    commitsDe.forEach((lista) => lista.sort((a, b) => b.fecha.localeCompare(a.fecha)));
    return { filas, mergesDev, commitsDe, hijasFusionadas, porId };
  }, [tareasAll, commits]);

  const q = normalizar(busqueda.trim());
  const filasVisibles = q
    ? filas.filter(({ tarea: t }) => normalizar(`${t.id} ${t.titulo} ${t.rama_asociada ?? ''}`).includes(q))
    : filas;

  return (
    <div className="h-full overflow-y-auto px-4 pb-4 pt-16">
      {!q && (
        <>
          <FilaRama
            id="main"
            nombre="main"
            color="#60a5fa"
            abierta={abierta === 'main'}
            onClick={() => onAbrir(abierta === 'main' ? null : 'main')}
          >
            <Seccion titulo="Historia">
              <Evento texto="main" detalle="antes del PR #17" />
              <Evento texto="dev fusionado en main" detalle="PR #17" />
            </Seccion>
          </FilaRama>
          <FilaRama
            id="dev"
            nombre="dev"
            color="#2dd4bf"
            detalle={`${mergesDev.length} fusionadas`}
            abierta={abierta === 'dev'}
            onClick={() => onAbrir(abierta === 'dev' ? null : 'dev')}
          >
            <Seccion titulo="Historia">
              <Evento texto="Nace desde main" />
              {mergesDev.map((t) => (
                <Evento key={t.id} texto={`${t.id} · ${t.titulo}`} detalle={`fusionada ${t.fecha_completada ?? ''}`} />
              ))}
            </Seccion>
          </FilaRama>
        </>
      )}

      {filasVisibles.length === 0 && <div className="px-2 py-6 text-center text-xs text-faint">Sin resultados</div>}

      {filasVisibles.map(({ tarea: t, nivel }) => {
        const m = estadoMeta[t.estado];
        const lista = commitsDe.get(t.id) ?? [];
        const fusionadas = hijasFusionadas(t.id);
        const padre = t.tarea_padre_id ? porId.get(t.tarea_padre_id) : null;
        return (
          <FilaRama
            key={t.id}
            id={t.id}
            nombre={t.id}
            titulo={t.titulo}
            color={m.dot}
            gris={t.estado === 'terminada'}
            nivel={q ? 0 : nivel}
            detalle={`${lista.length} commit${lista.length === 1 ? '' : 's'}`}
            abierta={abierta === t.id}
            onClick={() => onAbrir(abierta === t.id ? null : t.id)}
          >
            <Seccion titulo="Historia">
              <Evento texto={`Rama ${t.rama_asociada}`} />
              <Evento texto={padre ? `Sale de ${padre.id}` : 'Sale de dev'} />
              {t.tarea_origen_id && <Evento texto={`Corrige ${t.tarea_origen_id}`} />}
              {fusionadas.map((h) => (
                <Evento key={h.id} texto={`${h.id} · ${h.titulo}`} detalle={`fusionada ${h.fecha_completada ?? ''}`} />
              ))}
              <Evento
                texto={
                  t.estado === 'terminada'
                    ? `Fusionada en ${padre ? padre.id : 'dev'}`
                    : m.label
                }
                detalle={t.estado === 'terminada' ? t.fecha_completada ?? undefined : undefined}
                color={m.dot}
              />
            </Seccion>
            <Seccion titulo="Commits">
              <LineaCommits commits={lista} color={t.estado === 'terminada' ? '#64748b' : m.dot} usuarios={usuarios} />
            </Seccion>
          </FilaRama>
        );
      })}
    </div>
  );
}

function FilaRama({ id, nombre, titulo, color, gris, nivel = 0, detalle, abierta, onClick, children }: {
  id: string;
  nombre: string;
  titulo?: string;
  color: string;
  gris?: boolean;
  nivel?: number;
  detalle?: string;
  abierta: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <div id={`historial-${id}`} className="mb-1.5" style={{ marginLeft: nivel * 20 }}>
      <button
        type="button"
        onClick={onClick}
        className={`flex w-full items-center gap-3 rounded-lg border px-3 py-2 text-left text-xs transition hover:bg-surface-3 ${
          abierta ? 'border-border-strong bg-surface-3' : 'border-border bg-surface-2'
        }`}
      >
        <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: gris ? '#475063' : color }} />
        <span className={`shrink-0 font-mono font-semibold ${gris ? 'text-faint' : 'text-text'}`}>{nombre}</span>
        {titulo && <span className={`min-w-0 flex-1 truncate ${gris ? 'text-faint' : 'text-muted'}`}>{titulo}</span>}
        {!titulo && <span className="flex-1" />}
        {detalle && <span className="shrink-0 text-faint">{detalle}</span>}
        <span className="shrink-0 text-faint">{abierta ? '▾' : '▸'}</span>
      </button>
      {abierta && <div className="mt-1 space-y-3 rounded-lg border border-border bg-surface px-4 py-3">{children}</div>}
    </div>
  );
}

function Seccion({ titulo, children }: { titulo: string; children: ReactNode }) {
  return (
    <div>
      <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-faint">{titulo}</div>
      {children}
    </div>
  );
}

function Evento({ texto, detalle, color }: { texto: string; detalle?: string; color?: string }) {
  return (
    <div className="flex items-baseline gap-2 py-0.5 text-xs">
      <span className="shrink-0" style={{ color: color ?? '#6b7385' }}>●</span>
      <span className="text-text">{texto}</span>
      {detalle && <span className="font-mono text-faint">{detalle}</span>}
    </div>
  );
}

// ---- Commits: línea de tiempo vertical (tipo grafo de git) ----
const MESES = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sep', 'oct', 'nov', 'dic'];
const diaCorto = (iso: string) => `${Number(iso.slice(8, 10))} ${MESES[Number(iso.slice(5, 7)) - 1]}`;
function haceCuanto(iso: string) {
  const dias = Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000);
  if (dias <= 0) return 'hoy';
  if (dias === 1) return 'ayer';
  if (dias < 30) return `hace ${dias} días`;
  const meses = Math.floor(dias / 30);
  return meses === 1 ? 'hace 1 mes' : `hace ${meses} meses`;
}

function LineaCommits({ commits, color, usuarios }: { commits: Commit[]; color: string; usuarios: Usuario[] }) {
  const showToast = useStore((s) => s.showToast);
  if (!commits.length) return <div className="text-xs text-faint">Sin commits todavía</div>;

  const usuario = (email: string) => usuarios.find((u) => u.email === email);
  const autores = new Set(commits.map((c) => c.autor));
  // Actividad por día (para el mini gráfico), del más antiguo al más reciente.
  const porDia = new Map<string, number>();
  [...commits].reverse().forEach((c) => porDia.set(c.fecha.slice(0, 10), (porDia.get(c.fecha.slice(0, 10)) ?? 0) + 1));
  const maxDia = Math.max(...porDia.values());
  // Agrupados por día (los commits ya vienen del más reciente al más antiguo).
  const grupos: { dia: string; lista: Commit[] }[] = [];
  commits.forEach((c) => {
    const dia = c.fecha.slice(0, 10);
    if (grupos[grupos.length - 1]?.dia === dia) grupos[grupos.length - 1].lista.push(c);
    else grupos.push({ dia, lista: [c] });
  });

  const copiar = (hash: string) => {
    navigator.clipboard?.writeText(hash);
    showToast(`Hash ${hash} copiado`, 'info');
  };

  return (
    <div>
      {/* Resumen */}
      <div className="mb-3 flex flex-wrap items-end gap-x-5 gap-y-2 text-xs">
        <Dato valor={commits.length} etiqueta={commits.length === 1 ? 'commit' : 'commits'} />
        <Dato valor={autores.size} etiqueta={autores.size === 1 ? 'autor' : 'autores'} />
        <Dato valor={haceCuanto(commits[0].fecha)} etiqueta="último" />
        <div className="ml-auto flex h-7 items-end gap-0.5" title="Commits por día">
          {[...porDia.entries()].map(([dia, n]) => (
            <span
              key={dia}
              title={`${diaCorto(dia)}: ${n} commit${n === 1 ? '' : 's'}`}
              className="w-2 rounded-sm"
              style={{ height: `${Math.max(20, (n / maxDia) * 100)}%`, background: color, opacity: 0.85 }}
            />
          ))}
        </div>
      </div>

      {/* Línea de tiempo */}
      <div className="relative pl-6">
        <span className="absolute bottom-2 left-[7px] top-2 w-0.5 rounded-full" style={{ background: `${color}55` }} />
        {grupos.map((g) => (
          <div key={g.dia} className="mb-2">
            <div className="relative -ml-6 mb-1 flex items-center gap-2">
              <span className="z-10 ml-[3px] h-2.5 w-2.5 rounded-full border-2 bg-surface" style={{ borderColor: color }} />
              <span className="text-[11px] font-semibold uppercase tracking-wide text-faint">{diaCorto(g.dia)}</span>
            </div>
            {g.lista.map((c) => {
              const u = usuario(c.autor);
              const nombre = u ? `${u.nombre} ${u.apellido}` : c.autor;
              return (
                <div key={c.id} className="group relative flex items-center gap-3 rounded-md py-1.5 pr-2 transition hover:bg-surface-2">
                  <span className="absolute -left-[21px] top-1/2 z-10 h-2 w-2 -translate-y-1/2 rounded-full" style={{ background: color }} />
                  <span
                    className="grid h-6 w-6 shrink-0 place-items-center rounded-full text-[10px] font-semibold text-white"
                    style={{ background: avatarColor(c.autor) }}
                    title={nombre}
                  >
                    {u ? iniciales(u.nombre, u.apellido) : c.autor.slice(0, 2).toUpperCase()}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-xs text-text">{c.mensaje}</div>
                    <div className="text-[11px] text-faint" title={c.fecha.replace('T', ' ')}>
                      {nombre} · {haceCuanto(c.fecha)} · {c.fecha.slice(11, 16)}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => copiar(c.hash)}
                    className="shrink-0 rounded-md border border-border bg-surface-3 px-2 py-0.5 font-mono text-[11px] text-muted transition hover:border-border-strong hover:text-text"
                    title="Copiar hash"
                  >
                    {c.hash}
                  </button>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function Dato({ valor, etiqueta }: { valor: string | number; etiqueta: string }) {
  return (
    <div>
      <div className="text-sm font-semibold text-text">{valor}</div>
      <div className="text-[11px] text-faint">{etiqueta}</div>
    </div>
  );
}
