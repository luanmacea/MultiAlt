import { BadgeCheck, Loader2, Lock, UsersRound } from "lucide-react";
import { useTr } from "../../../i18n/text";
import type { GroupSummary } from "./shared";

/** Ícone do grupo: `undefined` = carregando, `null` = sem imagem. */
export function GroupIcon({ url, size = 40 }: { url: string | null | undefined; size?: number }) {
  return (
    <span
      className="shrink-0 rounded-lg overflow-hidden bg-[var(--panel-soft)] flex items-center justify-center theme-muted"
      style={{ width: size, height: size }}
    >
      {url ? (
        <img src={url} alt="" className="w-full h-full object-cover" draggable={false} />
      ) : url === undefined ? (
        <span className="w-full h-full animate-pulse bg-[var(--panel-soft)]" />
      ) : (
        <UsersRound size={Math.round(size * 0.5)} strokeWidth={1.5} />
      )}
    </span>
  );
}

/** "Entra direto" ou "precisa de aprovação" — é o que decide o "pendente". */
export function EntryChip({ group }: { group: GroupSummary }) {
  const t = useTr();
  if (group.isLocked) {
    return (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full border text-[11px] border-red-500/30 bg-red-500/15 text-red-300">
        <Lock size={10} aria-hidden="true" />
        {t("Locked")}
      </span>
    );
  }
  return group.publicEntryAllowed ? (
    <span className="px-1.5 py-0.5 rounded-full border text-[11px] border-emerald-500/30 bg-emerald-500/15 text-emerald-300">
      {t("Open to join")}
    </span>
  ) : (
    <span className="px-1.5 py-0.5 rounded-full border text-[11px] border-violet-500/30 bg-violet-500/15 text-violet-300">
      {t("Approval required")}
    </span>
  );
}

export function VerifiedBadge() {
  const t = useTr();
  return (
    <BadgeCheck
      size={13}
      strokeWidth={2}
      role="img"
      aria-label={t("Verified")}
      className="shrink-0 text-sky-400"
    />
  );
}

/**
 * Os cartões de grupo. Clicar escolhe o grupo para a entrada lá embaixo — o
 * mesmo para a busca e para os "Grupos populares".
 */
export function GroupCards({
  groups,
  icons,
  selectedId,
  onSelect,
  label,
}: {
  groups: GroupSummary[];
  icons: Map<number, string | null>;
  selectedId: number | null;
  onSelect: (group: GroupSummary) => void;
  /** Nome da lista para leitor de tela (já traduzido). */
  label: string;
}) {
  const t = useTr();
  const formatter = new Intl.NumberFormat();
  return (
    <ul aria-label={label} className="grid grid-cols-1 gap-2 @xl:grid-cols-2 @4xl:grid-cols-3">
      {groups.map((group) => {
        const selected = group.id === selectedId;
        return (
          <li key={group.id}>
            <button
              type="button"
              aria-pressed={selected}
              onClick={() => onSelect(group)}
              className={`w-full h-full flex items-start gap-3 rounded-xl border p-2.5 text-left transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
                selected
                  ? "border-[var(--accent-color)] bg-[var(--accent-soft)]"
                  : "theme-border hover:bg-[var(--panel-soft)]"
              }`}
            >
              <GroupIcon url={icons.get(group.id)} />
              <span className="flex-1 min-w-0">
                <span className="flex items-center gap-1 min-w-0">
                  <span className="truncate text-[12.5px] font-medium text-[var(--panel-fg)]" title={group.name}>
                    {group.name}
                  </span>
                  {group.hasVerifiedBadge ? <VerifiedBadge /> : null}
                </span>
                <span className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1">
                  <span className="text-[11px] theme-muted tabular-nums">
                    {t("{{members}} members", { members: formatter.format(group.memberCount) })}
                  </span>
                  <EntryChip group={group} />
                </span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** Resultado da busca: carregando, erro, nada achado ou os cartões. */
export function GroupResults({
  groups,
  icons,
  selectedId,
  onSelect,
  loading,
  error,
  canLoadMore,
  loadingMore,
  onLoadMore,
}: {
  groups: GroupSummary[];
  icons: Map<number, string | null>;
  selectedId: number | null;
  onSelect: (group: GroupSummary) => void;
  loading: boolean;
  error: string | null;
  canLoadMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
}) {
  const t = useTr();

  if (loading) {
    return (
      <div className="flex items-center gap-2 py-6 justify-center text-[12px] theme-muted">
        <Loader2 size={14} className="animate-spin" aria-hidden="true" />
        {t("Searching groups...")}
      </div>
    );
  }
  if (error) {
    return (
      <div role="alert" className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-[12px] text-red-300 break-words">
        {error}
      </div>
    );
  }
  if (groups.length === 0) {
    return <div className="py-4 text-[12px] theme-muted">{t("No groups found.")}</div>;
  }

  return (
    <div>
      <GroupCards groups={groups} icons={icons} selectedId={selectedId} onSelect={onSelect} label={t("Groups found")} />
      {canLoadMore ? (
        <div className="mt-3 flex justify-center">
          <button
            type="button"
            onClick={onLoadMore}
            disabled={loadingMore}
            className="sidebar-btn-sm px-3 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {loadingMore ? t("Loading...") : t("Load more")}
          </button>
        </div>
      ) : null}
    </div>
  );
}
