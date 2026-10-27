import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useStore } from '../store/useStore';
import * as fx from '../data/fixtures';
import type { WorkspaceUnityVC } from '../types';
import { Toast } from './Toast';

// Pantallas de entrada antes de los tableros — ver documentacion/FRONTEND.md,
// "Autenticación / acceso al proyecto": Pantalla inicial → workspaces de Unity VC
// (o "Clonar proyecto") → según el .db de BranchBoard: entrar, pendiente de
// confirmación, solicitar ingreso o inicializar. Es solo mockup: nunca bloquea,
// "Saltar al tablero" está siempre disponible.
type Paso = 'inicio' | 'workspaces' | 'clonar' | 'pendiente' | 'no-registrado' | 'solicitud' | 'inicializar';

export function AccesoProyecto() {
  const usuarios = useStore((s) => s.usuarios);
  const vpnOn = useStore((s) => s.vpnOn);
  const toggleVpn = useStore((s) => s.toggleVpn);
  const correoContacto = useStore((s) => s.correoContacto);
  const setEmail = useStore((s) => s.setEmail);
  const setRolActual = useStore((s) => s.setRolActual);
  const setAccesoCompletado = useStore((s) => s.setAccesoCompletado);
  const showToast = useStore((s) => s.showToast);

  const [paso, setPaso] = useState<Paso>('inicio');
  const [workspaces, setWorkspaces] = useState<WorkspaceUnityVC[]>(fx.workspaces);
  const [elegido, setElegido] = useState<WorkspaceUnityVC | null>(null);
  // Simula el correo que devuelve `cm whoami` — de ahí sale cuál de los casos aplica.
  const [correo, setCorreo] = useState(usuarios[0].email);

  const usuario = usuarios.find((u) => u.email === correo);

  const entrar = (msg: string) => {
    if (usuario) setEmail(usuario.email);
    setAccesoCompletado(true);
    showToast(msg);
  };

  const abrirWorkspace = (ws: WorkspaceUnityVC) => {
    setElegido(ws);
    if (!ws.tiene_db_branchboard) return setPaso('inicializar');
    if (!usuario) return setPaso('no-registrado');
    if (!usuario.firma) return setPaso('pendiente');
    entrar(`Conectado a ${ws.nombre} como ${usuario.rol}`);
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 overflow-y-auto bg-surface p-4">
      <div className="card flex w-full max-w-lg flex-col gap-5 p-8">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-baseline gap-1 text-2xl font-bold tracking-tight">
            <span className="text-text">Branch</span>
            <span className="text-accent">Board</span>
          </div>
          {paso !== 'inicio' && (
            <button className="btn-ghost btn-sm" onClick={() => setPaso(paso === 'solicitud' ? 'no-registrado' : paso === 'clonar' ? 'workspaces' : 'inicio')}>
              ← Volver
            </button>
          )}
        </div>

        {paso === 'inicio' && (
          <PasoInicio
            vpnOn={vpnOn}
            onToggleVpn={toggleVpn}
            onCrear={() => showToast('"Crear un nuevo proyecto" todavía no está disponible', 'info')}
            onIngresar={() => setPaso('workspaces')}
          />
        )}

        {paso === 'workspaces' && (
          <PasoWorkspaces
            vpnOn={vpnOn}
            workspaces={workspaces}
            onElegir={abrirWorkspace}
            onClonar={() => setPaso('clonar')}
            onVaciar={() => setWorkspaces([])}
          />
        )}

        {paso === 'clonar' && (
          <PasoClonar
            onListo={(ws) => {
              setWorkspaces((prev) => [...prev, ws]);
              showToast(`Workspace ${ws.nombre} clonado en ${ws.ruta}`);
              setPaso('workspaces');
            }}
          />
        )}

        {paso === 'pendiente' && (
          <Aviso icono="⏳" tono="bg-warn/15" titulo="Tu cuenta está pendiente de confirmación">
            Tu cuenta (<span className="font-mono text-xs text-text">{correo}</span>) ya fue agregada a{' '}
            <span className="font-semibold text-text">{elegido?.nombre}</span> por un Líder o Administrador, y está
            pendiente de que el Administrador la confirme. Cuando lo haga, entrarás directo la próxima vez que abras BranchBoard.
          </Aviso>
        )}

        {paso === 'no-registrado' && (
          <>
            <Aviso icono="🚫" tono="bg-red-500/15" titulo="Tu cuenta no está registrada en este proyecto">
              La cuenta detectada por Unity VC (<span className="font-mono text-xs text-text">{correo}</span>) todavía no
              está registrada en <span className="font-semibold text-text">{elegido?.nombre}</span>.
            </Aviso>
            <button className="btn-primary w-full" onClick={() => setPaso('solicitud')}>
              ✉️ Solicitar ingreso a una organización
            </button>
          </>
        )}

        {paso === 'solicitud' && (
          <PasoSolicitud
            correoDetectado={correo}
            correoContacto={correoContacto}
            onEnviada={() => showToast('Abriendo tu cliente de correo con la solicitud…', 'info')}
          />
        )}

        {paso === 'inicializar' && (
          <>
            <Aviso icono="🧩" tono="bg-primary-soft" titulo="Este proyecto no usa BranchBoard todavía">
              <span className="font-semibold text-text">{elegido?.nombre}</span> nunca se ha configurado con BranchBoard.
              Puedes inicializarlo aquí mismo — quien lo inicializa queda como{' '}
              <span className="font-semibold text-text">Administrador</span> del proyecto.
            </Aviso>
            <button
              className="btn-primary w-full"
              onClick={() => {
                // Sin un usuario registrado que usar, el mockup entra como el Administrador semilla.
                if (!usuario) setEmail(usuarios[0].email);
                else setEmail(usuario.email);
                setRolActual('Administrador');
                setAccesoCompletado(true);
                showToast(`${elegido?.nombre} inicializado — eres su Administrador`);
              }}
            >
              Inicializar BranchBoard en {elegido?.nombre}
            </button>
          </>
        )}
      </div>

      {/* Controles del mockup: elegir qué devuelve `cm whoami` y saltarse el flujo */}
      <div className="flex w-full max-w-lg flex-wrap items-center justify-between gap-2 rounded-xl border border-dashed border-border px-3 py-2 text-xs">
        <label className="flex items-center gap-2">
          <span className="text-faint">Demo · cm whoami:</span>
          <select
            className="bg-transparent font-medium text-text outline-none"
            value={correo}
            onChange={(e) => setCorreo(e.target.value)}
          >
            {usuarios.map((u) => (
              <option key={u.email} value={u.email} className="bg-surface text-text">
                {u.email} ({u.firma ? u.rol : 'sin confirmar'})
              </option>
            ))}
            <option value={fx.CORREO_NO_REGISTRADO} className="bg-surface text-text">
              {fx.CORREO_NO_REGISTRADO} (no registrado)
            </option>
          </select>
        </label>
        <button className="btn-ghost btn-sm" onClick={() => setAccesoCompletado(true)}>
          Saltar al tablero →
        </button>
      </div>

      <Toast />
    </div>
  );
}

function Aviso({ icono, tono, titulo, children }: { icono: string; tono: string; titulo: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-3 text-center">
      <div className={`grid h-14 w-14 place-items-center rounded-2xl text-2xl ${tono}`}>{icono}</div>
      <h2 className="text-base font-semibold text-text">{titulo}</h2>
      <p className="text-sm text-muted">{children}</p>
    </div>
  );
}

function PasoInicio({
  vpnOn,
  onToggleVpn,
  onCrear,
  onIngresar,
}: {
  vpnOn: boolean;
  onToggleVpn: () => void;
  onCrear: () => void;
  onIngresar: () => void;
}) {
  return (
    <>
      <button
        onClick={onToggleVpn}
        className="flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-2 px-4 py-3 text-left transition hover:border-border-strong"
        title="Conexión VPN (Cloudflare WARP)"
      >
        <div>
          <div className="text-sm font-medium text-text">Conexión VPN (Cloudflare WARP)</div>
          <div className="text-xs text-muted">
            En redes como la de la universidad, Unity VC no conecta sin la VPN activa.
          </div>
        </div>
        <span
          className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors ${
            vpnOn ? 'bg-done' : 'bg-faint'
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              vpnOn ? 'translate-x-4' : 'translate-x-0.5'
            }`}
          />
        </span>
      </button>

      <div className="grid gap-3 sm:grid-cols-2">
        <button
          onClick={onCrear}
          className="flex flex-col items-start gap-1 rounded-xl border border-dashed border-border p-4 text-left opacity-60 transition hover:opacity-80"
        >
          <span className="text-xl">✨</span>
          <span className="text-sm font-semibold text-text">Crear un nuevo proyecto</span>
          <span className="text-xs text-faint">Próximamente</span>
        </button>
        <button
          onClick={onIngresar}
          className="flex flex-col items-start gap-1 rounded-xl border border-border bg-surface-2 p-4 text-left transition hover:border-primary"
        >
          <span className="text-xl">📂</span>
          <span className="text-sm font-semibold text-text">Ingresar a un proyecto existente</span>
          <span className="text-xs text-muted">Elige uno de tus workspaces de Unity VC</span>
        </button>
      </div>
    </>
  );
}

function PasoWorkspaces({
  vpnOn,
  workspaces,
  onElegir,
  onClonar,
  onVaciar,
}: {
  vpnOn: boolean;
  workspaces: WorkspaceUnityVC[];
  onElegir: (ws: WorkspaceUnityVC) => void;
  onClonar: () => void;
  onVaciar: () => void;
}) {
  return (
    <>
      <div>
        <h2 className="text-base font-semibold text-text">Tus workspaces de Unity VC</h2>
        <p className="mt-1 text-sm text-muted">
          Detectados con <code className="font-mono text-xs">cm workspace list</code> en este equipo.
        </p>
      </div>

      {!vpnOn && (
        <div className="rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-xs text-muted">
          ⚠️ La VPN está apagada — es posible que Unity VC no pueda conectar con el servidor.
        </div>
      )}

      {workspaces.length > 0 ? (
        <div className="flex flex-col gap-2">
          {workspaces.map((ws) => (
            <button
              key={ws.repo}
              onClick={() => onElegir(ws)}
              className="flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-2 px-4 py-3 text-left transition hover:border-primary"
            >
              <div className="min-w-0">
                <div className="text-sm font-semibold text-text">{ws.nombre}</div>
                <div className="truncate font-mono text-[11px] text-faint">
                  {ws.repo} · {ws.ruta}
                </div>
              </div>
              <span className="text-faint">→</span>
            </button>
          ))}
          <button className="btn-ghost btn-sm self-start text-faint" onClick={onVaciar}>
            Demo: simular que no hay workspaces
          </button>
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border px-4 py-6 text-center">
          <p className="text-sm text-muted">
            No tienes ningún workspace de Unity VC en este equipo todavía. Si no tienes Unity VC instalado, instálalo
            primero; luego puedes clonar el proyecto desde aquí.
          </p>
          <button className="btn-primary btn-sm" onClick={onClonar}>
            ⬇️ Clonar proyecto
          </button>
        </div>
      )}
    </>
  );
}

function PasoClonar({ onListo }: { onListo: (ws: WorkspaceUnityVC) => void }) {
  const [carpeta, setCarpeta] = useState('D:\\Unity');
  const [progreso, setProgreso] = useState<number | null>(null);
  const terminado = useRef(false);

  // Simula `cm workspace create` + checkout con progreso.
  useEffect(() => {
    if (progreso === null || terminado.current) return;
    if (progreso >= 100) {
      terminado.current = true;
      const nombre = fx.PROYECTO_NOMBRE;
      onListo({ nombre, repo: `${nombre}@MIGUELMEZA`, ruta: `${carpeta.replace(/\\$/, '')}\\${nombre}`, tiene_db_branchboard: true });
      return;
    }
    const t = setTimeout(() => setProgreso((p) => Math.min(100, (p ?? 0) + 8)), 120);
    return () => clearTimeout(t);
  }, [progreso, carpeta, onListo]);

  return (
    <>
      <div>
        <h2 className="text-base font-semibold text-text">Clonar proyecto</h2>
        <p className="mt-1 text-sm text-muted">
          Elige dónde crear el workspace. BranchBoard corre <code className="font-mono text-xs">cm workspace create</code> y
          descarga el proyecto ahí.
        </p>
      </div>
      <div>
        <label className="label">Carpeta de destino</label>
        <input
          className="input"
          value={carpeta}
          disabled={progreso !== null}
          onChange={(e) => setCarpeta(e.target.value)}
        />
      </div>
      {progreso === null ? (
        <button className="btn-primary w-full" disabled={!carpeta.trim()} onClick={() => setProgreso(0)}>
          Clonar {fx.PROYECTO_NOMBRE}
        </button>
      ) : (
        <div className="flex flex-col gap-1.5">
          <div className="h-2 w-full overflow-hidden rounded-full bg-surface-3">
            <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progreso}%` }} />
          </div>
          <span className="text-xs text-muted">Descargando archivos… {progreso}%</span>
        </div>
      )}
    </>
  );
}

function PasoSolicitud({
  correoDetectado,
  correoContacto,
  onEnviada,
}: {
  correoDetectado: string;
  correoContacto: string;
  onEnviada: () => void;
}) {
  const [nombre, setNombre] = useState('');
  const [apellido, setApellido] = useState('');
  const [correo, setCorreo] = useState(correoDetectado);
  const [organizacion, setOrganizacion] = useState('');

  const completo = nombre.trim() && apellido.trim() && correo.trim() && organizacion.trim();
  const mailto =
    `mailto:${correoContacto}?subject=${encodeURIComponent('Solicitud de ingreso a BranchBoard')}` +
    `&body=${encodeURIComponent(
      `Nombre: ${nombre}\nApellido: ${apellido}\nCorreo: ${correo}\nID de la organización: ${organizacion}`
    )}`;

  return (
    <>
      <div>
        <h2 className="text-base font-semibold text-text">Solicitar ingreso a una organización</h2>
        <p className="mt-1 text-sm text-muted">
          Se abrirá tu cliente de correo con la solicitud ya escrita, dirigida a{' '}
          <span className="font-mono text-xs text-text">{correoContacto}</span>. BranchBoard no guarda nada.
        </p>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div>
          <label className="label">Nombre</label>
          <input className="input" value={nombre} onChange={(e) => setNombre(e.target.value)} />
        </div>
        <div>
          <label className="label">Apellido</label>
          <input className="input" value={apellido} onChange={(e) => setApellido(e.target.value)} />
        </div>
        <div className="sm:col-span-2">
          <label className="label">Correo (el mismo de Unity VC)</label>
          <input className="input" value={correo} onChange={(e) => setCorreo(e.target.value)} />
        </div>
        <div className="sm:col-span-2">
          <label className="label">ID de la organización</label>
          <input className="input" value={organizacion} onChange={(e) => setOrganizacion(e.target.value)} />
        </div>
      </div>
      <a
        href={completo ? mailto : undefined}
        onClick={(e) => (completo ? onEnviada() : e.preventDefault())}
        className={`btn-primary w-full ${completo ? '' : 'pointer-events-none opacity-50'}`}
      >
        Enviar solicitud por correo
      </a>
    </>
  );
}
