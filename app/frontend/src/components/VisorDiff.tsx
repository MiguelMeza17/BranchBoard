import { useEffect, useState } from 'react';
import { api, mensajeDeError } from '../lib/api';
import type { ArchivoDiff, Diferencias } from '../lib/diff';

/** Solo lectura. Ninguna llamada cambia el workspace ni abre Unity. */
export function VisorDiff({ tareaId, hash }: { tareaId: string; hash?: string }) {
  const [lista, setLista] = useState<Diferencias | null>(null);
  const [ruta, setRuta] = useState<string | null>(null);
  const [archivo, setArchivo] = useState<ArchivoDiff | null>(null);
  const [errorLista, setErrorLista] = useState<string | null>(null);
  const [errorArchivo, setErrorArchivo] = useState<string | null>(null);
  const [intento, setIntento] = useState(0);

  useEffect(() => {
    let vigente = true;
    setLista(null); setRuta(null); setArchivo(null); setErrorLista(null); setErrorArchivo(null);
    const pedido = hash === undefined ? api.diffRama(tareaId) : api.diffChangeset(tareaId, hash);
    void pedido.then((datos) => {
      if (!vigente) return;
      setLista(datos); setRuta(datos.archivos[0]?.ruta ?? null);
    }).catch((e) => { if (vigente) setErrorLista(mensajeDeError(e)); });
    return () => { vigente = false; };
  }, [tareaId, hash, intento]);

  useEffect(() => {
    let vigente = true;
    setArchivo(null); setErrorArchivo(null);
    const elegido = lista?.archivos.find((a) => a.ruta === ruta);
    if (!elegido) return;
    if (elegido.binario || elegido.aviso || elegido.lineas !== null) {
      setArchivo(elegido);
      return;
    }
    void api.diffArchivo(tareaId, elegido.ruta, hash).then((datos) => {
      if (vigente) setArchivo(datos);
    }).catch((e) => { if (vigente) setErrorArchivo(mensajeDeError(e)); });
    return () => { vigente = false; };
  }, [lista, ruta, tareaId, hash]);

  const error = errorLista ?? errorArchivo;
  return (
    <section aria-label={hash ? `Diferencias del check-in ${hash}` : 'Diferencias de la rama'} className="min-w-0 space-y-2">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-text">{hash ? `Diferencias · cs:${hash}` : 'Diferencias de la rama'}</h3>
        <button type="button" className="btn-ghost btn-sm" onClick={() => setIntento((n) => n + 1)}>Recargar</button>
      </div>
      {error && <p role="alert" className="rounded-lg border border-danger/40 bg-danger/10 p-3 text-xs text-red-300">{error}</p>}
      {!lista && !errorLista && <p role="status" className="text-xs text-muted">Cargando diferencias…</p>}
      {lista?.archivos.length === 0 && <p className="text-xs text-muted">Sin archivos cambiados.</p>}
      {!!lista?.archivos.length && (
        <div className="grid min-w-0 overflow-hidden rounded-lg border border-border md:grid-cols-[minmax(10rem,1fr)_minmax(0,3fr)]">
          <nav aria-label="Archivos cambiados" className="max-h-80 overflow-auto border-b border-border bg-surface-2 md:border-b-0 md:border-r">
            {lista.archivos.map((a) => (
              <button type="button" key={a.ruta} aria-pressed={ruta === a.ruta} onClick={() => setRuta(a.ruta)}
                className={`block w-full px-3 py-2 text-left text-xs transition ${ruta === a.ruta ? 'bg-primary-soft text-primary' : 'text-text hover:bg-surface-3'}`}>
                <span className="block break-all font-mono">{a.ruta}</span>
                <span className="text-[10px] text-muted">{a.estado}{a.binario ? ' · binario' : ''}</span>
              </button>
            ))}
          </nav>
          <div className="min-w-0 bg-surface">
            {!archivo && !errorArchivo && <p role="status" className="p-3 text-xs text-muted">Cargando archivo…</p>}
            {archivo && (
              <>
                {archivo.ruta_anterior && archivo.ruta_anterior !== archivo.ruta && <p className="p-2 font-mono text-xs text-muted">{archivo.ruta_anterior} → {archivo.ruta}</p>}
                {archivo.binario ? <p className="p-4 text-xs text-muted">Archivo binario cambiado.</p>
                  : archivo.aviso ? <p className="p-4 text-xs text-muted">{archivo.aviso}</p>
                  : archivo.lineas?.length === 0 ? <p className="p-4 text-xs text-muted">Sin diferencias de texto.</p>
                  : <div className="max-h-96 overflow-auto" tabIndex={0} aria-label="Líneas cambiadas">
                    <table className="w-full border-collapse whitespace-pre font-mono text-[11px] leading-5"><tbody>
                      {archivo.lineas?.map((l, i) => (
                        <tr key={i} className={l.tipo === 'agregada' ? 'bg-green-500/10 text-green-300' : l.tipo === 'borrada' ? 'bg-red-500/10 text-red-300' : 'text-muted'}>
                          <td className="select-none px-2 text-right text-faint">{l.anterior ?? ''}</td>
                          <td className="select-none px-2 text-right text-faint">{l.nueva ?? ''}</td>
                          <td className="select-none px-1">{l.tipo === 'agregada' ? '+' : l.tipo === 'borrada' ? '−' : ' '}</td>
                          <td className="w-full pr-3">{l.texto.replace(/\r?\n$/, '') || ' '}</td>
                        </tr>
                      ))}
                    </tbody></table>
                  </div>}
              </>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
