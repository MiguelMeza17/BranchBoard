import { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useStore } from '../store/useStore';
import { ESTADOS, type EstadoTarea } from '../types';
import { estadoMeta } from '../lib/ui';
import { PageHeader, Card, EmptyState, EstadoBadge, TipoBadge } from '../components/primitives';
import { Modal } from '../components/Modal';

const MAX_MENSAJE = 500;

// --- Actividad simulada (no viene de datos reales de checkins de Unity VC) ---
// Nivel 0-4 determinista por persona+día, solo para dar una vista visual tipo
// "mapa de calor" de actividad — en la app real vendría del historial de `cm`.
function nivelActividad(email: string, fechaISO: string): number {
  let h = 0;
  const s = `${email}|${fechaISO}`;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  const r = (h % 1000) / 1000;
  if (r < 0.35) return 0;
  if (r < 0.6) return 1;
  if (r < 0.8) return 2;
  if (r < 0.93) return 3;
  return 4;
}

const NIVEL_COLOR = ['#1e2530', '#1e3a2a', '#1f6b3f', '#22a35a', '#4ade80'];

function toISO(d: Date) {
  return d.toISOString().slice(0, 10);
}

function lunesDeLaSemana(d: Date): Date {
  const copia = new Date(d);
  const dow = (copia.getDay() + 6) % 7; // 0 = lunes
  copia.setDate(copia.getDate() - dow);
  copia.setHours(0, 0, 0, 0);
  return copia;
}

const DIAS_LABEL = ['L', 'M', 'X', 'J', 'V', 'S', 'D'];

function CalendarioActividad({ email, semanas }: { email: string; semanas: number }) {
  const hoy = useMemo(() => new Date(), []);
  const lunesActual = useMemo(() => lunesDeLaSemana(hoy), [hoy]);

  // Matriz [semana][díaDeLaSemana], semana 0 = más antigua.
  const grid = useMemo(() => {
    const inicio = new Date(lunesActual);
    inicio.setDate(inicio.getDate() - 7 * (semanas - 1));
    const out: { fecha: string; esFuturo: boolean }[][] = [];
    for (let w = 0; w < semanas; w++) {
      const fila: { fecha: string; esFuturo: boolean }[] = [];
      for (let d = 0; d < 7; d++) {
        const fecha = new Date(inicio);
        fecha.setDate(fecha.getDate() + w * 7 + d);
        fila.push({ fecha: toISO(fecha), esFuturo: fecha > hoy });
      }
      out.push(fila);
    }
    return out;
  }, [lunesActual, semanas, hoy]);

  return (
    <div className="overflow-x-auto">
      <div className="inline-flex gap-1.5">
        <div className="flex flex-col justify-between gap-1.5 pr-1 pt-5 text-[10px] text-faint">
          {DIAS_LABEL.map((d) => (
            <span key={d} className="flex h-4 items-center">
              {d}
            </span>
          ))}
        </div>
        {grid.map((semana, wi) => (
          <div key={wi} className="flex flex-col gap-1.5">
            <span className="block h-3 text-center text-[9px] text-faint">
              {wi === grid.length - 1 ? 'hoy' : ''}
            </span>
            {semana.map(({ fecha, esFuturo }) => {
              const nivel = esFuturo ? -1 : nivelActividad(email, fecha);
              return (
                <div
                  key={fecha}
                  title={esFuturo ? '' : `${fecha} · nivel ${nivel}/4`}
                  className="h-4 w-4 rounded-[3px]"
                  style={{ background: nivel < 0 ? 'transparent' : NIVEL_COLOR[nivel] }}
                />
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

export function PerfilUsuario() {
  const { email = '' } = useParams();
  const navigate = useNavigate();
  const usuarios = useStore((s) => s.usuarios);
  const tareas = useStore((s) => s.tareas);
  const [mesExpandido, setMesExpandido] = useState(false);
  const currentEmail = useStore((s) => s.currentEmail);
  const enviarMensaje = useStore((s) => s.enviarMensaje);
  const [escribiendo, setEscribiendo] = useState(false);
  const [mensaje, setMensaje] = useState('');
  const cerrarMensaje = () => {
    setEscribiendo(false);
    setMensaje('');
  };

  const u = usuarios.find((x) => x.email === email);
  const asignadas = useMemo(
    () => tareas.filter((t) => t.asignado_a === email && !t.eliminada),
    [tareas, email]
  );

  const porEstado = (estado: EstadoTarea) => asignadas.filter((t) => t.estado === estado);

  if (!u) {
    return (
      <EmptyState
        icon="🕵️"
        title="Usuario no encontrado"
        desc={`No hay ningún miembro del equipo con el correo ${email}.`}
        action={
          <button className="btn-outline btn-sm" onClick={() => navigate(-1)}>
            ← Volver
          </button>
        }
      />
    );
  }

  return (
    <div>
      <PageHeader
        title="Perfil de usuario"
        desc="Información básica visible para todo el equipo — tareas asignadas y actividad reciente."
        actions={
          <button className="btn-ghost btn-sm" onClick={() => navigate(-1)}>
            ← Volver
          </button>
        }
      />

      <div className="space-y-6">
        <Card className="flex flex-wrap items-center gap-4">
          <span
            className="grid h-16 w-16 shrink-0 place-items-center rounded-full text-xl font-bold text-white ring-2 ring-black/20"
            style={{ background: `linear-gradient(135deg, ${estadoMeta['en producción'].dot}, ${estadoMeta['en revisión'].dot})` }}
          >
            {u.nombre[0]}
            {u.apellido[0]}
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="text-lg font-bold text-text">
              {u.nombre} {u.apellido}
            </h2>
            <p className="truncate font-mono text-xs text-faint">{u.email}</p>
          </div>
          <span className="chip border border-primary/40 bg-primary-soft text-indigo-200">{u.rol}</span>
          {u.firma === null && (
            <span className="chip border border-amber-500/40 bg-amber-500/15 text-amber-300">
              ⏳ Pendiente de confirmar
            </span>
          )}
          <span className="chip border border-border bg-surface-2 text-muted">Desde {u.fecha_alta}</span>
          {u.email !== currentEmail && (
            <button className="btn-primary btn-sm" onClick={() => setEscribiendo(true)}>
              💬 Enviar mensaje
            </button>
          )}
        </Card>

        <Modal
          open={escribiendo}
          onClose={cerrarMensaje}
          title={`Mensaje para ${u.nombre}`}
          size="sm"
          footer={
            <div className="flex w-full justify-end gap-2">
              <button className="btn-ghost btn-sm" onClick={cerrarMensaje}>Cancelar</button>
              <button
                className="btn-primary btn-sm"
                disabled={!mensaje.trim()}
                onClick={() => {
                  enviarMensaje(u.email, mensaje);
                  cerrarMensaje();
                }}
              >
                Enviar
              </button>
            </div>
          }
        >
          <textarea
            className="input min-h-[120px] resize-y"
            autoFocus
            maxLength={MAX_MENSAJE}
            placeholder="Escribe tu mensaje…"
            value={mensaje}
            onChange={(e) => setMensaje(e.target.value)}
          />
          <div className="mt-1 text-right text-[11px] text-faint">{mensaje.length}/{MAX_MENSAJE}</div>
        </Modal>

        <Card>
          <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-sm font-semibold text-text">Actividad</h3>
            <button className="btn-ghost btn-sm" onClick={() => setMesExpandido((v) => !v)}>
              {mesExpandido ? 'Ver solo esta semana' : 'Ampliar al mes →'}
            </button>
          </div>
          <CalendarioActividad email={email} semanas={mesExpandido ? 5 : 1} />
          <div className="mt-3 flex items-center gap-1.5 text-[11px] text-faint">
            <span>Menos</span>
            {NIVEL_COLOR.map((c) => (
              <span key={c} className="h-3 w-3 rounded-[3px]" style={{ background: c }} />
            ))}
            <span>Más</span>
            <span className="ml-2 italic">
              (actividad simulada para este mockup — vendría del historial real de Unity VC)
            </span>
          </div>
        </Card>

        <Card>
          <h3 className="mb-4 text-sm font-semibold text-text">
            Tareas asignadas <span className="text-faint">({asignadas.length})</span>
          </h3>
          {asignadas.length === 0 ? (
            <p className="text-sm text-muted">No tiene ninguna tarea asignada ahora mismo.</p>
          ) : (
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
              {ESTADOS.map((estado) => {
                const items = porEstado(estado);
                const m = estadoMeta[estado];
                return (
                  <div key={estado} className={`rounded-xl border p-3 ${m.bg} ${m.border}`}>
                    <div className="mb-2 flex items-center gap-2">
                      <span className="h-2 w-2 rounded-full" style={{ background: m.dot }} />
                      <span className="text-xs font-semibold text-text">{m.label}</span>
                      <span className="ml-auto rounded-full bg-surface-3 px-2 py-0.5 text-[11px] text-muted">
                        {items.length}
                      </span>
                    </div>
                    <div className="space-y-1.5">
                      {items.map((t) => (
                        <div key={t.id} className="rounded-lg bg-surface-2/80 p-2">
                          <p className="truncate text-xs font-medium text-text">{t.titulo}</p>
                          <div className="mt-1 flex items-center gap-1">
                            <TipoBadge tipo={t.tipo} />
                          </div>
                        </div>
                      ))}
                      {items.length === 0 && <p className="text-[11px] text-faint">— vacío —</p>}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
