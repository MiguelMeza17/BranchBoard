import { useEffect } from 'react';
import { useStore } from '../store/useStore';
import { api } from './api';

/** Cada cuánto se revisa `ops/` mientras la app está abierta (BACKEND.md: 2 minutos). */
export const INTERVALO_SINCRONIZACION_MS = 2 * 60 * 1000;

/**
 * Trae los cambios de las demás máquinas (`sincronizar_eventos`, #92): al entrar al proyecto, al volver
 * a la ventana y cada 2 minutos. Si llegó algo nuevo, sube `versionDatos` en el store: las pantallas
 * que leen del backend lo usan como dependencia para volver a pedir sus datos solas.
 * Un fallo no se muestra cada 2 minutos: se reintenta en la próxima revisión.
 */
export function useSincronizacionEventos(activo: boolean) {
  const subirVersionDatos = useStore((s) => s.subirVersionDatos);

  useEffect(() => {
    if (!activo) return;
    let cancelado = false;
    const revisar = async () => {
      try {
        const resumen = await api.sincronizarEventos();
        if (!cancelado && resumen.aplicados > 0) subirVersionDatos();
      } catch (e) {
        console.warn('No se pudieron traer los cambios del equipo', e);
      }
    };
    const alVolver = () => {
      if (document.visibilityState === 'visible') void revisar();
    };

    void revisar();
    const temporizador = window.setInterval(() => void revisar(), INTERVALO_SINCRONIZACION_MS);
    window.addEventListener('focus', alVolver);
    document.addEventListener('visibilitychange', alVolver);
    return () => {
      cancelado = true;
      window.clearInterval(temporizador);
      window.removeEventListener('focus', alVolver);
      document.removeEventListener('visibilitychange', alVolver);
    };
  }, [activo, subirVersionDatos]);
}
