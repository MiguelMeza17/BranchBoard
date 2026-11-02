import { useEffect, useState } from 'react';
import { Modal } from './Modal';

// "Iniciar sesión con Unity VC". BranchBoard no tiene login propio: la identidad es la sesión de
// Unity Version Control (`cm whoami`). Esta ventana solo inicia esa sesión, igual que el cliente de
// Unity VC: con la cuenta de Unity o con correo. En los dos casos el inicio de sesión se completa en
// el navegador, en la página oficial de Unity; BranchBoard nunca ve ni guarda la contraseña.
// No se elige servidor ni proyecto: la sesión es de tu cuenta, y al iniciarla se listan todos los
// proyectos que tienes en Unity VC. En el mockup se simula: tras un momento "esperando al
// navegador", la sesión queda iniciada.
type Paso = 'elegir' | 'correo' | 'esperando';

export function LoginUnityVCModal({
  open,
  onClose,
  onSesionIniciada,
  correoDemo,
}: {
  open: boolean;
  onClose: () => void;
  /** Correo con el que quedó la sesión (lo que devolverá `cm whoami`). */
  onSesionIniciada: (correo: string) => void;
  /** Mockup: correo con el que "entra" la opción de cuenta de Unity. */
  correoDemo: string;
}) {
  const [paso, setPaso] = useState<Paso>('elegir');
  const [correo, setCorreo] = useState('');
  const [resultado, setResultado] = useState('');

  useEffect(() => {
    if (open) {
      setPaso('elegir');
      setCorreo('');
    }
  }, [open]);

  // Simula la espera mientras la persona completa el inicio de sesión en el navegador.
  useEffect(() => {
    if (paso !== 'esperando') return;
    const t = setTimeout(() => onSesionIniciada(resultado), 1400);
    return () => clearTimeout(t);
  }, [paso, resultado, onSesionIniciada]);

  const esperar = (c: string) => {
    setResultado(c);
    setPaso('esperando');
  };
  const correoOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(correo.trim());

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Iniciar sesión en Unity Version Control"
      subtitle="Al iniciar sesión verás todos tus proyectos de Unity VC"
      size="sm"
    >
      <div className="space-y-4 text-sm">
        {paso === 'elegir' && (
          <div className="space-y-2">
            <button
              type="button"
              className="flex w-full items-center justify-center gap-2 rounded-lg border border-border bg-text px-4 py-2.5 font-medium text-surface transition hover:opacity-90"
              onClick={() => esperar(correoDemo)}
            >
              <span aria-hidden>🔑</span> Con tu cuenta de Unity
            </button>
            <button
              type="button"
              className="flex w-full items-center justify-center gap-2 rounded-lg border border-border bg-surface-2 px-4 py-2.5 font-medium text-text transition hover:border-border-strong"
              onClick={() => setPaso('correo')}
            >
              <span aria-hidden>✉️</span> Con correo electrónico
            </button>
          </div>
        )}

        {paso === 'correo' && (
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
          </div>
        )}
      </div>
    </Modal>
  );
}
