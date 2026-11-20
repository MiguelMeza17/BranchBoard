// Cuentas de BranchBoard para la pantalla de inicio ("Bienvenido" / "Regístrate").
// Por ahora viven en este equipo (localStorage del webview): no hay backend de cuentas todavía.
// La contraseña nunca se guarda tal cual: se guarda su hash SHA-256 con una sal aleatoria.

export interface Cuenta {
  nombre: string;
  apellido: string;
  correo: string;
}

interface CuentaGuardada extends Cuenta {
  sal: string;
  hash: string;
}

const CLAVE = 'branchboard.cuentas';

function leer(): CuentaGuardada[] {
  try {
    return JSON.parse(localStorage.getItem(CLAVE) ?? '[]') as CuentaGuardada[];
  } catch {
    return [];
  }
}

function guardar(cuentas: CuentaGuardada[]) {
  localStorage.setItem(CLAVE, JSON.stringify(cuentas));
}

const aHex = (bytes: ArrayBuffer | Uint8Array) =>
  Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, '0')).join('');

async function hashClave(clave: string, sal: string) {
  const datos = new TextEncoder().encode(`${sal}:${clave}`);
  return aHex(await crypto.subtle.digest('SHA-256', datos));
}

const normalizar = (correo: string) => correo.trim().toLowerCase();

// Correos con los que ya se entró en este equipo, el más reciente primero: se sugieren en "Bienvenido".
const CLAVE_RECIENTES = 'branchboard.correosRecientes';
const MAX_RECIENTES = 8;

export function correosRecientes(): string[] {
  try {
    const lista = JSON.parse(localStorage.getItem(CLAVE_RECIENTES) ?? '[]');
    return Array.isArray(lista) ? lista.filter((c): c is string => typeof c === 'string') : [];
  } catch {
    return [];
  }
}

function guardarRecientes(lista: string[]) {
  try {
    localStorage.setItem(CLAVE_RECIENTES, JSON.stringify(lista));
  } catch {
    // sin almacenamiento: solo no se recuerdan
  }
}

export function recordarCorreo(correo: string) {
  const c = normalizar(correo);
  guardarRecientes([c, ...correosRecientes().filter((x) => x !== c)].slice(0, MAX_RECIENTES));
}

export function olvidarCorreo(correo: string) {
  const c = normalizar(correo);
  guardarRecientes(correosRecientes().filter((x) => x !== c));
}

/** Crea la cuenta. Falla si ya hay una con ese correo. */
export async function registrarCuenta(datos: Cuenta & { clave: string }): Promise<Cuenta> {
  const cuentas = leer();
  const correo = normalizar(datos.correo);
  if (cuentas.some((c) => c.correo === correo)) {
    throw new Error('Ya existe una cuenta con ese correo. Inicia sesión.');
  }
  const sal = aHex(crypto.getRandomValues(new Uint8Array(16)));
  const cuenta: Cuenta = { nombre: datos.nombre.trim(), apellido: datos.apellido.trim(), correo };
  guardar([...cuentas, { ...cuenta, sal, hash: await hashClave(datos.clave, sal) }]);
  return cuenta;
}

/** Devuelve la cuenta si el correo y la contraseña coinciden. */
export async function ingresar(correo: string, clave: string): Promise<Cuenta> {
  const c = leer().find((x) => x.correo === normalizar(correo));
  if (!c || (await hashClave(clave, c.sal)) !== c.hash) {
    throw new Error('Correo o contraseña incorrectos.');
  }
  recordarCorreo(c.correo);
  return { nombre: c.nombre, apellido: c.apellido, correo: c.correo };
}
