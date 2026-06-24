import { User, X } from "lucide-react";
import { useStore } from "../../store";
import type { Account } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { useTr } from "../../i18n/text";

function chipMaskName(name: string, previewLetters: number): string {
  if (previewLetters > 0 && previewLetters < name.length) {
    return name.slice(0, previewLetters) + "********";
  }
  return "************";
}

export function AccountChip({
  account,
  avatarUrl,
  onRemove,
}: {
  account: Account;
  avatarUrl?: string;
  onRemove: () => void;
}) {
  const t = useTr();
  const store = useStore();
  const rawName = account.Alias || account.Username;
  const displayName = store.hideUsernames ? chipMaskName(rawName, store.hiddenNameLetters) : rawName;
  const hideAvatar = store.hideUsernames && !store.showAvatarsWhenHidden;
  const showPresence = store.settings?.General?.ShowPresence === "true";
  const presenceType = store.presenceByUserId.get(account.UserID) ?? 0;
  const isJoining = store.joiningAccounts.has(account.UserID);
  const presenceDotClass =
    presenceType === 3 ? "bg-violet-500" : presenceType >= 2 ? "bg-emerald-500" : presenceType === 1 ? "bg-sky-500" : "";
  const presenceDotStyle = presenceType === 0 ? { backgroundColor: "var(--panel-muted)" } : undefined;

  return (
    <div className="theme-panel theme-border flex items-center gap-2 px-1.5 py-1 rounded-md border group/chip">
      {hideAvatar ? (
        <div className="theme-avatar w-5 h-5 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted shrink-0">
          <User size={10} strokeWidth={1.5} />
        </div>
      ) : avatarUrl ? (
        <img src={avatarUrl} alt="" className="theme-avatar w-5 h-5 rounded-full bg-[var(--panel-soft)] shrink-0" />
      ) : (
        <div className="theme-avatar w-5 h-5 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted text-[9px] font-medium shrink-0">
          {(account.Username || "?").charAt(0).toUpperCase()}
        </div>
      )}
      <span className="text-[12px] text-[var(--panel-fg)] truncate flex-1 min-w-0 inline-flex items-center gap-1.5">
        {showPresence && (
          <span
            className={`w-1.5 h-1.5 rounded-full shrink-0 ${presenceDotClass} ${presenceType >= 1 ? "animate-pulse" : ""}`}
            style={presenceDotStyle}
          />
        )}
        <span className="truncate">{displayName}</span>
      </span>
      {isJoining && (
        <Tooltip content={t("Joining...")}>
          <span
            className="w-2.5 h-2.5 border border-[var(--accent-color)] border-t-transparent rounded-full animate-spin shrink-0"
          />
        </Tooltip>
      )}
      <Tooltip content={t("Remove from selection")}>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="theme-muted hover:text-[var(--panel-fg)] opacity-0 group-hover/chip:opacity-100 transition-opacity shrink-0"
        >
          <X size={12} strokeWidth={2} />
        </button>
      </Tooltip>
    </div>
  );
}
