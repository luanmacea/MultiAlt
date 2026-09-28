import { invoke } from "@tauri-apps/api/core";

export type TabId = "servers" | "games" | "favorites" | "recent";

export interface ServerData {
  id: string;
  maxPlayers: number;
  playing: number;
  playerTokens: string[];
  fps: number;
  ping: number | null;
  name: string | null;
  vipServerId: number | null;
  accessCode: string | null;
}

export interface ServersResponse {
  data: ServerData[];
  nextPageCursor: string | null;
}

export interface PlaceDetails {
  placeId: number;
  universeId: number;
  name: string;
  description: string;
  sourceName: string;
  sourceDescription: string;
  url: string;
}

export interface GameEntry {
  placeId: number;
  name: string;
  playerCount: number;
  likeRatio: number | null;
  iconUrl: string | null;
  universeId?: number;
}

export interface VipServer {
  id: string;
  name: string;
  link: string;
}

export interface FavoriteGame {
  placeId: number;
  name: string;
  iconUrl: string | null;
  addedAt: number;
  privateServer?: string; // legacy single VIP, migrated into vipServers on load
  vipServers?: VipServer[];
}

export interface RecentGame {
  placeId: number;
  name: string;
  iconUrl: string | null;
  lastPlayed: number;
}

export interface ServerRegion {
  region: string;
  loading: boolean;
}

/**
 * O que a entrada de "servidor recente" guarda: um Job ID público, um código
 * VIP (`vip:<código>`) ou um link privado a decodificar. A distinção não é
 * enfeite — ver `visibleRecentJobs`.
 *
 * Na prática o `link` quase não é alcançado pelo caminho da store: ela
 * normaliza o que veio do launch para `vip:<código>` (ver `raw` abaixo), e
 * `vip:` classifica como `vip`. Ele cobre o que já está gravado e qualquer
 * chamador que grave o link cru — e é por isso que `classifyJobInput` continua
 * tendo que reconhecer link, inclusive o duplo-codificado.
 */
export type RecentJobKind = "job" | "vip" | "link";

export interface RecentJobEntry {
  kind: RecentJobKind;
  /**
   * O alvo **como o launch o usou**, no vocabulário que o campo de Job ID
   * aceita de volta: Job ID público cru, ou `vip:<código>`.
   *
   * Não é o texto que o usuário colou: a store **normaliza** link privado para
   * `vip:<código>` antes de gravar (`store.tsx`, `joinServer`/`launchMultiple`),
   * porque num alvo VIP o Job ID vai vazio e o código viaja em `linkCode` —
   * guardar o Job ID cru perderia o servidor. O `resolve_launch_job`
   * (`docs/features/join-links.md`) chega ao mesmo `link_code` pelas duas
   * formas, e a normalizada ainda classifica como privada, que é o lado seguro
   * do erro. O que **não** se faz é reescrever o valor dentro deste módulo.
   */
  raw: string;
  placeId: number | null;
  lastUsed: number;
  /** Contas que já entraram por este alvo — o "dono" de um alvo privado. */
  userIds: number[];
}

const STORAGE_KEY_FAVORITES = "ram_favorite_games";
const STORAGE_KEY_RECENT = "ram_recent_games";
const STORAGE_KEY_RECENT_JOBS = "ram_recent_jobs";

/** Vira número só se for um place plausível. */
function toPlaceId(digits: string): number | null {
  const id = Number(digits);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

/**
 * Lê o Place ID de um texto digitado ou colado no campo "Place ID".
 *
 * O campo fazia `replace(/[^0-9]/g, "")`, então colar a URL do jogo **juntava
 * todos os dígitos dela**: `.../games/606849621/Jailbreak?privateServerLinkCode=98765`
 * virava o place inexistente `60684962198765`, e a busca falhava sem explicar
 * nada. Aqui só valem as formas que de fato carregam o place — o resto devolve
 * `null` para a tela reclamar em vez de inventar número.
 *
 * Link de convite e `share?code=` **não** carregam place: quem resolve esses é
 * o `resolve_join_link` do backend, pela aba Follow.
 */
export function parsePlaceIdInput(text: string): number | null {
  const value = text.trim();
  if (!value) return null;

  if (/^\d+$/.test(value)) return toPlaceId(value);

  // roblox.com/games/<id>/<slug> — com ou sem protocolo, www, query ou hash.
  const path = value.match(/\/games\/(\d+)/i);
  if (path) return toPlaceId(path[1]);

  // games/start?placeId=<id> e deep links `roblox://placeId=<id>`.
  const query = value.match(/[?&#/]placeid=(\d+)/i);
  if (query) return toPlaceId(query[1]);

  return null;
}

/**
 * O texto parece um link do Roblox? Só serve para escolher a mensagem de erro:
 * link sem place vai para a aba Follow, texto qualquer vira "não achei place".
 */
export function looksLikeJoinLink(text: string): boolean {
  const value = text.trim();
  return /^[a-z]+:\/\//i.test(value) || /roblox\.com|ro\.blox\.com|^www\./i.test(value);
}

export function makeVipId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  }
}

export function loadFavorites(): FavoriteGame[] {
  try {
    const list: FavoriteGame[] = JSON.parse(localStorage.getItem(STORAGE_KEY_FAVORITES) || "[]");
    return list.map((f) => {
      if (!f.vipServers) {
        f.vipServers = f.privateServer
          ? [{ id: makeVipId(), name: "VIP", link: f.privateServer }]
          : [];
      }
      return f;
    });
  } catch {
    return [];
  }
}

export function saveFavorites(favorites: FavoriteGame[]) {
  localStorage.setItem(STORAGE_KEY_FAVORITES, JSON.stringify(favorites));
}

export function loadRecentGames(): RecentGame[] {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY_RECENT) || "[]");
  } catch {
    return [];
  }
}

export function saveRecentGames(games: RecentGame[]) {
  localStorage.setItem(STORAGE_KEY_RECENT, JSON.stringify(games));
}

export function addRecentGame(game: RecentGame, maxCount: number) {
  const existing = loadRecentGames().filter((g) => g.placeId !== game.placeId);
  existing.unshift({ ...game, lastPlayed: Date.now() });
  saveRecentGames(existing.slice(0, maxCount));
}

/**
 * Desfaz as camadas de `%XX` de um link.
 *
 * Link curto do AppsFlyer carrega a query de verdade **codificada** dentro do
 * `af_dp` (`…?af_dp=roblox%3A%2F%2F…%3Fcode%3DDEADBEEF`), então `code=` não
 * aparece no texto cru. É o mesmo motivo do
 * `extract_query_param_value_recursive` no backend
 * (`commands/launch_shared.rs`). Escape quebrado faz `decodeURIComponent`
 * lançar: aí vale o que já se conseguiu decodificar.
 */
function decodeLayers(raw: string): string {
  let value = raw;
  for (let i = 0; i < 3; i++) {
    let next: string;
    try {
      next = decodeURIComponent(value);
    } catch {
      return value;
    }
    if (next === value) return value;
    value = next;
  }
  return value;
}

/**
 * Que tipo de alvo é este texto do campo "Job ID".
 *
 * Os mesmos formatos que o `resolve_launch_job` aceita: `vip:<código>` declara
 * a intenção, e um link traz o código em `privateServerLinkCode=`, `linkCode=`
 * ou `code=`. O resto é Job ID público.
 *
 * Errar para o lado de "público" é o erro caro: um alvo privado classificado
 * como `job` fica visível para **todas** as contas (`visibleRecentJobs`), com o
 * código do dono à mostra. Por isso o link é procurado também no texto
 * decodificado.
 */
export function classifyJobInput(raw: string): RecentJobKind {
  if (/^vip:\s*\S/i.test(raw.trim())) return "vip";
  const linkCode = /(?:privateServerLinkCode|linkCode|code)=[^&\s]+/i;
  if (linkCode.test(raw) || linkCode.test(decodeLayers(raw))) return "link";
  return "job";
}

export function loadRecentJobs(): RecentJobEntry[] {
  try {
    const list = JSON.parse(localStorage.getItem(STORAGE_KEY_RECENT_JOBS) || "[]");
    return Array.isArray(list) ? (list as RecentJobEntry[]) : [];
  } catch {
    return [];
  }
}

export function saveRecentJobs(entries: RecentJobEntry[]) {
  localStorage.setItem(STORAGE_KEY_RECENT_JOBS, JSON.stringify(entries));
}

/**
 * Guarda o servidor em que as contas acabaram de entrar.
 *
 * Mesmo desenho da lista de jogos recentes: sem duplicata, mais recente no
 * topo, cortada no limite. A diferença é o `userIds`, que soma as contas — um
 * link privado usado por duas contas pertence às duas.
 */
export function addRecentJob(
  raw: string,
  placeId: number | null,
  maxCount: number,
  userIds: number[]
) {
  const trimmed = raw.trim();
  if (!trimmed) return;

  const entries = loadRecentJobs();
  const existing = entries.find((e) => e.raw === trimmed);
  const owners = new Set<number>(existing?.userIds ?? []);
  for (const id of userIds) if (Number.isFinite(id)) owners.add(id);

  const entry: RecentJobEntry = {
    kind: classifyJobInput(trimmed),
    raw: trimmed,
    // Um launch pelo campo de Job ID pode não saber o place; o que já se sabia
    // sobre esta entrada não se perde por causa disso.
    placeId: placeId ?? existing?.placeId ?? null,
    lastUsed: Date.now(),
    userIds: [...owners].sort((a, b) => a - b),
  };

  const rest = entries.filter((e) => e.raw !== trimmed);
  rest.unshift(entry);
  // Limite estragado (0, NaN) não pode zerar a lista logo depois de gravar.
  const limit = Number.isFinite(maxCount) ? Math.max(1, Math.trunc(maxCount)) : 1;
  saveRecentJobs(rest.slice(0, limit));
}

/**
 * Filtra a lista para uma conta.
 *
 * Job ID público é o mesmo servidor que a aba Servers lista para qualquer
 * conta, então aparece para todas. Alvo **privado** (VIP ou link) só aparece
 * para as contas que já entraram por ele: mostrá-lo para outra conta entregaria
 * o servidor privado de uma conta a outra sem o dono pedir.
 */
export function visibleRecentJobs(
  entries: RecentJobEntry[],
  userId: number | null
): RecentJobEntry[] {
  return entries.filter((e) => {
    if (e.kind === "job") return true;
    return userId !== null && (e.userIds ?? []).includes(userId);
  });
}

export function removeRecentJob(raw: string) {
  saveRecentJobs(loadRecentJobs().filter((e) => e.raw !== raw));
}

/**
 * Nome e ícone de um place, para completar uma entrada de "recentes".
 *
 * Uma chamada só (`batched_get_game_info`): eram duas, e a do nome
 * (`get_place_details`) não tinha cache nenhum no backend. Não usa o
 * `useGameIdentity` de propósito — isso criaria um ciclo de import entre este
 * módulo e o hook, que lê `parsePlaceIdInput` daqui. O cache que importa é o
 * do backend, e ele é o mesmo para os dois caminhos.
 */
async function resolveNameAndIcon(
  placeId: number,
  userId: number | null,
  fallbackName: string,
  fallbackIcon: string | null
): Promise<{ name: string; iconUrl: string | null }> {
  let info: { name?: string | null; iconUrl?: string | null } | null = null;
  try {
    info = await invoke<{ name: string | null; iconUrl: string | null }>(
      "batched_get_game_info",
      { placeId, userId }
    );
  } catch {
    info = null;
  }

  const foundName = typeof info?.name === "string" ? info.name.trim() : "";
  const foundIcon = typeof info?.iconUrl === "string" ? info.iconUrl : "";
  return {
    name: foundName || fallbackName,
    iconUrl: foundIcon || fallbackIcon,
  };
}

export async function recordRecentGame(
  placeId: number,
  userId: number | null,
  maxCount: number,
  opts?: { name?: string; iconUrl?: string | null }
) {
  if (!placeId || placeId <= 0) return;

  const existing = loadRecentGames().find((g) => g.placeId === placeId);
  const existingHasName = !!existing && !!existing.name && existing.name !== String(placeId);
  const hasName = !!opts?.name && opts.name.trim().length > 0;
  const optimisticName = hasName
    ? (opts!.name as string)
    : existingHasName
      ? (existing!.name as string)
      : String(placeId);
  const optimisticIcon = opts?.iconUrl ?? existing?.iconUrl ?? null;
  addRecentGame(
    { placeId, name: optimisticName, iconUrl: optimisticIcon, lastPlayed: Date.now() },
    maxCount
  );

  const needName = optimisticName === String(placeId);
  const needIcon = !optimisticIcon;
  if (!needName && !needIcon) return;

  const { name, iconUrl } = await resolveNameAndIcon(
    placeId,
    userId,
    optimisticName,
    optimisticIcon
  );
  if (name === optimisticName && iconUrl === optimisticIcon) return;

  const games = loadRecentGames();
  const idx = games.findIndex((g) => g.placeId === placeId);
  if (idx >= 0) {
    games[idx] = { ...games[idx], name, iconUrl };
    saveRecentGames(games);
  }
}

export async function resolveRecentGame(
  game: RecentGame,
  userId: number | null
): Promise<RecentGame | null> {
  const needName = !game.name || game.name === String(game.placeId);
  const needIcon = !game.iconUrl;
  if (!needName && !needIcon) return null;

  const { name, iconUrl } = await resolveNameAndIcon(
    game.placeId,
    userId,
    game.name,
    game.iconUrl
  );
  if (name === game.name && iconUrl === game.iconUrl) return null;
  return { ...game, name, iconUrl };
}
