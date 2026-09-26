import { useState, useMemo, useEffect } from "react";
import { ChevronDown, Users } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../../store";
import { usePrompt, useConfirm } from "../../hooks/usePrompt";
import { useJoinOnlineWarning } from "../../hooks/useJoinOnlineWarning";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import { useCopyCredentialWarning } from "../../hooks/useCopyCredentialWarning";
import { isMultiRobloxCloseProcessError } from "../../utils/robloxErrors";
import { parseGroupName } from "../../types";
import { SidebarSection } from "./SidebarSection";
import { AccountChip } from "./AccountChip";
import { GameBadge } from "../ui/GameBadge";
import { tr, useTr } from "../../i18n/text";

export function MultiSelectSidebar() {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const confirm = useConfirm();
  const confirmJoinOnline = useJoinOnlineWarning();
  const confirmCopyCredential = useCopyCredentialWarning();
  const accounts = store.selectedAccounts;
  /** Que jogo é o place da barra de launch. */
  const game = useGameIdentity(store.placeId, accounts[0]?.UserID ?? null);
  const count = accounts.length;
  const [refreshing, setRefreshing] = useState(false);
  const [moveOpen, setMoveOpen] = useState(false);
  const [accountsExpanded, setAccountsExpanded] = useState(false);
  const [friendOpen, setFriendOpen] = useState(false);
  const [friendMode, setFriendMode] = useState<"mesh" | "star">("star");
  const [friendMain, setFriendMain] = useState<number | null>(null);
  const [friendBusy, setFriendBusy] = useState(false);
  const [friendDelay, setFriendDelay] = useState("2.5");
  const [friendProgress, setFriendProgress] = useState<{ phase: string; done: number; total: number } | null>(null);

  useEffect(() => {
    const unlisten = listen<{ phase: string; done: number; total: number }>(
      "friend-link-progress",
      (e) => {
        setFriendProgress(e.payload.phase === "done" ? null : e.payload);
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const previewAccounts = accounts.slice(0, 5);
  const remaining = count - previewAccounts.length;
  const hasManySelected = count > 5;
  const shownAccounts = accountsExpanded ? accounts : previewAccounts;
  const bottingEnabled = store.settings?.General?.BottingEnabled === "true";
  const showBottingButton = bottingEnabled || store.bottingStatus?.active === true;
  const bottingActive = store.bottingStatus?.active === true;
  const activeBottingUserIds = useMemo(() => new Set(store.bottingStatus?.userIds || []), [store.bottingStatus?.userIds]);
  const addableBottingIds = useMemo(
    () => accounts.map((a) => a.UserID).filter((id) => !activeBottingUserIds.has(id)),
    [accounts, activeBottingUserIds]
  );
  const addableBottingCount = addableBottingIds.length;
  const launchedSelectedIds = useMemo(
    () => accounts.map((a) => a.UserID).filter((id) => store.launchedByProgram.has(id)),
    [accounts, store.launchedByProgram]
  );
  const launchedSelectedCount = launchedSelectedIds.length;
  const pulseCloseAction = isMultiRobloxCloseProcessError(store.error);

  const allGroups = useMemo(() => {
    const set = new Set<string>();
    store.accounts.forEach((a) => set.add(a.Group || "Default"));
    return [...set].sort();
  }, [store.accounts]);

  async function handleJoin() {
    const ids = accounts.map((a) => a.UserID);
    if (!(await confirmJoinOnline(ids))) return;
    try {
      await store.launchMultiple(ids);
    } catch (e) {
      store.addToast(tr("Launch failed: {{error}}", { error: String(e) }));
    }
  }

  async function handleRefreshAll() {
    setRefreshing(true);
    let ok = 0;
    let fail = 0;
    for (const a of accounts) {
      const result = await store.refreshCookie(a.UserID).catch(() => false);
      if (result) ok++;
      else fail++;
      await new Promise((r) => setTimeout(r, 2000));
    }
    setRefreshing(false);
    store.addToast(tr("Refreshed: {{ok}} ok, {{fail}} failed", { ok, fail }));
  }

  async function handleCopyCookies() {
    const cookies = accounts.map((a) => a.SecurityToken).filter(Boolean);
    // Cookie de toda a seleção num clique, sem aviso: agora o aviso diz o que
    // um cookie entrega e quantas contas vão junto.
    if (!(await confirmCopyCredential("cookie", cookies.length))) return;
    await navigator.clipboard.writeText(cookies.join("\n"));
    store.addToast(tr("Copied {{count}} cookies", { count: cookies.length }));
  }

  const effectiveMain = friendMain ?? accounts[0]?.UserID ?? null;
  const friendRequestCount =
    friendMode === "mesh" ? count * (count - 1) : Math.max(0, count - 1) * 2;
  const friendBtnLabel = friendProgress
    ? friendProgress.phase === "checking"
      ? t("Checking {{d}}/{{t}}", { d: friendProgress.done, t: friendProgress.total })
      : friendProgress.phase === "verifying"
      ? t("Verifying {{d}}/{{t}}", { d: friendProgress.done, t: friendProgress.total })
      : t("Linking {{d}}/{{t}}", { d: friendProgress.done, t: friendProgress.total })
    : t("Linking friends...");

  async function handleMakeFriends() {
    if (count < 2) {
      store.addToast(t("Select at least 2 accounts."));
      return;
    }
    if (friendMode === "star" && effectiveMain == null) {
      store.addToast(t("Pick a main account."));
      return;
    }
    // Warn before large batches — Roblox rate-limits friend requests hard.
    if (
      friendRequestCount > 30 &&
      !(await confirm(
        tr(
          "This will send {{n}} friend requests (~{{min}} min). Roblox may rate-limit new accounts. Continue?",
          { n: friendRequestCount, min: Math.ceil((friendRequestCount * 2.5) / 60) }
        )
      ))
    ) {
      return;
    }
    setFriendOpen(false);
    setFriendBusy(true);
    const delaySec = parseFloat(friendDelay);
    const delayMs = Number.isFinite(delaySec) && delaySec >= 0 ? Math.round(delaySec * 1000) : null;
    try {
      const res = await invoke<{
        pairsTotal: number;
        alreadyFriends: number;
        attempted: number;
        verifiedOk: number;
        failed: number;
      }>("make_selected_friends", {
        userIds: accounts.map((a) => a.UserID),
        mode: friendMode,
        mainUserId: friendMode === "star" ? effectiveMain : null,
        delayMs,
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

  async function handleMoveToGroup(group: string) {
    setMoveOpen(false);
    await store.moveToGroup(
      accounts.map((a) => a.UserID),
      group
    );
  }

  async function handleNewGroup() {
    setMoveOpen(false);
    const name = await prompt(tr("New group name:"));
    if (!name?.trim()) return;
    await store.moveToGroup(
      accounts.map((a) => a.UserID),
      name.trim()
    );
  }

  async function handleAddToBottingMode() {
    if (!bottingActive) {
      store.addToast(t("Botting Mode is not running"));
      return;
    }
    if (addableBottingCount <= 0) {
      store.addToast(t("Selected accounts are already in Botting Mode"));
      return;
    }
    try {
      await store.addBottingAccounts(addableBottingIds);
    } catch (e) {
      store.addToast(t("Botting account action failed: {{error}}", { error: String(e) }));
    }
  }

  async function handleRestartLaunchedClients() {
    await store.restartRobloxClients(launchedSelectedIds);
  }

  return (
    <div data-tour="launch-sidebar" className="theme-surface theme-border w-72 border-l flex flex-col shrink-0 animate-slide-right">
      <div className="p-4 border-b theme-border">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm font-medium text-[var(--panel-fg)]">
              {count} selected
            </div>
            <div className="theme-muted text-[11px] mt-0.5">
              {t("Ctrl+click to toggle, Shift+click for range")}
            </div>
          </div>
          <button
            onClick={store.deselectAll}
            className="theme-btn-ghost text-[11px] transition-colors px-2 py-0.5 rounded"
          >
            {t("Clear")}
          </button>
        </div>

        <div
          className={`mt-3 flex flex-col gap-1 overflow-hidden transition-[max-height,opacity] duration-200 ease-out ${
            accountsExpanded
              ? "max-h-56 overflow-y-auto pr-1 opacity-100"
              : "max-h-44 opacity-95"
          }`}
        >
          {shownAccounts.map((a) => (
            <AccountChip
              key={a.UserID}
              account={a}
              avatarUrl={store.avatarUrls.get(a.UserID)}
              onRemove={() => {
                store.handleSelect(a.UserID, { ctrlKey: true, shiftKey: false, metaKey: false } as unknown as React.MouseEvent);
              }}
            />
          ))}
          {!accountsExpanded && remaining > 0 && (
            <div className="theme-muted text-[11px] px-1 py-0.5">
              {t("+{{count}} more", { count: remaining })}
            </div>
          )}
        </div>
        {hasManySelected && (
          <button
            onClick={() => setAccountsExpanded((v) => !v)}
            className="mt-2 w-full theme-btn-ghost text-[11px] px-2 py-1 rounded flex items-center justify-center gap-1.5 transition-colors"
          >
            <ChevronDown
              size={12}
              strokeWidth={2}
              className={`transition-transform duration-200 ${accountsExpanded ? "rotate-180" : "rotate-0"}`}
            />
            {accountsExpanded ? (
              <>
                <span>{t("Show less")}</span>
              </>
            ) : (
              <>
                <span>{t("Show all ({{count}})", { count })}</span>
              </>
            )}
          </button>
        )}
      </div>

      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
        <SidebarSection title={t("Launch")}>
          {store.launchProgress?.mode === "multi" && (
            <div className="theme-accent-bg theme-accent-border mb-2 rounded-lg border px-2.5 py-1.5 animate-fade-in">
              <div className="theme-accent flex items-center gap-2 text-[11px]">
                <span className="w-2 h-2 rounded-full bg-[var(--accent-color)] animate-pulse" />
                <span className="font-medium">
                  {t("Joining {{current}}/{{total}}", {
                    current: store.launchProgress.current,
                    total: store.launchProgress.total,
                  })}
                </span>
              </div>
            </div>
          )}
          <div className="flex flex-col gap-1.5">
            <div className="flex items-center gap-1.5">
              <label className="theme-label text-[10px] w-10 shrink-0">{t("Place")}</label>
              <input
                value={store.placeId}
                onChange={(e) => store.setPlaceId(e.target.value)}
                placeholder={t("Place ID")}
                className="sidebar-input flex-1 font-mono text-xs"
              />
            </div>
            {/* Qual jogo é esse place: o botão abaixo manda TODAS as contas. */}
            {game && (game.name || game.iconUrl) && (
              <div className="flex items-center gap-1.5 pl-[46px]">
                <GameBadge name={game.name} iconUrl={game.iconUrl} placeId={game.placeId} />
              </div>
            )}
            <div className="flex items-center gap-1.5">
              <label className="theme-label text-[10px] w-10 shrink-0">{t("Job")}</label>
              <input
                value={store.jobId}
                onChange={(e) => store.setJobId(e.target.value)}
                placeholder={t("Job ID")}
                className="sidebar-input flex-1 font-mono text-xs"
              />
            </div>
            <div className="flex items-center gap-1.5">
              <label className="theme-label text-[10px] w-10 shrink-0">{t("Data")}</label>
              <input
                value={store.launchData}
                onChange={(e) => store.setLaunchData(e.target.value)}
                placeholder={t("Launch Data")}
                className="sidebar-input flex-1 text-xs"
              />
            </div>
          </div>
          <button
            onClick={handleJoin}
            disabled={store.launchProgress?.mode === "multi"}
            className="sidebar-btn theme-btn mt-1.5 disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {store.launchProgress?.mode === "multi" ? t("Joining...") : t("Join All ({{count}})", { count })}
          </button>
          <button
            onClick={handleRestartLaunchedClients}
            disabled={launchedSelectedCount === 0 || store.launchProgress?.mode === "multi"}
            className="sidebar-btn theme-btn mt-1.5 disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {launchedSelectedCount <= 1
              ? t("Restart launched client")
              : t("Restart launched clients ({{count}})", { count: launchedSelectedCount })}
          </button>
          {showBottingButton && (
            <button
              onClick={() => store.openBottingDialog()}
              className="sidebar-btn theme-btn mt-1.5 bg-[var(--buttons-bg)]/80 border-[var(--buttons-bc)] animate-fade-in"
            >
              {t("Open Botting Mode")}
            </button>
          )}
          {bottingActive && (
            <button
              onClick={handleAddToBottingMode}
              disabled={addableBottingCount <= 0}
              className="sidebar-btn theme-btn mt-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {addableBottingCount <= 0
                ? t("Already in Botting Mode")
                : t(
                    addableBottingCount === 1
                      ? "Add {{count}} account to Botting Mode"
                      : "Add {{count}} accounts to Botting Mode",
                    { count: addableBottingCount }
                  )}
            </button>
          )}
          <button
            onClick={async () => {
              // `cmd_kill_all_roblox` -> `kill_all_roblox()` percorre
              // `get_roblox_pids()`: mata **todo** processo Roblox da maquina, nao
              // so os desta selecao nem so os que o app lancou. O texto tem de
              // dizer isso — era a unica acao destrutiva desta barra sem pergunta.
              const ok = await confirm(
                t(
                  "Close every Roblox process on this computer? This includes clients you opened outside this app, not just the accounts selected here."
                ),
                true
              );
              if (!ok) return;
              store.killAllRobloxProcesses();
            }}
            className={`sidebar-btn theme-btn mt-1.5 text-amber-200 hover:bg-amber-500/15 ${
              pulseCloseAction ? "animate-pulse" : ""
            }`}
          >
            {t("Close All Roblox")}
          </button>
        </SidebarSection>

        <SidebarSection title={t("Batch Actions")}>
          <div className="flex flex-col gap-1.5">
            <button
              onClick={handleRefreshAll}
              disabled={refreshing}
              className="sidebar-btn theme-btn disabled:opacity-50"
            >
              {refreshing ? t("Refreshing...") : t("Refresh Cookies ({{count}})", { count })}
            </button>
            <button onClick={handleCopyCookies} className="sidebar-btn theme-btn">
              {t("Copy All Cookies")}
            </button>

            <div className="relative">
              <button
                onClick={() => setFriendOpen(!friendOpen)}
                disabled={friendBusy}
                className="sidebar-btn theme-btn flex items-center justify-between disabled:opacity-50"
              >
                <span className="flex items-center gap-1.5 min-w-0 truncate">
                  <Users size={13} strokeWidth={1.5} className="shrink-0" />
                  {friendBusy ? friendBtnLabel : t("Make Friends ({{count}})", { count })}
                </span>
                <ChevronDown size={12} strokeWidth={2} className="theme-muted" />
              </button>
              {friendOpen && (
                <div className="theme-panel theme-border absolute left-0 right-0 top-full mt-1 border rounded-lg shadow-xl z-20 p-2.5 animate-scale-in flex flex-col gap-2">
                  <div className="grid grid-cols-2 gap-1.5">
                    {(["star", "mesh"] as const).map((m) => (
                      <button
                        key={m}
                        onClick={() => setFriendMode(m)}
                        className={`px-2 py-1.5 rounded-md border text-[11px] transition-colors ${
                          friendMode === m
                            ? "border-[var(--accent-color)] text-[var(--panel-fg)] bg-[var(--accent-soft)]"
                            : "theme-border theme-muted hover:text-[var(--panel-fg)]"
                        }`}
                      >
                        {m === "star" ? t("Star (1 main)") : t("Mesh (all)")}
                      </button>
                    ))}
                  </div>

                  {friendMode === "star" && (
                    <div className="flex flex-col gap-1">
                      <label className="theme-label text-[10px]">{t("Main account")}</label>
                      <select
                        value={effectiveMain ?? ""}
                        onChange={(e) => setFriendMain(Number(e.target.value))}
                        className="sidebar-input text-xs"
                      >
                        {accounts.map((a) => (
                          <option key={a.UserID} value={a.UserID}>
                            {a.Alias || a.Username}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}

                  <div className="flex items-center gap-1.5">
                    <label className="theme-label text-[10px] flex-1">{t("Delay between requests")}</label>
                    <input
                      type="number"
                      min={0}
                      step={0.5}
                      value={friendDelay}
                      onChange={(e) => setFriendDelay(e.target.value)}
                      className="sidebar-input w-16 text-xs tabular-nums"
                    />
                    <span className="theme-muted text-[10px]">s</span>
                  </div>

                  <p className="theme-muted text-[10px] leading-snug">
                    {friendMode === "star"
                      ? t("Every account will friend the main. Already-friend pairs are skipped. ~{{n}} requests.", { n: friendRequestCount })
                      : t("Every pair will friend each other. Already-friend pairs are skipped. ~{{n}} requests.", { n: friendRequestCount })}
                  </p>

                  <button
                    onClick={handleMakeFriends}
                    disabled={friendBusy || count < 2}
                    className="sidebar-btn theme-btn disabled:opacity-50"
                  >
                    {t("Start")}
                  </button>
                </div>
              )}
            </div>

            <div className="relative">
              <button
                onClick={() => setMoveOpen(!moveOpen)}
                className="sidebar-btn theme-btn flex items-center justify-between"
              >
                <span>{t("Move to Group")}</span>
                <ChevronDown size={12} strokeWidth={2} className="theme-muted" />
              </button>
              {moveOpen && (
                <div className="theme-panel theme-border absolute left-0 right-0 top-full mt-1 border rounded-lg shadow-xl z-20 py-1 max-h-40 overflow-y-auto animate-scale-in">
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
          </div>
        </SidebarSection>

        <SidebarSection title={t("Danger Zone")}>
          <button
            onClick={async () => {
              if (await confirm(tr("Remove {{count}} accounts?", { count }), true)) {
                store.removeAccounts(accounts.map((a) => a.UserID));
              }
            }}
            className="sidebar-btn theme-btn text-red-300/80 hover:bg-red-500/15 hover:text-red-300"
          >
            {t("Remove All ({{count}})", { count })}
          </button>
        </SidebarSection>
      </div>
    </div>
  );
}
