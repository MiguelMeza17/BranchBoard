import assert from 'node:assert/strict';

// Ajustes real y exportación por IPC/Supabase del ejemplo probar_exportacion_277.
// El destino del selector se controla en el auxiliar; las otras acciones de Ajustes no se ejecutan.
const puerto = process.env.BB_PRUEBA_CDP ?? '9465';
let pagina;
for (let intento = 0; intento < 100 && !pagina; intento++) {
  try {
    const paginas = await (await fetch(`http://127.0.0.1:${puerto}/json`)).json();
    pagina = paginas.find((p) => p.type === 'page' && p.url.startsWith('http://127.0.0.9:4300'));
    if (!pagina) await new Promise((resolve) => setTimeout(resolve, 250));
  }
  catch { await new Promise((resolve) => setTimeout(resolve, 250)); }
}
assert.ok(pagina, 'falta la WebView de prueba #277');
const ws = new WebSocket(pagina.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.addEventListener('open', resolve, { once: true });
  ws.addEventListener('error', reject, { once: true });
});
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
async function evaluar(expression) {
  const clave = ++id;
  const respuesta = new Promise((resolve, reject) => pendientes.set(clave, { resolve, reject }));
  ws.send(JSON.stringify({ id: clave, method: 'Runtime.evaluate', params: {
    expression: `(async()=>{${expression}})()`, awaitPromise: true, returnByValue: true,
  } }));
  const result = await respuesta;
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? 'falló la prueba');
  return result.result.value;
}
async function esperar(expression) {
  for (let intento = 0; intento < 100; intento++) {
    if (await evaluar(`return !!(${expression});`)) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`No se cumplió: ${expression}`);
}
try {
  await esperar(`window.__TAURI_INTERNALS__ && performance.getEntriesByType('resource').some(e=>e.name.includes('/src/store/useStore.ts'))`);
  await evaluar(`
    const recursos=performance.getEntriesByType('resource');
    const {useStore}=await import(recursos.find(e=>e.name.includes('/src/store/useStore.ts')).name);
    const {api}=await import(recursos.find(e=>e.name.includes('/src/lib/api.ts')).name);
    const invoke=window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    const info=await invoke('datos_prueba');
    window.__prueba277={store:useStore,api,invoke,info};
    api.proyectoEnElServidor=async()=>info.proyecto;
    api.obtenerConfiguracionProyecto=async()=>({nombre:'Exportacion UI 277',imagen:null});
    api.sincronizarEventos=async()=>({aplicados:0,descartados:0,desconocidos:0,ilegibles:0});
    const noop=async()=>{};
    useStore.setState({accesoCompletado:true,proyectoActual:'Exportacion UI 277',proyectoLocal:null,
      currentEmail:info.correo,cmSesionActiva:false,tareas:[],sprints:[],backups:[],
      usuarios:[{email:info.correo,nombre:'Prueba',apellido:'Exportar',rol:'Administrador',fecha_alta:''}],
      cargarCommits:noop,cargarTareas:noop,refrescarRamaWorkspace:noop,cargarSprints:noop,cargarWorkspacesYProyectos:noop});
    location.hash='#/ajustes';
  `);
  await esperar(`document.querySelector('h1')?.textContent==='Ajustes'`);
  for(const rol of ['Líder','Desarrollador','Tester','Colaborador externo']) {
    await evaluar(`__prueba277.store.setState({usuarios:[{email:__prueba277.info.correo,nombre:'Prueba',apellido:'Exportar',rol:${JSON.stringify(rol)},fecha_alta:''}]});`);
    await new Promise(resolve=>setTimeout(resolve,100));
    assert.equal(await evaluar(`return [...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Exportar .bbproyecto');`),false);
  }
  await evaluar(`__prueba277.store.setState({usuarios:[{email:__prueba277.info.correo,nombre:'Prueba',apellido:'Exportar',rol:'Administrador',fecha_alta:''}]});`);
  await esperar(`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Exportar .bbproyecto')`);
  await evaluar(`
    const seccion=[...document.querySelectorAll('details')].find(d=>d.querySelector('h2')?.textContent==='Exportar proyecto');
    if(!seccion)throw new Error('Falta Exportar proyecto');
    seccion.querySelector('summary').click();
    const boton=[...seccion.querySelectorAll('button')].find(b=>b.textContent.trim()==='Exportar .bbproyecto');
    boton.click();
  `);
  await esperar(`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Exportando…'&&b.disabled)`);
  await evaluar(`[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Exportando…').click();`);
  await esperar(`document.body.textContent.includes('Proyecto exportado en ')`);
  const resultado=await evaluar(`return await __prueba277.invoke('comprobar_respaldo');`);
  assert.deepEqual(resultado,{exportaciones:1,archivo:true,desvinculado:true,administrador_correcto:true});
  console.log('OK Tauri: exportación visible solo a Administrador, bloqueo de doble envío, IPC real crea archivo importable y desvinculado');
  await evaluar(`__prueba277.api.exportarProyectoLocal=async()=>null;[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Exportar .bbproyecto').click();`);
  await esperar(`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Exportar .bbproyecto'&&!b.disabled)`);
  await evaluar(`__prueba277.api.exportarProyectoLocal=async()=>{throw new Error('Sin conexión para la prueba');};[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Exportar .bbproyecto').click();`);
  await esperar(`document.body.textContent.includes('Sin conexión para la prueba')`);
  await esperar(`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Exportar .bbproyecto'&&!b.disabled)`);
  console.log('OK interfaz: cancelación y error controlados liberan el botón; el error se muestra');
} finally {
  void evaluar(`await window.__TAURI_INTERNALS__.invoke('terminar_prueba');`).catch(()=>{});
  await new Promise(resolve=>setTimeout(resolve,1000));
  ws.close();
}
