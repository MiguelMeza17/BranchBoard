import { useEffect, useState, type ReactNode } from 'react';
import { api, enTauri, ErrorBackend, mensajeDeError, type EntradaProyecto, type ProyectoDeLaCuenta } from '../lib/api';
import { Icono } from './Icono';

type EntradaDentro = Extract<EntradaProyecto, { estado: 'dentro' }>;

/** Los proyectos sin Unity VC de la cuenta en el servidor (#240), pedidos cada vez que `abierta` pasa a
 * `true`. Incluye altas locales pendientes de subir, también sin conexión. */
export function useProyectosDelServidor(abierta: boolean): ProyectoDeLaCuenta[] {
  const [proyectos, setProyectos] = useState<ProyectoDeLaCuenta[]>([]);
  useEffect(() => {
    if (!abierta || !enTauri()) return;
    api
      .listarProyectosDelServidor()
      .then((l) => setProyectos(l.filter((p) => !p.unity_repo)))
      .catch(() => setProyectos([]));
  }, [abierta]);
  return proyectos;
}

const fila =
  'flex w-full items-center gap-3 rounded-xl border border-border bg-surface-2 px-3 py-2.5 text-left transition hover:border-primary hover:bg-primary-soft disabled:opacity-60';

/**
 * "En el servidor" de "Cargar proyecto" (#240): los proyectos sin Unity VC de la cuenta. Se abren desde
 * cualquier PC con la cuenta (sin carpeta, sin archivo exportado). Los de Unity VC se cargan por su repo.
 */
export function ListaProyectosDelServidor({
  abierta,
  titulo,
  alEntrar,
}: {
  abierta: boolean;
  titulo: (t: string) => ReactNode;
  alEntrar: (entrada: EntradaDentro) => void;
}) {
  const proyectos = useProyectosDelServidor(abierta);
  const [abriendo, setAbriendo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Sin sesión del servidor, sin red o fuera de la app de escritorio no se muestra nada.
  if (proyectos.length === 0) return null;

  const abrir = async (p: ProyectoDeLaCuenta) => {
    setAbriendo(p.id);
    setError(null);
    try {
      const entrada = await api.abrirProyectoServidor(p.id, () => {
        throw new ErrorBackend('validacion', 'Solo en la app de escritorio');
      });
      if (entrada.estado === 'dentro') alEntrar(entrada);
      else setError(`Ya no estás en el equipo de ${p.nombre}.`);
    } catch (e) {
      setError(mensajeDeError(e));
    } finally {
      setAbriendo(null);
    }
  };

  return (
    <div>
      {titulo('Tus proyectos de BranchBoard')}
      <div className="space-y-1.5">
        {proyectos.map((p) => (
          <button key={p.id} type="button" className={fila} disabled={abriendo !== null} onClick={() => void abrir(p)}>
            <Icono nombre="carpetaAbierta" className="h-5 w-5" />
            <span className="min-w-0 flex-1 truncate font-medium text-text">{p.nombre}</span>
            <span className="shrink-0 rounded-full bg-primary-soft px-2 py-0.5 text-[10px] text-primary">
              {abriendo === p.id ? 'Abriendo…' : p.rol}
            </span>
          </button>
        ))}
      </div>
      {error && <p className="mt-1.5 text-[11px] text-red-300">{error}</p>}
    </div>
  );
}

/**
 * "En el servidor" de "Crear un nuevo proyecto" (#240): solo el nombre. Queda en el servidor con la cuenta
 * de BranchBoard como Administradora y se abre desde cualquier PC. Necesita la sesión del servidor.
 */
export function CrearProyectoEnServidor({
  alEntrar,
  ocupado,
}: {
  alEntrar: (entrada: EntradaDentro) => void;
  ocupado?: boolean;
}) {
  const [nombre, setNombre] = useState('');
  const [creando, setCreando] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!enTauri()) return null;

  const crear = async () => {
    if (!nombre.trim()) return setError('Escribe el nombre del proyecto.');
    setCreando(true);
    setError(null);
    try {
      const entrada = await api.crearProyectoServidor(nombre.trim(), () => {
        throw new ErrorBackend('validacion', 'Solo en la app de escritorio');
      });
      if (entrada.estado === 'dentro') {
        setNombre('');
        alEntrar(entrada);
      }
    } catch (e) {
      setError(mensajeDeError(e));
    } finally {
      setCreando(false);
    }
  };

  return (
    <div className="space-y-2 rounded-xl border border-primary/40 bg-primary-soft/40 p-3">
      <div className="text-xs font-semibold text-text">En el servidor</div>
      <div className="flex gap-2">
        <input
          className="input min-w-0 flex-1"
          value={nombre}
          onChange={(e) => setNombre(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void crear();
          }}
          placeholder="Nombre del proyecto"
          disabled={creando || ocupado}
        />
        <button type="button" className="btn-primary btn-sm" onClick={() => void crear()} disabled={creando || ocupado}>
          {creando ? 'Creando…' : 'Crear'}
        </button>
      </div>
      {error && <p className="text-[11px] text-red-300">{error}</p>}
    </div>
  );
}
