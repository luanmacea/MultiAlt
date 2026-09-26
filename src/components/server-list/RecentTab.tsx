import { RecentGamesList } from "./RecentGamesList";
import type { GameEntry } from "./types";

export interface RecentTabProps {
  onSelectGame: (placeId: number, name?: string, iconUrl?: string | null) => void;
  maxRecent: number;
  userId: number | null;
  /** Abre a lista de servidores daquele jogo. */
  onBrowseServers?: (placeId: number) => void;
  /** Salva o jogo nos favoritos (mesma ação da aba Games). */
  onAddFavorite?: (game: GameEntry) => void;
  /** Abre o Botting Mode / os Scripts **com este jogo**. */
  onBotting?: (placeId: number) => void;
  onScripts?: (placeId: number) => void;
}

export function RecentTab({
  onSelectGame,
  maxRecent,
  userId,
  onBrowseServers,
  onAddFavorite,
  onBotting,
  onScripts,
}: RecentTabProps) {
  return (
    <RecentGamesList
      userId={userId}
      maxRecent={maxRecent}
      onSelect={onSelectGame}
      onBrowseServers={onBrowseServers}
      onAddFavorite={onAddFavorite}
      onBotting={onBotting}
      onScripts={onScripts}
    />
  );
}
