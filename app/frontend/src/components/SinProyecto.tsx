import { useEffect, useState } from 'react';
import { create } from 'zustand';
import { useStore } from '../store/useStore';
import { api, enTauri, ErrorBackend, mensajeDeError, type EntradaProyecto, type ProyectoLocal } from '../lib/api';
import { SelectorCarpeta } from './SelectorCarpeta';
import { LoginUnityVCModal } from './LoginUnityVCModal';
import { EntradaClave } from './EntradaClave';
import { Modal } from './Modal';
import { Icono, IconoEnLinea } from './Icono';

// Entrar a BranchBoard sin proyecto: tras iniciar sesión con la cuenta de BranchBoard no se crea nada ni se
// asigna rol. Todo queda vacío con el aviso "No estás en ningún proyecto" y, desde ahí o desde el menú del
// proyecto (clic en el logo), se crea o abre un proyecto sin Unity VC o se conecta Unity VC.

type EntradaDentro = Extract<EntradaProyecto, { estado: 'dentro' }>;
type Ventana = 'crear' | 'abrir' | 'unity' | null;

/** Qué ventana de "sin proyecto" está abierta. La usan el aviso, el menú del proyecto y el Layout. */
export const useVentanasProyecto = create<{ abierta: Ventana; abrir: (v: Ventana) => void }>((set) => ({
  abierta: null,
  abrir: (abierta) => set({ abierta }),
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
        { email: entrada.email, nombre, apellido, rol: entrada.rol, fecha_alta: '', firma: 'backend', firmado_en: null },
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
  const cmSesionActiva = useStore((s) => s.cmSesionActiva);
  const cambiarDeProyecto = useStore((s) => s.cambiarDeProyecto);
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
        <button type="button" onClick={() => abrir('abrir')} className={tarjeta}>
          <Icono nombre="carpetaAbierta" className="h-10 w-10 transition group-hover:scale-105" />
          <span className="text-sm font-semibold text-text">Carga un proyecto</span>
          <span className="text-[11px] text-faint">Búscalo en una carpeta de tu equipo</span>
        </button>
      </div>
      <button
        type="button"
        onClick={() => (cmSesionActiva && enTauri() ? cambiarDeProyecto() : abrir('unity'))}
        className="w-full rounded-xl border border-border px-4 py-2.5 text-sm text-muted transition hover:border-border-strong hover:text-text"
      >
        {cmSesionActiva && enTauri() ? 'Mis proyectos de Unity VC' : 'Conectar con Unity VC'}
      </button>
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
      <ModalAbrirLocal open={abierta === 'abrir'} onClose={cerrar} />
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
    clave: '',
  });
  const [nuevo, setNuevo] = useState(vacio);
  useEffect(() => {
    if (open) setNuevo(vacio());
    // Solo al abrir la ventana.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  const [creando, setCreando] = useState(false);
  const [errorGeneral, setErrorGeneral] = useState<string | null>(null);
  type CampoNuevo = 'carpeta' | 'proyecto' | 'nombre' | 'correo' | 'clave';
  const correoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(nuevo.correo.trim());
  const errorNuevo: Record<CampoNuevo, string | null> = {
    // En el mockup no hay disco: la carpeta solo se pide en la app de escritorio.
    carpeta: !enTauri() || nuevo.carpeta.trim() ? null : 'Elige dónde guardar el proyecto.',
    proyecto: nuevo.proyecto.trim() ? null : 'Escribe el nombre del proyecto.',
    nombre: nuevo.nombre.trim() ? null : 'Escribe tu nombre.',
    correo: !nuevo.correo.trim() ? 'Escribe tu correo.' : correoOk ? null : 'El correo no es válido (ej. nombre@correo.com).',
    clave: !nuevo.clave
      ? 'Escribe una contraseña.'
      : nuevo.clave.length < 8
        ? 'La contraseña debe tener al menos 8 caracteres.'
        : null,
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
      setTocados(new Set(['carpeta', 'proyecto', 'nombre', 'correo', 'clave']));
      return;
    }
    const datos = {
      carpeta: nuevo.carpeta.trim(),
      proyecto: nuevo.proyecto.trim(),
      nombre: nuevo.nombre.trim(),
      correo: nuevo.correo.trim(),
      clave: nuevo.clave,
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
        <label className="block">
          <span className="mb-1 block text-xs text-faint">Contraseña del proyecto (mínimo 8 caracteres)</span>
          <EntradaClave
            className={`input ${errorVisible('clave') ? 'border-red-500/70' : ''}`}
            type="password"
            autoComplete="new-password"
            value={nuevo.clave}
            onBlur={() => tocar('clave')}
            onChange={(e) => setNuevo({ ...nuevo, clave: e.target.value })}
          />
          {aviso(errorVisible('clave'))}
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

function ModalAbrirLocal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const workspaces = useStore((s) => s.workspaces);
  const entrarAProyecto = useStore((s) => s.entrarAProyecto);
  const showToast = useStore((s) => s.showToast);
  const [carpeta, setCarpeta] = useState('');
  const [clave, setClave] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [ocupado, setOcupado] = useState(false);
  const [recientes, setRecientes] = useState<ProyectoLocal[]>([]);
  const proyectosDelMockup = () =>
    workspaces.filter((w) => w.repo.endsWith('@local')).map((w) => ({ nombre: w.nombre, ruta: w.ruta }));
  useEffect(() => {
    if (!open) return;
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
    setClave('');
    onClose();
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
    if (!clave) return setError('Escribe la contraseña del proyecto.');
    setOcupado(true);
    try {
      const entrada = await api.abrirProyectoLocal(ruta, clave, () => {
        throw new ErrorBackend('validacion', 'Solo en la app de escritorio');
      });
      setOcupado(false);
      if (entrada.estado === 'dentro') {
        cerrar();
        terminarEntradaLocal(entrada, ruta);
      }
    } catch (e) {
      setError(mensajeDeError(e));
      setOcupado(false);
    }
  };
  // Importar un `.bbproyecto`: se elige dónde descomprimirlo y queda listo para abrir con su contraseña.
  const importar = async () => {
    setError(null);
    try {
      const destino = await api.elegirCarpeta('¿En qué carpeta guardo el proyecto importado?');
      if (!destino) return;
      setOcupado(true);
      const importado = await api.importarProyectoLocal(destino);
      if (!importado) return;
      setCarpeta(importado.ruta);
      setClave('');
      showToast(`Proyecto ${importado.nombre} importado. Escribe su contraseña para abrirlo.`);
    } catch (e) {
      setError(mensajeDeError(e));
    } finally {
      setOcupado(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={cerrar}
      title="Abrir un proyecto de BranchBoard"
      subtitle="Elige la carpeta de un proyecto sin Unity VC"
      size="sm"
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          {enTauri() ? (
            <button type="button" className="btn-ghost btn-sm" onClick={() => void importar()} disabled={ocupado}>
              Importar .bbproyecto…
            </button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <button type="button" className="btn-ghost btn-sm" onClick={cerrar} disabled={ocupado}>
              Cancelar
            </button>
            <button type="button" className="btn-primary btn-sm" onClick={() => void abrirCarpeta()} disabled={ocupado}>
              {ocupado ? 'Abriendo…' : 'Abrir'}
            </button>
          </div>
        </div>
      }
    >
      <div className="space-y-3 text-sm">
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
        {enTauri() && (
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Contraseña del proyecto</span>
            <EntradaClave
              className="input"
              type="password"
              autoComplete="current-password"
              value={clave}
              onFocus={() => setError(null)}
              onChange={(e) => {
                setClave(e.target.value);
                setError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void abrirCarpeta();
              }}
            />
          </label>
        )}
        {error && (
          <p className="text-[11px] text-red-300">
            <IconoEnLinea nombre="alerta" sinColor />
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
