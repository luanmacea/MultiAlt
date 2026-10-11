import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import type { RememberState } from "../../types";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { Select } from "../ui/Select";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { LOCK_MINUTES_DEFAULT, LOCK_MINUTES_MAX, LOCK_MINUTES_MIN } from "../../utils/inactivityLock";
import { ActionCard, isWindowsUserAgent } from "./settingsParts";

type RestrictedStyle = "bubbles" | "warp" | "warpLegacy" | "waves";

function normalizeRestrictedStyle(value: string): RestrictedStyle {
  return value === "bubbles" || value === "warp" || value === "warpLegacy" || value === "waves" ? value : "warp";
}

/**
 * Settings › Privacy & security: nomes ocultos (de General), o arquivo de
 * contas e a tela de senha (a parte "Security" da antiga Misc e o fundo da
 * tela de senha, de General). Desde 11/10/2026.
 */
export function PrivacyTab({
  s,
  onRequestEncryptionSetup,
}: {
  s: UseSettingsReturn;
  onRequestEncryptionSetup?: () => void;
}) {
  const t = useTr();
  const store = useStore();
  const isWindows = isWindowsUserAgent();
  // Trancar por inatividade só faz sentido com senha do app (ideia 27).
  const hasAppPassword = store.accountsEncrypted === true;
  const [remembered, setRemembered] = useState<RememberState | null>(null);
  const restrictedBackgroundStyle = normalizeRestrictedStyle(s.get("General", "RestrictedBackgroundStyle", "warp"));

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
      <SectionLabel>Hidden names</SectionLabel>
      {/* `maskAccountName` (utils/accountName.ts) so preserva este prefixo; 0 troca
          o nome inteiro por asteriscos. "Preview Letters" sozinho nao dizia
          nem de que nome se trata. */}
      <NumberField
        value={s.getNumber("General", "HiddenNameLetters", 0)}
        onChange={(v) => s.setNumber("General", "HiddenNameLetters", v)}
        label="Preview Letters"
        description="First letters kept visible while names are hidden; the rest turns into asterisks. 0 hides the whole name."
        min={0}
        max={20}
        suffix="chars"
      />
      <Toggle
        checked={s.getBool("General", "ShowAvatarsWhenHidden")}
        onChange={(v) => s.setBool("General", "ShowAvatarsWhenHidden", v)}
        label="Show Avatars When Hidden"
        description="Display profile pictures even when names are hidden"
      />
      <Toggle
        checked={s.getBool("General", "HideRobuxWhenHidden")}
        onChange={(v) => s.setBool("General", "HideRobuxWhenHidden", v)}
        label="Hide Robux When Hidden"
        description="Mask the Robux balance in the sidebar when names are hidden"
      />
      {/* commands/client_health.rs: SetWindowText nas janelas que o app acompanha. */}
      {isWindows && (
        <Toggle
          checked={s.get("General", "ShowAccountNameOnWindow", "true") !== "false"}
          onChange={(v) => s.setBool("General", "ShowAccountNameOnWindow", v)}
          label="Show Account Name on Roblox Window"
          description="Titles each Roblox window “account — Roblox”, name first, so you can tell them apart on the taskbar. Hidden names stay hidden."
        />
      )}

      <Divider />
      <SectionLabel>Account file</SectionLabel>
      <ActionCard
        title={t("Change Encryption Method")}
        description={t("Re-encrypts your current AccountData.json with the selected method.")}
        action={t("Open")}
        onAction={() => onRequestEncryptionSetup?.()}
      />
      <Toggle
        checked={s.get("General", "WarnOnCopyCredential", "true") === "true"}
        onChange={(v) => s.setBool("General", "WarnOnCopyCredential", v)}
        label="Warn Before Copying Credentials"
        description="Show a confirmation before a cookie or password goes to the clipboard"
      />

      <Divider />
      <SectionLabel>Password screen</SectionLabel>
      <Toggle
        checked={hasAppPassword && s.getBool("General", "LockOnInactivity")}
        onChange={(v) => s.setBool("General", "LockOnInactivity", v)}
        disabled={!hasAppPassword}
        label="Lock after inactivity"
        description={
          hasAppPassword
            ? "Shows the password screen after the minutes below without using the MultiAlt window. Launches, AFK Mode and reconnects keep running."
            : "Needs an app password. Set one with Change Encryption Method above."
        }
      />
      <NumberField
        value={s.getNumber("General", "LockAfterMinutes", LOCK_MINUTES_DEFAULT)}
        onChange={(v) => s.setNumber("General", "LockAfterMinutes", v)}
        label="Lock after"
        description="Minutes without a click or key press in the MultiAlt window."
        disabled={!hasAppPassword || !s.getBool("General", "LockOnInactivity")}
        min={LOCK_MINUTES_MIN}
        max={LOCK_MINUTES_MAX}
        suffix="min"
      />
      {remembered?.supported && (
        <ActionCard
          title={t("Stay signed in on this computer")}
          description={
            remembered.active
              ? t("Your password is stored for this Windows user, protected by the system, and expires on its own.")
              : t("Not stored. Tick the box on the password screen to skip typing it for a while.")
          }
          action={t("Forget")}
          onAction={() => void forgetRemembered()}
          disabled={!remembered.active}
        />
      )}
      <div className="flex items-center gap-3 py-2 px-1">
        <div className="min-w-0">
          <div className="text-[13px] text-zinc-300">{t("Restricted Screen Style")}</div>
          <div className="mt-0.5 text-[12px] text-zinc-500">
            {t("Choose the animated background used on the password screen")}
          </div>
        </div>
        <div className="ml-auto min-w-[220px]">
          <Select
            value={restrictedBackgroundStyle}
            options={[
              { value: "warp", label: t("Fluid Warp") },
              { value: "warpLegacy", label: t("Fluid Warp Legacy") },
              { value: "waves", label: t("Waves & Lines") },
              { value: "bubbles", label: t("Fluid Bubbles") },
            ]}
            onChange={(value) => s.set("General", "RestrictedBackgroundStyle", normalizeRestrictedStyle(value))}
          />
        </div>
      </div>
    </div>
  );
}
