import { useEffect, useState, type ReactNode } from 'react';
import { Modal } from './Modal';

interface Props {
  open: boolean;
  onClose: () => void;
  onConfirm: (justificacion?: string) => void;
  title: string;
  description: ReactNode;
  /** palabra literal que se debe escribir para habilitar el botón (ej. "eliminar", "asignar") */
  palabra: string;
  confirmLabel?: string;
  tone?: 'danger' | 'primary';
  /** si true, muestra un campo de justificación obligatorio antes del candado */
  requiereJustificacion?: boolean;
  justificacionLabel?: string;
}

export function ConfirmLock({
  open,
  onClose,
  onConfirm,
  title,
  description,
  palabra,
  confirmLabel = 'Confirmar',
  tone = 'danger',
  requiereJustificacion = false,
  justificacionLabel = 'Justificación',
}: Props) {
  const [texto, setTexto] = useState('');
  const [justif, setJustif] = useState('');

  useEffect(() => {
    if (open) {
      setTexto('');
      setJustif('');
    }
  }, [open]);

  const palabraOk = texto.trim().toLowerCase() === palabra.toLowerCase();
  const justifOk = !requiereJustificacion || justif.trim().length >= 3;
  const enabled = palabraOk && justifOk;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      size="sm"
      footer={
        <>
          <button className="btn-ghost" onClick={onClose}>
            Cancelar
          </button>
          <button
            className={tone === 'danger' ? 'btn-danger' : 'btn-primary'}
            disabled={!enabled}
            onClick={() => onConfirm(requiereJustificacion ? justif : undefined)}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="text-sm text-muted">{description}</div>

        {requiereJustificacion && (
          <div>
            <label className="label">{justificacionLabel} (obligatorio)</label>
            <textarea
              className="input min-h-[72px] resize-y"
              value={justif}
              onChange={(e) => setJustif(e.target.value)}
              onKeyDown={(e) => {
                // Ctrl/Cmd+Enter confirma de una — Enter solo hace salto de línea aquí.
                if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && enabled) {
                  e.preventDefault();
                  onConfirm(requiereJustificacion ? justif : undefined);
                }
              }}
              placeholder="Explica el motivo — quedará en el historial del equipo"
            />
          </div>
        )}

        <div>
          <label className="label">
            Escribe <span className="font-mono font-semibold text-text">{palabra}</span> para
            confirmar
          </label>
          <input
            className={`input font-mono ${
              texto && !palabraOk ? 'border-danger focus:border-danger focus:ring-danger/30' : ''
            }`}
            value={texto}
            onChange={(e) => setTexto(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && enabled) {
                e.preventDefault();
                onConfirm(requiereJustificacion ? justif : undefined);
              }
            }}
            placeholder={palabra}
            autoComplete="off"
            spellCheck={false}
            autoFocus
          />
          <p className="mt-1.5 flex items-center gap-1.5 text-xs text-faint">
            <span
              className={`h-1.5 w-1.5 rounded-full ${palabraOk ? 'bg-done' : 'bg-faint'}`}
            />
            Doble candado — el botón se habilita solo cuando el texto coincide exactamente.
          </p>
        </div>
      </div>
    </Modal>
  );
}
