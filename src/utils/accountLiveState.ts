import type { AutoReconnectEntry, ClientHealth, CurrentSession, LaunchQueuePayload } from "../types";
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
 * - `none`: nada a dizer — conta parada, ou cliente aberto sem informação
 *   (fora do Windows o log não é lido; a bolinha "launched" já diz que abriu).
 */
export type AccountLiveKind = "playing" | "joining" | "reconnecting" | "dropped" | "notResponding" | "waiting" | "none";

export interface AccountLiveState {
  state: AccountLiveKind;
  /** O jogo, quando se sabe (sessão de agora ou alvo da fila). */
  placeId: number | null;
  /** Início da sessão de agora (só em jogo). */
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

export function isAttention(state: AccountLiveKind): boolean {
  return state !== "none" && state !== "joining" && needsAttention(state);
}

/**
 * Quem precisa de atenção agora (caiu, reconectando, não responde): as contas
 * com cliente aberto e as que reconectam sem cliente. Não depende da sessão —
 * esses três estados vêm antes dela na prioridade.
 */
export function attentionIds(sources: LiveSources): Set<number> {
  const ids = new Set<number>();
  for (const userId of new Set([...sources.running, ...sources.reconnecting])) {
    if (isAttention(accountLiveState(userId, sources).state)) ids.add(userId);
  }
  return ids;
}
