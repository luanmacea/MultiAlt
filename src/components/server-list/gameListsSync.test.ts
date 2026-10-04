import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import {
  STORAGE_KEY_FAVORITES,
  STORAGE_KEY_RECENT,
  STORAGE_KEY_RECENT_JOBS,
  hydrateGameLists,
  mergeFavorites,
  mergeRecentGames,
  mergeRecentJobs,
  pauseGameListsMirror,
  resetGameListsSyncForTests,
  startGameListsSync,
  subscribeGameLists,
  whenGameListsSaved,
  type GameListsSnapshot,
} from "./gameListsSync";
import {
  addRecentJob,
  loadFavorites,
  loadRecentGames,
  removeRecentJob,
  saveFavorites,
  updateFavorites,
  type FavoriteGame,
} from "./types";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

function fav(placeId: number, links: string[] = [], name = `Game ${placeId}`): FavoriteGame {
  return {
    placeId,
    name,
    iconUrl: null,
    addedAt: 1,
    vipServers: links.map((link, i) => ({ id: `${placeId}-${i}`, name: `VIP ${i + 1}`, link })),
  };
}

/** Um backend de mentira com o `RAMGameLists.json` em memória. */
function fakeBackend(initial: GameListsSnapshot | null) {
  const state = { file: initial, saves: [] as { lists: GameListsSnapshot; allowDestructive: boolean }[] };
  setInvokeHandler((cmd, args) => {
    if (cmd === "get_game_lists") return state.file;
    if (cmd === "save_game_lists") {
      const { lists, allowDestructive } = args as { lists: GameListsSnapshot; allowDestructive: boolean };
      state.saves.push({ lists, allowDestructive });
      state.file = lists;
      return null;
    }
    return undefined;
  });
  return state;
}

function stored(key: string): unknown {
  return JSON.parse(localStorage.getItem(key) ?? "null");
}

beforeEach(() => {
  resetTauriMocks();
  resetGameListsSyncForTests();
  localStorage.clear();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("hidratação na abertura", () => {
  it("o arquivo do backend repõe um localStorage vazio (WebView zerado, PC novo)", async () => {
    const backend = fakeBackend({
      favorites: [fav(1, ["vip:111"])],
      recentGames: [{ placeId: 9, name: "R", iconUrl: null, lastPlayed: 5 }],
      recentJobs: [{ kind: "job", raw: "abc", placeId: 9, lastUsed: 5, userIds: [] }],
    });
    const changed = vi.fn();
    const off = subscribeGameLists(changed);

    await hydrateGameLists();
    await whenGameListsSaved();
    off();

    expect(loadFavorites()[0].vipServers?.[0].link).toBe("vip:111");
    expect(loadRecentGames().map((g) => g.placeId)).toEqual([9]);
    expect(stored(STORAGE_KEY_RECENT_JOBS)).toHaveLength(1);
    expect(changed).toHaveBeenCalled();
    // Nada a acrescentar ao backend: ele não é regravado.
    expect(backend.saves).toHaveLength(0);
  });

  it("sem arquivo no backend, migra o que o localStorage tem — uma vez", async () => {
    const backend = fakeBackend(null);
    saveFavorites([fav(1, ["vip:111"])]);

    await hydrateGameLists();
    await whenGameListsSaved();

    expect(backend.saves).toHaveLength(1);
    expect(backend.saves[0].lists.favorites).toEqual([fav(1, ["vip:111"])]);
    expect(backend.saves[0].allowDestructive).toBe(false);

    await hydrateGameLists();
    await whenGameListsSaved();
    expect(backend.saves).toHaveLength(1);
  });

  it("sem arquivo e sem nada local, não grava nada", async () => {
    const backend = fakeBackend(null);
    await hydrateGameLists();
    await whenGameListsSaved();
    expect(backend.saves).toHaveLength(0);
  });

  it("os dois lados com dados diferentes são unidos, sem perder entrada nem VIP", async () => {
    const backend = fakeBackend({
      favorites: [fav(1, ["vip:111"]), fav(2)],
      recentGames: [],
      recentJobs: [],
    });
    saveFavorites([fav(1, ["vip:222"]), fav(3, ["vip:333"])]);

    await hydrateGameLists();
    await whenGameListsSaved();

    const merged = loadFavorites();
    expect(merged.map((f) => f.placeId)).toEqual([1, 2, 3]);
    expect(merged[0].vipServers?.map((v) => v.link)).toEqual(["vip:111", "vip:222"]);
    expect(merged[2].vipServers?.map((v) => v.link)).toEqual(["vip:333"]);
    // E o backend recebe a união.
    expect(backend.file?.favorites).toEqual(stored(STORAGE_KEY_FAVORITES));
  });

  it("um backend vazio nunca apaga favoritos locais", async () => {
    const backend = fakeBackend({ favorites: [], recentGames: [], recentJobs: [] });
    saveFavorites([fav(1, ["vip:111"])]);

    await hydrateGameLists();
    await whenGameListsSaved();

    expect(loadFavorites()).toEqual([fav(1, ["vip:111"])]);
    expect(backend.file?.favorites).toEqual([fav(1, ["vip:111"])]);
  });

  it("backend que falhou ao ler: não mexe no local nem grava nada depois", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_game_lists") throw new Error("Failed to parse game lists file");
      return null;
    });
    saveFavorites([fav(1, ["vip:111"])]);

    await hydrateGameLists();
    updateFavorites((list) => [...list, fav(2)]);
    await whenGameListsSaved();

    expect(loadFavorites().map((f) => f.placeId)).toEqual([1, 2]);
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "save_game_lists")).toHaveLength(0);
  });

  it("lista local ilegível: guarda o texto cru ao lado e repõe a do backend", async () => {
    fakeBackend({ favorites: [fav(1, ["vip:111"])], recentGames: [], recentJobs: [] });
    localStorage.setItem(STORAGE_KEY_FAVORITES, "{broken");

    await hydrateGameLists();
    await whenGameListsSaved();

    expect(localStorage.getItem(`${STORAGE_KEY_FAVORITES}.corrupt`)).toBe("{broken");
    expect(loadFavorites()).toEqual([fav(1, ["vip:111"])]);
  });

  it("lista local ilegível sem arquivo no backend: não migra nada", async () => {
    const backend = fakeBackend(null);
    saveFavorites([fav(1)]);
    localStorage.setItem(STORAGE_KEY_RECENT, "<<<");

    await hydrateGameLists();
    await whenGameListsSaved();

    expect(backend.saves).toHaveLength(0);
  });
});

describe("espelho das gravações", () => {
  it("nada é espelhado antes da hidratação (o backend pode ter o que o local não tem)", async () => {
    const backend = fakeBackend({ favorites: [fav(1, ["vip:111"])], recentGames: [], recentJobs: [] });
    saveFavorites([fav(2)]);
    await whenGameListsSaved();
    expect(backend.saves).toHaveLength(0);

    await hydrateGameLists();
    await whenGameListsSaved();
    expect(loadFavorites().map((f) => f.placeId)).toEqual([1, 2]);
  });

  it("depois da hidratação, cada gravação vai para o backend com as três listas", async () => {
    const backend = fakeBackend(null);
    await hydrateGameLists();

    updateFavorites((list) => [...list, fav(5, ["vip:5"])]);
    addRecentJob("job-1", 5, 10, [7]);
    await whenGameListsSaved();

    expect(backend.file?.favorites).toEqual([fav(5, ["vip:5"])]);
    expect(backend.file?.recentJobs).toHaveLength(1);
    expect(backend.saves.every((s) => s.allowDestructive === false)).toBe(true);
  });

  it("gravações seguidas saem uma de cada vez e a última vence", async () => {
    let release: () => void = () => {};
    const seen: GameListsSnapshot[] = [];
    let first = true;
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_game_lists") return null;
      if (cmd === "save_game_lists") {
        seen.push((args as { lists: GameListsSnapshot }).lists);
        if (first) {
          first = false;
          return new Promise<null>((resolve) => (release = () => resolve(null)));
        }
      }
      return null;
    });
    await hydrateGameLists();

    updateFavorites((list) => [...list, fav(1)]);
    updateFavorites((list) => [...list, fav(2)]);
    updateFavorites((list) => [...list, fav(3)]);
    release();
    await whenGameListsSaved();

    expect(seen).toHaveLength(2);
    expect((seen[1].favorites as FavoriteGame[]).map((f) => f.placeId)).toEqual([1, 2, 3]);
  });

  it("só a exclusão pedida pelo usuário vai com allowDestructive", async () => {
    const backend = fakeBackend(null);
    saveFavorites([fav(1, ["vip:1"])]);
    await hydrateGameLists();
    await whenGameListsSaved();

    updateFavorites((list) => list.filter((f) => f.placeId !== 1), { userDelete: true });
    await whenGameListsSaved();
    expect(backend.saves[backend.saves.length - 1]?.allowDestructive).toBe(true);

    addRecentJob("job-1", 1, 10, [1]);
    await whenGameListsSaved();
    expect(backend.saves[backend.saves.length - 1]?.allowDestructive).toBe(false);

    removeRecentJob("job-1");
    await whenGameListsSaved();
    expect(backend.saves[backend.saves.length - 1]?.allowDestructive).toBe(true);
  });
});

describe("restauração de backup", () => {
  it("o evento backup-restored traz de volta os VIPs que o backup tinha", async () => {
    const backend = fakeBackend(null);
    const stop = startGameListsSync();
    saveFavorites([fav(1, ["vip:111"])]);
    await vi.waitFor(() => expect(backend.saves).toHaveLength(1));
    await whenGameListsSaved();

    // O usuário apaga o VIP...
    updateFavorites((list) => list.map((f) => ({ ...f, vipServers: [] })), { userDelete: true });
    await whenGameListsSaved();
    expect(loadFavorites()[0].vipServers).toEqual([]);

    // ...e restaura um backup que o tinha.
    backend.file = { favorites: [fav(1, ["vip:111"])], recentGames: [], recentJobs: [] };
    emitTauriEvent("backup-restored", { restored: ["RAMGameLists.json"] });
    await vi.waitFor(() => expect(loadFavorites()[0].vipServers?.map((v) => v.link)).toEqual(["vip:111"]));
    stop();
  });

  it("durante a restauração nada é espelhado, e a hidratação religa o espelho", async () => {
    const backend = fakeBackend(null);
    await hydrateGameLists();

    await pauseGameListsMirror();
    updateFavorites((list) => [...list, fav(9)]);
    await whenGameListsSaved();
    expect(backend.saves).toHaveLength(0);

    backend.file = { favorites: [fav(1, ["vip:1"])], recentGames: [], recentJobs: [] };
    await hydrateGameLists();
    await whenGameListsSaved();
    expect((backend.file.favorites as FavoriteGame[]).map((f) => f.placeId)).toEqual([1, 9]);
  });
});

describe("mescla", () => {
  it("favoritos: união por placeId, ordem do primeiro, VIPs unidos pelo link", () => {
    const merged = mergeFavorites(
      [fav(1, ["vip:a"]), fav(2, [], "Two (backend)")],
      [fav(2, ["vip:b"], "Two (local)"), fav(1, ["vip:a", " vip:c "]), fav(3)]
    ) as FavoriteGame[];
    expect(merged.map((f) => f.placeId)).toEqual([1, 2, 3]);
    expect(merged[0].vipServers?.map((v) => v.link.trim())).toEqual(["vip:a", "vip:c"]);
    expect(merged[1].name).toBe("Two (backend)");
    expect(merged[1].vipServers?.map((v) => v.link)).toEqual(["vip:b"]);
  });

  it("favoritos: o link antigo em privateServer conta como VIP", () => {
    const merged = mergeFavorites(
      [{ placeId: 1, name: "Old", iconUrl: null, addedAt: 1, privateServer: "vip:legacy" }],
      [fav(1, ["vip:new"])]
    ) as FavoriteGame[];
    expect(merged[0].vipServers?.map((v) => v.link)).toEqual(["vip:legacy", "vip:new"]);
  });

  it("jogos recentes: fica a entrada mais nova, do mais novo para o mais velho", () => {
    const merged = mergeRecentGames(
      [
        { placeId: 1, name: "A", lastPlayed: 10 },
        { placeId: 2, name: "B", lastPlayed: 5 },
      ],
      [
        { placeId: 2, name: "B2", lastPlayed: 20 },
        { placeId: 3, name: "C", lastPlayed: 1 },
      ]
    ) as { placeId: number; name: string }[];
    expect(merged.map((g) => g.placeId)).toEqual([2, 1]);
    expect(merged[0].name).toBe("B2");
  });

  it("servidores recentes: une as contas donas do alvo", () => {
    const merged = mergeRecentJobs(
      [{ kind: "vip", raw: "vip:1", placeId: 5, lastUsed: 10, userIds: [1] }],
      [{ kind: "vip", raw: "vip:1", placeId: null, lastUsed: 20, userIds: [2] }]
    ) as { userIds: number[]; placeId: number | null; lastUsed: number }[];
    expect(merged).toHaveLength(1);
    expect(merged[0].userIds).toEqual([1, 2]);
    expect(merged[0].placeId).toBe(5);
    expect(merged[0].lastUsed).toBe(20);
  });
});
