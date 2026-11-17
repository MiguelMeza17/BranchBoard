import { useState } from 'react';
import { LoginUnityVCModal } from './LoginUnityVCModal';
import { useStore } from '../store/useStore';
import { Icono } from './Icono';

// Pantalla que reemplaza a los tableros cuando `cm whoami` falla al arrancar —
// ver documentacion/FRONTEND.md, "Autenticación / acceso al proyecto": BranchBoard
// no tiene login propio, la identidad depende de que Unity Version Control ya
// tenga una sesión iniciada en la máquina.
export function SesionUnityVCRequerida() {
  const showToast = useStore((s) => s.showToast);
  const currentEmail = useStore((s) => s.currentEmail);
  const setEmail = useStore((s) => s.setEmail);
  const usuarios = useStore((s) => s.usuarios);
  const [login, setLogin] = useState(false);
  const setSesionUnityVC = useStore((s) => s.setSesionUnityVC);
  const cmInstalado = useStore((s) => s.cmInstalado);
  const cmInstalando = useStore((s) => s.cmInstalando);
  const sesionCaducada = useStore((s) => s.sesionUnityVcCaducada);
  const instalarCm = useStore((s) => s.instalarCm);
  const reconectarUnityVc = useStore((s) => s.reconectarUnityVc);
  const [reconectando, setReconectando] = useState(false);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-surface">
      <div className="card flex w-full max-w-md flex-col items-center gap-4 p-8 text-center">
        <div className="flex items-baseline text-2xl font-bold tracking-tight">
          <span className="text-text">Branch</span>
          <span className="text-green-400">Board</span>
        </div>

        <div className="grid h-14 w-14 place-items-center rounded-2xl bg-warn/15">
          <Icono nombre="candado" className="h-7 w-7" />
        </div>

        <div>
          <h2 className="text-base font-semibold text-text">{sesionCaducada ? 'La sesión de Unity VC caducó' : 'No se detectó una sesión de Unity VC'}</h2>
          <p className="mt-1.5 text-sm text-muted">
            {sesionCaducada ? 'Pulsa Reconectar para volver a iniciar sesión con Unity VC.' : <>
            BranchBoard identifica al usuario a través de Unity Version Control (<code className="font-mono text-xs">cm whoami</code>),
            y no encontró ninguna sesión activa en este equipo. Inicia sesión con Unity VC para continuar.
            </>}
          </p>
        </div>

        <div className="flex w-full flex-col gap-2 pt-1">
          {!cmInstalado && (
            <button
              type="button"
              className="btn-primary btn-sm w-full"
              disabled={cmInstalando}
              onClick={() => void instalarCm()}
            >
              {cmInstalando ? 'Instalando Unity Version Control…' : 'Instalar Unity Version Control'}
            </button>
          )}
          <button
            className={`${cmInstalado ? 'btn-primary' : 'btn-outline'} btn-sm w-full`}
            disabled={!cmInstalado || cmInstalando || reconectando || sesionCaducada}
            onClick={() => setLogin(true)}
          >
            Iniciar sesión con Unity VC
          </button>
          <button
            type="button"
            className="btn-ghost btn-sm w-full"
            disabled={!cmInstalado || cmInstalando || reconectando}
            onClick={() => {
              setReconectando(true);
              void reconectarUnityVc().finally(() => setReconectando(false));
            }}
          >
            {reconectando ? 'Reconectando…' : 'Reconectar'}
          </button>
        </div>
      </div>
      <LoginUnityVCModal
        open={login}
        onClose={() => setLogin(false)}
        correoDemo={currentEmail}
        onSesionIniciada={(c) => {
          setLogin(false);
          if (usuarios.some((u) => u.email === c)) setEmail(c);
          setSesionUnityVC(c);
          useStore.setState({ cmSesionActiva: true });
          showToast(`Sesión de Unity VC iniciada como ${c}`);
        }}
      />
    </div>
  );
}
