import type { CSSProperties } from "react";
import { Clock, Gamepad2, Hammer, Rocket, Wifi, X, type LucideIcon } from "lucide-react";

/**
 * Selo de estado da conta (bolinha da linha e da legenda da barra de status).
 *
 * Cada estado tem **cor e ícone**: só a cor não serve para quem não distingue
 * vermelho de laranja ou verde de azul (pedido do dono, 08/10/2026 — ver
 * ACCESSIBILITY.md). As cores são as de antes; o ícone é a pista que não
 * depende delas.
 */
export const STATUS_KINDS = ["invalid", "aged", "launched", "online", "ingame", "studio"] as const;
export type StatusKind = (typeof STATUS_KINDS)[number];

const ICONS: Record<StatusKind, LucideIcon> = {
  invalid: X,
  aged: Clock,
  launched: Rocket,
  online: Wifi,
  ingame: Gamepad2,
  studio: Hammer,
};

/** Cores de sempre (as mesmas da legenda antiga). `aged` muda com a idade: vem por `color`. */
export const STATUS_COLORS: Record<StatusKind, string> = {
  invalid: "#ef4444",
  aged: "#f97316",
  launched: "#f59e0b",
  online: "#00a2ff",
  ingame: "#02b757",
  studio: "#4629d8",
};

export function StatusBadge({
  kind,
  label,
  color,
  background,
  size = 12,
  decorative = false,
}: {
  kind: StatusKind;
  /** Nome do estado para leitor de tela e tooltip. */
  label: string;
  color?: string;
  /** Fundo alternativo (ex.: o degradê da legenda de "sem uso 20d+"). */
  background?: string;
  size?: number;
  /** Na legenda o texto já está ao lado: o selo fica mudo para o leitor de tela não ler o nome duas vezes. */
  decorative?: boolean;
}) {
  const Icon = ICONS[kind];
  const style: CSSProperties = {
    width: size,
    height: size,
    boxShadow: "0 0 0 1px var(--app-bg)",
    backgroundColor: color ?? STATUS_COLORS[kind],
    ...(background ? { background } : null),
  };
  return (
    <span
      {...(decorative ? { "aria-hidden": true } : { role: "img", "aria-label": label })}
      data-status={kind}
      className="inline-flex items-center justify-center rounded-full shrink-0 text-white"
      style={style}
    >
      <Icon size={Math.round(size * 0.7)} strokeWidth={3} aria-hidden="true" />
    </span>
  );
}
