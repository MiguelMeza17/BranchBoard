import { useEffect, useState } from 'react';
import { api, mensajeDeError, type LogsUnity } from '../lib/api';
import type { Tarea } from '../types';
import { ConfirmLock } from './ConfirmLock';
import { Modal } from './Modal';

/** Los logs informan; solo la confirmación explícita permite ejecutar el merge. */
export function TerminarRevisionModal({ tarea, onClose, onConfirm }: {
  tarea: Tarea;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}) {
  const [logs, setLogs] = useState<LogsUnity | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);
  const [leyendo, setLeyendo] = useState(true);
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelado = false;
    setLeyendo(true);
    setLogs(null);
    setAviso(null);
    void api.logsUnityParaTerminar(tarea.id)
      .then((resultado) => { if (!cancelado) { setLogs(resultado); setAviso(resultado.aviso); } })
      .catch((e) => { if (!cancelado) setAviso(mensajeDeError(e)); })
      .finally(() => { if (!cancelado) setLeyendo(false); });
    return () => { cancelado = true; };
  }, [tarea.id]);

  if (leyendo) {
    return (
      <Modal open onClose={onClose} title="Terminar tarea" footer={<button className="btn-ghost" onClick={onClose}>Cancelar</button>}>
        <p role="status" className="text-sm text-muted">Revisando logs locales de Unity…</p>
      </Modal>
    );
  }

  const hayRegistros = !!logs?.contenido || !!aviso;
  const confirmar = async () => {
    if (ocupado) return;
    setOcupado(true);
    setError(null);
    try { await onConfirm(); } catch (e) { setError(mensajeDeError(e)); }
    finally { setOcupado(false); }
  };

  return (
    <ConfirmLock
      open
      onClose={onClose}
      onConfirm={() => { void confirmar(); }}
      title="Terminar tarea"
      tone="primary"
      palabra="terminar"
      confirmLabel={hayRegistros ? 'Terminar igualmente' : 'Terminar'}
      ocupado={ocupado}
      error={error}
      description={
        <div className="space-y-3">
          <p>Vas a terminar «{tarea.titulo}» y hacer el merge en Unity Version Control.</p>
          {aviso && <p role="alert" className="rounded-lg border border-yellow-500/40 bg-yellow-500/10 p-3">{aviso}</p>}
          {logs?.contenido && (
            <div className="space-y-2">
              <div className="text-xs font-medium text-text">Logs de Unity de este equipo · {logs.archivo}</div>
              {logs.modificado && <div className="text-xs text-faint">Modificado: {new Date(logs.modificado).toLocaleString()}</div>}
              <p className="text-xs">Son registros locales de Unity; pueden corresponder a otra sesión o proyecto. La decisión de terminar es tuya.</p>
              {logs.recortado && <p className="text-xs text-faint">Se muestra el fragmento final del archivo.</p>}
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-surface-2 p-3 font-mono text-xs text-text">{logs.contenido}</pre>
            </div>
          )}
        </div>
      }
    />
  );
}
