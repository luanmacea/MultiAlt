import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import type { GameEntry } from "./types";
import { GameContextMenu } from "./GameContextMenu";
import { tr, useTr } from "../../i18n/text";
import { Search, Play, Loader2 } from "lucide-react";
import { SessionCache } from "../../utils/sessionCache";
import { GAME_CARD_GRID, GameCard } from "./GameCard";

/**
 * A última lista da aba Games e a busca que a produziu.
 *
 * Sair da aba (ou da Choose Game) desmontava tudo e a volta começava com o
 * spinner e a lista vazia. Agora a volta mostra isto na hora — com a mesma
 * busca no campo — e consulta de novo por trás. É compartilhado com a aba
 * Games do Server List, que é o mesmo componente.
 */
interface GamesSnapshot {
  keyword: string;
  games: GameEntry[];
}

const gamesCache = new SessionCache<GamesSnapshot>(1);
const GAMES_CACHE_KEY = "last";

// As ações da linha moram em GameRowActions.tsx (o GameCard também usa);
// continuam exportadas daqui para quem já importava desta aba.
export { GameRowActions, browseServersIcon, favoriteIcon, joinGameIcon } from "./GameRowActions";
export type { GameRowAction } from "./GameRowActions";
import { browseServersIcon, favoriteIcon, joinGameIcon } from "./GameRowActions";

export interface GamesTabProps {
  onSelectGame: (placeId: number, name: string, iconUrl: string | null) => void;
  onJoinGame: (placeId: number) => void;
  addToast: (msg: string) => void;
  onAddFavorite: (game: GameEntry) => void;
  /**
   * Abre a lista de servidores daquele jogo. Opcional: ação sem callback não
   * aparece. As duas telas donas (Choose Game e Server List) passam as três.
   */
  onBrowseServers?: (placeId: number, name?: string) => void;
  /** Abre o Auto Rejoin / os Scripts **com este jogo**, sem copiar Place ID. */
  onBotting?: (placeId: number) => void;
  onScripts?: (placeId: number) => void;
}

export function GamesTab({
  onSelectGame,
  onJoinGame,
  addToast,
  onAddFavorite,
  onBrowseServers,
  onBotting,
  onScripts,
}: GamesTabProps) {
  const t = useTr();
  const [cached] = useState(() => gamesCache.get(GAMES_CACHE_KEY));
  const [search, setSearch] = useState(cached?.keyword ?? "");
  const [games, setGames] = useState<GameEntry[]>(cached?.games ?? []);
  /** A busca de onde veio a lista na tela — é ela que vai para o cache. */
  const [loadedKeyword, setLoadedKeyword] = useState<string | null>(cached?.keyword ?? null);
  const [loading, setLoading] = useState(false);
  const gamesRef = useRef(games);
  gamesRef.current = games;
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; game: GameEntry } | null>(null);
  const store = useStore();
  const searchTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const searchGames = useCallback(async (keyword: string) => {
    setLoading(true);
    // Ícone que já está na tela continua: a lista nova não pisca sem ícone e
    // não pede de novo o que já veio.
    const knownIcons = new Map<number, string>();
    for (const game of gamesRef.current) {
      if (game.iconUrl) knownIcons.set(game.placeId, game.iconUrl);
    }
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
          iconUrl: knownIcons.get(placeId) ?? null,
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
      setLoadedKeyword(keyword);

      const placeIds = entries.filter((g) => g.placeId > 0).map((g) => g.placeId).slice(0, 20);
      if (placeIds.length > 0) {
        for (const game of entries.slice(0, 20)) {
          if (game.placeId > 0 && !game.iconUrl) {
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

  // Ao montar: a última busca (ou a vitrine, sem busca), por trás da lista
  // guardada se houver.
  useEffect(() => {
    searchGames(cached?.keyword ?? "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (loadedKeyword !== null) gamesCache.set(GAMES_CACHE_KEY, { keyword: loadedKeyword, games });
  }, [games, loadedKeyword]);

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
        {loading && games.length > 0 && (
          // A lista na tela é a anterior; a nova chega e a substitui.
          <span
            data-testid="games-updating"
            className="shrink-0 flex items-center gap-1.5 text-[11px] text-zinc-500"
          >
            <Loader2 size={11} className="animate-spin" />
            {t("Updating...")}
          </span>
        )}
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
          // Grade de cards, como a vitrine do site do Roblox (pedido do dono,
          // 14/10/2026): quanto mais larga a janela, mais colunas.
          <div className={`${GAME_CARD_GRID} px-1`}>
            {games.map((game) => (
              <GameCard
                key={`${game.placeId}-${game.name}`}
                variant="grid"
                placeId={game.placeId}
                name={game.name}
                iconUrl={game.iconUrl}
                fallbackIcon={<Play size={22} strokeWidth={1.5} />}
                subtitle={
                  game.playerCount > 0 || game.likeRatio !== null ? (
                    <span className="flex items-center gap-2">
                      {game.likeRatio !== null && <span>👍 {game.likeRatio}%</span>}
                      {game.playerCount > 0 && (
                        <span>
                          {game.playerCount >= 1000
                            ? `${(game.playerCount / 1000).toFixed(1)}K`
                            : game.playerCount}{" "}
                          {t("playing")}
                        </span>
                      )}
                    </span>
                  ) : undefined
                }
                onOpen={() => onSelectGame(game.placeId, game.name, game.iconUrl)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setContextMenu({ x: e.clientX, y: e.clientY, game });
                }}
                actions={[
                  ...(onBrowseServers
                    ? [
                        {
                          key: "servers",
                          label: t("Browse servers"),
                          icon: browseServersIcon,
                          onClick: () => onBrowseServers(game.placeId, game.name),
                        },
                      ]
                    : []),
                  {
                    key: "favorite",
                    label: t("Favorite"),
                    icon: favoriteIcon,
                    onClick: () => onAddFavorite(game),
                  },
                  {
                    key: "join",
                    label: t("Join Game"),
                    icon: joinGameIcon,
                    onClick: () => onJoinGame(game.placeId),
                  },
                ]}
              />
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
          onBrowseServers={
            onBrowseServers
              ? () => onBrowseServers(contextMenu.game.placeId, contextMenu.game.name)
              : undefined
          }
          onBotting={onBotting ? () => onBotting(contextMenu.game.placeId) : undefined}
          onScripts={onScripts ? () => onScripts(contextMenu.game.placeId) : undefined}
        />
      )}
    </div>
  );
}
