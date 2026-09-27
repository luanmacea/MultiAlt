import { RecentGamesList } from "./RecentGamesList";
import { RecentJobsList } from "./RecentJobsList";
import { useTr } from "../../i18n/text";
import type { GameEntry } from "./types";

export interface RecentTabProps {
  onSelectGame: (placeId: number, name?: string, iconUrl?: string | null) => void;
  maxRecent: number;
  userId: number | null;
  /** Abre a lista de servidores daquele jogo. */
  onBrowseServers?: (placeId: number) => void;
  /** Salva o jogo nos favoritos (mesma ação da aba Games). */
  onAddFavorite?: (game: GameEntry) => void;
  /** Abre o Auto Rejoin / os Scripts **com este jogo**. */
  onBotting?: (placeId: number) => void;
  onScripts?: (placeId: number) => void;
  /**
   * Preenche o Job ID do launch com um servidor recente. Sem este callback a
   * coluna de servidores **não aparece**: a tela dona não tem onde pôr o Job ID,
   * e item que não faz nada é pior que item ausente.
   */
  onSelectJob?: (placeId: number | null, raw: string) => void;
  maxRecentJobs?: number;
}

export function RecentTab({
  onSelectGame,
  maxRecent,
  userId,
  onBrowseServers,
  onAddFavorite,
  onBotting,
  onScripts,
  onSelectJob,
  maxRecentJobs = 12,
}: RecentTabProps) {
  const t = useTr();
  const games = (
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

  if (!onSelectJob) return games;

  return (
    <div className="flex h-full min-h-0 gap-3">
      <div className="flex flex-col min-w-0 flex-1 min-h-0">
        <div className="px-1 pb-1 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
          {t("Games")}
        </div>
        <div className="flex-1 min-h-0">{games}</div>
      </div>
      <div className="w-px bg-zinc-800/60" />
      <div className="flex flex-col w-[252px] shrink-0 min-h-0">
        <div className="px-1 pb-1 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
          {t("Servers")}
        </div>
        <div className="flex-1 min-h-0">
          <RecentJobsList userId={userId} maxRecent={maxRecentJobs} onSelect={onSelectJob} />
        </div>
      </div>
    </div>
  );
}
