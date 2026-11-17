import { useEffect, useState } from 'react';
import { useStore } from '../store/useStore';
import {
  esLiderOAdmin,
  puedeCompactarEventos,
  puedeEliminarProyecto,
  puedeRestaurarBackup,
} from '../lib/permisos';
import { PageHeader, EmptyState } from '../components/primitives';
import { Seccion } from '../components/Seccion';
import { ConfirmLock } from '../components/ConfirmLock';
import { Modal } from '../components/Modal';
import { SelectorCarpeta } from '../components/SelectorCarpeta';
import { api, mensajeDeError, type InfoActualizacion } from '../lib/api';

export function Ajustes() {
  const user = useStore((s) => s.currentUser());

  return (
    <div>
      <PageHeader title="Ajustes" desc="Acciones administrativas del proyecto. Cada sección aparece según tu rol." />
      <div className="space-y-3">
        <SeccionActualizar />
        <SeccionCarpetaCompartida />
        <SeccionBackups />
        {puedeEliminarProyecto(user.rol) && <SeccionProyecto />}
        {!esLiderOAdmin(user.rol) && (
          <EmptyState icon="candado" title="Sin acciones administrativas" desc="Con tu rol actual no hay ajustes que gestionar aparte de reportar problemas de BD." />
        )}
      </div>
    </div>
  );
}

// Actualizar versión: visible para todos los roles. La app revisa si hay una versión nueva,
// la descarga y se reinstala encima sin perder datos.
function SeccionActualizar() {
  const showToast = useStore((s) => s.showToast);
  const [version, setVersion] = useState('');
  const [info, setInfo] = useState<InfoActualizacion | null>(null);
  const [estado, setEstado] = useState<'reposo' | 'buscando' | 'instalando'>('reposo');
  const [porcentaje, setPorcentaje] = useState<number | null>(null);

  useEffect(() => {
    void api.versionActual().then(setVersion);
  }, []);

  const buscar = async () => {
    setEstado('buscando');
    try {
      setInfo(await api.buscarActualizacion());
    } catch (e) {
      showToast(mensajeDeError(e), 'danger');
    }
    setEstado('reposo');
  };

  const instalar = async () => {
    setEstado('instalando');
    setPorcentaje(null);
    try {
      await api.instalarActualizacion((p) => setPorcentaje(p.total ? Math.round((p.descargado / p.total) * 100) : null));
    } catch (e) {
      showToast(mensajeDeError(e), 'danger');
      setEstado('reposo');
    }
  };

  const hayNueva = !!info?.version_nueva;

  return (
    <Seccion title="Actualizar versión">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="text-sm">
          <span className="text-muted">Versión instalada</span>
          <span className="ml-2 font-mono text-text">{version}</span>
          {info && !hayNueva && <span className="chip ml-3 border border-primary/40 bg-primary/15 text-primary">Al día</span>}
        </div>
        <button className="btn-outline btn-sm" disabled={estado !== 'reposo'} onClick={buscar}>
          {estado === 'buscando' ? 'Buscando…' : 'Buscar actualizaciones'}
        </button>
      </div>

      {hayNueva && (
        <div className="mt-4 rounded-lg border border-primary/50 bg-primary-soft p-4">
          <div className="text-sm font-semibold text-text">
            Versión nueva <span className="font-mono">{info?.version_nueva}</span>
          </div>
          {info?.notas && <p className="mt-1 whitespace-pre-line text-sm text-muted">{info.notas}</p>}
          {estado === 'instalando' ? (
            <div className="mt-3" role="status" aria-live="polite">
              <div className="h-2 overflow-hidden rounded-full bg-surface-3">
                <div className="h-full bg-primary transition-all" style={{ width: `${porcentaje ?? 5}%` }} />
              </div>
              <div className="mt-1 text-xs text-faint">{porcentaje === null ? 'Descargando…' : `Descargando… ${porcentaje}%`}</div>
            </div>
          ) : (
            <button className="btn-primary btn-sm mt-3" onClick={instalar}>Actualizar y reiniciar</button>
          )}
        </div>
      )}
    </Seccion>
  );
}

// Carpeta compartida (Drive/OneDrive) del proyecto en esta máquina: todos los roles. Se elige al entrar
// por primera vez; aquí se ve y se cambia (ej. si se movió la carpeta de OneDrive). El Administrador
// además puede compactar en un snapshot los eventos de más de 30 días (#94).
function SeccionCarpetaCompartida() {
  const user = useStore((s) => s.currentUser());
  const proyecto = useStore((s) => s.proyectoActual);
  const carpetasCompartidas = useStore((s) => s.carpetasCompartidas);
  const setCarpetaCompartida = useStore((s) => s.setCarpetaCompartida);
  const showToast = useStore((s) => s.showToast);
  const [actual, setActual] = useState<string | null>(null);
  const [ruta, setRuta] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [guardando, setGuardando] = useState(false);
  const [confirmarCompactar, setConfirmarCompactar] = useState(false);
  const [compactando, setCompactando] = useState(false);

  useEffect(() => {
    api
      .carpetaCompartida(() => carpetasCompartidas[proyecto] ?? null)
      .then((r) => {
        setActual(r);
        setRuta(r ?? '');
      })
      .catch((e) => setError(mensajeDeError(e)));
  }, [proyecto, carpetasCompartidas]);

  const guardar = async () => {
    if (!ruta.trim()) return setError('Elige una carpeta.');
    setGuardando(true);
    try {
      const guardada = await api.configurarCarpetaCompartida(ruta.trim(), () => ruta.trim());
      setCarpetaCompartida(proyecto, guardada);
      showToast('Carpeta compartida guardada');
    } catch (e) {
      setError(mensajeDeError(e));
    }
    setGuardando(false);
  };

  const compactar = async () => {
    setConfirmarCompactar(false);
    setCompactando(true);
    try {
      const r = await api.compactarEventos();
      if (r.archivo) {
        showToast(`Snapshot ${r.archivo} creado (${r.compactados} eventos compactados)`);
      } else {
        showToast('No hay eventos de más de 30 días por compactar', 'info');
      }
    } catch (e) {
      showToast(mensajeDeError(e), 'danger');
    }
    setCompactando(false);
  };

  return (
    <Seccion title="Carpeta compartida">
      <div className="space-y-2 text-sm">
        <div className="text-muted">
          Actual: <span className="font-mono text-xs text-text">{actual ?? 'sin configurar'}</span>
        </div>
        <SelectorCarpeta
          value={ruta}
          onChange={(v) => {
            setRuta(v);
            setError(null);
          }}
          placeholder="Carpeta de Drive/OneDrive"
          titulo={`Carpeta compartida de ${proyecto}`}
        />
        {error && <p className="text-[11px] text-red-300">{error}</p>}
        <div className="flex flex-wrap items-center justify-between gap-2">
          {puedeCompactarEventos(user.rol) ? (
            <button
              className="btn-outline btn-sm"
              disabled={compactando || !actual}
              onClick={() => setConfirmarCompactar(true)}
            >
              {compactando ? 'Compactando…' : 'Compactar eventos (+30 días)'}
            </button>
          ) : (
            <span />
          )}
          <button
            className="btn-outline btn-sm"
            disabled={guardando || !ruta.trim() || ruta.trim() === actual}
            onClick={() => void guardar()}
          >
            {guardando ? 'Guardando…' : 'Guardar'}
          </button>
        </div>
      </div>
      {puedeCompactarEventos(user.rol) && (
        <ConfirmLock
          open={confirmarCompactar}
          onClose={() => setConfirmarCompactar(false)}
          onConfirm={() => void compactar()}
          title="Compactar eventos"
          tone="primary"
          palabra="compactar"
          confirmLabel="Compactar eventos"
          description="Confirma que el equipo se sincronizó hace poco antes de compactar los eventos de más de 30 días."
        />
      )}
    </Seccion>
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
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const workspaces = useStore((s) => s.workspaces);
  const workspaceActual = useStore((s) => s.workspaceActual);
  const setWorkspaces = useStore((s) => s.setWorkspaces);
  const cambiarDeProyecto = useStore((s) => s.cambiarDeProyecto);
  const cambio = nombre.trim() && nombre.trim() !== proyecto;

  return (
    <Seccion title="Proyecto" danger>
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
          Elimina el repositorio del proyecto en Unity VC. Requiere escribir el nombre exacto del proyecto y permiso{' '}
          <span className="font-mono">rmrepository</span> en Unity Version Control.
        </p>
        <p className="mt-2 text-xs text-muted">Las carpetas del equipo y las copias locales no se borran.</p>
        <button className="btn-danger mt-3" onClick={() => { setError(null); setOpen(true); }}>Eliminar proyecto completo</button>
      </div>
      <ConfirmLock
        open={open}
        onClose={() => setOpen(false)}
        onConfirm={async (_, confirmacion) => {
          if (ocupado) return;
          setOcupado(true);
          setError(null);
          try {
            const resultado = await api.eliminarProyecto(confirmacion ?? '', () => ({
              repo: workspaces.find((w) => w.nombre === workspaceActual)?.repo ?? '',
              nombre: proyecto,
            }));
            setWorkspaces(workspaces.filter((w) => w.repo !== resultado.repo));
            setOpen(false);
            cambiarDeProyecto();
            showToast(`Proyecto ${resultado.nombre} eliminado de Unity VC`, 'info');
          } catch (e) {
            setError(mensajeDeError(e));
          } finally {
            setOcupado(false);
          }
        }}
        title="Eliminar proyecto completo"
        tone="danger"
        palabra={proyecto}
        exacta
        ocupado={ocupado}
        error={error}
        confirmLabel="Eliminar proyecto"
        description={<>Escribe el nombre exacto del proyecto <strong className="font-mono text-text">{proyecto}</strong> para confirmar. Esta es la acción más destructiva de la app.</>}
      />
    </Seccion>
  );
}
