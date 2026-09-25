import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { TextField } from "../ui/TextField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { RestartBadge } from "../ui/RestartBadge";
import { useTr } from "../../i18n/text";

/** Espelha o piso em `api/server/middleware.rs`: abaixo disso tudo e 401. */
const MIN_PASSWORD_LENGTH = 6;

interface WebServerStatus {
  running: boolean;
  port: number;
}

export function WebServerTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const devMode = s.getBool("Developer", "DevMode");
  const wsEnabled = s.getBool("Developer", "EnableWebServer");
  const [status, setStatus] = useState<WebServerStatus>({ running: false, port: 0 });
  const password = s.get("WebServer", "Password", "");
  const [loading, setLoading] = useState(false);

  const refreshStatus = useCallback(() => {
    invoke<WebServerStatus>("get_web_server_status").then(setStatus).catch(() => {});
  }, []);

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 3000);
    return () => clearInterval(interval);
  }, [refreshStatus]);

  const toggleServer = async () => {
    setLoading(true);
    try {
      if (status.running) {
        await invoke("stop_web_server");
      } else {
        await invoke("start_web_server");
      }
      await new Promise((r) => setTimeout(r, 200));
      refreshStatus();
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  if (!devMode && !wsEnabled) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-center">
        <div className="w-10 h-10 rounded-xl bg-zinc-800/60 flex items-center justify-center mb-3">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="text-zinc-600">
            <rect x="2" y="6" width="20" height="12" rx="2" />
            <path d="M6 12h.01M10 12h.01" />
          </svg>
        </div>
        {/* A aba aparece sempre (SettingsDialog), entao este estado e a unica
            explicacao que o usuario recebe: o que o servidor faz e onde liga. */}
        <div className="text-sm text-zinc-400">{t("Web Server is off")}</div>
        <div className="mt-1.5 max-w-[340px] text-[11px] leading-relaxed text-zinc-500">
          {t(
            "It serves a local HTTP API so external tools and scripts can list your accounts, read their cookies and launch them."
          )}
        </div>
        <div className="mt-2 text-[11px] text-zinc-500">
          {t("Turn on Enable Web Server in the Developer tab to unlock these settings.")}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-0">
      <SectionLabel>Permissions</SectionLabel>
      {/* Cada interruptor abaixo e uma porta de entrada real: sem uma linha
          dizendo O QUE cada um expoe, a aba e uma lista de nomes de endpoint. */}
      <p className="px-1 pb-1 text-[11px] leading-relaxed text-zinc-500">
        {t("Each switch below opens part of the local HTTP API to anything that can reach the port.")}
      </p>
      <Toggle
        checked={s.getBool("WebServer", "EveryRequestRequiresPassword")}
        onChange={(v) => s.setBool("WebServer", "EveryRequestRequiresPassword", v)}
        label="Every Request Requires Password"
        // `external_check` so deixa `/Running` passar sem senha (middleware.rs).
        description="Every endpoint except /Running refuses to answer without the password in the URL."
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowGetCookie")}
        onChange={(v) => s.setBool("WebServer", "AllowGetCookie", v)}
        label="Allow GetCookie"
      />
      {/* `handle_get_cookie` devolve o `security_token` cru e
          `handle_get_accounts_json` embute o mesmo cookie com
          `IncludeCookies=true`: e a conta inteira saindo pela porta. Nao pode
          sair no mesmo cinza das outras dicas. */}
      <div className="mx-1 mb-2 rounded-lg border border-amber-500/25 bg-amber-500/5 px-3 py-2.5 text-[11px] leading-relaxed text-amber-200/80">
        <div>
          {t(
            "Hands out the .ROBLOSECURITY cookie of any account: whoever reads it is logged into that Roblox account, with no password and no 2FA."
          )}
        </div>
        <div className="mt-1 text-amber-200/60">
          {t(
            "Covers /GetCookie and GetAccountsJson with IncludeCookies=true; both always demand the web server password."
          )}
        </div>
      </div>
      <Toggle
        checked={s.getBool("WebServer", "AllowGetAccounts")}
        onChange={(v) => s.setBool("WebServer", "AllowGetAccounts", v)}
        label="Allow GetAccounts"
        // /GetAccounts, /GetAccountsJson, /GetAlias, /GetDescription, /GetField.
        description="Lists every account with username, user ID, alias, description, group and custom fields. Cookies are not included."
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowLaunchAccount")}
        onChange={(v) => s.setBool("WebServer", "AllowLaunchAccount", v)}
        label="Allow LaunchAccount"
        // Libera /LaunchAccount (place/job/VIP) e /FollowUser.
        description="Starts Roblox on any account and sends it into any place, server or after another player."
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowAccountEditing")}
        onChange={(v) => s.setBool("WebServer", "AllowAccountEditing", v)}
        label="Allow Account Editing"
        // /SetField, /RemoveField, /SetAlias, /SetDescription, /AppendDescription:
        // tudo isso e o dado que o app guarda, nao a conta no Roblox.
        description="Lets callers rewrite the alias, description and custom fields this app stores for an account."
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowExternalConnections")}
        onChange={(v) => s.setBool("WebServer", "AllowExternalConnections", v)}
        label={
          <>
            Allow External Connections<RestartBadge />
          </>
        }
        // `runtime::start` escolhe o bind: 0.0.0.0 ligado, 127.0.0.1 desligado.
        description="Binds the port to every network interface instead of localhost, so other machines can reach the API."
      />

      <Divider />
      <SectionLabel>Server</SectionLabel>

      <div className="flex items-center justify-between px-3 py-2">
        <div className="flex flex-col">
          <span className="text-[13px] text-zinc-300">
            {status.running ? t("Running on port {{port}}", { port: status.port }) : t("Not running")}
          </span>
        </div>
        <button
          onClick={toggleServer}
          disabled={loading}
          className={`px-3 py-1 rounded text-[12px] font-medium transition-colors ${
            status.running
              ? "bg-red-500/15 text-red-400 hover:bg-red-500/25"
              : "bg-emerald-500/15 text-emerald-400 hover:bg-emerald-500/25"
          } disabled:opacity-50`}
        >
          {loading ? "..." : status.running ? t("Stop") : t("Start")}
        </button>
      </div>

      <Divider />
      <SectionLabel>Connection</SectionLabel>

      <TextField
        value={password}
        onChange={(v) => s.set("WebServer", "Password", v)}
        label="Password"
        placeholder="alphanumeric only"
        pattern={/[^0-9a-zA-Z ]/g}
      />
      {/* O middleware recusa QUALQUER requisicao com senha curta
          (api/server/middleware.rs:53), entao o servidor "liga" e nao responde
          nada — sem isto aqui o usuario nao tem como saber por que. */}
      {password.length < MIN_PASSWORD_LENGTH && (
        <p className="px-1 pb-2 text-[11px] text-amber-400/90">
          {t("Too short: the server answers 401 to everything until it has 6 characters.")}
        </p>
      )}
      <NumberField
        value={s.getNumber("WebServer", "WebServerPort", 7963)}
        onChange={(v) => s.setNumber("WebServer", "WebServerPort", v)}
        label="Port"
        min={1}
        max={65535}
      />
    </div>
  );
}
