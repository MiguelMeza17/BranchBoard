import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { avatarColor, iniciales } from '../lib/ui';
import { useStore } from '../store/useStore';
import { ROLES, type Rol } from '../types';
import { Avatar } from './primitives';
import { NotificationsPanel } from './NotificationsPanel';
import { AbrirEnUnityModal } from './AbrirEnUnityModal';
import { Icono } from './Icono';

export function Topbar({ onMenu }: { onMenu?: () => void }) {
  const user = useStore((s) => s.currentUser());
  const navigate = useNavigate();
  const vpnOn = useStore((s) => s.vpnOn);
  const toggleVpn = useStore((s) => s.toggleVpn);
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
    <header className="flex h-14 shrink-0 items-center justify-between gap-2 whitespace-nowrap border-b border-border bg-surface/80 px-2 backdrop-blur sm:gap-4 sm:px-5">
      {/* VPN switch */}
      <div className="flex min-w-0 items-center gap-2 overflow-x-auto [scrollbar-width:none] sm:gap-3">
        {/* Pantalla angosta: abre el menú lateral (cajón). */}
        {onMenu && (
          <button
            type="button"
            onClick={onMenu}
            aria-label="Menú"
            className="grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border bg-surface-2 text-text transition hover:border-border-strong"
          >
            <svg
              viewBox="0 0 20 20"
              className="h-[18px] w-[18px]"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              aria-hidden="true"
            >
              <path d="M3.5 5.5h13M3.5 10h13M3.5 14.5h13" />
            </svg>
          </button>
        )}
        <button
          onClick={toggleVpn}
          className="flex shrink-0 items-center gap-2 rounded-lg border border-border bg-surface-2 px-2.5 py-1.5 text-xs font-medium transition hover:border-border-strong"
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
            <span className="hidden sm:inline">VPN</span><span className="hidden md:inline"> {vpnOn ? 'conectada' : 'apagada'}</span>
          </span>
        </button>

        <button
          className="btn-outline btn-sm shrink-0"
          onClick={() => setAbrirUnity(true)}
          title="Abrir en Unity"
          aria-label="Abrir en Unity"
        >
          <Icono nombre="unity" className="h-3.5 w-3.5" />
          <span className="hidden sm:inline">Abrir en Unity</span>
        </button>
      </div>

      <div className="flex shrink-0 items-center gap-2 sm:gap-3">
        {/* Solo para el mockup (se quita en la app real): "Ver como". */}
        {/* Role selector (simula autenticación) */}
        <label className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 px-2.5 py-1.5 text-xs">
          <span className="hidden text-faint md:inline">Ver como</span>
          <select
            aria-label="Ver como"
            className="max-w-[4.75rem] truncate bg-transparent font-medium text-text outline-none sm:max-w-none"
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
            <Icono nombre="campana" className="h-[18px] w-[18px]" />
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

        {/* Identidad: todo el recuadro (iniciales, nombre y rol) lleva a tu perfil. */}
        <button
          type="button"
          onClick={() => navigate(`/perfil/${encodeURIComponent(user.email)}`)}
          title="Ver tu perfil"
          className="flex items-center gap-2.5 rounded-lg border border-border bg-surface-2 p-1 text-left transition hover:border-border-strong hover:bg-surface-3 sm:pr-3"
        >
          <span
            className="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-full text-[11px] font-semibold text-white ring-1 ring-black/20"
            style={{ background: avatarColor(user.email) }}
          >
            {iniciales(user.nombre, user.apellido)}
          </span>
          <span className="hidden leading-tight sm:block">
            <span className="block text-xs font-semibold text-text">
              {user.nombre} {user.apellido}
            </span>
            <span className="block font-mono text-[10px] text-faint">{user.rol}</span>
          </span>
        </button>
      </div>
    </header>
    {/* Fuera del header: su backdrop-blur atraparía a la ventana (position: fixed) dentro de la barra. */}
    <AbrirEnUnityModal open={abrirUnity} onClose={() => setAbrirUnity(false)} />
    </>
  );
}
