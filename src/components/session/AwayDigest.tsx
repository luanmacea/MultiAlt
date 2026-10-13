import { useState } from "react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { awayEventText, awayEventTime, type AwayEvent } from "../../utils/awayDigest";

/** Quantos eventos o cartão mostra antes do "Show all". */
const COLLAPSED_COUNT = 5;

const TONE_DOT = {
  problem: "bg-amber-400",
  recovered: "bg-emerald-400",
} as const;

/** Nome da conta do evento, com "Names hidden" aplicado; vazio sem conta. */
function useEventAccountName(): (event: AwayEvent) => string {
  const store = useStore();
  const label = useAccountLabel();
  return (event) => {
    if (event.userId === null) return "";
    return label(
      store.accounts.find((a) => a.UserID === event.userId),
      event.userId
    );
  };
}

/**
 * Cartão "While you were away" da página Session: os últimos eventos que
 * importam (quedas, reconexões, janela travada, falhas do Modo AFK, parada de
 * emergência), do mais novo para o mais antigo, com a hora e a conta. Os
 * toasts desses fatos somem em segundos; quem volta ao PC lê aqui.
 *
 * Abrir a página já conta como ter olhado (a store zera o "não lido"); o
 * clique no cartão também, para o que chegou com a página aberta. Sem nada a
 * contar, uma linha só — o cartão fica, para a pessoa saber que existe.
 */
export function AwayDigestCard() {
  const t = useTr();
  const store = useStore();
  const nameOf = useEventAccountName();
  const [expanded, setExpanded] = useState(false);
  const events = store.awayEvents ?? [];
  const unread = store.awayUnread ?? 0;
  const shown = expanded ? events : events.slice(0, COLLAPSED_COUNT);

  return (
    <section
      aria-label={t("While you were away")}
      data-tour="session-away"
      data-testid="away-digest"
      onClick={() => {
        if (unread > 0) store.markAwaySeen();
      }}
      className="rounded-xl border theme-border px-4 py-3"
    >
      <div className="flex items-center gap-2">
        <h2 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("While you were away")}</h2>
        {unread > 0 ? (
          <span
            data-testid="away-unread"
            className="rounded-full bg-amber-500/15 px-1.5 py-px text-[11px] font-medium text-amber-300"
          >
            {unread === 1 ? t("1 new") : t("{{count}} new", { count: unread })}
          </span>
        ) : null}
        {events.length > 0 ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setExpanded(false);
              store.clearAwayEvents();
            }}
            className="ml-auto text-[11px] theme-muted hover:text-[var(--panel-fg)] transition-colors"
          >
            {t("Clear")}
          </button>
        ) : null}
      </div>

      {events.length === 0 ? (
        <p className="mt-1.5 text-[12px] leading-snug text-[var(--panel-muted)]">{t("Nothing happened yet.")}</p>
      ) : (
        <ol className={`mt-2 flex flex-col gap-1 ${expanded ? "max-h-72 overflow-y-auto pr-1" : ""}`}>
          {shown.map((event) => {
            const { text, tone } = awayEventText(event, t);
            const name = nameOf(event);
            const full = name ? `${awayEventTime(event.atMs)} ${name} — ${text}` : `${awayEventTime(event.atMs)} ${text}`;
            return (
              <li
                key={event.id}
                data-testid="away-event"
                data-tone={tone}
                title={full}
                className="flex min-w-0 items-baseline gap-2 text-[12px] leading-snug"
              >
                <span aria-hidden="true" className={`h-1.5 w-1.5 shrink-0 -translate-y-px rounded-full ${TONE_DOT[tone]}`} />
                <time
                  dateTime={new Date(event.atMs).toISOString()}
                  className="shrink-0 tabular-nums text-[11px] text-[var(--panel-muted)]"
                >
                  {awayEventTime(event.atMs)}
                </time>
                {/* Duas linhas no máximo: a coluna da Session é estreita, e o
                    motivo da queda é o que interessa ler. */}
                <span className="min-w-0 line-clamp-2 break-words text-[var(--panel-fg)]">
                  {name ? <span className="font-medium">{name}</span> : null}
                  {name ? <span className="text-[var(--panel-muted)]"> · </span> : null}
                  {text}
                </span>
              </li>
            );
          })}
        </ol>
      )}

      {events.length > COLLAPSED_COUNT ? (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="mt-2 text-[11.5px] font-medium theme-accent hover:underline"
        >
          {expanded ? t("Show less") : t("Show all ({{count}})", { count: events.length })}
        </button>
      ) : null}
    </section>
  );
}

/**
 * A linha do Modo AFK: "3 events while you were away · View". Só aparece com
 * algo novo desde a última olhada; o "View" leva à página Session, que tem a
 * lista. No modal, `onNavigate` fecha o modal antes.
 */
export function AwayDigestLine({ onNavigate }: { onNavigate?: () => void }) {
  const t = useTr();
  const store = useStore();
  const nameOf = useEventAccountName();
  const unread = store.awayUnread ?? 0;
  const latest = (store.awayEvents ?? [])[0];
  if (unread === 0 || !latest) return null;
  const unreadEvents = (store.awayEvents ?? []).slice(0, unread);
  const anyProblem = unreadEvents.some((event) => awayEventText(event, t).tone === "problem");
  const { text } = awayEventText(latest, t);
  const name = nameOf(latest);

  return (
    <div
      data-testid="away-digest-line"
      role="status"
      className="flex shrink-0 min-w-0 items-center gap-2 rounded-lg border theme-border bg-[var(--panel-soft)] px-3 py-1.5 text-[12px]"
    >
      <span
        aria-hidden="true"
        className={`h-1.5 w-1.5 shrink-0 rounded-full ${anyProblem ? TONE_DOT.problem : TONE_DOT.recovered}`}
      />
      <span className="shrink-0 font-medium text-[var(--panel-fg)]">
        {unread === 1 ? t("1 event while you were away") : t("{{count}} events while you were away", { count: unread })}
      </span>
      <span className="min-w-0 truncate theme-muted" title={name ? `${name} — ${text}` : text}>
        {awayEventTime(latest.atMs)} {name ? `${name} · ` : ""}
        {text}
      </span>
      <button
        type="button"
        onClick={() => {
          onNavigate?.();
          store.setActivePage("session");
        }}
        className="ml-auto shrink-0 font-medium theme-accent hover:underline"
      >
        {t("View")}
      </button>
    </div>
  );
}
