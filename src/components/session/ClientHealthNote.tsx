import { WifiOff } from "lucide-react";
import { useTr } from "../../i18n/text";
import type { ClientHealth } from "../../types";
import { clientHealthLabel } from "../../utils/clientHealth";

/**
 * Aviso curto de queda ao lado do nome da conta (Sessão → Em jogo e painel da
 * conta). Some quando a conta volta a um jogo.
 */
export function ClientHealthNote({
  health,
  className = "",
}: {
  health: ClientHealth | null | undefined;
  className?: string;
}) {
  const t = useTr();
  const info = clientHealthLabel(health, t);
  if (!info) return null;
  const tooltip = info.detail ? `${info.label} — ${info.detail}` : info.label;
  return (
    <span
      data-testid="client-health-note"
      className={`inline-flex items-center gap-1 min-w-0 text-red-400 ${className}`}
      title={tooltip}
    >
      <WifiOff size={11} strokeWidth={1.75} className="shrink-0" aria-hidden="true" />
      <span className="truncate">{info.label}</span>
    </span>
  );
}
