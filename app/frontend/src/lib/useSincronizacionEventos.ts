import { useEffect } from 'react';
import { useStore } from '../store/useStore';
import { api, enTauri } from './api';

/** Cada cuánto se revisa `ops/` mientras la app está abierta (BACKEND.md: 2 minutos). */
export const INTERVALO_SINCRONIZACION_MS = 2 * 60 * 1000;

/**
 * Trae los cambios de las demás máquinas (`sincronizar_eventos`, #92): al entrar al proyecto, al volver
 * a la ventana y cada 2 minutos. Si llegó algo nuevo, sube `versionDatos` en el store: las pantallas
 * que leen del backend lo usan como dependencia para volver a pedir sus datos solas.
 * También escucha el polling de Unity VC (`unity-vc-actualizado`) para mantener al día commits y rama.
 * Un fallo no se muestra cada 2 minutos: se reintenta en la próxima revisión.
 */
export function useSincronizacionEventos(activo: boolean) {
  const subirVersionDatos = useStore((s) => s.subirVersionDatos);
  const aplicarResumenPolling = useStore((s) => s.aplicarResumenPolling);
  const mostrarAlertaRamaAjena = useStore((s) => s.mostrarAlertaRamaAjena);

  useEffect(() => {
    if (!activo) return;
    let cancelado = false;
    let dejarUnity: (() => void) | undefined;
    let dejarAlerta: (() => void) | undefined;
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

    if (enTauri()) {
      void api
        .alUnityActualizado((resumen) => {
          if (!cancelado) void aplicarResumenPolling(resumen);
        })
        .then((dejar) => {
          if (cancelado) {
            dejar();
          } else {
            dejarUnity = dejar;
          }
        });
      // Cambios en main, dev o la rama de otra persona en este workspace (#111).
      void api
        .alAlertaRamaAjena((alerta) => {
          if (!cancelado) mostrarAlertaRamaAjena(alerta);
        })
        .then((dejar) => {
          if (cancelado) {
            dejar();
          } else {
            dejarAlerta = dejar;
          }
        });
    }

    void revisar();
    const temporizador = window.setInterval(() => void revisar(), INTERVALO_SINCRONIZACION_MS);
    window.addEventListener('focus', alVolver);
    document.addEventListener('visibilitychange', alVolver);
    return () => {
      cancelado = true;
      dejarUnity?.();
      dejarAlerta?.();
      window.clearInterval(temporizador);
      window.removeEventListener('focus', alVolver);
      document.removeEventListener('visibilitychange', alVolver);
    };
  }, [activo, subirVersionDatos, aplicarResumenPolling, mostrarAlertaRamaAjena]);
}
