import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import type { GameEntry } from "./types";
import { GameContextMenu } from "./GameContextMenu";
import { Tooltip } from "../ui/Tooltip";
import { tr, useTr } from "../../i18n/text";
import { Search, Play } from "lucide-react";

export interface GamesTabProps {
  onSelectGame: (placeId: number, name: string, iconUrl: string | null) => void;
  onJoinGame: (placeId: number) => void;
  addToast: (msg: string) => void;
  onAddFavorite: (game: GameEntry) => void;
}

export function GamesTab({
  onSelectGame,
  onJoinGame,
  addToast,
  onAddFavorite,
}: GamesTabProps) {
  const t = useTr();
  const [search, setSearch] = useState("");
  const [games, setGames] = useState<GameEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; game: GameEntry } | null>(null);
  const store = useStore();
  const searchTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const searchGames = useCallback(async (keyword: string) => {
    setLoading(true);
    try {
      const searchResult: any = await invoke("search_games", {
        securityToken: store.selectedAccount?.SecurityToken || null,
        keyword,
        start: 0,
      });

      const entries: GameEntry[] = [];
      const seen = new Set<number>();

      const addGame = (game: any) => {
        const placeId = game.rootPlaceId || game.placeId || 0;
        const universeId = game.universeId || game.contentId || 0;
        const key = universeId || placeId;
        if (!key || seen.has(key)) return;
        seen.add(key);
        entries.push({
          placeId,
          name: game.name || t("Unknown"),
          playerCount: game.playerCount || 0,
          likeRatio: game.totalUpVotes > 0
            ? Math.round((game.totalUpVotes / (game.totalUpVotes + game.totalDownVotes)) * 100)
            : null,
          iconUrl: null,
          universeId,
        });
      };

      if (keyword) {
        const searchResults = searchResult?.searchResults || [];
        for (const group of searchResults) {
          for (const item of group?.contents || []) {
            if (item.contentType === "Game") addGame(item);
          }
        }
      } else {
        const sorts = searchResult?.sorts || [];
        for (const sort of sorts) {
          for (const game of sort?.games || []) {
            addGame(game);
          }
        }
      }

      setGames(entries);

      const placeIds = entries.filter((g) => g.placeId > 0).map((g) => g.placeId).slice(0, 20);
      if (placeIds.length > 0) {
        for (const game of entries.slice(0, 20)) {
          if (game.placeId > 0) {
            invoke<string | null>("batched_get_game_icon", {
              placeId: game.placeId,
              userId: store.selectedAccount?.UserID || null,
            }).then((url) => {
              if (url) {
                setGames((prev) =>
                  prev.map((g) => (g.placeId === game.placeId ? { ...g, iconUrl: url } : g))
                );
              }
            }).catch(() => {});
          }
        }
      }
    } catch (e) {
      addToast(tr("Search failed: {{error}}", { error: String(e) }));
    }
    setLoading(false);
  }, [addToast, store.selectedAccount, t]);

  useEffect(() => {
    searchGames("");
  }, []);

  function handleSearchInput(value: string) {
    setSearch(value);
    if (searchTimeout.current) clearTimeout(searchTimeout.current);
    searchTimeout.current = setTimeout(() => searchGames(value), 400);
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-1 pb-3">
        <div className="relative flex-1">
          <Search size={14} strokeWidth={2} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-600" />
          <input
            value={search}
            onChange={(e) => handleSearchInput(e.target.value)}
            placeholder={t("Search games...")}
            className="w-full pl-9 pr-3 py-[6px] bg-zinc-900/60 border border-zinc-800 rounded-lg text-[13px] text-zinc-300 placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
          />
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto">
        {loading && games.length === 0 ? (
          <div className="flex items-center justify-center py-16">
            <div className="w-4 h-4 border-2 border-sky-500 border-t-transparent rounded-full animate-spin" />
          </div>
        ) : games.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16">
            <Search size={28} strokeWidth={1} className="text-zinc-800 mb-2" />
            <span className="text-xs text-zinc-700">{t("No games found")}</span>
          </div>
        ) : (
          <div className="grid grid-cols-2 gap-2 px-1">
            {games.map((game) => (
              <div
                key={`${game.placeId}-${game.name}`}
                className="flex items-center gap-2.5 p-2 rounded-lg border border-zinc-800/40 bg-zinc-900/40 hover:bg-zinc-800/50 hover:border-zinc-700/60 transition-all cursor-pointer group"
                onClick={() => onSelectGame(game.placeId, game.name, game.iconUrl)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setContextMenu({ x: e.clientX, y: e.clientY, game });
                }}
              >
                <div className="w-10 h-10 rounded-md bg-zinc-800 shrink-0 overflow-hidden">
                  {game.iconUrl ? (
                    <img src={game.iconUrl} alt="" className="w-full h-full object-cover" />
                  ) : (
                    <div className="w-full h-full flex items-center justify-center">
                      <Play size={16} strokeWidth={1.5} className="text-zinc-700" />
                    </div>
                  )}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-[12px] text-zinc-200 truncate group-hover:text-zinc-100 transition-colors">
                    {game.name}
                  </div>
                  <div className="flex items-center gap-2 mt-0.5">
                    {game.playerCount > 0 && (
                      <span className="text-[10px] text-zinc-500">
                        {game.playerCount >= 1000
                          ? `${(game.playerCount / 1000).toFixed(1)}K`
                          : game.playerCount} {t("playing")}
                      </span>
                    )}
                    {game.likeRatio !== null && (
                      <div className="flex items-center gap-1">
                        <div className="w-8 h-1 bg-zinc-800 rounded-full overflow-hidden">
                          <div
                            className="h-full bg-emerald-500/60 rounded-full"
                            style={{ width: `${game.likeRatio}%` }}
                          />
                        </div>
                        <span className="text-[9px] text-zinc-600">{game.likeRatio}%</span>
                      </div>
                    )}
                  </div>
                </div>
                <Tooltip content={t("Join Game")}>
                  <button
                    onClick={(e) => { e.stopPropagation(); onJoinGame(game.placeId); }}
                    className="shrink-0 w-7 h-7 rounded-md bg-emerald-600/20 hover:bg-emerald-600/40 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-all"
                  >
                    <Play size={12} fill="currentColor" stroke="none" className="text-emerald-400" />
                  </button>
                </Tooltip>
              </div>
            ))}
          </div>
        )}
      </div>

      {contextMenu && (
        <GameContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          game={contextMenu.game}
          onClose={() => setContextMenu(null)}
          onJoin={() => onJoinGame(contextMenu.game.placeId)}
          onFavorite={() => onAddFavorite(contextMenu.game)}
          onCopyPlaceId={() => {
            navigator.clipboard.writeText(String(contextMenu.game.placeId));
            addToast(tr("Copied Place ID"));
          }}
        />
      )}
    </div>
  );
}
