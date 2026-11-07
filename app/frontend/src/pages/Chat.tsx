import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { CHAT_GRUPO, type GrupoChat, type Mensaje, type Usuario } from '../types';
import { Modal } from '../components/Modal';
import { avatarColor } from '../lib/ui';
import { Icono } from '../components/Icono';

const MAX_MENSAJE = 1000;

// Color estable por persona para su círculo de iniciales (y su nombre en el grupo).
const COLORES = ['#6366f1', '#22c55e', '#f59e0b', '#ec4899', '#06b6d4', '#a855f7', '#ef4444', '#14b8a6'];
const colorDe = (email: string) => {
  let h = 0;
  for (const c of email) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return COLORES[h % COLORES.length];
};

// Círculo de iniciales: como en el resto de la app, clic → perfil de esa persona (esté donde esté).
function Inicial({ u, size = 36 }: { u: Usuario; size?: number }) {
  const navigate = useNavigate();
  const irAlPerfil = (e: React.SyntheticEvent) => {
    e.preventDefault();
    e.stopPropagation();
    navigate(`/perfil/${encodeURIComponent(u.email)}`);
  };
  return (
    <span
      role="link"
      tabIndex={0}
      title={`${u.nombre} ${u.apellido}`.trim()}
      onClick={irAlPerfil}
      onKeyDown={(e) => {
        if (e.key === 'Enter') irAlPerfil(e);
      }}
      className="grid shrink-0 cursor-pointer place-items-center rounded-full font-semibold text-white transition hover:ring-2 hover:ring-white/40"
      style={{ width: size, height: size, background: avatarColor(u.email), fontSize: size * 0.38 }}
    >
      {u.nombre[0]}
      {u.apellido[0] ?? ''}
    </span>
  );
}

// El grupo general usa el mismo círculo blanco que el proyecto (todavía no tiene ícono); los demás
// grupos, un círculo con la inicial del grupo.
function CirculoGrupo({ size = 36, grupo }: { size?: number; grupo?: GrupoChat }) {
  if (!grupo) return <span className="shrink-0 rounded-full border border-white/70" style={{ width: size, height: size }} />;
  return (
    <span
      className="grid shrink-0 place-items-center rounded-full font-semibold text-white ring-2 ring-surface-3"
      style={{ width: size, height: size, background: colorDe(grupo.id), fontSize: size * 0.4 }}
    >
      {grupo.nombre[0]?.toUpperCase()}
    </span>
  );
}

const hora = (iso: string) => new Date(iso).toLocaleTimeString('es-CO', { hour: '2-digit', minute: '2-digit' });
const dia = (iso: string) => {
  const d = new Date(iso);
  const hoy = new Date();
  const ayer = new Date();
  ayer.setDate(hoy.getDate() - 1);
  if (d.toDateString() === hoy.toDateString()) return 'Hoy';
  if (d.toDateString() === ayer.toDateString()) return 'Ayer';
  return d.toLocaleDateString('es-CO', { day: 'numeric', month: 'long' });
};
const corto = (iso: string) => (new Date(iso).toDateString() === new Date().toDateString() ? hora(iso) : dia(iso));

interface Conversacion {
  id: string; // CHAT_GRUPO o email
  titulo: string;
  subtitulo: string;
  ultimo?: Mensaje;
  sinLeer: number;
  u?: Usuario;
  grupo?: GrupoChat;
  esGrupo: boolean;
}

// Chat del proyecto: el grupo general (todo el equipo, con el nombre del proyecto) siempre existe y va
// arriba; debajo los grupos que cree el equipo y los privados. Sin servidor: en la app real cada mensaje viaja como un evento por la carpeta compartida
// (llega cuando sincroniza Drive/OneDrive, con la app abierta).
export function Chat() {
  const { email: param } = useParams();
  const navigate = useNavigate();
  const yo = useStore((s) => s.currentUser());
  const usuarios = useStore((s) => s.usuarios);
  const proyecto = useStore((s) => s.proyectoActual);
  const mensajes = useStore((s) => s.mensajes);
  const enviar = useStore((s) => s.enviarChat);
  const marcarLeidos = useStore((s) => s.marcarChatLeido);
  const gruposChat = useStore((s) => s.gruposChat);
  const crearGrupo = useStore((s) => s.crearGrupoChat);
  const [nuevoGrupo, setNuevoGrupo] = useState(false);
  const [nuevoChat, setNuevoChat] = useState(false);
  const [verIntegrantes, setVerIntegrantes] = useState(false);
  const [confirmarEliminar, setConfirmarEliminar] = useState(false);
  const eliminados = useStore((s) => s.chatsEliminados);
  const eliminarChat = useStore((s) => s.eliminarChat);
  const [texto, setTexto] = useState('');
  const [buscar, setBuscar] = useState('');
  const fin = useRef<HTMLDivElement>(null);

  // Sin nada elegido se abre el grupo.
  const abierta = param ?? CHAT_GRUPO;
  const companeros = usuarios.filter((u) => u.email !== yo.email);
  const misGrupos = gruposChat.filter((g) => g.miembros.includes(yo.email));
  const esIdGrupo = (id: string) => id === CHAT_GRUPO || gruposChat.some((g) => g.id === id);
  const deConversacion = (id: string) => {
    const desde = eliminados[id] ?? '';
    const todos = esIdGrupo(id)
      ? mensajes.filter((m) => m.para === id)
      : mensajes.filter((m) => (m.de === yo.email && m.para === id) || (m.de === id && m.para === yo.email));
    return desde ? todos.filter((m) => m.fecha > desde) : todos;
  };
  const sinLeer = (conv: Mensaje[]) => conv.filter((m) => m.de !== yo.email && !m.leidoPor.includes(yo.email)).length;

  const grupo: Conversacion = useMemo(() => {
    const conv = deConversacion(CHAT_GRUPO);
    return {
      id: CHAT_GRUPO,
      titulo: proyecto,
      subtitulo: `${usuarios.length} integrantes`,
      ultimo: conv[conv.length - 1],
      sinLeer: sinLeer(conv),
      esGrupo: true,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mensajes, usuarios, proyecto, yo.email]);

  const otrosGrupos: Conversacion[] = useMemo(() => {
    const q = buscar.trim().toLowerCase();
    return misGrupos
      .filter((g) => !q || g.nombre.toLowerCase().includes(q))
      .map((g) => {
        const conv = deConversacion(g.id);
        return { id: g.id, titulo: g.nombre, subtitulo: `${g.miembros.length} integrantes`, ultimo: conv[conv.length - 1], sinLeer: sinLeer(conv), grupo: g, esGrupo: true };
      })
      .filter((c) => !eliminados[c.id] || c.ultimo || c.id === abierta)
      .sort((a, b) => (b.ultimo?.fecha ?? b.grupo!.fecha).localeCompare(a.ultimo?.fecha ?? a.grupo!.fecha));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mensajes, gruposChat, yo.email, buscar, eliminados, abierta]);

  const privados: Conversacion[] = useMemo(() => {
    const q = buscar.trim().toLowerCase();
    return companeros
      .filter((u) => !q || `${u.nombre} ${u.apellido}`.toLowerCase().includes(q))
      .map((u) => {
        const conv = deConversacion(u.email);
        return { id: u.email, titulo: `${u.nombre} ${u.apellido}`, subtitulo: u.rol, ultimo: conv[conv.length - 1], sinLeer: sinLeer(conv), u, esGrupo: false };
      })
      .filter((c) => c.ultimo || c.id === abierta)
      .sort((a, b) => (b.ultimo?.fecha ?? '').localeCompare(a.ultimo?.fecha ?? ''));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mensajes, usuarios, yo.email, buscar, eliminados, abierta]);

  const esGeneral = abierta === CHAT_GRUPO;
  const grupoAbierto = misGrupos.find((g) => g.id === abierta);
  const esGrupo = esGeneral || !!grupoAbierto;
  const otro = esGrupo ? undefined : companeros.find((u) => u.email === abierta);
  const valida = esGrupo || !!otro;
  const miembrosAbiertos = esGeneral ? usuarios : usuarios.filter((u) => grupoAbierto?.miembros.includes(u.email));
  const conv = valida ? deConversacion(abierta) : [];

  useEffect(() => {
    if (valida) marcarLeidos(abierta);
  }, [abierta, valida, conv.length, marcarLeidos]);
  useEffect(() => {
    fin.current?.scrollIntoView({ block: 'end' });
  }, [abierta, conv.length]);

  const mandar = () => {
    if (!valida || !texto.trim()) return;
    enviar(abierta, texto);
    setTexto('');
  };
  const autor = (email: string) => usuarios.find((u) => u.email === email);
  const ir = (id: string) => navigate(id === CHAT_GRUPO ? '/chat' : `/chat/${encodeURIComponent(id)}`);

  const Fila = ({ c }: { c: Conversacion }) => {
    const activo = c.id === abierta;
    const prefijo = c.ultimo
      ? c.ultimo.de === yo.email
        ? 'Tú: '
        : c.esGrupo
          ? `${autor(c.ultimo.de)?.nombre ?? ''}: `
          : ''
      : '';
    return (
      <button
        type="button"
        onClick={() => ir(c.id)}
        className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition ${activo ? 'bg-primary-soft' : 'hover:bg-surface-2'}`}
      >
        {c.u ? <Inicial u={c.u} size={40} /> : <CirculoGrupo size={40} grupo={c.grupo} />}
        <span className="min-w-0 flex-1">
          <span className="flex items-baseline justify-between gap-2">
            <span className={`truncate text-sm text-text ${c.sinLeer ? 'font-semibold' : 'font-medium'}`}>{c.titulo}</span>
            {c.ultimo && <span className="shrink-0 text-[10px] text-faint">{corto(c.ultimo.fecha)}</span>}
          </span>
          <span className="flex items-center justify-between gap-2">
            <span className={`truncate text-xs ${c.sinLeer ? 'text-muted' : 'text-faint'}`}>
              {c.ultimo ? `${prefijo}${c.ultimo.texto}` : c.subtitulo}
            </span>
            {c.sinLeer > 0 && (
              <span className="grid h-5 min-w-5 shrink-0 place-items-center rounded-full bg-primary px-1 text-[10px] font-bold text-white">
                {c.sinLeer}
              </span>
            )}
          </span>
        </span>
      </button>
    );
  };

  return (
    <div className="flex h-[calc(100vh-7.5rem)] min-h-[420px] overflow-hidden rounded-2xl border border-border bg-surface">
      {/* Conversaciones: el grupo fijo arriba y los privados debajo. */}
      <aside className={`w-full flex-col border-r border-border md:flex md:w-80 ${param ? 'hidden' : 'flex'}`}>
        <div className="border-b border-border p-4">
          <h1 className="mb-3 text-lg font-bold text-text">Chat</h1>
          <input className="input h-9" placeholder="Buscar chat…" value={buscar} onChange={(e) => setBuscar(e.target.value)} />
        </div>
        <div className="flex-1 overflow-y-auto p-2">
          <div className="flex items-center justify-between px-3 pb-1 pt-1">
            <span className="text-[10px] font-semibold uppercase tracking-wide text-faint">Grupos</span>
            <button type="button" className="text-xs font-medium text-primary hover:underline" onClick={() => setNuevoGrupo(true)}>
              + Nuevo grupo
            </button>
          </div>
          {(!buscar.trim() || proyecto.toLowerCase().includes(buscar.trim().toLowerCase())) && <Fila c={grupo} />}
          {otrosGrupos.map((c) => (
            <Fila key={c.id} c={c} />
          ))}
          <div className="mt-3 flex items-center justify-between px-3 pb-1">
            <span className="text-[10px] font-semibold uppercase tracking-wide text-faint">Privados</span>
            <button type="button" className="text-xs font-medium text-primary hover:underline" onClick={() => setNuevoChat(true)}>
              + Nuevo chat
            </button>
          </div>
          {privados.map((c) => (
            <Fila key={c.id} c={c} />
          ))}
          {privados.length === 0 && (
            <p className="p-4 text-center text-sm text-faint">{buscar.trim() ? 'Ningún chat con ese nombre.' : 'Sin chats privados.'}</p>
          )}
          {buscar.trim() && !proyecto.toLowerCase().includes(buscar.trim().toLowerCase()) && otrosGrupos.length === 0 && (
            <p className="-mt-2 px-4 pb-2 text-center text-[11px] text-faint">Tampoco hay grupos con ese nombre.</p>
          )}
        </div>
      </aside>

      {/* Conversación abierta */}
      <section className={`min-w-0 flex-1 flex-col md:flex ${param ? 'flex' : 'hidden'}`}>
        {!valida ? (
          <div className="grid flex-1 place-items-center p-8 text-center text-sm text-muted">Esa conversación no existe.</div>
        ) : (
          <>
            <header className="flex items-center gap-3 border-b border-border px-4 py-3">
              <button type="button" className="btn-ghost btn-sm md:hidden" onClick={() => navigate('/chat')} aria-label="Volver">
                ←
              </button>
              {esGrupo ? (
                <button
                  type="button"
                  className="group flex min-w-0 items-center gap-3 text-left"
                  onClick={() => setVerIntegrantes(true)}
                  title="Ver integrantes"
                >
                  <CirculoGrupo size={36} grupo={grupoAbierto} />
                  <span className="min-w-0">
                    <span className="block truncate text-sm font-semibold text-text group-hover:underline">{grupoAbierto?.nombre ?? proyecto}</span>
                    <span className="block truncate text-xs text-faint">
                      {miembrosAbiertos.map((u) => (u.email === yo.email ? 'Tú' : u.nombre)).join(', ')}
                    </span>
                  </span>
                </button>
              ) : (
                otro && (
                  <button type="button" className="flex min-w-0 items-center gap-3 text-left" onClick={() => navigate(`/perfil/${encodeURIComponent(otro.email)}`)}>
                    <Inicial u={otro} size={36} />
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-semibold text-text hover:underline">
                        {otro.nombre} {otro.apellido}
                      </span>
                      <span className="block text-xs text-faint">{otro.rol}</span>
                    </span>
                  </button>
                )
              )}
              {!esGeneral && (
                <MenuOpciones onEliminar={() => setConfirmarEliminar(true)} />
              )}
            </header>

            <div className="flex-1 overflow-y-auto bg-[radial-gradient(circle_at_top,rgba(99,102,241,0.06),transparent_60%)] px-4 py-4">
              {conv.length === 0 && <p className="mt-10 text-center text-sm text-faint">Todavía no hay mensajes. ¡Saluda! 👋</p>}
              {conv.map((m, i) => {
                const mio = m.de === yo.email;
                const nuevoDia = i === 0 || dia(conv[i - 1].fecha) !== dia(m.fecha);
                const seguido = i > 0 && conv[i - 1].de === m.de && !nuevoDia;
                const quien = autor(m.de);
                // En el grupo, los mensajes de otros llevan su círculo y su nombre (solo en el primero de una racha).
                const conAutor = esGrupo && !mio;
                return (
                  <div key={m.id}>
                    {nuevoDia && (
                      <div className="my-3 flex justify-center">
                        <span className="rounded-full bg-surface-2 px-3 py-1 text-[11px] text-faint">{dia(m.fecha)}</span>
                      </div>
                    )}
                    <div className={`flex items-end gap-2 ${mio ? 'justify-end' : 'justify-start'} ${seguido ? 'mt-0.5' : 'mt-2.5'}`}>
                      {conAutor && (seguido || !quien ? <span className="w-7 shrink-0" /> : <Inicial u={quien} size={28} />)}
                      <div
                        className={`max-w-[75%] whitespace-pre-wrap break-words px-3.5 py-2 text-sm shadow-sm ${
                          mio ? 'rounded-2xl rounded-br-md bg-primary text-white' : 'rounded-2xl rounded-bl-md border border-border bg-surface-2 text-text'
                        }`}
                      >
                        {conAutor && !seguido && quien && (
                          <span className="mb-0.5 block text-xs font-semibold" style={{ color: avatarColor(quien.email) }}>
                            {quien.nombre} {quien.apellido}
                          </span>
                        )}
                        {m.texto}
                        <span className={`ml-2 inline-block translate-y-0.5 text-[10px] ${mio ? 'text-white/70' : 'text-faint'}`}>{hora(m.fecha)}</span>
                      </div>
                    </div>
                  </div>
                );
              })}
              <div ref={fin} />
            </div>

            <form
              className="flex items-end gap-2 border-t border-border p-3"
              onSubmit={(e) => {
                e.preventDefault();
                mandar();
              }}
            >
              <textarea
                className="input max-h-32 min-h-[40px] flex-1 resize-none py-2"
                rows={1}
                maxLength={MAX_MENSAJE}
                placeholder={esGrupo ? `Escribe en ${grupoAbierto?.nombre ?? proyecto}…` : `Escribe a ${otro?.nombre}…`}
                value={texto}
                autoFocus
                onChange={(e) => setTexto(e.target.value)}
                onKeyDown={(e) => {
                  // Enter envía; Shift+Enter hace salto de línea.
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    mandar();
                  }
                }}
              />
              <button
                type="submit"
                disabled={!texto.trim()}
                aria-label="Enviar"
                className="grid h-10 w-10 shrink-0 place-items-center rounded-full bg-primary text-white transition hover:opacity-90 disabled:opacity-40"
              >
                <svg viewBox="0 0 20 20" className="h-4 w-4" fill="currentColor" aria-hidden="true">
                  <path d="M3.4 2.6a.75.75 0 0 1 .82-.12l13 6.5a.75.75 0 0 1 0 1.34l-13 6.5A.75.75 0 0 1 3.2 16l1.7-6-1.7-6a.75.75 0 0 1 .2-.78ZM6.3 10.75 5.1 15l9.8-4.9-9.8-4.9 1.2 4.25H11a.75.75 0 0 1 0 1.5H6.3Z" />
                </svg>
              </button>
            </form>
          </>
        )}
      </section>

      <Modal
        open={confirmarEliminar}
        onClose={() => setConfirmarEliminar(false)}
        title="¿Eliminar este chat?"
        size="sm"
        footer={
          <div className="flex w-full justify-end gap-2">
            <button className="btn-ghost btn-sm" onClick={() => setConfirmarEliminar(false)}>Cancelar</button>
            <button
              className="btn-danger btn-sm"
              onClick={() => {
                eliminarChat(abierta);
                setConfirmarEliminar(false);
                navigate('/chat');
              }}
            >
              Eliminar
            </button>
          </div>
        }
      >
        <p className="text-sm text-muted">Se borra de tu lista con sus mensajes. A los demás no les cambia nada.</p>
      </Modal>

      <Modal
        open={verIntegrantes && esGrupo}
        onClose={() => setVerIntegrantes(false)}
        title={grupoAbierto?.nombre ?? proyecto}
        subtitle={`${miembrosAbiertos.length} integrantes`}
        size="sm"
      >
        <div className="max-h-[26rem] space-y-1 overflow-y-auto">
          {miembrosAbiertos.map((u) => {
            const soyYo = u.email === yo.email;
            return (
              <div key={u.email} className="flex items-center gap-3 rounded-lg px-2 py-2 transition hover:bg-surface-2">
                <Inicial u={u} size={32} />
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => {
                    setVerIntegrantes(false);
                    navigate(`/perfil/${encodeURIComponent(u.email)}`);
                  }}
                >
                  <span className="block truncate text-sm text-text hover:underline">
                    {u.nombre} {u.apellido} {soyYo && <span className="text-[11px] text-faint">(tú)</span>}
                  </span>
                  <span className="block text-[11px] text-faint">{u.rol}</span>
                </button>
                {!soyYo && (
                  <button
                    type="button"
                    aria-label={`Escribirle a ${u.nombre}`}
                    title={`Escribirle a ${u.nombre}`}
                    onClick={() => {
                      setVerIntegrantes(false);
                      ir(u.email);
                    }}
                    className="grid h-8 w-8 shrink-0 place-items-center rounded-full text-primary transition hover:bg-primary-soft"
                  >
                    <Icono nombre="chat" className="h-4 w-4" />
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </Modal>

      <Modal open={nuevoChat} onClose={() => setNuevoChat(false)} title="Nuevo chat" size="sm">
        <div className="max-h-80 space-y-1 overflow-y-auto">
          {companeros.map((u) => (
            <button
              key={u.email}
              type="button"
              onClick={() => {
                setNuevoChat(false);
                ir(u.email);
              }}
              className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left transition hover:bg-surface-2"
            >
              <Inicial u={u} size={32} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm text-text">{u.nombre} {u.apellido}</span>
                <span className="block text-[11px] text-faint">{u.rol}</span>
              </span>
            </button>
          ))}
        </div>
      </Modal>

      <NuevoGrupoModal
        open={nuevoGrupo}
        companeros={companeros}
        onClose={() => setNuevoGrupo(false)}
        onCrear={(nombre, miembros) => {
          const id = crearGrupo(nombre, miembros);
          setNuevoGrupo(false);
          ir(id);
        }}
      />
    </div>
  );
}

// Menú de tres puntos de la cabecera del chat (por ahora solo "Eliminar chat").
function MenuOpciones({ onEliminar }: { onEliminar: () => void }) {
  const [abierto, setAbierto] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!abierto) return;
    const fuera = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setAbierto(false);
    };
    document.addEventListener('mousedown', fuera);
    return () => document.removeEventListener('mousedown', fuera);
  }, [abierto]);

  return (
    <div ref={ref} className="relative ml-auto">
      <button
        type="button"
        aria-label="Opciones del chat"
        aria-expanded={abierto}
        onClick={() => setAbierto((v) => !v)}
        className="grid h-8 w-8 place-items-center rounded-full text-lg leading-none text-muted transition hover:bg-surface-3 hover:text-text"
      >
        ⋮
      </button>
      {abierto && (
        <div className="absolute right-0 top-full z-20 mt-1 w-44 overflow-hidden rounded-xl border border-border bg-surface-2 py-1 shadow-pop">
          <button
            type="button"
            onClick={() => {
              setAbierto(false);
              onEliminar();
            }}
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-red-300 transition hover:bg-red-500/10"
          >
            <Icono nombre="papelera" className="h-4 w-4" sinColor /> Eliminar chat
          </button>
        </div>
      )}
    </div>
  );
}

function NuevoGrupoModal({
  open,
  companeros,
  onClose,
  onCrear,
}: {
  open: boolean;
  companeros: Usuario[];
  onClose: () => void;
  onCrear: (nombre: string, miembros: string[]) => void;
}) {
  const [nombre, setNombre] = useState('');
  const [elegidos, setElegidos] = useState<string[]>([]);
  const cerrar = () => {
    setNombre('');
    setElegidos([]);
    onClose();
  };
  const alternar = (email: string) =>
    setElegidos((v) => (v.includes(email) ? v.filter((x) => x !== email) : [...v, email]));

  return (
    <Modal
      open={open}
      onClose={cerrar}
      title="Nuevo grupo"
      size="sm"
      footer={
        <div className="flex w-full justify-end gap-2">
          <button className="btn-ghost btn-sm" onClick={cerrar}>Cancelar</button>
          <button
            className="btn-primary btn-sm"
            disabled={!nombre.trim() || elegidos.length === 0}
            onClick={() => {
              onCrear(nombre, elegidos);
              setNombre('');
              setElegidos([]);
            }}
          >
            Crear grupo
          </button>
        </div>
      }
    >
      <label className="label">Nombre del grupo</label>
      <input className="input" autoFocus maxLength={60} value={nombre} onChange={(e) => setNombre(e.target.value)} />
      <div className="label mt-4">Integrantes ({elegidos.length})</div>
      <div className="max-h-64 space-y-1 overflow-y-auto">
        {companeros.map((u) => {
          const marcado = elegidos.includes(u.email);
          return (
            <label
              key={u.email}
              className={`flex cursor-pointer items-center gap-3 rounded-lg px-2 py-1.5 transition ${marcado ? 'bg-primary-soft' : 'hover:bg-surface-2'}`}
            >
              <input type="checkbox" className="accent-primary" checked={marcado} onChange={() => alternar(u.email)} />
              <Inicial u={u} size={28} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm text-text">{u.nombre} {u.apellido}</span>
                <span className="block text-[11px] text-faint">{u.rol}</span>
              </span>
            </label>
          );
        })}
      </div>
    </Modal>
  );
}
