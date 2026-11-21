// Clientes de Supabase para las Edge Functions (#237).
//
// La clave secreta la pone Supabase en el entorno de la función; nunca está en el repo. Proyectos con las
// claves nuevas traen `SUPABASE_SECRET_KEYS` / `SUPABASE_PUBLISHABLE_KEYS` (JSON); los demás, las de antes.

import { createClient, type SupabaseClient } from "npm:@supabase/supabase-js@2.49.4";
import { ErrorHttp } from "./unity.ts";

function clave(json: string, anterior: string): string {
  try {
    const valor = JSON.parse(Deno.env.get(json) ?? "{}")?.default;
    if (typeof valor === "string" && valor) return valor;
  } catch {
    // sigue con la variable de antes
  }
  const valor = Deno.env.get(anterior);
  if (!valor) throw new ErrorHttp(500, "La función no tiene configuradas las claves de Supabase");
  return valor;
}

function cliente(k: string): SupabaseClient {
  return createClient(Deno.env.get("SUPABASE_URL")!, k, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
}

/** Con la clave secreta (`service_role`): Auth admin y las funciones `cuenta_por_correo` / `vincular_repo_unity`. */
export function clienteAdmin(): SupabaseClient {
  return cliente(clave("SUPABASE_SECRET_KEYS", "SUPABASE_SERVICE_ROLE_KEY"));
}

/** Con la clave pública: solo para canjear el enlace de un solo uso por una sesión. */
export function clientePublico(): SupabaseClient {
  return cliente(clave("SUPABASE_PUBLISHABLE_KEYS", "SUPABASE_ANON_KEY"));
}

/** Respuesta JSON. */
export function json(estado: number, cuerpo: unknown): Response {
  return new Response(JSON.stringify(cuerpo), {
    status: estado,
    headers: { "Content-Type": "application/json; charset=utf-8" },
  });
}

/** Convierte cualquier error en `{ error }` sin filtrar detalles internos. */
export function respuestaDeError(e: unknown): Response {
  if (e instanceof ErrorHttp) return json(e.estado, { error: e.message });
  console.error("error inesperado", e instanceof Error ? e.message : "desconocido");
  return json(500, { error: "Error inesperado en el servidor" });
}
