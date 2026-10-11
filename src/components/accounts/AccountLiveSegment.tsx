import type { ReactNode } from "react";
import { Hourglass, RotateCw, WifiOff } from "lucide-react";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { useAccountLiveState } from "../../hooks/useAccountLiveState";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import { clientHealthLabel } from "../../utils/clientHealth";
import { SessionClock } from "../session/CurrentSessionLine";
import type { AccountLiveKind } from "../../utils/accountLiveState";

/**
 * O pedaço "ao vivo" de uma linha da lista de contas: o estado (mesmas
 * palavras e cores da página Session — faixa de números e "Em jogo") e o jogo,
 * e o tempo em jogo quando cabe. Conta parada não ganha nada: a linha fica
 * como sempre foi.
 *
 * Uma linha só, para a linha não mudar de altura. Com espaço é uma coluna de
 * largura fixa (40% da linha), para a palavra do estado começar no mesmo
 * ponto em todas as linhas. O que cede primeiro quando a lista é estreita é o
 * jogo (corta, e abaixo de ~34rem some com a coluna), depois o relógio; a
 * palavra do estado nunca some. Janela larga (≥1600 px) dá metade da linha e
 * o nome do jogo aparece inteiro. Tudo vai no tooltip.
 */

const STYLES: Record<Exclude<AccountLiveKind, "none">, { label: string; tone: string; mark: ReactNode }> = {
  playing: {
    label: "Playing",
    tone: "text-emerald-400",
    mark: <span aria-hidden="true" className="w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0" />,
  },
  joining: {
    label: "Joining",
    tone: "text-[var(--accent-color)]",
    mark: (
      <span
        aria-hidden="true"
        className="w-1.5 h-1.5 rounded-full bg-[var(--accent-color)] shrink-0 animate-pulse motion-reduce:animate-none"
      />
    ),
  },
  reconnecting: {
    label: "Reconnecting",
    tone: "text-amber-400",
    mark: <RotateCw size={11} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />,
  },
  notResponding: {
    label: "Not responding",
    tone: "text-amber-400",
    mark: <Hourglass size={11} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />,
  },
  dropped: {
    // Chave própria: o "Dropped" da faixa de números da Session é plural no
    // português ("3 Caíram"); aqui é uma conta só ("Caiu").
    label: "account-dropped",
    tone: "text-red-400",
    mark: <WifiOff size={11} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />,
  },
  waiting: {
    label: "Not in a game yet",
    tone: "theme-muted",
    mark: <span aria-hidden="true" className="w-1.5 h-1.5 rounded-full bg-[var(--panel-muted)] shrink-0" />,
  },
};

export function AccountLiveSegment({ userId }: { userId: number }) {
  const t = useTr();
  const store = useStore();
  const live = useAccountLiveState(userId);
  const identity = useGameIdentity(live.placeId, userId);
  if (live.state === "none") return null;

  const style = STYLES[live.state];
  const word = t(style.label);
  const game = live.placeId ? (identity?.name ?? t("Place {{placeId}}", { placeId: live.placeId })) : null;
  // Caiu, travou ou reconecta sem jogo conhecido (a queda fecha a sessão): o
  // motivo ocupa o lugar do jogo — "Caiu" sozinho não diz o que aconteceu.
  const health =
    live.state === "dropped" || live.state === "notResponding" || live.state === "reconnecting"
      ? clientHealthLabel(store.clientHealth?.get(userId), t)
      : null;
  const reason = health && health.label !== word ? health.label : null;
  const secondary = game ?? reason;
  const tooltip = [word, game, reason, health?.detail].filter(Boolean).join(" — ");

  return (
    <div
      data-testid={`account-live-${userId}`}
      data-state={live.state}
      data-tour="accounts-live"
      title={tooltip}
      className="min-w-0 @[34rem]/row:w-[40%] min-[100rem]:w-[50%] flex items-center gap-1.5 text-[12px] leading-tight"
    >
      <span className={`shrink-0 inline-flex items-center gap-1 ${style.tone}`}>
        {style.mark}
        {word}
      </span>
      {secondary && (
        <>
          <span aria-hidden="true" className="hidden @[34rem]/row:inline theme-muted">
            ·
          </span>
          <span className="hidden @[34rem]/row:block min-w-0 truncate theme-muted">{secondary}</span>
        </>
      )}
      {live.state === "playing" && live.sinceMs !== null && (
        <span className="hidden @[46rem]/row:inline-flex shrink-0 items-center gap-1.5 theme-muted text-[11.5px]">
          <span aria-hidden="true">·</span>
          <SessionClock userId={userId} sinceMs={live.sinceMs} />
        </span>
      )}
    </div>
  );
}
