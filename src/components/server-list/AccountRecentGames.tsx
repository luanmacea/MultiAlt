import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, RefreshCw, User } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { hideAccountAvatar } from "../../utils/accountName";
import { browseServersIcon, favoriteIcon, joinGameIcon } from "./GamesTab";
import { GameCard } from "./GameCard";
import { CardRail } from "../ui/CardRail";
import type { GameEntry } from "./types";

/** Um jogo do "Continuar" de uma conta (`get_recent_games_for_accounts`). */
export interface AccountRecentGame {
  placeId: number;
  universeId: number;
  name: string;
  iconUrl: string | null;
}

export interface AccountRecentGamesEntry {
  userId: number;
  games: AccountRecentGame[];
  error: string | null;
}

/**
 * O resultado de cada conta fica na memória da sessão por um minuto: trocar de
 * aba e voltar não pede de novo ao Roblox (cada conta é uma chamada).
 */
const CACHE_MS = 60_000;
const cache = new Map<number, { at: number; entry: AccountRecentGamesEntry }>();

/**
 * Quantas contas são lidas ao mesmo tempo. Cada conta é uma chamada separada
 * (pedido do dono, 14/10/2026): a que volta já aparece, sem esperar o lote
 * inteiro — antes a tela ficava vazia até a última conta responder.
 */
export const RECENT_GAMES_CONCURRENCY = 3;

export function clearAccountRecentGamesCache() {
  cache.clear();
}

function fresh(userId: number): AccountRecentGamesEntry | null {
  const hit = cache.get(userId);
  return hit && Date.now() - hit.at < CACHE_MS ? hit.entry : null;
}

export interface AccountRecentGamesProps {
  userIds: number[];
  onSelectGame: (placeId: number, name?: string, iconUrl?: string | null) => void;
  onJoinGame: (placeId: number) => void;
  onBrowseServers?: (placeId: number) => void;
  onAddFavorite?: (game: GameEntry) => void;
}

/**
 * Os jogos recentes **de cada conta**, uma fileira por conta, como o
 * "Continue" do site do Roblox. Vêm do "Continuar" da página inicial: jogos em
 * que a conta entrou de qualquer lugar (app, site, celular), não só pelo
 * MultiAlt. A ordem das fileiras é a da seleção, não a de chegada.
 */
export function AccountRecentGames({
  userIds,
  onSelectGame,
  onJoinGame,
  onBrowseServers,
  onAddFavorite,
}: AccountRecentGamesProps) {
  const t = useTr();
  const store = useStore();
  const accountLabel = useAccountLabel();
  const key = userIds.join(",");
  const ids = [...new Set(userIds.filter((id) => id > 0))];
  const [entries, setEntries] = useState<Map<number, AccountRecentGamesEntry>>(() => {
    const map = new Map<number, AccountRecentGamesEntry>();
    for (const id of ids) {
      const hit = fresh(id);
      if (hit) map.set(id, hit);
    }
    return map;
  });
  /** Contas com leitura em andamento (a fileira mostra o indicador). */
  const [pending, setPending] = useState<Set<number>>(new Set());
  const [reload, setReload] = useState(0);
  const generation = useRef(0);
  /** Último "Refresh" já atendido: trocar de seleção depois dele usa o cache. */
  const servedReload = useRef(0);

  useEffect(() => {
    const mine = ++generation.current;
    const force = reload !== servedReload.current;
    servedReload.current = reload;
    const wanted = [...new Set(key.split(",").map(Number).filter((id) => id > 0))];
    const start = new Map<number, AccountRecentGamesEntry>();
    const queue: number[] = [];
    for (const id of wanted) {
      const hit = fresh(id);
      if (hit) start.set(id, hit);
      if (!hit || force) queue.push(id);
    }
    // Recarregar mantém o que está na tela enquanto a conta não volta.
    setEntries((prev) => {
      const next = new Map(start);
      for (const id of queue) {
        const old = prev.get(id);
        if (old && !next.has(id)) next.set(id, old);
      }
      return next;
    });
    setPending(new Set(queue));
    if (queue.length === 0) return;

    const settle = (userId: number, entry: AccountRecentGamesEntry) => {
      if (generation.current !== mine) return;
      setEntries((prev) => new Map(prev).set(userId, entry));
      setPending((prev) => {
        const next = new Set(prev);
        next.delete(userId);
        return next;
      });
    };
    const worker = async () => {
      while (queue.length > 0) {
        if (generation.current !== mine) return;
        const userId = queue.shift()!;
        try {
          const result = await invoke<AccountRecentGamesEntry[]>("get_recent_games_for_accounts", {
            userIds: [userId],
          });
          const entry = (Array.isArray(result) ? result : []).find((e) => e.userId === userId) ?? {
            userId,
            games: [],
            error: null,
          };
          cache.set(userId, { at: Date.now(), entry });
          settle(userId, entry);
        } catch (e) {
          settle(userId, { userId, games: [], error: String(e) });
        }
      }
    };
    // O número de leitores sai antes: cada um tira a sua conta da fila na hora.
    const workers = Math.min(RECENT_GAMES_CONCURRENCY, queue.length);
    for (let i = 0; i < workers; i++) void worker();
    return () => {
      // Seleção nova (ou aba fechada): as respostas desta rodada são ignoradas.
      generation.current++;
    };
  }, [key, reload]);

  if (ids.length === 0) return null;
  const loading = pending.size > 0;

  function accountAvatar(userId: number) {
    const url = store.avatarUrls?.get(userId);
    return !hideAccountAvatar(store) && url ? (
      <img src={url} alt="" className="w-6 h-6 rounded-full shrink-0" />
    ) : (
      <div className="w-6 h-6 rounded-full bg-[var(--panel-muted)] flex items-center justify-center shrink-0">
        <User size={12} strokeWidth={1.5} className="theme-muted" />
      </div>
    );
  }

  return (
    <section data-testid="account-recent-games" className="space-y-4">
      <div className="flex items-center justify-between gap-2 px-1">
        <div className="min-w-0">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
            {t("Recently played by each account")}
          </div>
          <div className="text-[11px] text-zinc-600">
            {t("From each account's Continue list on Roblox: games played from anywhere, not just MultiAlt.")}
          </div>
        </div>
        <button
          type="button"
          onClick={() => setReload((n) => n + 1)}
          disabled={loading}
          aria-label={t("Refresh")}
          className="shrink-0 p-1.5 rounded-md text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800/50 disabled:opacity-40"
        >
          <RefreshCw size={13} className={loading ? "animate-spin" : undefined} aria-hidden="true" />
        </button>
      </div>

      {ids.map((userId) => {
        const entry = entries.get(userId);
        const busy = pending.has(userId);
        const account = store.accounts.find((a) => a.UserID === userId);
        const name = accountLabel(account, userId);
        return (
          <CardRail
            key={userId}
            as="div"
            testId={entry ? `account-recent-${userId}` : `account-recent-pending-${userId}`}
            ariaBusy={busy}
            leading={accountAvatar(userId)}
            title={name}
            meta={
              <>
                {entry && !entry.error ? <span className="tabular-nums">{entry.games.length}</span> : null}
                {busy ? <Loader2 size={12} className="animate-spin" aria-label={t("Updating...")} /> : null}
              </>
            }
          >
            {!entry ? (
              // A conta ainda não voltou: lugares vazios do tamanho dos cards.
              Array.from({ length: 6 }, (_, i) => (
                <div key={i} className="w-[150px] shrink-0 p-1.5" aria-hidden="true">
                  <div className="aspect-square w-full rounded-lg bg-[var(--panel-soft)] animate-pulse" />
                  <div className="mt-2 h-3 w-3/4 rounded bg-[var(--panel-soft)] animate-pulse" />
                </div>
              ))
            ) : entry.error ? (
              <div className="py-2 text-[12px] text-red-400/90 break-words">{entry.error}</div>
            ) : entry.games.length === 0 ? (
              <div className="py-2 text-[12px] text-zinc-600">{t("No recent games on Roblox for this account.")}</div>
            ) : (
              entry.games.map((game) => (
                <GameCard
                  key={game.placeId}
                  testId={`account-recent-${userId}-${game.placeId}`}
                  placeId={game.placeId}
                  name={game.name}
                  iconUrl={game.iconUrl}
                  onOpen={() => onSelectGame(game.placeId, game.name, game.iconUrl)}
                  actions={[
                    ...(onBrowseServers
                      ? [
                          {
                            key: "servers",
                            label: t("Browse servers"),
                            icon: browseServersIcon,
                            onClick: () => onBrowseServers(game.placeId),
                          },
                        ]
                      : []),
                    ...(onAddFavorite
                      ? [
                          {
                            key: "favorite",
                            label: t("Favorite"),
                            icon: favoriteIcon,
                            onClick: () =>
                              onAddFavorite({
                                placeId: game.placeId,
                                name: game.name,
                                playerCount: 0,
                                likeRatio: null,
                                iconUrl: game.iconUrl,
                              }),
                          },
                        ]
                      : []),
                    {
                      key: "join",
                      label: t("Join Game"),
                      icon: joinGameIcon,
                      onClick: () => onJoinGame(game.placeId),
                    },
                  ]}
                />
              ))
            )}
          </CardRail>
        );
      })}
    </section>
  );
}
