/**
 * Aviso no Discord por webhook (ideia 5): a conferência da URL do lado da
 * tela, espelho de `parse_webhook_url` (src-tauri/src/api/discord_webhook.rs).
 * O backend confere de novo ao gravar (`update_setting`) e antes de cada envio
 * — e só ele faz rede. Ver docs/features/webhook.md.
 */

const DISCORD_HOSTS = new Set([
  "discord.com",
  "discordapp.com",
  "ptb.discord.com",
  "canary.discord.com",
  "ptb.discordapp.com",
  "canary.discordapp.com",
]);

/** A mesma frase do backend (`INVALID_URL_MESSAGE`). */
export const INVALID_WEBHOOK_URL_MESSAGE =
  "That is not a Discord webhook URL. Copy it from Discord: Server Settings › Integrations › Webhooks › Copy Webhook URL.";

/** Teto do texto antes de cada mensagem (`MAX_MENTION_CHARS` do backend). */
export const MAX_WEBHOOK_MENTION_CHARS = 100;

/** `https://discord.com/api/webhooks/<id>/<token>` (e as variantes do Discord)? */
export function isDiscordWebhookUrl(text: string): boolean {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    return false;
  }
  if (url.protocol !== "https:" || url.username || url.password || url.port || url.search || url.hash) return false;
  if (!DISCORD_HOSTS.has(url.hostname.toLowerCase())) return false;
  const segments = url.pathname.split("/").slice(1);
  if (segments[segments.length - 1] === "") segments.pop();
  let rest: string[];
  if (segments[0] === "api" && segments[1] === "webhooks") rest = segments.slice(2);
  else if (segments[0] === "api" && /^v\d+$/.test(segments[1] ?? "") && segments[2] === "webhooks") rest = segments.slice(3);
  else return false;
  if (rest.length !== 2) return false;
  const [id, token] = rest;
  return /^\d{15,25}$/.test(id) && /^[A-Za-z0-9_-]{20,200}$/.test(token);
}

/** O problema do campo, ou `null`: vazio vale (desliga o envio). */
export function webhookUrlProblem(text: string): string | null {
  if (!text.trim()) return null;
  return isDiscordWebhookUrl(text) ? null : INVALID_WEBHOOK_URL_MESSAGE;
}
