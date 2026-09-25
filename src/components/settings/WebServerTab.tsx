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
      <Toggle
        checked={s.getBool("WebServer", "EveryRequestRequiresPassword")}
        onChange={(v) => s.setBool("WebServer", "EveryRequestRequiresPassword", v)}
        label="Every Request Requires Password"
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowGetCookie")}
        onChange={(v) => s.setBool("WebServer", "AllowGetCookie", v)}
        label="Allow GetCookie"
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowGetAccounts")}
        onChange={(v) => s.setBool("WebServer", "AllowGetAccounts", v)}
        label="Allow GetAccounts"
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowLaunchAccount")}
        onChange={(v) => s.setBool("WebServer", "AllowLaunchAccount", v)}
        label="Allow LaunchAccount"
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowAccountEditing")}
        onChange={(v) => s.setBool("WebServer", "AllowAccountEditing", v)}
        label="Allow Account Editing"
      />
      <Toggle
        checked={s.getBool("WebServer", "AllowExternalConnections")}
        onChange={(v) => s.setBool("WebServer", "AllowExternalConnections", v)}
        label={
          <>
            Allow External Connections<RestartBadge />
          </>
        }
        description="Accept connections from other devices. Requires admin privileges."
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
