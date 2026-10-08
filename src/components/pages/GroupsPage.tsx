import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Search } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { PageShell } from "./PageShell";
import { GroupResults } from "./groups/GroupResults";
import { GroupJoinCard } from "./groups/GroupJoinCard";
import {
  IDLE_GROUP_JOIN,
  normalizeSnapshot,
  type GroupJoinSnapshot,
  type GroupJoinStatus,
  type GroupSearchPage,
  type GroupSummary,
} from "./groups/shared";

/**
 * Página Groups: acha um grupo do Roblox (por nome, link ou id) e põe as
 * contas marcadas nele, **uma por vez**, com pausa entre elas. Quando o Roblox
 * pede captcha para uma conta, ela é pulada (nada tenta resolver captcha) e a
 * linha dela oferece "Resolver no navegador": o navegador da própria conta
 * abre na página do grupo e a pessoa entra por lá. Ver docs/features/groups.md.
 *
 * O lote roda no backend e chega inteiro a cada passo pelo evento
 * `groups-join-state` — a página pode sair e voltar no meio
 * (`get_groups_join_state`).
 */
export function GroupsPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const store = useStore();
  const accountLabel = useAccountLabel();

  const storeRef = useRef(store);
  storeRef.current = store;
  const tRef = useRef(t);
  tRef.current = t;
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // ---- Busca ----------------------------------------------------------------
  const [query, setQuery] = useState("");
  const [lastQuery, setLastQuery] = useState("");
  const [groups, setGroups] = useState<GroupSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);
  const [icons, setIcons] = useState<Map<number, string | null>>(new Map());
  const [selected, setSelected] = useState<GroupSummary | null>(null);

  const loadIcons = useCallback(async (list: GroupSummary[]) => {
    const ids = list.map((g) => g.id);
    if (ids.length === 0) return;
    try {
      const thumbs = await invoke<{ targetId: number; imageUrl: string | null }[]>("groups_icons", { groupIds: ids });
      if (!mountedRef.current) return;
      setIcons((prev) => {
        const next = new Map(prev);
        for (const id of ids) next.set(id, null);
        for (const thumb of Array.isArray(thumbs) ? thumbs : []) {
          next.set(thumb.targetId, thumb.imageUrl || null);
        }
        return next;
      });
    } catch {
      if (!mountedRef.current) return;
      setIcons((prev) => {
        const next = new Map(prev);
        for (const id of ids) if (!next.has(id)) next.set(id, null);
        return next;
      });
    }
  }, []);

  async function runSearch(text: string, cursor: string | null) {
    const more = cursor !== null;
    if (more) setLoadingMore(true);
    else {
      setSearching(true);
      setSearchError(null);
    }
    try {
      const page = await invoke<GroupSearchPage>("groups_search", { query: text, cursor });
      if (!mountedRef.current) return;
      const found = Array.isArray(page?.groups) ? page.groups : [];
      setGroups((prev) => {
        if (!more) return found;
        const seen = new Set(prev.map((g) => g.id));
        return [...prev, ...found.filter((g) => !seen.has(g.id))];
      });
      setNextCursor(page?.nextCursor ?? null);
      setSearched(true);
      setLastQuery(text);
      // Link ou id colado: um resultado só, já escolhido.
      if (!more && found.length === 1 && /^\s*(https?:\/\/|www\.|roblox\.com|\d+\s*$)/i.test(text)) {
        setSelected(found[0]);
      }
      void loadIcons(found);
    } catch (e) {
      if (!mountedRef.current) return;
      if (more) storeRef.current.addToast(t(String(e)), "error");
      else {
        setGroups([]);
        setNextCursor(null);
        setSearched(true);
        setSearchError(t(String(e)));
      }
    } finally {
      if (mountedRef.current) {
        setSearching(false);
        setLoadingMore(false);
      }
    }
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const text = query.trim();
    if (!text || searching) return;
    void runSearch(text, null);
  }

  // ---- Lote -----------------------------------------------------------------
  const [batch, setBatch] = useState<GroupJoinSnapshot | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [checking, setChecking] = useState<Set<number>>(new Set());
  const prevRunningRef = useRef(false);

  const receive = useCallback((raw: unknown) => {
    const snapshot = normalizeSnapshot(raw);
    if (!snapshot) return;
    const finished = prevRunningRef.current && !snapshot.running;
    prevRunningRef.current = snapshot.running;
    setBatch(snapshot);
    if (!snapshot.running) setCancelling(false);
    if (finished && snapshot.accounts.length > 0) {
      const count = (status: GroupJoinStatus) => snapshot.accounts.filter((a) => a.status === status).length;
      storeRef.current.addToast(
        tRef.current("Group join finished: {{joined}} joined, {{pending}} pending, {{captcha}} need a captcha", {
          joined: count("joined") + count("alreadyMember"),
          pending: count("pending"),
          captcha: count("challenge"),
        })
      );
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen<GroupJoinSnapshot>("groups-join-state", (event) => receive(event.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [receive]);

  /** Contas que entram: nasce da seleção da lista principal a cada abertura. */
  const [picked, setPicked] = useState<Set<number>>(new Set());

  useEffect(() => {
    if (!active) return;
    setPicked(new Set(storeRef.current.selectedAccounts.map((a) => a.UserID)));
    invoke<GroupJoinSnapshot>("get_groups_join_state")
      .then((raw) => {
        if (!mountedRef.current) return;
        const snapshot = normalizeSnapshot(raw);
        if (!snapshot) return;
        receive(snapshot);
        // Voltou no meio de um lote: o grupo dele aparece escolhido.
        if (snapshot.groupId !== null && snapshot.accounts.length > 0) {
          setSelected(
            (current) =>
              current ?? {
                id: snapshot.groupId as number,
                name: snapshot.groupName,
                description: "",
                memberCount: 0,
                publicEntryAllowed: true,
                hasVerifiedBadge: false,
                isLocked: false,
              }
          );
          if (snapshot.running) setPicked(new Set(snapshot.accounts.map((a) => a.userId)));
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  useEffect(() => {
    if (selected && !icons.has(selected.id)) void loadIcons([selected]);
  }, [selected, icons, loadIcons]);

  const running = batch?.running === true;
  const batchForSelected = selected && batch?.groupId === selected.id ? batch : null;
  const pickedUserIds = store.accounts.filter((a) => picked.has(a.UserID)).map((a) => a.UserID);

  async function handleJoin() {
    if (!selected || running || starting || pickedUserIds.length === 0) return;
    setStarting(true);
    try {
      const final = await invoke<GroupJoinSnapshot>("groups_join_batch", {
        userIds: pickedUserIds,
        groupId: selected.id,
      });
      if (mountedRef.current) receive(final);
    } catch (e) {
      store.addToast(t(String(e)), "error");
      prevRunningRef.current = false;
      try {
        receive((await invoke<GroupJoinSnapshot>("get_groups_join_state")) ?? IDLE_GROUP_JOIN);
      } catch {
        receive(IDLE_GROUP_JOIN);
      }
    } finally {
      if (mountedRef.current) setStarting(false);
    }
  }

  async function handleCancel() {
    setCancelling(true);
    try {
      await invoke("groups_cancel_join");
    } catch (e) {
      setCancelling(false);
      store.addToast(String(e), "error");
    }
  }

  function nameOf(userId: number): string {
    const account = store.accounts.find((a) => a.UserID === userId);
    return accountLabel(account, `${t("User ID")}: ${userId}`);
  }

  async function handleSolveInBrowser(userId: number) {
    if (!selected) return;
    store.addToast(
      t("Opening the browser for {{name}}. Solve the captcha and click Join there, then press Check again.", {
        name: nameOf(userId),
      })
    );
    try {
      await invoke("open_account_browser", { userId, groupId: selected.id });
    } catch (e) {
      store.addToast(String(e), "error");
    }
  }

  async function handleCheckAgain(userId: number) {
    if (!selected) return;
    const groupId = selected.id;
    setChecking((prev) => new Set(prev).add(userId));
    try {
      const status = await invoke<GroupJoinStatus>("groups_check_membership", { userId, groupId });
      if (!mountedRef.current) return;
      setBatch((prev) =>
        prev && prev.groupId === groupId
          ? {
              ...prev,
              accounts: prev.accounts.map((row) => (row.userId === userId ? { ...row, status, reason: null } : row)),
            }
          : prev
      );
      if (status === "notMember") {
        store.addToast(t("{{name}} is not in the group yet", { name: nameOf(userId) }));
      }
    } catch (e) {
      store.addToast(String(e), "error");
    } finally {
      if (mountedRef.current) {
        setChecking((prev) => {
          const next = new Set(prev);
          next.delete(userId);
          return next;
        });
      }
    }
  }

  if (!active) return null;

  return (
    <PageShell
      title={t("Groups")}
      description={t(
        "Find a Roblox group and put your accounts in it, one at a time. If Roblox asks for a captcha, you solve it yourself in that account's browser."
      )}
      onLeave={onLeave}
      dataTour="groups-page"
      tour="groups"
      actions={
        running ? (
          <span className="px-2.5 py-1 rounded-full text-[11.5px] border border-emerald-500/30 bg-emerald-500/15 text-emerald-300 animate-pulse">
            {t("Joining {{done}}/{{total}}", { done: batch?.done ?? 0, total: batch?.total ?? 0 })}
          </span>
        ) : null
      }
    >
      <div className="space-y-4">
        <section
          data-tour="groups-search"
          aria-label={t("Search groups")}
          className="@container theme-surface rounded-xl border theme-border p-3"
        >
          <form onSubmit={handleSubmit} className="flex flex-wrap items-center gap-2 mb-3">
            <label className="flex-1 min-w-[200px] relative">
              <span className="sr-only">{t("Group name, link or ID")}</span>
              <Search
                size={14}
                aria-hidden="true"
                className="absolute left-2.5 top-1/2 -translate-y-1/2 theme-muted pointer-events-none"
              />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("Group name, link or ID")}
                className="w-full pl-8 pr-2.5 py-1.5 rounded-lg text-[12.5px] theme-input"
              />
            </label>
            <button type="submit" disabled={!query.trim() || searching} className="sidebar-btn-sm px-3 font-medium disabled:opacity-50 disabled:cursor-not-allowed">
              {t("Search")}
            </button>
          </form>
          <div data-tour="groups-results">
            <GroupResults
              groups={groups}
              icons={icons}
              selectedId={selected?.id ?? null}
              onSelect={setSelected}
              loading={searching}
              error={searchError}
              searched={searched}
              canLoadMore={nextCursor !== null}
              loadingMore={loadingMore}
              onLoadMore={() => void runSearch(lastQuery, nextCursor)}
            />
          </div>
        </section>

        <GroupJoinCard
          group={selected}
          icon={selected ? icons.get(selected.id) : undefined}
          accounts={store.accounts}
          picked={picked}
          onPickedChange={setPicked}
          batch={batchForSelected}
          starting={starting}
          otherBatchRunning={running && batchForSelected === null}
          cancelling={cancelling}
          checking={checking}
          onJoin={() => void handleJoin()}
          onCancel={() => void handleCancel()}
          onSolveInBrowser={(userId) => void handleSolveInBrowser(userId)}
          onCheckAgain={(userId) => void handleCheckAgain(userId)}
        />
      </div>
    </PageShell>
  );
}
