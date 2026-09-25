import { useState, useRef, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../../store";
import { usePrompt, useConfirm } from "../../hooks/usePrompt";
import { useCopyCredentialWarning } from "../../hooks/useCopyCredentialWarning";
import { maskAccountName, parseGroupName } from "../../types";
import { tr, useTr } from "../../i18n/text";
import { ChevronDown, Gamepad2, Settings2, Users } from "lucide-react";

export function BottomActionBar() {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const confirm = useConfirm();
  const confirmCopyCredential = useCopyCredentialWarning();

  const [actionsOpen, setActionsOpen] = useState(false);
  const [groupMenuOpen, setGroupMenuOpen] = useState(false);
  const [friendMenuOpen, setFriendMenuOpen] = useState(false);
  const [friendBusy, setFriendBusy] = useState(false);
  const [friendProgress, setFriendProgress] = useState<{ phase: string; done: number; total: number } | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const actionsRef = useRef<HTMLDivElement>(null);

  const count = store.selectedIds.size;
  const isSingle = count === 1;
  const accounts = store.selectedAccounts;

  const bottingActive = store.bottingStatus?.active === true;
  const bottingEnabled = store.settings?.General?.BottingEnabled === "true";
  /**
   * A entrada do Botting aparece mesmo com o toggle desligado: antes o recurso
   * só existia para quem já tinha ligado a opção, ou seja, ninguém descobria
   * que ele existe. Desligado, a linha explica o que o modo faz e manda para
   * Settings em vez de abrir um diálogo que não vai funcionar.
   */
  const showBottingButton = bottingEnabled || bottingActive;
  const bottingSummary = t(
    "Keeps a group of accounts in one server by closing and relaunching each client every few minutes. Needs Multi Roblox."
  );
  const bottingOffHint = t(
    "Keeps a group of accounts in one server by closing and relaunching each client every few minutes. Needs Multi Roblox — turn it on in Settings › General."
  );
  const activeBottingIds = useMemo(
    () => new Set(store.bottingStatus?.userIds || []),
    [store.bottingStatus?.userIds]
  );
  const addableBottingIds = useMemo(
    () => accounts.map((a) => a.UserID).filter((id) => !activeBottingIds.has(id)),
    [accounts, activeBottingIds]
  );
  const launchedSelectedIds = useMemo(
    () => accounts.map((a) => a.UserID).filter((id) => store.launchedByProgram.has(id)),
    [accounts, store.launchedByProgram]
  );

  const allGroups = useMemo(() => {
    const set = new Set<string>();
    store.accounts.forEach((a) => set.add(a.Group || "Default"));
    return [...set].sort();
  }, [store.accounts]);

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!actionsOpen && !groupMenuOpen && !friendMenuOpen) return;
    function handler(e: MouseEvent) {
      if (actionsRef.current && !actionsRef.current.contains(e.target as Node)) {
        setActionsOpen(false);
        setGroupMenuOpen(false);
        setFriendMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [actionsOpen, groupMenuOpen, friendMenuOpen]);

  // Live progress for the friend-linking batch.
  useEffect(() => {
    const unlisten = listen<{ phase: string; done: number; total: number }>(
      "friend-link-progress",
      (e) => setFriendProgress(e.payload.phase === "done" ? null : e.payload)
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function handleMakeFriends(mode: "mesh" | "star", mainUserId: number | null) {
    setFriendMenuOpen(false);
    setActionsOpen(false);
    if (count < 2) {
      store.addToast(t("Select at least 2 accounts."));
      return;
    }
    const reqCount = mode === "mesh" ? count * (count - 1) : Math.max(0, count - 1) * 2;
    if (
      reqCount > 30 &&
      !(await confirm(
        tr(
          "This will send {{n}} friend requests (~{{min}} min). Roblox may rate-limit new accounts. Continue?",
          { n: reqCount, min: Math.ceil((reqCount * 2.5) / 60) }
        )
      ))
    ) {
      return;
    }
    setFriendBusy(true);
    try {
      const res = await invoke<{
        pairsTotal: number;
        alreadyFriends: number;
        verifiedOk: number;
        failed: number;
      }>("make_selected_friends", {
        userIds: accounts.map((a) => a.UserID),
        mode,
        mainUserId: mode === "star" ? mainUserId : null,
        delayMs: null,
      });
      store.addToast(
        tr("Friends linked: {{ok}} formed, {{already}} already, {{fail}} failed (of {{total}} pairs)", {
          ok: res.verifiedOk,
          already: res.alreadyFriends,
          fail: res.failed,
          total: res.pairsTotal,
        })
      );
    } catch (e) {
      store.addToast(tr("Friend linking failed: {{error}}", { error: String(e) }));
    } finally {
      setFriendBusy(false);
      setFriendProgress(null);
    }
  }

  async function handleRefreshAll() {
    setActionsOpen(false);
    setRefreshing(true);
    let ok = 0, fail = 0;
    for (const a of accounts) {
      const result = await store.refreshCookie(a.UserID).catch(() => false);
      if (result) ok++; else fail++;
      await new Promise((r) => setTimeout(r, 2000));
    }
    setRefreshing(false);
    store.addToast(tr("Refreshed: {{ok}} ok, {{fail}} failed", { ok, fail }));
  }

  async function handleCopyCookies() {
    setActionsOpen(false);
    const cookies = accounts.map((a) => a.SecurityToken).filter(Boolean);
    // Um clique punha o cookie de toda a seleção na área de transferência sem
    // dizer o que um cookie entrega nem quantas contas iam junto.
    if (!(await confirmCopyCredential("cookie", cookies.length))) return;
    await navigator.clipboard.writeText(cookies.join("\n"));
    store.addToast(tr("Copied {{count}} cookies", { count: cookies.length }));
  }

  async function handleMoveToGroup(group: string) {
    setGroupMenuOpen(false);
    setActionsOpen(false);
    await store.moveToGroup(accounts.map((a) => a.UserID), group);
  }

  async function handleNewGroup() {
    setGroupMenuOpen(false);
    setActionsOpen(false);
    const name = await prompt(tr("New group name:"));
    if (!name?.trim()) return;
    await store.moveToGroup(accounts.map((a) => a.UserID), name.trim());
  }

  async function handleAddToBotting() {
    setActionsOpen(false);
    if (!bottingActive) {
      store.addToast(t("Start Botting Mode first"));
      store.setBottingDialogOpen(true);
      return;
    }
    if (addableBottingIds.length === 0) {
      store.addToast(t("Selected accounts are already in Botting Mode"));
      return;
    }
    try {
      await store.addBottingAccounts(addableBottingIds);
    } catch (e) {
      store.addToast(t("Botting account action failed: {{error}}", { error: String(e) }));
    }
  }

  // Com o modo desligado o diálogo não adianta: leva direto para o toggle.
  function handleOpenBottingSettings() {
    setActionsOpen(false);
    store.addToast(t("Botting Mode is off — enable it in Settings › General."));
    store.setSettingsOpen(true);
  }

  async function handleRestartClients() {
    setActionsOpen(false);
    if (launchedSelectedIds.length === 0) {
      store.addToast(t("No launched clients to restart"));
      return;
    }
    await store.restartRobloxClients(launchedSelectedIds);
  }

  async function handleRemoveAccounts() {
    setActionsOpen(false);
    const confirmed = await prompt(
      tr("Type REMOVE to delete {{count}} account(s):", { count }),
      ""
    );
    if (confirmed?.trim().toUpperCase() !== "REMOVE") return;
    store.removeAccounts(accounts.map((a) => a.UserID));
  }

  function handleOpenChooseGame() {
    store.setChooseGameOpen(true);
  }

  function handleOpenAccountSettings() {
    store.setSidebarOpen(!store.sidebarOpen);
  }

  const singleAccount = isSingle ? accounts[0] : null;
  /**
   * Com o modo "Hidden" ligado a barra tem que esconder o nome igual à lista —
   * ela era o único lugar onde o nome real continuava aparecendo na tela.
   */
  const maskIfHidden = (name: string) =>
    maskAccountName(name, store.hideUsernames, store.hiddenNameLetters);
  const displayName = singleAccount
    ? maskIfHidden(singleAccount.Alias || singleAccount.Username)
    : null;

  const friendPhaseLabel = friendProgress
    ? friendProgress.phase === "checking"
      ? t("Checking {{d}}/{{t}}", { d: friendProgress.done, t: friendProgress.total })
      : friendProgress.phase === "verifying"
      ? t("Verifying {{d}}/{{t}}", { d: friendProgress.done, t: friendProgress.total })
      : t("Linking {{d}}/{{t}}", { d: friendProgress.done, t: friendProgress.total })
    : t("Linking friends...");

  return (
    <div className="theme-border border-t shrink-0 flex items-center gap-3 px-4 h-14 bg-[var(--app-bg)]">
      {/* Selection info */}
      <div className="flex-1 min-w-0">
        {isSingle && displayName ? (
          <div>
            <div className="text-sm font-medium text-[var(--panel-fg)] truncate">{displayName}</div>
            <div className="text-[10px] theme-muted">{t("1 account selected")}</div>
          </div>
        ) : (
          <div>
            <div className="text-sm font-medium text-[var(--panel-fg)]">
              {t("{{count}} accounts selected", { count })}
            </div>
            <div className="text-[10px] theme-muted">
              {t("Ctrl+click to toggle · Shift+click for range · Ctrl+A for all")}
            </div>
          </div>
        )}
      </div>

      {/* Deselect */}
      <button
        onClick={store.deselectAll}
        className="theme-btn-ghost text-[11px] px-2.5 py-1.5 rounded-md"
      >
        {t("Clear")}
      </button>

      {/* Account Settings (single only) */}
      {isSingle && (
        <button
          onClick={handleOpenAccountSettings}
          className={`flex items-center gap-1.5 text-[11px] px-3 py-1.5 rounded-lg border transition-colors ${
            store.sidebarOpen
              ? "theme-border bg-[var(--panel-soft)] text-[var(--panel-fg)]"
              : "theme-border theme-btn-ghost"
          }`}
          title={t("Account settings: alias, description, tools")}
        >
          <Settings2 size={13} strokeWidth={1.5} />
          {t("Account")}
        </button>
      )}

      {/* Actions dropdown (multi — or single with batch ops) */}
      <div className="relative" ref={actionsRef}>
        <button
          onClick={() => { setActionsOpen((v) => !v); setGroupMenuOpen(false); }}
          className="flex items-center gap-1.5 text-[11px] px-3 py-1.5 rounded-lg border theme-border theme-btn-ghost"
        >
          {friendBusy ? friendPhaseLabel : refreshing ? t("Refreshing...") : t("Actions")}
          <ChevronDown size={11} strokeWidth={2} className={`transition-transform ${actionsOpen ? "rotate-180" : ""}`} />
        </button>

        {actionsOpen && (
          <div className="theme-panel theme-border absolute bottom-full mb-1.5 right-0 border rounded-xl shadow-2xl z-30 py-1.5 w-52 animate-scale-in">
            <div className="px-3 py-1 text-[9px] theme-muted uppercase tracking-widest font-semibold">
              {t("Batch Actions")}
            </div>

            <button
              onClick={handleRefreshAll}
              disabled={refreshing}
              className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-50 flex items-center gap-2"
            >
              🔄 {refreshing ? t("Refreshing...") : t("Refresh Cookies ({{count}})", { count })}
            </button>

            <button
              onClick={handleCopyCookies}
              className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
            >
              📋 {t("Copy All Cookies")}
            </button>

            {/* Make Friends submenu */}
            {count >= 2 && (
              <div className="relative">
                <button
                  onClick={() => setFriendMenuOpen((v) => !v)}
                  disabled={friendBusy}
                  className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-50 flex items-center justify-between gap-2"
                >
                  <span className="flex items-center gap-2">
                    <Users size={13} strokeWidth={1.5} />
                    {friendBusy ? friendPhaseLabel : t("Make Friends ({{count}})", { count })}
                  </span>
                  <ChevronDown size={10} className={`theme-muted transition-transform ${friendMenuOpen ? "-rotate-90" : "rotate-90"}`} />
                </button>
                {friendMenuOpen && (
                  <div className="theme-panel theme-border absolute bottom-0 right-full mr-1 border rounded-xl shadow-2xl z-40 py-1 w-56 animate-scale-in max-h-72 overflow-y-auto">
                    <button
                      onClick={() => handleMakeFriends("mesh", null)}
                      className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)]"
                    >
                      🕸 {t("Mesh - all friend all ({{n}} req)", { n: count * (count - 1) })}
                    </button>
                    <div className="theme-border h-px border-t my-1" />
                    <div className="px-3 py-1 text-[9px] theme-muted uppercase tracking-widest font-semibold">
                      {t("Star - pick main")}
                    </div>
                    {accounts.map((a) => (
                      <button
                        key={a.UserID}
                        onClick={() => handleMakeFriends("star", a.UserID)}
                        className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] truncate"
                      >
                        ⭐ {maskIfHidden(a.Alias || a.Username)}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* Move to Group submenu */}
            <div className="relative">
              <button
                onClick={() => setGroupMenuOpen((v) => !v)}
                className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center justify-between gap-2"
              >
                <span>📁 {t("Move to Group")}</span>
                <ChevronDown size={10} className={`theme-muted transition-transform ${groupMenuOpen ? "-rotate-90" : "rotate-90"}`} />
              </button>
              {groupMenuOpen && (
                <div className="theme-panel theme-border absolute bottom-0 right-full mr-1 border rounded-xl shadow-2xl z-40 py-1 w-44 animate-scale-in">
                  {allGroups.map((g) => (
                    <button
                      key={g}
                      onClick={() => handleMoveToGroup(g)}
                      className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] truncate"
                    >
                      {parseGroupName(g).displayName}
                    </button>
                  ))}
                  <div className="theme-border h-px border-t my-1" />
                  <button
                    onClick={handleNewGroup}
                    className="theme-accent w-full text-left px-3 py-1.5 text-[12px] hover:bg-[var(--panel-soft)]"
                  >
                    {t("+ New Group...")}
                  </button>
                </div>
              )}
            </div>

            {launchedSelectedIds.length > 0 && (
              <button
                onClick={handleRestartClients}
                className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
              >
                🔁 {t("Restart Launched ({{count}})", { count: launchedSelectedIds.length })}
              </button>
            )}

            {showBottingButton ? (
              <>
                <button
                  onClick={() => { setActionsOpen(false); store.setBottingDialogOpen(true); }}
                  title={bottingSummary}
                  className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
                >
                  🤖 {t("Open Botting Mode")}
                </button>
                {bottingActive && addableBottingIds.length > 0 && (
                  <button
                    onClick={handleAddToBotting}
                    className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
                  >
                    ➕ {t("Add to Botting ({{count}})", { count: addableBottingIds.length })}
                  </button>
                )}
              </>
            ) : (
              <>
                <button
                  onClick={handleOpenBottingSettings}
                  title={bottingSummary}
                  className="w-full text-left px-3 py-1.5 text-[12px] theme-muted hover:bg-[var(--panel-soft)] flex items-center justify-between gap-2"
                >
                  <span>🤖 {t("Botting Mode")}</span>
                  <span className="text-[9px] uppercase tracking-widest">{t("Off")}</span>
                </button>
                <div className="px-3 pb-1.5 text-[10px] theme-muted leading-snug">
                  {bottingOffHint}
                </div>
              </>
            )}

            <button
              onClick={() => { setActionsOpen(false); store.killAllRobloxProcesses(); }}
              className="w-full text-left px-3 py-1.5 text-[12px] text-amber-300/80 hover:bg-amber-500/10 flex items-center gap-2"
            >
              ✕ {t("Close All Roblox")}
            </button>

            <div className="theme-border h-px border-t my-1" />

            <button
              onClick={handleRemoveAccounts}
              className="w-full text-left px-3 py-1.5 text-[12px] text-red-400/80 hover:bg-red-500/10 flex items-center gap-2"
            >
              🗑 {t("Remove ({{count}})", { count })}
            </button>
          </div>
        )}
      </div>

      {/* Primary CTA */}
      <button
        onClick={handleOpenChooseGame}
        className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[var(--accent-color)] hover:opacity-90 text-white text-sm font-semibold transition-opacity"
      >
        <Gamepad2 size={15} strokeWidth={1.5} />
        {isSingle ? t("Choose Game") : t("Choose Game ({{count}})", { count })}
      </button>
    </div>
  );
}
