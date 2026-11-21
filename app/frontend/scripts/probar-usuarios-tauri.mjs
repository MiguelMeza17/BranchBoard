import assert from 'node:assert/strict';

// Pantalla Usuarios real en Tauri con dos cuentas de branchboard-pruebas (auxiliar probar_usuarios_231).
// A agrega a B, invita a un correo sin cuenta, cambia el rol de B y quita a ambos; B lo ve desde su sesión.
const puerto = process.env.BB_PRUEBA_CDP ?? '9467';
let pagina;
for (let intento = 0; intento < 200 && !pagina; intento++) {
  try {
    const paginas = await (await fetch(`http://127.0.0.1:${puerto}/json`)).json();
    pagina = paginas.find((p) => p.type === 'page' && p.url.startsWith('http://127.0.0.9:4300'));
    if (!pagina) await new Promise((resolve) => setTimeout(resolve, 250));
  }
  catch { await new Promise((resolve) => setTimeout(resolve, 250)); }
}
assert.ok(pagina, 'falta la WebView de prueba #231');
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
async function esperar(expression, intentos = 120) {
  for (let intento = 0; intento < intentos; intento++) {
    if (await evaluar(`return !!(${expression});`)) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`No se cumplió: ${expression}`);
}
const fila = (correo) => `[...document.querySelectorAll('tbody tr')].find(tr=>tr.textContent.includes(${JSON.stringify(correo)}))`;
const libre = `!__p.store.getState().equipoOcupado && !__p.store.getState().usuariosCargando`;

try {
  await esperar(`window.__TAURI_INTERNALS__ && performance.getEntriesByType('resource').some(e=>e.name.includes('/src/store/useStore.ts'))`);
  const info = await evaluar(`
    const recursos=performance.getEntriesByType('resource');
    const {useStore}=await import(recursos.find(e=>e.name.includes('/src/store/useStore.ts')).name);
    const {api}=await import(recursos.find(e=>e.name.includes('/src/lib/api.ts')).name);
    const invoke=window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    const info=await invoke('datos_prueba');
    window.__p={store:useStore,api,invoke,info};
    api.proyectoEnElServidor=async()=>info.proyecto;
    api.obtenerConfiguracionProyecto=async()=>({nombre:'Usuarios UI 231',imagen:null});
    api.sincronizarEventos=async()=>({aplicados:0,descartados:0,desconocidos:0,ilegibles:0});
    const noop=async()=>{};
    useStore.setState({accesoCompletado:true,proyectoActual:'Usuarios UI 231',proyectoLocal:null,
      currentEmail:info.a,cmSesionActiva:false,tareas:[],sprints:[],backups:[],
      usuarios:[{email:info.a,nombre:'Ana',apellido:'Prueba',rol:'Administrador',fecha_alta:''}],
      cargarCommits:noop,cargarTareas:noop,refrescarRamaWorkspace:noop,cargarSprints:noop,cargarWorkspacesYProyectos:noop});
    location.hash='#/usuarios';
    return info;
  `);
  await esperar(`document.querySelector('h1')?.textContent==='Usuarios'`);
  await esperar(`${fila(info.a)}?.textContent.includes('(tú)')`);

  // B todavía no está en el proyecto: el servidor no le deja ver el equipo.
  assert.match(await evaluar(`try{await __p.invoke('vista_de_b');return 'vio';}catch(e){return String(e);}`), /.+/);
  assert.notEqual(await evaluar(`try{await __p.invoke('vista_de_b');return 'vio';}catch(e){return 'rechazado';}`), 'vio');

  async function agregar(correo, rol) {
    await esperar(libre);
    await evaluar(`
      const seccion=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent.includes('Agregar usuario'));
      if(!seccion.open) seccion.querySelector('summary').click();
      const fijar=(el,v)=>{const p=Object.getPrototypeOf(el);Object.getOwnPropertyDescriptor(p,'value').set.call(el,v);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));};
      fijar(seccion.querySelector('input[type=email]'),${JSON.stringify(correo)});
      fijar(seccion.querySelector('select'),${JSON.stringify(rol)});
    `);
    await esperar(`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Agregar'&&!b.disabled)`);
    await evaluar(`[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Agregar').click();`);
    await esperar(`${fila(correo)}`);
  }
  await agregar(info.b, 'Desarrollador');
  await agregar(info.invitado, 'Tester');
  await esperar(`${fila(info.invitado)}?.textContent.includes('Invitado · aún sin cuenta')`);
  console.log('OK: A agrega a B e invita a un correo sin cuenta desde la pantalla');

  let vistaB = await evaluar(`return await __p.invoke('vista_de_b');`);
  assert.equal(vistaB.length, 2, 'B ve a A y a sí mismo, sin la invitación');
  assert.ok(vistaB.every((u) => u.email === null), 'un Desarrollador no ve correos');
  assert.equal(vistaB.find((u) => u.propio)?.rol, 'Desarrollador');
  assert.notEqual(await evaluar(`try{await __p.invoke('b_intenta_degradar_a_a');return 'pudo';}catch(e){return 'rechazado';}`), 'pudo');
  console.log('OK: B, desde su sesión, se ve como Desarrollador, sin correos ni invitados, y el servidor le rechaza cambiar el rol de A');

  await esperar(libre);
  await evaluar(`
    const sel=${fila(info.b)}.querySelector('select');
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(sel,'Líder');
    sel.dispatchEvent(new Event('change',{bubbles:true}));
  `);
  await esperar(`${fila(info.b)}?.querySelector('select')?.value==='Líder' && ${libre}`);
  vistaB = await evaluar(`return await __p.invoke('vista_de_b');`);
  assert.equal(vistaB.find((u) => u.propio)?.rol, 'Líder');
  assert.ok(vistaB.some((u) => u.invitado), 'como Líder ya ve la invitación');
  console.log('OK: el cambio de rol desde la pantalla llega al servidor y B lo ve como Líder');

  async function quitar(correo) {
    await esperar(libre);
    await evaluar(`[...${fila(correo)}.querySelectorAll('button')].find(b=>/Eliminar|Quitar invitación/.test(b.textContent)).click();`);
    await esperar(`[...document.querySelectorAll('input')].some(i=>i.placeholder==='eliminar')`, 40);
    await evaluar(`
      const campos=[...document.querySelectorAll('input[placeholder=eliminar]')];
      const campo=campos[campos.length-1];
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(campo,'eliminar');
      campo.dispatchEvent(new Event('input',{bubbles:true}));
    `);
    await esperar(`[...document.querySelectorAll('button')].some(b=>b.textContent.trim()==='Eliminar del equipo'&&!b.disabled)`);
    await evaluar(`[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Eliminar del equipo').click();`);
    await esperar(`!${fila(correo)} && ${libre}`);
  }
  await quitar(info.invitado);
  await quitar(info.b);
  assert.notEqual(await evaluar(`try{await __p.invoke('vista_de_b');return 'vio';}catch(e){return 'rechazado';}`), 'vio');
  const final = await evaluar(`return await __p.invoke('listar_usuarios');`);
  assert.deepEqual(final.map((u) => u.email), [info.a]);
  console.log('OK: quitar la invitación y a B desde la pantalla; B pierde el acceso en el servidor');
} finally {
  void evaluar(`await window.__TAURI_INTERNALS__.invoke('terminar_prueba');`).catch(() => {});
  await new Promise((resolve) => setTimeout(resolve, 1000));
  ws.close();
}
