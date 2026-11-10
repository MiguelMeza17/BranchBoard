import { useEffect, useState } from 'react';

/** `true` mientras la ventana cumple la media query (ej. `(max-width: 899px)`). */
export function useMediaQuery(query: string): boolean {
  const [cumple, setCumple] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const mq = window.matchMedia(query);
    const alCambiar = () => setCumple(mq.matches);
    alCambiar();
    mq.addEventListener('change', alCambiar);
    return () => mq.removeEventListener('change', alCambiar);
  }, [query]);
  return cumple;
}

/** Pantalla angosta (celular o tablet en vertical): el menú lateral pasa a ser un cajón. */
export const PANTALLA_ANGOSTA = '(max-width: 899px)';
