// "Iniciar sesión con Unity VC" → sesión de Supabase (#237).
//
// POST { token, org, servidor }
//   token    — token de Unity VC que dejó el login por el navegador (`sso_navegador.rs`) o un PAT
//   org      — organización de Unity VC (p. ej. `MiguelM17`)
//   servidor — `https://…plasticscm.com:7178` de esa organización (`servidor/region.rs`)
// 200 { access_token, refresh_token, expires_in, expires_at, usuario: { id, correo }, cuenta_nueva }
// 400 pedido inválido · 401 token de Unity no válido · 502 Unity VC no responde
//
// 1. Le pregunta a Unity VC de quién es el token (no le cree a la app).
// 2. Si ese correo ya tiene cuenta (de correo y contraseña o de Unity), es la misma (decisión 1 de #231).
//    Si no, se crea con el correo ya confirmado: Unity ya lo verificó.
//    Si la cuenta existía SIN confirmar, se confirma y se le cambia la contraseña por una aleatoria: quien
//    la registró pudo no ser el dueño del correo y no debe poder entrar con ella.
// 3. Entrega una sesión de Supabase de esa cuenta (enlace mágico generado y canjeado aquí mismo; no se
//    manda ningún correo).

import { clienteAdmin, clientePublico, json, respuestaDeError } from "../_shared/supabase.ts";
import { correoDelToken, ErrorHttp, leerPedidoUnity } from "../_shared/unity.ts";

function contrasenaAleatoria(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return btoa(String.fromCharCode(...bytes));
}

async function cuentaDe(correo: string): Promise<{ id: string; confirmada: boolean } | null> {
  const { data, error } = await clienteAdmin().rpc("cuenta_por_correo", { p_correo: correo });
  if (error) throw new ErrorHttp(500, "No se pudo revisar la cuenta");
  return (data as { id: string; confirmada: boolean }[] | null)?.[0] ?? null;
}

/** Deja una cuenta confirmada para `correo` y dice si se creó ahora. */
async function asegurarCuenta(correo: string): Promise<boolean> {
  const admin = clienteAdmin().auth.admin;
  const cuenta = await cuentaDe(correo);
  if (!cuenta) {
    const { error } = await admin.createUser({
      email: correo,
      email_confirm: true,
      user_metadata: { nombre: "", apellido: "" },
    });
    if (!error) return true;
    // Otra petición la creó al mismo tiempo: se sigue con esa.
    if (error.code !== "email_exists") throw new ErrorHttp(500, "No se pudo crear la cuenta");
    return asegurarCuenta(correo);
  }
  if (!cuenta.confirmada) {
    const { error } = await admin.updateUserById(cuenta.id, {
      email_confirm: true,
      password: contrasenaAleatoria(),
    });
    if (error) throw new ErrorHttp(500, "No se pudo confirmar la cuenta");
  }
  return false;
}

async function sesionPara(correo: string) {
  const { data: enlace, error: e1 } = await clienteAdmin().auth.admin.generateLink({
    type: "magiclink",
    email: correo,
  });
  if (e1 || !enlace?.properties?.hashed_token) throw new ErrorHttp(500, "No se pudo abrir la sesión");
  const { data, error: e2 } = await clientePublico().auth.verifyOtp({
    type: "email",
    token_hash: enlace.properties.hashed_token,
  });
  if (e2 || !data.session || !data.user) throw new ErrorHttp(500, "No se pudo abrir la sesión");
  return data;
}

Deno.serve(async (req) => {
  try {
    if (req.method !== "POST") return json(405, { error: "Solo POST" });
    const pedido = leerPedidoUnity(await req.json().catch(() => null));
    const correo = await correoDelToken(pedido);
    const cuentaNueva = await asegurarCuenta(correo);
    const { session, user } = await sesionPara(correo);
    return json(200, {
      access_token: session!.access_token,
      refresh_token: session!.refresh_token,
      expires_in: session!.expires_in,
      expires_at: session!.expires_at,
      usuario: { id: user!.id, correo },
      cuenta_nueva: cuentaNueva,
    });
  } catch (e) {
    return respuestaDeError(e);
  }
});
