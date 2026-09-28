import { useState, useEffect, useRef } from "react";
import type { GameEntry, RecentGame } from "./types";
import { loadRecentGames, saveRecentGames, resolveRecentGame } from "./types";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import { GameRowActions, browseServersIcon, favoriteIcon, joinGameIcon } from "./GamesTab";
import { GameContextMenu } from "./GameContextMenu";

export interface RecentGamesListProps {
  userId: number | null;
  maxRecent: number;
  onSelect: (placeId: number, name?: string, iconUrl?: string | null) => void;
  /**
   * O "Join Game" da linha e do menu: entra no jogo, como na aba Games. Sem
   * ele (o popover de escolha de jogo), "Join Game" é o mesmo que escolher.
   */
  onJoinGame?: (placeId: number) => void;
  /** Abre a lista de servidores daquele jogo. */
  onBrowseServers?: (placeId: number) => void;
  /** Salva o jogo nos favoritos (mesma ação da aba Games). */
  onAddFavorite?: (game: GameEntry) => void;
  /** Abre o Auto Rejoin / os Scripts **com este jogo**. */
  onBotting?: (placeId: number) => void;
  onScripts?: (placeId: number) => void;
}

export function RecentGamesList({
  userId,
  maxRecent,
  onSelect,
  onJoinGame,
  onBrowseServers,
  onAddFavorite,
  onBotting,
  onScripts,
}: RecentGamesListProps) {
  const t = useTr();
  const confirm = useConfirm();
  const [games, setGames] = useState<RecentGame[]>(loadRecentGames);
  /**
   * Os recentes eram a única lista de jogos sem clique direito: as ações do
   * jogo estavam só nos botões da linha, e Auto Rejoin/Scripts em lugar nenhum.
   */
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; game: RecentGame } | null>(
    null
  );
  const backfilledRef = useRef(false);

  useEffect(() => {
    if (backfilledRef.current) return;
    backfilledRef.current = true;

    let cancelled = false;
    (async () => {
      const next = loadRecentGames();
      let changed = false;
      for (let i = 0; i < next.length; i++) {
        const patched = await resolveRecentGame(next[i], userId);
        if (cancelled) return;
        if (patched) {
          next[i] = patched;
          changed = true;
          setGames([...next]);
        }
      }
      if (!cancelled && changed) {
        saveRecentGames(next);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [userId]);

  /**
   * A lista de recentes só existe neste computador (`localStorage`): apagada,
   * não volta. A frase diz quantos jogos somem antes de somarem.
   */
  async function handleClear() {
    const ok = await confirm(
      games.length === 1
        ? t("Clear the recent games list? Its 1 game is deleted from this computer and cannot be recovered.")
        : t(
            "Clear the recent games list? Its {{count}} games are deleted from this computer and cannot be recovered.",
            { count: games.length }
          ),
      true
    );
    if (!ok) return;
    saveRecentGames([]);
    setGames([]);
  }

  function formatTime(ts: number) {
    const diff = Date.now() - ts;
    const mins = Math.floor(diff / 60000);
    const hours = Math.floor(mins / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return t("{{count}}d ago", { count: days });
    if (hours > 0) return t("{{count}}h ago", { count: hours });
    if (mins > 0) return t("{{count}}m ago", { count: mins });
    return t("just now");
  }

  if (games.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full py-8">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" className="text-zinc-800 mb-3">
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
        <p className="text-xs text-zinc-700">{t("No recent games")}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex items-center justify-between px-1 pb-2">
        <span className="text-[11px] text-zinc-600">{t("{{count}} of {{max}} max", { count: games.length, max: maxRecent })}</span>
        <button
          onClick={() => {
            void handleClear();
          }}
          className="text-[11px] text-zinc-600 hover:text-red-400 transition-colors"
        >
          {t("Clear all")}
        </button>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="flex flex-col gap-1 px-1">
          {games.map((game) => (
            <div
              key={game.placeId}
              role="button"
              tabIndex={0}
              className="flex items-center gap-3 p-2 rounded-lg hover:bg-zinc-800/40 transition-colors cursor-pointer outline-none"
              onClick={() => onSelect(game.placeId, game.name, game.iconUrl)}
              onContextMenu={(e) => {
                e.preventDefault();
                setContextMenu({ x: e.clientX, y: e.clientY, game });
              }}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget) return;
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(game.placeId, game.name, game.iconUrl);
                }
              }}
            >
              <div className="w-9 h-9 rounded-md bg-zinc-800 shrink-0 overflow-hidden">
                {game.iconUrl ? (
                  <img src={game.iconUrl} alt="" className="w-full h-full object-cover" />
                ) : (
                  <div className="w-full h-full flex items-center justify-center">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-zinc-700">
                      <circle cx="12" cy="12" r="10" />
                      <polyline points="12 6 12 12 16 14" />
                    </svg>
                  </div>
                )}
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-[12px] text-zinc-200 truncate">{game.name}</div>
                <div className="text-[11px] text-zinc-600 font-mono">{t("ID: {{id}}", { id: game.placeId })}</div>
              </div>
              <span className="text-[11px] text-zinc-600 shrink-0">{formatTime(game.lastPlayed)}</span>
              {/* Mesmas ações da aba Games: aqui a lista só sabia lançar. */}
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
                    onClick: () =>
                      onJoinGame
                        ? onJoinGame(game.placeId)
                        : onSelect(game.placeId, game.name, game.iconUrl),
                  },
                ]}
              />
            </div>
          ))}
        </div>
      </div>

      {contextMenu && (
        <GameContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          game={{
            placeId: contextMenu.game.placeId,
            name: contextMenu.game.name,
            playerCount: 0,
            likeRatio: null,
            iconUrl: contextMenu.game.iconUrl,
          }}
          onClose={() => setContextMenu(null)}
          onJoin={() =>
            onJoinGame
              ? onJoinGame(contextMenu.game.placeId)
              : onSelect(contextMenu.game.placeId, contextMenu.game.name, contextMenu.game.iconUrl)
          }
          onFavorite={() =>
            onAddFavorite?.({
              placeId: contextMenu.game.placeId,
              name: contextMenu.game.name,
              playerCount: 0,
              likeRatio: null,
              iconUrl: contextMenu.game.iconUrl,
            })
          }
          onCopyPlaceId={() => {
            navigator.clipboard.writeText(String(contextMenu.game.placeId));
          }}
          onBrowseServers={
            onBrowseServers ? () => onBrowseServers(contextMenu.game.placeId) : undefined
          }
          onBotting={onBotting ? () => onBotting(contextMenu.game.placeId) : undefined}
          onScripts={onScripts ? () => onScripts(contextMenu.game.placeId) : undefined}
        />
      )}
    </div>
  );
}
