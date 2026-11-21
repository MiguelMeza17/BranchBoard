// Preguntarle a Unity VC de quién es un token y quién es el dueño de un repo (#237).
//
// La Edge Function no le cree a la app: con el token que manda, le pregunta al servidor de Unity VC. Solo
// se aceptan servidores `https://*.plasticscm.com` (los de Unity VC Cloud), así una app modificada no
// puede apuntar a un servidor propio que conteste lo que quiera.
//
// Endpoints usados (API REST de Unity VC, probados el 2026-11-21 con el token de sesión de `cm` y con PAT):
//   GET {servidor}/api/v1/organizations/{org}/user            → 200 {"name": "<correo>"}, 401 si no vale
//   GET {servidor}/api/v1/organizations/{org}/repos/{repo}    → 200 {"name", "owner", …}, 404 si no existe

export const TIEMPO_MAXIMO_MS = 15_000;

/** Servidores de Unity VC Cloud, p. ej. `https://prd-azure-eastus-01-cloud.plasticscm.com:7178`. */
const SERVIDOR = /^https:\/\/[a-z0-9-]+\.plasticscm\.com(:\d{1,5})?$/;
/** Nombre (slug) de organización, p. ej. `MiguelM17`. Mismo criterio que `sso_navegador.rs`. */
const ORGANIZACION = /^[A-Za-z0-9._-]{1,100}$/;

/** Error con el código HTTP que devuelve la función y un mensaje en español para la app. */
export class ErrorHttp extends Error {
  constructor(readonly estado: number, mensaje: string) {
    super(mensaje);
  }
}

export interface PedidoUnity {
  /** Token de Unity VC: el de la sesión de `cm` (`TOKEN…`) o un PAT. Nunca se guarda ni se registra. */
  token: string;
  org: string;
  servidor: string;
}

/** Valida `{ token, org, servidor }` del cuerpo del pedido. */
export function leerPedidoUnity(cuerpo: unknown): PedidoUnity {
  const c = (cuerpo ?? {}) as Record<string, unknown>;
  const token = typeof c.token === "string" ? c.token.trim() : "";
  const org = typeof c.org === "string" ? c.org.trim() : "";
  const servidor = typeof c.servidor === "string" ? c.servidor.trim().replace(/\/+$/, "").toLowerCase() : "";
  if (!token || token.length > 4096 || /\s/.test(token)) {
    throw new ErrorHttp(400, "Falta el token de Unity VC");
  }
  if (!ORGANIZACION.test(org)) {
    throw new ErrorHttp(400, "La organización de Unity VC no es válida");
  }
  if (!SERVIDOR.test(servidor)) {
    throw new ErrorHttp(400, "El servidor no es de Unity VC Cloud");
  }
  return { token, org, servidor };
}

/** Valida el nombre del repo (`BranchBoard-Pruebas/BranchBoard-Pruebas`): sin saltos de línea ni vacío. */
export function leerRepo(valor: unknown): string {
  const repo = typeof valor === "string" ? valor.trim() : "";
  if (!repo || repo.length > 200 || /[\u0000-\u001f]/.test(repo)) {
    throw new ErrorHttp(400, "Falta el repositorio de Unity VC");
  }
  return repo;
}

function urlApi(p: PedidoUnity, ruta: string): string {
  return `${p.servidor}/api/v1/organizations/${encodeURIComponent(p.org)}${ruta}`;
}

async function pedir(p: PedidoUnity, ruta: string, buscar: typeof fetch): Promise<Response> {
  try {
    return await buscar(urlApi(p, ruta), {
      headers: { Authorization: `Bearer ${p.token}`, Accept: "application/json" },
      signal: AbortSignal.timeout(TIEMPO_MAXIMO_MS),
    });
  } catch {
    throw new ErrorHttp(502, "No se pudo hablar con Unity VC. Vuelve a intentarlo.");
  }
}

function tokenRechazado(estado: number): boolean {
  return estado === 401 || estado === 403;
}

/** Correo (en minúsculas) de la cuenta de Unity VC dueña del token. */
export async function correoDelToken(p: PedidoUnity, buscar: typeof fetch = fetch): Promise<string> {
  const r = await pedir(p, "/user", buscar);
  if (tokenRechazado(r.status)) {
    throw new ErrorHttp(401, "La sesión de Unity VC no es válida o caducó. Vuelve a iniciar sesión con Unity.");
  }
  if (!r.ok) {
    throw new ErrorHttp(502, `Unity VC respondió ${r.status} al revisar la sesión`);
  }
  const datos = await r.json().catch(() => null) as { name?: unknown } | null;
  const correo = typeof datos?.name === "string" ? datos.name.trim().toLowerCase() : "";
  if (!/^[^@\s]+@[^@\s]+$/.test(correo)) {
    throw new ErrorHttp(502, "Unity VC no devolvió el correo de la cuenta");
  }
  return correo;
}

/** Correo (en minúsculas) del dueño del repo, o `null` si el repo no existe o no se puede ver. */
export async function duenoDelRepo(
  p: PedidoUnity,
  repo: string,
  buscar: typeof fetch = fetch,
): Promise<string | null> {
  const r = await pedir(p, `/repos/${encodeURIComponent(repo)}`, buscar);
  if (r.status === 404) return null;
  if (tokenRechazado(r.status)) {
    throw new ErrorHttp(401, "La sesión de Unity VC no es válida o caducó. Vuelve a iniciar sesión con Unity.");
  }
  if (!r.ok) {
    throw new ErrorHttp(502, `Unity VC respondió ${r.status} al buscar el repositorio`);
  }
  const datos = await r.json().catch(() => null) as { owner?: unknown } | null;
  return typeof datos?.owner === "string" ? datos.owner.trim().toLowerCase() : null;
}
