import { useState, useRef, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { usePrompt, useConfirm } from "../../hooks/usePrompt";
import { useCopyCredentialWarning } from "../../hooks/useCopyCredentialWarning";
import { copyAccountSecret } from "../../utils/copySecret";
import { collectGroupNames, parseGroupName } from "../../types";
import { accountLabel } from "../../utils/accountName";
import { tr, useTr } from "../../i18n/text";
import { ChevronDown, Gamepad2, Settings2, Users } from "lucide-react";

/**
 * Faixa aceita para o delay entre pedidos de amizade, em segundos. É a mesma
 * do backend (`resolve_friend_delay_ms` limita a 500–60000 ms): um campo que
 * mostra 0,1 s enquanto o backend usa 0,5 s estaria mentindo.
 */
const FRIEND_DELAY_MIN_S = 0.5;
const FRIEND_DELAY_MAX_S = 60;
const FRIEND_DELAY_DEFAULT_S = 2.5;

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

  // Mesma fonte que o resto do app (`collectGroupNames`): a cópia manual daqui
  // era a última sobrevivente depois que a sidebar de multi-seleção saiu.
  const allGroups = useMemo(() => collectGroupNames(store.accounts), [store.accounts]);

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
        delayMs: friendDelayMs(),
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
    // Pelo backend: fora do histórico do Win+V e apagado sozinho em 30 s.
    try {
      const result = await copyAccountSecret(
        accounts.map((a) => a.UserID),
        "cookie",
        () => cookies.join("\n")
      );
      store.addToast(
        result.clearsInSecs
          ? tr("Copied {{count}} cookies. Cleared from the clipboard in {{seconds}} s.", {
              count: result.count,
              seconds: result.clearsInSecs,
            })
          : tr("Copied {{count}} cookies", { count: result.count })
      );
    } catch {
      store.addToast(tr("Failed to copy"));
    }
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
      store.addToast(t("Start Auto Rejoin first"));
      store.openBottingDialog();
      return;
    }
    if (addableBottingIds.length === 0) {
      store.addToast(t("Selected accounts are already in Auto Rejoin"));
      return;
    }
    try {
      await store.addBottingAccounts(addableBottingIds);
    } catch (e) {
      store.addToast(t("Auto Rejoin account action failed: {{error}}", { error: String(e) }));
    }
  }

  // Com o modo desligado o diálogo não adianta: leva direto para o toggle.
  function handleOpenBottingSettings() {
    setActionsOpen(false);
    store.addToast(t("Auto Rejoin is off — enable it in Settings › General."));
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
  const displayName = singleAccount ? accountLabel(singleAccount, store) : null;

  /**
   * Progresso do Make Friends vindo da store (evento `friend-link-state`).
   * Conta **contas**, não pares: "2 de 3" com 3 pares não dizia quantas contas
   * já tinham terminado. O estado morava aqui num `useState` + listener
   * duplicado na sidebar; agora os dois leem a mesma fonte.
   */
  /**
   * Segundos entre pedidos de amizade. O ritmo é o que decide se o Roblox
   * aplica rate limit (ou pede captcha) no lote inteiro, e o controle existia
   * só na barra lateral de multi-seleção — que ninguém conseguia abrir e foi
   * apagada, deixando o valor editável apenas pelo INI.
   *
   * Guardado em `Friends.RequestDelayMs` (o mesmo que o backend já consultava
   * quando o pedido vem sem delay explícito): ajustar uma vez basta.
   */
  const savedDelayMs = Number(store.settings?.Friends?.RequestDelayMs);
  const savedDelaySeconds =
    Number.isFinite(savedDelayMs) && savedDelayMs > 0 ? savedDelayMs / 1000 : FRIEND_DELAY_DEFAULT_S;
  const [friendDelay, setFriendDelay] = useState(String(savedDelaySeconds));

  // A tela pode abrir antes de as settings chegarem; quando chegam, o campo
  // acompanha — desde que o usuário ainda não tenha digitado nada nele.
  const delayTouchedRef = useRef(false);
  useEffect(() => {
    if (!delayTouchedRef.current) setFriendDelay(String(savedDelaySeconds));
  }, [savedDelaySeconds]);

  /** Fecha a edição: limita à faixa que o backend respeita e persiste. */
  function commitFriendDelay() {
    const parsed = parseFloat(friendDelay.replace(",", "."));
    const seconds = Number.isFinite(parsed)
      ? Math.min(Math.max(parsed, FRIEND_DELAY_MIN_S), FRIEND_DELAY_MAX_S)
      : savedDelaySeconds;
    setFriendDelay(String(seconds));
    delayTouchedRef.current = false;
    const ms = Math.round(seconds * 1000);
    if (ms === Math.round(savedDelaySeconds * 1000)) return;
    invoke("update_setting", { section: "Friends", key: "RequestDelayMs", value: String(ms) })
      .then(() => store.reloadSettings())
      .catch((e) => store.addToast(tr("Could not save: {{error}}", { error: String(e) })));
  }

  /** O que vai no `invoke`: o campo manda, já limitado. */
  function friendDelayMs(): number {
    const parsed = parseFloat(friendDelay.replace(",", "."));
    const seconds = Number.isFinite(parsed)
      ? Math.min(Math.max(parsed, FRIEND_DELAY_MIN_S), FRIEND_DELAY_MAX_S)
      : savedDelaySeconds;
    return Math.round(seconds * 1000);
  }

  const friendLink = store.friendLinkState;
  const friendPhaseLabel =
    friendLink && friendLink.active && friendLink.total > 0
      ? t("Friends {{done}}/{{total}}", {
          done: friendLink.processed,
          total: friendLink.total,
        })
      : t("Linking friends...");

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
          {friendBusy ? friendPhaseLabel : refreshing ? t("Refreshing...") : t("Actions")}
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
                    {/* Rótulo em cima: lado a lado com o campo, ele quebrava em
                        três linhas dentro do menu (medido na tela). */}
                    <label className="flex flex-col gap-1 px-3 py-1.5 text-[12px] theme-muted">
                      <span>{t("Delay between requests (s)")}</span>
                      <input
                        type="number"
                        min={FRIEND_DELAY_MIN_S}
                        max={FRIEND_DELAY_MAX_S}
                        step={0.5}
                        value={friendDelay}
                        onChange={(e) => {
                          delayTouchedRef.current = true;
                          setFriendDelay(e.target.value);
                        }}
                        onBlur={commitFriendDelay}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") commitFriendDelay();
                        }}
                        title={t("Roblox rate-limits new accounts. Slower is safer.")}
                        className="sidebar-input w-full text-[12px] tabular-nums"
                      />
                    </label>
                    <div className="theme-border h-px border-t my-1" />
                    <button
                      onClick={() => handleMakeFriends("mesh", null)}
                      className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)]"
                    >
                      🕸 {t("Mesh - all friend all ({{n}} req)", { n: count * (count - 1) })}
                    </button>
                    <div className="theme-border h-px border-t my-1" />
                    <div className="px-3 py-1 text-[11px] theme-muted uppercase tracking-widest font-semibold">
                      {t("Star - pick main")}
                    </div>
                    {accounts.map((a) => (
                      <button
                        key={a.UserID}
                        onClick={() => handleMakeFriends("star", a.UserID)}
                        className="w-full text-left px-3 py-1.5 text-[12px] text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] truncate"
                      >
                        ⭐ {accountLabel(a, store)}
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
        data-tour="choose-game-button"
        className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[var(--accent-color)] hover:opacity-90 text-white text-sm font-semibold transition-opacity"
      >
        <Gamepad2 size={15} strokeWidth={1.5} />
        {isSingle ? t("Choose Game") : t("Choose Game ({{count}})", { count })}
      </button>
    </div>
  );
}
