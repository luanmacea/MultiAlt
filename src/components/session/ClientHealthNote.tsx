import { Hourglass, WifiOff } from "lucide-react";
import { useTr } from "../../i18n/text";
import type { ClientHealth } from "../../types";
import { clientHealthLabel } from "../../utils/clientHealth";

/**
 * Aviso curto ao lado do nome da conta (Sessão → Em jogo e painel da conta):
 * por que ela caiu (vermelho), ou "Não respondendo" (âmbar). Some quando a
 * conta volta a um jogo / a janela volta a responder.
 */
export function ClientHealthNote({
  health,
  className = "",
  wrap = false,
}: {
  health: ClientHealth | null | undefined;
  className?: string;
  /**
   * Quebra em até 3 linhas em vez de cortar numa só. O painel da conta tem
   * espaço na vertical, e a mensagem de kick ("Kicked: You have been kicked
   * for being A…") não dizia nada cortada. O texto inteiro segue no tooltip.
   */
  wrap?: boolean;
}) {
  const t = useTr();
  const info = clientHealthLabel(health, t);
  if (!info) return null;
  const tooltip = info.detail ? `${info.label} — ${info.detail}` : info.label;
  const Icon = info.tone === "hung" ? Hourglass : WifiOff;
  const color = info.tone === "hung" ? "text-amber-400" : "text-red-400";
  return (
    <span
      data-testid="client-health-note"
      data-tone={info.tone}
      className={`inline-flex ${wrap ? "items-start" : "items-center"} gap-1 min-w-0 ${color} ${className}`}
      title={tooltip}
    >
      <Icon size={11} strokeWidth={1.75} className={`shrink-0${wrap ? " mt-[2px]" : ""}`} aria-hidden="true" />
      <span className={wrap ? "line-clamp-3 break-words" : "truncate"}>{info.label}</span>
    </span>
  );
}
