import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../../store";
import type { GeneratorDialogTab } from "../../store";
import { collectGroupNames } from "../../types";
import { accountInitial, accountLabel, hideAccountAvatar } from "../../utils/accountName";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useTr } from "../../i18n/text";
import { Select } from "../ui/Select";
import { NumericInput } from "../ui/NumericInput";
import { X, Eye, EyeOff, Wallet, Info } from "lucide-react";
import { SignupPanel } from "../signup/SignupPanel";

interface GeneratorDialogProps {
  open: boolean;
  onClose: () => void;
  /** Aba em que o diálogo abre; quem escolhe é a entrada usada no menu Add. */
  initialTab?: GeneratorDialogTab;
}

interface GeneratedEntry {
  userId: number;
  username: string;
  at: number;
}

const PROVIDER_OPTIONS = [{ value: "bloxgen", label: "BloxGen" }];

const BLOXGEN_TYPE_OPTIONS = [
  { value: "alt", label: "alt" },
  { value: "+30 days old", label: "+30 days old" },
  { value: "+1 year old", label: "+1 year old" },
  { value: "5+ years old", label: "5+ years old" },
  { value: "dump", label: "dump" },
];

function formatCountdown(targetMs: number | null, nowMs: number): string {
  if (targetMs === null) return "--";
  if (targetMs <= nowMs) return "0:00";
  const secs = Math.ceil((targetMs - nowMs) / 1000);
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function phaseLabel(phase: string, t: (s: string) => string): string {
  switch (phase) {
    case "starting":
      return t("Starting");
    case "generating":
      return t("Generating");
    case "adding":
      return t("Adding account");
    case "cooldown":
      return t("Cooldown");
    case "waiting":
      return t("Waiting to retry");
    case "completed":
      return t("Completed");
    case "stopped":
      return t("Stopped");
    case "error":
      return t("Error");
    default:
      return t("Idle");
  }
}

function phaseTone(phase: string): string {
  switch (phase) {
    case "generating":
    case "adding":
      return "text-violet-300";
    case "cooldown":
      return "text-cyan-300";
    case "waiting":
      return "text-amber-300";
    case "error":
      return "text-red-300";
    case "completed":
      return "text-emerald-300";
    default:
      return "theme-muted";
  }
}

export function GeneratorDialog({ open, onClose, initialTab = "provider" }: GeneratorDialogProps) {
  const t = useTr();
  const store = useStore();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const backdropClose = useBackdropClose(handleClose);
  const status = store.generatorStatus;
  const running = status?.active === true;

  const [provider, setProvider] = useState("bloxgen");
  const [endpoint, setEndpoint] = useState("https://core.bloxgen.net");
  const [apiKey, setApiKey] = useState("");
  const [accountType, setAccountType] = useState("alt");
  const [extraDelaySeconds, setExtraDelaySeconds] = useState(1);
  const [targetGroup, setTargetGroup] = useState("BloxGen");
  const [maxAccounts, setMaxAccounts] = useState(0);

  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [nowMs, setNowMs] = useState(Date.now());
  const [log, setLog] = useState<GeneratedEntry[]>([]);
  /**
   * Duas formas de conseguir conta nova moram no mesmo diálogo: comprar de um
   * provedor (BloxGen) ou criar no formulário do Roblox com o CAPTCHA resolvido
   * pelo usuário.
   */
  const [mode, setMode] = useState<GeneratorDialogTab>(initialTab);

  // Sugestão para o campo "Add To Group": os grupos que já existem entre as
  // contas, em texto cru (ver `collectGroupNames` — não passa por
  // `parseGroupName`). O campo continua um `<input>` livre, isso é só datalist.
  const groupOptions = useMemo(() => collectGroupNames(store.accounts), [store.accounts]);

  // Cada abertura respeita a entrada usada no menu Add.
  useEffect(() => {
    if (visible) setMode(initialTab);
  }, [visible, initialTab]);

  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    (async () => {
      try {
        const all = await invoke<Record<string, Record<string, string>>>("get_all_settings");
        if (cancelled) return;
        const gen = all.Generator || {};
        const bg = all.BloxGen || {};
        setProvider(gen.Provider || "bloxgen");
        setEndpoint(bg.Endpoint || "https://core.bloxgen.net");
        setApiKey(bg.ApiKey || "");
        setAccountType(bg.AccountType || "alt");
        setExtraDelaySeconds(parseInt(gen.ExtraDelaySeconds || "1", 10) || 0);
        setTargetGroup(gen.TargetGroup || "BloxGen");
        setMaxAccounts(parseInt(gen.MaxAccounts || "0", 10) || 0);
      } catch {}
    })();
    return () => {
      cancelled = true;
    };
  }, [visible]);

  useEffect(() => {
    if (!visible) return;
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [visible]);

  useEffect(() => {
    if (!visible) return;
    const promise = listen<{ userId?: number; username?: string }>("generator-account-added", (e) => {
      const userId = e.payload?.userId;
      const username = e.payload?.username;
      if (typeof userId !== "number") return;
      setLog((prev) =>
        [{ userId, username: username || `${userId}`, at: Date.now() }, ...prev].slice(0, 50)
      );
    });
    return () => {
      promise.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [visible]);

  function persist(section: string, key: string, value: string) {
    void invoke("update_setting", { section, key, value }).catch(() => {});
  }

  async function handleTestKey() {
    setTesting(true);
    setTestResult(null);
    try {
      const balance = await invoke<number>("generator_test_key", { provider, endpoint, apiKey });
      setTestResult({
        ok: true,
        message: t("Valid key — balance {{balance}}", { balance: balance.toFixed(4) }),
      });
    } catch (e) {
      setTestResult({ ok: false, message: String(e) });
    } finally {
      setTesting(false);
    }
  }

  async function handleStart() {
    if (!apiKey.trim()) {
      setTestResult({ ok: false, message: t("An API key is required") });
      return;
    }
    setLog([]);
    setBusy(true);
    try {
      await store.startGenerator({
        provider,
        endpoint,
        apiKey,
        accountType,
        extraDelaySeconds,
        targetGroup,
        maxAccounts,
      });
    } catch {
    } finally {
      setBusy(false);
    }
  }

  async function handleStop() {
    setBusy(true);
    try {
      await store.stopGenerator();
    } catch {
    } finally {
      setBusy(false);
    }
  }

  if (!visible) return null;

  const configDisabled = running || busy;
  const countdown = formatCountdown(status?.nextAttemptAtMs ?? null, nowMs);
  const showCountdown =
    running && (status?.phase === "cooldown" || status?.phase === "waiting") && !!status?.nextAttemptAtMs;

  return (
    <div
      className={`fixed inset-0 z-[70] flex items-center justify-center bg-black/60 backdrop-blur-sm ${
        closing ? "animate-fade-out" : "animate-fade-in"
      }`}
      {...backdropClose}
    >
      <div
        className={`theme-panel theme-border rounded-2xl border w-[900px] max-w-[calc(100vw-24px)] h-[640px] max-h-[calc(100vh-24px)] flex flex-col overflow-hidden shadow-2xl ${
          closing ? "animate-scale-out" : "animate-scale-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 py-4 border-b theme-border flex items-center justify-between">
          {/*
            O título era "Account Generator" — o nome de **uma** das duas
            funções servindo de nome para o conjunto, o que fazia a criação
            grátis parecer parte do gerador pago. O título agora é do conjunto;
            cada função guarda o seu nome na aba. Ver docs/features/account-creation.md.
          */}
          <div className="text-[15px] font-semibold text-[var(--panel-fg)]">{t("New Accounts")}</div>
          <div className="flex items-center gap-2">
            <span
              className={`px-2 py-1 rounded-full text-[11px] border ${
                running
                  ? "border-emerald-500/30 bg-emerald-500/15 text-emerald-300 animate-pulse"
                  : "theme-border theme-soft theme-muted"
              }`}
            >
              {running ? t("RUNNING") : t("IDLE")}
            </span>
            <button
              onClick={handleClose}
              className="p-1 rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors"
            >
              <X size={16} strokeWidth={2} />
            </button>
          </div>
        </div>

        {/*
          Nome e linha de apoio são os mesmos do menu `Add` e do
          AddAccountDialog, palavra por palavra: eram quatro nomes para duas
          funções. Mexeu aqui, mexa lá.
        */}
        <div className="px-5 pt-3 flex flex-wrap items-stretch gap-1.5">
          {([
            {
              id: "provider" as const,
              label: t("Account Generator"),
              hint: t("Paid — buys ready-made accounts from BloxGen (third party, API key)"),
            },
            {
              id: "signup" as const,
              label: t("Create Accounts"),
              hint: t("Free — the app fills Roblox's signup form; you solve the CAPTCHA"),
            },
          ]).map((tab) => (
            <button
              key={tab.id}
              onClick={() => setMode(tab.id)}
              className={`px-2.5 py-1.5 text-[12px] rounded-md border text-left transition-colors ${
                mode === tab.id
                  ? "border-[var(--accent-color)] text-[var(--panel-fg)] theme-soft"
                  : "theme-border theme-muted theme-btn-ghost"
              }`}
            >
              {tab.label}
              <span className="block text-[11px] theme-muted leading-snug">{tab.hint}</span>
            </button>
          ))}
        </div>

        {mode === "signup" && (
          <div className="p-4 md:p-5 flex-1 min-h-0">
            <SignupPanel
              onOpenLoginSettings={() => {
                // As Settings abrem por cima de tudo; fechar o diálogo primeiro
                // evita dois modais empilhados sobre o mesmo assunto.
                handleClose();
                store.setSettingsOpen(true);
              }}
            />
          </div>
        )}

        <div
          className={`p-4 md:p-5 flex-1 min-h-0 grid-cols-1 lg:grid-cols-12 gap-4 ${
            mode === "provider" ? "grid" : "hidden"
          }`}
        >
          <div className="lg:col-span-5 min-h-0 animate-slide-left">
            <div className="theme-surface rounded-2xl border theme-border h-full p-3 overflow-y-auto space-y-3">
              {/*
                O gerador não é uma função do app: é uma chamada para
                `core.bloxgen.net` com a API key da pessoa, e cada conta sai do
                saldo dessa chave (`commands/generators.rs`, `/api/generate` e
                `/api/balance`). A tela não dizia nem que era de terceiro, nem
                que custava, nem que a aba ao lado faz de graça.
              */}
              <section className="rounded-xl border border-amber-500/30 bg-amber-500/10 p-3 space-y-1.5">
                <div className="flex items-center gap-1.5 text-[12px] font-medium text-amber-200">
                  <Info size={13} strokeWidth={1.75} className="shrink-0" />
                  {t("BloxGen is a third-party service, not part of this app")}
                </div>
                <p className="text-[12px] text-amber-100/80 leading-snug">
                  {t(
                    "Every account it hands over is charged to the balance on your API key, so generating costs money. The app only calls their API with the key you paste — the accounts, the prices and the balance are theirs."
                  )}
                </p>
                <p className="text-[12px] text-amber-100/80 leading-snug">
                  {t(
                    "Free alternative: the Create Accounts tab signs up on Roblox's own form at no cost — the app fills the form and you solve the CAPTCHA."
                  )}
                </p>
              </section>

              <section className="theme-surface rounded-xl border theme-border p-3">
                <div className="text-[13px] font-medium text-[var(--panel-fg)] mb-2">{t("Provider")}</div>
                <Select
                  value={provider}
                  options={PROVIDER_OPTIONS}
                  disabled={configDisabled}
                  onChange={(v) => {
                    setProvider(v);
                    persist("Generator", "Provider", v);
                  }}
                  className="w-full"
                />
                <div className="mt-2 grid grid-cols-1 gap-2">
                  <input
                    value={endpoint}
                    disabled={configDisabled}
                    onChange={(e) => setEndpoint(e.target.value)}
                    onBlur={() => persist("BloxGen", "Endpoint", endpoint.trim())}
                    placeholder="https://core.bloxgen.net"
                    className="sidebar-input text-xs font-mono disabled:opacity-60"
                  />
                  <div className="flex items-center gap-1.5">
                    <input
                      type={showKey ? "text" : "password"}
                      value={apiKey}
                      disabled={configDisabled}
                      spellCheck={false}
                      autoComplete="off"
                      onChange={(e) => setApiKey(e.target.value.trim())}
                      onBlur={() => persist("BloxGen", "ApiKey", apiKey.trim())}
                      placeholder="BLOX-XXXXXXXXXXXXXXXX"
                      className="sidebar-input text-xs font-mono flex-1 min-w-0 disabled:opacity-60"
                    />
                    <button
                      type="button"
                      onClick={() => setShowKey((v) => !v)}
                      className="p-1.5 rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors shrink-0"
                      aria-label={showKey ? t("Hide API key") : t("Show API key")}
                    >
                      {showKey ? <EyeOff size={14} strokeWidth={1.5} /> : <Eye size={14} strokeWidth={1.5} />}
                    </button>
                  </div>
                  <Select
                    value={accountType}
                    options={BLOXGEN_TYPE_OPTIONS}
                    disabled={configDisabled}
                    onChange={(v) => {
                      setAccountType(v);
                      persist("BloxGen", "AccountType", v);
                    }}
                    className="w-full"
                  />
                  <button
                    onClick={handleTestKey}
                    disabled={testing || !apiKey.trim()}
                    className="sidebar-btn-sm flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <Wallet size={13} strokeWidth={1.75} />
                    {testing ? t("Checking...") : t("Test API key")}
                  </button>
                  {testResult ? (
                    <div
                      className={`text-[12px] ${testResult.ok ? "text-emerald-300" : "text-red-300"} break-words`}
                    >
                      {testResult.message}
                    </div>
                  ) : null}
                </div>
              </section>

              <section className="theme-surface rounded-xl border theme-border p-3">
                <div className="text-[13px] font-medium text-[var(--panel-fg)] mb-2">{t("Generation")}</div>
                <div className="grid grid-cols-1 gap-2">
                  <label className="flex items-center gap-2">
                    <span className="text-[12px] theme-muted w-40 shrink-0">{t("Extra delay after cooldown")}</span>
                    <NumericInput
                      value={extraDelaySeconds}
                      min={0}
                      max={3600}
                      integer
                      disabled={configDisabled}
                      onChange={setExtraDelaySeconds}
                      onCommit={(v) => persist("Generator", "ExtraDelaySeconds", String(v))}
                      containerClassName="relative flex-1"
                      className="sidebar-input text-xs w-full disabled:opacity-60"
                    />
                    <span className="text-[12px] theme-muted">{t("sec")}</span>
                  </label>
                  <label className="flex items-center gap-2">
                    <span className="text-[12px] theme-muted w-40 shrink-0">{t("Add To Group")}</span>
                    <input
                      value={targetGroup}
                      disabled={configDisabled}
                      list="generator-dialog-target-group-options"
                      onChange={(e) => setTargetGroup(e.target.value)}
                      onBlur={() => persist("Generator", "TargetGroup", targetGroup.trim())}
                      placeholder="BloxGen"
                      className="sidebar-input text-xs flex-1 disabled:opacity-60"
                    />
                    <datalist id="generator-dialog-target-group-options">
                      {groupOptions.map((g) => (
                        <option key={g} value={g} />
                      ))}
                    </datalist>
                  </label>
                  <label className="flex items-center gap-2">
                    <span className="text-[12px] theme-muted w-40 shrink-0">{t("Stop after")}</span>
                    <NumericInput
                      value={maxAccounts}
                      min={0}
                      max={10000}
                      integer
                      disabled={configDisabled}
                      onChange={setMaxAccounts}
                      onCommit={(v) => persist("Generator", "MaxAccounts", String(v))}
                      containerClassName="relative flex-1"
                      className="sidebar-input text-xs w-full disabled:opacity-60"
                    />
                    <span className="text-[12px] theme-muted">{t("accts")}</span>
                  </label>
                  <div className="text-[11px] theme-muted">{t("Set Stop After to 0 to keep generating until stopped")}</div>
                </div>
              </section>

              <section className="theme-surface rounded-xl border theme-border p-3">
                <div className="grid grid-cols-1 gap-1.5">
                  <button
                    onClick={handleStart}
                    disabled={running || busy || !apiKey.trim()}
                    className="sidebar-btn-sm disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {t("Start Generator")}
                  </button>
                  <button
                    onClick={handleStop}
                    disabled={!running || busy}
                    className="sidebar-btn-sm text-red-200 border-red-400/40 hover:bg-red-500/15 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {t("Stop Generator")}
                  </button>
                </div>
              </section>
            </div>
          </div>

          <div className="lg:col-span-7 min-h-0 animate-slide-right">
            <section className="theme-surface rounded-2xl border theme-border h-full p-3 flex flex-col min-h-0">
              <div className="grid grid-cols-3 gap-2">
                <div className="rounded-xl border theme-border bg-[rgba(0,0,0,0.18)] px-3 py-2.5">
                  <div className="text-[11px] uppercase tracking-[0.08em] theme-muted">{t("Status")}</div>
                  <div className={`text-[13px] font-medium leading-tight mt-0.5 ${phaseTone(status?.phase || "idle")}`}>
                    {phaseLabel(status?.phase || "idle", t)}
                  </div>
                </div>
                <div className="rounded-xl border theme-border bg-[rgba(0,0,0,0.18)] px-3 py-2.5">
                  <div className="text-[11px] uppercase tracking-[0.08em] theme-muted">{t("Generated")}</div>
                  <div className="text-[15px] font-mono text-[var(--panel-fg)] leading-tight mt-0.5">
                    {status?.totalGenerated ?? 0}
                    {status && status.maxAccounts > 0 ? (
                      <span className="text-[12px] theme-muted"> / {status.maxAccounts}</span>
                    ) : null}
                  </div>
                </div>
                <div
                  className={`rounded-xl border px-3 py-2.5 ${
                    showCountdown
                      ? "border-cyan-500/30 bg-cyan-500/10"
                      : "theme-border bg-[rgba(0,0,0,0.18)]"
                  }`}
                >
                  <div className="text-[11px] uppercase tracking-[0.08em] theme-muted">{t("Next in")}</div>
                  <div
                    className={`text-[15px] font-mono leading-tight mt-0.5 ${
                      showCountdown ? "text-cyan-200" : "theme-muted"
                    }`}
                  >
                    {showCountdown ? countdown : "--"}
                  </div>
                </div>
              </div>

              {status?.lastError ? (
                <div className="mt-2 rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-[12px] text-red-300 break-words animate-fade-in">
                  {status.lastError}
                </div>
              ) : null}

              <div className="mt-3 flex items-center justify-between">
                <div className="text-[12px] font-medium text-[var(--panel-fg)]">{t("Recently added")}</div>
                {log.length > 0 ? (
                  <button
                    onClick={() => setLog([])}
                    className="text-[12px] theme-muted hover:text-[var(--panel-fg)] transition-colors"
                  >
                    {t("Clear")}
                  </button>
                ) : null}
              </div>

              <div className="mt-2 flex-1 min-h-0 overflow-y-auto space-y-1.5 pr-1">
                {log.length === 0 ? (
                  <div className="h-full rounded-xl border theme-border theme-soft flex flex-col items-center justify-center text-center px-6">
                    <div className="text-[12px] theme-muted">
                      {running
                        ? t("Waiting for the first account to come through")
                        : t("Start the generator to begin adding accounts")}
                    </div>
                  </div>
                ) : (
                  log.map((entry, index) => {
                    // As contas geradas já entram na lista do usuário: com "Names
                    // hidden" nome e foto somem aqui como lá.
                    const avatarUrl = hideAccountAvatar(store) ? undefined : store.avatarUrls.get(entry.userId);
                    const named = { Username: entry.username, Alias: "" };
                    return (
                      <div
                        key={`${entry.userId}-${entry.at}`}
                        className="flex items-center gap-2.5 rounded-xl border theme-border theme-soft px-3 py-2 animate-fade-in-up"
                        style={{ animationDelay: `${Math.min(index * 30, 150)}ms` }}
                      >
                        {avatarUrl ? (
                          <img
                            src={avatarUrl}
                            alt=""
                            className="w-7 h-7 rounded-full bg-[var(--panel-soft)] shrink-0"
                            loading="lazy"
                          />
                        ) : (
                          <div className="w-7 h-7 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted text-[11px] font-medium shrink-0">
                            {accountInitial(named, store)}
                          </div>
                        )}
                        <div className="min-w-0 flex-1">
                          <div className="text-[12px] text-[var(--panel-fg)] truncate">{accountLabel(named, store)}</div>
                          <div className="text-[11px] theme-muted font-mono">{entry.userId}</div>
                        </div>
                        <div className="text-[11px] theme-muted shrink-0">
                          {new Date(entry.at).toLocaleTimeString([], {
                            hour: "2-digit",
                            minute: "2-digit",
                            second: "2-digit",
                            hour12: false,
                          })}
                        </div>
                      </div>
                    );
                  })
                )}
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}
