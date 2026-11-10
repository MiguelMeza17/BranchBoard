import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useStore } from '../store/useStore';
import {
  esLiderOAdmin,
  puedeCrearUsuarios,
  puedeEliminarUsuario,
  puedeConfirmarUsuario,
  rolesAsignablesA,
} from '../lib/permisos';
import { ROLES, type Rol } from '../types';
import { PageHeader, Card, Avatar } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { Seccion } from '../components/Seccion';

// Usuarios del proyecto. Todos ven la lista (nombre y rol) y pueden entrar al perfil de cada uno.
// Líder y Administrador además ven el correo, crean usuarios, cambian roles, confirman y eliminan,
// con las restricciones de lib/permisos.ts.
export function Usuarios() {
  const user = useStore((s) => s.currentUser());
  const gestiona = esLiderOAdmin(user.rol);

  return (
    <div>
      <PageHeader
        title="Usuarios"
        desc={gestiona ? 'Quiénes están en el proyecto, con qué rol, y altas o cambios de rol.' : 'Quiénes están en el proyecto y con qué rol. Entra a un perfil para ver sus tareas.'}
      />
      <div className="space-y-3">
        {puedeCrearUsuarios(user.rol) && <SeccionCrearUsuario />}
        <ListaUsuarios gestiona={gestiona} />
      </div>
    </div>
  );
}

function SeccionCrearUsuario() {
  const user = useStore((s) => s.currentUser());
  const crearUsuario = useStore((s) => s.crearUsuario);

  const [correo, setCorreo] = useState('');
  const [rol, setRol] = useState<Rol>('Desarrollador');
  const correoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(correo.trim());
  // Un Líder no puede crear Líderes ni Administradores.
  const rolesAsignables = ROLES.filter((r) => (user.rol === 'Líder' ? r !== 'Líder' && r !== 'Administrador' : true));

  return (
    <Seccion title="Crear usuario">
      <div className="flex flex-wrap items-end gap-2">
        <div className="min-w-[14rem] flex-1">
          <label className="label">Correo</label>
          <input
            className="input"
            type="email"
            placeholder="nombre@correo.com"
            value={correo}
            onChange={(e) => setCorreo(e.target.value)}
          />
        </div>
        <div>
          <label className="label">Rol</label>
          <select className="input" value={rol} onChange={(e) => setRol(e.target.value as Rol)}>
            {rolesAsignables.map((r) => <option key={r} value={r}>{r}</option>)}
          </select>
        </div>
        <button
          className="btn-primary"
          disabled={!correoOk}
          onClick={() => {
            if (crearUsuario(correo, rol)) {
              setCorreo('');
            }
          }}
        >
          Crear usuario
        </button>
      </div>
    </Seccion>
  );
}

function ListaUsuarios({ gestiona }: { gestiona: boolean }) {
  const user = useStore((s) => s.currentUser());
  const usuarios = useStore((s) => s.usuarios);
  const eliminar = useStore((s) => s.eliminarUsuario);
  const cambiarRol = useStore((s) => s.cambiarRol);
  const confirmar = useStore((s) => s.confirmarUsuario);
  const navigate = useNavigate();
  const [target, setTarget] = useState<string | null>(null);
  const verPerfil = (email: string) => navigate(`/perfil/${encodeURIComponent(email)}`);

  return (
    <Card className="p-0">
      <div className="acotado overflow-x-auto">
        <table className="w-full min-w-[560px] text-sm">
          <thead className="sticky top-0 z-10 bg-surface">
            <tr className="border-b border-border text-left text-xs text-faint">
              <th className="px-4 py-2.5 font-medium">Usuario</th>
              {gestiona && <th className="px-4 py-2.5 font-medium">Correo</th>}
              <th className="px-4 py-2.5 font-medium">Rol</th>
              {gestiona && <th className="px-4 py-2.5 text-right font-medium">Acciones</th>}
            </tr>
          </thead>
          <tbody>
            {usuarios.map((u) => {
              const pendienteConfirmar = u.firma === null;
              const rolesPosibles = !gestiona || u.email === user.email ? [] : rolesAsignablesA(user.rol, u.rol);
              return (
                <tr key={u.email} className="border-b border-border/50 last:border-0">
                  <td className="px-4 py-2.5">
                    <div className="flex items-center gap-2">
                      <Avatar email={u.email} size={26} />
                      <button
                        type="button"
                        className="text-left text-text hover:text-primary hover:underline"
                        onClick={() => verPerfil(u.email)}
                        title={`Ver el perfil de ${u.nombre}`}
                      >
                        {u.nombre} {u.apellido}
                      </button>
                      {u.email === user.email && <span className="text-[11px] text-faint">(tú)</span>}
                      {gestiona && pendienteConfirmar && (
                        <span className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300">Pendiente de confirmar</span>
                      )}
                    </div>
                  </td>
                  {gestiona && <td className="px-4 py-2.5 font-mono text-xs text-faint">{u.email}</td>}
                  <td className="px-4 py-2.5 text-muted">
                    {rolesPosibles.length > 0 ? (
                      <select
                        className="input h-8 w-auto py-0 text-sm"
                        value={u.rol}
                        aria-label={`Rol de ${u.nombre}`}
                        onChange={(e) => cambiarRol(u.email, e.target.value as Rol)}
                      >
                        {rolesPosibles.map((r) => <option key={r} value={r}>{r}</option>)}
                      </select>
                    ) : (
                      u.rol
                    )}
                  </td>
                  {gestiona && (
                    <td className="px-4 py-2.5">
                      <div className="flex items-center justify-end gap-2">
                        {pendienteConfirmar && puedeConfirmarUsuario(user.rol) && (
                          <button className="btn-primary btn-sm" onClick={() => confirmar(u.email)}>Confirmar</button>
                        )}
                        {u.email !== user.email && puedeEliminarUsuario(user.rol, u.rol) && (
                          <button className="btn-danger btn-sm" onClick={() => setTarget(u.email)}>Eliminar</button>
                        )}
                      </div>
                    </td>
                  )}
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
        description={`Se revocará el acceso de ${target} a BranchBoard de inmediato. Recuerda quitarle también el acceso a la carpeta de Drive/OneDrive del proyecto: BranchBoard no puede hacerlo por ti.`}
      />
    </Card>
  );
}
