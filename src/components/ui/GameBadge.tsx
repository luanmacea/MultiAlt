/**
 * Diz, em uma linha, **qual jogo** é o Place ID que a tela está usando.
 *
 * Puramente visual: quem descobre nome e ícone é o `useGameIdentity`. Regra de
 * desenho: sem nome e sem ícone o componente **não desenha nada** — um
 * "Place 606849621" não informa mais do que o número que já está no campo ao
 * lado, e um esqueleto piscando a cada tecla informa menos.
 */
export interface GameBadgeProps {
  name: string | null;
  iconUrl: string | null;
  /** Só para o `title`, quando o nome ainda não veio. */
  placeId?: number;
  /**
   * Lado do ícone. Menor em linha de rótulo (10px), onde um ícone de 18px
   * empurraria o campo para baixo e desalinharia a coluna vizinha.
   */
  iconSize?: number;
  className?: string;
}

export function GameBadge({
  name,
  iconUrl,
  placeId,
  iconSize = 18,
  className = "",
}: GameBadgeProps) {
  if (!name && !iconUrl) return null;

  const label = name ?? (placeId ? `Place ${placeId}` : "");
  return (
    <span
      data-testid="game-badge"
      title={label}
      className={`inline-flex items-center gap-1.5 min-w-0 max-w-[220px] ${className}`}
    >
      {iconUrl ? (
        <img
          src={iconUrl}
          alt=""
          width={iconSize}
          height={iconSize}
          style={{ width: iconSize, height: iconSize }}
          className="rounded shrink-0 object-cover"
        />
      ) : null}
      {name ? (
        <span className="truncate text-[11px] text-[var(--panel-fg)]">{name}</span>
      ) : null}
    </span>
  );
}
