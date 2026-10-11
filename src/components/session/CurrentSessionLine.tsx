import { useEffect, useState } from "react";
import { Clock, Lock } from "lucide-react";
import { useTr } from "../../i18n/text";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import { formatDuration } from "../../utils/sessionHistory";
import type { CurrentSession } from "../../types";

/**
 * Tempo em jogo, andando sozinho ("45s", "12m", "1h 05m"). O relógio é da
 * linha, não do painel: o resto da lista não redesenha a cada segundo.
 */
function SessionClock({ userId, sinceMs }: { userId: number; sinceMs: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [sinceMs]);
  return (
    <span
      data-testid={`session-time-${userId}`}
      className="shrink-0 inline-flex items-center gap-1 tabular-nums"
    >
      <Clock size={10} strokeWidth={1.75} aria-hidden="true" />
      {formatDuration(now - sinceMs)}
    </span>
  );
}

/**
 * Segunda linha de uma conta no "Em jogo": o jogo (nome resolvido como nas
 * outras telas; sem nome, o Place ID), se o servidor é público ou privado
 * (quando se sabe) e há quanto tempo está nele. Nome longo é cortado, com o
 * texto inteiro no tooltip.
 */
export function CurrentSessionLine({ session }: { session: CurrentSession }) {
  const t = useTr();
  const identity = useGameIdentity(session.placeId, session.userId);
  const game = identity?.name ?? t("Place {{placeId}}", { placeId: session.placeId });
  const server =
    session.privateServer === true
      ? t("Private server")
      : session.privateServer === false
        ? t("Public server")
        : null;
  return (
    <span className="flex items-center gap-1.5 min-w-0 text-[11px] theme-muted">
      <span className="truncate min-w-0" title={game}>
        {game}
      </span>
      {server && (
        <>
          <span aria-hidden="true">·</span>
          <span className="shrink-0 inline-flex items-center gap-1">
            {session.privateServer && <Lock size={10} strokeWidth={1.75} aria-hidden="true" />}
            {server}
          </span>
        </>
      )}
      <span aria-hidden="true">·</span>
      <SessionClock userId={session.userId} sinceMs={session.sinceMs} />
    </span>
  );
}
