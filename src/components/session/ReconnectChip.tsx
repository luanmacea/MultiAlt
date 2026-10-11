import { Lock, RotateCw } from "lucide-react";
import { useTr } from "../../i18n/text";
import type { ReconnectChoice } from "../../utils/autoReconnect";
import { ChipMenu, type ChipMenuOption } from "./ChipMenu";

/**
 * Reconexão automática de uma conta, num chip da linha da lista "Em jogo"
 * (Painel de Sessão): ↻ Ligada/Desligada, e o menu com Ligada, Desligada e
 * "Usar o padrão". Texto apagado = a conta segue o padrão
 * (`General.AutoReconnect`); cadeado = o AutoRelaunch do Nexus a mantém ligada
 * (o menu não abre). O tooltip diz de onde vem o valor.
 */

/** `null` volta ao padrão de todas as contas. */
type ReconnectValue = boolean | null;

function reconnectOptions(t: ReturnType<typeof useTr>, globalDefault: boolean, own: ReconnectValue | undefined): ChipMenuOption[] {
  return [
    { value: "on", label: t("On"), checked: own === true },
    { value: "off", label: t("Off"), checked: own === false },
    {
      value: "default",
      label: globalDefault ? t("Use the default (on)") : t("Use the default (off)"),
      checked: own === null,
      separated: true,
    },
  ];
}

function toValue(raw: string): ReconnectValue {
  return raw === "on" ? true : raw === "off" ? false : null;
}

export function ReconnectChip({
  name,
  choice,
  disabled = false,
  onChange,
}: {
  name: string;
  choice: ReconnectChoice;
  disabled?: boolean;
  onChange: (value: ReconnectValue) => void;
}) {
  const t = useTr();
  const { own, globalDefault, nexusForced, effective } = choice;
  const title = nexusForced
    ? t("Nexus AutoRelaunch is on for this account, so it reconnects even with this off.")
    : own === null
      ? globalDefault
        ? t("Following the default for all accounts (on).")
        : t("Following the default for all accounts (off).")
      : effective
        ? t("Reconnects if it drops (set for this account)")
        : t("Does not reconnect if it drops (set for this account)");
  const state =
    own === null && !nexusForced
      ? effective
        ? t("on (default)")
        : t("off (default)")
      : effective
        ? t("on")
        : t("off");
  const Icon = nexusForced ? Lock : RotateCw;
  return (
    <ChipMenu
      ariaLabel={`${t("Auto-reconnect for {{name}}", { name })}: ${state}`}
      title={title}
      menuLabel={t("Auto-reconnect for {{name}}", { name })}
      options={reconnectOptions(t, globalDefault, own)}
      onSelect={(raw) => onChange(toValue(raw))}
      disabled={disabled || nexusForced}
      testId="reconnect-chip"
    >
      <Icon
        size={11}
        strokeWidth={1.75}
        aria-hidden="true"
        className={nexusForced ? "text-amber-400" : effective ? "text-emerald-400" : "theme-muted"}
      />
      {/* "On" sozinho não dizia de quê (revisão de design, 11/10/2026). */}
      <span className={own === null && !nexusForced ? "theme-muted" : undefined}>
        {effective ? t("Reconnect: on") : t("Reconnect: off")}
      </span>
    </ChipMenu>
  );
}

/** O mesmo menu para as contas marcadas (barra de lote). */
export function ReconnectBulkChip({
  count,
  globalDefault,
  disabled = false,
  onChange,
}: {
  count: number;
  globalDefault: boolean;
  disabled?: boolean;
  onChange: (value: ReconnectValue) => void;
}) {
  const t = useTr();
  const label =
    count === 1 ? t("Auto-reconnect for 1 selected") : t("Auto-reconnect for {{count}} selected", { count });
  return (
    <ChipMenu
      ariaLabel={label}
      title={label}
      menuLabel={label}
      options={reconnectOptions(t, globalDefault, undefined)}
      onSelect={(raw) => onChange(toValue(raw))}
      disabled={disabled}
    >
      <RotateCw size={11} strokeWidth={1.75} aria-hidden="true" />
      {t("Reconnect")}
    </ChipMenu>
  );
}
