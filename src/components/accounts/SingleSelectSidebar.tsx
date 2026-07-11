import { useState, useEffect, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../../store";
import { usePrompt } from "../../hooks/usePrompt";
import { SidebarSection } from "./SidebarSection";
import { Select } from "../ui/Select";
import { tr, useTr } from "../../i18n/text";
import { User, Package } from "lucide-react";

function chipMaskName(name: string, previewLetters: number): string {
  if (previewLetters > 0 && previewLetters < name.length) {
    return name.slice(0, previewLetters) + "********";
  }
  return "************";
}

export function SingleSelectSidebar() {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const account = store.selectedAccount!;
  const [alias, setAlias] = useState("");
  const [description, setDescription] = useState("");
  const [installedVersions, setInstalledVersions] = useState<
    { channel: string; versionHash: string; displayVersion: string | null; userLabel: string | null }[]
  >([]);

  useEffect(() => {
    setAlias(account.Alias);
    setDescription(account.Description);

    invoke<typeof installedVersions>("versions_list_installed")
      .then((list) => setInstalledVersions(list))
      .catch(() => {});
  }, [account.UserID]);

  useEffect(() => {
    const unlisten = listen<{ stage: string }>("version-install-progress", (e) => {
      if (e.payload.stage === "ready") {
        invoke<typeof installedVersions>("versions_list_installed")
          .then((list) => setInstalledVersions(list))
          .catch(() => {});
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  function handleSetAlias() {
    store.updateAccount({ ...account, Alias: alias.slice(0, 30) });
    store.addToast(tr("Alias updated"));
  }

  function handleSetDescription() {
    store.updateAccount({ ...account, Description: description });
    store.addToast(tr("Description updated"));
  }

  async function handleJoinGroup() {
    const input = await prompt(tr("Group ID:"));
    if (!input) return;
    const groupId = parseInt(input.trim(), 10);
    if (!Number.isFinite(groupId) || groupId <= 0) {
      store.addToast(tr("Invalid group ID"));
      return;
    }
    try {
      await invoke("join_group", { userId: account.UserID, groupId });
      store.addToast(tr("Joined group {{groupId}}", { groupId }));
    } catch (e) {
      store.addToast(tr("Join group failed: {{error}}", { error: String(e) }));
    }
  }

  const avatarUrl = store.avatarUrls.get(account.UserID);
  const rawName = account.Alias || account.Username;
  const displayName = store.hideUsernames ? chipMaskName(rawName, store.hiddenNameLetters) : rawName;
  const hideAvatar = store.hideUsernames && !store.showAvatarsWhenHidden;
  const presenceType = store.presenceByUserId.get(account.UserID) ?? 0;

  const presenceMeta =
    presenceType === 3
      ? { label: t("In Studio"), dot: "bg-violet-500", dotStyle: undefined as CSSProperties | undefined, text: "text-violet-400" }
      : presenceType >= 2
      ? { label: t("In Game"), dot: "bg-emerald-500", dotStyle: undefined as CSSProperties | undefined, text: "text-emerald-400" }
      : presenceType === 1
        ? { label: t("Online"), dot: "bg-sky-500", dotStyle: undefined as CSSProperties | undefined, text: "text-sky-400" }
        : { label: t("Offline"), dot: "", dotStyle: { backgroundColor: "var(--panel-muted)" }, text: "theme-muted" };

  return (
    <div data-tour="launch-sidebar" className="theme-surface theme-border w-64 border-l flex flex-col shrink-0 animate-slide-right">
      {/* Account header */}
      <div className="p-4 border-b theme-border">
        <div className="flex items-center gap-3">
          {hideAvatar ? (
            <div className="theme-avatar w-10 h-10 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted">
              <User size={18} strokeWidth={1.5} />
            </div>
          ) : avatarUrl ? (
            <img src={avatarUrl} alt="" className="theme-avatar w-10 h-10 rounded-full bg-[var(--panel-soft)]" />
          ) : (
            <div className="theme-avatar w-10 h-10 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted text-base font-medium">
              {(account.Username || "?").charAt(0).toUpperCase()}
            </div>
          )}
          <div className="min-w-0">
            <div className="text-sm font-medium text-[var(--panel-fg)] truncate">{displayName}</div>
            {account.Alias && !store.hideUsernames && (
              <div className="text-xs theme-muted truncate">@{account.Username}</div>
            )}
            {store.settings?.General?.ShowPresence === "true" && (
              <div className={`inline-flex items-center gap-1 text-[10px] mt-0.5 ${presenceMeta.text}`}>
                <span
                  className={`w-1.5 h-1.5 rounded-full ${presenceMeta.dot} ${presenceType >= 1 ? "animate-pulse" : ""}`}
                  style={presenceMeta.dotStyle}
                />
                <span>{presenceMeta.label}</span>
              </div>
            )}
          </div>
        </div>

        <div className="mt-2 text-[11px]">
          <span className={account.Valid ? "text-emerald-500" : "text-red-400"}>
            {account.Valid ? t("Valid") : t("Invalid")}
          </span>
          {!store.hideUsernames && (
            <span className="theme-muted font-mono ml-2">ID: {account.UserID}</span>
          )}
        </div>
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
        <SidebarSection title={t("Alias")}>
          <p className="text-[10px] theme-muted mb-1.5">{t("Display name shown in the account list")}</p>
          <div className="flex gap-1.5">
            <input
              value={alias}
              onChange={(e) => setAlias(e.target.value)}
              maxLength={30}
              placeholder={account.Username}
              className="sidebar-input flex-1"
              onKeyDown={(e) => e.key === "Enter" && handleSetAlias()}
            />
            <button onClick={handleSetAlias} className="sidebar-btn-sm">
              {t("Set")}
            </button>
          </div>
        </SidebarSection>

        <SidebarSection title={t("Description")}>
          <p className="text-[10px] theme-muted mb-1.5">{t("Private notes about this account")}</p>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("Notes...")}
            className="sidebar-input min-h-[56px] resize-none"
            rows={3}
          />
          <button onClick={handleSetDescription} className="sidebar-btn-sm mt-1.5 self-end">
            {t("Save")}
          </button>
        </SidebarSection>

        {installedVersions.length > 0 && (
          <SidebarSection title={t("Roblox Version")}>
            <p className="text-[10px] theme-muted mb-1.5">{t("Override the Roblox version used when this account launches")}</p>
            <Select
              value={store.settings?.Versions?.DefaultVersion ?? ""}
              options={[
                { value: "", label: t("Latest installed") },
                ...installedVersions.map((v) => ({
                  value: `${v.channel}:${v.versionHash}`,
                  label:
                    v.userLabel ??
                    `${v.channel} · ${v.displayVersion ?? v.versionHash.slice(8, 16)}`,
                })),
              ]}
              onChange={(versionId) => store.setDefaultVersion(versionId || null)}
              className="w-full"
            />
            <button
              onClick={() => store.setVersionsDialogOpen(true)}
              className="flex items-center gap-1 mt-1.5 text-[11px] text-sky-400 hover:text-sky-300"
            >
              <Package size={11} strokeWidth={1.5} />
              {t("Manage versions...")}
            </button>
          </SidebarSection>
        )}

        <SidebarSection title={t("Tools")}>
          <p className="text-[10px] theme-muted mb-1.5">{t("Account utilities and quick actions")}</p>
          <div className="grid grid-cols-2 gap-1.5">
            <button
              onClick={() => store.setServerListOpen(true)}
              className="sidebar-btn-tool"
            >
              {t("Server List")}
            </button>
            <button
              onClick={() => store.setAccountUtilsOpen(true)}
              className="sidebar-btn-tool"
            >
              {t("Utilities")}
            </button>
            <button
              onClick={() => store.openAccountBrowser(account.UserID)}
              className="sidebar-btn-tool"
            >
              {t("Browser")}
            </button>
            <button
              onClick={handleJoinGroup}
              className="sidebar-btn-tool"
            >
              {t("Join Group")}
            </button>
          </div>
        </SidebarSection>
      </div>
    </div>
  );
}
