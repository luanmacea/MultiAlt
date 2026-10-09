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
}

/** `null` quando não há nada a avisar (cliente em jogo, ou sem informação). */
export function clientHealthLabel(health: ClientHealth | null | undefined, t: Translate): ClientHealthLabel | null {
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
