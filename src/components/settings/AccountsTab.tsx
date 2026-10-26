import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { TextField } from "../ui/TextField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { ActionCard } from "./settingsParts";

/**
 * Settings › Accounts: como a lista mostra as contas e o navegador que faz o
 * login delas. Saiu de General (e o intervalo de presença, de Misc) em
 * 11/10/2026.
 */
export function AccountsTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const store = useStore();
  const [browserReady, setBrowserReady] = useState<boolean | null>(null);
  const browserDownload = store.browserDownload;
  const browserBusy = browserDownload?.active === true;

  useEffect(() => {
    let cancelled = false;
    void invoke<boolean>("is_browser_ready")
      .then((ready) => {
        if (!cancelled) setBrowserReady(ready);
      })
      .catch(() => {
        if (!cancelled) setBrowserReady(null);
      });
    return () => {
      cancelled = true;
    };
    // Reconsulta quando um download termina (sucesso ou erro) para não deixar
    // o botão preso em "Download" depois de uma instalação bem-sucedida.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [browserBusy]);

  const handleBrowserDownload = async () => {
    if (browserBusy) return;
    const ok = await store.ensureBrowserDownload(browserReady === true);
    if (ok) setBrowserReady(true);
  };

  return (
    <div className="space-y-0">
      <SectionLabel>Account list</SectionLabel>
      {/* Pedido do dono (12/10/2026): no lugar do "Hide Robux When Hidden". O
          saldo saiu do painel em julho; volta só com esta opção ligada. */}
      <Toggle
        checked={s.getBool("General", "ShowRobux")}
        onChange={(v) => s.setBool("General", "ShowRobux", v)}
        label="Show Robux"
        description="Show the account's Robux balance in its side panel. Hidden names hide it too."
      />
      <Toggle
        checked={s.getBool("General", "ShowPresence")}
        onChange={(v) => s.setBool("General", "ShowPresence", v)}
        label="Show Presence"
        description="Display online status for accounts in the list"
      />
      {/* O laco de presenca (store.tsx) so roda com `ShowPresence` ligado e o
          intervalo e `max(30s, minutos)`: o campo aceitava minutos sem dizer
          nem do que depende nem que existe um piso. */}
      <NumberField
        value={s.getNumber("General", "PresenceUpdateRate", 5)}
        onChange={(v) => s.setNumber("General", "PresenceUpdateRate", v)}
        label="Presence Refresh"
        description="How often the online status of every account is re-checked. Needs Show Presence on, and never runs faster than every 30 seconds."
        min={1}
        max={9999}
        suffix="min"
      />
      <Toggle
        checked={s.getBool("General", "WrapLongNames")}
        onChange={(v) => s.setBool("General", "WrapLongNames", v)}
        label="Wrap Long Names"
        description="Show long aliases and usernames in full instead of cutting them off"
      />
      <Toggle
        checked={s.getBool("General", "DisableAgingAlert")}
        onChange={(v) => s.setBool("General", "DisableAgingAlert", v)}
        label="Disable Aging Alert"
        description="Hide the freshness dots on accounts unused for 20+ days"
      />
      <Toggle
        checked={s.getBool("General", "DisableImages")}
        onChange={(v) => s.setBool("General", "DisableImages", v)}
        label="Disable Image Loading"
        description="Reduces memory usage by skipping avatar thumbnails"
      />
      <Toggle
        checked={s.getBool("General", "AutoCookieRefresh")}
        onChange={(v) => s.setBool("General", "AutoCookieRefresh", v)}
        label="Auto Cookie Refresh"
        description="Periodically refresh account cookies to prevent expiration"
      />

      <Divider />
      <SectionLabel>Login browser</SectionLabel>
      <Toggle
        checked={s.get("Login", "PersistentProfile", "true") !== "false"}
        onChange={(v) => s.setBool("Login", "PersistentProfile", v)}
        label="Persistent login profile"
        description="Reuse the login browser profile so the device builds trust over time, which cuts down on login captchas. Only the Roblox sign-in cookie is cleared before each login"
      />
      <Toggle
        checked={s.get("Login", "StealthMode", "true") !== "false"}
        onChange={(v) => s.setBool("Login", "StealthMode", v)}
        label="Reduce automation signals"
        description="Hide browser automation flags Roblox can detect during login. Helps lower captcha prompts but is not a complete solution"
      />
      <TextField
        value={s.get("Login", "ManualBinaryPath", "")}
        onChange={(v) => s.set("Login", "ManualBinaryPath", v)}
        label="Custom browser executable"
        // `{}` para o escape ser processado uma vez, como no placeholder de
        // Custom ClientSettings na seção Optimization.
        placeholder={"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"}
      />
      {/* `TextField` não tem `description`; o caminho vira `Login.ManualBinaryPath`
          e passa por `resolve_browser_binary` (chromium/download.rs), que exige
          um arquivo comum de verdade — nunca pasta, nunca atalho/link. */}
      <div className="px-1 -mt-1 mb-1 text-[12px] text-zinc-500">
        {t(
          "Use your own Chrome, Edge or Chromium instead of the downloaded copy. Must be a file you picked yourself, not a folder — leave empty to use the downloaded browser."
        )}
      </div>

      <ActionCard
        title={t("Bundled Browser")}
        description={
          browserReady === true
            ? t("Chrome for Testing is installed and used for browser logins")
            : browserReady === false
              ? t("Not installed yet. It downloads automatically on the first browser login, or you can download it now")
              : t("Used for browser logins and the account browser")
        }
        action={
          browserBusy
            ? t("Downloading...")
            : browserDownload?.stage === "error"
              ? t("Retry Download")
              : browserReady === true
                ? t("Reinstall")
                : t("Download")
        }
        onAction={() => {
          void handleBrowserDownload();
        }}
        disabled={browserBusy}
      >
        {browserBusy && (
          <div className="mt-2">
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
              <div
                className="h-full rounded-full bg-emerald-500/70 transition-all duration-300"
                style={{
                  width:
                    browserDownload?.stage === "downloading" && browserDownload.percent !== null
                      ? `${browserDownload.percent}%`
                      : "100%",
                }}
              />
            </div>
            <div className="mt-1 text-[11px] text-zinc-500">
              {browserDownload?.stage === "downloading" && browserDownload.percent !== null
                ? t("Downloading browser ({{percent}}%)", { percent: browserDownload.percent })
                : browserDownload?.stage === "extracting"
                  ? t("Preparing browser...")
                  : t("Contacting download service...")}
            </div>
          </div>
        )}
        {!browserBusy && browserDownload?.stage === "error" && browserDownload.error && (
          <div className="mt-2 text-[11px] leading-snug text-red-400">{browserDownload.error}</div>
        )}
      </ActionCard>
      {/* Um usuário relatou o navegador "não fazer nada" ao criar contas — era o
          Chrome do PC, usado quando o download falha. A checagem diz qual abre
          e se ele responde (chromium/diagnose.rs). */}
      <ActionCard
        title={t("Login browser does nothing?")}
        description={t(
          "Checks which browser logins and account creation use, whether it opens and answers, and whether the Roblox page loads. Downloads nothing."
        )}
        action={t("Run check")}
        onAction={() => store.setBrowserCheckOpen(true)}
      />
    </div>
  );
}
