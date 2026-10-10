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

export function autoReconnectLabel(entry: AutoReconnectEntry, nowMs: number, t: Translate): AutoReconnectLabel {
  const counts = { attempt: entry.attempt, max: entry.maxAttempts };
  const detail = entry.error ?? "";
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
