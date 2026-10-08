import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Loader2, Search } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { PageShell } from "./PageShell";
import { GroupCards, GroupResults } from "./groups/GroupResults";
import { GroupJoinCard } from "./groups/GroupJoinCard";
import { SessionCache } from "../../utils/sessionCache";
import {
  IDLE_GROUP_JOIN,
  MIN_KEYWORD_LENGTH,
  SEARCH_DEBOUNCE_MS,
  looksLikeGroupReference,
  normalizeSnapshot,
  type GroupJoinSnapshot,
  type GroupJoinStatus,
  type GroupSearchPage,
  type GroupSummary,
} from "./groups/shared";

/**
 * "Grupos populares" (campo vazio): uma consulta por sessão do app. Voltar à
 * página não pede tudo de novo.
 */
const popularGroupsCache = new SessionCache<GroupSummary[]>(1);
const POPULAR_KEY = "popular";

/**
 * Página Groups: acha um grupo do Roblox (por nome, link ou id) e põe as
 * contas marcadas nele, **uma por vez**, com pausa entre elas. A busca roda
 * sozinha 500 ms depois da última tecla (Enter busca na hora); com o campo
 * vazio aparecem os "Grupos populares". Quando o Roblox pede uma confirmação
 * para uma conta (captcha, termos de uso, verificação), ela é pulada — nada
 * tenta resolver — e a linha dela oferece "Abrir no navegador", "Tentar de
 * novo" e "Conferir de novo". Ver docs/features/groups.md.
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
  const [icons, setIcons] = useState<Map<number, string | null>>(new Map());
  const [selected, setSelected] = useState<GroupSummary | null>(null);
  /**
   * Cada busca ganha um número; a resposta de uma busca que já não é a última
   * é jogada fora (senão a resposta lenta de "pe" apagava a de "pet").
   */
  const searchSeqRef = useRef(0);
  /** Texto da última busca começada — o que a lista embaixo mostra. */
  const [startedQuery, setStartedQuery] = useState<string | null>(null);
  const startedQueryRef = useRef<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const requestedIconsRef = useRef<Set<number>>(new Set());

  const markStarted = (text: string | null) => {
    startedQueryRef.current = text;
    setStartedQuery(text);
  };

  const loadIcons = useCallback(async (list: GroupSummary[]) => {
    const ids = list.map((g) => g.id).filter((id) => !requestedIconsRef.current.has(id));
    if (ids.length === 0) return;
    for (const id of ids) requestedIconsRef.current.add(id);
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
    const seq = more ? searchSeqRef.current : ++searchSeqRef.current;
    const isCurrent = () => mountedRef.current && seq === searchSeqRef.current;
    if (more) setLoadingMore(true);
    else {
      markStarted(text);
      // Um "Load more" da busca anterior que ainda não voltou não conta mais.
      setLoadingMore(false);
      setSearching(true);
      setSearchError(null);
    }
    try {
      const page = await invoke<GroupSearchPage>("groups_search", { query: text, cursor });
      if (!isCurrent()) return;
      const found = Array.isArray(page?.groups) ? page.groups : [];
      setGroups((prev) => {
        if (!more) return found;
        const seen = new Set(prev.map((g) => g.id));
        return [...prev, ...found.filter((g) => !seen.has(g.id))];
      });
      setNextCursor(page?.nextCursor ?? null);
      setLastQuery(text);
      // Link ou id colado: um resultado só, já escolhido.
      if (!more && found.length === 1 && looksLikeGroupReference(text)) {
        setSelected(found[0]);
      }
      void loadIcons(found);
    } catch (e) {
      if (!isCurrent()) return;
      if (more) storeRef.current.addToast(t(String(e)), "error");
      else {
        setGroups([]);
        setNextCursor(null);
        setSearchError(t(String(e)));
      }
    } finally {
      if (isCurrent()) {
        if (more) setLoadingMore(false);
        else setSearching(false);
      }
    }
  }

  const trimmedQuery = query.trim();
  const tooShort =
    trimmedQuery.length > 0 &&
    !looksLikeGroupReference(trimmedQuery) &&
    [...trimmedQuery].length < MIN_KEYWORD_LENGTH;

  // Busca sozinha: 500 ms sem mudar o texto. Texto vazio ou curto demais
  // cancela a busca em andamento (a resposta dela é descartada).
  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = null;
    if (!trimmedQuery || tooShort) {
      searchSeqRef.current += 1;
      markStarted(null);
      setSearching(false);
      setLoadingMore(false);
      setSearchError(null);
      setGroups([]);
      setNextCursor(null);
      return;
    }
    if (trimmedQuery === startedQueryRef.current) return;
    debounceRef.current = setTimeout(() => {
      debounceRef.current = null;
      void runSearch(trimmedQuery, null);
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trimmedQuery, tooShort]);

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (!trimmedQuery || tooShort || trimmedQuery === startedQueryRef.current) return;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = null;
    void runSearch(trimmedQuery, null);
  }

  /** Enquanto espera os 500 ms, a lista já diz "Buscando" (não o resultado velho). */
  const waitingForSearch = trimmedQuery !== "" && !tooShort && startedQuery !== trimmedQuery;

  // ---- Grupos populares (campo vazio) -------------------------------------
  const [popular, setPopular] = useState<GroupSummary[] | null>(() => popularGroupsCache.get(POPULAR_KEY) ?? null);
  const [popularFailed, setPopularFailed] = useState(false);
  const popularRequestedRef = useRef(false);

  useEffect(() => {
    if (!active || trimmedQuery) return;
    const cached = popularGroupsCache.get(POPULAR_KEY);
    if (cached) {
      setPopular(cached);
      void loadIcons(cached);
      return;
    }
    if (popularRequestedRef.current) return;
    popularRequestedRef.current = true;
    invoke<GroupSummary[]>("groups_popular")
      .then((list) => {
        const found = Array.isArray(list) ? [...list].sort((a, b) => b.memberCount - a.memberCount) : [];
        if (found.length > 0) popularGroupsCache.set(POPULAR_KEY, found);
        if (!mountedRef.current) return;
        setPopular(found);
        setPopularFailed(found.length === 0);
        void loadIcons(found);
      })
      .catch(() => {
        if (mountedRef.current) setPopularFailed(true);
      });
  }, [active, trimmedQuery, loadIcons]);

  // ---- Lote -----------------------------------------------------------------
  const [batch, setBatch] = useState<GroupJoinSnapshot | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [checking, setChecking] = useState<Set<number>>(new Set());
  const [retrying, setRetrying] = useState<Set<number>>(new Set());
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
        tRef.current("Group join finished: {{joined}} joined, {{pending}} pending, {{confirm}} need a confirmation", {
          joined: count("joined") + count("alreadyMember"),
          pending: count("pending"),
          confirm: count("challenge"),
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
    if (!selected || running || starting || retrying.size > 0 || pickedUserIds.length === 0) return;
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

  async function handleOpenInBrowser(userId: number) {
    if (!selected) return;
    store.addToast(
      t("Opening the browser for {{name}}. Accept what Roblox shows there, then press Try again.", {
        name: nameOf(userId),
      })
    );
    try {
      await invoke("open_account_browser", { userId, groupId: selected.id });
    } catch (e) {
      store.addToast(String(e), "error");
    }
  }

  /** "Tentar de novo": a entrada de novo, só para esta conta. */
  async function handleRetry(userId: number) {
    if (!selected || running || starting || retrying.size > 0) return;
    const groupId = selected.id;
    setRetrying((prev) => new Set(prev).add(userId));
    try {
      const final = await invoke<GroupJoinSnapshot>("groups_join_retry", { userId, groupId });
      if (mountedRef.current) receive(final);
    } catch (e) {
      store.addToast(t(String(e)), "error");
    } finally {
      if (mountedRef.current) {
        setRetrying((prev) => {
          const next = new Set(prev);
          next.delete(userId);
          return next;
        });
      }
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
              accounts: prev.accounts.map((row) =>
                row.userId === userId ? { ...row, status, reason: null, challengeType: null, detail: null } : row
              ),
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
        "Find a Roblox group and put your accounts in it, one at a time. If Roblox asks an account to confirm something (a captcha, new terms), you do it yourself in that account's browser."
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
          <label className="block relative mb-3">
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
              onKeyDown={handleKeyDown}
              placeholder={t("Group name, link or ID")}
              className="w-full pl-8 pr-2.5 py-1.5 rounded-lg text-[12.5px] theme-input"
            />
          </label>
          <div data-tour="groups-results">
            {!trimmedQuery ? (
              <PopularGroups
                groups={popular}
                failed={popularFailed}
                icons={icons}
                selectedId={selected?.id ?? null}
                onSelect={setSelected}
              />
            ) : tooShort ? (
              <div className="py-4 text-[12px] theme-muted">{t("Type at least 2 characters to search.")}</div>
            ) : (
              <GroupResults
                groups={groups}
                icons={icons}
                selectedId={selected?.id ?? null}
                onSelect={setSelected}
                loading={searching || waitingForSearch}
                error={searchError}
                canLoadMore={nextCursor !== null}
                loadingMore={loadingMore}
                onLoadMore={() => void runSearch(lastQuery, nextCursor)}
              />
            )}
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
          retrying={retrying}
          otherBatchRunning={running && batchForSelected === null}
          cancelling={cancelling}
          checking={checking}
          onJoin={() => void handleJoin()}
          onCancel={() => void handleCancel()}
          onOpenInBrowser={(userId) => void handleOpenInBrowser(userId)}
          onRetry={(userId) => void handleRetry(userId)}
          onCheckAgain={(userId) => void handleCheckAgain(userId)}
        />
      </div>
    </PageShell>
  );
}

/**
 * Campo vazio: os grupos grandes da lista curada (`POPULAR_GROUP_IDS` no
 * backend), maior primeiro. Sem resposta, volta a dica de antes.
 */
function PopularGroups({
  groups,
  failed,
  icons,
  selectedId,
  onSelect,
}: {
  groups: GroupSummary[] | null;
  failed: boolean;
  icons: Map<number, string | null>;
  selectedId: number | null;
  onSelect: (group: GroupSummary) => void;
}) {
  const t = useTr();
  if (failed || (groups !== null && groups.length === 0)) {
    return <div className="py-4 text-[12px] theme-muted">{t("Search by name, or paste a group link or ID.")}</div>;
  }
  return (
    <div>
      <div className="mb-2 flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <h3 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("Popular groups")}</h3>
        <span className="text-[11px] theme-muted">{t("Or search by name, or paste a group link or ID.")}</span>
      </div>
      {groups === null ? (
        <div className="flex items-center gap-2 py-6 justify-center text-[12px] theme-muted">
          <Loader2 size={14} className="animate-spin" aria-hidden="true" />
          {t("Loading popular groups...")}
        </div>
      ) : (
        <GroupCards groups={groups} icons={icons} selectedId={selectedId} onSelect={onSelect} label={t("Popular groups")} />
      )}
    </div>
  );
}
