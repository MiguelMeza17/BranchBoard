import { useState, type InputHTMLAttributes } from 'react';
import { Icono } from './Icono';

// Contraseña con el ojito para verla mientras se escribe (y volver a ocultarla).
export function EntradaClave({ className = 'input', ...props }: InputHTMLAttributes<HTMLInputElement>) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <input {...props} type={visible ? 'text' : 'password'} className={`${className} pr-10`} />
      <button
        type="button"
        onClick={() => setVisible((v) => !v)}
        aria-label={visible ? 'Ocultar contraseña' : 'Mostrar contraseña'}
        title={visible ? 'Ocultar contraseña' : 'Mostrar contraseña'}
        className="absolute inset-y-0 right-0 flex w-10 items-center justify-center text-faint transition hover:text-text"
      >
        <Icono nombre={visible ? 'ojoTachado' : 'ojo'} className="h-4 w-4" sinColor />
      </button>
    </div>
  );
}
