import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle } from "lucide-react";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { useTr } from "../../i18n/text";
import { MAX_WEBHOOK_MENTION_CHARS, webhookUrlProblem } from "../../utils/discordWebhook";

/**
 * Settings › Notifications: avisos no Discord por webhook (ideia 5).
 *
 * Desligado por padrão — os avisos saem do PC. A tela só grava as opções da
 * seção `[Webhook]`; quem confere a URL de novo, monta a mensagem e manda é o
 * backend (commands/webhook.rs, api/discord_webhook.rs). Ver
 * docs/features/webhook.md.
 */
export function NotificationsTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const enabled = s.getBool("Webhook", "Enabled");
  const savedUrl = s.get("Webhook", "Url", "");
  const [url, setUrl] = useState(savedUrl);
  const [mention, setMention] = useState(s.get("Webhook", "Mention", ""));
  const [urlError, setUrlError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  // As settings chegam depois de montar: o campo pega o valor salvo.
  useEffect(() => setUrl(savedUrl), [savedUrl]);
  const savedMention = s.get("Webhook", "Mention", "");
  useEffect(() => setMention(savedMention), [savedMention]);

  // Caixas com padrão "ligado": chave ausente conta como ligada.
  const flag = (key: string, defaultOn: boolean) => {
    const value = s.get("Webhook", key, defaultOn ? "true" : "false");
    return value === "true";
  };

  function saveUrl(value: string) {
    const problem = webhookUrlProblem(value);
    setUrlError(problem);
    if (problem) return;
    if (value.trim() !== savedUrl) s.set("Webhook", "Url", value.trim());
  }

  async function sendTest() {
    const problem = webhookUrlProblem(url) ?? (url.trim() ? null : "Paste the webhook URL first.");
    if (problem) {
      setUrlError(problem);
      return;
    }
    setTesting(true);
    setTestResult(null);
    try {
      await invoke("send_test_webhook", { url: url.trim(), mention: mention.trim() });
      setTestResult({ ok: true, text: t("Test message sent. Check the Discord channel.") });
    } catch (e) {
      setTestResult({ ok: false, text: t(String(e)) });
    } finally {
      setTesting(false);
    }
  }

  const inputClass =
    "w-full min-w-0 px-2.5 py-1 rounded-md text-[13px] placeholder-zinc-600 bg-zinc-800/60 border border-zinc-700/60 text-zinc-200 focus:outline-none focus:border-sky-500/40";

  return (
    <div className="space-y-0">
      <Toggle
        checked={enabled}
        onChange={(v) => s.setBool("Webhook", "Enabled", v)}
        label="Send notifications to Discord"
        description="Off by default. When on, MultiAlt posts the events you pick below to your Discord webhook. This information leaves your PC."
      />

      <div className="px-1 py-2 space-y-1.5">
        <label htmlFor="webhook-url" className="block text-[13px] text-zinc-300">
          {t("Webhook URL")}
        </label>
        <div className="flex items-center gap-2">
          <input
            id="webhook-url"
            type="password"
            value={url}
            onChange={(e) => {
              setUrl(e.target.value);
              setUrlError(null);
              setTestResult(null);
            }}
            onBlur={(e) => saveUrl(e.target.value)}
            placeholder="https://discord.com/api/webhooks/..."
            autoComplete="off"
            spellCheck={false}
            aria-invalid={urlError ? true : undefined}
            aria-describedby={urlError ? "webhook-url-error" : "webhook-url-note"}
            className={inputClass}
          />
          <button
            type="button"
            onClick={() => void sendTest()}
            disabled={testing}
            className="shrink-0 rounded-lg border border-zinc-700/70 bg-zinc-800 px-3 py-1.5 text-[12px] font-medium text-zinc-200 transition-colors hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {testing ? t("Sending...") : t("Send test message")}
          </button>
        </div>
        {urlError ? (
          <p id="webhook-url-error" role="alert" className="flex items-start gap-1.5 text-[12px] text-red-400">
            <AlertTriangle size={12} strokeWidth={1.75} className="shrink-0 mt-[2px]" aria-hidden="true" />
            <span>{t(urlError)}</span>
          </p>
        ) : (
          <p id="webhook-url-note" className="text-[12px] text-zinc-500">
            {t("Anyone with this URL can post in that channel. It is saved in RAMSettings.ini as plain text, and never written to the logs.")}
          </p>
        )}
        {testResult && (
          <p role="status" className={`text-[12px] ${testResult.ok ? "text-emerald-400" : "text-red-400"}`}>
            {testResult.text}
          </p>
        )}
      </div>

      <div className="px-1 py-2 space-y-1.5">
        <label htmlFor="webhook-mention" className="block text-[13px] text-zinc-300">
          {t("Text before each message (optional)")}
        </label>
        <input
          id="webhook-mention"
          value={mention}
          maxLength={MAX_WEBHOOK_MENTION_CHARS}
          onChange={(e) => setMention(e.target.value)}
          onBlur={(e) => {
            if (e.target.value.trim() !== savedMention) s.set("Webhook", "Mention", e.target.value.trim());
          }}
          placeholder={t("e.g. @everyone or <@&role id>")}
          autoComplete="off"
          spellCheck={false}
          className={inputClass}
        />
      </div>

      <Divider />
      <SectionLabel>Send these events</SectionLabel>
      <Toggle
        checked={flag("NotifyDropped", true)}
        onChange={(v) => s.setBool("Webhook", "NotifyDropped", v)}
        label="An account dropped or its client closed unexpectedly"
        description="With the reason, when the client log says it."
      />
      <Toggle
        checked={flag("NotifyReconnectFailed", true)}
        onChange={(v) => s.setBool("Webhook", "NotifyReconnectFailed", v)}
        label="Auto-reconnect gave up"
      />
      <Toggle
        checked={flag("NotifyNotResponding", true)}
        onChange={(v) => s.setBool("Webhook", "NotifyNotResponding", v)}
        label="A client stopped responding"
        description="The window stayed frozen for 30 seconds."
      />
      <Toggle
        checked={flag("NotifyReconnected", false)}
        onChange={(v) => s.setBool("Webhook", "NotifyReconnected", v)}
        label="An account reconnected"
      />
      <Toggle
        checked={flag("NotifyLaunchFailed", false)}
        onChange={(v) => s.setBool("Webhook", "NotifyLaunchFailed", v)}
        label="A launch failed"
      />
      <Toggle
        checked={flag("NotifyWrongAccount", true)}
        onChange={(v) => s.setBool("Webhook", "NotifyWrongAccount", v)}
        label="An account opened as a different Roblox account"
        description="The launch queue is paused and the client is left open."
      />

      <Divider />
      <Toggle
        checked={flag("HideNames", true)}
        onChange={(v) => s.setBool("Webhook", "HideNames", v)}
        label="Hide account names in messages"
        description="On: names are masked like the Names hidden mode. Off: the alias (or username) is sent. Cookies, passwords and private server links are never sent."
      />
    </div>
  );
}
