export interface LineaDiff {
  tipo: 'igual' | 'agregada' | 'borrada';
  texto: string;
  anterior: number | null;
  nueva: number | null;
}

export interface ArchivoDiff {
  ruta: string;
  ruta_anterior: string | null;
  estado: string;
  binario: boolean;
  aviso: string | null;
  lineas: LineaDiff[] | null;
}

export interface Diferencias { archivos: ArchivoDiff[] }

/** Solo para el navegador; el escritorio siempre pide datos al backend. */
export function diffEjemplo(): Diferencias {
  return { archivos: [
    {
      ruta: '/Assets/Scripts/Jugador.cs', ruta_anterior: null, estado: 'modificado', binario: false, aviso: null,
      lineas: [
        { tipo: 'igual', texto: 'public class Jugador\n', anterior: 1, nueva: 1 },
        { tipo: 'igual', texto: '{\n', anterior: 2, nueva: 2 },
        { tipo: 'borrada', texto: '    float velocidad = 3f;\n', anterior: 3, nueva: null },
        { tipo: 'agregada', texto: '    float velocidad = 5f;\n', anterior: null, nueva: 3 },
        { tipo: 'igual', texto: '}\n', anterior: 4, nueva: 4 },
      ],
    },
    { ruta: '/Assets/Imagenes/jugador.png', ruta_anterior: null, estado: 'agregado', binario: true, aviso: null, lineas: null },
  ] };
}
