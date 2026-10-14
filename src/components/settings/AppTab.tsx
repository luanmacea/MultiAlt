import { enable, disable } from "@tauri-apps/plugin-autostart";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { TextField } from "../ui/TextField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { Select } from "../ui/Select";
import i18n, { LANGUAGE_OPTIONS, normalizeLanguage } from "../../i18n";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { UI_SCALE_FIXED_MAX, UI_SCALE_FIXED_MIN, UI_SCALE_SETTINGS, clampUiScalePercent, normalizeUiScaleSetting } from "../../uiScale";
import { NumericInput } from "../ui/NumericInput";
import { announceUiScale, useAppliedUiScale } from "../../hooks/useUiScale";
import { ActionCard, isWindowsUserAgent } from "./settingsParts";

/**
 * Settings › App: o próprio MultiAlt — idioma, tamanho, janela, listas de
 * recentes e o walkthrough. Saiu de General em 11/10/2026.
 */
export function AppTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const store = useStore();
  const isWindows = isWindowsUserAgent();
  // Tamanho da interface: no automático, mostra o zoom que ele escolheu agora
  // ("Automatic (170%)" numa janela grande do 4K). Ver uiScale.ts.
  const uiScaleSetting = normalizeUiScaleSetting(s.get("General", "InterfaceScale", "auto"));
  const appliedUiScale = useAppliedUiScale();
  const autoScaleLabel =
    uiScaleSetting === "auto" && appliedUiScale !== null
      ? t("Automatic ({{percent}}%)", { percent: Math.round(appliedUiScale * 100) })
      : "Automatic";
  // O campo personalizado mostra o valor em vigor (no automático, o que ele escolheu).
  const customPercent =
    uiScaleSetting !== "auto" ? Number(uiScaleSetting) : Math.round((appliedUiScale ?? 1) * 100);
  const saveUiScale = (value: string) => {
    const next = normalizeUiScaleSetting(value);
    if (next === uiScaleSetting) return;
    s.set("General", "InterfaceScale", next);
    announceUiScale(next);
  };

  return (
    <div className="space-y-0">
      <div className="flex items-center gap-3 py-2 px-1">
        <span className="text-[13px] text-zinc-300 shrink-0">{t("Language")}</span>
        <div className="ml-auto min-w-[180px]">
          <Select
            value={normalizeLanguage(s.get("General", "Language", "en"))}
            options={LANGUAGE_OPTIONS}
            onChange={(value) => {
              const next = normalizeLanguage(value);
              s.set("General", "Language", next);
              void i18n.changeLanguage(next);
            }}
          />
        </div>
      </div>

      <div className="flex items-center gap-3 py-2 px-1">
        <div className="min-w-0">
          <div className="text-[13px] text-zinc-300">{t("Interface size")}</div>
          <div className="mt-0.5 text-[12px] text-zinc-500">
            {t("Automatic follows the screen: smaller on small screens, larger in big windows. Or type any size from 80% to 200%.")}
          </div>
        </div>
        <div className="ml-auto min-w-[180px]">
          <Select
            ariaLabel="Interface size"
            value={uiScaleSetting}
            options={[
              { value: "auto", label: autoScaleLabel },
              ...UI_SCALE_SETTINGS.filter((v) => v !== "auto").map((v) => ({ value: v, label: `${v}%` })),
              // Valor personalizado (digitado no campo ao lado) também aparece.
              ...(uiScaleSetting !== "auto" && !(UI_SCALE_SETTINGS as readonly string[]).includes(uiScaleSetting)
                ? [{ value: uiScaleSetting, label: `${uiScaleSetting}%` }]
                : []),
            ]}
            onChange={(value) => saveUiScale(value)}
          />
        </div>
        {/* Personalizado: monitor muito grande pede valores fora da lista. */}
        <div className="w-[76px] shrink-0" title={t("Custom interface size (%)")}>
          <NumericInput
            ariaLabel="Custom interface size (%)"
            value={customPercent}
            onChange={() => {}}
            onCommit={(value) => saveUiScale(String(clampUiScalePercent(value)))}
            min={UI_SCALE_FIXED_MIN}
            max={UI_SCALE_FIXED_MAX}
            integer
            className="sidebar-input w-full text-right tabular-nums"
          />
        </div>
      </div>

      <Divider />
      <SectionLabel>Window</SectionLabel>
      {isWindows && (
        <Toggle
          checked={s.getBool("General", "ThemeWindowsNavbar")}
          onChange={(v) => s.setBool("General", "ThemeWindowsNavbar", v)}
          label="Theme Windows window navbar"
          description="Makes the top window navbar follow your active MultiAlt theme"
        />
      )}
      <Toggle
        checked={s.getBool("General", "StartOnPCStartup")}
        onChange={(v) => {
          s.setBool("General", "StartOnPCStartup", v);
          (v ? enable() : disable()).catch(() => {});
        }}
        label="Run on Windows Startup"
      />
      <Toggle
        checked={s.getBool("General", "MinimizeToTray")}
        onChange={(v) => s.setBool("General", "MinimizeToTray", v)}
        label="Minimize to Tray"
        description="Close button hides to system tray instead of exiting"
      />

      <Divider />
      <SectionLabel>Recent games and servers</SectionLabel>
      {/* Corta a lista local de recentes (`addRecentGame`,
          server-list/types.ts) — nada a ver com o historico do Roblox. */}
      <NumberField
        value={s.getNumber("General", "MaxRecentGames", 8)}
        onChange={(v) => s.setNumber("General", "MaxRecentGames", v)}
        label="Max Recent Games"
        description="How many games the Recent list keeps before the oldest one drops off."
        min={1}
        max={30}
      />
      {/* Corta a lista de servidores recentes (`addRecentJob`,
          server-list/types.ts), que fica ao lado dos jogos na aba Recent. */}
      <NumberField
        value={s.getNumber("General", "MaxRecentJobs", 12)}
        onChange={(v) => s.setNumber("General", "MaxRecentJobs", v)}
        label="Max Recent Servers"
        description="How many servers (Job IDs) the Recent list keeps before the oldest one drops off."
        min={1}
        max={50}
      />
      <TextField
        value={s.get("General", "ServerRegionFormat", "<city>, <countryCode>")}
        onChange={(v) => s.set("General", "ServerRegionFormat", v)}
        label="Region Format"
        placeholder="<city>, <countryCode>"
      />
      {/* O campo pedia um template sem dizer que tokens existem. Os cinco
          abaixo sao exatamente os que `format_region`
          (api/roblox/server_regions.rs) substitui; o resto do texto passa
          intacto, e template que nao resolve nada cai no IP cru.
          `TextField` nao tem `description`, entao a ajuda fica ao lado. */}
      <div className="px-1 -mt-1 mb-1 space-y-0.5">
        <div className="text-[12px] text-zinc-500">
          {t("Tokens: <city>, <region>, <country>, <countryCode>, <ip>. Any other text is kept as typed.")}
        </div>
        <div className="text-[12px] text-zinc-500">
          {t(
            'Example: "<city>, <countryCode>" shows as "Ashburn, US" in the server list. A template that fills in empty falls back to the raw IP.'
          )}
        </div>
      </div>

      <Divider />
      <SectionLabel>Help</SectionLabel>
      <ActionCard
        title={t("First-Time Walkthrough")}
        description={t("Replay the guided setup tour with launch and safety essentials")}
        action={t("Open Walkthrough")}
        onAction={store.openFirstRunWalkthroughFromSettings}
      />
    </div>
  );
}
