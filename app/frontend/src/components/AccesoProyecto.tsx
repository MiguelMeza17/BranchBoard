import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useStore } from '../store/useStore';
import * as fx from '../data/fixtures';
import type { WorkspaceUnityVC } from '../types';
import {
  api,
  enTauri,
  ErrorBackend,
  mensajeDeError,
  proyectosPadreUnity,
  type EntradaProyecto,
  type ProyectoServidor,
} from '../lib/api';
import { SelectorCarpeta } from './SelectorCarpeta';
import { LoginUnityVCModal } from './LoginUnityVCModal';
import { Modal } from './Modal';
import { Toast } from './Toast';
import { Icono, IconoEnLinea } from './Icono';

// Pantallas de entrada antes de los tableros — ver documentacion/FRONTEND.md,
// "Autenticación / acceso al proyecto": Pantalla inicial → workspaces de Unity VC
// (o "Clonar proyecto") → lo que responde `entrar_a_proyecto` (#89): entrar,
// pendiente de confirmación, sin usuario o esperar al dueño del repositorio (que
// lo configura solo la primera vez que lo abre). En el navegador lo simula el
// mockup y "Saltar al tablero" está siempre disponible.
type Paso = 'inicio' | 'workspaces' | 'clonar' | 'pendiente' | 'no-registrado' | 'esperar-dueno' | 'carpeta-compartida';
type EntradaDentro = Extract<EntradaProyecto, { estado: 'dentro' }>;

export function AccesoProyecto() {
  const usuarios = useStore((s) => s.usuarios);
  const vpnOn = useStore((s) => s.vpnOn);
  const vpnInstalado = useStore((s) => s.vpnInstalado);
  const vpnCargando = useStore((s) => s.vpnCargando);
  const vpnInstalando = useStore((s) => s.vpnInstalando);
  const toggleVpn = useStore((s) => s.toggleVpn);
  const instalarWarp = useStore((s) => s.instalarWarp);
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
  const [sinUvcs, setSinUvcs] = useState(false);
  // "Crear un nuevo proyecto": nombre del proyecto y tus datos; quedas como Administrador.
  const crearProyectoNuevo = useStore((s) => s.crearProyectoNuevo);
  const [crear, setCrear] = useState(false);
  // "Ingresar a un proyecto existente" sin Unity VC: abre el buscador de carpetas del equipo.
  const [abrirLocal, setAbrirLocal] = useState(false);
  const [carpetaLocal, setCarpetaLocal] = useState('');
  const [errorLocal, setErrorLocal] = useState<string | null>(null);
  const entrarAProyectoLocal = useStore((s) => s.entrarAProyecto);
  const abrirCarpetaLocal = () => {
    const ruta = carpetaLocal.trim().replace(/[\\/]+$/, '');
    const ws = workspaces.find((w) => w.repo.endsWith('@local') && w.ruta.toLowerCase() === ruta.toLowerCase());
    if (!ws) {
      setErrorLocal('En esa carpeta no hay un proyecto de BranchBoard.');
      return;
    }
    setAbrirLocal(false);
    setErrorLocal(null);
    entrarAProyectoLocal(ws.nombre);
    showToast(`Abriste ${ws.nombre}`);
  };
  const [nuevo, setNuevo] = useState({ proyecto: '', nombre: '', correo: '', clave: '' });
  const correoNuevoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(nuevo.correo.trim());
  // Errores por campo: se marcan en cuanto sales del campo (o al pulsar "Crear", si nunca entraste).
  type CampoNuevo = 'proyecto' | 'nombre' | 'correo' | 'clave';
  const errorNuevo: Record<CampoNuevo, string | null> = {
    proyecto: nuevo.proyecto.trim() ? null : 'Escribe el nombre del proyecto.',
    nombre: nuevo.nombre.trim() ? null : 'Escribe tu nombre.',
    correo: !nuevo.correo.trim()
      ? 'Escribe tu correo.'
      : correoNuevoOk
        ? null
        : 'El correo no es válido (ej. nombre@correo.com).',
    clave: !nuevo.clave
      ? 'Escribe una contraseña.'
      : nuevo.clave.length < 8
        ? 'La contraseña debe tener al menos 8 caracteres.'
        : null,
  };
  const [tocados, setTocados] = useState<Set<CampoNuevo>>(new Set());
  const tocar = (c: CampoNuevo) => setTocados((t) => new Set(t).add(c));
  const errorVisible = (c: CampoNuevo) => (tocados.has(c) ? errorNuevo[c] : null);
  const cerrarCrear = () => {
    setCrear(false);
    setTocados(new Set());
  };

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
    <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 overflow-y-auto bg-surface p-4">
      <div className="card flex w-full max-w-lg flex-col gap-5 p-8">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-baseline text-2xl font-bold tracking-tight">
            <span className="text-text">Branch</span>
            <span className="text-green-400">Board</span>
          </div>
          {(paso !== 'inicio' || sinUvcs) && (
            <button
              className="btn-ghost btn-sm"
              onClick={() =>
                paso === 'inicio'
                  ? setSinUvcs(false)
                  : setPaso(paso === 'clonar' || paso === 'carpeta-compartida' ? 'workspaces' : 'inicio')
              }
            >
              ← Volver
            </button>
          )}
        </div>

        {paso === 'inicio' && (
          <PasoInicio
            sesion={sesion}
            onLogin={() => setLogin(true)}
            sinUvcs={sinUvcs}
            onSinUvcs={() => setSinUvcs(true)}
            vpnOn={vpnOn}
            vpnInstalado={vpnInstalado}
            vpnCargando={vpnCargando}
            vpnInstalando={vpnInstalando}
            onToggleVpn={() => void toggleVpn()}
            onInstalarWarp={() => void instalarWarp()}
            cmInstalado={cmInstalado}
            cmInstalando={cmInstalando}
            onInstalarCm={() => void instalarCm()}
            onCrear={() => setCrear(true)}
            // Con sesión: tus proyectos de Unity VC. Sin Unity VC: buscar en tu equipo la carpeta de un
            // proyecto de BranchBoard que no está conectado a Unity VC.
            onIngresar={() => (sinUvcs && !sesion ? setAbrirLocal(true) : setPaso('workspaces'))}
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
            onElegir={abrirWorkspace}
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

      <ModalCrearProyectoReal
        open={crear && enTauri()}
        onClose={cerrarCrear}
        onCreado={async (ws, entrada) => {
          setWorkspaces([...workspaces.filter((w) => w.repo !== ws.repo), ws]);
          cerrarCrear();
          if (entrada.estado !== 'dentro') {
            const pasos = {
              pendiente: 'pendiente',
              no_registrado: 'no-registrado',
              esperar_dueno: 'esperar-dueno',
            } as const;
            setPaso(pasos[entrada.estado]);
            return;
          }
          const carpeta = await api.carpetaCompartida(() => null);
          if (!carpeta) {
            setPorEntrar({ ws, entrada });
            setPaso('carpeta-compartida');
            return;
          }
          terminarEntrada(ws, entrada);
        }}
      />

      <Modal
        open={crear && !enTauri()}
        onClose={cerrarCrear}
        title="Crear un nuevo proyecto"
        subtitle="Quedarás como Administrador del proyecto"
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={cerrarCrear}>
              Cancelar
            </button>
            <button
              type="button"
              className="btn-primary btn-sm"
              onClick={() => {
                if (Object.values(errorNuevo).some(Boolean)) {
                  setTocados(new Set(['proyecto', 'nombre', 'correo', 'clave']));
                  return;
                }
                crearProyectoNuevo({ proyecto: nuevo.proyecto.trim(), nombre: nuevo.nombre.trim(), correo: nuevo.correo.trim() });
                cerrarCrear();
                setNuevo({ proyecto: '', nombre: '', correo: '', clave: '' });
              }}
            >
              Crear
            </button>
          </div>
        }
      >
        <div className="space-y-3 text-sm">
          {/* El nombre del proyecto va destacado: más grande que tus datos. */}
          <label className="block pb-1">
            <span className="mb-1.5 block text-sm font-semibold text-text">Nombre del proyecto</span>
            <input
              className={`input h-12 text-lg font-semibold placeholder:font-normal placeholder:opacity-30 ${errorVisible('proyecto') ? 'border-red-500/70' : ''}`}
              autoFocus
              placeholder="Mi proyecto"
              value={nuevo.proyecto}
              onBlur={() => tocar('proyecto')}
              onChange={(e) => setNuevo({ ...nuevo, proyecto: e.target.value })}
            />
            {errorVisible('proyecto') && <span className="mt-1 block text-[11px] text-red-300"><IconoEnLinea nombre="alerta" sinColor />{errorVisible('proyecto')}</span>}
          </label>
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Tu nombre</span>
            <input
              className={`input ${errorVisible('nombre') ? 'border-red-500/70' : ''}`}
              value={nuevo.nombre}
              onBlur={() => tocar('nombre')}
              onChange={(e) => setNuevo({ ...nuevo, nombre: e.target.value })}
            />
            {errorVisible('nombre') && <span className="mt-1 block text-[11px] text-red-300"><IconoEnLinea nombre="alerta" sinColor />{errorVisible('nombre')}</span>}
          </label>
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Tu correo</span>
            <input
              className={`input ${errorVisible('correo') ? 'border-red-500/70' : ''}`}
              type="email"
              value={nuevo.correo}
              onBlur={() => tocar('correo')}
              onChange={(e) => setNuevo({ ...nuevo, correo: e.target.value })}
            />
            {errorVisible('correo') && <span className="mt-1 block text-[11px] text-red-300"><IconoEnLinea nombre="alerta" sinColor />{errorVisible('correo')}</span>}
          </label>
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Tu contraseña (mínimo 8 caracteres)</span>
            <input
              className={`input ${errorVisible('clave') ? 'border-red-500/70' : ''}`}
              type="password"
              autoComplete="new-password"
              value={nuevo.clave}
              onBlur={() => tocar('clave')}
              onChange={(e) => setNuevo({ ...nuevo, clave: e.target.value })}
            />
            {errorVisible('clave') && <span className="mt-1 block text-[11px] text-red-300"><IconoEnLinea nombre="alerta" sinColor />{errorVisible('clave')}</span>}
          </label>
        </div>
      </Modal>

      <Modal
        open={abrirLocal}
        onClose={() => {
          setAbrirLocal(false);
          setErrorLocal(null);
        }}
        title="Abrir un proyecto de BranchBoard"
        subtitle="Busca en tu equipo la carpeta de un proyecto que no está conectado a Unity VC"
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={() => setAbrirLocal(false)}>
              Cancelar
            </button>
            <button type="button" className="btn-primary btn-sm" onClick={abrirCarpetaLocal}>
              Abrir
            </button>
          </div>
        }
      >
        <div className="space-y-2 text-sm">
          <SelectorCarpeta
            value={carpetaLocal}
            onChange={(v) => {
              setCarpetaLocal(v);
              setErrorLocal(null);
            }}
            placeholder="Carpeta del proyecto"
          />
          {errorLocal && <p className="text-[11px] text-red-300"><IconoEnLinea nombre="alerta" sinColor />{errorLocal}</p>}
          <p className="text-[11px] text-faint">
            En la app de escritorio, el botón de carpeta abre el explorador de carpetas de Windows.
          </p>
        </div>
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

function PasoInicio({
  sesion,
  onLogin,
  sinUvcs,
  onSinUvcs,
  vpnOn,
  vpnInstalado,
  vpnCargando,
  vpnInstalando,
  onToggleVpn,
  onInstalarWarp,
  cmInstalado,
  cmInstalando,
  onInstalarCm,
  onCrear,
  onIngresar,
}: {
  sesion: string | null;
  onLogin: () => void;
  sinUvcs: boolean;
  onSinUvcs: () => void;
  vpnOn: boolean;
  vpnInstalado: boolean;
  vpnCargando: boolean;
  vpnInstalando: boolean;
  onToggleVpn: () => void;
  onInstalarWarp: () => void;
  cmInstalado: boolean;
  cmInstalando: boolean;
  onInstalarCm: () => void;
  onCrear: () => void;
  onIngresar: () => void;
}) {
  return (
    <>
      {/* Primero la VPN: sin ella, en algunas redes Unity VC no conecta (ni para iniciar sesión). */}
      <div className="flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-2 px-4 py-3 text-left transition">
        <div className="min-w-0">
          <div className="text-sm font-medium text-text">Conexión VPN (Cloudflare WARP)</div>
          <div className="text-xs text-muted">
            {!vpnInstalado
              ? 'Cloudflare WARP no está instalado en este equipo.'
              : vpnCargando
                ? 'Actualizando conexión VPN…'
                : 'En redes como la de la universidad, Unity VC no conecta sin la VPN activa.'}
          </div>
          {!vpnInstalado && (
            <button
              type="button"
              onClick={onInstalarWarp}
              disabled={vpnInstalando}
              className="mt-1 text-xs font-medium text-primary underline hover:opacity-80 disabled:opacity-60"
            >
              {vpnInstalando ? 'Instalando Cloudflare WARP…' : 'Instalar Cloudflare WARP'}
            </button>
          )}
        </div>
        <button
          type="button"
          onClick={onToggleVpn}
          disabled={!vpnInstalado || vpnCargando || vpnInstalando}
          title={vpnInstalado ? 'Conexión VPN (Cloudflare WARP)' : 'Cloudflare WARP no está instalado'}
          aria-label="Conexión VPN (Cloudflare WARP)"
          className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
            vpnInstalado && vpnOn ? 'bg-done' : 'bg-faint'
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              vpnInstalado && vpnOn ? 'translate-x-4' : 'translate-x-0.5'
            }`}
          />
        </button>
      </div>

      {!cmInstalado && !(sinUvcs && !sesion) && (
        <AvisoInstalarCm instalando={cmInstalando} onInstalar={onInstalarCm} />
      )}

      {/* Sesión de Unity VC: es la identidad en BranchBoard (no hay login propio). */}
      {sinUvcs && !sesion ? null : sesion ? (
        <div className="flex items-center justify-between gap-3 rounded-xl border border-done/40 bg-done/10 px-4 py-3">
          <div className="min-w-0">
            <div className="text-sm font-medium text-text">Sesión de Unity VC iniciada</div>
            <div className="truncate font-mono text-xs text-muted">{sesion}</div>
          </div>
          <div className="flex shrink-0 gap-1.5">
            <button className="btn-ghost btn-sm" onClick={onLogin}>
              Cambiar de cuenta
            </button>
            <button className="btn-primary btn-sm" onClick={onIngresar}>
              Mis proyectos →
            </button>
          </div>
        </div>
      ) : (
        <button
          onClick={onLogin}
          disabled={!cmInstalado || cmInstalando}
          className="flex w-full items-center justify-center gap-2 rounded-xl bg-primary px-4 py-3 text-sm font-semibold text-white transition hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          Iniciar sesión con Unity VC
        </button>
      )}

      {/* Sin sesión de Unity VC solo se puede crear un proyecto nuevo (que todavía no está en Unity VC);
          para entrar a uno existente hace falta la sesión. */}
      {!sesion && !sinUvcs && (
        <button
          onClick={onSinUvcs}
          className="w-full rounded-xl border border-border px-4 py-2.5 text-sm text-muted transition hover:border-border-strong hover:text-text"
        >
          Continuar sin Unity VC
        </button>
      )}

      {/* Las dos opciones solo salen al elegir "Continuar sin Unity VC". Con sesión, se va directo a tus
          workspaces. */}
      {sinUvcs && !sesion && (
      <div className="grid gap-3 sm:grid-cols-2">
        <button
          onClick={onCrear}
          className="flex flex-col items-start gap-1 rounded-xl border border-border bg-surface-2 p-4 text-left transition hover:border-primary"
        >
          <Icono nombre="chispa" className="h-6 w-6" />
          <span className="text-sm font-semibold text-text">Crear un nuevo proyecto</span>
        </button>
        <button
          onClick={onIngresar}
          className="flex flex-col items-start gap-1 rounded-xl border border-border bg-surface-2 p-4 text-left transition hover:border-primary"
        >
          <Icono nombre="carpetaAbierta" className="h-6 w-6" />
          <span className="text-sm font-semibold text-text">Ingresar a un proyecto existente</span>
        </button>
      </div>
      )}
    </>
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
          {workspaces.map((ws) => (
            <button
              key={ws.ruta}
              onClick={() => onElegir(ws)}
              className="flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-2 px-4 py-3 text-left transition hover:border-primary"
            >
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-semibold text-text">{ws.nombre}</span>
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

function PasoClonar({
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

  const iniciarClonReal = async () => {
    if (!repoElegido || !carpeta.trim()) return;
    setErrorClon(null);
    setProgreso(5);
    setEstadoProgreso('Preparando workspace…');
    const dejar = await api.alProgresoClonado((ev) => {
      setProgreso(ev.porcentaje);
      setEstadoProgreso(ev.mensaje);
    });
    try {
      const ws = await api.clonarProyecto(repoElegido, carpeta.trim());
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
          Elige dónde crear el workspace. BranchBoard corre <code className="font-mono text-xs">cm workspace create</code> y
          descarga el proyecto ahí.
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
        <label className="label">Carpeta de destino</label>
        <SelectorCarpeta
          value={carpeta}
          onChange={(v) => {
            setCarpeta(v);
            setErrorClon(null);
          }}
          disabled={progreso !== null}
          placeholder="Carpeta donde se guardará el proyecto"
        />
      </div>
      {errorClon && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">
          {errorClon}
        </div>
      )}
      {progreso === null ? (
        <button
          className="btn-primary w-full"
          disabled={!carpeta.trim() || (esTauri && !repoElegido)}
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

function ModalCrearProyectoReal({
  open,
  onClose,
  onCreado,
}: {
  open: boolean;
  onClose: () => void;
  onCreado: (ws: WorkspaceUnityVC, entrada: EntradaProyecto) => void | Promise<void>;
}) {
  const proyectosServidor = useStore((s) => s.proyectosServidor);
  const proyectosUnity = proyectosPadreUnity(proyectosServidor);
  const [proyectoUnity, setProyectoUnity] = useState('');
  const [nombre, setNombre] = useState('');
  const [carpeta, setCarpeta] = useState('');
  const [creando, setCreando] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const proyectoUnityEfectivo = proyectoUnity || proyectosUnity[0] || '';

  const cerrar = () => {
    if (creando) return;
    setProyectoUnity('');
    setNombre('');
    setCarpeta('');
    setError(null);
    onClose();
  };

  const enviar = async () => {
    if (!nombre.trim() || !carpeta.trim() || creando) return;
    setCreando(true);
    setError(null);
    try {
      const res = await api.crearProyecto(
        nombre.trim(),
        carpeta.trim(),
        proyectoUnityEfectivo || undefined
      );
      setProyectoUnity('');
      setNombre('');
      setCarpeta('');
      await onCreado(res.workspace, res.entrada);
    } catch (e) {
      setError(mensajeDeError(e));
    } finally {
      setCreando(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={cerrar}
      title="Crear un nuevo proyecto"
      subtitle="Crea el repositorio y su workspace en Unity VC Cloud; quedarás como Administrador"
      size="sm"
      footer={
        <div className="flex w-full justify-end gap-2">
          <button type="button" className="btn-ghost btn-sm" disabled={creando} onClick={cerrar}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn-primary btn-sm"
            disabled={creando || !nombre.trim() || !carpeta.trim()}
            onClick={() => void enviar()}
          >
            {creando ? 'Creando proyecto…' : 'Crear proyecto'}
          </button>
        </div>
      }
    >
      <div className="space-y-3 text-sm">
        {proyectosUnity.length > 0 && (
          <label className="block">
            <span className="mb-1.5 block text-xs font-semibold text-text">Proyecto en Unity Cloud</span>
            <select
              className="input"
              disabled={creando}
              value={proyectoUnityEfectivo}
              onChange={(e) => {
                setProyectoUnity(e.target.value);
                setError(null);
              }}
            >
              {proyectosUnity.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className="block">
          <span className="mb-1.5 block text-xs font-semibold text-text">Nombre del proyecto</span>
          <input
            className="input"
            autoFocus
            disabled={creando}
            placeholder="Mi proyecto"
            value={nombre}
            onChange={(e) => {
              setNombre(e.target.value);
              setError(null);
            }}
          />
        </label>
        <div>
          <span className="mb-1.5 block text-xs font-semibold text-text">Carpeta de destino</span>
          <SelectorCarpeta
            value={carpeta}
            onChange={(v) => {
              setCarpeta(v);
              setError(null);
            }}
            disabled={creando}
            placeholder="Carpeta donde se creará el workspace"
          />
        </div>
        {error && (
          <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">
            {error}
          </div>
        )}
      </div>
    </Modal>
  );
}
