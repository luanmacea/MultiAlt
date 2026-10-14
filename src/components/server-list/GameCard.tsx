import type { MouseEvent, ReactNode } from "react";
import { Gamepad2 } from "lucide-react";
import { GameRowActions, type GameRowAction } from "./GameRowActions";

/**
 * Um jogo em formato de card (ícone quadrado grande, nome embaixo), como no
 * site do Roblox. Usado nas fileiras da aba Recentes e na grade das abas Jogos
 * e Favoritos da Escolher jogo (ver `CardRail`).
 *
 * O card inteiro é o clique principal (o mesmo das linhas antigas: abrir os
 * servidores / escolher o jogo); as ações ficam sempre visíveis embaixo, pela
 * mesma regra das linhas (`GameRowActions`: quem não passa o mouse por cima
 * também precisa achá-las).
 */
export interface GameCardProps {
  placeId: number;
  name: string;
  iconUrl: string | null;
  /** Linha pequena embaixo do nome (há 44m, jogando, ID). */
  subtitle?: ReactNode;
  /** Selo no canto da imagem (VIPs salvos, nos favoritos). */
  badge?: ReactNode;
  onOpen: () => void;
  onContextMenu?: (e: MouseEvent<HTMLDivElement>) => void;
  actions: GameRowAction[];
  testId?: string;
  /** `rail` = largura fixa para a fileira; `grid` = ocupa a coluna da grade. */
  variant?: "rail" | "grid";
  /** Destaque (favorito aberto). */
  active?: boolean;
  fallbackIcon?: ReactNode;
}

export function GameCard({
  placeId,
  name,
  iconUrl,
  subtitle,
  badge,
  onOpen,
  onContextMenu,
  actions,
  testId,
  variant = "rail",
  active = false,
  fallbackIcon,
}: GameCardProps) {
  return (
    <div
      role="button"
      tabIndex={0}
      data-testid={testId}
      className={`group shrink-0 flex flex-col gap-1.5 rounded-xl p-1.5 cursor-pointer outline-none transition-colors focus-visible:ring-2 focus-visible:ring-[var(--accent-color)] ${
        variant === "rail" ? "w-[150px]" : "w-full min-w-0"
      } ${active ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--panel-soft)]"}`}
      onClick={onOpen}
      onContextMenu={onContextMenu}
      onKeyDown={(e) => {
        if (e.target !== e.currentTarget) return;
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
    >
      <div className="relative aspect-square w-full rounded-lg overflow-hidden bg-[var(--panel-muted)]">
        {iconUrl ? (
          <img
            src={iconUrl}
            alt=""
            loading="lazy"
            className="w-full h-full object-cover transition-transform duration-200 group-hover:scale-[1.03]"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center theme-muted">
            {fallbackIcon ?? <Gamepad2 size={28} strokeWidth={1.25} />}
          </div>
        )}
        {badge ? <div className="absolute top-1.5 right-1.5">{badge}</div> : null}
      </div>
      <div className="px-0.5 min-w-0">
        <div className="text-[13px] font-medium leading-snug text-[var(--panel-fg)] line-clamp-2 break-words min-h-[2.5em]">
          {name}
        </div>
        {subtitle ? <div className="mt-0.5 text-[11px] theme-muted truncate">{subtitle}</div> : null}
      </div>
      <div className="px-0.5">
        <GameRowActions placeId={placeId} actions={actions} />
      </div>
    </div>
  );
}

/** Grade de cards que se ajusta à largura: quanto mais larga a tela, mais colunas. */
export const GAME_CARD_GRID = "grid gap-2 [grid-template-columns:repeat(auto-fill,minmax(150px,1fr))]";
