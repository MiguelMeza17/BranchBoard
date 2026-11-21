// Pruebas de `unity.ts` con un `fetch` simulado (no tocan Unity VC).
//   npx deno@2 test supabase/functions/_shared/unity.test.ts

import { assertEquals, assertRejects, assertThrows } from "jsr:@std/assert@1.0.13";
import { correoDelToken, duenoDelRepo, ErrorHttp, leerPedidoUnity, leerRepo } from "./unity.ts";

const SERVIDOR = "https://prd-azure-eastus-01-cloud.plasticscm.com:7178";
const PEDIDO = { token: "TOKENeyJabc", org: "MiguelM17", servidor: SERVIDOR };

/** `fetch` que responde `estado`/`cuerpo` y guarda la URL y la cabecera Authorization pedidas. */
function simulado(estado: number, cuerpo: unknown) {
  const vistos: { url: string; auth: string | null }[] = [];
  const buscar = (async (url: string | URL | Request, init?: RequestInit) => {
    vistos.push({ url: String(url), auth: new Headers(init?.headers).get("Authorization") });
    return new Response(JSON.stringify(cuerpo), { status: estado });
  }) as typeof fetch;
  return { buscar, vistos };
}

Deno.test("acepta un pedido válido y normaliza el servidor", () => {
  const p = leerPedidoUnity({ ...PEDIDO, servidor: SERVIDOR.toUpperCase() + "/" });
  assertEquals(p, PEDIDO);
});

Deno.test("solo acepta servidores https de plasticscm.com", () => {
  for (
    const servidor of [
      "http://prd-azure-eastus-01-cloud.plasticscm.com:7178",
      "https://plasticscm.com.atacante.dev",
      "https://atacante.dev/x.plasticscm.com",
      "https://x.plasticscm.com@atacante.dev",
      "https://atacante.dev",
      "",
    ]
  ) {
    assertThrows(() => leerPedidoUnity({ ...PEDIDO, servidor }), ErrorHttp, "Unity VC Cloud");
  }
});

Deno.test("rechaza organización, token o repo inválidos", () => {
  assertThrows(() => leerPedidoUnity({ ...PEDIDO, org: "a/../b" }), ErrorHttp, "organización");
  assertThrows(() => leerPedidoUnity({ ...PEDIDO, token: "" }), ErrorHttp, "token");
  assertThrows(() => leerPedidoUnity({ ...PEDIDO, token: "a b" }), ErrorHttp, "token");
  assertThrows(() => leerPedidoUnity(null), ErrorHttp);
  assertThrows(() => leerRepo("repo\ncon salto"), ErrorHttp);
  assertEquals(leerRepo(" BranchBoard-Pruebas/BranchBoard-Pruebas "), "BranchBoard-Pruebas/BranchBoard-Pruebas");
});

Deno.test("el correo sale de Unity VC, en minúsculas, con el token como Bearer", async () => {
  const { buscar, vistos } = simulado(200, { name: "Ana@VirusTF.dev" });
  assertEquals(await correoDelToken(PEDIDO, buscar), "ana@virustf.dev");
  assertEquals(vistos, [{ url: `${SERVIDOR}/api/v1/organizations/MiguelM17/user`, auth: "Bearer TOKENeyJabc" }]);
});

Deno.test("un token rechazado por Unity VC da 401", async () => {
  const { buscar } = simulado(401, { error: { message: "An invalid user api token was provided." } });
  const e = await assertRejects(() => correoDelToken(PEDIDO, buscar), ErrorHttp);
  assertEquals(e.estado, 401);
});

Deno.test("si Unity VC no devuelve un correo, no se inicia sesión", async () => {
  const { buscar } = simulado(200, { name: "MiguelM17" });
  const e = await assertRejects(() => correoDelToken(PEDIDO, buscar), ErrorHttp);
  assertEquals(e.estado, 502);
});

Deno.test("sin red da 502", async () => {
  const buscar = (() => Promise.reject(new TypeError("sin red"))) as typeof fetch;
  const e = await assertRejects(() => correoDelToken(PEDIDO, buscar), ErrorHttp);
  assertEquals(e.estado, 502);
});

Deno.test("el dueño del repo sale de `owner` y el nombre del repo va codificado", async () => {
  const { buscar, vistos } = simulado(200, { name: "BB/BB", owner: "Ana@VirusTF.dev" });
  assertEquals(await duenoDelRepo(PEDIDO, "BB/BB", buscar), "ana@virustf.dev");
  assertEquals(vistos[0].url, `${SERVIDOR}/api/v1/organizations/MiguelM17/repos/BB%2FBB`);
});

Deno.test("un repo que no existe da null", async () => {
  const { buscar } = simulado(404, { error: { message: "not found" } });
  assertEquals(await duenoDelRepo(PEDIDO, "nada", buscar), null);
});
