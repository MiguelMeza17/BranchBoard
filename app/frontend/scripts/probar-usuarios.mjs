import assert from 'node:assert/strict';
import { build } from 'vite';

const calls = [];
const a = 'ana@virustf.dev', b = 'beto@virustf.dev';
const user = (email, rol, extra = {}) => ({ email, rol, nombre: email?.split('@')[0] ?? 'Persona', apellido: '', fecha_alta: '2026-11-21', invitado: false, propio: email === a, ...extra });
let lista = [user(a, 'Administrador'), user(b, 'Desarrollador')];
let error = null, alta, lectura;
globalThis.window = Object.assign(new EventTarget(), { __TAURI_INTERNALS__: {
  invoke: async (command, args) => {
    calls.push({ command, args });
    if (command === 'listar_usuarios') return lectura ? lectura : structuredClone(lista);
    if (['crear_usuario', 'cambiar_rol', 'eliminar_usuario'].includes(command)) {
      if (error) throw error;
      if (alta) return alta;
      return command === 'crear_usuario' ? user(args.email, args.rol) : null;
    }
    return [];
  },
} });
const bundled = await build({ configFile: false, logLevel: 'silent', build: { write: false, minify: false, lib: { entry: 'src/store/useStore.ts', formats: ['es'] } } });
const output = (Array.isArray(bundled) ? bundled[0] : bundled).output.find(x => x.type === 'chunk' && x.isEntry).code;
const { useStore } = await import(`data:text/javascript;base64,${Buffer.from(output).toString('base64')}`);
const get = useStore.getState;
useStore.setState({ currentEmail: a, proyectoActual: 'Proyecto', accesoCompletado: true, usuarios: [user(a, 'Administrador')] });
assert.equal(await get().cargarUsuarios(), true);
assert.equal(get().usuariosVisibles.length, 2);

let confirmar;
alta = new Promise(resolve => { confirmar = resolve; });
const creando = get().crearUsuario('  CARO@VIRUSTF.DEV  ', 'Tester');
assert.equal(get().equipoOcupado, true);
assert.equal(get().usuariosVisibles.length, 2, 'no agrega optimistamente');
assert.equal(await get().crearUsuario('caro@virustf.dev', 'Tester'), false, 'bloquea doble envio');
assert.equal(calls.filter(c => c.command === 'crear_usuario').length, 1);
assert.deepEqual(calls.find(c => c.command === 'crear_usuario').args, { email: 'caro@virustf.dev', rol: 'Tester' });
lista.push(user('caro@virustf.dev', 'Tester', { invitado: true }));
confirmar(user('caro@virustf.dev', 'Tester'));
alta = null;
assert.equal(await creando, true);
assert.equal(get().usuariosVisibles.find(u => u.email === 'caro@virustf.dev').invitado, true);
assert.equal(get().equipoOcupado, false);

error = { codigo: 'sin_permiso', mensaje: '<rechazado por el servidor>' };
assert.equal(await get().cambiarRol(b, 'Administrador'), false);
assert.equal(get().usuariosVisibles.find(u => u.email === b).rol, 'Desarrollador');
assert.match(get().errorUsuarios, /rechazado/);
assert.equal(await get().eliminarUsuario(b), false);
assert.ok(get().usuariosVisibles.some(u => u.email === b), 'un fallo no elimina la fila');
error = null;
lista = lista.map(u => u.email === b ? { ...u, rol: 'Administrador' } : u);
assert.equal(await get().cambiarRol(b, 'Administrador'), true);
assert.equal(get().usuariosVisibles.find(u => u.email === b).rol, 'Administrador');
lista = lista.filter(u => u.email !== 'caro@virustf.dev');
assert.equal(await get().eliminarUsuario('caro@virustf.dev'), true);
assert.ok(!get().usuariosVisibles.some(u => u.email === 'caro@virustf.dev'));

let responder;
lectura = new Promise(resolve => { responder = resolve; });
const vieja = get().cargarUsuarios();
const revision = get().revisionEquipo;
get().entrarAProyecto('Proyecto', 'Proyecto', '');
assert.ok(get().revisionEquipo > revision, 'cambiar a un proyecto con el mismo nombre invalida la respuesta');
lectura = null;
lista = [user(a, 'Tester', { email: null, propio: true }), user(null, 'Administrador')];
await get().cargarUsuarios();
responder([user(b, 'Administrador')]);
assert.equal(await vieja, false);
assert.equal(get().currentUser().rol, 'Tester', 'usa la fila propia sin recuperar correos ocultos');
assert.equal(get().usuarios.length, 1);
assert.equal(get().usuarios[0].email, a);
assert.equal(get().usuariosVisibles[1].email, null);

const invocar = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = async command => {
  if (command === 'listar_usuarios') throw { codigo: 'sin_permiso', mensaje: 'Ya no estas en el proyecto' };
  return invocar(command);
};
assert.equal(await get().cargarUsuarios(), false);
assert.deepEqual(get().usuariosVisibles, []);
assert.equal(get().currentUser().rol, 'Colaborador externo');

delete window.__TAURI_INTERNALS__;
useStore.setState({ currentEmail: a, usuarios: [user(a, 'Administrador')], equipoOcupado: false });
assert.equal(await get().cambiarRol(a, 'Tester'), false);
assert.equal(await get().crearUsuario('caro@virustf.dev', 'Tester'), true);
assert.ok(get().usuarios.some(u => u.email === 'caro@virustf.dev'));
console.log('OK: IPC, errores sin cambios optimistas, doble envio, invitaciones, respuestas tardias, privacidad y mockup');
