import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { SidebarSection } from "./SidebarSection";
import { isWindowsPlatform } from "../../utils/platform";
import { autoReconnectLabel } from "../../utils/autoReconnect";
import type { Account } from "../../types";

/** Campo da conta lido por `commands/reconnect.rs` (`"true"`/`"false"`; sem ele, o padrão). */
export const AUTO_RECONNECT_FIELD = "AutoReconnect";

/**
 * O AutoRelaunch do Nexus está ligado para esta conta? `reconnect_enabled`
 * (commands/reconnect.rs) liga a reconexão com ele mesmo com o campo da conta
 * em "false" — o painel tem de dizer isso. Mesmo critério do backend: nome de
 * usuário sem diferença de maiúsculas. Sem o Nexus no build, a lista vem vazia.
 */
function useNexusAutoRelaunch(username: string, enabled: boolean): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    const name = username.toLowerCase();
    invoke<{ username?: string; auto_relaunch?: boolean }[]>("get_nexus_accounts")
      .then((accounts) => {
        if (cancelled) return;
        const list = Array.isArray(accounts) ? accounts : [];
        setOn(list.some((a) => a?.auto_relaunch === true && (a.username ?? "").toLowerCase() === name));
      })
      .catch(() => {
        if (!cancelled) setOn(false);
      });
    return () => {
      cancelled = true;
    };
  }, [username, enabled]);
  return enabled && on;
}

/**
 * Reconexão automática desta conta. O campo da conta manda; sem ele vale o
 * padrão de Settings › General (`General.AutoReconnect`). O AutoRelaunch do
 * Nexus liga por cima dos dois. Só no Windows, onde o log do Roblox é lido
 * (client_health.rs).
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
  const isWindows = isWindowsPlatform(store.platformCapabilities);
  const nexusForced = useNexusAutoRelaunch(account.Username ?? "", isWindows);
  if (!isWindows) return null;

  const field = account.Fields?.[AUTO_RECONNECT_FIELD];
  const own = field === "true" ? true : field === "false" ? false : null;
  const globalDefault = store.settings?.General?.AutoReconnect === "true";
  const chosen = own ?? globalDefault;
  const status = entry ? autoReconnectLabel(entry, now, t) : null;
  // Desligada com a tentativa em andamento: o backend só a solta depois dela.
  const stillRunning = !chosen && !nexusForced && status?.tone === "pending";

  function save(value: boolean | null) {
    const fields = { ...(account.Fields || {}) };
    if (value === null) delete fields[AUTO_RECONNECT_FIELD];
    else fields[AUTO_RECONNECT_FIELD] = value ? "true" : "false";
    void store.updateAccount({ ...account, Fields: fields });
  }

  return (
    <SidebarSection title={t("Auto-reconnect")}>
      <label
        className={`flex items-center gap-2 text-[12px] select-none ${nexusForced ? "opacity-70" : "cursor-pointer"}`}
      >
        <input
          type="checkbox"
          checked={chosen || nexusForced}
          disabled={nexusForced}
          onChange={(e) => save(e.target.checked)}
          className="accent-[var(--accent-color)]"
        />
        <span className="text-[var(--panel-fg)]">{t("Reconnect automatically if it drops")}</span>
      </label>
      <p className="text-[11px] theme-muted mt-1">
        {t(
          "Reopens it in the same game after a lost connection, a kick or a crash. Never when you close it, when it joins somewhere else, or for windows opened from the website."
        )}
      </p>
      {nexusForced ? (
        <p className="text-[11px] text-amber-400 mt-1">
          {t("Nexus AutoRelaunch is on for this account, so it reconnects even with this off.")}
        </p>
      ) : own === null ? (
        <p className="text-[11px] theme-muted mt-1">
          {globalDefault
            ? t("Following the default from Settings › General (on).")
            : t("Following the default from Settings › General (off).")}
        </p>
      ) : (
        <button
          onClick={() => save(null)}
          className="mt-1 text-[11px] text-sky-400 hover:text-sky-300 self-start"
        >
          {globalDefault
            ? t("Use the default from Settings › General (on)")
            : t("Use the default from Settings › General (off)")}
        </button>
      )}
      {status && (
        <>
          <p
            data-testid="account-reconnect-status"
            className={`text-[11px] mt-1 ${status.tone === "pending" ? "text-amber-400" : "text-red-400"}`}
          >
            {status.label}
          </p>
          {status.detail && (
            <p data-testid="account-reconnect-detail" className="text-[11px] theme-muted">
              {status.detail}
            </p>
          )}
          {stillRunning && (
            <p className="text-[11px] theme-muted">{t("Turned off: it stops after the attempt in progress.")}</p>
          )}
        </>
      )}
    </SidebarSection>
  );
}
