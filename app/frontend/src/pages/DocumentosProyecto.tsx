import { useEffect, useRef, useState } from 'react';
import { useStore } from '../store/useStore';
import { esLiderOAdmin } from '../lib/permisos';
import type { Documento } from '../types';
import { Icono } from '../components/Icono';

type Vista = 'medianos' | 'pequenos' | 'lista';

const VISTAS: { id: Vista; label: string; icono: string }[] = [
  { id: 'medianos', label: 'Cuadros medianos', icono: 'cuadros' },
  { id: 'pequenos', label: 'Cuadros pequeños', icono: 'cuadrosPequenos' },
  { id: 'lista', label: 'Lista', icono: 'lista' },
];
const CLAVE_VISTA = 'bb-vista-documentos';

const icono = (nombre: string) => {
  const ext = nombre.split('.').pop()?.toLowerCase() ?? '';
  if (ext === 'pdf') return 'pdf';
  if (['doc', 'docx', 'odt', 'txt', 'md'].includes(ext)) return 'documento';
  if (['xls', 'xlsx', 'csv'].includes(ext)) return 'hoja';
  if (['ppt', 'pptx'].includes(ext)) return 'diapositivas';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(ext)) return 'imagen';
  if (['zip', 'rar', '7z'].includes(ext)) return 'comprimido';
  return 'clip';
};

const tamanoLegible = (bytes: number) =>
  bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;

// Apartado con el nombre del proyecto (estilo perfil de GitHub): a la izquierda el ícono grande del
// proyecto y su nombre; a la derecha sus documentos en cuadros (medianos, pequeños) o en lista.
// En el mockup los archivos viven en memoria; en la app real irían a la carpeta compartida.
export function DocumentosProyecto() {
  const proyecto = useStore((s) => s.proyectoActual);
  const documentos = useStore((s) => s.documentos);
  const usuarios = useStore((s) => s.usuarios);
  const user = useStore((s) => s.currentUser());
  const subir = useStore((s) => s.subirDocumentos);
  const eliminar = useStore((s) => s.eliminarDocumento);
  const aprobar = useStore((s) => s.aprobarDocumento);
  const abrirDoc = useStore((s) => s.abrirDocumento);
  const input = useRef<HTMLInputElement>(null);
  const [arrastrando, setArrastrando] = useState(false);
  const [vista, setVista] = useState<Vista>(() => {
    try {
      const v = localStorage.getItem(CLAVE_VISTA) as Vista | null;
      return v && VISTAS.some((x) => x.id === v) ? v : 'medianos';
    } catch {
      return 'medianos';
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(CLAVE_VISTA, vista);
    } catch {
      /* sin almacenamiento: no pasa nada */
    }
  }, [vista]);

  const nombreDe = (email: string) => {
    const u = usuarios.find((x) => x.email === email);
    return u ? `${u.nombre} ${u.apellido}`.trim() : email;
  };
  const agregar = (files: FileList | null) => {
    if (files && files.length) subir(Array.from(files));
  };
  // Abrir: todos, haciendo clic en cualquier parte del documento. Eliminar y dar el visto bueno: Líder/Administrador.
  const gestiona = esLiderOAdmin(user.rol);
  const abrir = (d: Documento) => abrirDoc(d.id);
  const pendiente = (d: Documento) =>
    d.pendiente ? (
      <span className="chip border border-amber-500/40 bg-amber-500/15 text-[10px] text-amber-300">Pendiente</span>
    ) : null;
  const cargar = () => input.current?.click();

  const grid =
    vista === 'medianos'
      ? 'grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4'
      : vista === 'pequenos'
        ? 'grid grid-cols-3 gap-2 sm:grid-cols-4 lg:grid-cols-6'
        : 'flex flex-col divide-y divide-border/60 rounded-xl border border-border bg-surface';

  return (
    <div
      className="grid gap-8 md:grid-cols-[240px_1fr]"
      onDragOver={(e) => {
        e.preventDefault();
        setArrastrando(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node)) setArrastrando(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setArrastrando(false);
        agregar(e.dataTransfer.files);
      }}
    >
      <input ref={input} type="file" multiple className="hidden" onChange={(e) => { agregar(e.target.files); e.target.value = ''; }} />

      {/* Columna izquierda: ícono grande y nombre del proyecto. */}
      <aside className="flex flex-col items-center gap-4 md:items-start">
        {/* Ícono del proyecto: círculo vacío hasta que exista uno. */}
        {/* Imagen del proyecto: el + la carga. Se implementa con el backend (FRONTEND.md → apartado del
            proyecto); en el mockup solo avisa. */}
        <button
          type="button"
          title="Cargar imagen del proyecto"
          aria-label="Cargar imagen del proyecto"
          onClick={() => useStore.getState().showToast('Cargar la imagen del proyecto llega con el backend', 'info')}
          className="group grid aspect-square w-48 place-items-center rounded-full border border-white/70 transition hover:border-white hover:bg-white/5 md:w-full"
        >
          <Icono nombre="mas" className="h-10 w-10 text-white/60 transition group-hover:text-white" sinColor />
        </button>
        <div className="text-center md:text-left">
          <h1 className="text-2xl font-bold text-text">{proyecto}</h1>
          <p className="mt-1 text-sm text-faint">
            {documentos.length} {documentos.length === 1 ? 'documento' : 'documentos'} · {usuarios.length} personas
          </p>
        </div>
      </aside>

      {/* Columna derecha: documentos. */}
      <section className="min-w-0">
        <div className="mb-3 flex items-center justify-between gap-2">
          <h2 className="text-sm font-semibold text-text">Documentos</h2>
          <div className="flex rounded-lg border border-border bg-surface-2 p-0.5" role="group" aria-label="Cómo mostrar">
            {VISTAS.map((v) => (
              <button
                key={v.id}
                type="button"
                title={v.label}
                aria-label={v.label}
                aria-pressed={vista === v.id}
                onClick={() => setVista(v.id)}
                className={`grid h-7 w-8 place-items-center rounded-md text-sm transition ${
                  vista === v.id ? 'bg-primary text-white' : 'text-muted hover:bg-surface-3 hover:text-text'
                }`}
              >
                <Icono nombre={v.icono} className="h-3.5 w-3.5" sinColor />
              </button>
            ))}
          </div>
        </div>

        <div className={`acotado p-1 ${grid} ${arrastrando ? 'rounded-xl ring-2 ring-primary ring-offset-4 ring-offset-bg' : ''}`}>
          {/* "Cargar documentos" con el mismo estilo que los documentos de la vista elegida. */}
          {vista === 'lista' ? (
            <button type="button" onClick={cargar} className="flex items-center gap-3 px-4 py-3 text-left text-sm text-primary transition hover:bg-surface-2">
              <span className="grid h-8 w-8 place-items-center rounded-lg border border-dashed border-primary/60"><Icono nombre="mas" className="h-4 w-4" /></span>
              Cargar documentos
            </button>
          ) : (
            <button
              type="button"
              onClick={cargar}
              className={`flex aspect-square flex-col items-center justify-center gap-1 rounded-xl border-2 border-dashed border-border text-primary transition hover:border-primary hover:bg-primary-soft ${
                vista === 'pequenos' ? 'text-xs' : 'text-sm'
              }`}
            >
              <Icono nombre="mas" className={vista === 'pequenos' ? 'h-6 w-6' : 'h-9 w-9'} />
              <span className="px-2 text-center font-medium">Cargar documentos</span>
            </button>
          )}

          {documentos.map((d) =>
            vista === 'lista' ? (
              <div
                key={d.id}
                role="button"
                tabIndex={0}
                onClick={() => abrir(d)}
                onKeyDown={(e) => { if (e.key === 'Enter') abrir(d); }}
                className="group flex cursor-pointer items-center gap-3 px-4 py-3 transition hover:bg-surface-2"
              >
                <span className="grid h-8 w-8 place-items-center"><Icono nombre={icono(d.nombre)} className="h-6 w-6" /></span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium text-text group-hover:text-primary">{d.nombre}</span>
                    {pendiente(d)}
                  </div>
                  <div className="text-xs text-faint">
                    {tamanoLegible(d.tamano)} · {nombreDe(d.subido_por)} · {d.fecha}
                  </div>
                </div>
                {gestiona && d.pendiente && (
                  <button className="btn-primary btn-sm" onClick={(e) => { e.stopPropagation(); aprobar(d.id); }}>
                    <Icono nombre="check" className="h-3.5 w-3.5" sinColor /> Visto bueno
                  </button>
                )}
                {gestiona && (
                  <button
                    className="btn-ghost btn-sm text-red-300 opacity-0 transition group-hover:opacity-100 focus:opacity-100"
                    onClick={(e) => { e.stopPropagation(); eliminar(d.id); }}
                  >
                    Eliminar
                  </button>
                )}
              </div>
            ) : (
              <div
                key={d.id}
                role="button"
                tabIndex={0}
                onClick={() => abrir(d)}
                onKeyDown={(e) => { if (e.key === 'Enter') abrir(d); }}
                className={`group relative flex aspect-square cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border bg-surface p-3 text-center transition hover:border-primary/60 hover:bg-surface-2 ${
                  d.pendiente ? `border-amber-500/50 ${gestiona ? 'pt-7 pb-11' : 'pt-7'}` : 'border-border'
                }`}
                title={`${d.nombre}\n${tamanoLegible(d.tamano)} · ${nombreDe(d.subido_por)} · ${d.fecha}`}
              >
                {d.pendiente && <span className="absolute left-1.5 top-1.5">{pendiente(d)}</span>}
                <Icono nombre={icono(d.nombre)} className={vista === 'pequenos' ? 'h-8 w-8' : d.pendiente ? 'h-10 w-10' : 'h-12 w-12'} />
                <span className={`line-clamp-2 w-full break-words font-medium text-text ${vista === 'pequenos' ? 'text-[11px]' : 'text-sm'}`}>
                  {d.nombre}
                </span>
                {vista === 'medianos' && <span className="text-[11px] text-faint">{tamanoLegible(d.tamano)}</span>}
                {gestiona && d.pendiente && (
                  <button
                    type="button"
                    className={`btn-primary absolute inset-x-2 bottom-2 justify-center ${vista === 'pequenos' ? 'px-1 py-0.5 text-[10px]' : 'btn-sm'}`}
                    onClick={(e) => { e.stopPropagation(); aprobar(d.id); }}
                  >
                    <Icono nombre="check" className="h-3.5 w-3.5" sinColor /> Visto bueno
                  </button>
                )}
                {gestiona && (
                  <button
                    type="button"
                    aria-label={`Eliminar ${d.nombre}`}
                    title="Eliminar"
                    onClick={(e) => { e.stopPropagation(); eliminar(d.id); }}
                    className="absolute right-1.5 top-1.5 grid h-6 w-6 place-items-center rounded-full bg-surface-3 text-xs text-red-300 opacity-0 transition hover:bg-red-500/20 group-hover:opacity-100 focus:opacity-100"
                  >
                    <Icono nombre="x" className="h-3 w-3" sinColor />
                  </button>
                )}
              </div>
            )
          )}
        </div>
      </section>
    </div>
  );
}
