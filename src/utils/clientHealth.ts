/**
 * O texto curto da saúde de um cliente (queda com motivo), igual na Sessão e
 * no painel da conta. Só o que o backend mandou: nada aqui decide se caiu.
 */
import type { TOptions } from "i18next";
import type { ClientHealth } from "../types";

type Translate = (text: string, options?: TOptions) => string;

export interface ClientHealthLabel {
  /** O que aparece ao lado do nome. */
  label: string;
  /** Detalhe para o tooltip (código do Roblox), quando houver. */
  detail: string;
  /** `drop`: caiu (vermelho); `hung`: janela travada (âmbar). */
  tone: "drop" | "hung";
}

/** `null` quando não há nada a avisar (cliente em jogo, ou sem informação). */
export function clientHealthLabel(health: ClientHealth | null | undefined, t: Translate): ClientHealthLabel | null {
  const label = dropLabel(health, t);
  if (label) return { ...label, tone: "drop" };
  // Queda vence: uma conta caída com a janela travada mostra o motivo.
  if (health?.notResponding) {
    return {
      label: t("Not responding"),
      detail: t("The Roblox window has not responded for 30 seconds"),
      tone: "hung",
    };
  }
  return null;
}

function dropLabel(
  health: ClientHealth | null | undefined,
  t: Translate
): Omit<ClientHealthLabel, "tone"> | null {
  const drop = health?.drop;
  if (!drop) return null;
  const detail = drop.code !== null ? t("Roblox error code {{code}}", { code: drop.code }) : "";
  switch (drop.kind) {
    case "disconnected":
      switch (drop.reason) {
        case "connectionLost":
          return { label: t("Disconnected: lost connection"), detail };
        case "joinedElsewhere":
          return { label: t("Disconnected: the account joined somewhere else"), detail };
        case "idle":
          return { label: t("Disconnected: idle for too long"), detail };
        default:
          return { label: t("Disconnected from the game"), detail };
      }
    case "kicked":
      return drop.message
        ? { label: t("Kicked: {{message}}", { message: drop.message }), detail }
        : { label: t("Kicked from the game"), detail };
    case "serverShutdown":
      return { label: t("The server shut down"), detail };
    case "crashed":
      return { label: t("Closed without leaving the game"), detail };
    default:
      return null;
  }
}

/**
 * O cliente que o app abriu entrou como outra conta do Roblox
 * (`commands/client_health.rs`, `wrong_account`). `null` quando não.
 */
export function wrongAccountLabel(health: ClientHealth | null | undefined, t: Translate): string | null {
  const id = health?.wrongAccountUserId;
  if (typeof id !== "number") return null;
  return t("Opened as a different Roblox account (user {{id}})", { id });
}

/**
 * O toast do evento `roblox-client-wrong-account`. A fila de launch só é
 * citada quando o backend de fato a parou (havia conta esperando a vez).
 */
export function wrongAccountToast(name: string, loggedUserId: number, queueStopped: boolean, t: Translate): string {
  return queueStopped
    ? t("{{name}} opened as a different Roblox account (user {{id}}). The launch queue was paused.", {
        name,
        id: loggedUserId,
      })
    : t("{{name}} opened as a different Roblox account (user {{id}}).", { name, id: loggedUserId });
}
