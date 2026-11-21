import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../../src/index.css';
import { TerminarRevisionModal } from '../../src/components/TerminarRevisionModal';
import { api, type LogsUnity } from '../../src/lib/api';
import type { Tarea } from '../../src/types';

const real = api.logsUnityParaTerminar;
const vacio: LogsUnity = { archivo: null, contenido: '', modificado: null, recortado: false, aviso: null };
let resolver: ((l: LogsUnity) => void) | null = null;
let terminar: (() => void) | null = null;
const prueba = { confirmaciones: 0, cerrar: () => {}, resolver: () => resolver?.({ ...vacio, archivo: 'Editor.log', contenido: 'resultado antiguo' }), completar: () => terminar?.() };
Object.assign(window, { __logs153: prueba });

function Fixture() {
  const [modo, setModo] = useState('sin');
  const [abierto, setAbierto] = useState(false);
  const [numero, setNumero] = useState(0);
  prueba.cerrar = () => setAbierto(false);
  const abrir = () => {
    api.logsUnityParaTerminar = modo === 'real' ? real : async () => {
      if (modo === 'lento') return new Promise<LogsUnity>((r) => { resolver = r; });
      if (modo === 'fallo') throw new Error('No se puede leer el archivo');
      return modo === 'errores' ? { ...vacio, archivo: 'Editor.log', modificado: '2026-11-21T10:00:00Z', contenido: 'Assets/Salto.cs(3,1): error CS1002: ; expected\n<script>window.__inyectado153=true</script>' } : vacio;
    };
    setNumero((n) => n + 1);
    setAbierto(true);
  };
  const tarea = { id: 'T-153', titulo: 'Revisar salto' } as Tarea;
  return <main className="p-8 text-text">
    <select aria-label="Caso" value={modo} onChange={(e) => setModo(e.target.value)}>
      {['sin', 'errores', 'fallo', 'lento', 'real'].map((m) => <option key={m}>{m}</option>)}
    </select>
    <button onClick={abrir}>Terminar tarea de prueba</button>
    <p>Confirmaciones: {prueba.confirmaciones}</p>
    {abierto && <TerminarRevisionModal key={numero} tarea={tarea} onClose={() => setAbierto(false)} onConfirm={async () => {
      prueba.confirmaciones++;
      await new Promise<void>((r) => { terminar = r; });
      setAbierto(false);
    }} />}
  </main>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
