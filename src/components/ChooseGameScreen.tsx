import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { useConfirm, usePrompt } from "../hooks/usePrompt";
import { useJoinOnlineWarning } from "../hooks/useJoinOnlineWarning";
import { FavoritesTab } from "./server-list/FavoritesTab";
import { GamesTab } from "./server-list/GamesTab";
import { RecentTab } from "./server-list/RecentTab";
import { loadFavorites, saveFavorites, recordRecentGame } from "./server-list/types";
import type { GameEntry } from "./server-list/types";
import { tr, useTr } from "../i18n/text";
import { ArrowLeft, User } from "lucide-react";

type TabId = "favorites" | "games" | "recent" | "follow";

function maskName(name: string, previewLetters: number) {
  if (previewLetters > 0 && previewLetters < name.length) return name.slice(0, previewLetters) + "********";
  return "************";
}

/** Launches all selected accounts into a given place/job. */
function useLauncher() {
  const store = useStore();
  const confirmJoinOnline = useJoinOnlineWarning();

  async function launchAll(userIds: number[], placeId: number, jobId: string = "") {
    if (!(await confirmJoinOnline(userIds))) return;
    store.setPlaceId(String(placeId));
    store.setJobId(jobId);
    const maxRecent = parseInt(store.settings?.General?.MaxRecentGames || "8") || 8;
    try {
      if (userIds.length === 1) {
        await store.joinServer(userIds[0]);
      } else {
        await store.launchMultiple(userIds);
      }
      void recordRecentGame(placeId, userIds[0], maxRecent).catch(() => {});
    } catch (e) {
      store.addToast(tr("Launch failed: {{error}}", { error: String(e) }));
    }
  }

  return launchAll;
}

// ── Follow Tab ────────────────────────────────────────────────────────────────
function FollowTab({ userIds }: { userIds: number[] }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const [followUser, setFollowUser] = useState("");
  const [launching, setLaunching] = useState(false);

  async function handleFollow() {
    if (!followUser.trim()) return;
    setLaunching(true);
    try {
      const user = await invoke<{ id: number }>("lookup_user", { username: followUser.trim() });
      const presence = await invoke<{ userPresenceType?: number; user_presence_type?: number }[]>(
        "get_presence", { userIds: [user.id] }
      );
      const presenceType = presence[0]?.userPresenceType ?? presence[0]?.user_presence_type ?? 0;
      if (presenceType < 2) {
        if (!(await confirm(tr("{{name}} is not in a game. Try anyway?", { name: followUser })))) {
          setLaunching(false);
          return;
        }
      }
      // Launch all selected accounts to follow the target
      for (const userId of userIds) {
        await invoke("launch_roblox", {
          userId,
          placeId: user.id,
          jobId: "",
          launchData: "",
          followUser: true,
          joinVip: false,
          linkCode: "",
          shuffleJob: false,
        });
        if (userIds.length > 1) await new Promise((r) => setTimeout(r, 3000));
      }
      store.addToast(tr("Following {{name}} with {{count}} account(s)...", { name: followUser, count: userIds.length }));
    } catch (e) {
      store.addToast(tr("Follow failed: {{error}}", { error: String(e) }));
    } finally {
      setLaunching(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-5">
      {/* Follow card */}
      <div className="theme-panel theme-border border rounded-xl p-5 mb-4">
        <div className="flex items-start justify-between mb-1">
          <h3 className="text-sm font-semibold text-[var(--panel-fg)]">{t("Follow a Player")}</h3>
          <span className="text-[10px] bg-[var(--accent-soft)] text-[var(--accent-color)] px-2 py-0.5 rounded-md font-medium">
            {userIds.length === 1
              ? t("1 account")
              : t("{{count}} accounts", { count: userIds.length })}
          </span>
        </div>
        <p className="text-[11px] theme-muted mb-4 leading-relaxed">
          {userIds.length === 1
            ? t("This account will join the game that this player is currently in.")
            : t("All {{count}} selected accounts will join the same game that this player is currently in, launched one at a time.", { count: userIds.length })}
        </p>

        <label className="text-[11px] theme-label font-medium block mb-1.5">
          {t("Roblox Username")}
        </label>
        <div className="flex gap-2 mb-3">
          <input
            value={followUser}
            onChange={(e) => setFollowUser(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleFollow()}
            placeholder={t("e.g. Builderman")}
            className="sidebar-input flex-1"
            disabled={launching}
          />
          <button
            onClick={handleFollow}
            disabled={launching || !followUser.trim()}
            className="sidebar-btn theme-btn px-4 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {launching ? t("Launching...") : t("Follow")}
          </button>
        </div>

        <div className="text-[11px] theme-muted bg-[var(--panel-soft)] rounded-lg px-3 py-2 leading-relaxed">
          ℹ️ {t("If the player is not currently in a game, you'll be asked to confirm before proceeding. The player's profile must be public.")}
        </div>
      </div>

      {/* Divider */}
      <div className="theme-border border-t my-4" />

      {/* Other batch tools */}
      <div className="theme-panel theme-border border rounded-xl p-4">
        <h3 className="text-xs font-semibold theme-muted uppercase tracking-wider mb-3">
          {t("Other Batch Tools")}
        </h3>
        <div className="grid grid-cols-2 gap-2">
          {[
            { label: t("Server List"), icon: "🖥", onClick: () => store.setServerListOpen(true) },
            { label: t("Utilities"), icon: "🔧", onClick: () => store.setAccountUtilsOpen(true) },
            { label: t("Botting Mode"), icon: "🤖", onClick: () => store.setBottingDialogOpen(true) },
            { label: t("Scripts"), icon: "📜", onClick: () => store.setScriptsOpen(true) },
          ].map(({ label, icon, onClick }) => (
            <button
              key={label}
              onClick={onClick}
              className="sidebar-btn-tool flex items-center gap-2 text-left"
            >
              <span>{icon}</span>
              <span className="text-[11px]">{label}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Main Screen ───────────────────────────────────────────────────────────────
export function ChooseGameScreen() {
  const t = useTr();
  const store = useStore();
  const prompt = usePrompt();
  const launchAll = useLauncher();
  const [activeTab, setActiveTab] = useState<TabId>("favorites");

  const accounts = store.selectedAccounts;
  const userIds = accounts.map((a) => a.UserID);
  const maxRecent = parseInt(store.settings?.General?.MaxRecentGames || "8") || 8;

  // Close on ESC
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") store.setChooseGameOpen(false);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store.setChooseGameOpen]);

  // ── Game selection handlers ────────────────────────────────────────────────

  function handleSelectGame(placeId: number, _name?: string, _iconUrl?: string | null, privateServer?: string) {
    // For Games/Recent tabs: just launch directly
    launchAll(userIds, placeId, privateServer || "");
  }

  async function handleFavoritesSelectGame(placeId: number, privateServer?: string) {
    await launchAll(userIds, placeId, privateServer || "");
  }

  async function handleJoinGame(placeId: number) {
    await launchAll(userIds, placeId, "");
  }

  async function handleAddFavorite(game: GameEntry) {
    const existing = loadFavorites();
    if (existing.some((f) => f.placeId === game.placeId)) {
      store.addToast(t("Already in favorites"));
      return;
    }
    const customName = await prompt(t("Favorite name:"), game.name);
    if (!customName?.trim()) return;
    existing.push({ placeId: game.placeId, name: customName.trim(), iconUrl: game.iconUrl, addedAt: Date.now() });
    saveFavorites(existing);
    store.addToast(t("Added to favorites"));
  }

  // ── Tab definitions ────────────────────────────────────────────────────────
  const TABS: { id: TabId; label: string; hint?: string }[] = [
    {
      id: "favorites",
      label: t("Favorites"),
      hint: t("Your saved games with VIP server links. Click a game to expand and choose public or VIP."),
    },
    {
      id: "games",
      label: t("Games"),
      hint: t("Browse Roblox games. Click a game to launch all selected accounts into it."),
    },
    {
      id: "recent",
      label: t("Recent"),
      hint: t("Games you've joined recently across all accounts."),
    },
    {
      id: "follow",
      label: t("Follow"),
      hint: undefined,
    },
  ];
  const activeHint = TABS.find((t) => t.id === activeTab)?.hint;

  return (
    <div className="flex-1 flex flex-col min-h-0 animate-fade-in">

      {/* ── Header ── */}
      <div className="shrink-0 px-4 pt-3 pb-0 theme-border border-b">
        {/* Top row: back + title */}
        <div className="flex items-center gap-3 mb-3">
          <button
            onClick={() => store.setChooseGameOpen(false)}
            className="flex items-center gap-1.5 text-[11px] theme-muted hover:text-[var(--panel-fg)] px-2.5 py-1.5 rounded-md theme-btn-ghost border theme-border transition-colors"
          >
            <ArrowLeft size={13} strokeWidth={1.5} />
            {t("Back")}
          </button>
          <div>
            <h2 className="text-sm font-semibold text-[var(--panel-fg)]">
              {t("Choose Game")}
            </h2>
            <p className="text-[10px] theme-muted">
              {accounts.length === 1
                ? t("1 account will be launched")
                : t("{{count}} accounts will be launched together", { count: accounts.length })}
            </p>
          </div>
        </div>

        {/* Account chips */}
        <div className="flex flex-wrap gap-1.5 pb-3 max-h-[52px] overflow-hidden">
          {accounts.slice(0, 8).map((a) => {
            const rawName = a.Alias || a.Username;
            const name = store.hideUsernames ? maskName(rawName, store.hiddenNameLetters) : rawName;
            const avatarUrl = store.avatarUrls.get(a.UserID);
            return (
              <div
                key={a.UserID}
                className="flex items-center gap-1.5 bg-[var(--panel-soft)] border theme-border rounded-full pl-0.5 pr-2.5 py-0.5 text-[11px] text-[var(--panel-fg)]"
              >
                {avatarUrl ? (
                  <img src={avatarUrl} alt="" className="w-4 h-4 rounded-full" />
                ) : (
                  <div className="w-4 h-4 rounded-full bg-[var(--panel-muted)] flex items-center justify-center">
                    <User size={8} strokeWidth={1.5} className="theme-muted" />
                  </div>
                )}
                <span className="max-w-[100px] truncate">{name}</span>
              </div>
            );
          })}
          {accounts.length > 8 && (
            <div className="bg-[var(--panel-soft)] border theme-border rounded-full px-2.5 py-0.5 text-[11px] theme-muted">
              +{accounts.length - 8}
            </div>
          )}
        </div>

        {/* Tab bar */}
        <div className="flex gap-0 -mb-px">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-4 py-2.5 text-[12px] border-b-2 transition-colors ${
                activeTab === tab.id
                  ? "border-[var(--accent-color)] text-[var(--panel-fg)] font-medium"
                  : "border-transparent theme-muted hover:text-[var(--panel-fg)]"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>

      {/* ── Tab hint ── */}
      {activeHint && (
        <div className="shrink-0 px-4 pt-2.5 pb-0">
          <p className="text-[11px] theme-muted bg-[var(--panel-soft)] rounded-lg px-3 py-2 leading-relaxed border theme-border">
            💡 {activeHint}
          </p>
        </div>
      )}

      {/* ── Tab content ── */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {activeTab === "favorites" && (
          <div className="h-full overflow-y-auto px-4 pt-3 pb-4">
            <FavoritesTab
              onSelectGame={handleFavoritesSelectGame}
              addToast={store.addToast}
            />
          </div>
        )}
        {activeTab === "games" && (
          <div className="h-full overflow-y-auto px-4 pt-3 pb-4">
            <GamesTab
              onSelectGame={(placeId, name, iconUrl) => handleSelectGame(placeId, name, iconUrl)}
              onJoinGame={handleJoinGame}
              addToast={store.addToast}
              onAddFavorite={handleAddFavorite}
            />
          </div>
        )}
        {activeTab === "recent" && (
          <div className="h-full overflow-y-auto px-4 pt-3 pb-4">
            <RecentTab
              onSelectGame={(placeId, name, iconUrl) => handleSelectGame(placeId, name, iconUrl)}
              maxRecent={maxRecent}
              userId={userIds[0] ?? null}
            />
          </div>
        )}
        {activeTab === "follow" && (
          <FollowTab userIds={userIds} />
        )}
      </div>

      {/* ── Launch progress ── */}
      {store.launchProgress && (
        <div className="shrink-0 px-4 py-2.5 border-t theme-border bg-[var(--panel-soft)] animate-fade-in">
          <div className="flex items-center gap-2 text-[11px] theme-accent">
            <span className="w-2 h-2 rounded-full bg-[var(--accent-color)] animate-pulse" />
            {store.launchProgress.mode === "multi"
              ? t("Launching {{current}}/{{total}} accounts...", {
                  current: store.launchProgress.current,
                  total: store.launchProgress.total,
                })
              : t("Launching account...")}
          </div>
        </div>
      )}
    </div>
  );
}
