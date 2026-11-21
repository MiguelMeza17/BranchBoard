import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useStore } from '../store/useStore';
import {
  esLiderOAdmin,
  puedeCrearUsuarios,
  puedeEliminarUsuario,
  rolesAsignablesA,
} from '../lib/permisos';
import { ROLES, type Rol } from '../types';
import { PageHeader, Card, Avatar } from '../components/primitives';
import { ConfirmLock } from '../components/ConfirmLock';
import { Seccion } from '../components/Seccion';
import { enTauri } from '../lib/api';

// Usuarios del proyecto. Todos ven la lista (nombre y rol) y pueden entrar al perfil de cada uno.
// Líder y Administrador además ven el correo, agregan (o invitan, si la persona aún no tiene cuenta),
// cambian roles y quitan, con las restricciones de lib/permisos.ts. Los roles los guarda y revisa el
// servidor (#241): ya no hay usuarios "pendientes de confirmar".
export function Usuarios() {
  const user = useStore((s) => s.currentUser());
  const gestiona = esLiderOAdmin(user.rol);
  const cargar = useStore((s) => s.cargarUsuarios);
  const cargando = useStore((s) => s.usuariosCargando);
  const ocupado = useStore((s) => s.equipoOcupado);
  const error = useStore((s) => s.errorUsuarios);
  const revision = useStore((s) => s.revisionEquipo);
  useEffect(() => {
    const refrescar = () => {
      if (!useStore.getState().equipoOcupado) void cargar();
    };
    refrescar();
    const timer = window.setInterval(refrescar, 15_000);
    window.addEventListener('focus', refrescar);
    return () => { window.clearInterval(timer); window.removeEventListener('focus', refrescar); };
  }, [cargar, revision]);

  return (
    <div>
      <PageHeader
        title="Usuarios"
        desc={gestiona ? 'Quiénes están en el proyecto, con qué rol, e invitaciones o cambios de rol.' : 'Quiénes están en el proyecto y con qué rol. Entra a un perfil para ver sus tareas.'}
      />
      <div className="space-y-3">
        <div className="flex items-center justify-end gap-3">
          {error && <p role="alert" className="text-sm text-red-300">{error}</p>}
          <button className="btn-outline btn-sm" disabled={cargando || ocupado} onClick={() => void cargar()}>{cargando ? 'Actualizando…' : 'Actualizar'}</button>
        </div>
        {puedeCrearUsuarios(user.rol) && <SeccionCrearUsuario />}
        <ListaUsuarios key={revision} gestiona={gestiona} />
      </div>
    </div>
  );
}

function SeccionCrearUsuario() {
  const user = useStore((s) => s.currentUser());
  const crearUsuario = useStore((s) => s.crearUsuario);
  const ocupado = useStore((s) => s.equipoOcupado || s.usuariosCargando);

  const [correo, setCorreo] = useState('');
  const [rol, setRol] = useState<Rol>('Desarrollador');
  const correoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(correo.trim());
  // Un Líder no puede agregar Líderes ni Administradores; el Administrador puede agregar otros Administradores.
  const rolesAsignables = ROLES.filter((r) => (user.rol === 'Líder' ? r !== 'Líder' && r !== 'Administrador' : true));

  return (
    <Seccion title="Agregar usuario">
      <div className="flex flex-wrap items-end gap-2">
        <div className="min-w-[14rem] flex-1">
          <label className="label">Correo</label>
          <input
            className="input"
            type="email"
            placeholder="nombre@correo.com"
            value={correo}
            disabled={ocupado}
            onChange={(e) => setCorreo(e.target.value)}
          />
        </div>
        <div>
          <label className="label">Rol</label>
          <select className="input" disabled={ocupado} value={rol} onChange={(e) => setRol(e.target.value as Rol)}>
            {rolesAsignables.map((r) => <option key={r} value={r}>{r}</option>)}
          </select>
        </div>
        <button
          className="btn-primary"
          disabled={!correoOk || ocupado}
          onClick={async () => {
            if (await crearUsuario(correo, rol)) {
              setCorreo('');
            }
          }}
        >
          Agregar
        </button>
      </div>
    </Seccion>
  );
}

function ListaUsuarios({ gestiona }: { gestiona: boolean }) {
  const user = useStore((s) => s.currentUser());
  const usuariosLocales = useStore((s) => s.usuarios);
  const usuariosRemotos = useStore((s) => s.usuariosVisibles);
  const usuarios = enTauri() ? usuariosRemotos : usuariosLocales.map((u) => ({ ...u, propio: u.email === user.email }));
  const ocupado = useStore((s) => s.equipoOcupado || s.usuariosCargando);
  const error = useStore((s) => s.errorUsuarios);
  const eliminar = useStore((s) => s.eliminarUsuario);
  const cambiarRol = useStore((s) => s.cambiarRol);
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
            {usuarios.map((u, indice) => {
              const correo = u.email ?? (u.propio ? user.email : null);
              const invitado = !!u.invitado;
              // A un invitado no se le cambia el rol: se quita la invitación y se vuelve a invitar.
              const rolesPosibles = !gestiona || invitado || !correo || u.propio ? [] : rolesAsignablesA(user.rol, u.rol);
              return (
                <tr key={correo ?? `usuario-${indice}`} className="border-b border-border/50 last:border-0">
                  <td className="px-4 py-2.5">
                    <div className="flex items-center gap-2">
                      {correo && !invitado ? <Avatar email={correo} size={26} /> : <span className="flex h-7 w-7 items-center justify-center rounded-full bg-surface text-xs">{u.nombre.slice(0, 1)}{u.apellido.slice(0, 1)}</span>}
                      <button
                        type="button"
                        className="text-left text-text hover:text-primary hover:underline"
                        disabled={!correo || invitado}
                        onClick={() => correo && verPerfil(correo)}
                        title={`Ver el perfil de ${u.nombre}`}
                      >
                        {u.nombre} {u.apellido}
                      </button>
                      {u.propio && <span className="text-[11px] text-faint">(tú)</span>}
                      {gestiona && invitado && (
                        <span className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300">Invitado · aún sin cuenta</span>
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
                        disabled={ocupado}
                        onChange={(e) => { if (correo) void cambiarRol(correo, e.target.value as Rol); }}
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
                        {correo && !u.propio && puedeEliminarUsuario(user.rol, u.rol) && (
                          <button className="btn-danger btn-sm" disabled={ocupado} onClick={() => setTarget(correo)}>
                            {invitado ? 'Quitar invitación' : 'Eliminar'}
                          </button>
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
        onClose={() => { if (!ocupado) setTarget(null); }}
        onConfirm={async () => { if (target && await eliminar(target)) setTarget(null); }}
        ocupado={ocupado}
        error={error}
        title="Eliminar usuario"
        tone="danger"
        palabra="eliminar"
        confirmLabel="Eliminar del equipo"
        description={`${target} pierde el acceso al proyecto y a sus documentos de inmediato.`}
      />
    </Card>
  );
}
