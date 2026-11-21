// Usa la App real de la WebView con dos cuentas reales del auxiliar probar_refresco_239.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const modo=process.argv[2]??'green';
let page;
for(let intento=0;intento<100;intento++){
  try {
    const pages=await(await fetch('http://127.0.0.1:9457/json/list')).json();
    page=pages.find(p=>p.type==='page'&&p.url.includes('refresco239.html'));
    if(page)break;
  }catch{}
  await new Promise(r=>setTimeout(r,300));
}
assert.ok(page,'WebView de prueba');
const ws=new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r,j)=>{ws.addEventListener('open',r,{once:true});ws.addEventListener('error',j,{once:true});});
let id=0;const pending=new Map();
ws.addEventListener('message',e=>{const m=JSON.parse(e.data),p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.j(new Error(JSON.stringify(m.error))):p.r(m.result);}});
const call=(method,params={})=>new Promise((r,j)=>{const n=++id;pending.set(n,{r,j});ws.send(JSON.stringify({id:n,method,params}));});
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function evaluate(body){const r=await call('Runtime.evaluate',{expression:`(async()=>{${body}})()`,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw new Error(r.exceptionDetails.exception?.description??'error de WebView');return r.result.value;}
async function until(expr,timeout=25000){const fin=Date.now()+timeout;while(Date.now()<fin){if(await evaluate(`return !!(${expr});`))return;await delay(200);}throw new Error(`Tiempo agotado: ${expr}`);}
async function captura(nombre){if(!process.env.BB239_EVIDENCIA)return;const r=await call('Page.captureScreenshot',{format:'png'});fs.writeFileSync(`${process.env.BB239_EVIDENCIA}/${nombre}.png`,Buffer.from(r.data,'base64'));}
if(modo==='close'){
  await call('Runtime.evaluate',{expression:"window.__refresco239.invoke('terminar_prueba')"});
  ws.close();
  process.exit(0);
}
try {
  await until('window.__refresco239 && document.body.textContent.includes("Lista de tareas")');
  assert.equal(await evaluate('return __refresco239.store.getState().cmSesionActiva;'),false);
  if(modo==='verify'){
    assert.equal(await evaluate('return __refresco239.store.getState().tareas.length;'),2);
    assert.equal(await evaluate('return __refresco239.store.getState().versionDatos;'),3,'un solo aviso por cada creacion/edicion, tambien con StrictMode');
    console.log('OK: tres cambios remotos producen tres avisos de versionDatos; no hay suscripciones duplicadas');
  }else if(modo==='red'){
    await evaluate("await __refresco239.invoke('cambio_remoto',{editar:false});");
    await delay(15000);
    const backend=await evaluate("return (await __refresco239.invoke('listar_tareas')).some(t=>t.titulo==='Cambio remoto 239 nuevo');");
    const pantalla=await evaluate("return document.body.textContent.includes('Cambio remoto 239 nuevo');");
    assert.ok(backend,'el cambio remoto llego a SQLite');
    assert.equal(pantalla,false,'reproduccion: sin Unity no se refrescaba la pantalla');
    await captura('antes-sin-refresco');
    console.log('REPRODUCIDO: la tarea de otra cuenta llega a SQLite y no aparece en la App sin sesion Unity');
  }else{
    assert.ok(await evaluate('return __refresco239.store.getState().tareas.length<=1;'),'usar un proyecto nuevo para la corrida completa');
    if(await evaluate('return __refresco239.store.getState().tareas.length===0;')){
      const origen=await evaluate('return performance.timeOrigin;');
      await evaluate("await __refresco239.invoke('cambio_remoto',{editar:false});");
      await until("document.body.textContent.includes('Cambio remoto 239 nuevo') && __refresco239.store.getState().tareas.length===1");
      assert.equal(await evaluate('return performance.timeOrigin;'),origen);
      console.log('OK: primera tarea de otra cuenta aparece automaticamente en la lista vacia');
    }
    await until("__refresco239.store.getState().tareas.length===1");
    const inicio=await evaluate('return {origen:performance.timeOrigin,version:__refresco239.store.getState().versionDatos};');
    const comienzo=Date.now();
    await evaluate("await __refresco239.invoke('cambio_remoto',{editar:true});");
    await until("document.body.textContent.includes('Cambio remoto 239 editado')");
    assert.ok(await evaluate(`return __refresco239.store.getState().versionDatos>${inicio.version};`));
    assert.equal(await evaluate('return performance.timeOrigin;'),inicio.origen,'sin recargar la WebView');
    console.log(`OK: edicion de otra cuenta visible automaticamente en ${Date.now()-comienzo}ms`);
    await captura('despues-edicion-automatica');
    await evaluate("await __refresco239.invoke('cambio_remoto',{editar:false});");
    await until("document.body.textContent.includes('Cambio remoto 239 nuevo') && __refresco239.store.getState().tareas.length===2");
    await delay(11000);
    assert.equal(await evaluate('return __refresco239.store.getState().tareas.length;'),2,'sin duplicar eventos propios o recibidos');
    assert.equal(await evaluate('return performance.timeOrigin;'),inicio.origen);
    assert.equal(await evaluate('return __refresco239.store.getState().cmSesionActiva;'),false);
    assert.equal(await evaluate('return __refresco239.store.getState().versionDatos;'),3,'un aviso por cambio en StrictMode');
    await captura('despues-creacion-automatica');
    console.log('OK: tarea nueva de otra cuenta visible sin recargar; dos vueltas adicionales sin duplicados; Unity desconectado');
  }
}finally{ws.close();}
