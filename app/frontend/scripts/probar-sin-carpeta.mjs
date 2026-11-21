import assert from 'node:assert/strict';

// App real en una WebView oculta del ejemplo probar_sin_carpeta_244.
// Solo el acceso al workspace usa IPC real; los otros paneles reciben datos controlados.
const puerto = process.env.BB_PRUEBA_CDP ?? '9461';
let pagina;
for (let intento = 0; intento < 100 && !pagina; intento++) {
  try {
    const paginas = await (await fetch(`http://127.0.0.1:${puerto}/json`)).json();
    pagina = paginas.find((p) => p.type === 'page' && p.url.startsWith('http://127.0.0.8:4300'));
    if (!pagina) await new Promise((resolve) => setTimeout(resolve, 250));
  }
  catch { await new Promise((resolve) => setTimeout(resolve, 250)); }
}
assert.ok(pagina, 'falta la WebView de prueba #244');
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
    const ruta=performance.getEntriesByType('resource').find(e=>e.name.includes('/src/store/useStore.ts')).name;
    const {useStore}=await import(ruta);
    const rutaApi=performance.getEntriesByType('resource').find(e=>e.name.includes('/src/lib/api.ts')).name;
    const {api}=await import(rutaApi);
    const invoke=window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    const workspace=await invoke('datos_prueba');
    window.__prueba244={store:useStore,invoke,workspace,comandos:[]};
    const entrar=api.entrarAProyecto;
    api.entrarAProyecto=(...args)=>{
      __prueba244.comandos.push('entrar_a_proyecto');
      return entrar(...args);
    };
    for(const [metodo,comando] of [['carpetaCompartida','carpeta_compartida'],['configurarCarpetaCompartida','configurar_carpeta_compartida']]) {
      if(metodo in api) throw new Error('El cliente conserva '+metodo);
      api[metodo]=async()=>{__prueba244.comandos.push(comando);throw new Error('Comando retirado');};
    }
    api.proyectoEnElServidor=async()=> 'proyecto-controlado';
    api.obtenerConfiguracionProyecto=async()=>({nombre:'BranchBoard-Pruebas',imagen:null});
    api.sincronizarEventos=async()=>({aplicados:0,descartados:0,desconocidos:0,ilegibles:0});
    const noop=async()=>{};
    useStore.setState({accesoCompletado:true,proyectoActual:'',cmInstalado:true,cmSesionActiva:false,accesoIrAWorkspaces:true,
      workspaces:[workspace],cargarWorkspacesYProyectos:noop,cargarCommits:noop,cargarTareas:noop,
      refrescarRamaWorkspace:noop,cargarSprints:noop,usuarios:[],tareas:[],sprints:[],backups:[]});
  `);
  await esperar(`__prueba244.store.getState().accesoCompletado`);
  await evaluar(`__prueba244.store.setState({accesoCompletado:false});`);
  await esperar(`[...document.querySelectorAll('h2')].some(h=>h.textContent==='Tus workspaces de Unity VC')`);
  await evaluar(`
    const b=[...document.querySelectorAll('button')].find(b=>b.textContent.includes('BranchBoard-Pruebas'));
    if(!b)throw new Error('Falta el workspace'); b.click();
  `);
  await esperar(`__prueba244.store.getState().accesoCompletado&&__prueba244.store.getState().proyectoActual==='BranchBoard-Pruebas'`);
  assert.equal(await evaluar(`return __prueba244.comandos.filter(c=>c==='entrar_a_proyecto').length;`), 1);
  assert.equal(await evaluar(`return document.body.textContent.includes('Carpeta compartida');`), false);
  console.log('OK Tauri: un clic abre el workspace por IPC real sin pedir carpeta compartida');

  await evaluar(`location.hash='#/ajustes';`);
  await esperar(`document.querySelector('h1')?.textContent==='Ajustes'`);
  for (const rol of ['Administrador', 'Líder', 'Desarrollador', 'Tester']) {
    await evaluar(`
      const email=__prueba244.store.getState().currentEmail;
      __prueba244.store.setState({usuarios:[{email,nombre:'Prueba',apellido:'244',rol:${JSON.stringify(rol)},fecha_alta:''}]});
    `);
    await new Promise((resolve) => setTimeout(resolve, 100));
    assert.equal(await evaluar(`return document.body.textContent.includes('Carpeta compartida');`), false);
  }
  await evaluar(`
    const email=__prueba244.store.getState().currentEmail;
    __prueba244.store.setState({usuarios:[{email,nombre:'Prueba',apellido:'244',rol:'Administrador',fecha_alta:''}],proyectoLocal:'C:/Prueba244/local'});
  `);
  await esperar(`document.body.textContent.includes('Exportar .bbproyecto')`);
  const retirados = await evaluar(`return __prueba244.comandos.filter(c=>c==='carpeta_compartida'||c==='configurar_carpeta_compartida');`);
  assert.deepEqual(retirados, []);
  console.log('OK Tauri: Ajustes sin carpeta en cuatro roles y exportación local conservada; cero comandos retirados');
} finally {
  void evaluar(`await window.__TAURI_INTERNALS__.invoke('terminar_prueba');`).catch(() => {});
  await new Promise((resolve) => setTimeout(resolve, 1000));
  ws.close();
}
