import type { AutoReconnectEntry, ClientHealth, ClosedClient, CurrentSession, LaunchQueuePayload } from "../types";
import { needsAttention, runningStatus } from "./sessionGroups";

/**
 * O estado ao vivo de UMA conta, para a lista de contas e o chip da barra de
 * cima (crítica de design P0, 11/10/2026: com 7 clientes abertos a lista só
 * mostrava a bolinha laranja). Sai das mesmas fontes da página Session, com a
 * mesma prioridade (`runningStatus` em utils/sessionGroups.ts) — nada aqui
 * decide se caiu ou reconecta: isso é do backend.
 *
 * - `playing`: cliente aberto e sessão de agora conhecida (jogo + início).
 * - `joining`: a fila de launch está abrindo esta conta agora (ou o join de
 *   uma conta só, `joiningAccounts`).
 * - `reconnecting`, `notResponding`, `dropped`: os três que pedem atenção.
 * - `waiting`: cliente aberto, o log diz que ainda não entrou num jogo.
 * - `closed`: o cliente que o app abriu fechou no meio do jogo e ninguém o
 *   está reabrindo (commands/closed_clients.rs) — a linha oferece "Entrar
 *   novamente". Reconectando e entrando de novo vêm antes dele.
 * - `none`: nada a dizer — conta parada, ou cliente aberto sem informação
 *   (fora do Windows o log não é lido; a bolinha "launched" já diz que abriu).
 */
export type AccountLiveKind =
  | "playing"
  | "joining"
  | "reconnecting"
  | "dropped"
  | "notResponding"
  | "closed"
  | "waiting"
  | "none";

export interface AccountLiveState {
  state: AccountLiveKind;
  /** O jogo, quando se sabe (sessão de agora ou alvo da fila). */
  placeId: number | null;
  /** Início da sessão de agora (em jogo) ou a hora em que o cliente fechou. */
  sinceMs: number | null;
}

export interface LiveSources {
  /** `store.launchedByProgram`: contas com cliente aberto. */
  running: ReadonlySet<number>;
  /** `store.joiningAccounts`. */
  joining: ReadonlySet<number>;
  launchQueue: LaunchQueuePayload | null;
  clientHealth: ReadonlyMap<number, ClientHealth> | undefined;
  /** `activeReconnectIds(store.autoReconnect)`. */
  reconnecting: ReadonlySet<number>;
  /** `useCurrentSessions`. */
  sessions: ReadonlyMap<number, CurrentSession>;
  /** `store.closedClients`: clientes que fecharam no meio do jogo. */
  closed: ReadonlyMap<number, ClosedClient>;
}

const NONE: AccountLiveState = { state: "none", placeId: null, sinceMs: null };

/**
 * Contas com a reconexão automática ainda em curso. Quem desistiu ou parou
 * (`gaveUp`/`stopped`) não está reconectando — mesma regra da página Session.
 */
export function activeReconnectIds(entries: readonly AutoReconnectEntry[] | undefined): Set<number> {
  return new Set(
    (entries ?? []).filter((entry) => entry.phase !== "gaveUp" && entry.phase !== "stopped").map((entry) => entry.userId)
  );
}

export function accountLiveState(userId: number, sources: LiveSources): AccountLiveState {
  const reconnecting = sources.reconnecting.has(userId);
  const queueEntry = sources.launchQueue?.entries.find(
    (entry) => entry.userId === userId && entry.state === "launching"
  );
  const joining = !!queueEntry || sources.joining.has(userId);
  const joiningState: AccountLiveState = {
    state: "joining",
    placeId: queueEntry ? queueEntry.placeId || sources.launchQueue?.placeId || null : null,
    sinceMs: null,
  };

  // Fechou no meio do jogo: o backend só anota quem não tem mais cliente
  // vivo, então isto vale mesmo com o processo morto ainda no tracker.
  const closed = sources.closed.get(userId);
  if (closed && !reconnecting && !joining) {
    return { state: "closed", placeId: closed.placeId, sinceMs: closed.closedAtMs };
  }

  if (!sources.running.has(userId)) {
    // Cliente fechou e a reconexão espera a próxima tentativa.
    if (reconnecting) return { state: "reconnecting", placeId: null, sinceMs: null };
    return joining ? joiningState : NONE;
  }

  const session = sources.sessions.get(userId);
  const status = runningStatus(session, sources.clientHealth?.get(userId), reconnecting);
  switch (status) {
    case "playing":
      return { state: "playing", placeId: session?.placeId ?? null, sinceMs: session?.sinceMs ?? null };
    case "waiting":
      return joining ? joiningState : { state: "waiting", placeId: null, sinceMs: null };
    case "unknown":
      return joining ? joiningState : NONE;
    default:
      return { state: status, placeId: session?.placeId ?? null, sinceMs: session?.sinceMs ?? null };
  }
}

/**
 * Fechou conta como atenção (decisão de 12/10/2026): ninguém vai reabrir a
 * conta sozinho, então ela espera a pessoa — "Entrar novamente" ou o ×.
 */
export function isAttention(state: AccountLiveKind): boolean {
  if (state === "closed") return true;
  return state !== "none" && state !== "joining" && needsAttention(state);
}

/**
 * Quem precisa de atenção agora (caiu, reconectando, não responde, fechou):
 * as contas com cliente aberto, as que reconectam sem cliente e as que
 * fecharam. Não depende da sessão — esses estados vêm antes dela na prioridade.
 */
export function attentionIds(sources: LiveSources): Set<number> {
  const ids = new Set<number>();
  for (const userId of new Set([...sources.running, ...sources.reconnecting, ...sources.closed.keys()])) {
    if (isAttention(accountLiveState(userId, sources).state)) ids.add(userId);
  }
  return ids;
}
