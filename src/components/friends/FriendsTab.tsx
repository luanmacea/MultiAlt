import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AlertTriangle, Gamepad2, Loader2, RefreshCw, User, Users } from "lucide-react";
import { useStore } from "../../store";
import { accountLabel, hideAccountAvatar } from "../../utils/accountName";
import { useTr } from "../../i18n/text";
import { SessionCache } from "../../utils/sessionCache";
import type {
  AccountFriends,
  FriendsOnlineProgress,
  OnlineFriend,
  ThumbnailData,
} from "../../types";

/**
 * Aba "Friends" da Choose Game.
 *
 * Lista os amigos **online** de cada conta selecionada, agrupados por conta, e
 * manda **todas** as contas selecionadas para o servidor do amigo clicado.
 *
 * Regras de produto que não podem se perder:
 *
 * - O launch passa SEMPRE por `launchAll` (`useLauncher` da Choose Game), que
 *   chama `launch_multiple`. Um laço próprio com `launch_roblox` fura o piso
 *   anti-captcha de 8 s que o backend aplica entre contas.
 * - A lista mostra TODOS os online, não só os entráveis: quem não dá para
 *   seguir aparece esmaecido com o motivo, senão o usuário fica sem entender
 *   por que o amigo sumiu.
 * - Nomes dos AMIGOS nunca são mascarados (são de terceiros e o usuário precisa
 *   reconhecê-los); só os nomes das contas do próprio usuário respeitam
 *   `hideUsernames`.
 */

/** Espaço entre as consultas de cada conta, para não tomar 429 da API. */
const FRIENDS_REQUEST_DELAY_MS = 1200;

/**
 * Última lista de cada seleção de contas (chave: os ids na ordem da seleção).
 *
 * Sair da aba desmontava tudo, e a volta começava da tela vazia esperando o
 * lote inteiro de novo. Agora a volta mostra esta lista na hora e atualiza por
 * trás — cada conta é trocada quando a resposta dela chega.
 */
const friendsCache = new SessionCache<AccountFriends[]>();

/** Miniatura de cada amigo já buscada (chave: userId do amigo). */
const friendAvatarCache = new SessionCache<string>(1000);

/**
 * Uma rodada do lote no backend. Fica registrada enquanto roda para que voltar
 * à aba no meio dela **se junte** à rodada em vez de começar outra — duas
 * rodadas ao mesmo tempo dobrariam as chamadas à API de amigos, cujo rate
 * limit é por IP (todas as contas saem do mesmo).
 */
interface FriendsRun {
  requestId: number;
  promise: Promise<AccountFriends[]>;
  /** Contas que já voltaram nesta rodada (as outras ainda mostram o cache). */
  arrived: Set<number>;
}

const runningLoads = new SessionCache<FriendsRun>();
let nextRequestId = 1;

function startOrJoinRun(key: string, ids: number[]): FriendsRun {
  const existing = runningLoads.get(key);
  if (existing) return existing;
  const requestId = nextRequestId++;
  const promise = invoke<AccountFriends[]>("get_online_friends_for_accounts", {
    userIds: ids,
    delayMs: FRIENDS_REQUEST_DELAY_MS,
    requestId,
  }).then((rows) => {
    const list = rows || [];
    friendsCache.set(key, list);
    return list;
  });
  const run: FriendsRun = { requestId, promise, arrived: new Set() };
  runningLoads.set(key, run);
  const settle = () => {
    if (runningLoads.get(key) === run) runningLoads.delete(key);
  };
  promise.then(settle, settle);
  return run;
}

/**
 * Põe a entrada de uma conta no lugar dela: troca a que já existia ou entra na
 * posição da conta na seleção — a ordem da tela é a da seleção, não a ordem em
 * que as respostas chegam.
 */
export function upsertAccountFriends(
  rows: AccountFriends[],
  entry: AccountFriends,
  order: number[]
): AccountFriends[] {
  const next = rows.filter((row) => row.userId !== entry.userId);
  next.push(entry);
  const position = (userId: number) => {
    const index = order.indexOf(userId);
    return index === -1 ? Number.MAX_SAFE_INTEGER : index;
  };
  return next.sort((a, b) => position(a.userId) - position(b.userId));
}

/**
 * Assinatura de `useLauncher().launchAll`. Declarada aqui (e não importada da
 * `ChooseGameScreen`) só para não criar import circular — a Choose Game importa
 * esta aba.
 */
export type LaunchAllFn = (
  userIds: number[],
  placeId: number,
  jobId?: string,
  onStarted?: () => void
) => Promise<{ ok: boolean; error?: string }>;

/** Um amigo ou é entrável (com alvo resolvido) ou tem um motivo para não ser. */
export type FriendTarget =
  | { joinable: true; placeId: number; jobId: string }
  | { joinable: false; reason: string };

type Translate = ReturnType<typeof useTr>;

/**
 * Decide se dá para entrar no servidor do amigo e, quando não dá, por quê.
 *
 * `placeId` tem prioridade sobre `rootPlaceId`: o Job ID é de um servidor **do
 * place em que o amigo está**, que pode ser um sub-place (Life Sentence tem 6 —
 * o raiz distribui para "VC Only", "Pro Players"...). O raiz com o Job ID de um
 * sub-place pede um servidor que não existe no raiz, e o cliente abria em "This
 * experience has ended". O raiz fica de reserva, para quando o `placeId` não
 * vem.
 */
export function friendTarget(friend: OnlineFriend, t: Translate): FriendTarget {
  if (friend.presenceType === 3) return { joinable: false, reason: t("In Studio") };
  if (friend.presenceType === 1) return { joinable: false, reason: t("On the website") };

  const placeId = friend.placeId ?? friend.rootPlaceId;
  if (friend.presenceType === 2 && friend.gameId && placeId) {
    return { joinable: true, placeId, jobId: friend.gameId };
  }
  // Em jogo, mas sem Job ID: a privacidade do amigo esconde o servidor.
  return { joinable: false, reason: t("Server not visible") };
}

export interface FriendsTabProps {
  /** Contas selecionadas — todas entram juntas no servidor do amigo clicado. */
  userIds: number[];
  /** `useLauncher().launchAll` da Choose Game. Único caminho de launch. */
  launchAll: LaunchAllFn;
  onGoToConsole?: () => void;
}

export function FriendsTab({ userIds, launchAll, onGoToConsole }: FriendsTabProps) {
  const t = useTr();
  const store = useStore();

  // A seleção muda de identidade a cada render da Choose Game; a chave é o que
  // realmente define "outra seleção".
  const userIdsKey = userIds.join(",");
  const userIdsRef = useRef(userIds);
  userIdsRef.current = userIds;
  const userIdsKeyRef = useRef(userIdsKey);
  userIdsKeyRef.current = userIdsKey;

  // Nasce com a última lista desta seleção, se houver: a volta à aba não
  // começa vazia.
  const [groups, setGroups] = useState<AccountFriends[] | null>(
    () => friendsCache.get(userIdsKey) ?? null
  );
  // Toda montagem com contas consulta o backend; já nascer "carregando" evita
  // um quadro com o estado vazio antes do efeito rodar.
  const [loading, setLoading] = useState(userIds.length > 0);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<FriendsOnlineProgress | null>(null);
  const [joining, setJoining] = useState<number | null>(null);
  /** Contas que já voltaram na rodada atual (as outras mostram o cache ou "pendente"). */
  const [arrived, setArrived] = useState<Set<number>>(new Set());
  const [, setAvatarVersion] = useState(0);

  /** A rodada que esta tela está acompanhando — eventos de outra são ignorados. */
  const runRef = useRef<FriendsRun | null>(null);
  const avatarsLoadingRef = useRef<Set<number>>(new Set());

  /**
   * Busca as miniaturas dos amigos ainda sem avatar, **em lote** (mesmo comando
   * usado pela lista de contas). O cache é da sessão: voltar à aba não refaz a
   * chamada para quem já tem foto.
   */
  const loadFriendAvatars = useCallback(async (rows: AccountFriends[]) => {
    const wanted = new Set<number>();
    for (const row of rows) for (const friend of row.friends) wanted.add(friend.userId);
    const missing = [...wanted].filter(
      (id) => !friendAvatarCache.has(String(id)) && !avatarsLoadingRef.current.has(id)
    );
    if (missing.length === 0) return;
    missing.forEach((id) => avatarsLoadingRef.current.add(id));
    try {
      const results = await invoke<ThumbnailData[]>("batched_get_avatar_headshots", {
        userIds: missing,
        size: "48x48",
      });
      for (const result of results || []) {
        if (result.imageUrl) friendAvatarCache.set(String(result.targetId), result.imageUrl);
      }
      setAvatarVersion((v) => v + 1);
    } catch {
      // Avatar é enfeite: falhar aqui não pode derrubar a lista de amigos.
    } finally {
      missing.forEach((id) => avatarsLoadingRef.current.delete(id));
    }
  }, []);

  const load = useCallback(async () => {
    const ids = userIdsRef.current;
    const key = userIdsKeyRef.current;
    if (ids.length === 0) {
      runRef.current = null;
      setGroups([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    setProgress({ done: 0, total: ids.length });
    // Se o backend ainda está percorrendo esta mesma seleção (a aba saiu e
    // voltou no meio), acompanha aquela rodada em vez de abrir outra.
    const run = startOrJoinRun(key, ids);
    runRef.current = run;
    setArrived(new Set(run.arrived));
    try {
      const list = await run.promise;
      if (runRef.current !== run) return;
      setGroups(list);
      void loadFriendAvatars(list);
    } catch (e) {
      if (runRef.current !== run) return;
      // Falha global (o comando inteiro caiu) — erro por conta vem em `row.error`.
      // O que estava na tela (do cache) fica: o erro aparece em cima dela.
      setError(String(e));
      setGroups((prev) => prev ?? []);
    } finally {
      if (runRef.current === run) {
        setLoading(false);
        setProgress(null);
      }
    }
  }, [loadFriendAvatars]);

  // Recarrega quando a seleção muda — partindo da última lista dela, se houver.
  useEffect(() => {
    setGroups(friendsCache.get(userIdsKey) ?? null);
    void load();
  }, [userIdsKey, load]);

  // Progresso do backend enquanto percorre as contas: a contagem e a entrada
  // da conta que acabou de voltar, que já vai para a tela. `listen` é
  // assíncrono: se o efeito já foi desmontado quando a promise resolver,
  // desinscreve na hora.
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void listen<FriendsOnlineProgress>("friends-online-progress", (event) => {
      const payload = event.payload;
      if (!payload) return;
      const run = runRef.current;
      if (!run) return;
      // Sem `requestId` é evento antigo do backend: vale só a contagem.
      if (payload.requestId != null && payload.requestId !== run.requestId) return;
      setProgress({ done: payload.done, total: payload.total });

      const entry = payload.entry;
      if (!entry || payload.requestId == null) return;
      run.arrived.add(entry.userId);
      setArrived(new Set(run.arrived));
      const key = userIdsKeyRef.current;
      const order = userIdsRef.current;
      setGroups((prev) => upsertAccountFriends(prev ?? [], entry, order));
      // O cache também recebe a conta: sair da aba no meio da rodada e voltar
      // mostra o que já tinha chegado.
      friendsCache.set(key, upsertAccountFriends(friendsCache.get(key) ?? [], entry, order));
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  /** Nome da CONTA do usuário (respeita o mascaramento). */
  function accountName(userId: number): string {
    const account = store.accounts.find((a) => a.UserID === userId);
    return accountLabel(account, store, userId);
  }

  async function handleJoinFriend(friend: OnlineFriend, target: FriendTarget) {
    if (!target.joinable || joining !== null) return;
    setJoining(friend.userId);
    try {
      // Um alvo só, resolvido uma vez: TODAS as contas selecionadas entram no
      // mesmo servidor pelo caminho normal de launch em lote.
      await launchAll(userIdsRef.current, target.placeId, target.jobId, onGoToConsole);
    } finally {
      setJoining(null);
    }
  }

  const rows = groups ?? [];
  const totalFriends = rows.reduce((sum, row) => sum + row.friends.length, 0);
  const hasAccountError = rows.some((row) => row.error);
  // Com contas na tela, cada uma já diz "No friends online" no próprio bloco:
  // o aviso grande do topo só repetia isso (pedido do dono, 08/10/2026). Ele
  // fica só para quando não sobra bloco nenhum para dizer.
  const showEmptyState =
    !loading && groups !== null && totalFriends === 0 && !hasAccountError && !error && rows.length === 0;

  /**
   * O que a tela desenha, conta por conta. Enquanto a rodada corre, a ordem é
   * a da seleção (a mesma que o backend devolve no fim) e cada conta é uma de
   * três: já voltou nesta rodada, ainda mostra a lista anterior (com um
   * indicador de atualização) ou ainda não tem nada (pendente). Fora da
   * rodada, é a lista do backend como veio.
   */
  type Slot =
    | { kind: "row"; row: AccountFriends; updating: boolean }
    | { kind: "pending"; userId: number };
  const slots: Slot[] = loading
    ? [...new Set(userIds.filter((id) => id > 0))].map((userId): Slot => {
        const row = rows.find((r) => r.userId === userId);
        return row
          ? { kind: "row", row, updating: !arrived.has(userId) }
          : { kind: "pending", userId };
      })
    : rows.map((row): Slot => ({ kind: "row", row, updating: false }));

  /** Foto da CONTA do usuário (respeita o mascaramento). */
  function accountAvatar(userId: number) {
    return !hideAccountAvatar(store) && store.avatarUrls.get(userId) ? (
      <img src={store.avatarUrls.get(userId)} alt="" className="w-5 h-5 rounded-full" />
    ) : (
      <div className="w-5 h-5 rounded-full bg-[var(--panel-muted)] flex items-center justify-center">
        <User size={10} strokeWidth={1.5} className="theme-muted" />
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-5" data-testid="friends-tab">
      {/* ── Cabeçalho ──────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between gap-3 mb-4">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-[var(--panel-fg)] flex items-center gap-1.5">
            <Users size={14} strokeWidth={1.5} />
            {t("Online friends")}
          </h3>
          <p className="text-[12px] theme-muted truncate">
            {loading
              ? progress
                ? t("Checking friends {{done}}/{{total}}...", {
                    done: progress.done,
                    total: progress.total,
                  })
                : t("Loading friends...")
              : t("{{count}} online", { count: totalFriends })}
          </p>
        </div>
        <button
          onClick={() => void load()}
          disabled={loading}
          style={{ width: "auto" }}
          className="sidebar-btn theme-btn shrink-0 flex items-center gap-1.5 px-3 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <RefreshCw size={13} strokeWidth={1.5} className={loading ? "animate-spin" : ""} />
          {t("Reload")}
        </button>
      </div>

      {error && (
        <div
          role="alert"
          className="flex items-start gap-2 mb-4 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-[12px] text-red-400"
        >
          <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
          <span className="break-words">{error}</span>
        </div>
      )}

      {showEmptyState && (
        <div
          data-testid="friends-empty"
          className="flex flex-col items-center justify-center gap-2 py-12 theme-muted"
        >
          <Gamepad2 size={22} strokeWidth={1.5} />
          <p className="text-[12px]">{t("No friends online right now")}</p>
        </div>
      )}

      {/* ── Um bloco por conta selecionada ─────────────────────────────── */}
      <div className="flex flex-col gap-3">
        {slots.map((slot) =>
          slot.kind === "pending" ? (
            // Conta que ainda não voltou: o cabeçalho já aparece, com um
            // indicador discreto no lugar da contagem.
            <section
              key={slot.userId}
              data-testid={`friends-pending-${slot.userId}`}
              aria-busy="true"
              className="rounded-lg border theme-border bg-[var(--panel-soft)]"
            >
              <header className="flex items-center gap-2 px-3 py-2">
                {accountAvatar(slot.userId)}
                <h4 className="text-[12px] font-semibold text-[var(--panel-fg)] truncate">
                  {accountName(slot.userId)}
                </h4>
                <span className="ml-auto shrink-0 flex items-center gap-1.5 text-[12px] theme-muted">
                  <Loader2 size={11} className="animate-spin" />
                  {t("Loading friends...")}
                </span>
              </header>
            </section>
          ) : (
          <section
            key={slot.row.userId}
            data-testid={`friends-group-${slot.row.userId}`}
            aria-busy={slot.updating || undefined}
            className="rounded-lg border theme-border bg-[var(--panel-soft)]"
          >
            <header className="flex items-center gap-2 px-3 py-2 border-b theme-border">
              {accountAvatar(slot.row.userId)}
              <h4 className="text-[12px] font-semibold text-[var(--panel-fg)] truncate">
                {accountName(slot.row.userId)}
              </h4>
              <span className="ml-auto shrink-0 flex items-center gap-1.5 text-[12px] theme-muted">
                {slot.updating && (
                  // A lista é a da última consulta; a nova desta conta ainda
                  // não voltou.
                  <Loader2
                    size={11}
                    className="animate-spin"
                    data-testid={`friends-updating-${slot.row.userId}`}
                    aria-label={t("Updating...")}
                  />
                )}
                {t("{{count}} online", { count: slot.row.friends.length })}
              </span>
            </header>

            {slot.row.error ? (
              <div className="flex items-start gap-2 px-3 py-2.5 text-[12px] text-red-400">
                <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
                <span className="break-words">{slot.row.error}</span>
              </div>
            ) : slot.row.friends.length === 0 ? (
              <p className="px-3 py-2.5 text-[12px] theme-muted">{t("No friends online")}</p>
            ) : (
              <ul className="py-1">
                {slot.row.friends.map((friend) => {
                  const target = friendTarget(friend, t);
                  const label = friend.displayName || friend.name;
                  const avatar = friendAvatarCache.get(String(friend.userId));
                  const busy = joining === friend.userId;

                  const inner = (
                    <>
                      {avatar ? (
                        <img src={avatar} alt="" className="w-7 h-7 rounded-full shrink-0" />
                      ) : (
                        <div className="w-7 h-7 rounded-full bg-[var(--panel-muted)] flex items-center justify-center shrink-0">
                          <User size={12} strokeWidth={1.5} className="theme-muted" />
                        </div>
                      )}
                      <span className="min-w-0 flex flex-col items-start">
                        <span className="text-[12px] text-[var(--panel-fg)] truncate max-w-[220px]">
                          {label}
                        </span>
                        <span className="text-[11px] theme-muted truncate max-w-[220px]">
                          {target.joinable ? friend.lastLocation || t("In Game") : target.reason}
                        </span>
                      </span>
                    </>
                  );

                  return (
                    <li
                      key={friend.userId}
                      data-testid={`friend-${slot.row.userId}-${friend.userId}`}
                      className="px-1.5"
                    >
                      {target.joinable ? (
                        <button
                          onClick={() => void handleJoinFriend(friend, target)}
                          disabled={joining !== null || userIds.length === 0}
                          className="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-md text-left hover:bg-[var(--accent-soft)] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
                          title={t("Send every selected account into {{name}}'s server", { name: label })}
                        >
                          {inner}
                          <span className="ml-auto shrink-0 text-[11px] text-[var(--accent-color)]">
                            {busy ? t("Joining...") : t("Join")}
                          </span>
                        </button>
                      ) : (
                        <div
                          className="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-md opacity-50 cursor-not-allowed"
                          title={target.reason}
                        >
                          {inner}
                          <span className="ml-auto shrink-0 text-[11px] theme-muted">{target.reason}</span>
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
          )
        )}
      </div>
    </div>
  );
}
