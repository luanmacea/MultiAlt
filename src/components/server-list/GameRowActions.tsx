import type { ReactNode } from "react";
import { Play, Server, Star } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";

/** Uma ação da linha de um jogo (ver servidores, favoritar, entrar). */
export interface GameRowAction {
  key: string;
  /** Texto em inglês: vira `aria-label` e tooltip. */
  label: string;
  icon: ReactNode;
  onClick: () => void;
}

/**
 * Ações de uma linha de jogo — as mesmas em Games, Favorites e Recent.
 *
 * Ficam **sempre visíveis**. Antes eram `opacity-0 group-hover:opacity-100`,
 * então a ação mais útil da tela (ver os servidores do jogo) era invisível em
 * repouso: quem não passa o mouse por cima nunca descobre que existe.
 *
 * O peso é o da lista da aba Servers: botão fantasma, ícone apagado, contraste
 * só no hover. Aparece sem competir com o nome do jogo.
 *
 * O container corta a propagação do clique: a linha inteira tem a sua própria
 * ação (lançar, nas listas Games/Recent; expandir, nos favoritos) e usar um
 * botão não pode disparar as duas coisas.
 */
export function GameRowActions({ placeId, actions }: { placeId: number; actions: GameRowAction[] }) {
  if (actions.length === 0) return null;
  return (
    <div
      data-testid={`game-actions-${placeId}`}
      className="shrink-0 flex items-center gap-1"
      onClick={(e) => e.stopPropagation()}
    >
      {actions.map((action) => (
        <Tooltip key={action.key} content={action.label}>
          <button
            onClick={(e) => {
              e.stopPropagation();
              action.onClick();
            }}
            aria-label={action.label}
            className="w-7 h-7 rounded-md theme-btn-ghost border theme-border theme-muted hover:text-[var(--panel-fg)] flex items-center justify-center transition-colors"
          >
            {action.icon}
          </button>
        </Tooltip>
      ))}
    </div>
  );
}

/** Ícone de "ver servidores" — mesmo desenho nas três listas. */
export const browseServersIcon = <Server size={12} strokeWidth={1.5} />;
/** Ícone de "favoritar". */
export const favoriteIcon = <Star size={12} strokeWidth={1.5} />;
/** Ícone de "entrar" — o único com cor própria, porque é o que lança. */
export const joinGameIcon = (
  <Play size={12} fill="currentColor" stroke="none" className="text-emerald-400" />
);
