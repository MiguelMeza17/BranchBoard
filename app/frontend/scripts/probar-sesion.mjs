import assert from 'node:assert/strict';
import { build } from 'vite';

// Usa el bundler del proyecto; no necesita instalar otro runner ni abrir Unity.
async function cargar(entry, plugins = []) {
  const resultado = await build({
    configFile: false,
    logLevel: 'silent',
    plugins,
    build: {
      write: false,
      minify: false,
      lib: { entry, formats: ['es'] },
    },
  });
  const salida = Array.isArray(resultado) ? resultado[0] : resultado;
  const codigo = salida.output.find((item) => item.type === 'chunk' && item.isEntry).code;
  return import(`data:text/javascript;base64,${Buffer.from(codigo).toString('base64')}`);
}

const eventos = new EventTarget();
const invocaciones = [];
globalThis.window = Object.assign(eventos, {
  __TAURI_INTERNALS__: {
    invoke: async (comando) => {
      invocaciones.push(comando);
      throw { codigo: 'sesion_unity_vc_caducada', mensaje: 'La sesión caducó. Pulsa Reconectar.' };
    },
  },
});
const real = await cargar('src/lib/api.ts');
let avisos = 0;
window.addEventListener(real.EVENTO_SESION_CADUCADA, () => { avisos++; });
await assert.rejects(
  real.llamar('operacion', undefined, () => null),
  (error) => error instanceof real.ErrorBackend && error.codigo === 'sesion_unity_vc_caducada',
);
assert.equal(avisos, 1);
assert.deepEqual(invocaciones, ['operacion'], 'no reintenta ni inicia SSO por detectar el error');
window.__TAURI_INTERNALS__.invoke = async () => { throw { codigo: 'cm', mensaje: 'No hay red' }; };
await assert.rejects(real.llamar('operacion', undefined, () => null));
assert.equal(avisos, 1, 'un fallo de red no caduca la sesión');

globalThis.__apiSesion = {};
const store = await cargar('src/store/useStore.ts', [{
  name: 'backend-controlado-para-sesion',
  enforce: 'pre',
  resolveId(source) { if (source === '../lib/api') return '\0api-sesion'; },
  load(id) {
    if (id !== '\0api-sesion') return;
    return `export const enTauri = () => true;
      export class ErrorBackend extends Error { constructor(codigo, mensaje) { super(mensaje); this.codigo=codigo; this.mensaje=mensaje; } }
      export const mensajeDeError = e => e.message;
      export const api = new Proxy({}, { get: (_, key) => globalThis.__apiSesion[key] });`;
  },
}]);
const get = store.useStore.getState;
const tareas = [{ id: 'T-203', estado: 'en producción' }];
store.useStore.setState({ tareas, cmSesionActiva: true, accesoCompletado: true });
get().marcarSesionUnityVcCaducada();
assert.equal(get().cmSesionActiva, false);
assert.equal(get().accesoCompletado, false);
assert.equal(get().sesionUnityVcCaducada, true);
assert.equal(get().tareas, tareas, 'conserva los datos del proyecto');
let completarConsulta;
store.useStore.setState({ sesionUnityVcCaducada: false, cmSesionActiva: true });
__apiSesion.sesionUnityVc = () => new Promise((resolve) => { completarConsulta = resolve; });
const consultaPendiente = get().cargarSesionUnityVc();
get().marcarSesionUnityVcCaducada();
completarConsulta({ activa: true, email: 'dev@example.test' });
await consultaPendiente;
assert.equal(get().cmSesionActiva, false, 'una respuesta local anterior no restaura la sesión');
assert.equal(get().sesionUnityVcCaducada, true);
__apiSesion.sesionUnityVc = () => { throw new Error('whoami local no debe saltarse la reconexión'); };
await get().cargarSesionUnityVc();
assert.equal(get().sesionUnityVcCaducada, true);

const llamadas = [];
let completar;
__apiSesion.iniciarSesionUnityVc = (forzar) => {
  llamadas.push(['iniciar', forzar]);
  return new Promise((resolve) => { completar = resolve; });
};
__apiSesion.reconectarUnityVc = async () => { llamadas.push(['reconectar']); return { email: 'dev@example.test', mensaje: 'Conexión verificada' }; };
const pendiente = get().reconectarUnityVc();
assert.deepEqual(llamadas, [['iniciar', true]], 'pide SSO antes de verificar PAT/CLI');
assert.equal(get().cmSesionActiva, false);
completar({ activa: false, email: null });
assert.equal(await pendiente, false);
assert.equal(get().sesionUnityVcCaducada, true, 'cancelar no restaura la sesión');
assert.deepEqual(llamadas, [['iniciar', true]]);

llamadas.length = 0;
__apiSesion.iniciarSesionUnityVc = async (forzar) => { llamadas.push(['iniciar', forzar]); return { activa: true, email: 'dev@example.test' }; };
assert.equal(await get().reconectarUnityVc(), true);
assert.deepEqual(llamadas, [['iniciar', true], ['reconectar']]);
assert.equal(get().cmSesionActiva, true);
assert.equal(get().sesionUnityVcCaducada, false);
assert.equal(get().accesoCompletado, false, 'vuelve al acceso normal del proyecto');
llamadas.length = 0;
assert.equal(await get().reconectarUnityVc(), true);
assert.deepEqual(llamadas, [['reconectar']], 'una sesión sana no fuerza SSO');
console.log('OK: error de sesión, aviso sin reintentos, datos conservados, cancelación y reconexión explícita');
