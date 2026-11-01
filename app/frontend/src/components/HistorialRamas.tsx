import { useMemo, useState, type ReactNode } from 'react';
import { useStore } from '../store/useStore';
import { Modal } from './Modal';
import { avatarColor, esCheckinTarea, estadoMeta, iniciales } from '../lib/ui';
import type { Commit, Tarea, Usuario } from '../types';

// Vista "Historial" de Branches: cada rama es una caja que contiene a sus hijas (main guarda a
// todo: dentro dev, dentro de dev las tareas y dentro de cada tarea sus subtareas). Cada una
// con su historia (de dónde salió, qué se fusionó en ella, en qué se fusionó) y sus commits.
// Solo datos: las explicaciones viven en documentacion/AGENTE.md.

const normalizar = (s: string) =>
  s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();
const porNumero = (a: Tarea, b: Tarea) => a.id.localeCompare(b.id, undefined, { numeric: true });

export function HistorialRamas({ busqueda, abierta, onAbrir }: {
  busqueda: string;
  abierta: string | null;
  onAbrir: (id: string | null) => void;
}) {
  const tareasAll = useStore((s) => s.tareas);
  const commits = useStore((s) => s.commits);
  const usuarios = useStore((s) => s.usuarios);

  const { hijas, mergesDev, commitsDe, hijasFusionadas, porId, padreDe } = useMemo(() => {
    const tareas = tareasAll.filter((t) => !t.eliminada && t.rama_asociada);
    const porId = new Map(tareasAll.map((t) => [t.id, t]));
    const conRama = new Set(tareas.map((t) => t.id));
    const padreEnGrafo = (t: Tarea): string | null => {
      let cur = t.tarea_padre_id;
      while (cur && !conRama.has(cur)) cur = porId.get(cur)?.tarea_padre_id ?? null;
      return cur;
    };
    const padreDe = new Map<string, string | null>();
    const hijas = new Map<string | null, Tarea[]>();
    tareas.forEach((t) => {
      const p = padreEnGrafo(t);
      padreDe.set(t.id, p);
      hijas.set(p, [...(hijas.get(p) ?? []), t]);
    });
    hijas.forEach((l) => l.sort(porNumero));

    const porFecha = (a: Tarea, b: Tarea) => (a.fecha_completada ?? '').localeCompare(b.fecha_completada ?? '');
    const mergesDev = (hijas.get(null) ?? []).filter((t) => t.estado === 'terminada').sort(porFecha);
    const hijasFusionadas = (id: string) => (hijas.get(id) ?? []).filter((t) => t.estado === 'terminada').sort(porFecha);
    const commitsDe = new Map<string, typeof commits>();
    // Los commits de una subtarea con check-in directo (sin rama propia) van en la rama de su padre.
    const ramaDeCommit = (tareaId: string) => {
      const tt = porId.get(tareaId);
      return tt && !tt.rama_asociada && tt.modo_trabajo === 'checkin' && tt.tarea_padre_id ? tt.tarea_padre_id : tareaId;
    };
    commits.forEach((c) => {
      const r = ramaDeCommit(c.tarea_id);
      commitsDe.set(r, [...(commitsDe.get(r) ?? []), c]);
    });
    commitsDe.forEach((lista) => lista.sort((a, b) => b.fecha.localeCompare(a.fecha)));
    return { hijas, mergesDev, commitsDe, hijasFusionadas, porId, padreDe };
  }, [tareasAll, commits]);

  // Con búsqueda: las que coinciden y todos sus contenedores (para ver dónde está cada una).
  const q = normalizar(busqueda.trim());
  const visibles = useMemo(() => {
    if (!q) return null;
    const set = new Set<string>();
    porId.forEach((t) => {
      if (!padreDe.has(t.id)) return; // sin rama: no está en Branches
      if (!normalizar(`${t.id} ${t.titulo} ${t.rama_asociada ?? ''}`).includes(q)) return;
      let cur: string | null = t.id;
      while (cur) {
        set.add(cur);
        cur = padreDe.get(cur) ?? null;
      }
    });
    return set;
  }, [q, porId, padreDe]);
  const ver = (t: Tarea) => !visibles || visibles.has(t.id);
  const alternar = (id: string) => onAbrir(abierta === id ? null : id);

  // Cada rama es una caja con su encabezado (click: historia y commits) y, dentro, sus hijas.
  const cajaTarea = (t: Tarea, nivel: number): ReactNode => {
    const m = estadoMeta[t.estado];
    const lista = commitsDe.get(t.id) ?? [];
    const fusionadas = hijasFusionadas(t.id);
    const padre = padreDe.get(t.id) ? porId.get(padreDe.get(t.id)!) : null;
    // Cajas hijas: solo ramas abiertas. Las fusionadas van dentro de su historial (abajo).
    const internas = (hijas.get(t.id) ?? []).filter((h) => h.estado !== 'terminada' && ver(h));
    return (
      <Caja
        key={t.id}
        id={t.id}
        nombre={t.id}
        titulo={t.titulo}
        color={m.dot}
        gris={t.estado === 'terminada'}
        nivel={nivel}
        detalle={`${lista.length} commit${lista.length === 1 ? '' : 's'}`}
        abierta={abierta === t.id || fusionadas.some((h) => h.id === abierta)}
        onClick={() => alternar(t.id)}
        historial={
          <>
            <Seccion titulo="Historia">
              <Evento texto={`Rama ${t.rama_asociada}`} />
              <Evento texto={`${t.id} sale de ${padre ? padre.id : 'dev'}`} />
              {t.tarea_origen_id && <Evento texto={`${t.id} corrige ${t.tarea_origen_id}`} />}
              {fusionadas.map((h) => (
                <Fusionada key={h.id} tarea={h} destino={t.id} commits={commitsDe.get(h.id) ?? []} usuarios={usuarios} enfocada={abierta === h.id} />
              ))}
              <Evento
                texto={t.estado === 'terminada' ? `${t.id} se fusionó en ${padre ? padre.id : 'dev'}` : `${t.id} ahora: ${m.label}`}
                detalle={t.estado === 'terminada' ? t.fecha_completada ?? undefined : undefined}
                color={m.dot}
              />
            </Seccion>
            <Seccion titulo="Commits">
              <LineaCommits commits={lista} color={t.estado === 'terminada' ? '#64748b' : m.dot} usuarios={usuarios} />
            </Seccion>
          </>
        }
      >
        {internas.map((h) => cajaTarea(h, nivel + 1))}
      </Caja>
    );
  };

  const raices = (hijas.get(null) ?? []).filter((t) => t.estado !== 'terminada' && ver(t));
  return (
    <div className="h-full overflow-y-auto px-4 pb-4 pt-16">
      {visibles && visibles.size === 0 ? (
        <div className="px-2 py-6 text-center text-xs text-faint">Sin resultados</div>
      ) : (
        // main guarda todo (directa o indirectamente): dentro va dev, y dentro de dev las ramas.
        <Caja
          id="main"
          nombre="main"
          color="#60a5fa"
          nivel={0}
          abierta={abierta === 'main'}
          onClick={() => alternar('main')}
          historial={
            <Seccion titulo="Historia">
              <Evento texto="main" detalle="antes del PR #17" />
              <Evento texto="dev fusionado en main" detalle="PR #17" />
            </Seccion>
          }
        >
          <Caja
            id="dev"
            nombre="dev"
            color="#2dd4bf"
            nivel={1}
            detalle={`${mergesDev.length} fusionadas`}
            abierta={abierta === 'dev' || mergesDev.some((t) => t.id === abierta)}
            onClick={() => alternar('dev')}
            historial={
              <Seccion titulo="Historia">
                <Evento texto="dev nace desde main" />
                {mergesDev.map((t) => (
                  <Fusionada key={t.id} tarea={t} destino="dev" commits={commitsDe.get(t.id) ?? []} usuarios={usuarios} enfocada={abierta === t.id} />
                ))}
              </Seccion>
            }
          >
            {raices.map((t) => cajaTarea(t, 2))}
          </Caja>
        </Caja>
      )}
    </div>
  );
}

// Caja de una rama: encabezado (siempre visible), su historial al desplegarla y, dentro, sus
// hijas. Los niveles alternan dos tonos de fondo para que se lea qué está dentro de qué.
function Caja({ id, nombre, titulo, color, gris, nivel, detalle, abierta, onClick, historial, children }: {
  id: string;
  nombre: string;
  titulo?: string;
  color: string;
  gris?: boolean;
  nivel: number;
  detalle?: string;
  abierta: boolean;
  onClick: () => void;
  historial: ReactNode;
  children?: ReactNode;
}) {
  const tieneHijas = Array.isArray(children) ? children.length > 0 : !!children;
  return (
    <div
      id={`historial-${id}`}
      className={`rounded-lg border p-1.5 ${nivel % 2 === 0 ? 'bg-surface-2' : 'bg-surface'}`}
      style={{ borderColor: gris ? '#2b313f' : `${color}55` }}
    >
      <button
        type="button"
        onClick={onClick}
        className={`flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-xs transition hover:bg-surface-3 ${
          abierta ? 'bg-surface-3' : ''
        }`}
      >
        <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: gris ? '#475063' : color }} />
        <span className={`shrink-0 font-mono font-semibold ${gris ? 'text-faint' : 'text-text'}`}>{nombre}</span>
        {titulo ? (
          <span className={`min-w-0 flex-1 truncate ${gris ? 'text-faint' : 'text-muted'}`}>{titulo}</span>
        ) : (
          <span className="flex-1" />
        )}
        {detalle && <span className="shrink-0 text-faint">{detalle}</span>}
        <span className="shrink-0 text-faint">{abierta ? '▾' : '▸'}</span>
      </button>
      {abierta && <div className="mx-1 mb-1.5 mt-1 space-y-3 rounded-md border border-border bg-surface px-4 py-3">{historial}</div>}
      {tieneHijas && <div className="space-y-1.5 pb-0.5 pl-4 pr-0.5 pt-1">{children}</div>}
    </div>
  );
}

// Rama ya fusionada, dentro del historial de su padre (no es una caja: ya no es una rama
// abierta). En gris; click despliega sus commits ahí mismo.
function Fusionada({ tarea, destino, commits, usuarios, enfocada }: {
  tarea: Tarea;
  destino: string;
  commits: Commit[];
  usuarios: Usuario[];
  enfocada: boolean;
}) {
  const [abierta, setAbierta] = useState(enfocada);
  const ver = abierta || enfocada;
  return (
    <div id={`historial-${tarea.id}`} className={`rounded-md ${enfocada ? 'bg-surface-2' : ''}`}>
      <button
        type="button"
        onClick={() => setAbierta((v) => !v)}
        className="flex w-full items-baseline gap-2 rounded-md px-1 py-0.5 text-left text-xs transition hover:bg-surface-2"
      >
        <span className="shrink-0 text-[#475063]">●</span>
        <span className="shrink-0 font-mono text-faint">{tarea.id}</span>
        <span className="min-w-0 flex-1 truncate text-faint">{tarea.titulo}</span>
        <span className="shrink-0 text-faint">
          se fusionó en {destino} · <span className="font-mono">{tarea.fecha_completada ?? ''}</span>
        </span>
        <span className="shrink-0 text-faint">
          {commits.length} commit{commits.length === 1 ? '' : 's'} {ver ? '▾' : '▸'}
        </span>
      </button>
      {ver && (
        <div className="mb-2 ml-4 mt-1">
          <LineaCommits commits={commits} color="#64748b" usuarios={usuarios} />
        </div>
      )}
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
  const tareas = useStore((s) => s.tareas);
  // Si el cambio viene de una subtarea con check-in directo, se marca como check-in de esa tarea.
  const checkinDe = (c: Commit) => {
    const t = tareas.find((x) => x.id === c.tarea_id);
    return t && esCheckinTarea(t) ? t.id : null;
  };
  // Click en cualquier parte de un commit abre sus detalles (como las tareas en el Scrum).
  const [seleccionado, setSeleccionado] = useState<number | null>(null);
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
                <div
                  key={c.id}
                  onClick={() => setSeleccionado(commits.indexOf(c))}
                  className="group relative flex cursor-pointer items-center gap-3 rounded-md py-1.5 pr-2 transition hover:bg-surface-2"
                >
                  <span className="absolute -left-[21px] top-1/2 z-10 h-2 w-2 -translate-y-1/2 rounded-full" style={{ background: color }} />
                  <span
                    className="grid h-6 w-6 shrink-0 place-items-center rounded-full text-[10px] font-semibold text-white"
                    style={{ background: avatarColor(c.autor) }}
                    title={nombre}
                  >
                    {u ? iniciales(u.nombre, u.apellido) : c.autor.slice(0, 2).toUpperCase()}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="truncate text-xs text-text">{c.mensaje}</span>
                      {checkinDe(c) && (
                        <span className="shrink-0 rounded border border-border bg-surface-3 px-1.5 py-px text-[10px] text-muted">
                          check-in · {checkinDe(c)}
                        </span>
                      )}
                    </div>
                    <div className="text-[11px] text-faint" title={c.fecha.replace('T', ' ')}>
                      {nombre} · {haceCuanto(c.fecha)} · {c.fecha.slice(11, 16)}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation(); // la pastilla copia; no abre los detalles
                      copiar(c.hash);
                    }}
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

      {seleccionado !== null && commits[seleccionado] && (
        <DetalleCommit
          commit={commits[seleccionado]}
          posicion={commits.length - seleccionado}
          total={commits.length}
          usuario={usuario(commits[seleccionado].autor)}
          onCerrar={() => setSeleccionado(null)}
          onCopiar={copiar}
          // la lista va del más reciente al más antiguo: "anterior" en el tiempo = índice mayor
          onAnterior={seleccionado < commits.length - 1 ? () => setSeleccionado(seleccionado + 1) : undefined}
          onSiguiente={seleccionado > 0 ? () => setSeleccionado(seleccionado - 1) : undefined}
        />
      )}
    </div>
  );
}

// Detalles de un commit: mensaje, rama/tarea, autor, fecha exacta y hash; con anterior/siguiente
// para recorrer los commits de la rama sin cerrar.
export function DetalleCommit({ commit, posicion, total, usuario, onCerrar, onCopiar, onAnterior, onSiguiente }: {
  commit: Commit;
  posicion: number;
  total: number;
  usuario?: Usuario;
  onCerrar: () => void;
  onCopiar: (hash: string) => void;
  onAnterior?: () => void;
  onSiguiente?: () => void;
}) {
  const tarea = useStore((s) => s.tareas.find((t) => t.id === commit.tarea_id));
  const nombre = usuario ? `${usuario.nombre} ${usuario.apellido}` : commit.autor;
  const [d, h] = commit.fecha.split('T');
  const fechaLarga = `${Number(d.slice(8, 10))} ${MESES[Number(d.slice(5, 7)) - 1]} ${d.slice(0, 4)}, ${(h ?? '').slice(0, 5)}`;
  return (
    <Modal
      open
      onClose={onCerrar}
      title={commit.mensaje}
      subtitle={
        tarea && esCheckinTarea(tarea)
          ? `Check-in de ${tarea.id} sobre la rama de ${tarea.tarea_padre_id ?? '—'}`
          : `Commit ${posicion} de ${total} en esta rama`
      }
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <button type="button" className="btn-ghost btn-sm" onClick={onAnterior} disabled={!onAnterior}>
            ← Anterior
          </button>
          <button type="button" className="btn-ghost btn-sm" onClick={onSiguiente} disabled={!onSiguiente}>
            Siguiente →
          </button>
        </div>
      }
    >
      <div className="space-y-3 text-sm">
        {tarea && (
          <FilaDetalle k="Rama">
            <span className="font-mono text-text">{tarea.rama_asociada}</span>
            <span className="ml-2 text-muted">
              {tarea.id} · {tarea.titulo}
            </span>
          </FilaDetalle>
        )}
        <FilaDetalle k="Autor">
          <span className="inline-flex items-center gap-2">
            <span
              className="grid h-6 w-6 place-items-center rounded-full text-[10px] font-semibold text-white"
              style={{ background: avatarColor(commit.autor) }}
            >
              {usuario ? iniciales(usuario.nombre, usuario.apellido) : commit.autor.slice(0, 2).toUpperCase()}
            </span>
            <span className="text-text">{nombre}</span>
            {usuario && <span className="text-faint">· {usuario.rol}</span>}
          </span>
        </FilaDetalle>
        <FilaDetalle k="Fecha">
          <span className="text-text">{fechaLarga}</span>
          <span className="ml-2 text-faint">{haceCuanto(commit.fecha)}</span>
        </FilaDetalle>
        <FilaDetalle k="Hash">
          <span className="font-mono text-text">{commit.hash}</span>
          <button
            type="button"
            onClick={() => onCopiar(commit.hash)}
            className="ml-2 rounded-md border border-border bg-surface-3 px-2 py-0.5 text-[11px] text-muted transition hover:border-border-strong hover:text-text"
          >
            Copiar
          </button>
        </FilaDetalle>
      </div>
    </Modal>
  );
}

function FilaDetalle({ k, children }: { k: string; children: ReactNode }) {
  return (
    <div className="flex gap-3">
      <span className="w-14 shrink-0 text-faint">{k}</span>
      <span className="min-w-0 flex-1">{children}</span>
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
