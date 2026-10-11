import { createContext, useContext, type ReactNode } from "react";
import { useStore } from "../store";
import { isWindowsPlatform } from "../utils/platform";
import { useCurrentSessions } from "./useCurrentSessions";
import {
  accountLiveState,
  activeReconnectIds,
  attentionIds,
  type AccountLiveState,
  type LiveSources,
} from "../utils/accountLiveState";
import type { CurrentSession } from "../types";

/**
 * Estado ao vivo das contas na lista de contas (ver utils/accountLiveState.ts).
 *
 * A sessão de agora (`get_current_sessions`) é lida UMA vez pela lista
 * (`AccountLiveStateProvider`), não uma vez por linha; o resto já está na
 * store. Sem cliente aberto nada é lido — a lista parada não toca o backend.
 */

const EMPTY: ReadonlyMap<number, CurrentSession> = new Map();
const SessionsContext = createContext<ReadonlyMap<number, CurrentSession>>(EMPTY);

export function AccountLiveStateProvider({ children }: { children: ReactNode }) {
  const store = useStore();
  const running = store.launchedByProgram;
  // Só no Windows: é lá que o log do Roblox é lido (mesma regra da Session).
  const enabled = isWindowsPlatform(store.platformCapabilities) && running.size > 0;
  const runningKey = [...running].sort((a, b) => a - b).join(",");
  const sessions = useCurrentSessions(enabled, runningKey);
  return <SessionsContext.Provider value={sessions}>{children}</SessionsContext.Provider>;
}

function useLiveSources(sessions: ReadonlyMap<number, CurrentSession>): LiveSources {
  const store = useStore();
  return {
    running: store.launchedByProgram,
    joining: store.joiningAccounts,
    launchQueue: store.launchQueue,
    clientHealth: store.clientHealth,
    reconnecting: activeReconnectIds(store.autoReconnect),
    sessions,
  };
}

/** O estado de uma conta. Fora do provider, sem sessão (só os estados da store). */
export function useAccountLiveState(userId: number): AccountLiveState {
  const sessions = useContext(SessionsContext);
  return accountLiveState(userId, useLiveSources(sessions));
}

/** Quantas contas precisam de atenção (caiu, reconectando, não responde). */
export function useAttentionCount(): number {
  return attentionIds(useLiveSources(EMPTY)).size;
}
