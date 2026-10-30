import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle } from "lucide-react";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { NumberField } from "../ui/NumberField";
import { useTr } from "../../i18n/text";
import { MAX_WEBHOOK_MENTION_CHARS, webhookUrlProblem } from "../../utils/discordWebhook";

/**
 * Página Discord alerts: avisos, status ao vivo e resumo diário pelo webhook do
 * próprio usuário. Antes era a seção Settings › Notifications (só os avisos).
 *
 * Tudo desligado por padrão — as informações saem do PC. A tela só grava as
 * opções da seção `[Webhook]`; quem confere a URL, monta as mensagens e manda é
 * o backend (commands/webhook.rs, commands/discord_status.rs). Ver
 * docs/features/webhook.md.
 */

interface EventDef {
  /** O nome do evento nas chaves: `Notify<key>` e `Mention<key>`. */
  key: string;
  label: string;
  description?: string;
  defaultOn: boolean;
}

/** Os mesmos padrões do backend (`WebhookEventKind::default_on`). */
const EVENTS: EventDef[] = [
  {
    key: "Dropped",
    label: "An account dropped or its client closed unexpectedly",
    description: "With the reason, when the client log says it.",
    defaultOn: true,
  },
  { key: "ReconnectFailed", label: "Auto-reconnect gave up", defaultOn: true },
  {
    key: "NotResponding",
    label: "A client stopped responding",
    description: "The window stayed frozen for 30 seconds.",
    defaultOn: true,
  },
  { key: "Reconnected", label: "An account reconnected", defaultOn: false },
  { key: "LaunchFailed", label: "A launch failed", defaultOn: false },
  {
    key: "WrongAccount",
    label: "An account opened as a different Roblox account",
    description: "The launch queue is paused and the client is left open.",
    defaultOn: true,
  },
  {
    key: "Moderated",
    label: "An account was banned",
    description: "Roblox is blocking the account (ban or review). Sent once, when the account moves to the moderated group.",
    defaultOn: true,
  },
  {
    key: "BatchFinished",
    label: "A launch batch finished",
    description: "How many accounts opened, failed or were cancelled. Only batches of 2 accounts or more.",
    defaultOn: false,
  },
  {
    key: "MemoryClosed",
    label: "A client was closed for memory",
    description: "Over the memory limit in Optimization, or under the Watcher minimum.",
    defaultOn: true,
  },
  {
    key: "ScriptMessage",
    label: "A message from a Python script",
    description: "What your scripts send with multialt.discord(...) or the notify block. Grouped and rate limited like the other alerts.",
    defaultOn: true,
  },
  {
    key: "SystemLowMemory",
    label: "Windows is almost out of memory",
    description: "Under 10% free. Sent again only after it goes back over 15%.",
    defaultOn: false,
  },
];

const inputClass =
  "w-full min-w-0 px-2.5 py-1 rounded-md text-[13px] placeholder-zinc-600 bg-zinc-800/60 border border-zinc-700/60 text-zinc-200 focus:outline-none focus:border-sky-500/40";

const buttonClass =
  "shrink-0 rounded-lg border border-zinc-700/70 bg-zinc-800 px-3 py-1.5 text-[12px] font-medium text-zinc-200 transition-colors hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-50";

/** Campo de texto que grava ao sair, só quando mudou. */
function SavedText({
  s,
  settingKey,
  ariaLabel,
  placeholder,
  maxLength,
  id,
}: {
  s: UseSettingsReturn;
  settingKey: string;
  ariaLabel: string;
  placeholder?: string;
  maxLength?: number;
  id?: string;
}) {
  const saved = s.get("Webhook", settingKey, "");
  const [value, setValue] = useState(saved);
  useEffect(() => setValue(saved), [saved]);
  return (
    <input
      id={id}
      aria-label={ariaLabel}
      value={value}
      maxLength={maxLength}
      onChange={(e) => setValue(e.target.value)}
      onBlur={(e) => {
        if (e.target.value.trim() !== saved) s.set("Webhook", settingKey, e.target.value.trim());
      }}
      placeholder={placeholder}
      autoComplete="off"
      spellCheck={false}
      className={inputClass}
    />
  );
}

/** Hora (24 h, "HH:MM") que grava ao escolher. */
function SavedTime({
  s,
  settingKey,
  ariaLabel,
  fallback,
}: {
  s: UseSettingsReturn;
  settingKey: string;
  ariaLabel: string;
  fallback: string;
}) {
  const value = s.get("Webhook", settingKey, fallback) || fallback;
  return (
    <input
      type="time"
      aria-label={ariaLabel}
      value={value}
      onChange={(e) => {
        if (/^\d{2}:\d{2}$/.test(e.target.value)) s.set("Webhook", settingKey, e.target.value);
      }}
      style={{ width: 112 }}
      className="sidebar-input tabular-nums"
    />
  );
}

/** Botão que chama um comando do backend e mostra o resultado ao lado. */
function SendButton({ command, label, sentText }: { command: string; label: string; sentText: string }) {
  const t = useTr();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  async function run() {
    setBusy(true);
    setResult(null);
    try {
      await invoke(command);
      setResult({ ok: true, text: t(sentText) });
    } catch (e) {
      setResult({ ok: false, text: t(String(e)) });
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="flex flex-wrap items-center gap-2 px-1 py-1.5">
      <button type="button" onClick={() => void run()} disabled={busy} className={buttonClass}>
        {busy ? t("Sending...") : t(label)}
      </button>
      {result && (
        <p role="status" className={`text-[12px] ${result.ok ? "text-emerald-400" : "text-red-400"}`}>
          {result.text}
        </p>
      )}
    </div>
  );
}

export function DiscordSettings({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const enabled = s.getBool("Webhook", "Enabled");
  const savedUrl = s.get("Webhook", "Url", "");
  const [url, setUrl] = useState(savedUrl);
  const [urlError, setUrlError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  // As settings chegam depois de montar: o campo pega o valor salvo.
  useEffect(() => setUrl(savedUrl), [savedUrl]);

  // Caixas com padrão "ligado": chave ausente conta como ligada.
  const flag = (key: string, defaultOn: boolean) => s.get("Webhook", key, defaultOn ? "true" : "false") === "true";

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
      await invoke("send_test_webhook", { url: url.trim(), mention: s.get("Webhook", "Mention", "").trim() });
      setTestResult({ ok: true, text: t("Test message sent. Check the Discord channel.") });
    } catch (e) {
      setTestResult({ ok: false, text: t(String(e)) });
    } finally {
      setTesting(false);
    }
  }

  const interval = Number.parseInt(s.get("Webhook", "StatusInterval", "5"), 10);

  return (
    <div className="space-y-0">
      <SectionLabel>Connection</SectionLabel>
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
          <button type="button" onClick={() => void sendTest()} disabled={testing} className={buttonClass}>
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
        <SavedText
          s={s}
          id="webhook-mention"
          settingKey="Mention"
          ariaLabel={t("Text before each message (optional)")}
          maxLength={MAX_WEBHOOK_MENTION_CHARS}
          placeholder={t("e.g. @everyone or <@&role id>")}
        />
      </div>

      <Toggle
        checked={flag("MaskNames", false)}
        onChange={(v) => s.setBool("Webhook", "MaskNames", v)}
        label="Hide account names in messages"
        description="On: names are masked like the Names hidden mode. Off: the alias (or username) is sent. Cookies, passwords and private server links are never sent."
      />

      <Divider />
      <SectionLabel>Send these events</SectionLabel>
      <p className="px-1 pb-1 text-[12px] text-zinc-500">
        {t("Each event can ping someone else: fill its box with a mention. Empty uses the text above.")}
      </p>
      {EVENTS.map((event) => (
        <div key={event.key} className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <Toggle
              checked={flag(`Notify${event.key}`, event.defaultOn)}
              onChange={(v) => s.setBool("Webhook", `Notify${event.key}`, v)}
              label={event.label}
              description={event.description}
            />
          </div>
          <div className="w-44 shrink-0 pt-2">
            <SavedText
              s={s}
              settingKey={`Mention${event.key}`}
              ariaLabel={`${t("Mention for this event")}: ${t(event.label)}`}
              maxLength={MAX_WEBHOOK_MENTION_CHARS}
              placeholder={t("Same as above")}
            />
          </div>
        </div>
      ))}

      <Divider />
      <SectionLabel>Quiet hours</SectionLabel>
      <Toggle
        checked={flag("QuietHours", false)}
        onChange={(v) => s.setBool("Webhook", "QuietHours", v)}
        label="Quiet hours"
        description="Between these times messages arrive without pinging anyone. Uses this PC's clock."
      />
      <div className="flex flex-wrap items-center gap-2 px-1 py-1.5 text-[13px] text-zinc-300">
        <span>{t("From")}</span>
        <SavedTime s={s} settingKey="QuietStart" ariaLabel={t("Quiet hours start")} fallback="23:00" />
        <span>{t("to")}</span>
        <SavedTime s={s} settingKey="QuietEnd" ariaLabel={t("Quiet hours end")} fallback="08:00" />
      </div>
      <Toggle
        checked={flag("QuietSkip", false)}
        onChange={(v) => s.setBool("Webhook", "QuietSkip", v)}
        label="Don't send notifications during quiet hours"
        description="Off: they still arrive, only without the mention. The live status keeps updating either way."
      />

      <Divider />
      <SectionLabel>Live status</SectionLabel>
      <Toggle
        checked={flag("StatusEnabled", false)}
        onChange={(v) => s.setBool("Webhook", "StatusEnabled", v)}
        label="Keep a live status message"
        description="One message in the channel, edited every few minutes: each open account, its game and how long it has been playing. It never pings anyone. Delete it in Discord and a new one is posted."
      />
      <NumberField
        value={Number.isFinite(interval) ? interval : 5}
        onChange={(v) => s.set("Webhook", "StatusInterval", String(Math.round(v)))}
        label="Update every"
        min={1}
        max={60}
        suffix="min"
      />
      <SendButton command="send_discord_status_now" label="Update now" sentText="Status sent. Check the Discord channel." />

      <Divider />
      <SectionLabel>Daily summary</SectionLabel>
      <Toggle
        checked={flag("SummaryEnabled", false)}
        onChange={(v) => s.setBool("Webhook", "SummaryEnabled", v)}
        label="Send a daily summary"
        description="Once a day: play time of each account and each game, and how many times they dropped. Counted only while MultiAlt is open. If the PC is off at that time, it goes the next time it passes."
      />
      <div className="flex flex-wrap items-center gap-2 px-1 py-1.5 text-[13px] text-zinc-300">
        <span>{t("Send at")}</span>
        <SavedTime s={s} settingKey="SummaryTime" ariaLabel={t("Summary time")} fallback="23:00" />
      </div>
      <div className="px-1 py-2 space-y-1.5">
        <label htmlFor="webhook-mention-summary" className="block text-[13px] text-zinc-300">
          {t("Mention in the summary (optional)")}
        </label>
        <SavedText
          s={s}
          id="webhook-mention-summary"
          settingKey="MentionSummary"
          ariaLabel={t("Mention in the summary (optional)")}
          maxLength={MAX_WEBHOOK_MENTION_CHARS}
          placeholder={t("Empty: no ping")}
        />
      </div>
      <SendButton
        command="send_discord_summary_now"
        label="Send a preview"
        sentText="Preview sent. The day keeps counting."
      />
    </div>
  );
}
