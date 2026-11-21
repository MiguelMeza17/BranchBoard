import { useEffect, useState } from 'react';
import { useStore } from '../store/useStore';
import { api, enTauri, mensajeDeError, type ResumenSubida } from '../lib/api';
import { Seccion } from './Seccion';
import { Modal } from './Modal';

/**
 * "Servidor" en Ajustes (solo Administrador, #245): sube al servidor un proyecto que todavía no está
 * allí, con su equipo, sus tareas y sus documentos. Desde ahí sigue sincronizándose por el servidor.
 * No se muestra si el proyecto ya está en el servidor ni fuera de la app de escritorio.
 */
export function SeccionSubirAlServidor() {
  const showToast = useStore((s) => s.showToast);
  const subirVersionDatos = useStore((s) => s.subirVersionDatos);
  const [enServidor, setEnServidor] = useState<boolean | null>(null);
  const [confirmar, setConfirmar] = useState(false);
  const [subiendo, setSubiendo] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enTauri()) return;
    api
      .proyectoEnElServidor()
      .then((id) => setEnServidor(Boolean(id)))
      .catch(() => setEnServidor(null));
  }, []);

  if (!enTauri() || enServidor !== false) return null;

  const subir = async () => {
    setSubiendo(true);
    setError(null);
    try {
      const r: ResumenSubida = await api.subirProyectoAlServidor();
      setConfirmar(false);
      setEnServidor(true);
      subirVersionDatos();
      const partes = [`${r.miembros} del equipo agregados`];
      if (r.invitados) partes.push(`${r.invitados} invitados (entran al crear su cuenta)`);
      if (r.documentos_sin_archivo) partes.push(`${r.documentos_sin_archivo} documentos sin archivo en este equipo`);
      showToast(`Proyecto subido al servidor: ${partes.join(', ')}`);
    } catch (e) {
      setError(mensajeDeError(e));
    } finally {
      setSubiendo(false);
    }
  };

  return (
    <Seccion title="Servidor">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 text-sm text-muted">Este proyecto todavía no está en el servidor.</div>
        <button className="btn-primary btn-sm" onClick={() => setConfirmar(true)}>
          Subir este proyecto al servidor
        </button>
      </div>
      <Modal
        open={confirmar}
        onClose={() => !subiendo && setConfirmar(false)}
        title="Subir este proyecto al servidor"
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button className="btn-ghost btn-sm" onClick={() => setConfirmar(false)} disabled={subiendo}>
              Cancelar
            </button>
            <button className="btn-primary btn-sm" onClick={() => void subir()} disabled={subiendo}>
              {subiendo ? 'Subiendo…' : 'Subir'}
            </button>
          </div>
        }
      >
        <div className="space-y-2 text-sm text-muted">
          <p>Se suben las tareas, los sprints, el chat, los documentos y el equipo con sus roles, tal como están en este equipo.</p>
          <p>Si alguien ya subió este proyecto, no se mezcla nada: pídele acceso a uno de sus Administradores.</p>
          {error && <p className="text-[12px] text-red-300">{error}</p>}
        </div>
      </Modal>
    </Seccion>
  );
}
