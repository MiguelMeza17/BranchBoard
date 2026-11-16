import { useEffect, useState } from 'react';
import { api, enTauri, mensajeDeError } from '../lib/api';
import { Modal } from './Modal';
import { Icono } from './Icono';

// "Iniciar sesión con Unity VC". BranchBoard no tiene login propio: la identidad es la sesión de
// Unity Version Control (`cm whoami`). En escritorio llama a `iniciar_sesion_unity_vc` (que abre
// `plastic.exe --configure` para autenticación oficial SSO en navegador); BranchBoard nunca ve ni
// guarda la contraseña. En navegador (mockup) simula el flujo.
type Paso = 'elegir' | 'correo' | 'esperando' | 'tauri-listo' | 'tauri-error';

export function LoginUnityVCModal({
  open,
  onClose,
  onSesionIniciada,
  correoDemo,
  forzarNueva = false,
}: {
  open: boolean;
  onClose: () => void;
  /** Correo con el que quedó la sesión (lo que devolverá `cm whoami`). */
  onSesionIniciada: (correo: string) => void;
  /** Mockup: correo con el que "entra" la opción de cuenta de Unity. */
  correoDemo: string;
  /** Si es `true` ("Cambiar de cuenta"), abre `plastic --configure` aunque ya exista sesión. */
  forzarNueva?: boolean;
}) {
  const [paso, setPaso] = useState<Paso>('elegir');
  const [correo, setCorreo] = useState('');
  const [resultado, setResultado] = useState('');
  const [errorReal, setErrorReal] = useState<string | null>(null);
  const esTauri = enTauri();

  useEffect(() => {
    if (open) {
      setPaso(esTauri ? 'tauri-listo' : 'elegir');
      setCorreo('');
      setErrorReal(null);
    }
  }, [open, esTauri]);

  // En navegador (mockup): simula la espera mientras se completa el inicio de sesión.
  useEffect(() => {
    if (esTauri || paso !== 'esperando') return;
    const t = setTimeout(() => onSesionIniciada(resultado), 1400);
    return () => clearTimeout(t);
  }, [esTauri, paso, resultado, onSesionIniciada]);

  const esperar = (c: string) => {
    setResultado(c);
    setPaso('esperando');
  };
  const correoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(correo.trim());

  const iniciarReal = async (forzar: boolean) => {
    setErrorReal(null);
    setPaso('esperando');
    try {
      const res = await api.iniciarSesionUnityVc(forzar);
      if (res.activa && res.email) {
        onSesionIniciada(res.email);
      } else {
        setErrorReal('No se detectó una sesión iniciada en Unity VC. Completa el asistente e intenta verificar.');
        setPaso('tauri-error');
      }
    } catch (e) {
      setErrorReal(mensajeDeError(e));
      setPaso('tauri-error');
    }
  };

  const verificarAhoraReal = async () => {
    setErrorReal(null);
    try {
      const res = await api.sesionUnityVc();
      if (res.activa && res.email) {
        onSesionIniciada(res.email);
      } else {
        setErrorReal('Unity VC todavía no reporta una sesión activa (`cm whoami`).');
        setPaso('tauri-error');
      }
    } catch (e) {
      setErrorReal(mensajeDeError(e));
      setPaso('tauri-error');
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Iniciar sesión en Unity Version Control"
      subtitle="Al iniciar sesión verás todos tus proyectos de Unity VC"
      size="sm"
    >
      <div className="space-y-4 text-sm">
        {esTauri && paso === 'tauri-listo' && (
          <div className="space-y-3">
            <p className="text-xs text-muted">
              Se abrirá el asistente oficial de Unity Version Control (<code className="font-mono">plastic --configure</code>)
              para iniciar sesión en tu navegador. BranchBoard nunca ve ni guarda tu contraseña.
            </p>
            <button
              type="button"
              className="btn-primary w-full"
              onClick={() => void iniciarReal(forzarNueva)}
            >
              Abrir inicio de sesión de Unity VC
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm w-full"
              onClick={() => void verificarAhoraReal()}
            >
              Ya inicié sesión, verificar con cm whoami
            </button>
          </div>
        )}

        {esTauri && paso === 'tauri-error' && (
          <div className="space-y-3">
            <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200">
              {errorReal ?? 'No se pudo completar el inicio de sesión.'}
            </div>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                className="btn-primary btn-sm flex-1"
                onClick={() => void iniciarReal(true)}
              >
                Reintentar inicio de sesión
              </button>
              <button
                type="button"
                className="btn-outline btn-sm"
                onClick={() => void verificarAhoraReal()}
              >
                Verificar ahora
              </button>
            </div>
          </div>
        )}

        {!esTauri && paso === 'elegir' && (
          <div className="space-y-2">
            <button
              type="button"
              className="flex w-full items-center justify-center gap-2 rounded-lg border border-border bg-text px-4 py-2.5 font-medium text-surface transition hover:opacity-90"
              onClick={() => esperar(correoDemo)}
            >
              <Icono nombre="llave" className="h-4 w-4" /> Con tu cuenta de Unity
            </button>
            <button
              type="button"
              className="flex w-full items-center justify-center gap-2 rounded-lg border border-border bg-surface-2 px-4 py-2.5 font-medium text-text transition hover:border-border-strong"
              onClick={() => setPaso('correo')}
            >
              <Icono nombre="sobre" className="h-4 w-4" /> Con correo electrónico
            </button>
          </div>
        )}

        {!esTauri && paso === 'correo' && (
          <div className="space-y-2">
            <label className="block">
              <span className="mb-1 block text-xs text-faint">Correo</span>
              <input
                className="input"
                type="email"
                autoFocus
                placeholder="tu@correo.com"
                value={correo}
                onChange={(e) => setCorreo(e.target.value)}
              />
            </label>
            <p className="text-[11px] text-faint">
              La contraseña se escribe en la página de Unity que se abre en tu navegador, no aquí.
            </p>
            <div className="flex gap-2">
              <button type="button" className="btn-ghost btn-sm" onClick={() => setPaso('elegir')}>
                ← Volver
              </button>
              <button
                type="button"
                className="btn-primary btn-sm flex-1"
                disabled={!correoOk}
                onClick={() => esperar(correo.trim())}
              >
                Continuar
              </button>
            </div>
          </div>
        )}

        {paso === 'esperando' && (
          <div className="flex flex-col items-center gap-2 rounded-lg border border-border bg-surface-2 px-4 py-5 text-center">
            <span className="h-6 w-6 animate-spin rounded-full border-2 border-border border-t-primary" />
            <p className="text-text">Completa el inicio de sesión en tu navegador…</p>
            <p className="text-[11px] text-faint">BranchBoard sigue en cuanto Unity VC confirme la sesión.</p>
            {esTauri && (
              <button
                type="button"
                className="btn-outline btn-sm mt-2"
                onClick={() => void verificarAhoraReal()}
              >
                Ya terminé, verificar sesión
              </button>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}
