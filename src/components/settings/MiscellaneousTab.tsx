import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import type { RememberState } from "../../types";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { useTr } from "../../i18n/text";

export function MiscellaneousTab({
  s,
  onRequestEncryptionSetup,
}: {
  s: UseSettingsReturn;
  onRequestEncryptionSetup?: () => void;
}) {
  const t = useTr();
  const [remembered, setRemembered] = useState<RememberState | null>(null);

  useEffect(() => {
    let disposed = false;
    invoke<RememberState>("remembered_unlock_state")
      .then((state) => {
        if (!disposed) setRemembered(state);
      })
      .catch(() => {});
    return () => {
      disposed = true;
    };
  }, []);

  async function forgetRemembered() {
    await invoke("forget_remembered_unlock").catch(() => {});
    setRemembered((prev) => (prev ? { ...prev, active: false } : prev));
  }

  return (
    <div className="space-y-0">
      {/* As duas opções do Auto Rejoin que moravam aqui (campos compartilhados
          com a sidebar e New View/Classic) saíram com a tela simplificada
          (03/10/2026): o Auto Rejoin não tem mais esses campos nem duas visões. */}
      <SectionLabel>Shuffle</SectionLabel>

      <Toggle
        checked={s.getBool("General", "ShuffleJobId")}
        onChange={(v) => s.setBool("General", "ShuffleJobId", v)}
        label="Shuffle Job ID"
        description="Picks a random server when you have not chosen one. A typed Job ID or following a player wins over it."
      />

      <Divider />
      <SectionLabel>Other</SectionLabel>

      <Toggle
        checked={s.getBool("General", "AutoCloseLastProcess")}
        onChange={(v) => s.setBool("General", "AutoCloseLastProcess", v)}
        label="Auto Close Last Process"
        description="Close the previous Roblox instance when launching a new one for the same account"
      />
      <Toggle
        checked={s.getBool("General", "AutoCloseRobloxForMultiRbx")}
        onChange={(v) => s.setBool("General", "AutoCloseRobloxForMultiRbx", v)}
        label="Auto Close Roblox for Multi Roblox"
        description="If Multi Roblox cannot be enabled, close open Roblox windows automatically and continue"
      />
      {/* O laco de presenca (store.tsx) so roda com `ShowPresence` ligado e o
          intervalo e `max(30s, minutos)`: o campo aceitava minutos sem dizer
          nem do que depende nem que existe um piso. */}
      <NumberField
        value={s.getNumber("General", "PresenceUpdateRate", 5)}
        onChange={(v) => s.setNumber("General", "PresenceUpdateRate", v)}
        label="Presence Refresh"
        description="How often the online status of every account is re-checked. Needs Show Presence on, and never runs faster than every 30 seconds."
        min={1}
        max={9999}
        suffix="min"
      />

      {/* Backups saiu daqui (era Data > Backups > Manage, que abria um
          diálogo) e virou a seção Backups da página. */}
      <Divider />
      <SectionLabel>Security</SectionLabel>
      <div className="flex items-center justify-between gap-3 py-2 px-1 rounded-lg border border-zinc-800/70 bg-zinc-900/35">
        <div className="min-w-0">
          <div className="text-[13px] text-zinc-200">{t("Change Encryption Method")}</div>
          <div className="text-[12px] text-zinc-500 mt-0.5">
            {t("Re-encrypts your current AccountData.json with the selected method.")}
          </div>
        </div>
        <button
          type="button"
          onClick={onRequestEncryptionSetup}
          className="px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 border border-zinc-700/70 text-[12px] text-zinc-200 font-medium transition-colors"
        >
          {t("Open")}
        </button>
      </div>

      {remembered?.supported && (
        <div className="flex items-center justify-between gap-3 py-2 px-1 mt-2 rounded-lg border border-zinc-800/70 bg-zinc-900/35">
          <div className="min-w-0">
            <div className="text-[13px] text-zinc-200">{t("Stay signed in on this computer")}</div>
            <div className="text-[12px] text-zinc-500 mt-0.5">
              {remembered.active
                ? t("Your password is stored for this Windows user, protected by the system, and expires on its own.")
                : t("Not stored. Tick the box on the password screen to skip typing it for a while.")}
            </div>
          </div>
          <button
            type="button"
            disabled={!remembered.active}
            onClick={() => void forgetRemembered()}
            className="px-3 py-1.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 border border-zinc-700/70 text-[12px] text-zinc-200 font-medium transition-colors disabled:opacity-40"
          >
            {t("Forget")}
          </button>
        </div>
      )}
    </div>
  );
}
