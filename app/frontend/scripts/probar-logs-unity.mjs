// Ejecutar con el auxiliar nativo probar_logs_153 y su WebView en CDP 9453.
import assert from 'node:assert/strict';
import fs from 'node:fs';

const pages = await (await fetch('http://127.0.0.1:9453/json/list')).json();
const page = pages.find(p => p.type === 'page' && p.url.includes('logs153.html'));
assert.ok(page, 'WebView aislada de #153');
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve,reject) => { ws.addEventListener('open',resolve,{once:true}); ws.addEventListener('error',reject,{once:true}); });
let id=0; const pending=new Map();
ws.addEventListener('message',e=>{const m=JSON.parse(e.data),p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.reject(new Error(JSON.stringify(m.error))):p.resolve(m.result);}});
const call=(method,params={})=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function evaluate(body){const r=await call('Runtime.evaluate',{expression:`(async()=>{${body}})()`,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw new Error(r.exceptionDetails.exception?.description??'error en WebView');return r.result.value;}
async function until(expr){for(let n=0;n<100;n++){if(await evaluate(`return !!(${expr});`))return;await delay(100);}throw new Error(`Tiempo agotado: ${expr}`);}
async function click(text){await evaluate(`const b=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===${JSON.stringify(text)});if(!b)throw new Error('Falta boton');b.click();`);await delay(100);}
async function modo(value){await evaluate(`const e=document.querySelector('select');e.value=${JSON.stringify(value)};e.dispatchEvent(new Event('change',{bubbles:true}));`);await delay(100);await click('Terminar tarea de prueba');}
async function escribir(value){await evaluate(`const e=document.querySelector('input[placeholder=terminar]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,${JSON.stringify(value)});e.dispatchEvent(new Event('input',{bubbles:true}));`);await delay(100);}
const count=()=>evaluate('return window.__logs153.confirmaciones;');

try {
  await until('window.__logs153');
  const real=await evaluate("return await window.__TAURI_INTERNALS__.invoke('logs_unity_para_terminar',{tareaId:'T-153'});");
  assert.ok(real.contenido.length <= 64*1024*3);
  assert.ok(!real.archivo || real.archivo==='Editor.log');
  const permiso=await evaluate("try {await window.__TAURI_INTERNALS__.invoke('logs_unity_para_terminar',{tareaId:'NO-EXISTE'});return false;}catch{return true;}");
  assert.ok(permiso, 'valida la tarea antes de leer logs');
  console.log('OK: IPC nativo y lectura real del log local, sin imprimir ni guardar su contenido');

  await modo('sin');
  await until("document.querySelector('input[placeholder=terminar]')");
  assert.equal(await count(),0);
  await click('Cancelar');
  assert.equal(await count(),0);
  console.log('OK: cancelar sin logs no confirma');

  await modo('sin');await until("document.querySelector('input[placeholder=terminar]')");
  assert.equal(await evaluate("return [...document.querySelectorAll('button')].find(b=>b.textContent==='Terminar').disabled;"),true);
  await escribir('terminar');await click('Terminar');
  assert.equal(await count(),1);
  assert.ok(await evaluate("return [...document.querySelectorAll('[role=dialog] button')].every(b=>b.disabled || b.getAttribute('aria-label')==='Cerrar');"));
  await evaluate('window.__logs153.completar();');await until("!document.querySelector('[role=dialog]')");
  console.log('OK: sin logs exige confirmacion y evita doble envio durante el merge');

  await modo('errores');await until("document.querySelector('pre')");
  assert.ok(await evaluate("return document.querySelector('pre').textContent.includes('error CS1002');"));
  assert.equal(await evaluate('return !!window.__inyectado153;'),false);
  assert.equal(await count(),1);
  const shot=await call('Page.captureScreenshot',{format:'png'});
  if(process.env.BB153_CAPTURA)fs.writeFileSync(process.env.BB153_CAPTURA,Buffer.from(shot.data,'base64'));
  await escribir('terminar');await click('Terminar igualmente');assert.equal(await count(),2);
  await evaluate('window.__logs153.completar();');await until("!document.querySelector('[role=dialog]')");
  console.log('OK: errores visibles como texto; Terminar igualmente sigue siendo posible');

  await modo('fallo');await until("document.querySelector('[role=alert]')");
  assert.equal(await count(),2);await click('Cancelar');
  console.log('OK: fallo de lectura se informa y cancelar conserva la tarea');

  await modo('lento');await until("document.querySelector('[role=status]')");
  await click('Cancelar');await modo('sin');await until("document.querySelector('input[placeholder=terminar]')");
  await evaluate('window.__logs153.resolver();');await delay(200);
  assert.equal(await evaluate("return !!document.querySelector('pre');"),false);
  assert.equal(await count(),2);await click('Cancelar');
  console.log('OK: una lectura cancelada no contamina la siguiente confirmacion');
} finally {
  await call('Runtime.evaluate',{expression:"window.__TAURI_INTERNALS__.invoke('terminar_prueba')"}).catch(()=>{});
  ws.close();
}
