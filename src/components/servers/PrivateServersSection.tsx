import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight, Loader2, Lock, RefreshCw } from "lucide-react";
import { useTr } from "../../i18n/text";
import type { PrivateServer, PrivateServersPage } from "../../types";

/**
 * Seção "Private servers" da aba Servers: os servidores privados deste place
 * que **cada conta selecionada** pode acessar, lidos com o cookie dela
 * (`get_private_servers`, só leitura, sem refresh de sessão).
 *
 * - Fechada por padrão: cada conta custa uma chamada à API, e a maioria das
 *   aberturas da aba é para os públicos. Aberta, fica aberta (preferência local).
 * - Uma conta por vez, na ordem da seleção; o erro de uma não esconde as outras.
 * - Entrar passa pelo caminho VIP de sempre (`vip:<accessCode>`), e vão só as
 *   contas selecionadas que **veem** aquele servidor — mandar quem não tem
 *   acesso seria uma recusa do Roblox por conta.
 */

const OPEN_KEY = "ram_private_servers_open";
/** Espera a digitação do Place ID parar antes de perguntar (cada conta é uma chamada). */
const PLACE_DEBOUNCE_MS = 400;

type AccountEntry = {
  status: "loading" | "done" | "error";
  servers: PrivateServer[];
  cursor: string | null;
  error: string | null;
  loadingMore: boolean;
};

/** A identidade de um servidor privado entre contas (o mesmo VIP visto por duas). */
export function privateServerKey(server: PrivateServer): string {
  if (server.vipServerId != null) return `vip:${server.vipServerId}`;
  if (server.accessCode) return `code:${server.accessCode}`;
  return `job:${server.id ?? ""}:${server.name ?? ""}`;
}

/** As contas selecionadas (na ordem da seleção) cuja lista carregada tem este servidor. */
export function accountsWithAccess(
  key: string,
  entries: Map<number, { servers: PrivateServer[] }>,
  userIds: number[]
): number[] {
  return userIds.filter((id) => entries.get(id)?.servers.some((server) => privateServerKey(server) === key));
}

function readOpen(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === "1";
  } catch {
    return false;
  }
}

function writeOpen(open: boolean) {
  try {
    localStorage.setItem(OPEN_KEY, open ? "1" : "0");
  } catch {
    // Preferência de conveniência: sem storage, a seção só não lembra.
  }
}

export interface PrivateServersSectionProps {
  /** Contas selecionadas — cada uma tem a sua lista. */
  userIds: number[];
  /** Place digitado (0 = nenhum válido). */
  placeId: number;
  /** Entra no servidor com as contas que o veem. */
  onJoin: (server: PrivateServer, userIds: number[]) => Promise<unknown> | void;
  /**
   * Nome de uma conta **do usuário** como a tela mostra (com a máscara de
   * nomes), ou `null` para quem não é conta dele. Sem isto, mostra o id.
   */
  accountName?: (userId: number) => string | null;
  className?: string;
}

export function PrivateServersSection({
  userIds,
  placeId,
  onJoin,
  accountName,
  className = "",
}: PrivateServersSectionProps) {
  const t = useTr();
  const [open, setOpen] = useState(readOpen);
  const [entries, setEntries] = useState<Map<number, AccountEntry>>(new Map());
  const [joining, setJoining] = useState<string | null>(null);
  const [reloadNonce, setReloadNonce] = useState(0);
  const generation = useRef(0);
  const userKey = userIds.join(",");

  useEffect(() => {
    if (!open || !(placeId > 0) || userIds.length === 0) {
      generation.current++;
      setEntries(new Map());
      return;
    }
    const gen = ++generation.current;
    const ids = [...userIds];
    setEntries(
      new Map(ids.map((id) => [id, { status: "loading", servers: [], cursor: null, error: null, loadingMore: false }]))
    );
    const timer = setTimeout(async () => {
      for (const userId of ids) {
        if (generation.current !== gen) return;
        let next: AccountEntry;
        try {
          const page = await invoke<PrivateServersPage>("get_private_servers", { userId, placeId, cursor: null });
          next = {
            status: "done",
            servers: page?.data ?? [],
            cursor: page?.nextPageCursor || null,
            error: null,
            loadingMore: false,
          };
        } catch (e) {
          next = { status: "error", servers: [], cursor: null, error: String(e), loadingMore: false };
        }
        if (generation.current !== gen) return;
        setEntries((prev) => new Map(prev).set(userId, next));
      }
    }, PLACE_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
      // Fechar a seção / trocar de aba tem que parar o laço por conta já em
      // andamento: só limpar o timer deixava o `for` seguir chamando
      // `get_private_servers` conta após conta. Avançar a geração faz a
      // próxima volta do laço (e um `loadMore` em voo) desistir.
      generation.current++;
    };
    // `userKey` resume a seleção; a lista em si muda de identidade a cada render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, placeId, userKey, reloadNonce]);

  async function loadMore(userId: number) {
    const entry = entries.get(userId);
    if (!entry?.cursor || entry.loadingMore) return;
    const gen = generation.current;
    setEntries((prev) => new Map(prev).set(userId, { ...entry, loadingMore: true }));
    try {
      const page = await invoke<PrivateServersPage>("get_private_servers", { userId, placeId, cursor: entry.cursor });
      if (generation.current !== gen) return;
      setEntries((prev) => {
        const current = prev.get(userId) ?? entry;
        return new Map(prev).set(userId, {
          ...current,
          servers: [...current.servers, ...(page?.data ?? [])],
          cursor: page?.nextPageCursor || null,
          loadingMore: false,
        });
      });
    } catch (e) {
      if (generation.current !== gen) return;
      setEntries((prev) => {
        const current = prev.get(userId) ?? entry;
        return new Map(prev).set(userId, { ...current, loadingMore: false, error: String(e) });
      });
    }
  }

  function toggle() {
    setOpen((was) => {
      writeOpen(!was);
      return !was;
    });
  }

  async function handleJoin(server: PrivateServer, holders: number[]) {
    const key = privateServerKey(server);
    setJoining(key);
    try {
      await onJoin(server, holders);
    } finally {
      setJoining(null);
    }
  }

  /** O dono: se for uma das contas do usuário, com a máscara de nomes; senão o nome dele. */
  function ownerName(owner: PrivateServer["owner"]): string | null {
    if (!owner) return null;
    return accountName?.(owner.id) ?? (owner.displayName || owner.name || String(owner.id));
  }

  const totalLoaded = [...entries.values()].reduce((sum, entry) => sum + entry.servers.length, 0);

  return (
    <section
      data-testid="private-servers"
      className={`rounded-xl border theme-border theme-surface ${className}`}
    >
      <div className="flex items-center gap-2 px-3 py-2">
        <button
          onClick={toggle}
          aria-expanded={open}
          className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--panel-fg)]"
        >
          {open ? <ChevronDown size={13} strokeWidth={1.5} /> : <ChevronRight size={13} strokeWidth={1.5} />}
          <Lock size={12} strokeWidth={1.5} className="theme-muted" />
          {t("Private servers")}
          {open && totalLoaded > 0 && <span className="theme-muted font-normal tabular-nums">({totalLoaded})</span>}
        </button>
        {!open && (
          <span className="text-[11px] theme-muted truncate">
            {t("The private servers each selected account can access in this game.")}
          </span>
        )}
        {open && placeId > 0 && (
          <button
            onClick={() => setReloadNonce((n) => n + 1)}
            aria-label={t("Reload private servers")}
            title={t("Reload private servers")}
            className="ml-auto p-1 rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors"
          >
            <RefreshCw size={12} strokeWidth={1.5} />
          </button>
        )}
      </div>

      {open && (
        <div className="max-h-[220px] overflow-y-auto border-t theme-border px-3 py-2 space-y-2">
          {!(placeId > 0) && <p className="text-[12px] theme-muted">{t("Enter a Place ID to see private servers.")}</p>}
          {placeId > 0 &&
            userIds.map((userId) => {
              const entry = entries.get(userId);
              const name = accountName?.(userId) ?? String(userId);
              return (
                <div key={userId} data-testid={`private-servers-${userId}`}>
                  <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider theme-muted pb-1">
                    {(!entry || entry.status === "loading") && <Loader2 size={11} className="animate-spin" />}
                    <span className="truncate">{name}</span>
                  </div>
                  {entry?.status === "error" && (
                    <p className="text-[12px] text-red-400 break-words">{entry.error}</p>
                  )}
                  {entry?.status === "done" && entry.servers.length === 0 && (
                    <p className="text-[12px] theme-muted">{t("No private servers for this account in this game")}</p>
                  )}
                  {entry && entry.servers.length > 0 && (
                    <ul className="divide-y divide-[var(--panel-border)]">
                      {entry.servers.map((server) => {
                        const key = privateServerKey(server);
                        const holders = accountsWithAccess(key, entries, userIds);
                        const owner = ownerName(server.owner);
                        const partial = holders.length < userIds.length;
                        return (
                          <li key={key} className="flex items-center gap-3 py-1.5">
                            <div className="min-w-0 flex-1">
                              <div className="text-[12px] text-[var(--panel-fg)] truncate">
                                {server.name || t("Private server")}
                              </div>
                              {owner && (
                                <div className="text-[11px] theme-muted truncate">{t("by {{owner}}", { owner })}</div>
                              )}
                            </div>
                            <span className="text-[12px] tabular-nums theme-muted shrink-0">
                              {server.playing}/{server.maxPlayers}
                            </span>
                            <button
                              onClick={() => void handleJoin(server, holders)}
                              disabled={!server.accessCode || joining !== null || holders.length === 0}
                              title={
                                !server.accessCode
                                  ? t("Roblox did not send a join code for this server")
                                  : t("Joins with the {{count}} selected account(s) that can access this server", {
                                      count: holders.length,
                                    })
                              }
                              className="shrink-0 px-3 py-1 rounded-lg text-[12px] theme-btn-ghost border theme-border text-[var(--panel-fg)] disabled:opacity-30 transition-colors"
                            >
                              {joining === key
                                ? t("Joining...")
                                : partial
                                  ? t("Join with {{count}}", { count: holders.length })
                                  : t("Join")}
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                  {entry?.status === "done" && entry.cursor && (
                    <button
                      onClick={() => void loadMore(userId)}
                      disabled={entry.loadingMore}
                      className="mt-1 text-[12px] theme-muted hover:text-[var(--panel-fg)] underline underline-offset-2 disabled:opacity-50"
                    >
                      {entry.loadingMore ? t("Loading...") : t("Load more")}
                    </button>
                  )}
                  {entry?.status === "done" && entry.error && (
                    <p className="text-[12px] text-red-400 break-words">{entry.error}</p>
                  )}
                </div>
              );
            })}
        </div>
      )}
    </section>
  );
}
