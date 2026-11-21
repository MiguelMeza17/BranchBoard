import { useEffect, useState } from 'react';
import { create } from 'zustand';
import { useStore } from '../store/useStore';
import {
  api,
  enTauri,
  ErrorBackend,
  etiquetaProyectoServidor,
  mensajeDeError,
  nombreDeRepo,
  type EntradaProyecto,
  type ProyectoLocal,
} from '../lib/api';
import { PasoClonar } from './AccesoProyecto';
import type { WorkspaceUnityVC } from '../types';
import { SelectorCarpeta } from './SelectorCarpeta';
import { LoginUnityVCModal } from './LoginUnityVCModal';
import { Modal } from './Modal';
import { CrearProyectoEnServidor, ListaProyectosDelServidor } from './ProyectosDelServidor';
import { Icono, IconoEnLinea } from './Icono';

// Entrar a BranchBoard sin proyecto: tras iniciar sesión con la cuenta de BranchBoard no se crea nada ni se
// asigna rol. Todo queda vacío con el aviso "No estás en ningún proyecto" y, desde ahí o desde el menú del
// proyecto (clic en el logo), se crea o abre un proyecto sin Unity VC o se conecta Unity VC.

type EntradaDentro = Extract<EntradaProyecto, { estado: 'dentro' }>;
type Ventana = 'crear' | 'cargar' | 'unity' | null;

/** Qué ventana de "sin proyecto" está abierta. La usan el aviso, el menú del proyecto y el Layout. */
export const useVentanasProyecto = create<{
  abierta: Ventana;
  abrir: (v: Ventana) => void;
  /** Workspace recién clonado: el menú del proyecto (Sidebar) lo configura y entra a él ("Traer"). */
  trasClonar: WorkspaceUnityVC | null;
  setTrasClonar: (ws: WorkspaceUnityVC | null) => void;
}>((set) => ({
  abierta: null,
  abrir: (abierta) => set({ abierta }),
  trasClonar: null,
  setTrasClonar: (trasClonar) => set({ trasClonar }),
}));

/** Hay sesión de BranchBoard pero todavía no se entró a ningún proyecto. */
export const useSinProyecto = () => useStore((s) => !s.proyectoActual);

// Proyecto sin Unity VC: no hay workspace ni carpeta compartida que elegir (va dentro del proyecto).
function terminarEntradaLocal(entrada: EntradaDentro, ruta: string) {
  const st = useStore.getState();
  if (!st.usuarios.some((u) => u.email === entrada.email)) {
    const [nombre, apellido = ''] = entrada.email.split('@')[0].split(/[._-]/);
    useStore.setState((s) => ({
      usuarios: [
        ...s.usuarios,
        { email: entrada.email, nombre, apellido, rol: entrada.rol, fecha_alta: '' },
      ],
    }));
  }
  st.setEmail(entrada.email);
  st.setRolActual(entrada.rol);
  st.entrarAProyecto(entrada.proyecto, entrada.proyecto, ruta);
  st.showToast(
    entrada.configurado_ahora ? `Proyecto ${entrada.proyecto} creado — eres su Administrador` : `Abriste ${entrada.proyecto}`
  );
}

/** Aviso que reemplaza a cualquier página mientras no se está en ningún proyecto. */
export function AvisoSinProyecto() {
  const abrir = useVentanasProyecto((s) => s.abrir);
  const tarjeta =
    'group flex flex-col items-center justify-center gap-3 rounded-2xl border border-border bg-surface-2 p-6 text-center transition hover:border-primary hover:bg-primary-soft';
  return (
    <div className="mx-auto flex max-w-xl flex-col items-center gap-6 py-16 text-center">
      <div className="grid h-16 w-16 place-items-center rounded-2xl bg-surface-2">
        <Icono nombre="carpetaAbierta" className="h-8 w-8" />
      </div>
      <div>
        <h1 className="text-xl font-semibold text-text">No estás en ningún proyecto</h1>
        <p className="mt-2 text-sm text-muted">
          Ingresa a un proyecto o crea uno para ver sus tareas, su equipo y sus configuraciones.
        </p>
      </div>
      <div className="grid w-full grid-cols-1 gap-3 sm:grid-cols-2">
        <button type="button" onClick={() => abrir('crear')} className={tarjeta}>
          <Icono nombre="mas" className="h-10 w-10 transition group-hover:scale-105" />
          <span className="text-sm font-semibold text-text">Crea tu proyecto</span>
          <span className="text-[11px] text-faint">Quedas como Administrador</span>
        </button>
        <button type="button" onClick={() => abrir('cargar')} className={tarjeta}>
          <Icono nombre="carpetaAbierta" className="h-10 w-10 transition group-hover:scale-105" />
          <span className="text-sm font-semibold text-text">Carga un proyecto</span>
          <span className="text-[11px] text-faint">De Unity VC, una carpeta o un archivo</span>
        </button>
      </div>
    </div>
  );
}

/** Ventanas de crear/abrir proyecto sin Unity VC y de conectar Unity VC. Va una sola vez, en el Layout. */
export function VentanasProyecto() {
  const abierta = useVentanasProyecto((s) => s.abierta);
  const abrir = useVentanasProyecto((s) => s.abrir);
  const cerrar = () => abrir(null);
  const sesion = useStore((s) => s.sesionUnityVC);
  const showToast = useStore((s) => s.showToast);
  const cambiarDeProyecto = useStore((s) => s.cambiarDeProyecto);
  const cargarWorkspacesYProyectos = useStore((s) => s.cargarWorkspacesYProyectos);
  return (
    <>
      <ModalCrearLocal open={abierta === 'crear'} onClose={cerrar} />
      <ModalCargarProyecto open={abierta === 'cargar'} onClose={cerrar} />
      <LoginUnityVCModal
        open={abierta === 'unity'}
        onClose={cerrar}
        correoDemo={useStore.getState().currentEmail}
        forzarNueva={Boolean(sesion)}
        onSesionIniciada={(c) => {
          cerrar();
          useStore.setState({ cmSesionActiva: true, sesionUnityVC: c });
          showToast(`Sesión de Unity VC iniciada como ${c}`);
          if (enTauri()) void cargarWorkspacesYProyectos();
          // Con la sesión iniciada se elige el workspace de Unity VC.
          cambiarDeProyecto();
        }}
      />
    </>
  );
}

function ModalCrearLocal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const cuenta = useStore((s) => s.cuentaBranchBoard);
  const crearProyectoNuevo = useStore((s) => s.crearProyectoNuevo);
  const vacio = () => ({
    carpeta: '',
    proyecto: '',
    nombre: cuenta ? `${cuenta.nombre} ${cuenta.apellido}`.trim() : '',
    correo: cuenta?.correo ?? '',
  });
  const [nuevo, setNuevo] = useState(vacio);
  useEffect(() => {
    if (open) setNuevo(vacio());
    // Solo al abrir la ventana.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  const [creando, setCreando] = useState(false);
  const [errorGeneral, setErrorGeneral] = useState<string | null>(null);
  type CampoNuevo = 'carpeta' | 'proyecto' | 'nombre' | 'correo';
  const correoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(nuevo.correo.trim());
  const errorNuevo: Record<CampoNuevo, string | null> = {
    // En el mockup no hay disco: la carpeta solo se pide en la app de escritorio.
    carpeta: !enTauri() || nuevo.carpeta.trim() ? null : 'Elige dónde guardar el proyecto.',
    proyecto: nuevo.proyecto.trim() ? null : 'Escribe el nombre del proyecto.',
    nombre: nuevo.nombre.trim() ? null : 'Escribe tu nombre.',
    correo: !nuevo.correo.trim() ? 'Escribe tu correo.' : correoOk ? null : 'El correo no es válido (ej. nombre@correo.com).',
  };
  const [tocados, setTocados] = useState<Set<CampoNuevo>>(new Set());
  const tocar = (c: CampoNuevo) => setTocados((t) => new Set(t).add(c));
  const errorVisible = (c: CampoNuevo) => (tocados.has(c) ? errorNuevo[c] : null);
  const cerrar = () => {
    if (creando) return;
    setTocados(new Set());
    setErrorGeneral(null);
    onClose();
  };
  const crear = async () => {
    if (Object.values(errorNuevo).some(Boolean)) {
      setTocados(new Set(['carpeta', 'proyecto', 'nombre', 'correo']));
      return;
    }
    const datos = {
      carpeta: nuevo.carpeta.trim(),
      proyecto: nuevo.proyecto.trim(),
      nombre: nuevo.nombre.trim(),
      correo: nuevo.correo.trim(),
    };
    if (!enTauri()) {
      crearProyectoNuevo({ proyecto: datos.proyecto, nombre: datos.nombre, correo: datos.correo });
      cerrar();
      return;
    }
    setCreando(true);
    setErrorGeneral(null);
    try {
      const { ruta, entrada } = await api.crearProyectoLocal(datos, () => {
        throw new ErrorBackend('validacion', 'Solo en la app de escritorio');
      });
      setCreando(false);
      cerrar();
      if (entrada.estado === 'dentro') terminarEntradaLocal(entrada, ruta);
    } catch (e) {
      setErrorGeneral(mensajeDeError(e));
      setCreando(false);
    }
  };
  const aviso = (texto: string | null) =>
    texto && (
      <span className="mt-1 block text-[11px] text-red-300">
        <IconoEnLinea nombre="alerta" sinColor />
        {texto}
      </span>
    );

  return (
    <Modal
      open={open}
      onClose={cerrar}
      title="Crear un nuevo proyecto"
      subtitle="Sin Unity VC. Quedarás como Administrador del proyecto"
      size="sm"
      footer={
        <div className="flex w-full justify-end gap-2">
          <button type="button" className="btn-ghost btn-sm" onClick={cerrar} disabled={creando}>
            Cancelar
          </button>
          <button type="button" className="btn-primary btn-sm" onClick={() => void crear()} disabled={creando}>
            {creando ? 'Creando…' : 'Crear'}
          </button>
        </div>
      }
    >
      <div className="space-y-3 text-sm">
        <CrearProyectoEnServidor
          ocupado={creando}
          alEntrar={(entrada) => {
            onClose();
            terminarEntradaLocal(entrada, '');
          }}
        />
        {enTauri() && (
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Dónde guardarlo</span>
            <SelectorCarpeta
              value={nuevo.carpeta}
              onChange={(v) => {
                setNuevo({ ...nuevo, carpeta: v });
                tocar('carpeta');
              }}
              placeholder="Carpeta donde se crea la del proyecto"
              titulo="¿Dónde guardo el proyecto?"
            />
            {aviso(errorVisible('carpeta'))}
            {nuevo.carpeta.trim() && nuevo.proyecto.trim() && (
              <span className="mt-1 block truncate font-mono text-[11px] text-faint">
                {nuevo.carpeta.trim().replace(/[\\/]+$/, '')}\{nuevo.proyecto.trim()}
              </span>
            )}
          </label>
        )}
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
          {aviso(errorVisible('proyecto'))}
        </label>
        <label className="block">
          <span className="mb-1 block text-xs text-faint">Tu nombre</span>
          <input
            className={`input ${errorVisible('nombre') ? 'border-red-500/70' : ''}`}
            value={nuevo.nombre}
            onBlur={() => tocar('nombre')}
            onChange={(e) => setNuevo({ ...nuevo, nombre: e.target.value })}
          />
          {aviso(errorVisible('nombre'))}
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
          {aviso(errorVisible('correo'))}
        </label>
        {errorGeneral && (
          <p className="text-[11px] text-red-300">
            <IconoEnLinea nombre="alerta" sinColor />
            {errorGeneral}
          </p>
        )}
      </div>
    </Modal>
  );
}

// "Cargar proyecto": una sola ventana con las tres formas de traer un proyecto:
// 1. De Unity VC (servidor): se clona aquí mismo, sin salir de la app.
// 2. Una carpeta del equipo: un proyecto sin Unity VC que ya está como carpeta (se abre en su sitio).
// 3. Un archivo .bbproyecto: se descomprime en la carpeta que se elija y queda listo para abrir.
function ModalCargarProyecto({ open, onClose }: { open: boolean; onClose: () => void }) {
  const workspaces = useStore((s) => s.workspaces);
  const proyectosServidor = useStore((s) => s.proyectosServidor);
  const cmSesionActiva = useStore((s) => s.cmSesionActiva);
  const setWorkspaces = useStore((s) => s.setWorkspaces);
  const cargarWorkspacesYProyectos = useStore((s) => s.cargarWorkspacesYProyectos);
  const entrarAProyecto = useStore((s) => s.entrarAProyecto);
  const showToast = useStore((s) => s.showToast);
  const abrirVentana = useVentanasProyecto((s) => s.abrir);
  const [vista, setVista] = useState<'lista' | 'clonar' | 'carpeta'>('lista');
  const [aClonar, setAClonar] = useState<string | null>(null);
  const [carpeta, setCarpeta] = useState('');
  // Solo se pide la contraseña cuando el proyecto que se intentó abrir la tiene.
  const [error, setError] = useState<string | null>(null);
  const [ocupado, setOcupado] = useState(false);
  const [recientes, setRecientes] = useState<ProyectoLocal[]>([]);
  const delServidor = proyectosServidor.filter((p) => !workspaces.some((w) => w.repo === p.repo));
  // Si la carpeta elegida es un workspace de Unity VC, no se pide contraseña: se ofrece entrar a él.
  const normalizar = (r: string) => r.trim().replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase();
  const wsDeUnity = carpeta.trim() ? workspaces.find((w) => normalizar(w.ruta) === normalizar(carpeta)) : undefined;
  const proyectosDelMockup = () =>
    workspaces.filter((w) => w.repo.endsWith('@local')).map((w) => ({ nombre: w.nombre, ruta: w.ruta }));

  useEffect(() => {
    if (!open) return;
    setVista('lista');
    setAClonar(null);
    setError(null);
    if (enTauri()) void cargarWorkspacesYProyectos();
    api
      .listarProyectosLocales(proyectosDelMockup)
      .then(setRecientes)
      .catch(() => setRecientes([]));
    // Solo al abrir la ventana.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const cerrar = () => {
    if (ocupado) return;
    setError(null);
    onClose();
  };
  const verCarpeta = (ruta = '') => {
    setCarpeta(ruta);
    setError(null);
    setVista('carpeta');
  };
  const abrirCarpeta = async () => {
    const ruta = carpeta.trim().replace(/[\\/]+$/, '');
    if (!ruta) return setError('Elige la carpeta del proyecto.');
    if (!enTauri()) {
      const ws = workspaces.find((w) => w.repo.endsWith('@local') && w.ruta.toLowerCase() === ruta.toLowerCase());
      if (!ws) return setError('En esa carpeta no hay un proyecto de BranchBoard.');
      cerrar();
      entrarAProyecto(ws.nombre, ws.nombre, ws.ruta);
      showToast(`Abriste ${ws.nombre}`);
      return;
    }
    // Workspace de Unity VC: se entra como con "Traer".
    if (wsDeUnity) {
      cerrar();
      useVentanasProyecto.getState().setTrasClonar(wsDeUnity);
      return;
    }
    setOcupado(true);
    try {
      const entrada = await api.abrirProyectoLocal(ruta, () => {
        throw new ErrorBackend('validacion', 'Solo en la app de escritorio');
      });
      setOcupado(false);
      if (entrada.estado === 'dentro') {
        cerrar();
        terminarEntradaLocal(entrada, ruta);
      }
    } catch (e) {
      setOcupado(false);
      setError(mensajeDeError(e));
    }
  };
  // Archivo .bbproyecto: se elige dónde descomprimirlo y queda en la vista de carpeta, listo para abrir.
  const importar = async () => {
    setError(null);
    if (!enTauri()) {
      showToast('Importar un .bbproyecto solo funciona en la app de escritorio', 'info');
      return;
    }
    try {
      const destino = await api.elegirCarpeta('¿En qué carpeta guardo el proyecto importado?');
      if (!destino) return;
      setOcupado(true);
      const importado = await api.importarProyectoLocal(destino);
      if (!importado) return;
      verCarpeta(importado.ruta);
      showToast(`Proyecto ${importado.nombre} importado. Pulsa Abrir.`);
    } catch (e) {
      setError(mensajeDeError(e));
    } finally {
      setOcupado(false);
    }
  };

  const fila =
    'flex w-full items-center gap-3 rounded-lg border border-border bg-surface-2 px-3 py-2.5 text-left transition hover:border-primary disabled:opacity-60';
  const titulo = (t: string) => (
    <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-faint">{t}</div>
  );
  const avisoError = error && (
    <p className="text-[11px] text-red-300">
      <IconoEnLinea nombre="alerta" sinColor />
      {error}
    </p>
  );
  const volver = (
    <button type="button" className="btn-ghost btn-sm self-start" onClick={() => setVista('lista')} disabled={ocupado}>
      ← Volver
    </button>
  );

  return (
    <Modal
      open={open}
      onClose={cerrar}
      title="Cargar proyecto"
      subtitle={vista === 'lista' ? 'De Unity VC, de una carpeta de tu equipo o de un archivo .bbproyecto' : undefined}
      size="md"
    >
      {vista === 'clonar' && aClonar ? (
        <div className="flex flex-col gap-4">
          {volver}
          <PasoClonar
            nombre={delServidor.find((p) => p.repo === aClonar)?.nombre ?? aClonar}
            repoInicial={aClonar}
            proyectosServidor={delServidor}
            onListo={(ws) => {
              setWorkspaces([...workspaces.filter((w) => w.repo !== ws.repo), ws]);
              cerrar();
              showToast(`${nombreDeRepo(ws)} quedó en ${ws.ruta}`);
              // Se entra directo al proyecto: el menú del proyecto lo configura (como "Traer") y pide la
              // carpeta compartida si falta.
              useVentanasProyecto.getState().setTrasClonar(ws);
            }}
          />
        </div>
      ) : vista === 'carpeta' ? (
        <div className="flex flex-col gap-3 text-sm">
          {volver}
          {recientes.length > 0 && (
            <div>
              <span className="mb-1 block text-xs text-faint">Recientes</span>
              <div className="space-y-1">
                {recientes.map((p) => (
                  <button
                    key={p.ruta}
                    type="button"
                    onClick={() => {
                      setCarpeta(p.ruta);
                      setError(null);
                                      }}
                    className={`flex w-full flex-col items-start rounded-lg border px-3 py-2 text-left transition hover:border-primary ${
                      carpeta === p.ruta ? 'border-primary bg-primary-soft' : 'border-border bg-surface-2'
                    }`}
                  >
                    <span className="text-sm font-medium text-text">{p.nombre}</span>
                    <span className="w-full truncate font-mono text-[11px] text-faint">{p.ruta}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Carpeta del proyecto</span>
            <SelectorCarpeta
              value={carpeta}
              onChange={(v) => {
                setCarpeta(v);
                setError(null);
                          }}
              placeholder="Carpeta del proyecto"
              titulo="Carpeta del proyecto de BranchBoard"
            />
          </label>
          {wsDeUnity && (
            <p className="text-[11px] text-faint">Proyecto de Unity VC: {nombreDeRepo(wsDeUnity)}</p>
          )}
          {avisoError}
          <div className="flex justify-end gap-2 pt-1">
            <button type="button" className="btn-ghost btn-sm" onClick={cerrar} disabled={ocupado}>
              Cancelar
            </button>
            <button type="button" className="btn-primary btn-sm" onClick={() => void abrirCarpeta()} disabled={ocupado}>
              {ocupado ? 'Abriendo…' : 'Abrir'}
            </button>
          </div>
        </div>
      ) : (
        <div className="space-y-5 text-sm">
          <ListaProyectosDelServidor
            abierta={open}
            titulo={titulo}
            alEntrar={(entrada) => {
              cerrar();
              terminarEntradaLocal(entrada, '');
            }}
          />
          <div>
            {titulo('De Unity VC')}
            {!cmSesionActiva ? (
              <button type="button" className={fila} onClick={() => abrirVentana('unity')}>
                <Icono nombre="unity" className="h-5 w-5" />
                <span className="min-w-0 flex-1 text-text">Conecta Unity VC para ver tus proyectos</span>
              </button>
            ) : delServidor.length === 0 ? (
              <p className="text-[12px] text-faint">Ya tienes en este equipo todos tus proyectos de Unity VC.</p>
            ) : (
              <div className="space-y-1.5">
                {delServidor.map((p) => (
                  <button
                    key={p.repo}
                    type="button"
                    className={fila}
                    onClick={() => {
                      setAClonar(p.repo);
                      setVista('clonar');
                    }}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium text-text">{p.nombre}</span>
                      <span className="block truncate font-mono text-[10px] text-faint">{p.repo}</span>
                    </span>
                    <span
                      className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] ${
                        p.tiene_db_branchboard ? 'bg-primary-soft text-primary' : 'bg-surface-3 text-faint'
                      }`}
                    >
                      {etiquetaProyectoServidor(p)}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <div>
            {titulo('Desde tu equipo')}
            <div className="space-y-1.5">
              <button type="button" className={fila} onClick={() => verCarpeta()} disabled={ocupado}>
                <Icono nombre="carpetaAbierta" className="h-5 w-5" />
                <span className="min-w-0 flex-1">
                  <span className="block font-medium text-text">Una carpeta</span>
                  <span className="block text-[11px] text-faint">Un proyecto que ya está como carpeta en tu equipo</span>
                </span>
              </button>
              <button type="button" className={fila} onClick={() => void importar()} disabled={ocupado}>
                <Icono nombre="descargar" className="h-5 w-5" />
                <span className="min-w-0 flex-1">
                  <span className="block font-medium text-text">Un archivo .bbproyecto</span>
                  <span className="block text-[11px] text-faint">Un proyecto que alguien exportó</span>
                </span>
              </button>
            </div>
          </div>
          {avisoError}
        </div>
      )}
    </Modal>
  );
}
