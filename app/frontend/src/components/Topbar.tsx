import { useEffect, useRef, useState } from 'react';
import { useStore } from '../store/useStore';
import { ROLES, type Rol } from '../types';
import { Avatar } from './primitives';
import { NotificationsPanel } from './NotificationsPanel';
import { AbrirEnUnityModal } from './AbrirEnUnityModal';

export function Topbar() {
  const user = useStore((s) => s.currentUser());
  const vpnOn = useStore((s) => s.vpnOn);
  const toggleVpn = useStore((s) => s.toggleVpn);
  const setAccesoCompletado = useStore((s) => s.setAccesoCompletado);
  const setRolActual = useStore((s) => s.setRolActual);
  const notifs = useStore((s) => s.notificaciones);
  const email = useStore((s) => s.currentEmail);

  const [showNotifs, setShowNotifs] = useState(false);
  const [abrirUnity, setAbrirUnity] = useState(false);
  const [vistas, setVistas] = useState(false);
  const notifsWrapRef = useRef<HTMLDivElement>(null);
  const notifsPanelRef = useRef<HTMLDivElement>(null);
  const misNotifs = notifs.filter((n) => n.para === email);
  const noLeidas = misNotifs.filter((n) => !n.leida).length;
  const totalNotifs = misNotifs.length;
  const badgeNum = vistas ? totalNotifs : noLeidas;

  // Cierra al hacer click en cualquier parte fuera del panel. El panel vive en un
  // portal (document.body), así que hay que revisar tanto el botón como el panel
  // portado — ninguno de los dos depende de un overlay "fixed": el header tiene
  // backdrop-blur, que le crea un containing block propio y recorta los fixed a su franja.
  useEffect(() => {
    if (!showNotifs) return;
    const onClickOutside = (e: MouseEvent) => {
      const target = e.target as Node;
      const dentroBoton = notifsWrapRef.current?.contains(target);
      const dentroPanel = notifsPanelRef.current?.contains(target);
      if (!dentroBoton && !dentroPanel) setShowNotifs(false);
    };
    document.addEventListener('mousedown', onClickOutside);
    return () => document.removeEventListener('mousedown', onClickOutside);
  }, [showNotifs]);

  return (
    <>
    {/* whitespace-nowrap: los botones nunca parten su texto en dos renglones (se desbordaban de la
        barra). Si no cabe, el grupo de la izquierda se desplaza en horizontal; el de la derecha no se encoge. */}
    <header className="flex h-14 shrink-0 items-center justify-between gap-4 whitespace-nowrap border-b border-border bg-surface/80 px-5 backdrop-blur">
      {/* VPN switch */}
      <div className="flex min-w-0 items-center gap-3 overflow-x-auto [scrollbar-width:none]">
        <button
          onClick={toggleVpn}
          className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 px-2.5 py-1.5 text-xs font-medium transition hover:border-border-strong"
          title="Conexión VPN (Cloudflare WARP)"
        >
          <span
            className={`relative inline-flex h-4 w-7 items-center rounded-full transition-colors ${
              vpnOn ? 'bg-done' : 'bg-faint'
            }`}
          >
            <span
              className={`inline-block h-3 w-3 transform rounded-full bg-white transition-transform ${
                vpnOn ? 'translate-x-3.5' : 'translate-x-0.5'
              }`}
            />
          </span>
          <span className={vpnOn ? 'text-green-300' : 'text-muted'}>
            VPN {vpnOn ? 'conectada' : 'apagada'}
          </span>
        </button>

        <button className="btn-outline btn-sm shrink-0" onClick={() => setAbrirUnity(true)}>
          🎮 Abrir en Unity
        </button>
      </div>

      <div className="flex shrink-0 items-center gap-3">
        {/* Solo para el mockup (se quitan en la app real): volver a las pantallas de acceso y "Ver como". */}
        <button
          className="shrink-0 rounded-lg border border-border bg-surface-2 px-2.5 py-1.5 text-xs text-muted transition hover:border-border-strong hover:text-text"
          title="Demo: volver a la Pantalla inicial (acceso al proyecto). Se elimina en la app real."
          onClick={() => setAccesoCompletado(false)}
        >
          🚪 Pantallas de acceso
        </button>
        {/* Role selector (simula autenticación) */}
        <label className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 px-2.5 py-1.5 text-xs">
          <span className="text-faint">Ver como</span>
          <select
            className="bg-transparent font-medium text-text outline-none"
            value={user.rol}
            onChange={(e) => setRolActual(e.target.value as Rol)}
          >
            {ROLES.map((r) => (
              <option key={r} value={r} className="bg-surface text-text">
                {r}
              </option>
            ))}
          </select>
        </label>

        {/* Notifications */}
        <div className="relative" ref={notifsWrapRef}>
          <button
            className="relative grid h-9 w-9 place-items-center rounded-lg border border-border bg-surface-2 text-base transition hover:border-border-strong"
            onClick={() => {
              setShowNotifs((v) => !v);
              setVistas(true);
            }}
            aria-label={noLeidas > 0 ? `Notificaciones, ${noLeidas} sin leer` : 'Notificaciones'}
          >
            🔔
            {badgeNum > 0 && (
              <span className="absolute -right-1 -top-1 h-4 w-4 rounded-full bg-primary">
                <span className="absolute inset-0 flex translate-x-[-0.1px] translate-y-[0.5px] items-center justify-center text-[11px] font-bold leading-none text-white">
                  {badgeNum > 99 ? '99+' : badgeNum}
                </span>
              </span>
            )}
          </button>
          {showNotifs && (
            <NotificationsPanel
              onClose={() => setShowNotifs(false)}
              anchorRef={notifsWrapRef}
              panelRef={notifsPanelRef}
            />
          )}
        </div>

        {/* Identity */}
        <div className="flex items-center gap-2.5 rounded-lg border border-border bg-surface-2 py-1 pl-1 pr-3">
          <Avatar email={user.email} size={30} sinTooltip />
          <div className="leading-tight">
            <div className="text-xs font-semibold text-text">
              {user.nombre} {user.apellido}
            </div>
            <div className="font-mono text-[10px] text-faint">{user.rol}</div>
          </div>
        </div>
      </div>
    </header>
    {/* Fuera del header: su backdrop-blur atraparía a la ventana (position: fixed) dentro de la barra. */}
    <AbrirEnUnityModal open={abrirUnity} onClose={() => setAbrirUnity(false)} />
    </>
  );
}
