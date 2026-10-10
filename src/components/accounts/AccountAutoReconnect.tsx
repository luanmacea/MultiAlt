import { useEffect, useState } from "react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { SidebarSection } from "./SidebarSection";
import { isWindowsPlatform } from "../../utils/platform";
import { autoReconnectLabel } from "../../utils/autoReconnect";
import type { Account } from "../../types";

/** Campo da conta lido por `commands/reconnect.rs` (`"true"`/`"false"`; sem ele, o padrão). */
export const AUTO_RECONNECT_FIELD = "AutoReconnect";

/**
 * Reconexão automática desta conta. O campo da conta manda; sem ele vale o
 * padrão de Settings › General (`General.AutoReconnect`). Só no Windows, onde
 * o log do Roblox é lido (client_health.rs).
 */
export function AccountAutoReconnect({ account }: { account: Account }) {
  const t = useTr();
  const store = useStore();
  const entry = (store.autoReconnect ?? []).find((e) => e.userId === account.UserID);
  // A contagem ("em 30 s") anda só enquanto esta conta espera a tentativa.
  const [now, setNow] = useState(() => Date.now());
  const countingDown = entry?.phase === "waiting";
  useEffect(() => {
    if (!countingDown) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [countingDown]);
  if (!isWindowsPlatform(store.platformCapabilities)) return null;

  const field = account.Fields?.[AUTO_RECONNECT_FIELD];
  const own = field === "true" ? true : field === "false" ? false : null;
  const globalDefault = store.settings?.General?.AutoReconnect === "true";
  const checked = own ?? globalDefault;
  const status = entry ? autoReconnectLabel(entry, now, t) : null;

  function save(value: boolean | null) {
    const fields = { ...(account.Fields || {}) };
    if (value === null) delete fields[AUTO_RECONNECT_FIELD];
    else fields[AUTO_RECONNECT_FIELD] = value ? "true" : "false";
    void store.updateAccount({ ...account, Fields: fields });
  }

  return (
    <SidebarSection title={t("Auto-reconnect")}>
      <label className="flex items-center gap-2 text-[12px] cursor-pointer select-none">
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => save(e.target.checked)}
          className="accent-[var(--accent-color)]"
        />
        <span className="text-[var(--panel-fg)]">{t("Reconnect automatically if it drops")}</span>
      </label>
      <p className="text-[11px] theme-muted mt-1">
        {t(
          "Reopens it in the same game after a lost connection, a kick or a crash. Never when you close it or the account joins somewhere else."
        )}
      </p>
      {own === null ? (
        <p className="text-[11px] theme-muted mt-1">{t("Following the default in Settings › General.")}</p>
      ) : (
        <button
          onClick={() => save(null)}
          className="mt-1 text-[11px] text-sky-400 hover:text-sky-300 self-start"
        >
          {t("Use the default from Settings › General")}
        </button>
      )}
      {status && (
        <p
          data-testid="account-reconnect-status"
          className={`text-[11px] mt-1 ${status.tone === "pending" ? "text-amber-400" : "text-red-400"}`}
          title={status.detail || undefined}
        >
          {status.label}
        </p>
      )}
    </SidebarSection>
  );
}
