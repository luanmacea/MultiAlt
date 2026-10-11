/**
 * O texto da reconexão automática de uma conta (Sessão e painel da conta).
 * Só o que o backend mandou (`commands/reconnect.rs`): nada aqui decide se
 * reconecta.
 */
import type { TOptions } from "i18next";
import type { AutoReconnectEntry } from "../types";

type Translate = (text: string, options?: TOptions) => string;

export interface AutoReconnectLabel {
  label: string;
  /** O erro da última tentativa, para o tooltip. */
  detail: string;
  /** `pending`: ainda vai tentar; `final`: parou (desistiu ou não reconecta). */
  tone: "pending" | "final";
  /** Botões que fazem sentido agora. */
  canTryNow: boolean;
  canTryAgain: boolean;
}

/** "em 30 s" / "em 2 min" até a próxima tentativa (nunca negativo). */
function waitingLabel(ms: number, attempt: number, max: number, t: Translate): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  if (seconds < 60) {
    return t("Reconnecting in {{seconds}} s (attempt {{attempt}}/{{max}})", { seconds, attempt, max });
  }
  return t("Reconnecting in {{minutes}} min (attempt {{attempt}}/{{max}})", {
    minutes: Math.ceil(seconds / 60),
    attempt,
    max,
  });
}

/**
 * O erro da tentativa na língua da tela. As frases que o backend escreve
 * (`RECONNECT_ERROR_*` em commands/reconnect.rs) chegam em inglês; o teste lê
 * a lista do fonte do Rust. Outro texto (erro do launch) passa como veio.
 */
export function reconnectErrorText(error: string, t: Translate): string {
  switch (error) {
    case "The client was closed":
      return t("The client was closed");
    case "It did not get into the game in 2 minutes":
      return t("It did not get into the game in 2 minutes");
    case "The old client did not close":
      return t("The old client did not close");
    case "The Roblox client did not start":
      return t("The Roblox client did not start");
    default:
      return error;
  }
}

export function autoReconnectLabel(entry: AutoReconnectEntry, nowMs: number, t: Translate): AutoReconnectLabel {
  const counts = { attempt: entry.attempt, max: entry.maxAttempts };
  const detail = entry.error ? reconnectErrorText(entry.error, t) : "";
  switch (entry.phase) {
    case "waiting":
      return {
        label: waitingLabel((entry.nextAttemptAtMs ?? nowMs) - nowMs, entry.attempt, entry.maxAttempts, t),
        detail,
        tone: "pending",
        canTryNow: true,
        canTryAgain: false,
      };
    case "waitingForInternet":
      return {
        label: t("Waiting for the internet to come back (attempt {{attempt}}/{{max}})", counts),
        detail,
        tone: "pending",
        canTryNow: true,
        canTryAgain: false,
      };
    case "launching":
      return {
        label: t("Reconnecting now (attempt {{attempt}}/{{max}})", counts),
        detail,
        tone: "pending",
        canTryNow: false,
        canTryAgain: false,
      };
    case "checking":
      return {
        label: t("Reopened, checking it stays in the game (attempt {{attempt}}/{{max}})", counts),
        detail,
        tone: "pending",
        canTryNow: false,
        canTryAgain: false,
      };
    case "gaveUp":
      return {
        label: t("Gave up after {{count}} tries", { count: entry.attempt }),
        detail,
        tone: "final",
        canTryNow: false,
        canTryAgain: true,
      };
    case "stopped":
    default:
      return {
        label: stopLabel(entry, t),
        detail,
        tone: "final",
        canTryNow: false,
        // Banida não tem tentativa; sem destino, não há para onde voltar.
        canTryAgain: entry.reason !== "banned" && entry.reason !== "noDestination",
      };
  }
}

/** Campo da conta lido por `commands/reconnect.rs` (`"true"`/`"false"`; sem ele, o padrão). */
export const AUTO_RECONNECT_FIELD = "AutoReconnect";

/** O que vale para uma conta, e de onde vem. */
export interface ReconnectChoice {
  /** Escolha da própria conta; `null` = segue o padrão. */
  own: boolean | null;
  /** O padrão de todas as contas (`General.AutoReconnect`). */
  globalDefault: boolean;
  /** O AutoRelaunch do Nexus liga por cima de tudo (`reconnect_enabled`). */
  nexusForced: boolean;
  /** Se a conta reconecta, somando os três. */
  effective: boolean;
}

/** Mesma regra de `reconnect_enabled` (commands/reconnect.rs). */
export function reconnectChoice(
  fields: Record<string, string> | undefined,
  globalDefault: boolean,
  nexusForced: boolean
): ReconnectChoice {
  const field = fields?.[AUTO_RECONNECT_FIELD];
  const own = field === "true" ? true : field === "false" ? false : null;
  return { own, globalDefault, nexusForced, effective: nexusForced || (own ?? globalDefault) };
}

/** Os campos da conta com a escolha gravada (`null` apaga: volta ao padrão). */
export function fieldsWithReconnect(
  fields: Record<string, string> | undefined,
  value: boolean | null
): Record<string, string> {
  const next = { ...(fields || {}) };
  if (value === null) delete next[AUTO_RECONNECT_FIELD];
  else next[AUTO_RECONNECT_FIELD] = value ? "true" : "false";
  return next;
}

function stopLabel(entry: AutoReconnectEntry, t: Translate): string {
  switch (entry.reason) {
    case "joinedElsewhere":
      return t("Not reconnecting: the account joined somewhere else");
    case "closedByUser":
      return t("Not reconnecting: the client was closed");
    case "banned":
      return t("Not reconnecting: the account is banned");
    case "sessionExpired":
      return t("Not reconnecting: sign in to this account again");
    case "noDestination":
      return t("Not reconnecting: no game to go back to");
    case "openedOutsideApp":
      return t("Not reconnecting: the account is open outside the app");
    default:
      return t("Not reconnecting");
  }
}
