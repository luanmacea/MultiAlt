import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Copy, Eye, EyeOff, Loader2, ShieldCheck, SlidersHorizontal, UserPlus } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { chromiumErrorText } from "../../utils/chromiumErrors";
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
/** O mínimo que o Roblox aceita numa senha (`MIN_CUSTOM_PASSWORD` no backend). */
export const MIN_CHOSEN_PASSWORD = 8;
/** Maior número que cabe em `prefixo_N` (20 caracteres com o prefixo de 14). */
const MAX_NAME_NUMBER = 99_999;

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
   * Atalho para Settings › Accounts › Login browser, onde moram os dois ajustes
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
  /**
   * Padrão do nome das contas criadas: "arvore" gera "arvore_k3p9z".
   *
   * Guardado em `Generator.SignupUsernamePrefix` para valer no próximo lote sem
   * ser digitado de novo. Só letras e dígitos: o underscore que separa o
   * prefixo do sorteio já gasta o **único** que o Roblox aceita num nome, e o
   * teto de 14 vem de 20 (limite do Roblox) − 1 underscore − 5 sorteados.
   */
  const [usernamePrefix, setUsernamePrefix] = useState("");
  /**
   * `Generator.SignupUsernameNumbered` / `SignupUsernameNext`: com prefixo, os
   * nomes saem em ordem (`arvore_1`, `arvore_2`...) em vez de sorteados (pedido
   * do Discord, 30/10/2026). O backend grava o próximo número a cada conta.
   */
  const [numbered, setNumbered] = useState(false);
  const [nextNumber, setNextNumber] = useState(1);
  /**
   * Senha de todas as contas do lote (vazia = uma sorteada por conta). Fica só
   * aqui e na memória da sessão: não vai para o RAMSettings.ini, que é texto
   * puro. A senha guardada de cada conta continua sendo a do campo do Roblox
   * quando o cadastro deu certo.
   */
  const [chosenPassword, setChosenPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  /**
   * `Generator.SignupAutoSubmit`: o app clica em "Cadastrar-se" sozinho (pedido
   * do dono, 14/10/2026). Ligado por padrão; o CAPTCHA continua da pessoa.
   */
  const [autoSubmit, setAutoSubmit] = useState(true);
  const [status, setStatus] = useState<SignupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // O prefixo salvo aparece preenchido ao abrir.
  useEffect(() => {
    invoke<Record<string, Record<string, string>>>("get_all_settings")
      .then((all) => {
        if (!mounted.current) return;
        setUsernamePrefix(all?.Generator?.SignupUsernamePrefix ?? "");
        setAutoSubmit(all?.Generator?.SignupAutoSubmit !== "false");
        setNumbered(all?.Generator?.SignupUsernameNumbered === "true");
        setNextNumber(readNextNumber(all?.Generator?.SignupUsernameNext));
      })
      .catch(() => {});
  }, []);

  // Cada conta salva avança o número no backend: o campo acompanha.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    listen("generator-account-added", () => {
      invoke<Record<string, Record<string, string>>>("get_all_settings")
        .then((all) => {
          if (mounted.current) setNextNumber(readNextNumber(all?.Generator?.SignupUsernameNext));
        })
        .catch(() => {});
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
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

  /** Grava o prefixo; o backend lê essa chave ao montar cada nome. */
  async function persistPrefix(value: string) {
    try {
      await invoke("update_setting", {
        section: "Generator",
        key: "SignupUsernamePrefix",
        value,
      });
    } catch (e) {
      store.addToast(String(e));
    }
  }

  async function saveGeneratorSetting(key: string, value: string) {
    try {
      await invoke("update_setting", { section: "Generator", key, value });
    } catch (e) {
      store.addToast(String(e));
    }
  }

  async function changeNumbered(value: boolean) {
    setNumbered(value);
    await saveGeneratorSetting("SignupUsernameNumbered", value ? "true" : "false");
  }

  async function changeNextNumber(value: number) {
    setNextNumber(value);
    await saveGeneratorSetting("SignupUsernameNext", String(value));
  }

  async function changeAutoSubmit(value: boolean) {
    setAutoSubmit(value);
    try {
      await invoke("update_setting", { section: "Generator", key: "SignupAutoSubmit", value: value ? "true" : "false" });
    } catch (e) {
      store.addToast(String(e));
    }
  }

  async function handleStart() {
    if (chosenPassword && chosenPassword.length < MIN_CHOSEN_PASSWORD) {
      store.addToast(t("The password needs at least {{count}} characters.", { count: MIN_CHOSEN_PASSWORD }));
      return;
    }
    setBusy(true);
    try {
      // Grava antes de começar: o nome do primeiro lote já sai no padrão.
      await persistPrefix(usernamePrefix);
      const started = await invoke<SignupStatus>("start_signup_session", {
        count,
        password: chosenPassword || null,
      });
      setStatus(started);
    } catch (e) {
      store.addToast(chromiumErrorText(e, t));
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

  /**
   * Não passa pelo aviso de copiar credencial (`useCopyCredentialWarning`), de
   * propósito: é **uma** conta, que está sendo criada nesta tela, e o painel já
   * mostra usuário e senha em texto logo abaixo. O aviso existe para a cópia em
   * lote que a pessoa não vê acontecer — aqui ele seria só ruído.
   */
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
        {/* Mesmo nome e mesma linha de apoio do menu Add, do AddAccountDialog e
            do GeneratorDialog: a função gratuita tinha quatro nomes na
            interface, e este era mais um deles. */}
        <div className="flex items-center gap-2 text-[13px] font-medium text-[var(--panel-fg)]">
          <UserPlus size={14} strokeWidth={1.5} />
          {t("Create Accounts")}
        </div>
        <p className="mt-0.5 text-[12px] theme-muted">
          {t("Free — the app fills Roblox's signup form; you solve the CAPTCHA")}
        </p>
        <p className="mt-1 text-[12px] theme-muted">
          {autoSubmit
            ? t(
                "The app fills in username, password, birthday (18+) and gender and presses Create Account. If Roblox shows a CAPTCHA, you solve it — the app saves each account and moves on to the next."
              )
            : t(
                "The app fills in username, password, birthday (18+) and gender. You solve the CAPTCHA and press Create Account — the app saves each account and moves on to the next."
              )}
        </p>

        <label className="mt-2 flex items-center gap-2 text-[12px] text-[var(--panel-fg)] cursor-pointer select-none">
          <input
            type="checkbox"
            checked={autoSubmit}
            disabled={running}
            onChange={(e) => void changeAutoSubmit(e.target.checked)}
            className="accent-[var(--accent-color)]"
          />
          {t("Press Create Account by itself")}
          <span className="theme-muted">{t("(the CAPTCHA stays with you)")}</span>
        </label>

        <div className="mt-3 flex flex-wrap items-end gap-2">
          <label className="flex flex-col gap-1">
            <span className="text-[11px] theme-muted">{t("How many accounts")}</span>
            <NumericInput
              value={count}
              min={1}
              max={MAX_ACCOUNTS}
              disabled={running}
              onChange={setCount}
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-[11px] theme-muted">{t("Name prefix (optional)")}</span>
            <input
              type="text"
              value={usernamePrefix}
              // Filtra na digitação: o que o Roblox recusa nem chega ao campo.
              onChange={(e) => setUsernamePrefix(e.target.value.replace(/[^A-Za-z0-9]/g, ""))}
              onBlur={() => void persistPrefix(usernamePrefix)}
              disabled={running}
              maxLength={14}
              aria-label={t("Name prefix (optional)")}
              title={t("Only letters and digits. Each account becomes prefix_ plus 5 random characters.")}
              className="w-[132px] px-2.5 py-1 rounded-md text-[12px] bg-[var(--forms-bg)] border theme-border text-[var(--panel-fg)] placeholder-[var(--panel-muted)] focus:outline-none disabled:opacity-50"
            />
          </label>

          {/* div e não label: o label envolveria também o botão do olho. */}
          <div className="flex flex-col gap-1">
            <span className="text-[11px] theme-muted">{t("Password for every account (optional)")}</span>
            <span className="relative flex items-center">
              <input
                type={showPassword ? "text" : "password"}
                value={chosenPassword}
                onChange={(e) => setChosenPassword(e.target.value)}
                disabled={running}
                autoComplete="new-password"
                aria-label={t("Password for every account (optional)")}
                title={t(
                  "Empty: a random password for each account. At least 8 characters, without the username. It is kept only in each account."
                )}
                className="w-[150px] pl-2.5 pr-7 py-1 rounded-md text-[12px] bg-[var(--forms-bg)] border theme-border text-[var(--panel-fg)] placeholder-[var(--panel-muted)] focus:outline-none disabled:opacity-50"
              />
              <button
                type="button"
                onClick={() => setShowPassword((v) => !v)}
                aria-label={showPassword ? t("Hide password") : t("Show password")}
                className="absolute right-1.5 theme-muted"
              >
                {showPassword ? <EyeOff size={12} /> : <Eye size={12} />}
              </button>
            </span>
          </div>

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

        <div className="mt-2 flex flex-wrap items-center gap-2 text-[12px] text-[var(--panel-fg)]">
          <label
            className="flex items-center gap-2 cursor-pointer select-none"
            title={usernamePrefix ? undefined : t("Type a name prefix first.")}
          >
            <input
              type="checkbox"
              checked={numbered && !!usernamePrefix}
              disabled={running || !usernamePrefix}
              onChange={(e) => void changeNumbered(e.target.checked)}
              className="accent-[var(--accent-color)]"
            />
            {t("Number the names in order")}
            <span className="theme-muted">
              {t("({{example}} instead of random characters)", {
                example: `${usernamePrefix || "prefix"}_${nextNumber}`,
              })}
            </span>
          </label>
          {numbered && !!usernamePrefix && (
            <label className="flex items-center gap-1.5">
              <span className="text-[11px] theme-muted">{t("Next number")}</span>
              <NumericInput
                value={nextNumber}
                min={1}
                max={MAX_NAME_NUMBER}
                disabled={running}
                onChange={(value) => void changeNextNumber(value)}
              />
            </label>
          )}
        </div>

        {/* O CAPTCHA é a parte mais cara deste fluxo e os dois ajustes que o
            reduzem moram longe daqui (Settings › Accounts › Login browser).
            Referência, não cópia: os toggles continuam morando lá. */}
        <div className="mt-3 flex flex-wrap items-center gap-2 rounded-lg border theme-border theme-soft px-2.5 py-2">
          <p className="min-w-0 flex-1 text-[12px] theme-muted">
            {t(
              "Getting a lot of CAPTCHAs? Settings › Accounts › Login browser has Keep browser data between uses and Reduce automation signals, which make this browser look less automated."
            )}
          </p>
          {onOpenLoginSettings && (
            <button
              onClick={onOpenLoginSettings}
              className="shrink-0 flex items-center gap-1.5 px-2 py-1 text-[12px] rounded-md theme-btn-ghost border theme-border theme-muted"
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
            <p className="mt-2 flex items-center gap-1.5 text-[12px] text-amber-300">
              <ShieldCheck size={12} strokeWidth={1.5} />
              {autoSubmit
                ? t("The app presses Create Account by itself. If Roblox shows a CAPTCHA, solve it in the browser window.")
                : t("Solve the CAPTCHA in the browser window and press Create Account.")}
            </p>
          )}

          {identity && (
            <div className="mt-2 rounded-lg border theme-border theme-soft p-2.5 text-[12px]">
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
            <p className="mt-2 text-[12px] text-red-400">{status.lastError}</p>
          )}

          {status.createdUsernames.length > 0 && (
            <div className="mt-2">
              <div className="text-[11px] theme-muted mb-1">{t("Created in this session")}</div>
              <ul className="space-y-0.5 text-[12px] font-mono text-[var(--panel-fg)]">
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

/** O próximo número gravado, ou 1 quando falta ou é lixo. */
function readNextNumber(value: string | undefined): number {
  const parsed = Number.parseInt(value ?? "", 10);
  return Number.isFinite(parsed) && parsed >= 1 ? Math.min(parsed, MAX_NAME_NUMBER) : 1;
}
