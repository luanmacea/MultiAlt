import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Copy, Loader2, ShieldCheck, SlidersHorizontal, UserPlus } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { NumericInput } from "../ui/NumericInput";
import type { SignupIdentity, SignupStatus } from "../../types";

/**
 * Criação de contas em série no formulário do Roblox.
 *
 * O app abre o navegador embutido, preenche usuário, senha, data de nascimento
 * (18+) e gênero, e **espera o usuário resolver o CAPTCHA e clicar em "Criar
 * conta"**. Quando o cookie aparece, a conta é salva com a senha gerada e o
 * fluxo segue para a próxima.
 *
 * Regras de produto que não podem se perder:
 *
 * - **o app nunca envia o formulário nem tenta passar pela verificação**; a
 *   parte humana é humana de propósito;
 * - a senha gerada aparece na tela e é copiável: é a única cópia dela, e uma
 *   conta cuja senha se perdeu só serve enquanto o cookie durar;
 * - a lista de contas criadas sobrevive ao fim da sessão, para o usuário
 *   conferir o que entrou.
 */

const MAX_ACCOUNTS = 50;

/** Rótulo de cada fase publicada pelo backend. */
export function phaseLabel(phase: string, t: (s: string) => string): string {
  switch (phase) {
    case "opening":
      return t("Opening the browser");
    case "filling":
      return t("Filling the form");
    case "waiting-user":
      return t("Waiting for you to solve the CAPTCHA");
    case "saving":
      return t("Saving the account");
    case "stopping":
      return t("Stopping");
    case "done":
      return t("Finished");
    case "error":
      return t("Error");
    default:
      return t("Idle");
  }
}

interface SignupPanelProps {
  /**
   * Atalho para Settings › General › Login Browser, onde moram os dois ajustes
   * que reduzem o CAPTCHA. Quem hospeda o painel decide como sair dele (o
   * diálogo precisa se fechar antes de abrir as Settings); sem o atalho a dica
   * continua na tela, só sem o botão.
   */
  onOpenLoginSettings?: () => void;
}

export function SignupPanel({ onOpenLoginSettings }: SignupPanelProps = {}) {
  const t = useTr();
  const store = useStore();
  const [count, setCount] = useState(5);
  const [status, setStatus] = useState<SignupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    listen<SignupStatus>("signup-progress", (event) => {
      setStatus(event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    // Estado inicial: a sessão pode já estar rodando de antes de abrir o diálogo.
    invoke<SignupStatus>("get_signup_status")
      .then((s) => {
        if (!disposed) setStatus(s);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const running = status?.active === true;
  const identity = status?.identity ?? null;

  async function handleStart() {
    setBusy(true);
    try {
      const started = await invoke<SignupStatus>("start_signup_session", { count });
      setStatus(started);
    } catch (e) {
      store.addToast(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function handleStop() {
    try {
      await invoke("stop_signup_session");
    } catch (e) {
      store.addToast(String(e));
    }
  }

  function copyIdentity(value: SignupIdentity) {
    const text = `${value.username}\t${value.password}`;
    navigator.clipboard?.writeText(text).then(
      () => store.addToast(t("Username and password copied")),
      () => store.addToast(t("Could not copy"))
    );
  }

  return (
    <div className="h-full flex flex-col gap-3 overflow-y-auto">
      <section className="theme-surface rounded-xl border theme-border p-3">
        <div className="flex items-center gap-2 text-[13px] font-medium text-[var(--panel-fg)]">
          <UserPlus size={14} strokeWidth={1.5} />
          {t("Create accounts in the browser")}
        </div>
        <p className="mt-1 text-[11px] theme-muted">
          {t(
            "The app fills in username, password, birthday (18+) and gender. You solve the CAPTCHA and press Create Account — the app saves each account and moves on to the next."
          )}
        </p>

        <div className="mt-3 flex flex-wrap items-end gap-2">
          <label className="flex flex-col gap-1">
            <span className="text-[10px] theme-muted">{t("How many accounts")}</span>
            <NumericInput
              value={count}
              min={1}
              max={MAX_ACCOUNTS}
              disabled={running}
              onChange={setCount}
            />
          </label>

          {running ? (
            <button
              onClick={() => void handleStop()}
              className="px-3 py-1.5 text-[12px] rounded-md border border-red-500/40 bg-red-500/10 text-red-300"
            >
              {t("Stop")}
            </button>
          ) : (
            <button
              onClick={() => void handleStart()}
              disabled={busy}
              className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] rounded-md theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-50"
            >
              {busy ? <Loader2 size={12} className="animate-spin" /> : <UserPlus size={12} />}
              {t("Start")}
            </button>
          )}
        </div>

        {/* O CAPTCHA é a parte mais cara deste fluxo e os dois ajustes que o
            reduzem moram longe daqui (Settings › General › Login Browser).
            Referência, não cópia: os toggles continuam morando lá. */}
        <div className="mt-3 flex flex-wrap items-center gap-2 rounded-lg border theme-border theme-soft px-2.5 py-2">
          <p className="min-w-0 flex-1 text-[11px] theme-muted">
            {t(
              "Getting a lot of CAPTCHAs? Settings › General › Login Browser has Persistent login profile and Reduce automation signals, which make this browser look less automated."
            )}
          </p>
          {onOpenLoginSettings && (
            <button
              onClick={onOpenLoginSettings}
              className="shrink-0 flex items-center gap-1.5 px-2 py-1 text-[11px] rounded-md theme-btn-ghost border theme-border theme-muted"
            >
              <SlidersHorizontal size={11} strokeWidth={1.5} />
              {t("Open login settings")}
            </button>
          )}
        </div>
      </section>

      {status && (
        <section className="theme-surface rounded-xl border theme-border p-3">
          <div className="flex items-center justify-between text-[12px]">
            <span className="text-[var(--panel-fg)]">{phaseLabel(status.phase, t)}</span>
            <span className="theme-muted tabular-nums">
              {t("{{done}} of {{total}} created", { done: status.created, total: status.total })}
            </span>
          </div>

          {status.phase === "waiting-user" && (
            <p className="mt-2 flex items-center gap-1.5 text-[11px] text-amber-300">
              <ShieldCheck size={12} strokeWidth={1.5} />
              {t("Solve the CAPTCHA in the browser window and press Create Account.")}
            </p>
          )}

          {identity && (
            <div className="mt-2 rounded-lg border theme-border theme-soft p-2.5 text-[11px]">
              <div className="flex items-center justify-between gap-2">
                <span className="theme-muted">{t("Account being created")}</span>
                <button
                  onClick={() => copyIdentity(identity)}
                  className="flex items-center gap-1 px-1.5 py-0.5 rounded theme-btn-ghost border theme-border theme-muted"
                >
                  <Copy size={11} strokeWidth={1.5} />
                  {t("Copy")}
                </button>
              </div>
              <dl className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-[var(--panel-fg)]">
                <dt className="theme-muted font-sans">{t("Username")}</dt>
                <dd>{identity.username}</dd>
                <dt className="theme-muted font-sans">{t("Password")}</dt>
                <dd>{identity.password}</dd>
                <dt className="theme-muted font-sans">{t("Birthday")}</dt>
                <dd>
                  {identity.day}/{identity.month}/{identity.year}
                </dd>
              </dl>
            </div>
          )}

          {status.lastError && (
            <p className="mt-2 text-[11px] text-red-400">{status.lastError}</p>
          )}

          {status.createdUsernames.length > 0 && (
            <div className="mt-2">
              <div className="text-[10px] theme-muted mb-1">{t("Created in this session")}</div>
              <ul className="space-y-0.5 text-[11px] font-mono text-[var(--panel-fg)]">
                {status.createdUsernames.map((name) => (
                  <li key={name}>{name}</li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}
    </div>
  );
}
