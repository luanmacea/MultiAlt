import { useState } from "react";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { Select } from "../ui/Select";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { normalizeUpdaterReleaseChannel, normalizeUpdaterFeatureChannel } from "../../updaterChannels";
import { ActionCard } from "./settingsParts";

/** Settings › Updates: o updater do app (saiu de General em 11/10/2026). */
export function UpdatesTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const store = useStore();
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const updaterReleaseChannel = normalizeUpdaterReleaseChannel(s.get("General", "UpdaterReleaseChannel", "beta"));
  const updaterFeatureChannel = normalizeUpdaterFeatureChannel(
    s.get("General", "UpdaterFeatureChannel", "standard")
  );

  const handleManualUpdateCheck = async () => {
    if (checkingUpdate) return;
    setCheckingUpdate(true);
    try {
      await store.checkForUpdates(true, {
        releaseChannel: updaterReleaseChannel,
        featureChannel: updaterFeatureChannel,
      });
    } finally {
      setCheckingUpdate(false);
    }
  };

  return (
    <div className="space-y-0">
      <Toggle
        checked={s.getBool("General", "CheckForUpdates")}
        onChange={(v) => s.setBool("General", "CheckForUpdates", v)}
        label="Auto Check for Updates"
        description="Automatically check for new versions on launch"
      />

      <div className="flex items-center gap-3 py-2 px-1">
        <div className="min-w-0">
          <div className="text-[13px] text-zinc-300">{t("Update Release Channel")}</div>
          <div className="mt-0.5 text-[12px] text-zinc-500">{t("Pick which release stream is used by the updater")}</div>
        </div>
        <div className="ml-auto min-w-[180px]">
          <Select
            value={updaterReleaseChannel}
            options={[
              { value: "beta", label: "Beta" },
              { value: "stable", label: "Stable" },
            ]}
            onChange={(value) => s.set("General", "UpdaterReleaseChannel", normalizeUpdaterReleaseChannel(value))}
          />
        </div>
      </div>

      <div className="flex items-center gap-3 py-2 px-1">
        <div className="min-w-0">
          <div className="text-[13px] text-zinc-300">{t("Update Feature Channel")}</div>
          <div className="mt-0.5 text-[12px] text-zinc-500">
            {t("Choose whether updates use the standard or Nexus/WebServer build")}
          </div>
        </div>
        <div className="ml-auto min-w-[220px]">
          <Select
            value={updaterFeatureChannel}
            options={[
              { value: "standard", label: "Standard (Non-Nexus/WebServer)" },
              { value: "nexus-ws", label: "Nexus + WebServer" },
            ]}
            onChange={(value) => s.set("General", "UpdaterFeatureChannel", normalizeUpdaterFeatureChannel(value))}
          />
        </div>
      </div>

      <ActionCard
        title={t("Manual Update Check")}
        description={t("Run an update check immediately")}
        action={checkingUpdate ? t("Checking...") : t("Check Now")}
        onAction={() => {
          void handleManualUpdateCheck();
        }}
        disabled={checkingUpdate}
      />
    </div>
  );
}
