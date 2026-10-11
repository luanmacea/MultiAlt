import { MemoryStick } from "lucide-react";
import { usePrompt } from "../../hooks/usePrompt";
import { useTranslation } from "react-i18next";
import { useTr } from "../../i18n/text";
import type { ClientMemory } from "../../types";
import {
  MEMORY_LIMIT_PRESETS_MB,
  formatMemoryMb,
  formatMemoryReading,
  parseCustomMemoryLimit,
  parseMemoryLimit,
  type MemoryLimitChoice,
} from "../../utils/memoryLimit";
import { ChipMenu, type ChipMenuOption } from "./ChipMenu";

/**
 * Teto de memória de um cliente, num chip da linha da lista "Em jogo" (Painel
 * de Sessão): a memória privada de agora ("1,187 MB", o número da coluna
 * "Memória" do Gerenciador de Tarefas; âmbar acima do limite) e o limite
 * que vale; o menu muda o limite — padrão, sem limite, os tamanhos comuns, o
 * próprio da conta e "Custom…". Grava no campo `MemoryLimit` da conta (vale na
 * hora, sem relançar, e nos próximos launches). Passou do limite, o app libera a
 * memória primeiro; fecha só com a opção própria
 * `Optimization.CloseOverMemoryLimit` (desligada por padrão).
 *
 * Até 11/10/2026 era um seletor nativo de 104 px ao lado da leitura: cobria a
 * leitura e o aviso de acima do limite, e espremia o resto da linha.
 */

/** `null` = volta ao padrão; `0` = sem limite; MB. */
export type MemoryLimitValue = number | null;

type Translate = ReturnType<typeof useTr>;

function limitText(mb: number, t: Translate, lang: string): string {
  return mb > 0 ? formatMemoryMb(mb, lang) : t("no limit");
}

/** As opções do menu. `current`: o valor escolhido (`undefined` no lote, onde não há um só). */
function limitOptions(
  defaultMb: number,
  own: number | null,
  current: MemoryLimitValue | undefined,
  t: Translate,
  lang: string
): ChipMenuOption[] {
  const custom = own !== null && own > 0 && !MEMORY_LIMIT_PRESETS_MB.includes(own) ? own : null;
  return [
    {
      value: "default",
      label: t("Default ({{limit}})", { limit: limitText(defaultMb, t, lang) }),
      checked: current === null,
    },
    { value: "0", label: t("No limit"), checked: current === 0 },
    ...MEMORY_LIMIT_PRESETS_MB.map((mb) => ({ value: String(mb), label: formatMemoryMb(mb, lang), checked: current === mb })),
    ...(custom !== null ? [{ value: String(custom), label: formatMemoryMb(custom, lang), checked: current === custom }] : []),
    { value: "custom", label: t("Custom…"), separated: true },
  ];
}

/** Lê a escolha do menu; "Custom…" pergunta em MB ou GB. `undefined` = nada a gravar. */
function usePickLimit() {
  const t = useTr();
  const prompt = usePrompt();
  return async (raw: string): Promise<MemoryLimitValue | undefined> => {
    if (raw === "default") return null;
    if (raw === "custom") {
      const typed = await prompt(t("Memory limit for this client, in MB or GB (for example 1800 or 2.5 GB)"), "");
      if (typed === null) return undefined;
      const mb = parseCustomMemoryLimit(typed);
      if (mb === null) return undefined;
      return parseMemoryLimit(String(mb)) ?? undefined;
    }
    return parseMemoryLimit(raw) ?? undefined;
  };
}

export function MemoryChip({
  name,
  choice,
  memory,
  disabled = false,
  onChange,
}: {
  name: string;
  choice: MemoryLimitChoice;
  memory: ClientMemory | undefined;
  disabled?: boolean;
  onChange: (value: MemoryLimitValue) => void;
}) {
  const t = useTr();
  const lang = useTranslation().i18n.language;
  const pick = usePickLimit();
  const { own, defaultMb, effectiveMb } = choice;
  const limit = limitText(effectiveMb, t, lang);
  // MB inteiros, o mesmo número da coluna "Memória" do Gerenciador de Tarefas.
  const reading = memory && memory.memoryMb !== null ? formatMemoryReading(memory.memoryMb, lang) : null;
  const over = memory?.over === true;
  const limitSource =
    own === null
      ? t("Following the default ({{limit}}).", { limit: limitText(defaultMb, t, lang) })
      : t("Set for this account: {{limit}}.", { limit });
  // O "acima do limite" é do backend: vale o limite que ele mediu.
  const measuredLimit = memory?.limitMb ? formatMemoryMb(memory.limitMb, lang) : limit;
  const usage = reading
    ? over
      ? memory?.trimmedAtMs !== null
        ? t("Over its limit of {{limit}}: MultiAlt asked Windows to free this client's memory.", { limit: measuredLimit })
        : t("Over its limit of {{limit}}.", { limit: measuredLimit })
      : t("Private memory now: {{memory}} (the Memory column in Task Manager).", { memory: reading })
    : null;
  // Liberou e caiu para baixo do limite: fica mais baixa até o jogo precisar
  // de novo, e isso é normal (não é a leitura que parou).
  const afterTrim =
    reading && !over && memory?.trimmedAtMs != null
      ? t("MultiAlt freed this client's memory: it stays lower until the game needs it again, which is expected.")
      : null;
  const title = [usage, afterTrim, limitSource, t("Above the limit, MultiAlt first asks Windows to free the client's memory.")]
    .filter(Boolean)
    .join(" ");
  return (
    <ChipMenu
      ariaLabel={`${t("Memory limit for {{name}}", { name })}: ${own === null ? t("default, {{limit}}", { limit }) : limit}`}
      title={title}
      menuLabel={t("Memory limit for {{name}}", { name })}
      options={limitOptions(defaultMb, own, own, t, lang)}
      onSelect={(raw) =>
        void pick(raw).then((value) => {
          if (value !== undefined) onChange(value);
        })
      }
      disabled={disabled}
      tone={over ? "warn" : "default"}
      testId="memory-chip"
    >
      <MemoryStick size={11} strokeWidth={1.75} aria-hidden="true" className={over ? undefined : "theme-muted"} />
      {reading ? (
        <>
          <span data-testid="memory-reading">{reading}</span>
          <span className={over ? undefined : "theme-muted"}>/ {effectiveMb > 0 ? limit : "∞"}</span>
        </>
      ) : (
        <span className={own === null ? "theme-muted" : undefined}>{effectiveMb > 0 ? limit : "∞"}</span>
      )}
    </ChipMenu>
  );
}

/** O mesmo menu para as contas marcadas (barra de lote). */
export function MemoryBulkChip({
  count,
  defaultMb,
  disabled = false,
  onChange,
}: {
  count: number;
  defaultMb: number;
  disabled?: boolean;
  onChange: (value: MemoryLimitValue) => void;
}) {
  const t = useTr();
  const lang = useTranslation().i18n.language;
  const pick = usePickLimit();
  const label = count === 1 ? t("Memory limit for 1 selected") : t("Memory limit for {{count}} selected", { count });
  return (
    <ChipMenu
      ariaLabel={label}
      title={t("Above the limit, MultiAlt first asks Windows to free the client's memory.")}
      menuLabel={label}
      options={limitOptions(defaultMb, null, undefined, t, lang)}
      onSelect={(raw) =>
        void pick(raw).then((value) => {
          if (value !== undefined) onChange(value);
        })
      }
      disabled={disabled}
    >
      <MemoryStick size={11} strokeWidth={1.75} aria-hidden="true" />
      {t("Memory limit")}
    </ChipMenu>
  );
}

/**
 * O padrão de todos os clientes (`Optimization.MemoryLimit`), no resumo da
 * página Session: sem limite, os tamanhos comuns, o valor atual se for outro e
 * "Custom…". `onChange` recebe MB (0 = sem limite).
 */
export function MemoryDefaultSelect({
  valueMb,
  onChange,
}: {
  valueMb: number;
  onChange: (mb: number) => void;
}) {
  const t = useTr();
  const lang = useTranslation().i18n.language;
  const prompt = usePrompt();
  const custom = valueMb > 0 && !MEMORY_LIMIT_PRESETS_MB.includes(valueMb) ? valueMb : null;

  async function pick(raw: string) {
    if (raw === "custom") {
      const typed = await prompt(t("Memory limit for each client, in MB or GB (for example 1800 or 2.5 GB)"), "");
      if (typed === null) return;
      const mb = parseCustomMemoryLimit(typed);
      const limit = mb === null ? null : parseMemoryLimit(String(mb));
      if (limit !== null) onChange(limit);
      return;
    }
    const mb = parseMemoryLimit(raw);
    if (mb !== null) onChange(mb);
  }

  return (
    <select
      aria-label={t("Memory limit for every client")}
      value={String(valueMb)}
      onChange={(e) => void pick(e.target.value)}
      className="rounded-md border theme-border bg-[var(--panel-bg)] px-2 py-1 text-[12px] text-[var(--panel-fg)]"
    >
      <option value="0">{t("No limit")}</option>
      {MEMORY_LIMIT_PRESETS_MB.map((mb) => (
        <option key={mb} value={String(mb)}>
          {formatMemoryMb(mb, lang)}
        </option>
      ))}
      {custom !== null && <option value={String(custom)}>{formatMemoryMb(custom, lang)}</option>}
      <option value="custom">{t("Custom…")}</option>
    </select>
  );
}
