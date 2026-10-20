import { useState } from 'react';
import { useStore } from '../store/useStore';
import { PROYECTO_NOMBRE } from '../data/fixtures';
import {
  esLiderOAdmin,
  puedeEliminarProyecto,
  puedeRestaurarBackup,
  puedeGestionarSolicitudes,
  puedeGestionarColaboradores,
  puedeEliminarUsuario,
  puedeConfirmarUsuario,
  puedeAsignarLider,
} from '../lib/permisos';
import { ROLES, type Rol } from '../types';
import { PageHeader, Card, Avatar, EmptyState } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { Modal } from '../components/Modal';

export function Ajustes() {
  const user = useStore((s) => s.currentUser());

  return (
    <div>
      <PageHeader title="Ajustes" desc="Acciones administrativas del proyecto. Cada sección aparece según tu rol." />
      <div className="space-y-6">
        {puedeGestionarSolicitudes(user.rol) && <SeccionSolicitudes />}
        {esLiderOAdmin(user.rol) && <SeccionUsuarios />}
        <SeccionBackups />
        {puedeGestionarColaboradores(user.rol) && <SeccionColaboradores />}
        {puedeEliminarProyecto(user.rol) && <SeccionPeligro />}
        {!esLiderOAdmin(user.rol) && (
          <EmptyState icon="🔒" title="Sin acciones administrativas" desc="Con tu rol actual no hay ajustes que gestionar aparte de reportar problemas de BD." />
        )}
      </div>
    </div>
  );
}

function Seccion({ title, children, danger }: { title: string; children: React.ReactNode; danger?: boolean }) {
  return (
    <Card className={danger ? 'border-danger/40' : ''}>
      <h2 className={`mb-4 text-sm font-semibold ${danger ? 'text-red-300' : 'text-text'}`}>{title}</h2>
      {children}
    </Card>
  );
}

function SeccionSolicitudes() {
  const user = useStore((s) => s.currentUser());
  const solicitudes = useStore((s) => s.solicitudes);
  const aceptar = useStore((s) => s.aceptarSolicitud);
  const rechazar = useStore((s) => s.rechazarSolicitud);
  const correoContacto = useStore((s) => s.correoContacto);
  const setCorreo = useStore((s) => s.setCorreoContacto);
  const showToast = useStore((s) => s.showToast);

  const [aceptando, setAceptando] = useState<string | null>(null);
  const [rol, setRol] = useState<Rol>('Desarrollador');
  const [historial, setHistorial] = useState(false);
  const [correoDraft, setCorreoDraft] = useState(correoContacto);

  const pendientes = solicitudes.filter((s) => s.estado === 'pendiente');
  const resueltas = solicitudes.filter((s) => s.estado !== 'pendiente');
  // Un Líder no puede asignar el rol Líder
  const rolesAsignables = ROLES.filter((r) => (user.rol === 'Líder' ? r !== 'Líder' && r !== 'Administrador' : true));

  return (
    <Seccion title="Solicitudes de ingreso">
      {pendientes.length === 0 ? (
        <p className="text-sm text-muted">No hay solicitudes pendientes.</p>
      ) : (
        <div className="space-y-2">
          {pendientes.map((s) => (
            <div key={s.id} className="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface-2 p-3">
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium text-text">{s.nombre} {s.apellido}</div>
                <div className="text-xs text-muted">{s.correo} · org: {s.id_organizacion} · {s.fecha_solicitud}</div>
              </div>
              <button className="btn-primary btn-sm" onClick={() => setAceptando(s.id)}>Aceptar</button>
              <button className="btn-danger btn-sm" onClick={() => rechazar(s.id)}>Rechazar</button>
            </div>
          ))}
        </div>
      )}

      <div className="mt-4 flex flex-wrap items-center gap-2">
        <button className="btn-outline btn-sm" onClick={() => showToast('Formulario de solicitud manual (mockup)', 'info')}>
          + Registrar solicitud manual
        </button>
        {resueltas.length > 0 && (
          <button className="btn-ghost btn-sm" onClick={() => setHistorial((v) => !v)}>
            {historial ? 'Ocultar' : 'Ver'} historial ({resueltas.length})
          </button>
        )}
      </div>

      {historial && (
        <div className="mt-3 space-y-1.5">
          {resueltas.map((s) => (
            <div key={s.id} className="flex items-center justify-between rounded-md border border-border/50 px-3 py-2 text-xs">
              <span className="text-muted">{s.nombre} {s.apellido} · {s.correo}</span>
              <span className={s.estado === 'aceptada' ? 'text-green-300' : 'text-red-300'}>{s.estado}</span>
            </div>
          ))}
        </div>
      )}

      {user.rol === 'Administrador' && (
        <div className="mt-5 border-t border-border pt-4">
          <label className="label">Correo de contacto para solicitudes (mailto)</label>
          <div className="flex gap-2">
            <input className="input max-w-sm" value={correoDraft} onChange={(e) => setCorreoDraft(e.target.value)} />
            <button className="btn-outline btn-sm" onClick={() => setCorreo(correoDraft)}>Guardar</button>
          </div>
        </div>
      )}

      {/* Modal aceptar con selector de rol */}
      <Modal
        open={!!aceptando}
        onClose={() => setAceptando(null)}
        title="Aceptar solicitud"
        subtitle="Se creará el usuario con firma pendiente (lo confirma el Administrador)."
        size="sm"
        footer={
          <>
            <button className="btn-ghost" onClick={() => setAceptando(null)}>Cancelar</button>
            <button className="btn-primary" onClick={() => { if (aceptando) { aceptar(aceptando, rol); showToast('Puedes usar "Avisar por correo" para notificar al solicitante', 'info'); } setAceptando(null); }}>
              Aceptar y crear usuario
            </button>
          </>
        }
      >
        <label className="label">Rol a asignar</label>
        <select className="input" value={rol} onChange={(e) => setRol(e.target.value as Rol)}>
          {rolesAsignables.map((r) => <option key={r} value={r}>{r}</option>)}
        </select>
        <button className="btn-outline btn-sm mt-4" onClick={() => showToast('Abriendo cliente de correo (mailto:)…', 'info')}>
          ✉️ Avisar por correo
        </button>
      </Modal>
    </Seccion>
  );
}

function SeccionUsuarios() {
  const user = useStore((s) => s.currentUser());
  const usuarios = useStore((s) => s.usuarios);
  const eliminar = useStore((s) => s.eliminarUsuario);
  const ascender = useStore((s) => s.ascenderALider);
  const confirmar = useStore((s) => s.confirmarUsuario);
  const [target, setTarget] = useState<string | null>(null);

  return (
    <Seccion title="Gestión de usuarios">
      <div className="overflow-hidden rounded-lg border border-border">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border text-left text-xs text-faint">
              <th className="px-4 py-2.5 font-medium">Usuario</th>
              <th className="px-4 py-2.5 font-medium">Correo</th>
              <th className="px-4 py-2.5 font-medium">Rol</th>
              <th className="px-4 py-2.5 text-right font-medium">Acciones</th>
            </tr>
          </thead>
          <tbody>
            {usuarios.map((u) => {
              const pendienteConfirmar = u.firma === null;
              const puedeAsc = puedeAsignarLider(user.rol) && (u.rol === 'Desarrollador' || u.rol === 'Tester');
              return (
                <tr key={u.email} className="border-b border-border/50">
                  <td className="px-4 py-2.5">
                    <div className="flex items-center gap-2">
                      <Avatar email={u.email} size={26} />
                      <span className="text-text">{u.nombre} {u.apellido}</span>
                      {pendienteConfirmar && (
                        <span className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300">Pendiente de confirmar</span>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-2.5 font-mono text-xs text-faint">{u.email}</td>
                  <td className="px-4 py-2.5 text-muted">{u.rol}</td>
                  <td className="px-4 py-2.5">
                    <div className="flex items-center justify-end gap-2">
                      {pendienteConfirmar && puedeConfirmarUsuario(user.rol) && (
                        <button className="btn-primary btn-sm" onClick={() => confirmar(u.email)}>Confirmar</button>
                      )}
                      {puedeAsc && (
                        <button className="btn-outline btn-sm" onClick={() => ascender(u.email)}>Ascender a Líder</button>
                      )}
                      {u.email !== user.email && puedeEliminarUsuario(user.rol, u.rol) && (
                        <button className="btn-danger btn-sm" onClick={() => setTarget(u.email)}>Eliminar</button>
                      )}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <ConfirmLock
        open={!!target}
        onClose={() => setTarget(null)}
        onConfirm={() => { if (target) eliminar(target); setTarget(null); }}
        title="Eliminar usuario"
        tone="danger"
        palabra="eliminar"
        confirmLabel="Eliminar del equipo"
        description={`Se revocará el acceso de ${target} de inmediato. Esta acción no cierra sesión — borra la fila del usuario.`}
      />
    </Seccion>
  );
}

function SeccionBackups() {
  const user = useStore((s) => s.currentUser());
  const backups = useStore((s) => s.backups);
  const showToast = useStore((s) => s.showToast);
  const [restaurar, setRestaurar] = useState<string | null>(null);
  const [reportar, setReportar] = useState(false);
  const [motivo, setMotivo] = useState('');

  if (puedeRestaurarBackup(user.rol)) {
    return (
      <Seccion title="Copias de seguridad">
        <div className="space-y-2">
          {backups.map((b) => (
            <div key={b.id} className="flex items-center justify-between rounded-lg border border-border bg-surface-2 px-3 py-2.5">
              <div className="text-sm">
                <span className="text-text">{b.fecha}</span>
                <span className="ml-2 text-xs text-faint">{b.tamano}</span>
              </div>
              <button className="btn-outline btn-sm" onClick={() => setRestaurar(b.id)}>Restaurar</button>
            </div>
          ))}
        </div>
        <ConfirmLock
          open={!!restaurar}
          onClose={() => setRestaurar(null)}
          onConfirm={() => { showToast('Restaurando backup…', 'info'); setRestaurar(null); }}
          title="Restaurar backup"
          tone="danger"
          palabra="restaurar"
          confirmLabel="Restaurar base de datos"
          description="Se reemplazará el estado actual de la base de datos por el del backup seleccionado."
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

function SeccionColaboradores() {
  const colaboradores = useStore((s) => s.colaboradores);
  const agregar = useStore((s) => s.agregarColaborador);
  const revocar = useStore((s) => s.revocarColaborador);
  const [email, setEmail] = useState('');
  const [nombre, setNombre] = useState('');

  return (
    <Seccion title="Permisos de subtareas simples">
      <p className="mb-3 text-sm text-muted">
        Colaboradores externos (Gluon) con permiso de subir subtareas simples de un solo archivo.
      </p>
      <div className="space-y-2">
        {colaboradores.map((c) => (
          <div key={c.email} className="flex items-center justify-between rounded-lg border border-border bg-surface-2 px-3 py-2.5">
            <div className="text-sm">
              <span className="text-text">{c.nombre}</span>
              <span className="ml-2 font-mono text-xs text-faint">{c.email}</span>
            </div>
            <button className="btn-danger btn-sm" onClick={() => revocar(c.email)}>Revocar</button>
          </div>
        ))}
        {colaboradores.length === 0 && <p className="text-sm text-faint">Ningún colaborador con permiso.</p>}
      </div>
      <div className="mt-4 flex flex-wrap items-end gap-2">
        <div><label className="label">Nombre</label><input className="input max-w-[180px]" value={nombre} onChange={(e) => setNombre(e.target.value)} /></div>
        <div><label className="label">Correo</label><input className="input max-w-[220px]" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="colab@correo.dev" /></div>
        <button className="btn-outline btn-sm" disabled={!email.trim() || !nombre.trim()} onClick={() => { agregar(email.trim(), nombre.trim()); setEmail(''); setNombre(''); }}>
          Agregar colaborador
        </button>
      </div>
    </Seccion>
  );
}

function SeccionPeligro() {
  const showToast = useStore((s) => s.showToast);
  const [open, setOpen] = useState(false);
  return (
    <Seccion title="⚠️ Zona de peligro" danger>
      <p className="text-sm text-muted">
        Eliminar el proyecto borra absolutamente todo. Requiere escribir el nombre exacto del proyecto y permiso <span className="font-mono">rmrepository</span> en Unity Version Control.
      </p>
      <button className="btn-danger mt-4" onClick={() => setOpen(true)}>Eliminar proyecto completo</button>
      <ConfirmLock
        open={open}
        onClose={() => setOpen(false)}
        onConfirm={() => { showToast('Proyecto eliminado (mockup)', 'danger'); setOpen(false); }}
        title="Eliminar proyecto completo"
        tone="danger"
        palabra={PROYECTO_NOMBRE}
        confirmLabel="Eliminar proyecto"
        description={<>Escribe el nombre exacto del proyecto <strong className="font-mono text-text">{PROYECTO_NOMBRE}</strong> para confirmar. Esta es la acción más destructiva de la app.</>}
      />
    </Seccion>
  );
}
