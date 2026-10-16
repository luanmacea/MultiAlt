/**
 * Política pura do "Wait until they're in a game" da aba Seguir (Choose Game):
 * quando o jogador não está num estado em que dê para entrar, a tela continua
 * perguntando a presença dele e lança as contas assim que der.
 *
 * Nada aqui toca rede, relógio ou store — só decide. O laço que usa isto mora
 * em `components/follow/waitForPlayerWatch.ts`.
 */

/** Intervalo entre consultas enquanto o jogador não está em jogo. */
export const WAIT_POLL_INTERVAL_MS = 30_000;
/** Teto da espera depois de erro (inclusive 429 sem `Retry-After`). */
export const WAIT_ERROR_BACKOFF_MAX_MS = 5 * 60_000;
/** Depois disto a espera desiste sozinha. */
export const WAIT_MAX_DURATION_MS = 2 * 60 * 60_000;

/** Presença como o comando `get_presence` devolve (as duas grafias). */
export type WaitPresence = {
  userPresenceType?: number;
  user_presence_type?: number;
  placeId?: number | null;
  place_id?: number | null;
  rootPlaceId?: number | null;
  root_place_id?: number | null;
  gameId?: string | null;
  game_id?: string | null;
};

export type WaitDecision =
  | { joinable: false; reason: "not-in-game" | "server-hidden" }
  | { joinable: true; placeId: number; jobId: string; follow: boolean };

/**
 * Dá para entrar agora?
 *
 * - **Seguindo** (`General.JoinFriendsByFollowing` ligado): basta o jogador
 *   estar **em jogo** (`userPresenceType == 2`); o Roblox acha o servidor.
 *   Online no site (1) ou no Studio (3) não serve — seguir daria em nada.
 * - **Servidor exato** (toggle desligado): em jogo **e** com Job ID e place
 *   visíveis. O place é o do servidor (pode ser sub-place), como no Follow.
 */
export function evaluateWaitPresence(entry: WaitPresence | null | undefined, byFollowing: boolean): WaitDecision {
  const type = entry?.userPresenceType ?? entry?.user_presence_type ?? 0;
  if (type !== 2) return { joinable: false, reason: "not-in-game" };
  const serverPlace = entry?.placeId ?? entry?.place_id ?? null;
  const rootPlace = entry?.rootPlaceId ?? entry?.root_place_id ?? null;
  if (byFollowing) {
    return { joinable: true, placeId: rootPlace ?? serverPlace ?? 0, jobId: "", follow: true };
  }
  const jobId = (entry?.gameId ?? entry?.game_id ?? "").trim();
  const placeId = serverPlace ?? rootPlace;
  if (!jobId || !placeId) return { joinable: false, reason: "server-hidden" };
  return { joinable: true, placeId, jobId, follow: false };
}

/** `Retry-After` que o backend põe no erro do 429 (`retry after 90 s`), em ms. */
export function parseRetryAfterMs(error: string): number | null {
  const match = /retry after (\d+)\s*s\b/i.exec(error);
  if (!match) return null;
  const secs = Number(match[1]);
  return Number.isFinite(secs) && secs >= 0 ? secs * 1000 : null;
}

/** O backend escreve `status 429`; o texto do Roblox, "Too many requests". */
export function isRateLimitError(error: string): boolean {
  return /\b429\b/.test(error) || /too many requests/i.test(error);
}

/**
 * Quanto esperar até a próxima consulta.
 *
 * - Sem erro: `WAIT_POLL_INTERVAL_MS`.
 * - Erro: dobra a partir do intervalo (60 s, 120 s, 240 s…) até
 *   `WAIT_ERROR_BACKOFF_MAX_MS`.
 * - 429 sem `Retry-After`: vai direto ao teto.
 * - `Retry-After` vence a conta quando pede mais (é o servidor dizendo quando).
 */
export function nextCheckDelayMs(consecutiveErrors: number, lastError: string | null = null): number {
  if (consecutiveErrors <= 0 || !lastError) return WAIT_POLL_INTERVAL_MS;
  const retryAfter = parseRetryAfterMs(lastError);
  let backoff = Math.min(WAIT_POLL_INTERVAL_MS * 2 ** consecutiveErrors, WAIT_ERROR_BACKOFF_MAX_MS);
  if (retryAfter == null && isRateLimitError(lastError)) backoff = WAIT_ERROR_BACKOFF_MAX_MS;
  return retryAfter != null ? Math.max(backoff, retryAfter) : backoff;
}

export type WaitPlan = { action: "give-up" } | { action: "wait"; delayMs: number };

/**
 * Próximo passo da espera: desistir (passou do tempo máximo) ou esperar
 * `delayMs`. A última consulta cai no prazo, não depois dele.
 */
export function planNextCheck(opts: {
  startedAt: number;
  now: number;
  consecutiveErrors: number;
  lastError?: string | null;
  maxDurationMs?: number;
}): WaitPlan {
  const max = opts.maxDurationMs ?? WAIT_MAX_DURATION_MS;
  const remaining = opts.startedAt + max - opts.now;
  if (remaining <= 0) return { action: "give-up" };
  const delay = nextCheckDelayMs(opts.consecutiveErrors, opts.lastError ?? null);
  return { action: "wait", delayMs: Math.min(delay, remaining) };
}
