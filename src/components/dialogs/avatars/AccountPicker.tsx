import { Check, Users } from "lucide-react";
import { useTr } from "../../../i18n/text";
import type { Account } from "../../../types";
import { Headshot } from "./shared";

/**
 * Quais contas recebem avatar. Nasce com a seleção da lista principal ao abrir
 * o diálogo, mas dá para marcar e desmarcar aqui mesmo — antes era preciso
 * fechar o diálogo e selecionar na lista.
 */
export function AccountPicker({
  accounts,
  avatarUrls,
  picked,
  onChange,
  disabled,
}: {
  accounts: Account[];
  avatarUrls: Map<number, string>;
  picked: ReadonlySet<number>;
  onChange: (next: Set<number>) => void;
  disabled: boolean;
}) {
  const t = useTr();
  const count = accounts.filter((a) => picked.has(a.UserID)).length;

  function toggle(userId: number) {
    const next = new Set(picked);
    if (next.has(userId)) next.delete(userId);
    else next.add(userId);
    onChange(next);
  }

  return (
    <>
      <div className="px-3 py-2.5 border-b theme-border shrink-0 flex items-center justify-between gap-2">
        <div className="min-w-0 text-[12.5px] font-semibold text-[var(--panel-fg)] truncate">
          {t("Accounts")}
          <span className="ml-1.5 text-[11px] font-normal theme-muted tabular-nums">
            {t("{{count}} of {{total}}", { count, total: accounts.length })}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            onClick={() => onChange(new Set(accounts.map((a) => a.UserID)))}
            disabled={disabled || accounts.length === 0}
            className="px-2 py-0.5 rounded-md text-[11px] theme-btn-ghost disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t("Select all")}
          </button>
          <button
            onClick={() => onChange(new Set())}
            disabled={disabled || count === 0}
            className="px-2 py-0.5 rounded-md text-[11px] theme-btn-ghost disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t("Select none")}
          </button>
        </div>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto p-1.5">
        {accounts.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center gap-2 text-center px-5">
            <Users size={26} strokeWidth={1.25} className="theme-muted" />
            <div className="text-[11.5px] theme-muted leading-4">{t("No accounts yet — add accounts first.")}</div>
          </div>
        ) : (
          <div className="space-y-px">
            {accounts.map((account) => {
              const on = picked.has(account.UserID);
              const name = account.Alias || account.Username;
              return (
                <button
                  key={account.UserID}
                  role="checkbox"
                  aria-checked={on}
                  aria-label={name}
                  disabled={disabled}
                  onClick={() => toggle(account.UserID)}
                  className={`w-full flex items-center gap-2 px-1.5 py-1 rounded-md text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
                    on ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--panel-soft)]"
                  }`}
                >
                  <span
                    className={`w-4 h-4 shrink-0 rounded border flex items-center justify-center ${
                      on
                        ? "bg-[var(--accent-color)] border-[var(--accent-color)] text-[var(--panel-bg)]"
                        : "theme-border"
                    }`}
                  >
                    {on ? <Check size={11} strokeWidth={3} /> : null}
                  </span>
                  <Headshot url={avatarUrls.get(account.UserID)} size={22} />
                  <span className="flex-1 min-w-0">
                    <span className="block text-[12px] text-[var(--panel-fg)] truncate">{name}</span>
                    {account.Alias ? (
                      <span className="block text-[11px] theme-muted truncate">{account.Username}</span>
                    ) : null}
                  </span>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}
