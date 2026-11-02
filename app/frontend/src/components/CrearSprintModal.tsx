import { useState } from 'react';
import { Modal } from './Modal';
import { useStore } from '../store/useStore';

// "Nuevo sprint" (con fecha: empieza hoy y dura N días; al terminar genera su reporte) y "Nuevo
// backlog" (sin fecha: agrupa tareas sin plazo). Los dos son sprints en el modelo de datos
// (`con_fecha` true/false, ver BACKEND.md "Modelo de datos: tabla sprints"). Solo Líder/Administrador.
export function CrearSprintModal({ tipo, onClose }: { tipo: 'sprint' | 'backlog' | null; onClose: () => void }) {
  const crearSprint = useStore((s) => s.crearSprint);
  const [nombre, setNombre] = useState('');
  const [descripcion, setDescripcion] = useState('');
  const [dias, setDias] = useState('14');
  const [tocado, setTocado] = useState(false);
  const esSprint = tipo === 'sprint';
  const nDias = Number(dias);
  const errorNombre = !nombre.trim() ? 'Escribe el nombre.' : null;
  const errorDias = esSprint && (!Number.isInteger(nDias) || nDias < 1) ? 'La duración debe ser un número de días (1 o más).' : null;

  const cerrar = () => {
    setNombre('');
    setDescripcion('');
    setDias('14');
    setTocado(false);
    onClose();
  };

  return (
    <Modal
      open={!!tipo}
      onClose={cerrar}
      title={esSprint ? 'Nuevo sprint' : 'Nuevo backlog'}
      subtitle={esSprint ? 'Empieza hoy y dura los días que indiques' : 'Agrupa tareas sin fecha límite'}
      size="sm"
      footer={
        <div className="flex w-full justify-end gap-2">
          <button type="button" className="btn-ghost btn-sm" onClick={cerrar}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn-primary btn-sm"
            onClick={() => {
              if (errorNombre || errorDias) {
                setTocado(true);
                return;
              }
              crearSprint(nombre.trim(), descripcion.trim(), esSprint, esSprint ? nDias : null);
              cerrar();
            }}
          >
            Crear
          </button>
        </div>
      }
    >
      <div className="space-y-3 text-sm">
        <label className="block">
          <span className="mb-1 block text-xs text-faint">Nombre</span>
          <input
            className={`input ${tocado && errorNombre ? 'border-red-500/70' : ''}`}
            autoFocus
            placeholder={esSprint ? 'Sprint 1' : 'Backlog de arte'}
            value={nombre}
            onBlur={() => setTocado(true)}
            onChange={(e) => setNombre(e.target.value)}
          />
          {tocado && errorNombre && <span className="mt-1 block text-[11px] text-red-300">⚠ {errorNombre}</span>}
        </label>
        <label className="block">
          <span className="mb-1 block text-xs text-faint">Descripción (opcional)</span>
          <textarea
            className="input min-h-[64px] resize-y"
            value={descripcion}
            onChange={(e) => setDescripcion(e.target.value)}
          />
        </label>
        {esSprint && (
          <label className="block">
            <span className="mb-1 block text-xs text-faint">Duración (días)</span>
            <input
              className={`input w-32 ${tocado && errorDias ? 'border-red-500/70' : ''}`}
              type="number"
              min={1}
              value={dias}
              onBlur={() => setTocado(true)}
              onChange={(e) => setDias(e.target.value)}
            />
            {tocado && errorDias && <span className="mt-1 block text-[11px] text-red-300">⚠ {errorDias}</span>}
          </label>
        )}
      </div>
    </Modal>
  );
}
