/**
 * Conta cujo cliente fechou no meio do jogo (commands/closed_clients.rs):
 * o motivo, nas mesmas palavras da Sessão, e o alvo do "Entrar novamente".
 * Só o que o backend mandou: nada aqui decide se a conta fechou.
 */
import type { TOptions } from "i18next";
import type { LaunchTarget } from "../store";
import type { ClosedClient, LaunchQueuePayload, RejoinTarget } from "../types";
import { clientHealthLabel } from "./clientHealth";

type Translate = (text: string, options?: TOptions) => string;

/**
 * O destino no formato do launch da tela (`joinServer`), para "Entrar
 * novamente" entrar pela fila como qualquer launch. `joinVip`/`linkCode` só
 * vão quando o backend os resolveu: um `false`/"" explícito apagaria o código
 * que o `joinServer` tira de um Job ID `vip:<código>` ou de um link colado —
 * o mesmo caminho que o launch original fez.
 */
export function rejoinLaunchTarget(target: RejoinTarget): LaunchTarget {
  const out: LaunchTarget = {
    placeId: String(target.placeId),
    jobId: target.jobId,
    launchData: target.launchData,
  };
  if (target.joinVip || target.linkCode) {
    out.joinVip = target.joinVip;
    out.linkCode = target.linkCode;
  }
  return out;
}

/** "Fechou sem sair do jogo", "Expulso: …", "O servidor fechou"… */
export function closedReasonLabel(closed: ClosedClient, t: Translate): string {
  return (
    clientHealthLabel({ pid: 0, logFound: true, drop: closed.drop }, t)?.label ?? t("Closed without leaving the game")
  );
}

/** A conta espera a vez (ou já está abrindo) na fila de launch. */
export function isPendingInQueue(queue: LaunchQueuePayload | null | undefined, userId: number): boolean {
  return (queue?.entries ?? []).some(
    (entry) => entry.userId === userId && (entry.state === "queued" || entry.state === "launching")
  );
}
