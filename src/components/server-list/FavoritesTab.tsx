import { useState } from "react";
import { usePrompt } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import type { FavoriteGame, VipServer } from "./types";
import { loadFavorites, saveFavorites, makeVipId } from "./types";
import { FavoriteContextMenu } from "./FavoriteContextMenu";

export interface FavoritesTabProps {
  onSelectGame: (placeId: number, privateServer?: string) => void;
  addToast: (msg: string) => void;
}

export function FavoritesTab({
  onSelectGame,
  addToast,
}: FavoritesTabProps) {
  const t = useTr();
  const prompt = usePrompt();
  const [favorites, setFavorites] = useState<FavoriteGame[]>(loadFavorites);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; game: FavoriteGame } | null>(null);

  function persist(updated: FavoriteGame[]) {
    setFavorites(updated);
    saveFavorites(updated);
  }

  function getVips(game: FavoriteGame): VipServer[] {
    return game.vipServers ?? [];
  }

  async function handleRename(game: FavoriteGame) {
    const newName = await prompt(t("Rename favorite:"), game.name);
    if (!newName?.trim()) return;
    persist(favorites.map((f) => (f.placeId === game.placeId ? { ...f, name: newName.trim() } : f)));
    addToast(t("Renamed"));
  }

  function handleRemove(game: FavoriteGame) {
    persist(favorites.filter((f) => f.placeId !== game.placeId));
    addToast(t("Removed from favorites"));
  }

  async function handleAddVip(game: FavoriteGame) {
    const link = await prompt(t("Private server link or VIP code:"), "");
    if (!link?.trim()) return;
    const label = await prompt(t("Name for this server (optional):"), "");
    if (label === null) return; // cancelled
    const vips = getVips(game);
    const name = label.trim() || `${t("VIP")} ${vips.length + 1}`;
    const newVip: VipServer = { id: makeVipId(), name, link: link.trim() };
    persist(
      favorites.map((f) =>
        f.placeId === game.placeId
          ? { ...f, vipServers: [...getVips(f), newVip], privateServer: undefined }
          : f
      )
    );
    addToast(t("VIP server added"));
  }

  function handleRemoveVip(game: FavoriteGame, vipId: string) {
    persist(
      favorites.map((f) =>
        f.placeId === game.placeId ? { ...f, vipServers: getVips(f).filter((v) => v.id !== vipId) } : f
      )
    );
    addToast(t("VIP server removed"));
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
          {favorites.map((game) => {
            const vips = getVips(game);
            const expanded = expandedId === game.placeId;
            return (
              <div key={game.placeId} className="rounded-lg">
                <div
                  className="flex items-center gap-3 p-2 rounded-lg hover:bg-zinc-800/40 transition-colors cursor-pointer group"
                  onClick={() => setExpandedId(expanded ? null : game.placeId)}
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
                  {vips.length > 0 && (
                    <span className="text-[9px] text-amber-400/70 bg-amber-500/10 px-1.5 py-0.5 rounded shrink-0">
                      {vips.length} {t("VIP")}
                    </span>
                  )}
                  <svg
                    width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
                    className={`text-zinc-600 shrink-0 transition-transform ${expanded ? "rotate-180" : ""}`}
                  >
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                </div>

                {expanded && (
                  <div className="ml-12 mr-1 mb-2 mt-1 flex flex-col gap-1.5">
                    <button
                      onClick={() => onSelectGame(game.placeId)}
                      className="flex items-center justify-center gap-2 w-full px-3 py-1.5 rounded-lg text-[12px] font-medium bg-sky-600 hover:bg-sky-500 text-white transition-colors"
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <polygon points="5 3 19 12 5 21 5 3" />
                      </svg>
                      {t("Join Game")}
                    </button>

                    {vips.length > 0 && (
                      <div className="flex flex-col gap-1 mt-1">
                        {vips.map((vip) => (
                          <div key={vip.id} className="flex items-center gap-2 px-2 py-1.5 rounded-lg bg-zinc-800/40">
                            <div className="flex-1 min-w-0">
                              <div className="text-[11px] text-amber-300/90 truncate">{vip.name}</div>
                              <div className="text-[9px] text-zinc-600 font-mono truncate">{vip.link}</div>
                            </div>
                            <button
                              onClick={() => onSelectGame(game.placeId, vip.link)}
                              className="px-3 py-1 rounded-md text-[11px] font-medium bg-emerald-600 hover:bg-emerald-500 text-white transition-colors shrink-0"
                            >
                              {t("Join")}
                            </button>
                            <button
                              onClick={() => handleRemoveVip(game, vip.id)}
                              title={t("Remove")}
                              className="p-1 rounded-md text-zinc-500 hover:text-red-400 hover:bg-zinc-800 transition-colors shrink-0"
                            >
                              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                                <line x1="18" y1="6" x2="6" y2="18" />
                                <line x1="6" y1="6" x2="18" y2="18" />
                              </svg>
                            </button>
                          </div>
                        ))}
                      </div>
                    )}

                    <button
                      onClick={() => handleAddVip(game)}
                      className="flex items-center justify-center gap-1.5 w-full px-3 py-1.5 rounded-lg text-[11px] font-medium text-zinc-400 border border-dashed border-zinc-700 hover:border-zinc-600 hover:text-zinc-300 transition-colors mt-1"
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <line x1="12" y1="5" x2="12" y2="19" />
                        <line x1="5" y1="12" x2="19" y2="12" />
                      </svg>
                      {t("Add VIP Server")}
                    </button>
                  </div>
                )}
              </div>
            );
          })}
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
