import { useEffect, useMemo, useState } from 'react';
import { entrantesDe, useStore } from '../store/useStore';
import type { Tarea } from '../types';
import { api, enTauri, type CambiosEntrantes } from './api';

/**
 * Cambios entrantes (incoming changes, #110) de la rama de una tarea, vistos desde el workspace de
 * esta máquina. Se unen **todos juntos** (`cm update .`): Unity VC no deja unir una sola subtarea.
 *
 * - En el navegador (mockup) salen de las subtareas terminadas que no se han unido.
 * - En la app de escritorio se le preguntan al backend cada vez que cambian los datos.
 */
export interface Entrantes {
  /** Subtareas cuyo merge todavía no está en el workspace. */
  subtareas: Tarea[];
  /** Changesets que no vienen de una subtarea reconocida (ej. un merge hecho a mano en Unity VC). */
  otros: number;
  /** Si el workspace está en esta rama. Si no, no se puede saber qué le falta ni unirlo. */
  enRama: boolean;
  ramaActual: string | null;
  cargando: boolean;
  uniendo: boolean;
  hayAlgo: boolean;
  /** Une todo lo pendiente de la rama. */
  unir: () => Promise<boolean>;
}

export function useEntrantes(tarea: Tarea, activo = true): Entrantes {
  const tareas = useStore((s) => s.tareas);
  const recibidos = useStore((s) => s.entrantesRecibidos);
  const versionDatos = useStore((s) => s.versionDatos);
  const unirEntrantes = useStore((s) => s.unirEntrantes);
  const real = enTauri();
  const [datos, setDatos] = useState<CambiosEntrantes | null>(null);
  const [cargando, setCargando] = useState(false);
  const [uniendo, setUniendo] = useState(false);

  useEffect(() => {
    if (!real || !activo || !tarea.rama_asociada) return;
    let vigente = true;
    setCargando(true);
    api
      .cambiosEntrantes(tarea.id, () => {
        throw new Error('solo en la app de escritorio');
      })
      .then((d) => vigente && setDatos(d))
      .catch(() => vigente && setDatos(null))
      .finally(() => vigente && setCargando(false));
    return () => {
      vigente = false;
    };
  }, [real, activo, tarea.id, tarea.rama_asociada, versionDatos]);

  const vista = useMemo(() => {
    if (!real) {
      const subtareas = entrantesDe(tareas, recibidos, tarea.id);
      return { subtareas, otros: 0, enRama: true, ramaActual: tarea.rama_asociada };
    }
    if (!datos) return { subtareas: [], otros: 0, enRama: true, ramaActual: null };
    const ids = [...new Set(datos.changesets.map((c) => c.tarea_origen).filter((id): id is string => !!id))];
    const subtareas = ids.map((id) => tareas.find((t) => t.id === id)).filter((t): t is Tarea => !!t);
    const conocidas = new Set(subtareas.map((t) => t.id));
    const otros = datos.changesets.filter((c) => !c.tarea_origen || !conocidas.has(c.tarea_origen)).length;
    return { subtareas, otros, enRama: datos.en_rama, ramaActual: datos.rama_actual };
  }, [real, datos, tareas, recibidos, tarea.id, tarea.rama_asociada]);

  return {
    ...vista,
    cargando,
    uniendo,
    hayAlgo: vista.subtareas.length + vista.otros > 0,
    unir: async () => {
      setUniendo(true);
      const res = await unirEntrantes(tarea.id);
      setUniendo(false);
      if (res && real) setDatos(res);
      return !!res;
    },
  };
}
