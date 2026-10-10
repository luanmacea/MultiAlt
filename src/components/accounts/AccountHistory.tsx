import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Download, LogIn } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useGameIdentity, loadGameIdentity } from "../../hooks/useGameIdentity";
import { useJoinOnlineWarning } from "../../hooks/useJoinOnlineWarning";
import type { Account, SessionRecord } from "../../types";
import {
  canJoinAgain,
  formatDuration,
  historyCsv,
  maskNamesInText,
  playtimeByGame,
  sessionDurationMs,
  sessionEndLabel,
} from "../../utils/sessionHistory";
import { SidebarSection } from "./SidebarSection";

const FIRST_ROWS = 6;
const TOP_GAMES = 5;
const DATE_LOCALES: Record<string, string> = { en: "en-US", pt: "pt-BR", es: "es-419", de: "de-DE" };

function GameName({ placeId, userId }: { placeId: number | null; userId: number }) {
  const t = useTr();
  const identity = useGameIdentity(placeId, userId);
  if (placeId === null) return <>{t("Unknown game")}</>;
  return <>{identity?.name ?? t("Place {{placeId}}", { placeId })}</>;
}

/** "Today 14:02", "Yesterday 14:02", "Oct 3, 14:02". */
function formatWhen(ms: number, now: number, t: (s: string, o?: Record<string, unknown>) => string, language: string): string {
  const d = new Date(ms);
  const time = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(new Date(now)) - day(d)) / 86_400_000);
  if (diff <= 0) return t("Today {{time}}", { time });
  if (diff === 1) return t("Yesterday {{time}}", { time });
  try {
    const date = new Intl.DateTimeFormat(DATE_LOCALES[language.slice(0, 2)] ?? language, {
      month: "short",
      day: "numeric",
    }).format(d);
    return `${date}, ${time}`;
  } catch {
    return `${d.toLocaleDateString()} ${time}`;
  }
}

/**
 * Histórico de sessões da conta (ideia 6), no painel da conta: tempo de jogo
 * dos últimos 14 dias por jogo, as últimas sessões com como terminaram,
 * "Join again" para o servidor que ainda pode existir e o export CSV.
 * Ver docs/features/history.md.
 */
export function AccountHistory({ account }: { account: Account }) {
  const t = useTr();
  const store = useStore();
  const { i18n } = useTranslation();
  const confirmJoinOnline = useJoinOnlineWarning();
  const userId = account.UserID;
  const [sessions, setSessions] = useState<SessionRecord[] | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [exporting, setExporting] = useState(false);

  const load = useCallback(() => {
    invoke<SessionRecord[]>("get_session_history", { userId })
      .then((list) => setSessions(Array.isArray(list) ? list : []))
      .catch(() => setSessions([]));
  }, [userId]);

  useEffect(() => {
    setSessions(null);
    setShowAll(false);
    load();
  }, [load]);

  // Gravou algo desta conta: relê. E o relógio anda para a duração de quem
  // está em jogo.
  useEffect(() => {
    const unlisten = listen<{ userIds?: number[] }>("session-history-changed", (e) => {
      if (e.payload?.userIds?.includes(userId)) load();
    });
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
      window.clearInterval(timer);
    };
  }, [userId, load]);

  const games = useMemo(() => playtimeByGame(sessions ?? [], now).slice(0, TOP_GAMES), [sessions, now]);
  const maxMs = games[0]?.ms ?? 0;
  const maskText = (text: string) => maskNamesInText(text, account, store);
  const rows = sessions ? (showAll ? sessions : sessions.slice(0, FIRST_ROWS)) : [];

  async function joinAgain(session: SessionRecord) {
    if (!session.placeId || !session.jobId) return;
    if (!(await confirmJoinOnline([userId]))) return;
    await store.joinServer(userId, { placeId: String(session.placeId), jobId: session.jobId });
  }

  async function exportCsv() {
    if (!sessions?.length || exporting) return;
    setExporting(true);
    try {
      const places = [...new Set(sessions.map((s) => s.placeId).filter((p): p is number => p !== null))];
      const names = new Map<number, string | null>();
      await Promise.all(
        places.map(async (placeId) => names.set(placeId, (await loadGameIdentity(placeId, userId)).name))
      );
      const csv = historyCsv(sessions, (placeId) => names.get(placeId) ?? null, t, Date.now(), maskText);
      await invoke<string>("save_history_export", { userId, csv });
      store.addToast(t("History saved to the exports folder"), "success");
    } catch (e) {
      store.addToast(t("Could not export: {{error}}", { error: String(e) }), "error");
    } finally {
      setExporting(false);
    }
  }

  return (
    <SidebarSection title="History">
      {sessions === null ? (
        <p className="text-[11px] theme-muted">{t("Loading...")}</p>
      ) : sessions.length === 0 ? (
        <p className="text-[11px] theme-muted" data-testid="history-empty">
          {t("Nothing yet. Games this account plays while MultiAlt is open show up here.")}
        </p>
      ) : (
        <div className="flex flex-col gap-3" data-testid="account-history">
          {games.length > 0 && (
            <div>
              <p className="text-[11px] theme-muted mb-1">{t("Time played, last 14 days")}</p>
              <ul className="flex flex-col gap-1.5" aria-label={t("Time played, last 14 days")}>
                {games.map((g) => (
                  <li key={g.placeId} className="text-[11px]" data-testid="playtime-row">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="truncate text-[var(--panel-fg)]">
                        <GameName placeId={g.placeId} userId={userId} />
                      </span>
                      <span className="shrink-0 tabular-nums theme-muted">{formatDuration(g.ms)}</span>
                    </div>
                    <div className="mt-0.5 h-1.5 rounded-full bg-[var(--panel-soft)] overflow-hidden">
                      <div
                        className="h-full rounded-full bg-[var(--accent-color)]"
                        style={{ width: `${maxMs > 0 ? Math.max(4, Math.round((g.ms / maxMs) * 100)) : 0}%` }}
                      />
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          <div>
            <p className="text-[11px] theme-muted mb-1">{t("Recent sessions")}</p>
            <ul className="flex flex-col divide-y divide-[var(--border-color)] rounded-lg border theme-border">
              {rows.map((s, i) => {
                const end = sessionEndLabel(s, t);
                const duration = sessionDurationMs(s, now);
                const tone =
                  end.tone === "drop" ? "text-red-400" : end.tone === "playing" ? "text-emerald-400" : "theme-muted";
                return (
                  <li key={`${s.startedAt}-${i}`} className="px-2 py-1.5 text-[11px]" data-testid="history-row">
                    <div className="flex items-start justify-between gap-1.5">
                      <span className="min-w-0 truncate text-[var(--panel-fg)] font-medium">
                        {s.end === "moderated" ? t("Moderation notice") : <GameName placeId={s.placeId} userId={userId} />}
                      </span>
                      {canJoinAgain(s, now) && (
                        <button
                          onClick={() => joinAgain(s)}
                          className="shrink-0 flex items-center gap-1 text-[11px] text-sky-400 hover:text-sky-300"
                          title={t("Opens this account in the same server, if it still exists")}
                        >
                          <LogIn size={11} strokeWidth={1.75} />
                          {t("Join again")}
                        </button>
                      )}
                    </div>
                    <div className="theme-muted tabular-nums">
                      {formatWhen(s.startedAt, now, t, i18n.language)}
                      {duration !== null && s.end !== "moderated" ? ` · ${formatDuration(duration)}` : ""}
                    </div>
                    <div className={`${tone} break-words`} title={end.detail || undefined}>
                      {maskText(end.label)}
                    </div>
                  </li>
                );
              })}
            </ul>
            {sessions.length > FIRST_ROWS && (
              <button onClick={() => setShowAll((v) => !v)} className="mt-1 text-[11px] theme-muted hover:text-[var(--panel-fg)]">
                {showAll ? t("Show less") : t("Show all ({{count}})", { count: sessions.length })}
              </button>
            )}
          </div>

          <button
            onClick={exportCsv}
            disabled={exporting}
            className="sidebar-btn-tool flex items-center justify-center gap-1.5 disabled:opacity-50"
          >
            <Download size={11} strokeWidth={1.75} />
            {exporting ? t("Exporting...") : t("Export CSV")}
          </button>
        </div>
      )}
    </SidebarSection>
  );
}
