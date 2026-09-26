import { useState, useEffect, useRef, type UIEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { useConfirm, usePrompt } from "../hooks/usePrompt";
import { useJoinOnlineWarning } from "../hooks/useJoinOnlineWarning";
import { useEscapeStack } from "../hooks/useEscapeStack";
import { FavoritesTab } from "./server-list/FavoritesTab";
import { GamesTab } from "./server-list/GamesTab";
import { RecentTab } from "./server-list/RecentTab";
import { loadFavorites, recordRecentGame, saveFavorites } from "./server-list/types";
import type { GameEntry } from "./server-list/types";
import { FriendsTab } from "./friends/FriendsTab";
import { ServersTab } from "./servers/ServersTab";
import { tr, useTr } from "../i18n/text";
import { ArrowLeft, User, Trash2, Terminal, LayoutGrid, Check, Link2, AlertTriangle, X } from "lucide-react";
import type { LaunchLogLevel, LaunchTarget } from "../store";
import { TONE_STYLES, type ToneStyle } from "../utils/toastTone";
import type { JoinTarget, PickedServer } from "../types";
import { SessionPanel } from "./session/SessionPanel";

type TabId = "favorites" | "games" | "recent" | "servers" | "friends" | "follow" | "console" | "windows";

function maskName(name: string, previewLetters: number) {
  if (previewLetters > 0 && previewLetters < name.length) return name.slice(0, previewLetters) + "********";
  return "************";
}

/** Extra launch parameters for targets that were already resolved (join links). */
type LaunchExtras = Pick<LaunchTarget, "launchData" | "joinVip" | "linkCode">;

/** Result of a launch attempt, so callers can surface the failure inline. */
type LaunchResult = { ok: boolean; error?: string };

/** Launches all selected accounts into a given place/job. */
function useLauncher() {
  const store = useStore();
  const confirmJoinOnline = useJoinOnlineWarning();
  const confirm = useConfirm();

  /**
   * Resolve o servidor do lote conforme a preferência do usuário
   * (`Random`/`Emptiest`/`Fullest`, com filtro de país opcional).
   *
   * Resolvido **uma vez** para o lote inteiro: todas as contas recebem o mesmo
   * Job ID e caem juntas. Só roda quando o usuário não escolheu servidor — um
   * Job ID explícito, um VIP ou o Follow sempre vencem a preferência.
   *
   * Falha aqui nunca cancela o launch: sem servidor resolvido o lote segue com
   * Job vazio, que é o comportamento de sempre.
   */
  async function resolvePreferredJob(
    userIds: number[],
    placeId: number
  ): Promise<{ jobId: string; cancelled: boolean }> {
    const preference = store.serverPreference;
    if (preference === "none" || userIds.length === 0) {
      return { jobId: "", cancelled: false };
    }
    try {
      const picked = await invoke<PickedServer>("pick_server", {
        userId: userIds[0],
        placeId,
        preference,
        accounts: userIds.length,
        countryCode: store.serverRegionFilter || null,
      });
      if (picked.regionFallback) {
        const ok = await confirm(
          tr("No server found in {{region}}. Join the best available one instead?", {
            region: store.serverRegionFilter,
          })
        );
        if (!ok) return { jobId: "", cancelled: true };
      }
      return { jobId: picked.jobId, cancelled: false };
    } catch (e) {
      store.addToast(tr("Could not pick a server: {{error}}", { error: String(e) }));
      return { jobId: "", cancelled: false };
    }
  }

  async function launchAll(
    userIds: number[],
    placeId: number,
    jobId: string = "",
    onStarted?: () => void,
    extras?: LaunchExtras
  ): Promise<LaunchResult> {
    if (!(await confirmJoinOnline(userIds))) return { ok: false };

    // A preferência só entra quando o usuário não escolheu servidor.
    if (!jobId.trim() && !extras?.joinVip) {
      const preferred = await resolvePreferredJob(userIds, placeId);
      if (preferred.cancelled) return { ok: false };
      jobId = preferred.jobId;
    }
    onStarted?.();
    // Keep the launch input fields in sync for the UI, but pass the target
    // explicitly to the launch call. setPlaceId/setJobId are async state
    // updates, so the launch closure would otherwise read the PREVIOUS
    // place/job — this is what caused clicking a game's VIP to sometimes join
    // the previously-selected game's VIP instead.
    const placeIdText = String(placeId);
    const target: LaunchTarget = { placeId: placeIdText, jobId, ...(extras || {}) };
    store.setPlaceId(placeIdText);
    // The visible Job ID field holds the VIP code in its `vip:` form so a
    // manual re-launch from the field hits the same private server.
    store.setJobId(extras?.joinVip && extras.linkCode ? `vip:${extras.linkCode}` : jobId);
    try {
      if (userIds.length === 1) {
        await store.joinServer(userIds[0], target);
      } else {
        await store.launchMultiple(userIds, target);
      }
      // Recent games are recorded by the store on a successful launch.
      return { ok: true };
    } catch (e) {
      store.addToast(tr("Launch failed: {{error}}", { error: String(e) }));
      return { ok: false, error: String(e) };
    }
  }

  return launchAll;
}

// ── Join link section ─────────────────────────────────────────────────────────
/**
 * Resolves a pasted Roblox link (experience invite, VIP/private server, plain
 * game, `roblox://` deep link, `ro.blox.com` short link) through the backend
 * and launches every selected account into the resolved target.
 */
function JoinLinkSection({ userIds, onGoToConsole }: { userIds: number[]; onGoToConsole?: () => void }) {
  const t = useTr();
  const launchAll = useLauncher();
  const [link, setLink] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resolved, setResolved] = useState<JoinTarget | null>(null);

  function describeTarget(target: JoinTarget): string {
    const kindLabel =
      target.kind === "invite"
        ? t("Invite")
        : target.kind === "private"
          ? t("Private server")
          : target.kind === "job"
            ? t("Server")
            : t("Game");
    const parts = [kindLabel, t("place {{placeId}}", { placeId: String(target.placeId) })];
    if (target.jobId) parts.push(t("server {{jobId}}", { jobId: target.jobId }));
    return parts.join(" · ");
  }

  async function handleJoin() {
    const value = link.trim();
    if (!value || busy || userIds.length === 0) return;
    setBusy(true);
    setError(null);
    setResolved(null);
    try {
      const target = await invoke<JoinTarget>("resolve_join_link", {
        userId: userIds[0],
        link: value,
      });
      setResolved(target);

      // private → `joinVip` + code (launchMultiple re-encodes it as `vip:<code>`)
      // invite/job → the resolved job id; place → no job at all.
      const vipCode = (target.linkCode || target.accessCode || "").trim();
      const isPrivate = target.kind === "private" && vipCode !== "";
      const job = isPrivate || target.kind === "place" ? "" : target.jobId;

      const result = await launchAll(userIds, target.placeId, job, undefined, {
        launchData: target.launchData || undefined,
        joinVip: isPrivate || undefined,
        linkCode: isPrivate ? vipCode : undefined,
      });
      if (!result.ok && result.error) setError(result.error);
    } catch (e) {
      // Keep the typed link so the user can fix it.
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="theme-panel theme-border border rounded-xl p-5 mb-4">
      <div className="flex items-start justify-between mb-1">
        <h3 className="text-sm font-semibold text-[var(--panel-fg)] flex items-center gap-1.5">
          <Link2 size={14} strokeWidth={1.5} />
          {t("Join link")}
        </h3>
        <span className="text-[10px] bg-[var(--accent-soft)] text-[var(--accent-color)] px-2 py-0.5 rounded-md font-medium">
          {userIds.length === 1 ? t("1 account") : t("{{count}} accounts", { count: userIds.length })}
        </span>
      </div>
      <p className="text-[11px] theme-muted mb-4 leading-relaxed">
        {t("Paste an experience invite, a VIP/private server link or a plain game link. All selected accounts join the same place.")}
      </p>

      <label className="text-[11px] theme-label font-medium block mb-1.5">{t("Link")}</label>
      <div className="flex gap-2 mb-3">
        <input
          value={link}
          onChange={(e) => setLink(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleJoin();
          }}
          placeholder={t("e.g. https://www.roblox.com/share?code=abc123&type=ExperienceInvite")}
          className="sidebar-input flex-1 min-w-0"
          spellCheck={false}
          autoComplete="off"
          disabled={busy}
        />
        <button
          onClick={handleJoin}
          disabled={busy || !link.trim() || userIds.length === 0}
          style={{ width: "auto" }}
          className="sidebar-btn theme-btn shrink-0 px-4 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {busy ? t("Resolving link...") : t("Join")}
        </button>
      </div>

      {resolved && (
        <div className="flex items-center justify-between gap-2 text-[11px] text-[var(--panel-fg)] bg-[var(--panel-soft)] border theme-border rounded-lg px-3 py-2 mb-2">
          <span className="truncate">{describeTarget(resolved)}</span>
          {onGoToConsole && (
            <button
              onClick={onGoToConsole}
              className="shrink-0 text-[11px] theme-muted hover:text-[var(--panel-fg)] transition-colors"
            >
              {t("View console")}
            </button>
          )}
        </div>
      )}

      {resolved?.note && (
        <div className="flex items-start gap-2 text-[11px] text-amber-400 bg-[var(--panel-soft)] border theme-border rounded-lg px-3 py-2 mb-2 leading-relaxed">
          <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
          <span>
            {t("This invite is no longer valid ({{note}}) — joining the game's public servers instead.", {
              note: resolved.note,
            })}
          </span>
        </div>
      )}

      {error && (
        <div className="flex items-start gap-2 text-[11px] text-red-400 bg-[var(--panel-soft)] border theme-border rounded-lg px-3 py-2 leading-relaxed">
          <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
          <span className="break-words">{error}</span>
        </div>
      )}
    </div>
  );
}

// ── Follow Tab ────────────────────────────────────────────────────────────────
/** Presença do alvo do Follow — o backend devolve o place e o Job ID do servidor. */
type FollowPresence = {
  userPresenceType?: number;
  user_presence_type?: number;
  placeId?: number | null;
  place_id?: number | null;
  rootPlaceId?: number | null;
  root_place_id?: number | null;
  gameId?: string | null;
  game_id?: string | null;
};

function FollowTab({ userIds, onGoToConsole }: { userIds: number[]; onGoToConsole?: () => void }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const launchAll = useLauncher();
  const [followUser, setFollowUser] = useState("");
  const [launching, setLaunching] = useState(false);

  /**
   * Resolve o servidor do alvo UMA vez e manda todas as contas selecionadas
   * para lá com `launchAll`.
   *
   * Isto já foi um laço próprio de `launch_roblox` com `sleep(3000)` entre
   * contas — o que furava o piso anti-captcha de 8 s que `launch_multiple`
   * aplica no backend. Nada aqui pode voltar a lançar conta por conta.
   */
  async function handleFollow() {
    if (!followUser.trim()) return;
    setLaunching(true);
    try {
      const user = await invoke<{ id: number }>("lookup_user", { username: followUser.trim() });
      const presence = await invoke<FollowPresence[]>("get_presence", { userIds: [user.id] });
      const entry = presence?.[0];
      const presenceType = entry?.userPresenceType ?? entry?.user_presence_type ?? 0;
      const placeId = entry?.rootPlaceId ?? entry?.root_place_id ?? entry?.placeId ?? entry?.place_id ?? null;
      const jobId = entry?.gameId ?? entry?.game_id ?? "";

      if (presenceType < 2 || !placeId) {
        store.addToast(tr("{{name}} is not in a game right now.", { name: followUser }));
        return;
      }
      if (!jobId) {
        // Em jogo, mas com o servidor escondido pela privacidade: só dá para
        // cair num servidor público do mesmo jogo.
        const ok = await confirm(
          tr("{{name}}'s server is not visible. Join a public server of that game instead?", {
            name: followUser,
          })
        );
        if (!ok) return;
      }

      const result = await launchAll(userIds, placeId, jobId, onGoToConsole);
      if (result.ok) {
        store.addToast(
          tr("Following {{name}} with {{count}} account(s)...", { name: followUser, count: userIds.length })
        );
      }
    } catch (e) {
      store.addToast(tr("Follow failed: {{error}}", { error: String(e) }));
    } finally {
      setLaunching(false);
    }
  }

  return (
    <div className="flex-1 overflow-y-auto p-5">
      {/* Join link */}
      <JoinLinkSection userIds={userIds} onGoToConsole={onGoToConsole} />

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
            className="sidebar-input flex-1 min-w-0"
            disabled={launching}
          />
          <button
            onClick={handleFollow}
            disabled={launching || !followUser.trim()}
            style={{ width: "auto" }}
            className="sidebar-btn theme-btn shrink-0 px-4 disabled:opacity-50 disabled:cursor-not-allowed"
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
            { label: t("Botting Mode"), icon: "🤖", onClick: () => store.openBottingDialog() },
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

// ── Grid window arranger ──────────────────────────────────────────────────────
type MonitorInfo = { index: number; width: number; height: number; primary: boolean };

/**
 * Organiza as janelas do Roblox já abertas nos monitores escolhidos.
 *
 * Morava dentro da aba Console, que é o log de lançamento: não tem relação
 * nenhuma com o log e ainda o espremia em ~35 px de altura. Agora é a aba
 * "Windows", e o Console ficou só com a sessão e o log.
 */
function GridControls() {
  const t = useTr();
  const store = useStore();
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [gap, setGap] = useState<number>(() => parseInt(store.settings?.General?.GridGap || "20") || 20);
  // Raw text while typing so the field can be cleared; clamped/saved on blur.
  const [gapText, setGapText] = useState<string>(() => String(gap));
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<MonitorInfo[]>("list_display_monitors")
      .then((mons) => {
        if (cancelled) return;
        const list = (mons || []).map((m) => ({
          index: m.index,
          width: m.width,
          height: m.height,
          primary: !!m.primary,
        }));
        setMonitors(list);
        const saved = (store.settings?.General?.GridMonitors || "")
          .split(",")
          .map((s) => parseInt(s.trim()))
          .filter((n) => Number.isFinite(n));
        const initial =
          saved.length > 0 ? saved.filter((i) => list.some((m) => m.index === i)) : list.map((m) => m.index);
        setSelected(new Set(initial));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function persist(nextSel: Set<number>, nextGap: number) {
    invoke("update_setting", {
      section: "General",
      key: "GridMonitors",
      value: [...nextSel].sort((a, b) => a - b).join(","),
    }).catch(() => {});
    invoke("update_setting", { section: "General", key: "GridGap", value: String(nextGap) }).catch(() => {});
  }

  function toggleMonitor(i: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      persist(next, gap);
      return next;
    });
  }

  function commitGap() {
    const v = parseInt(gapText);
    const g = Number.isFinite(v) ? Math.max(0, Math.min(200, v)) : gap;
    setGapText(String(g));
    if (g !== gap) {
      setGap(g);
      persist(selected, g);
    }
  }

  async function arrange() {
    if (busy) return;
    setBusy(true);
    try {
      const res = await invoke<{ arranged: number; total: number }>("arrange_windows_grid", {
        monitorIndices: [...selected].sort((a, b) => a - b),
        gap,
      });
      const leftover = res.total - res.arranged;
      store.addToast(
        leftover > 0
          ? t("{{arranged}}/{{total}} windows arranged ({{leftover}} didn't fit)", {
              arranged: res.arranged,
              total: res.total,
              leftover,
            })
          : t("{{arranged}} window(s) arranged in grid", { arranged: res.arranged })
      );
    } catch (e) {
      store.addToast(t("Grid failed: {{error}}", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="theme-panel theme-border border rounded-xl p-4">
      <div className="flex items-center justify-between gap-3 mb-3">
        <div className="min-w-0">
          <h3 className="text-[13px] font-semibold text-[var(--panel-fg)]">{t("Window layout")}</h3>
          <p className="text-[11px] theme-muted">
            {t("Tile all open Roblox windows across the selected monitors.")}
          </p>
        </div>
        <button
          onClick={arrange}
          disabled={busy || selected.size === 0}
          style={{ width: "auto" }}
          className="sidebar-btn theme-btn shrink-0 flex items-center gap-1.5 px-3 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <LayoutGrid size={13} strokeWidth={1.5} />
          {busy ? t("Arranging...") : t("Arrange in grid")}
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        {monitors.length === 0 ? (
          <span className="text-[10px] theme-muted">{t("No monitors detected")}</span>
        ) : (
          monitors.map((m) => {
            const on = selected.has(m.index);
            return (
              <button
                key={m.index}
                onClick={() => toggleMonitor(m.index)}
                className={`flex items-center gap-1.5 px-2 py-1 rounded-md border text-[11px] transition-colors ${
                  on
                    ? "border-[var(--accent-color)] text-[var(--panel-fg)] bg-[var(--accent-soft)]"
                    : "theme-border theme-muted hover:text-[var(--panel-fg)]"
                }`}
                title={`${m.width}×${m.height}`}
              >
                <span
                  className={`w-3 h-3 rounded-[3px] border flex items-center justify-center ${
                    on ? "border-[var(--accent-color)] bg-[var(--accent-color)]" : "theme-border"
                  }`}
                >
                  {on && <Check size={9} stroke="var(--forms-bg)" strokeWidth={3} />}
                </span>
                {t("Monitor {{n}}", { n: m.index })}
                {m.primary ? ` ${t("(main)")}` : ""}
                <span className="theme-muted tabular-nums">
                  {m.width}×{m.height}
                </span>
              </button>
            );
          })
        )}

        <div className="flex items-center gap-1.5 ml-auto">
          <label className="text-[10px] theme-label">{t("Gap")}</label>
          <input
            type="number"
            min={0}
            max={200}
            value={gapText}
            onChange={(e) => setGapText(e.target.value)}
            onBlur={commitGap}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitGap();
            }}
            className="sidebar-input w-16 text-xs tabular-nums"
          />
          <span className="text-[10px] theme-muted">px</span>
        </div>
      </div>
    </div>
  );
}

// ── Console Tab ───────────────────────────────────────────────────────────────
/**
 * Os níveis do log de launch são os mesmos tons do feedback de ação, então a
 * paleta é a compartilhada (`TONE_STYLES`) — o toast e o rodapé combinam com
 * este Console em vez de cada tela ter a sua cor.
 */
const LEVEL_STYLES: Record<LaunchLogLevel, ToneStyle> = TONE_STYLES;

function ConsoleTab() {
  const t = useTr();
  const store = useStore();
  const logs = store.launchLogs;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  // Build a userId → display name map from the loaded accounts.
  const nameFor = (userId: number | null): string => {
    if (userId === null) return "—";
    const a = store.accounts.find((acc) => acc.UserID === userId);
    if (!a) return String(userId);
    const raw = a.Alias || a.Username;
    return store.hideUsernames ? maskName(raw, store.hiddenNameLetters) : raw;
  };

  const fmtTime = (ts: number) =>
    new Date(ts).toLocaleTimeString([], { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });

  // Auto-scroll to the newest line unless the user scrolled up.
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, autoScroll]);

  function onScroll(e: UIEvent<HTMLDivElement>) {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  }

  return (
    <div className="flex flex-col h-full">
      {/* Painel de Sessão acima do log: cancelar quem está entrando e
          achar/fechar quem já está em jogo sem sair da tela.
          Teto em 45% da aba: com as duas listas cheias (~500px) o painel
          empurrava o log a pouco mais que o padding — o log é o log de
          lançamento, não o Painel de Sessão. */}
      <div className="shrink-0 pb-3 max-h-[45%] overflow-y-auto">
        <SessionPanel />
      </div>

      <div className="shrink-0 flex items-center justify-between px-1 pb-2">
        <span className="text-[11px] theme-muted">
          {logs.length === 0
            ? t("No activity yet")
            : t("{{count}} log lines", { count: logs.length })}
        </span>
        <button
          onClick={() => store.clearLaunchLogs()}
          disabled={logs.length === 0}
          className="flex items-center gap-1.5 text-[11px] theme-muted hover:text-[var(--panel-fg)] px-2 py-1 rounded-md theme-btn-ghost border theme-border transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Trash2 size={12} strokeWidth={1.5} />
          {t("Clear")}
        </button>
      </div>

      <div
        ref={scrollRef}
        onScroll={onScroll}
        // Piso de 160px: sem ele o Painel de Sessão cheio espremia o log a
        // ~2px de texto visível (26px com 24px de padding).
        className="flex-1 min-h-[160px] overflow-y-auto rounded-lg border theme-border bg-[var(--panel-soft)] font-mono text-[11px] leading-relaxed p-3"
      >
        {logs.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center theme-muted gap-2 py-10">
            <Terminal size={22} strokeWidth={1.5} />
            <p className="text-[11px]">
              {t("Launch a game or start Botting Mode to see the activity here")}
            </p>
          </div>
        ) : (
          logs.map((log) => {
            const style = LEVEL_STYLES[log.level] ?? LEVEL_STYLES.info;
            return (
              <div key={log.id} className="flex items-start gap-2 py-0.5">
                <span className="theme-muted shrink-0 tabular-nums">{fmtTime(log.ts)}</span>
                <span className={`shrink-0 w-1.5 h-1.5 rounded-full mt-[6px] ${style.dot}`} />
                {/* De onde veio a linha. O `step` sempre existiu no evento e
                    nunca era desenhado; agora que o console tem launch,
                    Botting e Watcher juntos, ele é o que separa um do outro. */}
                {log.step ? (
                  <span
                    data-testid="log-step"
                    // Largura fixa: sem ela, `[watcher]` e `[botting-retry]`
                    // empurram o nome da conta para colunas diferentes e a
                    // leitura vertical do log se perde.
                    className="shrink-0 theme-muted w-[104px] truncate"
                    title={log.step}
                  >
                    [{log.step}]
                  </span>
                ) : null}
                <span className="shrink-0 text-[var(--accent-color)] max-w-[120px] truncate">
                  {nameFor(log.userId)}
                </span>
                <span className={`${style.text} break-words`}>{log.message}</span>
              </div>
            );
          })
        )}
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

  // Esta tela fica na **base** da pilha de Escape: qualquer diálogo ou popover
  // aberto por cima monta depois e consome o Escape antes. Antes ela era sempre
  // o primeiro listener de `window`, então um Escape fechava o diálogo de cima
  // **e** a tela de trás no mesmo evento. `ignoreFromFields` deixa o Escape
  // digitado num campo (o link da aba Follow, por exemplo) passar direto.
  useEscapeStack(true, () => store.setChooseGameOpen(false), { ignoreFromFields: true });

  // ── Game selection handlers ────────────────────────────────────────────────

  const goToConsole = () => setActiveTab("console");

  /**
   * Clique no card, nas abas Games/Recent.
   *
   * Já lançou direto — o único freio era `confirmJoinOnline`, que só entra
   * com a conta online, então uma conta offline lançava sem passo nenhum no
   * meio. Desde o P1, Games/Recent já têm a própria ação "Join Game"
   * (`handleJoinGame`, no ícone da linha), então o card lançando também era
   * um gesto duplicado. Agora o card só abre os servidores do jogo — mesmo
   * destino que `ServerListDialog.handleSelectGame` já usa para o clique no
   * card lá.
   *
   * Decisão sobre o Recent: como o jogo só entra na lista num launch
   * bem-sucedido (`store.tsx`), e o card não lança mais nada, gravamos aqui
   * manualmente — a mesma saída que o `ServerListDialog` já usa — para o
   * jogo clicado não sumir do Recent.
   */
  function handleSelectGame(placeId: number, name?: string, iconUrl?: string | null) {
    void recordRecentGame(placeId, userIds[0] ?? null, maxRecent, { name, iconUrl }).catch(() => {});
    handleBrowseServers(placeId);
  }

  async function handleFavoritesSelectGame(placeId: number, privateServer?: string) {
    await launchAll(userIds, placeId, privateServer || "", goToConsole);
  }

  async function handleJoinGame(placeId: number) {
    await launchAll(userIds, placeId, "", goToConsole);
  }

  /** Vai para a aba Servers já com o place do jogo clicado. */
  function handleBrowseServers(placeId: number) {
    store.setPlaceId(String(placeId));
    setActiveTab("servers");
  }

  /**
   * Abre o Botting Mode **com o jogo escolhido**. O place vai explícito na
   * abertura porque o rascunho salvo (`General.BottingDraftPlaceId`) vence a
   * store: sem isso, escolher o jogo aqui e ver outro place no diálogo.
   */
  function handleBottingForGame(placeId: number) {
    store.setPlaceId(String(placeId));
    store.openBottingDialog(String(placeId));
  }

  /** Abre os Scripts com este jogo como place atual (é o que `ram.window` expõe). */
  function handleScriptsForGame(placeId: number) {
    store.setPlaceId(String(placeId));
    store.setScriptsOpen(true);
  }

  /**
   * Tira uma conta do lote pelo "x" do chip. A última não sai: um lote vazio
   * deixaria a tela sem nada para lançar.
   */
  function handleRemoveAccount(userId: number) {
    if (userIds.length <= 1) return;
    store.setSelectedIds(new Set(userIds.filter((id) => id !== userId)));
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
  /**
   * Cada aba explica o que faz na dica 💡. Algumas ganham um atalho ao lado da
   * dica: é o caminho curto para a aba que resolve o caso que a pessoa tem em
   * mãos (colar um link, por exemplo) sem duplicar a lógica que resolve.
   */
  const TABS: { id: TabId; label: string; hint?: string; action?: { label: string; onClick: () => void } }[] = [
    {
      id: "favorites",
      label: t("Favorites"),
      hint: t("Your saved games with VIP server links. Click a game to expand and choose public or VIP."),
    },
    {
      id: "games",
      label: t("Games"),
      // Clicar no card não lança mais direto (item de usabilidade: era um
      // duplicado do botão "Join Game" da própria linha, sem nenhum passo no
      // meio). A dica tinha que parar de prometer isso.
      hint: t("Browse Roblox games. Click a game to see its servers, or use Join Game to launch directly."),
    },
    {
      id: "recent",
      label: t("Recent"),
      hint: t("Games you've joined recently across all accounts. Click a game to see its servers, or use Join Game to launch directly."),
    },
    {
      id: "servers",
      label: t("Servers"),
      hint: t("Public servers of a place. Pick one and every selected account joins it; load regions to find a specific country."),
      // O campo daqui só aceita Place ID: quem colou um convite ou um link de
      // servidor privado resolve isso na aba Follow, a um clique.
      action: { label: t("Paste a join link"), onClick: () => setActiveTab("follow") },
    },
    {
      id: "friends",
      label: t("Friends"),
      hint: t("Online friends of each selected account. Click a friend to send every selected account into their server."),
    },
    {
      id: "follow",
      label: t("Follow"),
      hint: t("Paste any Roblox link here — experience invite, VIP/private server or a plain game link — or follow a player into the game they are in right now."),
    },
    {
      id: "console",
      label: t("Console"),
      hint: t("Live history of what the app did: launch, Botting Mode, Watcher and errors, with the origin of each line."),
    },
    {
      id: "windows",
      label: t("Windows"),
      hint: t("Tile the Roblox windows that are already open across your monitors."),
    },
  ];
  const activeTabDef = TABS.find((tab) => tab.id === activeTab);
  const activeHint = activeTabDef?.hint;
  const activeAction = activeTabDef?.action;

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
                className="group flex items-center gap-1.5 bg-[var(--panel-soft)] border theme-border rounded-full pl-0.5 pr-1 py-0.5 text-[11px] text-[var(--panel-fg)]"
              >
                {avatarUrl ? (
                  <img src={avatarUrl} alt="" className="w-4 h-4 rounded-full" />
                ) : (
                  <div className="w-4 h-4 rounded-full bg-[var(--panel-muted)] flex items-center justify-center">
                    <User size={8} strokeWidth={1.5} className="theme-muted" />
                  </div>
                )}
                <span className="max-w-[100px] truncate">{name}</span>
                <button
                  onClick={() => handleRemoveAccount(a.UserID)}
                  disabled={accounts.length <= 1}
                  aria-label={t("Remove {{name}} from this launch", { name })}
                  title={
                    accounts.length <= 1
                      ? t("Keep at least one account selected")
                      : t("Remove from this launch")
                  }
                  className="w-4 h-4 rounded-full flex items-center justify-center theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--panel-muted)] disabled:opacity-0 transition-colors"
                >
                  <X size={10} strokeWidth={2} />
                </button>
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
          <div className="flex items-center gap-3 text-[11px] theme-muted bg-[var(--panel-soft)] rounded-lg px-3 py-2 leading-relaxed border theme-border">
            <p className="min-w-0">💡 {activeHint}</p>
            {activeAction && (
              <button
                onClick={activeAction.onClick}
                className="shrink-0 ml-auto flex items-center gap-1.5 text-[11px] px-2.5 py-1 rounded-md theme-btn-ghost border theme-border text-[var(--panel-fg)] transition-colors"
              >
                <Link2 size={12} strokeWidth={1.5} />
                {activeAction.label}
              </button>
            )}
          </div>
        </div>
      )}

      {/* ── Tab content ── */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {activeTab === "favorites" && (
          <div className="h-full overflow-y-auto px-4 pt-3 pb-4">
            <FavoritesTab
              onSelectGame={handleFavoritesSelectGame}
              addToast={store.addToast}
              onBrowseServers={handleBrowseServers}
              onBotting={handleBottingForGame}
              onScripts={handleScriptsForGame}
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
              onBrowseServers={(placeId) => handleBrowseServers(placeId)}
              onBotting={handleBottingForGame}
              onScripts={handleScriptsForGame}
            />
          </div>
        )}
        {activeTab === "recent" && (
          <div className="h-full overflow-y-auto px-4 pt-3 pb-4">
            <RecentTab
              onSelectGame={(placeId, name, iconUrl) => handleSelectGame(placeId, name, iconUrl)}
              maxRecent={maxRecent}
              userId={userIds[0] ?? null}
              onBrowseServers={handleBrowseServers}
              onAddFavorite={handleAddFavorite}
              onBotting={handleBottingForGame}
              onScripts={handleScriptsForGame}
            />
          </div>
        )}
        {activeTab === "servers" && (
          <ServersTab
            userIds={userIds}
            placeId={store.placeId}
            setPlaceId={store.setPlaceId}
            launchAll={launchAll}
            onGoToConsole={goToConsole}
          />
        )}
        {activeTab === "friends" && (
          <FriendsTab userIds={userIds} launchAll={launchAll} onGoToConsole={goToConsole} />
        )}
        {activeTab === "follow" && (
          <FollowTab userIds={userIds} onGoToConsole={goToConsole} />
        )}
        {activeTab === "console" && (
          <div className="h-full px-4 pt-3 pb-4">
            <ConsoleTab />
          </div>
        )}
        {activeTab === "windows" && (
          <div className="h-full overflow-y-auto px-4 pt-3 pb-4">
            <GridControls />
          </div>
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
