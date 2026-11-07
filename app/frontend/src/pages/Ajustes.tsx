import { useState } from 'react';
import { useStore } from '../store/useStore';
import {
  esLiderOAdmin,
  puedeEliminarProyecto,
  puedeRestaurarBackup,
} from '../lib/permisos';
import { PageHeader, EmptyState } from '../components/primitives';
import { Seccion } from '../components/Seccion';
import { ConfirmLock } from '../components/ConfirmLock';
import { Modal } from '../components/Modal';

export function Ajustes() {
  const user = useStore((s) => s.currentUser());

  return (
    <div>
      <PageHeader title="Ajustes" desc="Acciones administrativas del proyecto. Cada sección aparece según tu rol." />
      <div className="space-y-3">
        <SeccionBackups />
        {puedeEliminarProyecto(user.rol) && <SeccionProyecto />}
        {!esLiderOAdmin(user.rol) && (
          <EmptyState icon="🔒" title="Sin acciones administrativas" desc="Con tu rol actual no hay ajustes que gestionar aparte de reportar problemas de BD." />
        )}
      </div>
    </div>
  );
}

function SeccionBackups() {
  const user = useStore((s) => s.currentUser());
  const backups = useStore((s) => s.backups);
  const tamanoActual = useStore((s) => s.tamanoActual);
  const restaurarBackup = useStore((s) => s.restaurarBackup);
  const showToast = useStore((s) => s.showToast);
  // La fila "Actual" es la base de datos en uso: siempre arriba y con la fecha de hoy.
  const hoy = new Date().toLocaleDateString('en-CA'); // YYYY-MM-DD en hora local
  const [restaurar, setRestaurar] = useState<string | null>(null);
  const [reportar, setReportar] = useState(false);
  const [motivo, setMotivo] = useState('');
  const enRestaurar = backups.find((b) => b.id === restaurar);

  if (puedeRestaurarBackup(user.rol)) {
    return (
      <Seccion title="Copias de seguridad">
        <div className="space-y-2">
          <div className="flex items-center justify-between rounded-lg border border-primary/50 bg-primary-soft px-3 py-2.5">
            <div className="text-sm">
              <span className="text-text">{hoy}</span>
              <span className="ml-2 text-xs text-faint">{tamanoActual}</span>
            </div>
            <span className="chip border border-primary/40 bg-primary/15 text-primary">Actual</span>
          </div>
          {backups.map((b) => (
            <div key={b.id} className="flex items-center justify-between gap-3 rounded-lg border border-border bg-surface-2 px-3 py-2.5">
              <div className="min-w-0 text-sm">
                <span className="text-text">{b.fecha}</span>
                <span className="ml-2 text-xs text-faint">{b.tamano}</span>
                {b.nota && <div className="text-[11px] text-faint">{b.nota}</div>}
              </div>
              <button className="btn-outline btn-sm" onClick={() => setRestaurar(b.id)}>Restaurar</button>
            </div>
          ))}
        </div>
        <ConfirmLock
          open={!!restaurar}
          onClose={() => setRestaurar(null)}
          onConfirm={() => { if (restaurar) restaurarBackup(restaurar); setRestaurar(null); }}
          title="Restaurar backup"
          tone="danger"
          palabra="restaurar"
          confirmLabel="Restaurar base de datos"
          description={`La base de datos pasará a ser la de la copia del ${enRestaurar?.fecha ?? ''}. La actual se guarda antes como una copia más, tal cual está, así que puedes volver a ella.`}
        />
      </Seccion>
    );
  }

  // Líder / Desarrollador: reportar problema
  return (
    <Seccion title="Base de datos">
      <p className="text-sm text-muted">Solo el Administrador puede restaurar backups. Si detectas un problema, repórtalo.</p>
      <button className="btn-outline btn-sm mt-3" onClick={() => setReportar(true)}>Reportar problema con la base de datos</button>
      <Modal
        open={reportar}
        onClose={() => setReportar(false)}
        title="Reportar problema con la base de datos"
        size="sm"
        footer={
          <>
            <button className="btn-ghost" onClick={() => setReportar(false)}>Cancelar</button>
            <button className="btn-primary" disabled={motivo.trim().length < 3} onClick={() => { showToast('Reporte enviado a los Administradores'); setReportar(false); setMotivo(''); }}>Enviar</button>
          </>
        }
      >
        <label className="label">Motivo</label>
        <textarea className="input min-h-[90px] resize-y" value={motivo} onChange={(e) => setMotivo(e.target.value)} placeholder="Describe qué está pasando con la BD…" />
      </Modal>
    </Seccion>
  );
}

function SeccionProyecto() {
  const proyecto = useStore((s) => s.proyectoActual);
  const renombrar = useStore((s) => s.renombrarProyecto);
  const showToast = useStore((s) => s.showToast);
  const [nombre, setNombre] = useState(proyecto);
  const [open, setOpen] = useState(false);
  const cambio = nombre.trim() && nombre.trim() !== proyecto;

  return (
    <Seccion title="⚠️ Proyecto" danger>
      <label className="label">Nombre del proyecto</label>
      <div className="flex flex-wrap gap-2">
        <input className="input max-w-sm" value={nombre} onChange={(e) => setNombre(e.target.value)} />
        <button className="btn-outline btn-sm" disabled={!cambio} onClick={() => renombrar(nombre)}>
          Guardar
        </button>
      </div>

      <div className="mt-5 rounded-lg border border-danger/40 p-4">
        <div className="text-sm font-semibold text-red-300">Eliminar proyecto</div>
        <p className="mt-1 text-sm text-muted">
          Borra absolutamente todo. Requiere escribir el nombre exacto del proyecto y permiso{' '}
          <span className="font-mono">rmrepository</span> en Unity Version Control.
        </p>
        <button className="btn-danger mt-3" onClick={() => setOpen(true)}>Eliminar proyecto completo</button>
      </div>
      <ConfirmLock
        open={open}
        onClose={() => setOpen(false)}
        onConfirm={() => { showToast('Proyecto eliminado (mockup)', 'danger'); setOpen(false); }}
        title="Eliminar proyecto completo"
        tone="danger"
        palabra={proyecto}
        confirmLabel="Eliminar proyecto"
        description={<>Escribe el nombre exacto del proyecto <strong className="font-mono text-text">{proyecto}</strong> para confirmar. Esta es la acción más destructiva de la app.</>}
      />
    </Seccion>
  );
}
