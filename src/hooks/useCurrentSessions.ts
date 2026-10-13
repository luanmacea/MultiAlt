import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CurrentSession } from "../types";

const EMPTY: ReadonlyMap<number, CurrentSession> = new Map();

/**
 * O jogo de cada conta agora e desde quando (`get_current_sessions`): a sessão
 * aberta do histórico (commands/session_history.rs), o mesmo início do
 * "Playing now". Sem polling: lê ao montar, quando muda quem está rodando
 * (`refreshKey`) e a cada `session-history-changed` — que o backend manda em
 * toda entrada, teleporte, queda ou saída.
 */
export function useCurrentSessions(enabled: boolean, refreshKey = ""): ReadonlyMap<number, CurrentSession> {
  const [sessions, setSessions] = useState<ReadonlyMap<number, CurrentSession>>(EMPTY);

  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    const load = () => {
      invoke<CurrentSession[]>("get_current_sessions")
        .then((list) => {
          if (!alive) return;
          const rows = Array.isArray(list) ? list : [];
          setSessions(new Map(rows.map((s) => [s.userId, s])));
        })
        .catch(() => {
          if (alive) setSessions(EMPTY);
        });
    };
    load();
    const unlisten = listen("session-history-changed", load);
    return () => {
      alive = false;
      void unlisten.then((fn) => fn());
    };
  }, [enabled, refreshKey]);

  return enabled ? sessions : EMPTY;
}
