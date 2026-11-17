import assert from 'node:assert/strict';
import fs from 'node:fs';

// Node 22+. Requiere el ejemplo probar_sesion_203 y el servidor Vite de la prueba.
const puerto = process.env.BB_PRUEBA_CDP ?? '9433';
const paginas = await (await fetch(`http://127.0.0.1:${puerto}/json`)).json();
const pagina = paginas.find((p) => p.type === 'page' && p.url.startsWith('http://127.0.0.3:4300'));
assert.ok(pagina, 'falta la WebView aislada #203');
const ws = new WebSocket(pagina.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.addEventListener('open', resolve, { once: true }); ws.addEventListener('error', reject, { once: true }); });
let id = 0;
const pendientes = new Map();
ws.addEventListener('message', (event) => {
  const mensaje = JSON.parse(event.data);
  const tarea = pendientes.get(mensaje.id);
  if (!tarea) return;
  pendientes.delete(mensaje.id);
  if (mensaje.error) tarea.reject(new Error(JSON.stringify(mensaje.error)));
  else tarea.resolve(mensaje.result);
});
const llamar = (method, params) => new Promise((resolve, reject) => {
  const clave = ++id;
  pendientes.set(clave, { resolve, reject });
  ws.send(JSON.stringify({ id: clave, method, params }));
});
async function evaluar(expression) {
  const result = await llamar('Runtime.evaluate', { expression: `(async()=>{${expression}})()`, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? 'falló la evaluación de la prueba');
  return result.result.value;
}
async function esperar(expression) {
  for (let intento = 0; intento < 80; intento++) {
    if (await evaluar(`return !!(${expression});`)) return;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`No se cumplió: ${expression}`);
}
try {
  await esperar('window.__TAURI_INTERNALS__');
  await evaluar(`
    const recursos=performance.getEntriesByType('resource').map(e=>e.name);
    const ruta=recursos.find(n=>n.includes('/@fs/')&&n.endsWith('/src/store/useStore.ts'))??'/src/store/useStore.ts';
    const {useStore}=await import(ruta);
    const rutaApi=recursos.find(n=>n.includes('/@fs/')&&n.endsWith('/src/lib/api.ts'))??'/src/lib/api.ts';
    const {llamar}=await import(rutaApi);
    window.__prueba203={store:useStore,llamar,invoke:window.__TAURI_INTERNALS__.invoke};
    useStore.setState({cmInstalado:true,cmSesionActiva:true,accesoCompletado:false,sesionUnityVcCaducada:false});
  `);
  await new Promise((r) => setTimeout(r, 1200));
  const codigo = await evaluar(`try {await __prueba203.llamar('simular_caducidad',{segundoPlano:false},()=>null);} catch(e){return e.codigo;}`);
  assert.equal(codigo, 'sesion_unity_vc_caducada');
  await esperar(`document.body.textContent.includes('La sesión de Unity VC caducó')`);
  assert.deepEqual(await evaluar(`return await __prueba203.invoke('contadores');`), { inicios: 0, reconexiones: 0, identidad: false });
  console.log('OK Tauri: error IPC muestra aviso sin abrir SSO');
  await evaluar(`const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Reconectar');if(!b||b.disabled)throw new Error('Reconectar no disponible');b.click();`);
  await esperar(`!__prueba203.store.getState().sesionUnityVcCaducada&&__prueba203.store.getState().cmSesionActiva`);
  assert.deepEqual(await evaluar(`return await __prueba203.invoke('contadores');`), { inicios: 1, reconexiones: 1, identidad: true });
  console.log('OK Tauri: clic Reconectar solicita SSO controlado y verifica CLI real');
  await evaluar(`await __prueba203.invoke('simular_caducidad',{segundoPlano:true});`);
  await esperar(`document.body.textContent.includes('La sesión de Unity VC caducó')`);
  assert.deepEqual(await evaluar(`return await __prueba203.invoke('contadores');`), { inicios: 1, reconexiones: 1, identidad: false });
  if (process.env.BB_PRUEBA_CAPTURA) {
    const imagen = await llamar('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(process.env.BB_PRUEBA_CAPTURA, Buffer.from(imagen.data, 'base64'));
  }
  console.log('OK Tauri: evento nativo del polling muestra aviso sin reconexión automática');
  void evaluar(`await __prueba203.invoke('terminar_prueba');`).catch(() => {});
} finally { ws.close(); }
