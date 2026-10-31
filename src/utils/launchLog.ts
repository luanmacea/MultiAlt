import type { TOptions } from "i18next";
import type { LaunchLogEntry } from "../store";

type Translate = (text: string, options?: TOptions) => string;

/** Uma frase aninhada: o backend manda `{ key, params }` onde também cabe uma frase. */
function isNestedMessage(value: unknown): value is { key: string; params?: Record<string, unknown> } {
  return typeof value === "object" && value !== null && !Array.isArray(value) && typeof (value as { key?: unknown }).key === "string";
}

/**
 * O valor de um `{{parametro}}`. Número e texto entram como vieram (o texto de
 * fora — erro do Roblox, do Windows, nome de preset — nunca é traduzido);
 * frase aninhada é traduzida antes de entrar na frase de fora; lista vira
 * "a, b".
 */
function resolveParam(value: unknown, t: Translate): unknown {
  if (Array.isArray(value)) return value.map((item) => String(resolveParam(item, t) ?? "")).join(", ");
  if (isNestedMessage(value)) return t(value.key, resolveParams(value.params ?? {}, t));
  return value;
}

function resolveParams(params: Record<string, unknown>, t: Translate): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [name, value] of Object.entries(params)) out[name] = resolveParam(value, t);
  return out;
}

/**
 * O texto de uma linha do Console no idioma de agora. O backend manda a frase
 * em inglês com `{{parametros}}` (`key`), que é também a chave do catálogo;
 * frase que ainda não tem tradução cai no inglês da própria chave. Linha sem
 * `key` (texto de fora, ou ouvinte antigo) mostra o `message` como veio.
 */
export function launchLogText(entry: Pick<LaunchLogEntry, "key" | "params" | "message">, t: Translate): string {
  if (!entry.key) return entry.message;
  return t(entry.key, resolveParams(entry.params ?? {}, t));
}
