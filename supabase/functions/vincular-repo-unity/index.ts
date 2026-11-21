// Primer Administrador de un repo de Unity VC (#237, decisión 5 de #231).
//
// POST { token, org, servidor, repo, nombre? }   con   Authorization: Bearer <sesión de Supabase>
// 200 { proyecto_id }
// 400 pedido inválido · 401 sin sesión o token de Unity no válido · 403 no es el dueño o el token es de
// otra cuenta · 404 repo no encontrado · 409 el repo ya está en BranchBoard y la persona no es miembro
//
// Solo el dueño del repo (campo `owner` de la API REST de Unity VC) lo trae a BranchBoard y queda como
// Administrador. El servidor lo comprueba con el token de Unity de la persona; no le cree a la app.
// Listar los proyectos y unirse a uno existente es de #240/#241.

import { clienteAdmin, json, respuestaDeError } from "../_shared/supabase.ts";
import { correoDelToken, duenoDelRepo, ErrorHttp, leerPedidoUnity, leerRepo } from "../_shared/unity.ts";

async function usuarioDeLaSesion(req: Request): Promise<{ id: string; correo: string }> {
  const jwt = req.headers.get("Authorization")?.match(/^Bearer\s+(\S+)$/i)?.[1];
  if (!jwt) throw new ErrorHttp(401, "Inicia sesión en BranchBoard");
  const { data, error } = await clienteAdmin().auth.getUser(jwt);
  if (error || !data.user?.email) throw new ErrorHttp(401, "La sesión de BranchBoard caducó");
  return { id: data.user.id, correo: data.user.email.toLowerCase() };
}

Deno.serve(async (req) => {
  try {
    if (req.method !== "POST") return json(405, { error: "Solo POST" });
    const yo = await usuarioDeLaSesion(req);
    const cuerpo = await req.json().catch(() => null);
    const pedido = leerPedidoUnity(cuerpo);
    const repo = leerRepo(cuerpo?.repo);
    const nombre = typeof cuerpo?.nombre === "string" ? cuerpo.nombre.trim().slice(0, 200) : "";

    const correoUnity = await correoDelToken(pedido);
    if (correoUnity !== yo.correo) {
      throw new ErrorHttp(403, "La sesión de Unity VC es de otra cuenta que la de BranchBoard");
    }
    const dueno = await duenoDelRepo(pedido, repo);
    if (dueno === null) throw new ErrorHttp(404, "No se encontró ese repositorio en Unity VC");
    if (dueno !== yo.correo) {
      throw new ErrorHttp(403, "Solo el dueño del repositorio en Unity VC puede traerlo a BranchBoard");
    }

    const { data, error } = await clienteAdmin().rpc("vincular_repo_unity", {
      p_usuario: yo.id,
      p_org: pedido.org,
      p_repo: repo,
      p_nombre: nombre,
    });
    if (error) {
      if (error.code === "P0001") throw new ErrorHttp(409, error.message);
      throw new ErrorHttp(500, "No se pudo crear el proyecto");
    }
    return json(200, { proyecto_id: data });
  } catch (e) {
    return respuestaDeError(e);
  }
});
