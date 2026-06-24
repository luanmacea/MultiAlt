import { useState } from "react";
import { usePrompt } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import type { FavoriteGame } from "./types";
import { loadFavorites, saveFavorites } from "./types";
import { FavoriteContextMenu } from "./FavoriteContextMenu";

export interface FavoritesTabProps {
  onSelectGame: (placeId: number) => void;
  addToast: (msg: string) => void;
}

export function FavoritesTab({
  onSelectGame,
  addToast,
}: FavoritesTabProps) {
  const t = useTr();
  const prompt = usePrompt();
  const [favorites, setFavorites] = useState<FavoriteGame[]>(loadFavorites);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; game: FavoriteGame } | null>(null);

  async function handleRename(game: FavoriteGame) {
    const newName = await prompt(t("Rename favorite:"), game.name);
    if (!newName?.trim()) return;
    const updated = favorites.map((f) =>
      f.placeId === game.placeId ? { ...f, name: newName.trim() } : f
    );
    setFavorites(updated);
    saveFavorites(updated);
    addToast(t("Renamed"));
  }

  function handleRemove(game: FavoriteGame) {
    const updated = favorites.filter((f) => f.placeId !== game.placeId);
    setFavorites(updated);
    saveFavorites(updated);
    addToast(t("Removed from favorites"));
  }

  if (favorites.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" className="text-zinc-800 mb-3">
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
        <p className="text-xs text-zinc-700 mb-1">{t("No favorites yet")}</p>
        <p className="text-[10px] text-zinc-700">{t("Right-click a game in the Games tab to add one")}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="flex flex-col gap-1 px-1">
          {favorites.map((game) => (
            <div
              key={game.placeId}
              className="flex items-center gap-3 p-2 rounded-lg hover:bg-zinc-800/40 transition-colors cursor-pointer group"
              onClick={() => onSelectGame(game.placeId)}
              onContextMenu={(e) => {
                e.preventDefault();
                setContextMenu({ x: e.clientX, y: e.clientY, game });
              }}
            >
              <div className="w-9 h-9 rounded-md bg-zinc-800 shrink-0 overflow-hidden">
                {game.iconUrl ? (
                  <img src={game.iconUrl} alt="" className="w-full h-full object-cover" />
                ) : (
                  <div className="w-full h-full flex items-center justify-center">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-zinc-700">
                      <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                    </svg>
                  </div>
                )}
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-[12px] text-zinc-200 truncate">{game.name}</div>
                <div className="text-[10px] text-zinc-600 font-mono">{t("ID: {{id}}", { id: game.placeId })}</div>
              </div>
              {game.privateServer && (
                <span className="text-[9px] text-amber-400/70 bg-amber-500/10 px-1.5 py-0.5 rounded">{t("VIP")}</span>
              )}
            </div>
          ))}
        </div>
      </div>

      {contextMenu && (
        <FavoriteContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onJoin={() => onSelectGame(contextMenu.game.placeId)}
          onRename={() => handleRename(contextMenu.game)}
          onRemove={() => handleRemove(contextMenu.game)}
          onCopyPlaceId={() => {
            navigator.clipboard.writeText(String(contextMenu.game.placeId));
            addToast(t("Copied Place ID"));
          }}
        />
      )}
    </div>
  );
}
