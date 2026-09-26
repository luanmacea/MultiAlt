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

const STORAGE_KEY_FAVORITES = "ram_favorite_games";
const STORAGE_KEY_RECENT = "ram_recent_games";

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
