// Cuenta de BranchBoard para la pantalla de inicio ("Bienvenido" / "Regístrate").
// Las cuentas viven en Supabase Auth (#236): las crea y las comprueba el backend (`api.registrarCuenta`,
// `api.iniciarSesionBranchBoard`). Aquí solo quedan los correos recordados de este equipo.

export interface Cuenta {
  nombre: string;
  apellido: string;
  correo: string;
}

const normalizar = (correo: string) => correo.trim().toLowerCase();

// Las cuentas que guardaba este equipo antes de #236 (correo y hash de la contraseña) ya no sirven: se borran.
try {
  localStorage.removeItem('branchboard.cuentas');
} catch {
  // sin almacenamiento: no hay nada que borrar
}

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
