import { useState, useEffect, useRef } from "react";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import type { TabId } from "./tabs";
import { TAB_ORDER } from "./tabs";
import { LaunchTab } from "./LaunchTab";
import { KeepInGameTab } from "./KeepInGameTab";
import { AccountsTab } from "./AccountsTab";
import { PrivacyTab } from "./PrivacyTab";
import { UpdatesTab } from "./UpdatesTab";
import { AppTab } from "./AppTab";
import { DeveloperTab } from "./DeveloperTab";
import { OptimizationTab } from "./OptimizationTab";
import { WebServerTab } from "./WebServerTab";
import { WatcherTab } from "./WatcherTab";
import { GeneratorTab } from "./GeneratorTab";
import { IsolationTab } from "./IsolationTab";
import { VersionsTab } from "./VersionsTab";
import { BackupsTab } from "./BackupsTab";
import { useTr } from "../../i18n/text";

export function TabContent({
  activeTab,
  s,
  loaded,
  onRequestEncryptionSetup,
}: {
  activeTab: TabId;
  s: UseSettingsReturn;
  loaded: boolean;
  onRequestEncryptionSetup?: () => void;
}) {
  const t = useTr();
  const prevTab = useRef(activeTab);
  const [animKey, setAnimKey] = useState(0);
  const [direction, setDirection] = useState<"left" | "right">("right");

  useEffect(() => {
    if (activeTab === prevTab.current) return;
    const prevIdx = TAB_ORDER.indexOf(prevTab.current);
    const nextIdx = TAB_ORDER.indexOf(activeTab);
    setDirection(nextIdx > prevIdx ? "right" : "left");
    setAnimKey((k) => k + 1);
    prevTab.current = activeTab;
  }, [activeTab]);

  if (!loaded) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="text-sm text-zinc-600">{t("Loading settings...")}</div>
      </div>
    );
  }

  const enterClass = direction === "right"
    ? "settings-slide-in-right"
    : "settings-slide-in-left";

  return (
    <div className="grid" style={{ gridTemplate: "1fr / 1fr" }}>
      {TAB_ORDER.map((tab) => {
        const active = tab === activeTab;
        return (
          <div
            key={active ? `${tab}-${animKey}` : tab}
            data-settings-pane={tab}
            className={active ? enterClass : ""}
            style={{
              gridArea: "1 / 1",
              display: active ? "block" : "none",
              pointerEvents: active ? "auto" : "none",
            }}
          >
            {tab === "launch" && <LaunchTab s={s} />}
            {tab === "keep-in-game" && <KeepInGameTab s={s} />}
            {tab === "accounts" && <AccountsTab s={s} />}
            {tab === "privacy" && (
              <PrivacyTab s={s} onRequestEncryptionSetup={onRequestEncryptionSetup} />
            )}
            {tab === "updates" && <UpdatesTab s={s} />}
            {tab === "app" && <AppTab s={s} />}
            {tab === "backups" && <BackupsTab active={active} />}
            {tab === "developer" && <DeveloperTab s={s} />}
            {tab === "webserver" && <WebServerTab s={s} />}
            {tab === "watcher" && <WatcherTab s={s} />}
            {tab === "generator" && <GeneratorTab s={s} />}
            {tab === "isolation" && <IsolationTab s={s} />}
            {tab === "versions" && <VersionsTab s={s} />}
            {tab === "optimization" && <OptimizationTab s={s} />}
          </div>
        );
      })}
    </div>
  );
}
