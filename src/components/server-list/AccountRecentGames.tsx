import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, RefreshCw } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { GameRowActions, browseServersIcon, favoriteIcon, joinGameIcon } from "./GamesTab";
import type { GameEntry } from "./types";

/** Um jogo do "Continuar" de uma conta (`get_recent_games_for_accounts`). */
export interface AccountRecentGame {
  placeId: number;
  universeId: number;
  name: string;
  iconUrl: string | null;
}

export interface AccountRecentGamesEntry {
  userId: number;
  games: AccountRecentGame[];
  error: string | null;
}

/**
 * O resultado fica na memória da sessão por um minuto: trocar de aba e voltar
 * não pede tudo de novo ao Roblox (cada conta é uma chamada).
 */
const CACHE_MS = 60_000;
const cache = new Map<string, { at: number; entries: AccountRecentGamesEntry[] }>();

export function clearAccountRecentGamesCache() {
  cache.clear();
}

export interface AccountRecentGamesProps {
  userIds: number[];
  onSelectGame: (placeId: number, name?: string, iconUrl?: string | null) => void;
  onJoinGame: (placeId: number) => void;
  onBrowseServers?: (placeId: number) => void;
  onAddFavorite?: (game: GameEntry) => void;
}

/**
 * Os jogos recentes **de cada conta**, como a aba Amigos mostra os amigos de
 * cada uma. Vêm do "Continuar" da página inicial do Roblox: jogos em que a
 * conta entrou de qualquer lugar (app, site, celular), não só pelo MultiAlt.
 */
export function AccountRecentGames({
  userIds,
  onSelectGame,
  onJoinGame,
  onBrowseServers,
  onAddFavorite,
}: AccountRecentGamesProps) {
  const t = useTr();
  const store = useStore();
  const accountLabel = useAccountLabel();
  const key = userIds.join(",");
  const [entries, setEntries] = useState<AccountRecentGamesEntry[] | null>(() => {
    const hit = cache.get(key);
    return hit && Date.now() - hit.at < CACHE_MS ? hit.entries : null;
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (userIds.length === 0) return;
    const hit = cache.get(key);
    if (reload === 0 && hit && Date.now() - hit.at < CACHE_MS) {
      setEntries(hit.entries);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<AccountRecentGamesEntry[]>("get_recent_games_for_accounts", { userIds })
      .then((result) => {
        if (cancelled) return;
        const list = Array.isArray(result) ? result : [];
        cache.set(key, { at: Date.now(), entries: list });
        setEntries(list);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // `userIds` entra pela chave: a mesma seleção em outro array não refaz.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, reload]);

  if (userIds.length === 0) return null;

  return (
    <section data-testid="account-recent-games" className="space-y-3">
      <div className="flex items-center justify-between gap-2 px-1">
        <div className="min-w-0">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
            {t("Recently played by each account")}
          </div>
          <div className="text-[11px] text-zinc-600">
            {t("From each account's Continue list on Roblox: games played from anywhere, not just MultiAlt.")}
          </div>
        </div>
        <button
          type="button"
          onClick={() => setReload((n) => n + 1)}
          disabled={loading}
          aria-label={t("Refresh")}
          className="shrink-0 p-1.5 rounded-md text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800/50 disabled:opacity-40"
        >
          <RefreshCw size={13} className={loading ? "animate-spin" : undefined} aria-hidden="true" />
        </button>
      </div>

      {error ? <div className="px-1 text-[12px] text-red-400 break-words">{error}</div> : null}
      {loading && !entries ? (
        <div className="flex items-center gap-2 px-1 text-[12px] text-zinc-500">
          <Loader2 size={13} className="animate-spin" aria-hidden="true" />
          {t("Reading each account's recent games...")}
        </div>
      ) : null}

      {(entries ?? []).map((entry) => {
        const account = store.accounts.find((a) => a.UserID === entry.userId);
        const name = accountLabel(account, entry.userId);
        return (
          <div key={entry.userId} data-testid={`account-recent-${entry.userId}`} className="space-y-1">
            <div className="flex items-center gap-2 px-1">
              <span className="text-[12px] font-medium text-zinc-300 truncate">{name}</span>
              <span className="text-[11px] text-zinc-600 tabular-nums">{entry.games.length}</span>
            </div>
            {entry.error ? (
              <div className="px-2 text-[11px] text-red-400/90 break-words">{entry.error}</div>
            ) : entry.games.length === 0 ? (
              <div className="px-2 text-[11px] text-zinc-600">{t("No recent games on Roblox for this account.")}</div>
            ) : (
              <div className="flex flex-col gap-1">
                {entry.games.map((game) => (
                  <div
                    key={game.placeId}
                    role="button"
                    tabIndex={0}
                    data-testid={`account-recent-${entry.userId}-${game.placeId}`}
                    className="flex items-center gap-3 p-2 rounded-lg hover:bg-zinc-800/40 transition-colors cursor-pointer outline-none"
                    onClick={() => onSelectGame(game.placeId, game.name, game.iconUrl)}
                    onKeyDown={(e) => {
                      if (e.target !== e.currentTarget) return;
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        onSelectGame(game.placeId, game.name, game.iconUrl);
                      }
                    }}
                  >
                    <div className="w-9 h-9 rounded-md bg-zinc-800 shrink-0 overflow-hidden">
                      {game.iconUrl ? <img src={game.iconUrl} alt="" className="w-full h-full object-cover" /> : null}
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="text-[12px] text-zinc-200 truncate">{game.name}</div>
                      <div className="text-[11px] text-zinc-600 font-mono">{t("ID: {{id}}", { id: game.placeId })}</div>
                    </div>
                    <GameRowActions
                      placeId={game.placeId}
                      actions={[
                        ...(onBrowseServers
                          ? [
                              {
                                key: "servers",
                                label: t("Browse servers"),
                                icon: browseServersIcon,
                                onClick: () => onBrowseServers(game.placeId),
                              },
                            ]
                          : []),
                        ...(onAddFavorite
                          ? [
                              {
                                key: "favorite",
                                label: t("Favorite"),
                                icon: favoriteIcon,
                                onClick: () =>
                                  onAddFavorite({
                                    placeId: game.placeId,
                                    name: game.name,
                                    playerCount: 0,
                                    likeRatio: null,
                                    iconUrl: game.iconUrl,
                                  }),
                              },
                            ]
                          : []),
                        {
                          key: "join",
                          label: t("Join Game"),
                          icon: joinGameIcon,
                          onClick: () => onJoinGame(game.placeId),
                        },
                      ]}
                    />
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </section>
  );
}
