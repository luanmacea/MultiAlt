import { useEffect, useState } from "react";
import { RotateCcw } from "lucide-react";
import { useTr } from "../../../i18n/text";
import {
  RECORDING_SLOTS,
  accountSlotChoice,
  globalRecording,
  slotChoices,
  type AccountSlotChoice,
  type Recording,
  type RecordingSlot,
  type RecordingsPayload,
} from "../../../recordings";

type Translate = ReturnType<typeof useTr>;

/** O nome de cada vaga na tela (as colunas, nesta ordem). */
export function slotLabel(t: Translate, slot: RecordingSlot): string {
  return slot === "afterReconnect" ? t("After auto-reconnect") : t("On anti-AFK");
}

/** Valor do `<select>` de uma conta: herdar, nenhuma ou `rec:<id>`. */
function choiceValue(choice: AccountSlotChoice): string {
  return choice.kind === "recording" ? `rec:${choice.id}` : choice.kind;
}

function choiceFromValue(value: string): AccountSlotChoice {
  if (value === "none") return { kind: "none" };
  if (value.startsWith("rec:")) return { kind: "recording", id: value.slice(4) };
  return { kind: "inherit" };
}

/**
 * O valor mostrado enquanto a escolha vai ao backend: a lista é controlada pelo
 * arquivo, que só volta depois do `recordings-changed`. Sem isto, quem desce
 * as opções com a seta via a lista voltar à escolha antiga entre uma tecla e
 * outra (e a seta seguinte partia dela de novo). Some quando o arquivo traz o
 * valor novo, ou quando a gravação falha.
 */
function usePendingValue(saved: string) {
  const [pending, setPending] = useState<string | null>(null);
  useEffect(() => setPending(null), [saved]);
  const pick = (next: string, write: () => Promise<boolean>) => {
    setPending(next);
    void write().then((ok) => {
      if (!ok) setPending(null);
    });
  };
  return [pending ?? saved, pick] as const;
}

/** O mesmo campo das listas nativas do app (MemoryLimitControl, Servers). */
const FIELD =
  "w-full min-w-0 truncate rounded-md border bg-[var(--panel-bg)] pl-1.5 pr-1 py-1 text-[12px] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent-color)] disabled:opacity-50";

export interface SlotAccount {
  userId: number;
  name: string;
  /** Tem cliente aberto por este app agora. */
  open: boolean;
}

/**
 * "Which recording plays", embaixo do editor: a linha **All accounts** com a
 * gravação global de cada vaga e uma linha por conta (as com cliente aberto
 * pelo app e as que têm escolha própria), cada célula herdando a global
 * (apagada, "Global: …") ou com a escolha da conta (destacada: a própria ou
 * "None for this account"), que vence. ↺ volta a célula para a global.
 *
 * Listas nativas (`<select>`): setas, Home/End e digitar o nome funcionam no
 * teclado sem nada a mais. Estreito, cada conta vira um bloco com o nome da
 * vaga ao lado de cada lista (sem rolagem horizontal).
 */
export function RecordingSlots({
  payload,
  accounts,
  onSetGlobal,
  onSetAccount,
}: {
  payload: RecordingsPayload | null;
  accounts: SlotAccount[];
  /** Grava; `false` quando falhou (a tela já avisou). */
  onSetGlobal: (slot: RecordingSlot, id: string | null) => Promise<boolean>;
  onSetAccount: (slot: RecordingSlot, userId: number, choice: AccountSlotChoice) => Promise<boolean>;
}) {
  const t = useTr();
  const recordings: Recording[] = payload?.recordings ?? [];
  const ROW =
    "grid grid-cols-1 gap-x-2 gap-y-1 @md/slots:grid-cols-[minmax(5.5rem,7.5rem)_minmax(0,1fr)_minmax(0,1fr)] @md/slots:items-center";

  return (
    <section
      className="@container/slots theme-surface rounded-xl border theme-border p-3 space-y-2"
      aria-labelledby="recording-slots-title"
      data-testid="recording-slots"
    >
      <div>
        <h3 id="recording-slots-title" className="text-[13px] font-semibold text-[var(--panel-fg)]">
          {t("Which recording plays")}
        </h3>
        <p className="mt-0.5 text-[11px] leading-4 theme-muted">
          {t("Two separate choices: one plays after an automatic reconnect (to put the character in place), the other on every AFK mode turn.")}
        </p>
      </div>

      {recordings.length === 0 ? (
        <div className="rounded-lg border border-dashed theme-border px-3 py-3 text-[11px] leading-4 theme-muted">
          {t("Create or import a recording first. Then choose here which one plays after an automatic reconnect and which one on anti-AFK.")}
        </div>
      ) : (
        <>
          {/* Cabeçalho das colunas: só com espaço para as três colunas. */}
          <div className={`${ROW} hidden @md/slots:grid`} aria-hidden="true">
            <span />
            {RECORDING_SLOTS.map((slot) => (
              <span key={slot} className="text-[11px] font-semibold uppercase tracking-wide theme-muted">
                {slotLabel(t, slot)}
              </span>
            ))}
          </div>

          {/* A global de cada vaga: toda conta herda. */}
          <div className={`${ROW} rounded-lg bg-[var(--panel-soft)] px-2 py-1.5`} data-testid="recording-slots-global">
            <div className="min-w-0">
              <div className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("All accounts")}</div>
              <div className="text-[11px] theme-muted">{t("Global")}</div>
            </div>
            {RECORDING_SLOTS.map((slot) => (
              <GlobalCell
                key={slot}
                t={t}
                slot={slot}
                saved={slotChoices(payload, slot).defaultId ?? ""}
                recordings={recordings}
                onChange={(id) => onSetGlobal(slot, id)}
              />
            ))}
          </div>

          {accounts.length === 0 ? (
            <div className="px-2 text-[11px] leading-4 theme-muted">
              {t("Accounts with a client open by this app show up here, each with its own choice.")}
            </div>
          ) : (
            <ul className="space-y-1.5 @md/slots:space-y-1" aria-label={t("Accounts")}>
              {accounts.map((account) => (
                <li key={account.userId} className={`${ROW} px-2`} data-testid={`recording-slots-${account.userId}`}>
                  <div className="flex min-w-0 items-center gap-1.5">
                    <span
                      className={`h-1.5 w-1.5 shrink-0 rounded-full ${account.open ? "bg-emerald-400" : "bg-zinc-500"}`}
                      aria-hidden="true"
                    />
                    <span className="truncate text-[12px] text-[var(--panel-fg)]" title={account.name}>
                      {account.name}
                    </span>
                    <span className="sr-only">{account.open ? t("client open") : t("no client open")}</span>
                  </div>
                  {RECORDING_SLOTS.map((slot) => (
                    <SlotCell
                      key={slot}
                      t={t}
                      slot={slot}
                      account={account}
                      choice={accountSlotChoice(payload, slot, account.userId)}
                      globalName={globalRecording(payload, slot)?.name ?? null}
                      recordings={recordings}
                      onChange={(choice) => onSetAccount(slot, account.userId, choice)}
                    />
                  ))}
                </li>
              ))}
            </ul>
          )}

          <p className="flex flex-wrap items-center gap-x-1 text-[11px] leading-4 theme-muted">
            <span>{t("An account's own choice wins over the global one.")}</span>
            <span className="inline-flex items-center gap-1">
              <RotateCcw size={11} strokeWidth={2} aria-hidden="true" />
              {t("goes back to the global one.")}
            </span>
            <span>{t("Listed: accounts with a client open by this app, and the ones with their own choice.")}</span>
          </p>
        </>
      )}
    </section>
  );
}

function GlobalCell({
  t,
  slot,
  saved,
  recordings,
  onChange,
}: {
  t: Translate;
  slot: RecordingSlot;
  saved: string;
  recordings: Recording[];
  onChange: (id: string | null) => Promise<boolean>;
}) {
  const [value, pick] = usePendingValue(saved);
  return (
    <label className="flex min-w-0 items-center gap-2">
      <span className="w-32 shrink-0 text-[11px] theme-muted @md/slots:hidden">{slotLabel(t, slot)}</span>
      <select
        value={value}
        onChange={(e) => {
          const next = e.target.value;
          pick(next, () => onChange(next || null));
        }}
        aria-label={t("Global recording: {{slot}}", { slot: slotLabel(t, slot) })}
        title={recordings.find((r) => r.id === value)?.name ?? t("None")}
        className={`${FIELD} theme-border text-[var(--panel-fg)]`}
      >
        <option value="">{t("None")}</option>
        {recordings.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
          </option>
        ))}
      </select>
    </label>
  );
}

function SlotCell({
  t,
  slot,
  account,
  choice,
  globalName,
  recordings,
  onChange,
}: {
  t: Translate;
  slot: RecordingSlot;
  account: SlotAccount;
  choice: AccountSlotChoice;
  globalName: string | null;
  recordings: Recording[];
  onChange: (choice: AccountSlotChoice) => Promise<boolean>;
}) {
  const [value, pick] = usePendingValue(choiceValue(choice));
  // O que a lista mostra agora (a escolha que acabou de sair já conta).
  const current = choiceFromValue(value);
  const own = current.kind !== "inherit";
  const label = slotLabel(t, slot);
  const inheritLabel = globalName ? t("Global: {{name}}", { name: globalName }) : t("Global: none");
  // O texto inteiro no `title`: num espaço estreito a lista corta o nome.
  const shown =
    current.kind === "inherit"
      ? inheritLabel
      : current.kind === "none"
        ? t("None for this account")
        : recordings.find((r) => r.id === current.id)?.name ?? "";
  return (
    <div className="flex min-w-0 items-center gap-1">
      <span className="w-32 shrink-0 text-[11px] theme-muted @md/slots:hidden" aria-hidden="true">
        {label}
      </span>
      <select
        value={value}
        onChange={(e) => {
          const next = e.target.value;
          pick(next, () => onChange(choiceFromValue(next)));
        }}
        aria-label={t("{{slot}} for {{name}}", { slot: label, name: account.name })}
        data-own={own ? "true" : "false"}
        title={shown}
        className={`${FIELD} ${
          own
            ? "theme-accent-border font-medium text-[var(--panel-fg)]"
            : "theme-border border-dashed text-[var(--panel-muted)]"
        }`}
      >
        <option value="inherit">{inheritLabel}</option>
        <option value="none">{t("None for this account")}</option>
        {recordings.map((r) => (
          <option key={r.id} value={`rec:${r.id}`}>
            {r.name}
          </option>
        ))}
      </select>
      {own ? (
        <button
          type="button"
          onClick={() => pick("inherit", () => onChange({ kind: "inherit" }))}
          aria-label={t("Use global for {{slot}} on {{name}}", { slot: label, name: account.name })}
          title={t("Use global")}
          className="grid h-6 w-6 shrink-0 place-items-center rounded-md border theme-border hover:bg-[var(--panel-soft)] focus:outline-none focus-visible:ring-1 focus-visible:ring-[var(--accent-color)]"
        >
          <RotateCcw size={12} strokeWidth={1.9} aria-hidden="true" />
        </button>
      ) : (
        <span className="h-6 w-6 shrink-0" aria-hidden="true" />
      )}
    </div>
  );
}
