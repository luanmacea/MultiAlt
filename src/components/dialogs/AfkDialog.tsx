import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X, Check, Send, Crosshair } from "lucide-react";
import { useStore } from "../../store";
import {
  clampAfkPercent,
  formatAfkPoint,
  readAfkPoint,
  readAfkSettingsPoint,
  writeAfkPoint,
  type AfkMode,
  type AfkPoint,
} from "../../afkClickPoint";
import { useModalClose } from "../../hooks/useModalClose";
import { useTr } from "../../i18n/text";
import { Select } from "../ui/Select";
import { NumericInput } from "../ui/NumericInput";
import { ToggleRow } from "../ui/ToggleRow";

/**
 * Tempo até o próximo envio, no formato `m:ss` — nunca acima do intervalo.
 *
 * O prazo que chega do backend é "agora + intervalo" no relógio dele, e o
 * `nowMs` da tela só anda no tique de 1 s: comparado com um relógio de até 1 s
 * atrás, a contagem nascia em "10:01" (no start e depois de cada envio manual).
 * Faltar mais que um intervalo não existe, então o teto é o intervalo — na
 * hora do render, sem piscar um quadro com o valor errado.
 */
function formatCountdown(targetMs: number | null, nowMs: number, intervalMs: number): string {
  if (targetMs === null) return "--";
  if (targetMs <= nowMs) return "0:00";
  const remainingMs = intervalMs > 0 ? Math.min(targetMs - nowMs, intervalMs) : targetMs - nowMs;
  const secs = Math.ceil(remainingMs / 1000);
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/**
 * Tempo desde que a sessão ligou: `<1m`, `12m`, `1h 5m`. É o que responde "isso
 * está funcionando?" sem esperar o próximo ciclo.
 */
export function formatAfkElapsed(startedAtMs: number | null, nowMs: number): string {
  if (startedAtMs === null) return "--";
  const minutes = Math.floor(Math.max(0, nowMs - startedAtMs) / 60_000);
  if (minutes < 1) return "<1m";
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

/**
 * AFK mode: o app manda uma tecla, de tempo em tempo, para a janela de cada
 * conta que está no modo — para o jogo não contar a conta como parada e não
 * precisar de rejoin.
 *
 * A tela tem duas obrigações que não são enfeite:
 * 1. dizer que **cada ciclo tira o foco da janela do usuário** e só o devolve
 *    depois da última conta — cerca de meio segundo por conta (é o preço do
 *    `SendInput`, que só alcança a janela em primeiro plano);
 * 2. só oferecer tecla da lista fechada que o backend entrega (`afkKeys`) — sem
 *    campo livre e sem tecla padrão, porque ligar o modo não pode mexer no
 *    personagem com uma tecla que o usuário não escolheu.
 *
 * No modo **clique** não há tecla: cada conta leva um clique esquerdo num ponto
 * relativo (%) da janela dela — o padrão, ou o próprio da conta. O Marcar dá
 * 3 s para o usuário parar o mouse em cima do ponto e o backend lê a posição.
 */
export function AfkDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const { visible, closing, handleClose } = useModalClose(open, onClose);

  const status = store.afkStatus;
  const running = status?.active === true;

  const [intervalMinutes, setIntervalMinutes] = useState(10);
  const [key, setKey] = useState("");
  const [draftUserIds, setDraftUserIds] = useState<number[]>([]);
  const [beepOnCycle, setBeepOnCycle] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sendingNow, setSendingNow] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [mode, setMode] = useState<AfkMode>("key");
  const [defaultPoint, setDefaultPoint] = useState<AfkPoint>(() => readAfkSettingsPoint(undefined));
  /** Marcar em andamento: de quem é o ponto e quantos segundos faltam. */
  const [capture, setCapture] = useState<{ target: "default" | number; secondsLeft: number } | null>(
    null
  );
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Intervalo e tecla vêm do INI ao abrir; com sessão em andamento, o que vale
  // é o que a sessão está usando.
  useEffect(() => {
    if (!visible) return;
    const afk = store.settings?.Afk ?? {};
    setIntervalMinutes(parseInt(afk.IntervalMinutes || "10", 10) || 10);
    setKey(afk.Key || "");
    setBeepOnCycle(afk.BeepOnCycle === "true");
    setMode(afk.Mode === "click" ? "click" : "key");
    setDefaultPoint(readAfkSettingsPoint(afk));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible]);

  // Com sessão em andamento, o que vale é o que a sessão está usando — não o
  // rascunho local, que o efeito de abertura relê do INI a qualquer momento.
  const effectiveKey = running ? status?.key ?? "" : key;
  const effectiveInterval = running ? status?.intervalMinutes ?? 0 : intervalMinutes;
  const effectiveMode: AfkMode = running ? (status?.mode === "click" ? "click" : "key") : mode;
  const effectivePoint: AfkPoint = running
    ? { x: clampAfkPercent(status?.clickX ?? 50), y: clampAfkPercent(status?.clickY ?? 50) }
    : defaultPoint;
  const clickMode = effectiveMode === "click";

  // O tique não depende de `running`: o tempo decorrido tem de andar sempre que
  // existe sessão, inclusive no intervalo em que a tela ainda não recebeu o
  // status novo — senão o contador congela e parece que o modo morreu.
  useEffect(() => {
    if (!visible) return;
    setNowMs(Date.now());
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [visible]);

  const sessionUserIds = useMemo(
    () => (running ? (status?.accounts ?? []).map((a) => a.userId) : []),
    [running, status]
  );
  /** Quem está no modo: a sessão manda quando há sessão; senão, o rascunho. */
  const inAfk = running ? sessionUserIds : draftUserIds;

  // Com sessão, o rascunho acompanha quem está nela. Quando a sessão acaba —
  // Parar, ou por qualquer outro caminho —, a tela continua marcando quem
  // estava no modo, inclusive a conta que entrou com a sessão ligada, e religar
  // leva as mesmas contas. Sem isto a seleção voltava à de antes do start, e a
  // conta acrescentada ficava de fora do próximo start sem aviso.
  useEffect(() => {
    if (running) setDraftUserIds(sessionUserIds);
  }, [running, sessionUserIds]);

  /**
   * Só conta com cliente aberto **por este app** pode receber tecla: é o tracker
   * que sabe qual PID é de qual conta. Quem está no modo continua na lista mesmo
   * se o cliente caiu, senão não daria para tirá-la de lá.
   */
  const candidates = useMemo(() => {
    const ids = new Set<number>([...store.launchedByProgram, ...inAfk]);
    return store.accounts.filter((a) => ids.has(a.UserID));
  }, [store.accounts, store.launchedByProgram, inAfk]);

  const focusDenied = (status?.accounts ?? []).some((a) => a.lastErrorCode === "focusDenied");
  const keyAllowed = store.afkKeys.includes(effectiveKey);
  /** O modo clique não usa tecla; o modo tecla não liga sem uma da lista. */
  const sendReady = clickMode || keyAllowed;
  const canStart = sendReady && inAfk.length > 0 && !busy;
  const statusByUserId = useMemo(
    () => new Map((status?.accounts ?? []).map((a) => [a.userId, a])),
    [status]
  );

  /** A frase que explica por que uma conta não recebeu a tecla. */
  function sendErrorText(code: string | null, raw: string | null, name: string): string {
    switch (code) {
      case "focusDenied":
        return t(
          "{{name}}: Windows did not let this account's window come to the front, so nothing was sent.",
          { name }
        );
      case "noWindow":
        return t("{{name}}: has no Roblox client open right now.", { name });
      case "keyRefused":
        return t("{{name}}: Windows refused the key.", { name });
      case "clickRefused":
        return t("{{name}}: Windows refused the click.", { name });
      default:
        return `${name}: ${raw ?? ""}`.trim();
    }
  }

  /** A frase de um Marcar que não deu certo, pelo código que o backend devolve. */
  function captureErrorText(code: string): string {
    switch (code) {
      case "noCursor":
        return t("Could not read where the mouse is");
      case "noWindow":
        return t("There is no window under the mouse");
      case "notAnAccountWindow":
        return t("That window is not a Roblox client this app opened");
      case "outsideGameArea":
        return t("Put the mouse inside the game area, not on the border or the title bar");
      default:
        return code;
    }
  }

  function accountName(userId: number): string {
    const account = store.accounts.find((it) => it.UserID === userId);
    return account?.Alias || account?.Username || `${t("User ID")}: ${userId}`;
  }

  /**
   * Marcar: 3 s para o usuário parar o mouse em cima do ponto numa janela de
   * conta, e aí o backend lê a posição **uma vez**. O ponto vira o padrão ou o
   * próprio da conta `target`. Qualquer janela de conta serve: o ponto é
   * relativo, então cai no mesmo lugar nas outras.
   */
  async function startMark(target: "default" | number) {
    if (capture) return;
    for (let seconds = 3; seconds > 0; seconds--) {
      if (!mountedRef.current) return;
      setCapture({ target, secondsLeft: seconds });
      await new Promise((resolve) => window.setTimeout(resolve, 1000));
    }
    if (!mountedRef.current) return;
    setCapture(null);
    try {
      const got = await store.captureAfkPoint();
      const point = { x: clampAfkPercent(got.xPct), y: clampAfkPercent(got.yPct) };
      if (target === "default") {
        setDefaultPoint(point);
        persist("ClickX", String(point.x));
        persist("ClickY", String(point.y));
      } else {
        const account = store.accounts.find((it) => it.UserID === target);
        if (account) {
          await store.updateAccount({ ...account, Fields: writeAfkPoint(account.Fields, point) });
        }
      }
      store.addToast(t("Point marked on {{name}}'s window", { name: accountName(got.userId) }));
    } catch (e) {
      store.addToast(captureErrorText(String(e)), "error");
    }
  }

  /** Tira o ponto próprio da conta: ela volta a usar o padrão no ciclo seguinte. */
  async function clearOwnPoint(userId: number) {
    const account = store.accounts.find((it) => it.UserID === userId);
    if (!account) return;
    try {
      await store.updateAccount({ ...account, Fields: writeAfkPoint(account.Fields, null) });
    } catch {
      // O erro já virou toast no store.
    }
  }

  function persist(settingKey: string, value: string) {
    void invoke("update_setting", { section: "Afk", key: settingKey, value }).catch(() => {});
  }

  async function toggleAccount(userId: number) {
    const next = inAfk.includes(userId)
      ? inAfk.filter((id) => id !== userId)
      : [...inAfk, userId];
    if (!running) {
      setDraftUserIds(next);
      return;
    }
    setBusy(true);
    try {
      await store.setAfkAccounts(next);
      // Desmarcar a última conta encerra a sessão, e aí o espelho acima não
      // roda mais: o rascunho fica com o que o usuário pediu, e não com a
      // conta que ele acabou de tirar.
      setDraftUserIds(next);
      // E diz que desligou: sem isto a pílula virava "Off" calada, enquanto o
      // Parar avisa.
      if (next.length === 0) store.addToast(t("AFK mode off: no account is left in it"));
    } catch {
      // O erro já virou toast no store.
    } finally {
      setBusy(false);
    }
  }

  async function handleStart() {
    if (!canStart) return;
    setBusy(true);
    try {
      await store.startAfkMode({
        userIds: inAfk,
        intervalMinutes: effectiveInterval,
        key: effectiveKey,
        mode: effectiveMode,
        clickX: effectivePoint.x,
        clickY: effectivePoint.y,
      });
    } catch {
      // idem
    } finally {
      setBusy(false);
    }
  }

  async function handleSendNow() {
    // Sem sessão não há conta no modo, e envio manual não pode alcançar cliente
    // de conta fora dele.
    if (!running || !sendReady || inAfk.length === 0) return;
    setSendingNow(true);
    try {
      const sent = await store.afkTriggerNow(inAfk);
      if (clickMode) {
        store.addToast(
          sent === 1
            ? t("Clicked on 1 account")
            : sent > 1
              ? t("Clicked on {{count}} accounts", { count: sent })
              : t("No Roblox window received the click")
        );
        return;
      }
      store.addToast(
        sent === 1
          ? t("Sent {{key}} to 1 account", { key: effectiveKey })
          : sent > 1
            ? t("Sent {{key}} to {{count}} accounts", { key: effectiveKey, count: sent })
            : t("No Roblox window received the key")
      );
    } catch {
      // O erro já virou toast no store.
    } finally {
      setSendingNow(false);
    }
  }

  async function handleStop() {
    setBusy(true);
    try {
      await store.stopAfkMode();
    } catch {
      // idem
    } finally {
      setBusy(false);
    }
  }

  if (!visible) return null;

  const configDisabled = running || busy;

  return (
    <div
      className={`fixed inset-0 z-[70] flex items-center justify-center bg-black/60 backdrop-blur-sm ${
        closing ? "animate-fade-out" : "animate-fade-in"
      }`}
      onClick={handleClose}
    >
      <div
        className={`theme-panel theme-border rounded-2xl border w-[480px] max-w-[calc(100vw-24px)] max-h-[calc(100vh-24px)] flex flex-col overflow-hidden shadow-2xl ${
          closing ? "animate-scale-out" : "animate-scale-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 py-4 border-b theme-border flex items-center justify-between">
          <div className="text-[15px] font-semibold text-[var(--panel-fg)]">{t("AFK Mode")}</div>
          <div className="flex items-center gap-2">
            <span
              className={`px-2 py-1 rounded-full text-[11px] border ${
                running
                  ? "border-emerald-500/30 bg-emerald-500/15 text-emerald-300"
                  : "theme-border theme-soft theme-muted"
              }`}
            >
              {/* Estado, não ação: entre um ciclo e outro — ou com o foco negado
                  em todas as contas — nada está sendo enviado. */}
              {running ? t("On") : t("Off")}
            </span>
            <button
              onClick={handleClose}
              aria-label={t("Close")}
              className="p-1 rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors"
            >
              <X size={16} strokeWidth={2} />
            </button>
          </div>
        </div>

        <div className="p-4 overflow-y-auto space-y-3">
          <section className="theme-surface rounded-xl border theme-border p-3 space-y-2.5">
            <div className="flex items-center gap-2">
              <span className="text-[12px] theme-muted w-32 shrink-0">{t("Send every")}</span>
              <NumericInput
                value={effectiveInterval}
                min={1}
                max={120}
                integer
                disabled={configDisabled}
                ariaLabel={t("Send every")}
                onChange={setIntervalMinutes}
                onCommit={(v) => persist("IntervalMinutes", String(v))}
                containerClassName="relative flex-1"
                className="sidebar-input text-xs w-full disabled:opacity-60"
              />
              <span className="text-[12px] theme-muted">{t("min")}</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[12px] theme-muted w-32 shrink-0">{t("What to send")}</span>
              <Select
                value={effectiveMode}
                options={[
                  { value: "key", label: "Key press" },
                  { value: "click", label: "Mouse click" },
                ]}
                disabled={configDisabled}
                ariaLabel="What to send"
                onChange={(v) => {
                  const next: AfkMode = v === "click" ? "click" : "key";
                  setMode(next);
                  persist("Mode", next);
                }}
                className="flex-1"
              />
            </div>
            {clickMode ? (
              <>
                <div className="flex items-center gap-2">
                  <span className="text-[12px] theme-muted w-32 shrink-0">{t("Click point")}</span>
                  <span className="flex-1 text-[12px] font-mono text-[var(--panel-fg)]">
                    {formatAfkPoint(effectivePoint)}
                  </span>
                  <button
                    onClick={() => startMark("default")}
                    disabled={configDisabled || capture !== null}
                    aria-label={t("Mark the click point for all accounts")}
                    className="sidebar-btn-sm flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <Crosshair size={13} strokeWidth={1.75} />
                    {t("Mark")}
                  </button>
                </div>
                {capture ? (
                  <div className="text-[11px] text-sky-300 leading-4" role="status">
                    {t("Put the mouse over the spot in a game window: {{seconds}}", {
                      seconds: capture.secondsLeft,
                    })}
                  </div>
                ) : null}
                <div className="text-[11px] theme-muted leading-4">
                  {t(
                    "Mark gives you 3 seconds to rest the mouse on the spot inside the Roblox window of any account in the list. The point is a percentage of the window, so it lands in the same place in windows of any size. An account can have its own point below."
                  )}
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center gap-2">
                  {/* "Key" sozinho é a chave de campo da conta ("Chave" em pt), da
                      tela de campos; aqui é tecla, e precisa de texto próprio. */}
                  <span className="text-[12px] theme-muted w-32 shrink-0">{t("Key to send")}</span>
                  <Select
                    value={effectiveKey}
                    options={store.afkKeys.map((k) => ({ value: k, label: k }))}
                    disabled={configDisabled}
                    ariaLabel="Key to send"
                    onChange={(v) => {
                      setKey(v);
                      persist("Key", v);
                    }}
                    className="flex-1"
                  />
                </div>
                {!keyAllowed && !running ? (
                  <div className="text-[11px] text-amber-300/90 leading-4">
                    {t("Choose one of these keys — AFK mode does not start without a key you picked")}
                  </div>
                ) : null}
              </>
            )}
            <ToggleRow
              label="Beep when a cycle finishes"
              checked={beepOnCycle}
              onChange={(v) => {
                setBeepOnCycle(v);
                persist("BeepOnCycle", v ? "true" : "false");
              }}
            />
            <div className="text-[11px] theme-muted leading-4">
              {t(
                "Each cycle takes the focus away from the window you are using: it brings the Roblox window of each account whose turn it is to the front, one after another, for about half a second each, and gives the focus back only after the last one — about 4 seconds with 10 accounts."
              )}{" "}
              {t("Meanwhile, what you type goes to the Roblox window, not to the program you were using.")}{" "}
              {clickMode ? (
                <>{t("In click mode the cursor also jumps to the point and comes back.")} </>
              ) : null}
              {t(
                "And when Windows keeps the window in the background — which is what it usually does while this app is not the one you are using — nothing is sent at all, and the account below says so."
              )}
            </div>
          </section>

          <section className="theme-surface rounded-xl border theme-border p-3">
            <div className="text-[12px] text-[var(--panel-fg)] mb-2">
              {t("Accounts in AFK mode")}
            </div>
            {candidates.length === 0 ? (
              <div className="text-[11px] theme-muted leading-4">
                {t("Open an account first: AFK mode only reaches a Roblox client this app opened.")}
              </div>
            ) : (
              <div className="space-y-1">
                {candidates.map((account) => {
                  const picked = inAfk.includes(account.UserID);
                  const row = statusByUserId.get(account.UserID);
                  const name = account.Alias || account.Username;
                  const ownPoint = readAfkPoint(account.Fields);
                  return (
                    <div key={account.UserID}>
                      <button
                        onClick={() => toggleAccount(account.UserID)}
                        disabled={busy}
                        aria-pressed={picked}
                        // O nome acessível é só o da conta: o relógio e o aviso
                        // ao lado mudam a cada segundo e tornariam o botão
                        // impossível de achar por nome.
                        aria-label={account.Alias || account.Username}
                        className={`w-full flex items-center gap-2 px-2 py-1.5 rounded-lg border text-left transition-colors ${
                          picked
                            ? "border-emerald-500/30 bg-emerald-500/10"
                            : "theme-border hover:bg-[var(--panel-soft)]"
                        } disabled:opacity-60`}
                      >
                        <span
                          className={`w-4 h-4 shrink-0 rounded border flex items-center justify-center ${
                            picked ? "border-emerald-400/60 text-emerald-300" : "theme-border"
                          }`}
                        >
                          {picked ? <Check size={11} strokeWidth={3} /> : null}
                        </span>
                        <span className="flex-1 truncate text-[12px] text-[var(--panel-fg)]">
                          {account.Alias || account.Username}
                        </span>
                        {running && picked ? (
                          <span className="text-[11px] font-mono theme-muted shrink-0">
                            {formatCountdown(row?.nextSendAtMs ?? null, nowMs, effectiveInterval * 60_000)}
                          </span>
                        ) : null}
                        {row?.lastErrorCode === "focusDenied" ? (
                          <span className="text-[11px] text-amber-300/90 shrink-0">
                            {t("not sent")}
                          </span>
                        ) : null}
                        {!store.launchedByProgram.has(account.UserID) ? (
                          <span className="text-[11px] text-amber-300/90 shrink-0">
                            {t("no client")}
                          </span>
                        ) : null}
                      </button>
                      {clickMode && picked ? (
                        // Fora do botão da linha: botão dentro de botão não existe.
                        // O ponto próprio é lido a cada ciclo, então muda com a
                        // sessão ligada.
                        <div className="flex items-center gap-2 pl-8 pr-2 py-1 text-[11px] theme-muted">
                          <span className="flex-1 font-mono">
                            {ownPoint ? formatAfkPoint(ownPoint) : t("Default point")}
                          </span>
                          <button
                            onClick={() => startMark(account.UserID)}
                            disabled={capture !== null}
                            aria-label={t("Mark the click point for {{name}}", { name })}
                            className="px-1.5 py-0.5 rounded-md border theme-border hover:bg-[var(--panel-soft)] disabled:opacity-50"
                          >
                            {t("Mark")}
                          </button>
                          {ownPoint ? (
                            <button
                              onClick={() => clearOwnPoint(account.UserID)}
                              aria-label={t("Use the default point for {{name}}", { name })}
                              className="px-1.5 py-0.5 rounded-md border theme-border hover:bg-[var(--panel-soft)]"
                            >
                              {t("Use default")}
                            </button>
                          ) : null}
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            )}
            {running && statusByUserId.size > 0 ? (
              <div className="mt-2 text-[11px] theme-muted leading-4">
                {t("Sends so far: {{count}}", {
                  count: [...statusByUserId.values()].reduce((sum, a) => sum + a.sends, 0),
                })}
              </div>
            ) : null}
            {running
              ? [...statusByUserId.values()]
                  .filter((a) => a.lastError || a.lastErrorCode)
                  .slice(0, 4)
                  .map((a) => {
                    const account = store.accounts.find((it) => it.UserID === a.userId);
                    const name =
                      account?.Alias || account?.Username || `${t("User ID")}: ${a.userId}`;
                    return (
                      <div
                        key={a.userId}
                        className="mt-2 rounded-lg bg-amber-500/10 border border-amber-500/20 px-3 py-2 text-[11px] text-amber-200 break-words"
                      >
                        {sendErrorText(a.lastErrorCode, a.lastError, name)}
                      </div>
                    );
                  })
              : null}
            {running && focusDenied ? (
              <div className="mt-2 text-[11px] theme-muted leading-4">
                {t(
                  "Windows only lets an app change which window is in front in some situations, so the automatic send can be skipped for a while."
                )}{" "}
                {clickMode
                  ? t(
                      "\"Click now\" works because you just clicked this window, and the manager being the window you are using makes the next cycle go through."
                    )
                  : t(
                      "\"Send the key now\" works because you just clicked this window, and the manager being the window you are using makes the next cycle go through."
                    )}
              </div>
            ) : null}
          </section>

          <section className="theme-surface rounded-xl border theme-border p-3">
            {running ? (
              <button
                onClick={handleStop}
                disabled={busy}
                className="sidebar-btn-sm w-full text-red-200 border-red-400/40 hover:bg-red-500/15 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {t("Stop AFK Mode")}
              </button>
            ) : (
              <button
                onClick={handleStart}
                disabled={!canStart}
                className="sidebar-btn-sm w-full disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {t("Start AFK Mode")}
              </button>
            )}
            <button
              onClick={handleSendNow}
              disabled={sendingNow || !running || !sendReady || inAfk.length === 0}
              className="sidebar-btn-sm w-full mt-1.5 flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Send size={13} strokeWidth={1.75} />
              {sendingNow ? t("Sending...") : clickMode ? t("Click now") : t("Send the key now")}
            </button>
            {running ? (
              <div className="mt-2 text-[11px] theme-muted leading-4">
                {t("Running for {{elapsed}}", {
                  elapsed: formatAfkElapsed(status?.startedAtMs ?? null, nowMs),
                })}
              </div>
            ) : null}
            <div className="mt-2 text-[11px] theme-muted leading-4">
              {t("Stopping interrupts a cycle already under way and never closes a client.")}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
