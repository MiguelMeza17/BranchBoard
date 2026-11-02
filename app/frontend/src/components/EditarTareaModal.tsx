import { useState } from 'react';
import { useStore } from '../store/useStore';
import { Modal } from './Modal';
import type { Tarea } from '../types';

// "Editar tarea" desde los detalles: solo título y detalles. La edición se envía al
// Administrador/Líder para aprobarla (queda como edición pendiente; si la rechazan, se revierte).
export function EditarTareaModal({ tarea, onClose }: { tarea: Tarea; onClose: () => void }) {
  const proponerEdicion = useStore((s) => s.proponerEdicion);
  const [titulo, setTitulo] = useState(tarea.titulo);
  const [detalles, setDetalles] = useState(tarea.detalles);
  const sinCambios = titulo.trim() === tarea.titulo && detalles.trim() === tarea.detalles.trim();

  return (
    <Modal
      open
      onClose={onClose}
      title="Editar tarea"
      subtitle={`${tarea.id} · la edición la aprueba un Administrador o Líder`}
      size="lg"
      footer={
        <div className="flex w-full justify-end gap-2">
          <button type="button" className="btn-ghost btn-sm" onClick={onClose}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn-primary btn-sm"
            disabled={!titulo.trim() || sinCambios}
            onClick={() => {
              proponerEdicion(tarea.id, { titulo: titulo.trim(), detalles: detalles.trim() });
              onClose();
            }}
          >
            Enviar para aprobación
          </button>
        </div>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="label">Título</label>
          <input className="input" value={titulo} onChange={(e) => setTitulo(e.target.value)} />
        </div>
        <div>
          <label className="label">
            Detalles <span className="text-faint">(opcional)</span>
          </label>
          <textarea
            className="input min-h-[200px] resize-y"
            value={detalles}
            onChange={(e) => setDetalles(e.target.value)}
            placeholder="Qué hay que hacer a detalle: los pasos, dónde está cada cosa, lo que haga falta explicar…"
          />
        </div>
      </div>
    </Modal>
  );
}
