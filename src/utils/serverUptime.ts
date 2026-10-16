/**
 * Há quanto tempo o servidor está no ar, pelo carimbo do `Server Prefix` do
 * log do cliente (commands/client_health.rs). Curto, para o cabeçalho do
 * servidor na página Session: "server up for 3 h".
 */
import type { TOptions } from "i18next";
import type { ClientServer } from "../types";

type Translate = (text: string, options?: TOptions) => string;

/** Relógio do PC atrasado em relação ao servidor: até isto ainda vale "agora". */
const CLOCK_SKEW_MS = 5 * 60_000;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** `null` sem carimbo, ou com um carimbo no futuro (relógio errado). */
export function serverUptimeLabel(startedAtMs: number | null | undefined, nowMs: number, t: Translate): string | null {
  if (typeof startedAtMs !== "number" || !Number.isFinite(startedAtMs)) return null;
  const up = nowMs - startedAtMs;
  if (up < -CLOCK_SKEW_MS) return null;
  if (up < HOUR) return t("server up for {{count}} min", { count: Math.max(1, Math.floor(up / MINUTE)) });
  if (up < 48 * HOUR) return t("server up for {{count}} h", { count: Math.floor(up / HOUR) });
  return t("server up for {{count}} d", { count: Math.floor(up / (24 * HOUR)) });
}

/** Região e tempo no ar, o que houver, na ordem em que aparecem. */
export function serverInfoParts(server: ClientServer | null | undefined, nowMs: number, t: Translate): string[] {
  if (!server) return [];
  const parts: string[] = [];
  const region = server.region?.trim();
  if (region) parts.push(region);
  const uptime = serverUptimeLabel(server.startedAtMs, nowMs, t);
  if (uptime) parts.push(uptime);
  return parts;
}
