import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AlertTriangle, Gamepad2, RefreshCw, User, Users } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
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

/** Mesmo mascaramento da lista de contas / Choose Game / Session Panel. */
function maskName(name: string, previewLetters: number): string {
  if (previewLetters > 0 && previewLetters < name.length) return name.slice(0, previewLetters) + "********";
  return "************";
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

  const [groups, setGroups] = useState<AccountFriends[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<FriendsOnlineProgress | null>(null);
  const [joining, setJoining] = useState<number | null>(null);
  const [friendAvatars, setFriendAvatars] = useState<Map<number, string>>(new Map());

  // A seleção muda de identidade a cada render da Choose Game; a chave é o que
  // realmente define "outra seleção".
  const userIdsKey = userIds.join(",");
  const userIdsRef = useRef(userIds);
  userIdsRef.current = userIds;
  const avatarsLoadingRef = useRef<Set<number>>(new Set());
  const avatarsCacheRef = useRef<Map<number, string>>(new Map());

  /**
   * Busca as miniaturas dos amigos ainda sem avatar, **em lote** (mesmo comando
   * usado pela lista de contas). O cache vive num ref para sobreviver a
   * recarregamentos sem refazer a chamada.
   */
  const loadFriendAvatars = useCallback(async (rows: AccountFriends[]) => {
    const wanted = new Set<number>();
    for (const row of rows) for (const friend of row.friends) wanted.add(friend.userId);
    const missing = [...wanted].filter(
      (id) => !avatarsCacheRef.current.has(id) && !avatarsLoadingRef.current.has(id)
    );
    if (missing.length === 0) return;
    missing.forEach((id) => avatarsLoadingRef.current.add(id));
    try {
      const results = await invoke<ThumbnailData[]>("batched_get_avatar_headshots", {
        userIds: missing,
        size: "48x48",
      });
      for (const result of results || []) {
        if (result.imageUrl) avatarsCacheRef.current.set(result.targetId, result.imageUrl);
      }
      setFriendAvatars(new Map(avatarsCacheRef.current));
    } catch {
      // Avatar é enfeite: falhar aqui não pode derrubar a lista de amigos.
    } finally {
      missing.forEach((id) => avatarsLoadingRef.current.delete(id));
    }
  }, []);

  const load = useCallback(async () => {
    const ids = userIdsRef.current;
    if (ids.length === 0) {
      setGroups([]);
      return;
    }
    setLoading(true);
    setError(null);
    setProgress({ done: 0, total: ids.length });
    try {
      const rows = await invoke<AccountFriends[]>("get_online_friends_for_accounts", {
        userIds: ids,
        delayMs: FRIENDS_REQUEST_DELAY_MS,
      });
      const list = rows || [];
      setGroups(list);
      void loadFriendAvatars(list);
    } catch (e) {
      // Falha global (o comando inteiro caiu) — erro por conta vem em `row.error`.
      setError(String(e));
      setGroups([]);
    } finally {
      setLoading(false);
      setProgress(null);
    }
  }, [loadFriendAvatars]);

  // Recarrega quando a seleção muda.
  useEffect(() => {
    void load();
  }, [userIdsKey, load]);

  // Progresso do backend enquanto percorre as contas. `listen` é assíncrono:
  // se o efeito já foi desmontado quando a promise resolver, desinscreve na hora.
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void listen<FriendsOnlineProgress>("friends-online-progress", (event) => {
      const payload = event.payload;
      if (!payload) return;
      setProgress({ done: payload.done, total: payload.total });
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
    const raw = account ? account.Alias || account.Username : String(userId);
    return store.hideUsernames ? maskName(raw, store.hiddenNameLetters) : raw;
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
  const showEmptyState = !loading && groups !== null && totalFriends === 0 && !hasAccountError && !error;

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
        {rows.map((row) => (
          <section
            key={row.userId}
            data-testid={`friends-group-${row.userId}`}
            className="rounded-lg border theme-border bg-[var(--panel-soft)]"
          >
            <header className="flex items-center gap-2 px-3 py-2 border-b theme-border">
              {store.avatarUrls.get(row.userId) ? (
                <img src={store.avatarUrls.get(row.userId)} alt="" className="w-5 h-5 rounded-full" />
              ) : (
                <div className="w-5 h-5 rounded-full bg-[var(--panel-muted)] flex items-center justify-center">
                  <User size={10} strokeWidth={1.5} className="theme-muted" />
                </div>
              )}
              <h4 className="text-[12px] font-semibold text-[var(--panel-fg)] truncate">
                {accountName(row.userId)}
              </h4>
              <span className="ml-auto shrink-0 text-[12px] theme-muted">
                {t("{{count}} online", { count: row.friends.length })}
              </span>
            </header>

            {row.error ? (
              <div className="flex items-start gap-2 px-3 py-2.5 text-[12px] text-red-400">
                <AlertTriangle size={13} strokeWidth={1.5} className="shrink-0 mt-[2px]" />
                <span className="break-words">{row.error}</span>
              </div>
            ) : row.friends.length === 0 ? (
              <p className="px-3 py-2.5 text-[12px] theme-muted">{t("No friends online")}</p>
            ) : (
              <ul className="py-1">
                {row.friends.map((friend) => {
                  const target = friendTarget(friend, t);
                  const label = friend.displayName || friend.name;
                  const avatar = friendAvatars.get(friend.userId);
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
                      data-testid={`friend-${row.userId}-${friend.userId}`}
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
        ))}
      </div>
    </div>
  );
}
