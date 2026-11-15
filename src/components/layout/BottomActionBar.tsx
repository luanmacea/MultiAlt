import { useState, useRef, useEffect, useMemo } from "react";
import { useStore } from "../../store";
import { usePrompt } from "../../hooks/usePrompt";
import { useCopyCredentialWarning } from "../../hooks/useCopyCredentialWarning";
import { copyAccountSecret } from "../../utils/copySecret";
import { collectGroupNames, groupLabel } from "../../types";
import { accountLabel } from "../../utils/accountName";
import { tr, useTr } from "../../i18n/text";
import { ChevronDown, FileCheck, Gamepad2, Settings2 } from "lucide-react";

export function BottomActionBar() {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const confirmCopyCredential = useCopyCredentialWarning();

  const [actionsOpen, setActionsOpen] = useState(false);
  const [groupMenuOpen, setGroupMenuOpen] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const actionsRef = useRef<HTMLDivElement>(null);

  const count = store.selectedIds.size;
  const isSingle = count === 1;
  const accounts = store.selectedAccounts;

  const bottingActive = store.bottingStatus?.active === true;
  const bottingEnabled = store.settings?.General?.BottingEnabled === "true";
  /**
   * A entrada do Auto Rejoin aparece mesmo com o toggle desligado: antes o recurso
   * só existia para quem já tinha ligado a opção, ou seja, ninguém descobria
   * que ele existe. Desligado, a linha explica o que o modo faz e manda para
   * Settings em vez de abrir um diálogo que não vai funcionar.
   */
  const showBottingButton = bottingEnabled || bottingActive;
  const bottingSummary = t(
    "Keeps a group of accounts in one server by closing and relaunching each client every few minutes. Needs Multi Roblox."
  );
  const bottingOffHint = t(
    "Keeps a group of accounts in one server by closing and relaunching each client every few minutes. Needs Multi Roblox. Auto Rejoin is off: turn it on in Settings › Keep in game."
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

  // Mesma fonte que o resto do app (`collectGroupNames`): a cópia manual daqui
  // era a última sobrevivente depois que a sidebar de multi-seleção saiu.
  const allGroups = useMemo(() => collectGroupNames(store.accounts), [store.accounts]);

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!actionsOpen && !groupMenuOpen) return;
    function handler(e: MouseEvent) {
      if (actionsRef.current && !actionsRef.current.contains(e.target as Node)) {
        setActionsOpen(false);
        setGroupMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [actionsOpen, groupMenuOpen]);

  // Relato do dono (25/10/2026): com sessões inválidas, "nada acontecia" — o
  // erro de cada conta era engolido e só um toast de contagem passava.
  async function handleRefreshAll() {
    setActionsOpen(false);
    setRefreshing(true);
    try {
      await store.refreshCookies(accounts.map((a) => a.UserID));
    } finally {
      setRefreshing(false);
    }
  }

  async function handleCheckTerms() {
    setActionsOpen(false);
    await store.checkTermsBatch(accounts.map((a) => a.UserID));
  }

  async function handleCopyCookies() {
    setActionsOpen(false);
    const cookies = accounts.map((a) => a.SecurityToken).filter(Boolean);
    // Um clique punha o cookie de toda a seleção na área de transferência sem
    // dizer o que um cookie entrega nem quantas contas iam junto.
    if (!(await confirmCopyCredential("cookie", cookies.length))) return;
    // Pelo backend: fora do histórico do Win+V e apagado sozinho em 30 s.
    try {
      const result = await copyAccountSecret(
        accounts.map((a) => a.UserID),
        "cookie",
        () => cookies.join("\n")
      );
      store.setActionReport({
        kind: "copy-cookies",
        title: t("Copy cookies"),
        running: false,
        at: Date.now(),
        rows: accounts.map((a) =>
          a.SecurityToken
            ? { userId: a.UserID, outcome: "ok" as const }
            : { userId: a.UserID, outcome: "skipped" as const, detail: t("No cookie saved for this account") }
        ),
        summary: result.clearsInSecs
          ? tr("Copied {{count}} cookies. Cleared from the clipboard in {{seconds}} s.", {
              count: result.count,
              seconds: result.clearsInSecs,
            })
          : tr("Copied {{count}} cookies", { count: result.count }),
      });
    } catch {
      store.addToast(tr("Failed to copy"));
    }
  }

  async function handleMoveToGroup(group: string) {
    setGroupMenuOpen(false);
    setActionsOpen(false);
    await store.moveToGroup(accounts.map((a) => a.UserID), group, { report: true });
  }

  async function handleNewGroup() {
    setGroupMenuOpen(false);
    setActionsOpen(false);
    const name = await prompt(tr("New group name:"));
    if (!name?.trim()) return;
    await store.moveToGroup(accounts.map((a) => a.UserID), name.trim(), { report: true });
  }

  async function handleAddToBotting() {
    setActionsOpen(false);
    if (!bottingActive) {
      store.addToast(t("Start Auto Rejoin first"));
      store.openBottingDialog();
      return;
    }
    if (addableBottingIds.length === 0) {
      store.addToast(t("Selected accounts are already in Auto Rejoin"));
      return;
    }
    try {
      await store.addBottingAccounts(addableBottingIds, { report: true });
      for (const a of accounts) {
        if (activeBottingIds.has(a.UserID)) {
          store.patchActionRow({ userId: a.UserID, outcome: "already", detail: t("Already in Auto Rejoin") });
        }
      }
    } catch (e) {
      store.addToast(t("Auto Rejoin account action failed: {{error}}", { error: String(e) }));
    }
  }

  // Com o modo desligado o diálogo não adianta: leva direto para o toggle.
  function handleOpenBottingSettings() {
    setActionsOpen(false);
    store.addToast(t("Auto Rejoin is off — enable it in Settings › Keep in game."));
    store.openSettingsSection("keep-in-game");
  }

  async function handleRestartClients() {
    setActionsOpen(false);
    if (launchedSelectedIds.length === 0) {
      store.addToast(t("No launched clients to restart"));
      return;
    }
    await store.restartRobloxClients(accounts.map((a) => a.UserID), { report: true });
  }

  async function handleRemoveAccounts() {
    setActionsOpen(false);
    const confirmed = await prompt(
      tr("Type REMOVE to delete {{count}} account(s):", { count }),
      ""
    );
    if (confirmed?.trim().toUpperCase() !== "REMOVE") return;
    store.removeAccounts(accounts.map((a) => a.UserID), { report: true });
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
  const displayName = singleAccount ? accountLabel(singleAccount, store) : null;

  return (
    <div className="theme-border border-t shrink-0 flex items-center gap-3 px-4 h-14 bg-[var(--app-bg)]">
      {/* Selection info */}
      <div className="flex-1 min-w-0">
        {isSingle && displayName ? (
          <div>
            <div className="text-sm font-medium text-[var(--panel-fg)] truncate">{displayName}</div>
            <div className="text-[11px] theme-muted">{t("1 account selected")}</div>
          </div>
        ) : (
          <div>
            <div className="text-sm font-medium text-[var(--panel-fg)]">
              {t("{{count}} accounts selected", { count })}
            </div>
            <div className="text-[11px] theme-muted">
              {t("Ctrl+click to toggle · Shift+click for range · Ctrl+A for all")}
            </div>
          </div>
        )}
      </div>

      {/* Deselect */}
      <button
        onClick={store.deselectAll}
        className="theme-btn-ghost text-[12px] px-2.5 py-1.5 rounded-md"
      >
        {t("Clear")}
      </button>

      {/* Account Settings (single only) */}
      {isSingle && (
        <button
          onClick={handleOpenAccountSettings}
          className={`flex items-center gap-1.5 text-[12px] px-3 py-1.5 rounded-lg border transition-colors ${
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
          className="flex items-center gap-1.5 text-[12px] px-3 py-1.5 rounded-lg border theme-border theme-btn-ghost"
        >
          {refreshing ? t("Refreshing...") : t("Actions")}
          <ChevronDown size={11} strokeWidth={2} className={`transition-transform ${actionsOpen ? "rotate-180" : ""}`} />
        </button>

        {actionsOpen && (
          <div className="theme-panel theme-border absolute bottom-full mb-1.5 right-0 border rounded-xl shadow-2xl z-30 py-1.5 w-52 animate-scale-in">
            <div className="px-3 py-1 text-[11px] theme-muted uppercase tracking-widest font-semibold">
              {t("Batch Actions")}
            </div>

            <button
              onClick={handleRefreshAll}
              disabled={refreshing}
              className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-50 flex items-center gap-2"
            >
              🔄 {refreshing ? t("Refreshing...") : t("Refresh Cookies ({{count}})", { count })}
            </button>

            {/* Acordos atualizados (Termos / Privacidade): só lê; o aceite é na janela. */}
            <button
              onClick={handleCheckTerms}
              disabled={store.termsChecking}
              className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-50 flex items-start gap-2"
            >
              <FileCheck size={13} strokeWidth={1.5} className="mt-0.5 shrink-0" />
              <span className="min-w-0">
                {t("Check Terms ({{count}})", { count })}
                <span className="block text-[11px] theme-muted leading-snug">
                  {t("Which accounts have updated terms to accept. Read-only.")}
                </span>
              </span>
            </button>

            <button
              onClick={handleCopyCookies}
              className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
            >
              📋 {t("Copy All Cookies")}
            </button>

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
                      {groupLabel(g, t)}
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
                  onClick={() => { setActionsOpen(false); store.openBottingDialog(); }}
                  title={bottingSummary}
                  className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
                >
                  🤖 {t("Open Auto Rejoin")}
                </button>
                {bottingActive && addableBottingIds.length > 0 && (
                  <button
                    onClick={handleAddToBotting}
                    className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] flex items-center gap-2"
                  >
                    ➕ {t("Add to Auto Rejoin ({{count}})", { count: addableBottingIds.length })}
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
                  <span>🤖 {t("Auto Rejoin")}</span>
                  <span className="text-[11px] uppercase tracking-widest">{t("Off")}</span>
                </button>
                <div className="px-3 pb-1.5 text-[11px] theme-muted leading-snug">
                  {bottingOffHint}
                </div>
              </>
            )}

            <button
              onClick={() => { setActionsOpen(false); store.killAllRobloxProcesses({ report: true }); }}
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
        data-tour="choose-game-button"
        className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[var(--accent-color)] hover:opacity-90 text-white text-sm font-semibold transition-opacity"
      >
        <Gamepad2 size={15} strokeWidth={1.5} />
        {isSingle ? t("Choose Game") : t("Choose Game ({{count}})", { count })}
      </button>
    </div>
  );
}
