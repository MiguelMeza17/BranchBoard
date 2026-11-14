import { useStore } from '../store/useStore';
import { api, enTauri, mensajeDeError } from '../lib/api';
import { Icono } from './Icono';

// Campo de carpeta con el botón de carpeta para buscarla en el equipo. En la app de escritorio el botón abre
// el explorador de carpetas del sistema (lo abre el backend, comando `elegir_carpeta`) y escribe aquí la
// ruta elegida; en el mockup no hay acceso al disco, así que solo avisa. La ruta también se puede escribir a mano.
export function SelectorCarpeta({
  value,
  onChange,
  disabled,
  placeholder = 'Carpeta',
  titulo,
}: {
  value: string;
  onChange: (ruta: string) => void;
  disabled?: boolean;
  placeholder?: string;
  /** Título de la ventana del explorador de carpetas. */
  titulo?: string;
}) {
  const showToast = useStore((s) => s.showToast);
  const buscar = async () => {
    if (!enTauri()) {
      showToast('En la app de escritorio aquí se abre el explorador de carpetas', 'info');
      return;
    }
    try {
      const elegida = await api.elegirCarpeta(titulo);
      if (elegida) onChange(elegida);
    } catch (e) {
      showToast(mensajeDeError(e), 'danger');
    }
  };
  return (
    <div className="flex gap-2">
      <input
        className="input min-w-0 flex-1 font-mono text-xs"
        placeholder={placeholder}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      />
      <button
        type="button"
        className="btn-outline btn-sm shrink-0 px-3 text-base"
        disabled={disabled}
        title="Buscar carpeta en tu equipo"
        aria-label="Buscar carpeta en tu equipo"
        onClick={() => void buscar()}
      >
        <Icono nombre="carpeta" className="h-4 w-4" />
      </button>
    </div>
  );
}
