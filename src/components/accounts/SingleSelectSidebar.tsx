import { useState, useEffect, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../../store";
import { accountInitial, accountLabel, hideAccountAvatar } from "../../utils/accountName";
import { usePrompt } from "../../hooks/usePrompt";
import { SidebarSection } from "./SidebarSection";
import { AccountLaunchOverrides } from "./AccountLaunchOverrides";
import { AccountHistory } from "./AccountHistory";
import { ClientHealthNote } from "../session/ClientHealthNote";
import { Select } from "../ui/Select";
import { tr, useTr } from "../../i18n/text";
import { MAX_ALIAS_LENGTH } from "../../types";
import { User, Package } from "lucide-react";
import { StatusBadge } from "./StatusBadge";
import { moderationBadge, moderationLabel } from "../../utils/moderation";

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
    store.updateAccount({ ...account, Alias: alias.slice(0, MAX_ALIAS_LENGTH) });
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

  const [checkingModeration, setCheckingModeration] = useState(false);
  const moderation = store.moderationByUserId?.get(account.UserID);
  const moderationKind = moderationBadge(moderation);
  const moderationText = moderation
    ? moderationLabel(moderation, t)
    : t("Ban status not checked yet");

  async function handleCheckModeration() {
    setCheckingModeration(true);
    try {
      await store.checkModeration(account.UserID);
    } finally {
      setCheckingModeration(false);
    }
  }

  const avatarUrl = store.avatarUrls.get(account.UserID);
  const displayName = accountLabel(account, store);
  const hideAvatar = hideAccountAvatar(store);
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
              {accountInitial(account, store)}
            </div>
          )}
          <div className="min-w-0">
            <div className="text-sm font-medium text-[var(--panel-fg)] truncate">{displayName}</div>
            {account.Alias && !store.hideUsernames && (
              <div className="text-xs theme-muted truncate">@{account.Username}</div>
            )}
            {store.settings?.General?.ShowPresence === "true" && (
              <div className={`inline-flex items-center gap-1 text-[11px] mt-0.5 ${presenceMeta.text}`}>
                <span
                  className={`w-1.5 h-1.5 rounded-full ${presenceMeta.dot} ${presenceType >= 1 ? "animate-pulse" : ""}`}
                  style={presenceMeta.dotStyle}
                />
                <span>{presenceMeta.label}</span>
              </div>
            )}
          </div>
        </div>

        {/* Cliente desta conta caiu: o mesmo aviso da Sessão. */}
        {store.launchedByProgram.has(account.UserID) && (
          <div className="mt-1.5 text-[11px] flex min-w-0">
            <ClientHealthNote health={store.clientHealth?.get(account.UserID)} />
          </div>
        )}

        <div className="mt-2 text-[12px]">
          <span className={account.Valid ? "text-emerald-500" : "text-red-400"}>
            {account.Valid ? t("Valid") : t("Invalid")}
          </span>
          {!store.hideUsernames && (
            <span className="theme-muted font-mono ml-2">ID: {account.UserID}</span>
          )}
        </div>

        {/* Moderação (ideia 8): leitura no Roblox, sem renovar a sessão. */}
        <div className="mt-1.5 text-[12px] flex items-start gap-1.5" data-testid="moderation-status">
          {moderation && moderationKind && (
            <StatusBadge kind={moderationKind} label={moderationText} size={14} decorative />
          )}
          <div className="min-w-0 flex-1">
            <div className={moderationKind ? "text-[var(--panel-fg)] font-medium" : "theme-muted"}>
              {moderationText}
            </div>
            {moderation?.note?.trim() && (
              <div className="theme-muted text-[11px] break-words">
                {t("Moderator note: {{note}}", { note: moderation.note.trim() })}
              </div>
            )}
          </div>
          <button
            onClick={handleCheckModeration}
            disabled={checkingModeration}
            className="sidebar-btn-sm shrink-0 disabled:opacity-50"
            title={t("Asks Roblox if this account is banned or warned. Read-only: it doesn't sign anything out.")}
          >
            {checkingModeration ? t("Checking...") : t("Check ban status")}
          </button>
        </div>
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
        <SidebarSection title={t("Alias")}>
          <p className="text-[11px] theme-muted mb-1.5">{t("Display name shown in the account list")}</p>
          <div className="flex gap-1.5">
            <input
              value={alias}
              onChange={(e) => setAlias(e.target.value)}
              maxLength={MAX_ALIAS_LENGTH}
              // Com os nomes ocultos, nem o exemplo (o nome de usuário) nem o
              // apelido digitado aparecem: o campo vira bolinhas, como senha.
              placeholder={store.hideUsernames ? t("Alias") : account.Username}
              className={`sidebar-input flex-1${store.hideUsernames ? " masked-input" : ""}`}
              onKeyDown={(e) => e.key === "Enter" && handleSetAlias()}
            />
            <button onClick={handleSetAlias} className="sidebar-btn-sm">
              {t("Set")}
            </button>
          </div>
        </SidebarSection>

        <SidebarSection title={t("Description")}>
          <p className="text-[11px] theme-muted mb-1.5">{t("Private notes about this account")}</p>
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

        {/* Histórico de sessões (ideia 6): onde jogou, quanto e como terminou. */}
        <AccountHistory account={account} />

        {/* A versão gravada aqui é a global (Settings > Versions, chave
            Versions.DefaultVersion) — não existe versão por conta. A seção
            continua no painel da conta por ser onde se lança, mas o rótulo e a
            descrição precisam dizer que o ajuste vale para todas as contas. */}
        {installedVersions.length > 0 && (
          <SidebarSection title={t("Roblox Version (all accounts)")}>
            <p className="text-[11px] theme-muted mb-1.5">{t("Global setting — every account launches with this version, not just this one.")}</p>
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
              className="flex items-center gap-1 mt-1.5 text-[12px] text-sky-400 hover:text-sky-300"
            >
              <Package size={11} strokeWidth={1.5} />
              {t("Manage versions...")}
            </button>
          </SidebarSection>
        )}

        <AccountLaunchOverrides account={account} />

        <SidebarSection title={t("Tools")}>
          <p className="text-[11px] theme-muted mb-1.5">{t("Account utilities and quick actions")}</p>
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
