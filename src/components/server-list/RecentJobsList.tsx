import { useState } from "react";
import { X } from "lucide-react";
import type { RecentJobEntry, RecentJobKind } from "./types";
import { loadRecentJobs, removeRecentJob, updateRecentJobs, visibleRecentJobs } from "./types";
import { useGameListsChanged } from "./gameListsSync";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";

export interface RecentJobsListProps {
  /** Conta selecionada: decide quais alvos privados aparecem. */
  userId: number | null;
  maxRecent: number;
  /** Preenche o campo de Job ID do launch com aquele servidor. */
  onSelect: (placeId: number | null, raw: string) => void;
}

function badge(kind: RecentJobKind, t: (s: string) => string): { label: string; className: string } {
  switch (kind) {
    case "vip":
      return { label: "VIP", className: "border-amber-500/30 bg-amber-500/10 text-amber-300" };
    case "link":
      return { label: t("Link"), className: "border-violet-500/30 bg-violet-500/10 text-violet-300" };
    default:
      return { label: t("Job"), className: "border-zinc-600/40 bg-zinc-700/20 text-zinc-400" };
  }
}

/** Job ID e link são longos demais para a linha; o meio é o que menos informa. */
function shortRaw(raw: string): string {
  if (raw.length <= 26) return raw;
  return `${raw.slice(0, 12)}…${raw.slice(-10)}`;
}

export function RecentJobsList({ userId, maxRecent, onSelect }: RecentJobsListProps) {
  const t = useTr();
  const confirm = useConfirm();
  const [entries, setEntries] = useState<RecentJobEntry[]>(loadRecentJobs);
  useGameListsChanged(() => setEntries(loadRecentJobs()));

  const shown = visibleRecentJobs(entries, userId);

  function formatTime(ts: number) {
    const diff = Date.now() - ts;
    const mins = Math.floor(diff / 60000);
    const hours = Math.floor(mins / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return t("{{count}}d ago", { count: days });
    if (hours > 0) return t("{{count}}h ago", { count: hours });
    if (mins > 0) return t("{{count}}m ago", { count: mins });
    return t("just now");
  }

  /**
   * Apaga **só o que esta conta vê**: entrada privada de outra conta não está
   * na tela, e apagar o que não se vê é surpresa, não limpeza.
   */
  async function handleClear() {
    const ok = await confirm(
      shown.length === 1
        ? t("Clear the recent servers list? Its 1 server is deleted from this computer and cannot be recovered.")
        : t(
            "Clear the recent servers list? Its {{count}} servers are deleted from this computer and cannot be recovered.",
            { count: shown.length }
          ),
      true
    );
    if (!ok) return;
    const dropped = new Set(shown.map((e) => e.raw));
    updateRecentJobs((list) => list.filter((e) => !dropped.has(e.raw)), { userDelete: true });
    setEntries(loadRecentJobs());
  }

  if (shown.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full py-8">
        <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" className="text-zinc-800 mb-3">
          <rect x="2" y="4" width="20" height="7" rx="2" />
          <rect x="2" y="13" width="20" height="7" rx="2" />
        </svg>
        <p className="text-xs text-zinc-700">{t("No recent servers")}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex items-center justify-between px-1 pb-2">
        <span className="text-[11px] text-zinc-600">
          {t("{{count}} of {{max}} max", { count: shown.length, max: maxRecent })}
        </span>
        <button
          onClick={() => {
            void handleClear();
          }}
          className="text-[11px] text-zinc-600 hover:text-red-400 transition-colors"
        >
          {t("Clear all")}
        </button>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="flex flex-col gap-1 px-1">
          {shown.map((entry) => (
            <div
              key={entry.raw}
              className="group flex items-center gap-2 p-2 rounded-lg hover:bg-zinc-800/40 transition-colors"
            >
              <button
                type="button"
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
                onClick={() => onSelect(entry.placeId, entry.raw)}
                title={entry.raw}
              >
                <span
                  className={`shrink-0 rounded border px-1.5 py-0.5 text-[11px] font-medium ${badge(entry.kind, t).className}`}
                >
                  {badge(entry.kind, t).label}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-mono text-[12px] text-zinc-200">
                    {shortRaw(entry.raw)}
                  </span>
                  {entry.placeId ? (
                    <span className="block truncate text-[11px] text-zinc-600 font-mono">
                      {t("ID: {{id}}", { id: entry.placeId })}
                    </span>
                  ) : null}
                </span>
              </button>
              <span className="shrink-0 text-[11px] text-zinc-600 group-hover:hidden">
                {formatTime(entry.lastUsed)}
              </span>
              <button
                type="button"
                onClick={() => {
                  removeRecentJob(entry.raw);
                  setEntries(loadRecentJobs());
                }}
                className="hidden shrink-0 p-1 rounded text-zinc-500 hover:text-red-400 group-hover:block"
                aria-label={t("Remove")}
              >
                <X size={12} strokeWidth={1.75} />
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
