import { useEffect, useRef, useState, type ChangeEvent, type FormEvent, type InputHTMLAttributes, type ReactNode } from 'react';
import { useStore } from '../store/useStore';
import * as fx from '../data/fixtures';
import type { WorkspaceUnityVC } from '../types';
import {
  api,
  enTauri,
  ErrorBackend,
  mensajeDeError,
  nombreDeRepo,
  unoPorRepo,
  type EntradaProyecto,
  type ProyectoServidor,
} from '../lib/api';
import { correosRecientes, ingresar, olvidarCorreo, registrarCuenta, type Cuenta } from '../lib/cuentas';
import { SelectorCarpeta } from './SelectorCarpeta';
import { LoginUnityVCModal } from './LoginUnityVCModal';
import { FondoRamas } from './FondoRamas';
import { EntradaClave } from './EntradaClave';
import { Modal } from './Modal';
import { Toast } from './Toast';
import { Icono, IconoEnLinea } from './Icono';

// Pantallas de entrada antes de los tableros — ver documentacion/FRONTEND.md,
// "Autenticación / acceso al proyecto": Pantalla inicial → workspaces de Unity VC
// (o "Clonar proyecto") → lo que responde `entrar_a_proyecto` (#89): entrar,
// pendiente de confirmación, sin usuario o esperar al dueño del repositorio (que
// lo configura solo la primera vez que lo abre). En el navegador lo simula el
// mockup y "Saltar al tablero" está siempre disponible.
type Paso = 'inicio' | 'registro' | 'recuperar' | 'workspaces' | 'clonar' | 'pendiente' | 'no-registrado' | 'esperar-dueno' | 'carpeta-compartida';
type EntradaDentro = Extract<EntradaProyecto, { estado: 'dentro' }>;

export function AccesoProyecto() {
  const usuarios = useStore((s) => s.usuarios);
  const vpnOn = useStore((s) => s.vpnOn);
  const cmInstalado = useStore((s) => s.cmInstalado);
  const cmInstalando = useStore((s) => s.cmInstalando);
  const instalarCm = useStore((s) => s.instalarCm);
  const marcarCmNoInstalado = useStore((s) => s.marcarCmNoInstalado);
  const reconectarUnityVc = useStore((s) => s.reconectarUnityVc);
  const [errorAcceso, setErrorAcceso] = useState<string | null>(null);
  const setEmail = useStore((s) => s.setEmail);
  const setRolActual = useStore((s) => s.setRolActual);
  const setAccesoCompletado = useStore((s) => s.setAccesoCompletado);
  const showToast = useStore((s) => s.showToast);

  const entrarAProyecto = useStore((s) => s.entrarAProyecto);
  // Desde "Cambiar de proyecto" se llega directo a la lista de workspaces.
  // Proyecto del servidor elegido para clonar (desde el menú de BranchBoard), si lo hay.
  const proyectoAClonar = useStore((s) => s.proyectoAClonar);
  const proyectoRepoAClonar = useStore((s) => s.proyectoRepoAClonar);
  const proyectosServidor = useStore((s) => s.proyectosServidor);
  const cargarWorkspacesYProyectos = useStore((s) => s.cargarWorkspacesYProyectos);
  const [paso, setPaso] = useState<Paso>(
    useStore.getState().proyectoAClonar ? 'clonar' : useStore.getState().accesoIrAWorkspaces ? 'workspaces' : 'inicio'
  );
  const workspaces = useStore((s) => s.workspaces);
  const setWorkspaces = useStore((s) => s.setWorkspaces);

  useEffect(() => {
    if (!enTauri()) return;
    if (paso === 'workspaces' || paso === 'clonar') {
      void cargarWorkspacesYProyectos();
    }
  }, [paso, cargarWorkspacesYProyectos]);
  const [elegido, setElegido] = useState<WorkspaceUnityVC | null>(null);
  // Simula el correo que devuelve `cm whoami` — de ahí sale cuál de los casos aplica.
  const [correo, setCorreo] = useState(useStore.getState().sesionUnityVC ?? usuarios[0]?.email ?? '');

  // Sesión de Unity VC: hasta iniciarla no se puede entrar a un proyecto. Al iniciarla, su correo es
  // lo que devuelve `cm whoami`. Se guarda en el store: no se pide otra vez al cambiar de proyecto.
  const sesion = useStore((s) => s.sesionUnityVC);
  const setSesion = useStore((s) => s.setSesionUnityVC);
  const [login, setLogin] = useState(false);
  const [correoRegistrado, setCorreoRegistrado] = useState('');
  const cuenta = useStore((s) => s.cuentaBranchBoard);
  const entrarSinProyecto = useStore((s) => s.entrarSinProyecto);
  // Lo que decidiría el backend, con los datos de ejemplo y el correo elegido en "Demo · cm whoami".
  // En el mockup, el dueño del repositorio es el Administrador semilla.
  const entradaMockup = (ws: WorkspaceUnityVC): EntradaProyecto => {
    const base = { email: correo, proyecto: ws.nombre };
    if (!ws.tiene_db_branchboard) {
      const dueno = usuarios.find((u) => u.rol === 'Administrador')?.email;
      return correo === dueno
        ? { ...base, estado: 'dentro', rol: 'Administrador', rol_verificado: true, configurado_ahora: true }
        : { ...base, estado: 'esperar_dueno' };
    }
    const u = usuarios.find((x) => x.email === correo);
    if (!u) return { ...base, estado: 'no_registrado' };
    if (!u.firma) return { ...base, estado: 'pendiente' };
    return { ...base, estado: 'dentro', rol: u.rol, rol_verificado: true, configurado_ahora: false };
  };

  // Workspace que todavía no usa BranchBoard en este equipo: abrirlo siendo el dueño crea sus datos y una llave
  // de Administrador nueva, así que se confirma antes.
  const [porConfigurar, setPorConfigurar] = useState<WorkspaceUnityVC | null>(null);

  // Entrada ya aceptada que espera a que se elija la carpeta compartida.
  const [porEntrar, setPorEntrar] = useState<{ ws: WorkspaceUnityVC; entrada: EntradaDentro } | null>(null);

  const abrirWorkspace = async (ws: WorkspaceUnityVC) => {
    setElegido(ws);
    setErrorAcceso(null);
    let respuesta: EntradaProyecto;
    try {
      respuesta = await api.entrarAProyecto(ws.ruta, () => entradaMockup(ws));
    } catch (e) {
      if (e instanceof ErrorBackend && e.codigo === 'cm_no_instalado') {
        marcarCmNoInstalado();
      }
      const msg = mensajeDeError(e);
      setErrorAcceso(msg);
      showToast(msg, 'danger');
      return;
    }
    setCorreo(respuesta.email);
    if (respuesta.estado !== 'dentro') {
      const pasos = { pendiente: 'pendiente', no_registrado: 'no-registrado', esperar_dueno: 'esperar-dueno' } as const;
      return setPaso(pasos[respuesta.estado]);
    }
    // Sin carpeta compartida no se puede guardar ningún cambio del equipo: se pide antes del tablero.
    let carpeta: string | null;
    try {
      carpeta = await api.carpetaCompartida(() => useStore.getState().carpetasCompartidas[ws.nombre] ?? null);
    } catch (e) {
      showToast(mensajeDeError(e), 'danger');
      return;
    }
    if (!carpeta) {
      setPorEntrar({ ws, entrada: respuesta });
      return setPaso('carpeta-compartida');
    }
    terminarEntrada(ws, respuesta);
  };

  const terminarEntrada = (ws: WorkspaceUnityVC, entrada: EntradaDentro) => {
    // Mientras el resto de pantallas siga con datos de ejemplo, quien entra tiene que estar en la lista.
    if (!useStore.getState().usuarios.some((u) => u.email === entrada.email)) {
      const [nombre, apellido = ''] = entrada.email.split('@')[0].split(/[._-]/);
      useStore.setState((s) => ({
        usuarios: [
          ...s.usuarios,
          { email: entrada.email, nombre, apellido, rol: entrada.rol, fecha_alta: '', firma: 'backend', firmado_en: null },
        ],
      }));
    }
    setEmail(entrada.email);
    setRolActual(entrada.rol);
    // Lo que se muestra es el proyecto (el repositorio), no el nombre del workspace local.
    entrarAProyecto(entrada.proyecto, ws.nombre);
    showToast(
      entrada.configurado_ahora
        ? `${entrada.proyecto} configurado con BranchBoard — eres su Administrador`
        : `Conectado a ${entrada.proyecto} como ${entrada.rol}`
    );
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 overflow-y-auto bg-surface p-4 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      <FondoRamas />
      <div className="mb-10 flex items-baseline text-5xl font-bold tracking-tight [filter:drop-shadow(0_6px_18px_rgba(74,222,128,0.35))_drop-shadow(0_2px_4px_rgba(0,0,0,0.9))]">
        <span className="text-text">Branch</span>
        <span className="text-green-400">Board</span>
      </div>
      <div className="card flex w-full max-w-lg flex-col gap-5 p-8">
        {paso !== 'inicio' && paso !== 'registro' && paso !== 'recuperar' && (
          <div className="flex justify-end">
            <button
              className="btn-ghost btn-sm"
              onClick={() =>
                paso === 'workspaces' && cuenta
                  ? setAccesoCompletado(true)
                  : setPaso(
                  paso === 'clonar' || paso === 'carpeta-compartida'
                    ? 'workspaces'
                    : 'inicio'
                )
              }
            >
              ← Volver
            </button>
          </div>
        )}

        {paso === 'inicio' && (
          <PasoLogin
            correoInicial={correoRegistrado}
            onIngresado={(c) => entrarSinProyecto(c)}
            onRegistrarse={() => setPaso('registro')}
            onUnityVc={() => setLogin(true)}
            unityVcDeshabilitado={!cmInstalado || cmInstalando}
            avisoCm={!cmInstalado && <AvisoInstalarCm instalando={cmInstalando} onInstalar={() => void instalarCm()} />}
            onRecuperar={(c) => {
              setCorreoRegistrado(c);
              setPaso('recuperar');
            }}
          />
        )}

        {paso === 'recuperar' && (
          <PasoRecuperar correoInicial={correoRegistrado} onVolver={() => setPaso('inicio')} />
        )}

        {paso === 'registro' && (
          <PasoRegistro
            onRegistrado={(c) => {
              setCorreoRegistrado(c.correo);
              showToast('Cuenta creada. Ya puedes iniciar sesión.');
              setPaso('inicio');
            }}
            onIniciarSesion={() => setPaso('inicio')}
          />
        )}

        {paso === 'workspaces' && (
          <PasoWorkspaces
            vpnOn={vpnOn}
            cmInstalado={cmInstalado}
            cmInstalando={cmInstalando}
            onInstalarCm={() => void instalarCm()}
            errorAcceso={errorAcceso}
            onReconectar={() =>
              reconectarUnityVc().then((ok) => {
                if (ok) setErrorAcceso(null);
              })
            }
            workspaces={workspaces}
            onElegir={(ws) => (enTauri() && !ws.tiene_db_branchboard ? setPorConfigurar(ws) : void abrirWorkspace(ws))}
            onClonar={() => setPaso('clonar')}
            onVaciar={() => setWorkspaces([])}
          />
        )}

        {paso === 'clonar' && (
          <PasoClonar
            nombre={proyectoAClonar ?? fx.PROYECTO_NOMBRE}
            repoInicial={proyectoRepoAClonar}
            proyectosServidor={proyectosServidor}
            onListo={(ws) => {
              setWorkspaces([...workspaces.filter((w) => w.repo !== ws.repo), ws]);
              showToast(`Workspace ${ws.nombre} clonado en ${ws.ruta}`);
              setPaso('workspaces');
            }}
          />
        )}

        {paso === 'pendiente' && (
          <Aviso icono="reloj" tono="bg-warn/15" titulo="Tu cuenta está pendiente de confirmación">
            Tu cuenta (<span className="font-mono text-xs text-text">{correo}</span>) ya fue agregada a{' '}
            <span className="font-semibold text-text">{elegido?.nombre}</span> por un Líder o Administrador, y está
            pendiente de que el Administrador la confirme. Cuando lo haga, entrarás directo la próxima vez que abras BranchBoard.
          </Aviso>
        )}

        {paso === 'no-registrado' && (
          <>
            <Aviso icono="prohibido" tono="bg-red-500/15" titulo="Tu cuenta no está registrada en este proyecto">
              La cuenta detectada por Unity VC (<span className="font-mono text-xs text-text">{correo}</span>) todavía no
              está registrada en <span className="font-semibold text-text">{elegido?.nombre}</span>.
            </Aviso>
            <p className="text-center text-sm text-muted">
              Pídele a un Líder o Administrador del proyecto que te cree un usuario con este correo.
            </p>
          </>
        )}


        {paso === 'esperar-dueno' && (
          <Aviso icono="pieza" tono="bg-primary-soft" titulo="Este proyecto no usa BranchBoard todavía">
            <span className="font-semibold text-text">{elegido?.nombre}</span> se configura con BranchBoard la primera vez
            que lo abre el dueño del repositorio, que queda como Administrador. Pídele que lo abra; después podrá
            agregarte al equipo.
          </Aviso>
        )}

        {paso === 'carpeta-compartida' && porEntrar && (
          <PasoCarpetaCompartida
            entrada={porEntrar.entrada}
            onListo={() => {
              const { ws, entrada } = porEntrar;
              setPorEntrar(null);
              terminarEntrada(ws, entrada);
            }}
          />
        )}
      </div>

      <Modal
        open={porConfigurar !== null}
        onClose={() => setPorConfigurar(null)}
        title={`¿Configurar ${porConfigurar ? nombreDeRepo(porConfigurar) : ''} en BranchBoard?`}
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={() => setPorConfigurar(null)}>
              Cancelar
            </button>
            <button
              type="button"
              className="btn-primary btn-sm"
              onClick={() => {
                const ws = porConfigurar;
                setPorConfigurar(null);
                if (ws) void abrirWorkspace(ws);
              }}
            >
              Configurar
            </button>
          </div>
        }
      >
        <p className="text-sm text-muted">
          Este proyecto no usa BranchBoard en este equipo. Si eres el dueño del repositorio se crean sus datos y una
          llave de Administrador nueva. Si ya usó BranchBoard antes (en este u otro equipo), restaura sus datos en vez
          de configurarlo de nuevo.
        </p>
      </Modal>

      <LoginUnityVCModal
        open={login}
        onClose={() => setLogin(false)}
        correoDemo={correo}
        forzarNueva={Boolean(sesion)}
        onSesionIniciada={(c) => {
          setLogin(false);
          setSesion(c);
          setCorreo(c);
          showToast(`Sesión de Unity VC iniciada como ${c}`);
          useStore.setState({ cmSesionActiva: true });
          if (paso === 'inicio') {
            // Desde el login: se entra a BranchBoard sin proyecto, igual que con la cuenta de BranchBoard; los
            // proyectos de Unity VC se eligen desde el aviso "No estás en ningún proyecto" o el menú del proyecto.
            if (enTauri()) void cargarWorkspacesYProyectos();
            entrarSinProyecto({ nombre: c.split('@')[0], apellido: '', correo: c });
            return;
          }
          // Con la sesión iniciada se pasa directo a elegir el workspace.
          setPaso('workspaces');
        }}
      />

      {/* Controles del mockup (solo en el navegador): elegir qué devuelve `cm whoami` y saltarse el flujo.
          En la app de escritorio el correo sale de `cm whoami` y el acceso lo decide `entrar_a_proyecto`. */}
      {!enTauri() && (
      <div className="flex w-full max-w-lg flex-wrap items-center justify-between gap-2 rounded-xl border border-dashed border-border px-3 py-2 text-xs">
        <label className="flex items-center gap-2">
          <span className="text-faint">Demo · cm whoami:</span>
          <select
            className="bg-transparent font-medium text-text outline-none"
            value={correo}
            onChange={(e) => {
              setCorreo(e.target.value);
              if (sesion) setSesion(e.target.value);
            }}
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
      )}

      <Toast />
    </div>
  );
}

// Carpeta compartida (Drive/OneDrive) del proyecto en esta máquina: obligatoria antes del tablero, porque
// por ahí viajan todos los cambios del equipo (`ops/`). El Administrador elige o crea la carpeta y la comparte;
// los demás eligen la que les compartió (el backend comprueba que sea la de este proyecto).
function PasoCarpetaCompartida({ entrada, onListo }: { entrada: EntradaDentro; onListo: () => void }) {
  const setCarpetaCompartida = useStore((s) => s.setCarpetaCompartida);
  const [ruta, setRuta] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [guardando, setGuardando] = useState(false);
  const esAdmin = entrada.rol === 'Administrador';

  const guardar = async () => {
    if (!ruta.trim()) return setError('Elige una carpeta.');
    setGuardando(true);
    try {
      const guardada = await api.configurarCarpetaCompartida(ruta.trim(), () => ruta.trim());
      setCarpetaCompartida(entrada.proyecto, guardada);
      onListo();
    } catch (e) {
      setError(mensajeDeError(e));
    }
    setGuardando(false);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col items-center gap-3 text-center">
        <div className="grid h-14 w-14 place-items-center rounded-2xl bg-primary-soft">
          <Icono nombre="carpeta" className="h-7 w-7" />
        </div>
        <h2 className="text-base font-semibold text-text">Carpeta compartida de {entrada.proyecto}</h2>
        <p className="text-sm text-muted">
          {esAdmin
            ? 'Elige una carpeta de tu Drive/OneDrive para este proyecto y compártela con el equipo.'
            : 'Elige la carpeta del proyecto que te compartió el Administrador.'}
        </p>
      </div>
      <SelectorCarpeta
        value={ruta}
        onChange={(v) => {
          setRuta(v);
          setError(null);
        }}
        placeholder="Carpeta de Drive/OneDrive"
        titulo={`Carpeta compartida de ${entrada.proyecto}`}
      />
      {error && (
        <p className="text-[11px] text-red-300">
          <IconoEnLinea nombre="alerta" sinColor />
          {error}
        </p>
      )}
      <button className="btn-primary w-full" disabled={guardando} onClick={() => void guardar()}>
        {guardando ? 'Guardando…' : 'Continuar'}
      </button>
    </div>
  );
}

function Aviso({ icono, tono, titulo, children }: { icono: string; tono: string; titulo: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-3 text-center">
      <div className={`grid h-14 w-14 place-items-center rounded-2xl ${tono}`}><Icono nombre={icono} className="h-7 w-7" /></div>
      <h2 className="text-base font-semibold text-text">{titulo}</h2>
      <p className="text-sm text-muted">{children}</p>
    </div>
  );
}

function AvisoInstalarCm({
  instalando,
  onInstalar,
}: {
  instalando: boolean;
  onInstalar: () => void;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-warn/40 bg-warn/10 px-4 py-3">
      <div className="text-sm font-medium text-text">Unity Version Control (cm) no está instalado</div>
      <div className="text-xs text-muted">
        Se necesita para iniciar sesión y sincronizar workspaces en este equipo (incluye Gluon para subtareas simples).
      </div>
      <button
        type="button"
        disabled={instalando}
        onClick={onInstalar}
        className="btn-primary btn-sm self-start"
      >
        {instalando ? 'Instalando Unity Version Control…' : 'Instalar Unity Version Control'}
      </button>
    </div>
  );
}

const correoValido = (c: string) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(c.trim());

function Campo({
  etiqueta,
  error,
  ...props
}: { etiqueta: string; error?: string | null } & InputHTMLAttributes<HTMLInputElement>) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs text-faint">{etiqueta}</span>
      {props.type === 'password' ? (
        <EntradaClave className={`input ${error ? 'border-red-500/70' : ''}`} {...props} />
      ) : (
        <input className={`input ${error ? 'border-red-500/70' : ''}`} {...props} />
      )}
      {error && (
        <span className="mt-1 block text-[11px] text-red-300">
          <IconoEnLinea nombre="alerta" sinColor />
          {error}
        </span>
      )}
    </label>
  );
}

// Inicio clásico: "Bienvenido", correo, contraseña e "Ingresar". Abajo, "¿No tienes cuenta? Regístrate".
function PasoLogin({
  correoInicial,
  onIngresado,
  onRegistrarse,
  onUnityVc,
  unityVcDeshabilitado,
  avisoCm,
  onRecuperar,
}: {
  correoInicial: string;
  onIngresado: (c: Cuenta) => void;
  onRegistrarse: () => void;
  onUnityVc: () => void;
  unityVcDeshabilitado: boolean;
  avisoCm: ReactNode;
  onRecuperar: (correo: string) => void;
}) {
  const [recientes, setRecientes] = useState(correosRecientes);
  // Recién registrado: su correo. Si no, el último con el que se entró en este equipo.
  const [correo, setCorreo] = useState(() => correoInicial || correosRecientes()[0] || '');
  const conCorreo = Boolean(correoInicial || recientes[0]);
  const [verRecientes, setVerRecientes] = useState(false);
  const sugeridos = recientes.filter((c) => c !== correo.trim().toLowerCase() && c.includes(correo.trim().toLowerCase()));
  const [clave, setClave] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [enviando, setEnviando] = useState(false);

  const enviar = async (e: FormEvent) => {
    e.preventDefault();
    if (!correoValido(correo) || !clave) {
      setError('Escribe tu correo y tu contraseña.');
      return;
    }
    setEnviando(true);
    setError(null);
    try {
      onIngresado(await ingresar(correo, clave));
    } catch (err) {
      setError(err instanceof Error ? err.message : mensajeDeError(err));
      setEnviando(false);
    }
  };

  return (
    <form onSubmit={enviar} className="flex flex-col gap-4">
      <div className="text-center">
        <h2 className="text-2xl font-semibold text-text">Bienvenido</h2>
        <p className="mt-1 text-sm text-faint">Ingresa tu correo y contraseña</p>
      </div>
      <div className="relative">
        <Campo
          etiqueta="Correo"
          type="email"
          autoFocus={!conCorreo}
          autoComplete="off"
          placeholder="tu@correo.com"
          value={correo}
          onFocus={() => {
            setError(null);
            setVerRecientes(true);
          }}
          // Se espera un poco para que el clic en una sugerencia llegue antes de ocultarlas.
          onBlur={() => setTimeout(() => setVerRecientes(false), 150)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') setVerRecientes(false);
          }}
          onChange={(e) => {
            setCorreo(e.target.value);
            setError(null);
            setVerRecientes(true);
          }}
        />
        {verRecientes && sugeridos.length > 0 && (
          <div className="absolute inset-x-0 top-full z-20 mt-1 overflow-hidden rounded-lg border border-border bg-surface-2 py-1 shadow-pop">
            <div className="px-3 pb-1 pt-1.5 text-[10px] font-semibold uppercase tracking-wide text-faint">Usados en este equipo</div>
            {sugeridos.map((c) => (
              <div key={c} className="group flex items-center hover:bg-surface-3">
                <button
                  type="button"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    setCorreo(c);
                    setVerRecientes(false);
                    document.getElementById('clave-login')?.focus();
                  }}
                  className="min-w-0 flex-1 truncate px-3 py-2 text-left text-sm text-text"
                >
                  {c}
                </button>
                <button
                  type="button"
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    olvidarCorreo(c);
                    setRecientes(correosRecientes());
                  }}
                  title="Olvidar este correo"
                  aria-label={`Olvidar ${c}`}
                  className="px-3 py-2 text-xs text-faint opacity-60 transition hover:text-red-300 group-hover:opacity-100"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
      <Campo
        etiqueta="Contraseña"
        id="clave-login"
        type="password"
        autoFocus={conCorreo}
        autoComplete="current-password"
        value={clave}
        onFocus={() => setError(null)}
        onChange={(e) => {
          setClave(e.target.value);
          setError(null);
        }}
      />
      <div className="-mt-2 flex justify-end">
        <button type="button" onClick={() => onRecuperar(correo.trim())} className="text-xs text-sky-400 hover:underline">
          ¿Olvidaste tu contraseña?
        </button>
      </div>
      {error && (
        <p className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">{error}</p>
      )}
      <button
        type="submit"
        disabled={enviando}
        className="w-full rounded-xl bg-primary px-4 py-3 text-sm font-semibold text-white transition hover:opacity-90 disabled:opacity-50"
      >
        {enviando ? 'Ingresando…' : 'Ingresar'}
      </button>
      <div className="flex items-center gap-3 text-[11px] uppercase tracking-wide text-faint">
        <span className="h-px flex-1 bg-border" />o<span className="h-px flex-1 bg-border" />
      </div>
      <button
        type="button"
        onClick={onUnityVc}
        disabled={unityVcDeshabilitado}
        className="flex w-full items-center justify-center gap-2 rounded-xl border border-border bg-surface-2 px-4 py-3 text-sm font-semibold text-text transition hover:border-border-strong hover:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-50"
      >
        <Icono nombre="unity" className="h-4 w-4" sinColor />
        Iniciar sesión con Unity VC
      </button>
      {avisoCm}
      <p className="text-center text-sm text-muted">
        ¿No tienes cuenta?{' '}
        <button type="button" onClick={onRegistrarse} className="font-semibold text-sky-400 hover:underline">
          Regístrate
        </button>
      </p>
    </form>
  );
}

// Recuperar contraseña: pide el correo. Todavía no envía nada (no hay servidor de correo ni backend de
// cuentas): al enviar solo avisa que la recuperación aún no está disponible.
function PasoRecuperar({ correoInicial, onVolver }: { correoInicial: string; onVolver: () => void }) {
  const [correo, setCorreo] = useState(correoInicial);
  const [error, setError] = useState<string | null>(null);
  const [enviado, setEnviado] = useState(false);

  const enviar = (e: FormEvent) => {
    e.preventDefault();
    if (!correoValido(correo)) {
      setError(correo.trim() ? 'El correo no es válido (ej. nombre@correo.com).' : 'Escribe tu correo.');
      return;
    }
    setEnviado(true);
  };

  return (
    <form onSubmit={enviar} className="flex flex-col gap-4">
      <div className="text-center">
        <h2 className="text-2xl font-semibold text-text">Recuperar contraseña</h2>
        <p className="mt-1 text-sm text-faint">Escribe el correo de tu cuenta y te enviaremos cómo cambiarla</p>
      </div>
      <Campo
        etiqueta="Correo"
        type="email"
        autoFocus
        autoComplete="email"
        placeholder="tu@correo.com"
        value={correo}
        error={error}
        onFocus={() => setError(null)}
        onChange={(e) => {
          setCorreo(e.target.value);
          setError(null);
          setEnviado(false);
        }}
      />
      {enviado && (
        <p className="rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-xs text-muted">
          <IconoEnLinea nombre="alerta" />
          La recuperación de contraseña todavía no está disponible. Pronto podrás recibir aquí un correo para cambiarla.
        </p>
      )}
      <button
        type="submit"
        className="w-full rounded-xl bg-primary px-4 py-3 text-sm font-semibold text-white transition hover:opacity-90 disabled:opacity-50"
      >
        Enviar
      </button>
      <p className="text-center text-sm text-muted">
        ¿La recordaste?{' '}
        <button type="button" onClick={onVolver} className="font-semibold text-sky-400 hover:underline">
          Inicia sesión
        </button>
      </p>
    </form>
  );
}

// Registro: nombre, apellido, correo y contraseña. Al terminar vuelve a "Bienvenido" para iniciar sesión.
function PasoRegistro({
  onRegistrado,
  onIniciarSesion,
}: {
  onRegistrado: (c: Cuenta) => void;
  onIniciarSesion: () => void;
}) {
  const [datos, setDatos] = useState({ nombre: '', apellido: '', correo: '', clave: '' });
  type CampoR = keyof typeof datos;
  const [tocados, setTocados] = useState<Set<CampoR>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [enviando, setEnviando] = useState(false);

  const errores: Record<CampoR, string | null> = {
    nombre: datos.nombre.trim() ? null : 'Escribe tu nombre.',
    apellido: datos.apellido.trim() ? null : 'Escribe tu apellido.',
    correo: !datos.correo.trim()
      ? 'Escribe tu correo.'
      : correoValido(datos.correo)
        ? null
        : 'El correo no es válido (ej. nombre@correo.com).',
    clave: !datos.clave
      ? 'Escribe una contraseña.'
      : datos.clave.length < 8
        ? 'La contraseña debe tener al menos 8 caracteres.'
        : null,
  };
  const campo = (c: CampoR) => ({
    value: datos[c],
    error: tocados.has(c) ? errores[c] : null,
    onBlur: () => setTocados((t) => new Set(t).add(c)),
    onFocus: () => setError(null),
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setDatos({ ...datos, [c]: e.target.value });
      setError(null);
    },
  });

  const enviar = async (e: FormEvent) => {
    e.preventDefault();
    if (Object.values(errores).some(Boolean)) {
      setTocados(new Set<CampoR>(['nombre', 'apellido', 'correo', 'clave']));
      return;
    }
    setEnviando(true);
    setError(null);
    try {
      onRegistrado(await registrarCuenta(datos));
    } catch (err) {
      setError(err instanceof Error ? err.message : mensajeDeError(err));
      setEnviando(false);
    }
  };

  return (
    <form onSubmit={enviar} className="flex flex-col gap-4">
      <div className="text-center">
        <h2 className="text-2xl font-semibold text-text">Crea tu cuenta</h2>
        <p className="mt-1 text-sm text-faint">Con ella podrás iniciar sesión en BranchBoard</p>
      </div>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Campo etiqueta="Nombre" autoFocus autoComplete="given-name" {...campo('nombre')} />
        <Campo etiqueta="Apellido" autoComplete="family-name" {...campo('apellido')} />
      </div>
      <Campo etiqueta="Correo" type="email" autoComplete="email" placeholder="tu@correo.com" {...campo('correo')} />
      <Campo etiqueta="Contraseña (mínimo 8 caracteres)" type="password" autoComplete="new-password" {...campo('clave')} />
      {error && (
        <p className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">{error}</p>
      )}
      <button
        type="submit"
        disabled={enviando}
        className="w-full rounded-xl bg-primary px-4 py-3 text-sm font-semibold text-white transition hover:opacity-90 disabled:opacity-50"
      >
        {enviando ? 'Creando cuenta…' : 'Registrarme'}
      </button>
      <p className="text-center text-sm text-muted">
        ¿Ya tienes cuenta?{' '}
        <button type="button" onClick={onIniciarSesion} className="font-semibold text-sky-400 hover:underline">
          Inicia sesión
        </button>
      </p>
    </form>
  );
}

function PasoWorkspaces({
  vpnOn,
  cmInstalado,
  cmInstalando,
  onInstalarCm,
  errorAcceso,
  onReconectar,
  workspaces,
  onElegir,
  onClonar,
  onVaciar,
}: {
  vpnOn: boolean;
  cmInstalado: boolean;
  cmInstalando: boolean;
  onInstalarCm: () => void;
  errorAcceso: string | null;
  onReconectar: () => Promise<void> | void;
  workspaces: WorkspaceUnityVC[];
  onElegir: (ws: WorkspaceUnityVC) => void;
  onClonar: () => void;
  onVaciar: () => void;
}) {
  const [reconectando, setReconectando] = useState(false);

  return (
    <>
      <div>
        <h2 className="text-base font-semibold text-text">Tus workspaces de Unity VC</h2>
        <p className="mt-1 text-sm text-muted">
          Detectados con <code className="font-mono text-xs">cm workspace list</code> en este equipo.
        </p>
      </div>

      {!cmInstalado && <AvisoInstalarCm instalando={cmInstalando} onInstalar={onInstalarCm} />}

      {errorAcceso && (
        <div className="flex items-center justify-between gap-3 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">
          <span className="min-w-0">{errorAcceso}</span>
          <button
            type="button"
            disabled={reconectando || !cmInstalado}
            onClick={() => {
              setReconectando(true);
              Promise.resolve(onReconectar()).finally(() => setReconectando(false));
            }}
            className="btn-outline btn-sm shrink-0"
          >
            {reconectando ? 'Reconectando…' : 'Reconectar'}
          </button>
        </div>
      )}

      {!vpnOn && (
        <div className="rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-xs text-muted">
          <IconoEnLinea nombre="alerta" />La VPN está apagada — es posible que Unity VC no pueda conectar con el servidor.
        </div>
      )}

      {workspaces.length > 0 ? (
        <div className="flex flex-col gap-2">
          {unoPorRepo(workspaces).map((ws) => (
            <button
              key={ws.ruta}
              onClick={() => onElegir(ws)}
              className="flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-2 px-4 py-3 text-left transition hover:border-primary"
            >
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-semibold text-text">{nombreDeRepo(ws)}</span>
                  <span
                    className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] ${
                      ws.tiene_db_branchboard ? 'bg-primary-soft text-primary' : 'bg-surface-3 text-faint'
                    }`}
                  >
                    {ws.tiene_db_branchboard ? 'Con datos' : 'Sin configurar'}
                  </span>
                </div>
                <div className="truncate font-mono text-[11px] text-faint">
                  {ws.repo} · {ws.ruta}
                </div>
              </div>
              <span className="text-faint">→</span>
            </button>
          ))}
          {!enTauri() && (
            <button className="btn-ghost btn-sm self-start text-faint" onClick={onVaciar}>
              Demo: simular que no hay workspaces
            </button>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border px-4 py-6 text-center">
          <p className="text-sm text-muted">
            No tienes ningún workspace de Unity VC en este equipo todavía. Si no tienes Unity VC instalado, instálalo
            primero; luego puedes clonar el proyecto desde aquí.
          </p>
          <button className="btn-primary btn-sm" onClick={onClonar}>
            <Icono nombre="descargar" className="h-3.5 w-3.5" sinColor />
            Clonar proyecto
          </button>
        </div>
      )}
    </>
  );
}

// También lo usa "Cargar proyecto" del menú (Sidebar), dentro de su ventana.
export function PasoClonar({
  nombre,
  repoInicial,
  proyectosServidor,
  onListo,
}: {
  nombre: string;
  repoInicial: string | null;
  proyectosServidor: ProyectoServidor[];
  onListo: (ws: WorkspaceUnityVC) => void;
}) {
  const esTauri = enTauri();
  const [repoElegido, setRepoElegido] = useState(repoInicial ?? proyectosServidor[0]?.repo ?? '');
  const [carpeta, setCarpeta] = useState(esTauri ? '' : 'D:\\Unity');
  // Nombre de la carpeta nueva que se crea dentro de `carpeta`: arranca con el del proyecto y sigue al
  // proyecto elegido mientras no se escriba otro.
  const [nombreCarpeta, setNombreCarpeta] = useState('');
  const [nombreEditado, setNombreEditado] = useState(false);
  const [progreso, setProgreso] = useState<number | null>(null);
  const [estadoProgreso, setEstadoProgreso] = useState('Descargando archivos…');
  const [errorClon, setErrorClon] = useState<string | null>(null);
  const terminado = useRef(false);

  useEffect(() => {
    if (!repoElegido && proyectosServidor.length > 0) {
      setRepoElegido(repoInicial ?? proyectosServidor[0].repo);
    }
  }, [repoElegido, repoInicial, proyectosServidor]);

  // Mockup en navegador: simula `cm workspace create` + checkout con progreso.
  useEffect(() => {
    if (esTauri || progreso === null || terminado.current) return;
    if (progreso >= 100) {
      terminado.current = true;
      const delServidor = fx.proyectosServidor.find((p) => p.nombre === nombre);
      onListo({
        nombre,
        repo: delServidor?.repo ?? `${nombre}@MIGUELMEZA`,
        ruta: `${carpeta.replace(/\\$/, '')}\\${nombre}`,
        tiene_db_branchboard: delServidor?.tiene_db_branchboard ?? true,
      });
      return;
    }
    const t = setTimeout(() => setProgreso((p) => Math.min(100, (p ?? 0) + 8)), 120);
    return () => clearTimeout(t);
  }, [esTauri, progreso, carpeta, onListo, nombre]);

  const proyectoVisible =
    proyectosServidor.find((p) => p.repo === repoElegido)?.nombre ?? nombre;
  useEffect(() => {
    if (!nombreEditado) setNombreCarpeta(proyectoVisible);
  }, [proyectoVisible, nombreEditado]);
  const nombreLimpio = nombreCarpeta.trim();
  const nombreValido = nombreLimpio !== '' && !/[\\/:*?"<>|]/.test(nombreLimpio) && !nombreLimpio.endsWith('.');
  const carpetaFinal = carpeta.trim() && nombreLimpio ? `${carpeta.trim().replace(/[\\/]+$/, '')}\\${nombreLimpio}` : '';

  const iniciarClonReal = async () => {
    if (!repoElegido || !carpeta.trim() || !nombreValido) return;
    setErrorClon(null);
    setProgreso(5);
    setEstadoProgreso('Preparando workspace…');
    const dejar = await api.alProgresoClonado((ev) => {
      setProgreso(ev.porcentaje);
      setEstadoProgreso(ev.mensaje);
    });
    try {
      const ws = await api.clonarProyecto(repoElegido, carpeta.trim(), nombreLimpio);
      setProgreso(100);
      onListo(ws);
    } catch (e) {
      setErrorClon(mensajeDeError(e));
      setProgreso(null);
    } finally {
      dejar();
    }
  };

  return (
    <>
      <div>
        <h2 className="text-base font-semibold text-text">Clonar proyecto</h2>
        <p className="mt-1 text-sm text-muted">
          Elige dónde guardarlo y el nombre de la carpeta: BranchBoard la crea y descarga el proyecto ahí.
        </p>
      </div>
      {esTauri && (
        <div>
          <label className="label">Proyecto en Unity VC</label>
          {proyectosServidor.length > 0 ? (
            <select
              className="input"
              value={repoElegido}
              disabled={progreso !== null}
              onChange={(e) => setRepoElegido(e.target.value)}
            >
              {proyectosServidor.map((p) => (
                <option key={p.repo} value={p.repo}>
                  {p.nombre} ({p.repo}) — {p.tiene_db_branchboard ? 'Con tareas' : 'Sin tareas'}
                </option>
              ))}
            </select>
          ) : (
            <p className="text-xs text-muted">No se encontraron repositorios accesibles en Unity VC Cloud.</p>
          )}
        </div>
      )}
      <div>
        <label className="label">Dónde guardarlo</label>
        <SelectorCarpeta
          value={carpeta}
          onChange={(v) => {
            setCarpeta(v);
            setErrorClon(null);
          }}
          disabled={progreso !== null}
          placeholder="Elige una carpeta de tu equipo"
          titulo="¿Dónde guardo el proyecto?"
          // "Guardar como": se navega a donde se quiera y se escribe el nombre de la carpeta nueva (no tiene que
          // existir); llena "Dónde guardarlo" y "Nombre de la carpeta".
          alBuscar={async () => {
            if (!esTauri) return;
            try {
              const ruta = await api.elegirCarpetaNueva('¿Dónde guardo el proyecto?', nombreLimpio || proyectoVisible);
              if (!ruta) return;
              const corte = Math.max(ruta.lastIndexOf('\\'), ruta.lastIndexOf('/'));
              if (corte <= 0) return;
              setCarpeta(ruta.slice(0, corte));
              setNombreCarpeta(ruta.slice(corte + 1));
              setNombreEditado(true);
              setErrorClon(null);
            } catch (e) {
              setErrorClon(mensajeDeError(e));
            }
          }}
        />
      </div>
      <div>
        <label className="label">Nombre de la carpeta</label>
        <input
          className={`input ${nombreLimpio && !nombreValido ? 'border-red-500/70' : ''}`}
          value={nombreCarpeta}
          disabled={progreso !== null}
          onChange={(e) => {
            setNombreCarpeta(e.target.value);
            setNombreEditado(true);
            setErrorClon(null);
          }}
        />
        {nombreLimpio && !nombreValido ? (
          <span className="mt-1 block text-[11px] text-red-300">
            <IconoEnLinea nombre="alerta" sinColor />
            No puede llevar / \ : * ? " &lt; &gt; | ni terminar en punto.
          </span>
        ) : (
          carpetaFinal && (
            <span className="mt-1 block truncate font-mono text-[11px] text-faint">Se crea: {carpetaFinal}</span>
          )
        )}
      </div>
      {errorClon && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">
          {errorClon}
        </div>
      )}
      {progreso === null ? (
        <button
          className="btn-primary w-full"
          disabled={!carpeta.trim() || !nombreValido || (esTauri && !repoElegido)}
          onClick={() => (esTauri ? void iniciarClonReal() : setProgreso(0))}
        >
          Clonar {proyectoVisible}
        </button>
      ) : (
        <div className="flex flex-col gap-1.5">
          <div className="h-2 w-full overflow-hidden rounded-full bg-surface-3">
            <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progreso}%` }} />
          </div>
          <span className="truncate text-xs text-muted">
            {estadoProgreso} {progreso}%
          </span>
        </div>
      )}
    </>
  );
}
