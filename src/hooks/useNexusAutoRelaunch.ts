import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

const EMPTY: ReadonlySet<string> = new Set();

/**
 * Nomes de usuário (minúsculos) com o AutoRelaunch do Nexus ligado.
 * `reconnect_enabled` (commands/reconnect.rs) reconecta essas contas mesmo com
 * a reconexão desligada nelas — a tela tem de mostrar isso. Mesmo critério do
 * backend: nome sem diferença de maiúsculas. Sem o Nexus no build, a lista vem
 * vazia. Uma leitura por mudança de `refreshKey` (ex.: quem está em jogo), não
 * uma por linha.
 */
export function useNexusAutoRelaunch(enabled: boolean, refreshKey = ""): ReadonlySet<string> {
  const [names, setNames] = useState<ReadonlySet<string>>(EMPTY);
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    invoke<{ username?: string; auto_relaunch?: boolean }[]>("get_nexus_accounts")
      .then((accounts) => {
        if (cancelled) return;
        const list = Array.isArray(accounts) ? accounts : [];
        setNames(
          new Set(
            list
              .filter((a) => a?.auto_relaunch === true && typeof a.username === "string")
              .map((a) => (a.username as string).toLowerCase())
          )
        );
      })
      .catch(() => {
        if (!cancelled) setNames(EMPTY);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, refreshKey]);
  return enabled ? names : EMPTY;
}
