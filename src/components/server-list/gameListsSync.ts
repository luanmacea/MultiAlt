/**
 * Favoritos (com os servidores VIP), jogos recentes e servidores recentes:
 * onde moram e como chegam ao disco.
 *
 * Até a v0.1.x essas listas existiam **só** no `localStorage` do WebView —
 * fora do backup, da migração de pasta de dados, e perdidas num reset do
 * WebView ou na troca de PC. A cópia durável agora é o `RAMGameLists.json` do
 * backend (`src-tauri/src/data/game_lists.rs`, comandos `get_game_lists` /
 * `save_game_lists`). O `localStorage` continua como **cache síncrono**: as
 * telas leem e gravam nele sem `await`, e cada gravação é espelhada no backend
 * em segundo plano.
 *
 * Regras de segurança (o dono já perdeu VIPs salvos por um bug de tela):
 * - nada é espelhado antes da hidratação: até lá o backend pode ter dado que o
 *   `localStorage` não tem, e mandar a cópia local seria apagá-lo;
 * - hidratar **mescla**, nunca substitui: a união dos dois lados (favorito por
 *   `placeId`, VIP pelo link), então nem o backend vazio apaga o local nem o
 *   local vazio apaga o backend;
 * - lista ilegível nunca vira "lista vazia" para gravar: o espelho não manda
 *   nada, e a hidratação guarda o texto cru em `<chave>.corrupt` antes de
 *   repor a lista;
 * - `userDelete` marca a gravação que veio de uma exclusão pedida pelo usuário:
 *   só ela pode zerar favoritos ou VIPs no backend (ver `save_game_lists`).
 */
import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const STORAGE_KEY_FAVORITES = "ram_favorite_games";
export const STORAGE_KEY_RECENT = "ram_recent_games";
export const STORAGE_KEY_RECENT_JOBS = "ram_recent_jobs";

export interface GameListsSnapshot {
  favorites: unknown[];
  recentGames: unknown[];
  recentJobs: unknown[];
}

export type GameListName = keyof GameListsSnapshot;

const STORAGE_KEYS: Record<GameListName, string> = {
  favorites: STORAGE_KEY_FAVORITES,
  recentGames: STORAGE_KEY_RECENT,
  recentJobs: STORAGE_KEY_RECENT_JOBS,
};

const LIST_NAMES: GameListName[] = ["favorites", "recentGames", "recentJobs"];

/** Opções de uma gravação: `userDelete` só em exclusão pedida pelo usuário. */
export interface GameListWriteOptions {
  userDelete?: boolean;
}

export type StoredListRead =
  | { ok: true; list: unknown[] }
  | { ok: false; raw: string };

/**
 * Lê uma lista do `localStorage` distinguindo "não existe" (lista vazia) de
 * "existe e está estragada". Quem vai **gravar** precisa dessa diferença.
 */
export function readStoredList(name: GameListName): StoredListRead {
  let raw: string | null;
  try {
    raw = localStorage.getItem(STORAGE_KEYS[name]);
  } catch {
    return { ok: false, raw: "" };
  }
  if (raw === null || raw === "") return { ok: true, list: [] };
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? { ok: true, list: parsed } : { ok: false, raw };
  } catch {
    return { ok: false, raw };
  }
}

// ── avisos de mudança ────────────────────────────────────────────────────────

const CHANGE_EVENT = "ram:game-lists-changed";

/**
 * Avisa as telas montadas que uma lista mudou. Sem isto cada tela segurava a
 * cópia lida na montagem — e foi regravando essa cópia que VIPs sumiram.
 */
export function notifyGameListsChanged(): void {
  try {
    window.dispatchEvent(new Event(CHANGE_EVENT));
  } catch {
    // Fora do navegador (nunca no app): nada a avisar.
  }
}

export function subscribeGameLists(callback: () => void): () => void {
  window.addEventListener(CHANGE_EVENT, callback);
  return () => window.removeEventListener(CHANGE_EVENT, callback);
}

/** Hook: chama `callback` sempre que alguma das listas mudar. */
export function useGameListsChanged(callback: () => void): void {
  const ref = useRef(callback);
  ref.current = callback;
  useEffect(() => subscribeGameLists(() => ref.current()), []);
}

/** Grava uma lista no cache local, avisa as telas e espelha no backend. */
export function writeStoredList(
  name: GameListName,
  list: unknown[],
  opts: GameListWriteOptions = {}
): void {
  localStorage.setItem(STORAGE_KEYS[name], JSON.stringify(list));
  notifyGameListsChanged();
  persistGameLists(opts);
}

// ── espelho no backend ───────────────────────────────────────────────────────

let hydrated = false;
let inFlight: Promise<void> | null = null;
let pending = false;
let pendingDestructive = false;

/** As três listas do cache local, ou `null` se alguma estiver ilegível. */
function readLocalSnapshot(): GameListsSnapshot | null {
  const snapshot: Partial<GameListsSnapshot> = {};
  for (const name of LIST_NAMES) {
    const read = readStoredList(name);
    if (!read.ok) return null;
    snapshot[name] = read.list;
  }
  return snapshot as GameListsSnapshot;
}

async function flush(): Promise<void> {
  while (pending) {
    pending = false;
    const allowDestructive = pendingDestructive;
    pendingDestructive = false;
    const lists = readLocalSnapshot();
    if (!lists) {
      console.error("[gameLists] a local list is unreadable; not mirroring it to the backend");
      continue;
    }
    try {
      await invoke("save_game_lists", { lists, allowDestructive });
    } catch (error) {
      console.error("[gameLists] failed to save the game lists:", error);
    }
  }
  inFlight = null;
}

/**
 * Agenda o espelho das três listas no backend. Uma gravação por vez, sempre do
 * estado **atual** do cache: gravações seguidas se juntam e a última vence, sem
 * corrida entre `invoke`s.
 */
export function persistGameLists(opts: GameListWriteOptions = {}): void {
  if (!hydrated) return;
  pending = true;
  if (opts.userDelete) pendingDestructive = true;
  if (!inFlight) inFlight = flush();
}

/** Espera o espelho em andamento terminar (testes e encerramento). */
export async function whenGameListsSaved(): Promise<void> {
  while (inFlight) await inFlight;
}

/**
 * Antes de restaurar um backup: espera a gravação em andamento e desliga o
 * espelho até a próxima hidratação. Sem isso, uma gravação que chegasse ao
 * backend depois da restauração poria a lista de antes por cima da restaurada.
 */
export async function pauseGameListsMirror(): Promise<void> {
  await whenGameListsSaved();
  hydrated = false;
}

// ── mescla ───────────────────────────────────────────────────────────────────

type Entry = Record<string, unknown>;

function isEntry(value: unknown): value is Entry {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function num(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

/** Os VIPs de um favorito, incluindo o campo antigo `privateServer`. */
function vipsOf(favorite: Entry): Entry[] {
  if (Array.isArray(favorite.vipServers)) return favorite.vipServers.filter(isEntry);
  const legacy = typeof favorite.privateServer === "string" ? favorite.privateServer.trim() : "";
  return legacy ? [{ id: `legacy-${legacy}`, name: "VIP", link: legacy }] : [];
}

function vipKey(vip: Entry): string {
  return typeof vip.link === "string" ? vip.link.trim() : JSON.stringify(vip);
}

/**
 * União de duas listas de favoritos. A ordem e os campos são os de `primary`;
 * o que só `secondary` tem vai para o fim. No mesmo `placeId`, os VIPs são
 * unidos pelo link — nenhum se perde.
 */
export function mergeFavorites(primary: unknown[], secondary: unknown[]): unknown[] {
  const out: unknown[] = [];
  const indexByPlace = new Map<unknown, number>();
  const push = (item: unknown) => {
    if (!isEntry(item) || item.placeId === undefined) {
      out.push(item);
      return;
    }
    const at = indexByPlace.get(item.placeId);
    if (at === undefined) {
      indexByPlace.set(item.placeId, out.length);
      out.push(item);
      return;
    }
    const base = out[at] as Entry;
    const baseVips = vipsOf(base);
    const seen = new Set(baseVips.map(vipKey));
    const extra = vipsOf(item).filter((vip) => !seen.has(vipKey(vip)));
    if (extra.length > 0) {
      out[at] = { ...base, vipServers: [...baseVips, ...extra], privateServer: undefined };
    }
  };
  primary.forEach(push);
  secondary.forEach(push);
  return out;
}

/**
 * União de duas listas de "recentes" pela chave. Fica a entrada mais nova
 * (`stamp`), a lista sai do mais novo para o mais velho e com o tamanho da
 * maior das duas — recentes são cortados por desenho, favoritos nunca.
 */
function mergeRecent(
  primary: unknown[],
  secondary: unknown[],
  key: (e: Entry) => unknown,
  stamp: string,
  combine: (newer: Entry, older: Entry) => Entry = (newer) => newer
): unknown[] {
  const byKey = new Map<unknown, Entry>();
  const loose: unknown[] = [];
  for (const item of [...primary, ...secondary]) {
    if (!isEntry(item) || key(item) === undefined) {
      loose.push(item);
      continue;
    }
    const k = key(item);
    const prev = byKey.get(k);
    if (!prev) byKey.set(k, item);
    else if (num(item[stamp]) > num(prev[stamp])) byKey.set(k, combine(item, prev));
    else byKey.set(k, combine(prev, item));
  }
  const merged = [...byKey.values()].sort((a, b) => num(b[stamp]) - num(a[stamp]));
  const cap = Math.max(primary.length, secondary.length);
  return [...merged, ...loose].slice(0, cap);
}

export function mergeRecentGames(primary: unknown[], secondary: unknown[]): unknown[] {
  return mergeRecent(primary, secondary, (e) => e.placeId, "lastPlayed");
}

export function mergeRecentJobs(primary: unknown[], secondary: unknown[]): unknown[] {
  return mergeRecent(primary, secondary, (e) => e.raw, "lastUsed", (newer, older) => {
    const ids = new Set<number>();
    for (const list of [newer.userIds, older.userIds]) {
      if (Array.isArray(list)) for (const id of list) if (typeof id === "number") ids.add(id);
    }
    return {
      ...newer,
      placeId: newer.placeId ?? older.placeId ?? null,
      userIds: [...ids].sort((a, b) => a - b),
    };
  });
}

const MERGERS: Record<GameListName, (primary: unknown[], secondary: unknown[]) => unknown[]> = {
  favorites: mergeFavorites,
  recentGames: mergeRecentGames,
  recentJobs: mergeRecentJobs,
};

// ── hidratação ───────────────────────────────────────────────────────────────

function isSnapshot(value: unknown): value is Partial<GameListsSnapshot> {
  return isEntry(value) && LIST_NAMES.every((n) => value[n] === undefined || Array.isArray(value[n]));
}

/**
 * Junta o backend com o cache local. Chamada na abertura do app e depois de
 * uma restauração de backup (`backup-restored`).
 *
 * - backend sem arquivo (`null`): migração — o que o `localStorage` tem vai
 *   para o backend (se nada estiver ilegível);
 * - backend com arquivo: união dos dois lados, gravada nos dois. O backend
 *   vem primeiro (ordem e campos dele), o local acrescenta o que faltar;
 * - erro do backend (arquivo ilegível, por exemplo): nada é gravado e o
 *   espelho continua desligado — sobrescrever seria perder o que está lá.
 */
export async function hydrateGameLists(): Promise<void> {
  let remote: unknown;
  try {
    remote = await invoke("get_game_lists");
  } catch (error) {
    console.error("[gameLists] could not read the saved game lists; keeping the local copy:", error);
    return;
  }

  if (remote === null) {
    hydrated = true;
    const local = readLocalSnapshot();
    if (!local) {
      console.error("[gameLists] a local list is unreadable; not migrating it");
      return;
    }
    if (LIST_NAMES.some((n) => local[n].length > 0)) persistGameLists();
    return;
  }

  if (!isSnapshot(remote)) {
    console.error("[gameLists] unexpected answer from get_game_lists; keeping the local copy");
    return;
  }

  let changedLocal = false;
  let changedRemote = false;
  for (const name of LIST_NAMES) {
    const remoteList = remote[name] ?? [];
    const local = readStoredList(name);
    let merged: unknown[];
    if (local.ok) {
      merged = MERGERS[name](remoteList, local.list);
    } else {
      // Ilegível: o texto cru fica guardado ao lado antes de a lista ser reposta.
      try {
        const corruptKey = `${STORAGE_KEYS[name]}.corrupt`;
        if (localStorage.getItem(corruptKey) === null) localStorage.setItem(corruptKey, local.raw);
      } catch {
        // Sem espaço para guardar: segue com o backend, que é legível.
      }
      merged = remoteList;
    }
    const mergedJson = JSON.stringify(merged);
    if (!local.ok || mergedJson !== JSON.stringify(local.list)) {
      localStorage.setItem(STORAGE_KEYS[name], mergedJson);
      changedLocal = true;
    }
    if (mergedJson !== JSON.stringify(remoteList)) changedRemote = true;
  }

  hydrated = true;
  if (changedLocal) notifyGameListsChanged();
  if (changedRemote) persistGameLists();
}

/**
 * Liga a sincronização: hidrata agora e de novo a cada restauração de backup.
 * Devolve a função que desliga o ouvinte.
 */
export function startGameListsSync(): () => void {
  void hydrateGameLists();
  const unlisten = Promise.resolve()
    .then(() =>
      listen("backup-restored", () => {
        void hydrateGameLists();
      })
    )
    .catch((error) => {
      console.error("[gameLists] could not listen for backup restores:", error);
      return null;
    });
  return () => {
    void unlisten.then((fn) => fn?.());
  };
}

/** Só para testes: volta ao estado de antes da hidratação. */
export function resetGameListsSyncForTests(): void {
  hydrated = false;
  inFlight = null;
  pending = false;
  pendingDestructive = false;
}
