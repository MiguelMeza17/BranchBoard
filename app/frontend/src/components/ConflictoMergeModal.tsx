import { useState } from 'react';
import { useStore } from '../store/useStore';
import { Modal } from './Modal';
import { ConfirmLock } from './ConfirmLock';

/** Común a Revisión y a las autorizaciones desde los tableros (#107). */
export function ConflictoMergeModal() {
  const conflicto = useStore((s) => s.conflictoMerge);
  const cerrar = useStore((s) => s.cerrarConflictoMerge);
  const rechazar = useStore((s) => s.rechazarRevision);
  const resolverEnUnityVc = useStore((s) => s.resolverConflictoEnUnityVc);
  const [confirmarRechazo, setConfirmarRechazo] = useState(false);
  const [abriendo, setAbriendo] = useState(false);
  const tarea = useStore((s) => s.tareas.find((t) => t.id === conflicto?.tareaId));
  if (!conflicto) return null;
  const onClose = () => { setConfirmarRechazo(false); cerrar(); };
  return (
    <>
      <Modal open={!confirmarRechazo} onClose={onClose} title="Conflicto al autorizar merge"
        footer={<>
          <button className="btn-outline" disabled={abriendo} onClick={() => {
            setAbriendo(true);
            void resolverEnUnityVc().finally(() => setAbriendo(false));
          }}>{abriendo ? 'Abriendo Unity VC…' : 'Resolver en Unity VC'}</button>
          {tarea?.estado === 'en revisión' && <button className="btn-danger" onClick={() => setConfirmarRechazo(true)}>Enviar a corregir</button>}
        </>}>
        <p className="text-sm text-muted">{conflicto.mensaje}</p>
        <p className="mt-3 break-all text-sm">{conflicto.origen} → {conflicto.destino}</p>
        <ul className="mt-3 list-disc pl-5 text-sm">{conflicto.archivos.map((ruta) => <li key={ruta}>{ruta}</li>)}</ul>
      </Modal>
      <ConfirmLock open={confirmarRechazo} onClose={() => setConfirmarRechazo(false)}
        title="Enviar a corregir" palabra="rechazar" requiereJustificacion
        description="La tarea volverá a producción y el responsable recibirá el motivo del rechazo."
        onConfirm={(motivo) => {
          void rechazar(conflicto.tareaId, `Falló el merge por conflicto. ${motivo ?? ''}`).then((ok) => {
            if (ok) setConfirmarRechazo(false);
          });
        }} />
    </>
  );
}
