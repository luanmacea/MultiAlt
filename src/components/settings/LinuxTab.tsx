import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Select } from "../ui/Select";
import { TextField } from "../ui/TextField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { useTr } from "../../i18n/text";

/** `[Linux] PreferredRunner` (docs/features/settings.md): quem abre o Roblox. */
export const LINUX_RUNNER_OPTIONS = [
  { value: "sober", label: "Sober (Flatpak)" },
  { value: "custom", label: "Custom command" },
];

/**
 * Settings › Linux — só aparece no Linux (`requires: "linuxSection"` em tabs.tsx).
 *
 * Explica como o Roblox abre lá (Sober, o cliente Android pelo Flatpak) e
 * expõe as chaves `[Linux]` do INI que já existiam sem tela: runner, comando
 * próprio, nomes de processo e pasta de logs. O que fica de fora da interface
 * neste sistema (grade de janelas, Live, AFK, isolamento, versões...) está em
 * "Tela no Linux" de docs/features/linux.md.
 */
export function LinuxTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const runner = s.get("Linux", "PreferredRunner", "sober") || "sober";

  return (
    <div className="space-y-0">
      <div className="mx-1 mt-1 rounded-lg border border-zinc-800/70 bg-zinc-900/35 px-3 py-2.5 text-[12px] leading-relaxed text-zinc-400 space-y-1">
        <div>
          {t("Runs Roblox through Sober (Flatpak). Install Sober and passt, then launch as usual.")}
        </div>
        <div className="text-zinc-500">
          {t("Sober: flatpak install flathub org.vinegarhq.Sober. passt: from your distribution's package manager.")}
        </div>
        <div className="text-zinc-500">
          {t(
            "Pages and options that only work on Windows (window grid, Live, AFK keys and clicks, isolation, Roblox versions, process tuning) are hidden here."
          )}
        </div>
      </div>

      <SectionLabel>Runner</SectionLabel>
      <div className="flex items-center gap-3 py-2 px-1">
        <span className="text-[13px] text-zinc-300 shrink-0">{t("Preferred runner")}</span>
        <Select
          value={runner}
          options={LINUX_RUNNER_OPTIONS}
          onChange={(v) => s.set("Linux", "PreferredRunner", v)}
          className="ml-auto w-48"
          ariaLabel={t("Preferred runner")}
        />
      </div>
      <TextField
        value={s.get("Linux", "CustomLaunchCommand", "")}
        onChange={(v) => s.set("Linux", "CustomLaunchCommand", v)}
        label="Custom launch command"
        placeholder="e.g. flatpak run org.vinegarhq.Sober"
        disabled={runner !== "custom"}
      />
      <div className="px-1 -mt-1 mb-1 text-[12px] text-zinc-500">
        {t("Used when the runner is Custom command. The game link is added at the end.")}
      </div>

      <Divider />
      <SectionLabel>Detection</SectionLabel>
      <TextField
        value={s.get("Linux", "CustomProcessMatch", "")}
        onChange={(v) => s.set("Linux", "CustomProcessMatch", v)}
        label="Process names"
        placeholder="sober,flatpak,roblox,robloxplayerbeta"
      />
      <div className="px-1 -mt-1 mb-1 text-[12px] text-zinc-500">
        {t("Comma-separated words MultiAlt looks for in process names to find a running Roblox.")}
      </div>
      <TextField
        value={s.get("Linux", "CustomLogDir", "")}
        onChange={(v) => s.set("Linux", "CustomLogDir", v)}
        label="Log folder"
        placeholder="Empty = automatic"
      />
      <div className="px-1 -mt-1 mb-1 text-[12px] text-zinc-500">
        {t("Where the Roblox client writes its logs. Leave empty to let MultiAlt find it.")}
      </div>
    </div>
  );
}
