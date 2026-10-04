import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => null),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  addRecentGame,
  addRecentJob,
  classifyJobInput,
  loadFavorites,
  loadRecentGames,
  loadRecentJobs,
  looksLikeJoinLink,
  makeVipId,
  parsePlaceIdInput,
  recordRecentGame,
  removeRecentJob,
  resolveRecentGame,
  saveFavorites,
  saveRecentGames,
  saveRecentJobs,
  updateFavorites,
  visibleRecentJobs,
  type FavoriteGame,
  type RecentGame,
  type RecentJobEntry,
} from "./types";

const invokeMock = invoke as unknown as Mock;

const STORAGE_KEY_FAVORITES = "ram_favorite_games";
const STORAGE_KEY_RECENT = "ram_recent_games";

function recent(placeId: number, name = `Game ${placeId}`): RecentGame {
  return { placeId, name, iconUrl: `icon-${placeId}.png`, lastPlayed: 1 };
}

beforeEach(() => {
  localStorage.clear();
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("makeVipId", () => {
  it("produces unique non-empty ids", () => {
    const a = makeVipId();
    const b = makeVipId();
    expect(a).toBeTruthy();
    expect(typeof a).toBe("string");
    expect(a).not.toBe(b);
  });
});

describe("favorites storage", () => {
  it("returns an empty list when nothing is stored", () => {
    expect(loadFavorites()).toEqual([]);
  });

  it("round-trips through localStorage", () => {
    const favorites: FavoriteGame[] = [
      {
        placeId: 123,
        name: "Test Game",
        iconUrl: "icon.png",
        addedAt: 1700000000000,
        vipServers: [{ id: "vip-1", name: "VIP", link: "https://roblox.com/share?code=abc" }],
      },
    ];
    saveFavorites(favorites);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY_FAVORITES) || "null")).toEqual(favorites);
    expect(loadFavorites()).toEqual(favorites);
  });

  it("returns an empty list for corrupt JSON", () => {
    localStorage.setItem(STORAGE_KEY_FAVORITES, "{not json");
    expect(loadFavorites()).toEqual([]);
  });

  it("returns an empty list when the stored value is not an array", () => {
    localStorage.setItem(STORAGE_KEY_FAVORITES, '{"placeId":1}');
    expect(loadFavorites()).toEqual([]);
  });

  it("backfills vipServers from the legacy privateServer field", () => {
    localStorage.setItem(
      STORAGE_KEY_FAVORITES,
      JSON.stringify([
        { placeId: 1, name: "Legacy", iconUrl: null, addedAt: 1, privateServer: "vip:12345" },
      ])
    );

    const [entry] = loadFavorites();
    expect(entry.vipServers).toHaveLength(1);
    expect(entry.vipServers?.[0].name).toBe("VIP");
    expect(entry.vipServers?.[0].link).toBe("vip:12345");
    expect(entry.vipServers?.[0].id).toBeTruthy();
    expect(entry.privateServer).toBe("vip:12345");
  });

  it("backfills an empty vipServers array when there is no legacy link", () => {
    localStorage.setItem(
      STORAGE_KEY_FAVORITES,
      JSON.stringify([{ placeId: 1, name: "Plain", iconUrl: null, addedAt: 1 }])
    );
    expect(loadFavorites()[0].vipServers).toEqual([]);
  });

  it("leaves an existing vipServers list untouched", () => {
    const stored = [
      {
        placeId: 1,
        name: "Has VIP",
        iconUrl: null,
        addedAt: 1,
        privateServer: "vip:old",
        vipServers: [{ id: "keep", name: "Main", link: "vip:new" }],
      },
    ];
    localStorage.setItem(STORAGE_KEY_FAVORITES, JSON.stringify(stored));
    expect(loadFavorites()[0].vipServers).toEqual([{ id: "keep", name: "Main", link: "vip:new" }]);
  });
});

/**
 * O dono já perdeu VIPs salvos: uma tela gravava a cópia velha da lista por
 * cima da gravada. `updateFavorites` é o único caminho das telas, e ele lê a
 * lista atual, recusa gravar por cima de lista ilegível e só tira favorito ou
 * VIP quando a exclusão foi pedida pelo usuário.
 */
describe("updateFavorites", () => {
  const vip = (id: string, link: string) => ({ id, name: id, link });
  const base = (): FavoriteGame[] => [
    { placeId: 1, name: "One", iconUrl: null, addedAt: 1, vipServers: [vip("a", "vip:a")] },
    { placeId: 2, name: "Two", iconUrl: null, addedAt: 1, vipServers: [] },
  ];

  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("parte da lista gravada agora, não de uma cópia antiga", () => {
    saveFavorites(base());
    const staleCopy = loadFavorites();
    // Outra tela adiciona um VIP depois de esta ter lido a lista.
    updateFavorites((list) =>
      list.map((f) => (f.placeId === 2 ? { ...f, vipServers: [vip("b", "vip:b")] } : f))
    );

    updateFavorites((list) => list.map((f) => (f.placeId === 1 ? { ...f, name: "Renamed" } : f)));

    expect(staleCopy[1].vipServers).toEqual([]);
    expect(loadFavorites()[1].vipServers).toEqual([vip("b", "vip:b")]);
    expect(loadFavorites()[0].name).toBe("Renamed");
  });

  it("recusa, sem exclusão explícita, uma mudança que tira um VIP", () => {
    saveFavorites(base());
    const result = updateFavorites((list) => list.map((f) => ({ ...f, vipServers: [] })));
    expect(result).toBeNull();
    expect(loadFavorites()[0].vipServers).toEqual([vip("a", "vip:a")]);
  });

  it("recusa, sem exclusão explícita, uma mudança que tira um favorito", () => {
    saveFavorites(base());
    expect(updateFavorites((list) => list.slice(0, 1))).toBeNull();
    expect(updateFavorites(() => [])).toBeNull();
    expect(loadFavorites()).toHaveLength(2);
  });

  it("a exclusão pedida pelo usuário continua funcionando", () => {
    saveFavorites(base());
    expect(
      updateFavorites((list) => list.map((f) => ({ ...f, vipServers: [] })), { userDelete: true })
    ).not.toBeNull();
    expect(updateFavorites((list) => list.filter((f) => f.placeId !== 2), { userDelete: true })).toHaveLength(1);
    expect(loadFavorites().map((f) => f.placeId)).toEqual([1]);
    expect(loadFavorites()[0].vipServers).toEqual([]);
  });

  it("não grava por cima de uma lista ilegível — nem para adicionar", () => {
    localStorage.setItem(STORAGE_KEY_FAVORITES, "{not json");
    expect(updateFavorites((list) => [...list, base()[1]])).toBeNull();
    expect(localStorage.getItem(STORAGE_KEY_FAVORITES)).toBe("{not json");
  });

  it("recentes: lista ilegível não vira lista vazia ao registrar um jogo ou servidor", () => {
    localStorage.setItem(STORAGE_KEY_RECENT, "<<<broken>>>");
    addRecentGame(recent(1), 10);
    expect(localStorage.getItem(STORAGE_KEY_RECENT)).toBe("<<<broken>>>");

    localStorage.setItem("ram_recent_jobs", "<<<broken>>>");
    addRecentJob("abc", 1, 10, [1]);
    expect(localStorage.getItem("ram_recent_jobs")).toBe("<<<broken>>>");
  });
});

describe("recent games storage", () => {
  it("returns an empty list when nothing is stored", () => {
    expect(loadRecentGames()).toEqual([]);
  });

  it("round-trips through localStorage", () => {
    const games = [recent(1), recent(2)];
    saveRecentGames(games);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY_RECENT) || "null")).toEqual(games);
    expect(loadRecentGames()).toEqual(games);
  });

  it("returns an empty list for corrupt JSON", () => {
    localStorage.setItem(STORAGE_KEY_RECENT, "<<<broken>>>");
    expect(loadRecentGames()).toEqual([]);
  });
});

describe("addRecentGame", () => {
  it("prepends a new entry and stamps lastPlayed", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2024-01-01T00:00:00Z"));
    saveRecentGames([recent(1)]);

    addRecentGame(recent(2), 10);

    const games = loadRecentGames();
    expect(games.map((g) => g.placeId)).toEqual([2, 1]);
    expect(games[0].lastPlayed).toBe(Date.parse("2024-01-01T00:00:00Z"));
  });

  it("dedupes by placeId and moves the entry to the front", () => {
    saveRecentGames([recent(1), recent(2), recent(3)]);

    addRecentGame(recent(3, "Renamed"), 10);

    const games = loadRecentGames();
    expect(games.map((g) => g.placeId)).toEqual([3, 1, 2]);
    expect(games).toHaveLength(3);
    expect(games[0].name).toBe("Renamed");
  });

  it("caps the list at maxCount, dropping the oldest", () => {
    saveRecentGames([recent(1), recent(2), recent(3)]);

    addRecentGame(recent(4), 3);

    expect(loadRecentGames().map((g) => g.placeId)).toEqual([4, 1, 2]);
  });
});

describe("recordRecentGame", () => {
  it.each([0, -1, Number.NaN])("ignores non-positive placeId %s", async (placeId) => {
    await recordRecentGame(placeId, null, 10);
    expect(localStorage.getItem(STORAGE_KEY_RECENT)).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("stores the entry without calling invoke when name and icon are supplied", async () => {
    await recordRecentGame(123, 5, 10, { name: "Supplied", iconUrl: "supplied.png" });

    expect(loadRecentGames()).toEqual([
      expect.objectContaining({ placeId: 123, name: "Supplied", iconUrl: "supplied.png" }),
    ]);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reuses an existing entry's name and icon instead of calling invoke", async () => {
    saveRecentGames([{ placeId: 123, name: "Known", iconUrl: "known.png", lastPlayed: 1 }]);

    await recordRecentGame(123, null, 10);

    expect(invokeMock).not.toHaveBeenCalled();
    expect(loadRecentGames()[0]).toMatchObject({ name: "Known", iconUrl: "known.png" });
  });

  it("resolves both the name and the icon in a single call", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "batched_get_game_info"
        ? { placeId: 123, universeId: 9, name: "Resolved Game", iconUrl: "resolved.png" }
        : null
    );

    await recordRecentGame(123, 42, 10);

    // Uma chamada: nome e ícone saem do mesmo corpo da API do Roblox, e o
    // backend guarda os dois. Antes eram duas, e a do nome não tinha cache.
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("batched_get_game_info", {
      placeId: 123,
      userId: 42,
    });

    expect(loadRecentGames()[0]).toMatchObject({
      placeId: 123,
      name: "Resolved Game",
      iconUrl: "resolved.png",
    });
  });

  it("mantém o nome que já tinha quando só falta o ícone", async () => {
    saveRecentGames([{ placeId: 123, name: "Known", iconUrl: null, lastPlayed: 1 }]);
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "batched_get_game_info"
        ? { placeId: 123, universeId: null, name: null, iconUrl: "late.png" }
        : null
    );

    await recordRecentGame(123, null, 10);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(loadRecentGames()[0]).toMatchObject({ name: "Known", iconUrl: "late.png" });
  });

  it("keeps the optimistic entry when resolution fails", async () => {
    invokeMock.mockRejectedValue(new Error("offline"));

    await recordRecentGame(123, null, 10);

    expect(loadRecentGames()[0]).toMatchObject({ placeId: 123, name: "123", iconUrl: null });
  });

  it("honours maxCount while recording", async () => {
    saveRecentGames([recent(1), recent(2)]);

    await recordRecentGame(3, null, 2, { name: "Third", iconUrl: "third.png" });

    expect(loadRecentGames().map((g) => g.placeId)).toEqual([3, 1]);
  });
});

describe("resolveRecentGame", () => {
  it("returns null when nothing needs resolving", async () => {
    const game = recent(7, "Named");
    expect(await resolveRecentGame(game, null)).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("resolves a placeholder name", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "batched_get_game_info"
        ? { placeId: 9, universeId: null, name: "Real Name", iconUrl: null }
        : null
    );

    const resolved = await resolveRecentGame(
      { placeId: 9, name: "9", iconUrl: "icon.png", lastPlayed: 1 },
      null
    );

    expect(resolved).toMatchObject({ placeId: 9, name: "Real Name", iconUrl: "icon.png" });
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("returns null when the backend resolves nothing new", async () => {
    invokeMock.mockResolvedValue(null);

    const resolved = await resolveRecentGame(
      { placeId: 9, name: "9", iconUrl: null, lastPlayed: 1 },
      null
    );

    expect(resolved).toBeNull();
  });
});

describe("parsePlaceIdInput", () => {
  it("accepts a plain numeric place id", () => {
    expect(parsePlaceIdInput("606849621")).toBe(606849621);
    expect(parsePlaceIdInput("  606849621  ")).toBe(606849621);
  });

  it("reads the place id out of a game link", () => {
    expect(parsePlaceIdInput("https://www.roblox.com/games/606849621/Jailbreak")).toBe(606849621);
    expect(parsePlaceIdInput("http://roblox.com/games/606849621")).toBe(606849621);
    expect(parsePlaceIdInput("www.roblox.com/games/606849621/Jailbreak")).toBe(606849621);
    expect(parsePlaceIdInput("https://ROBLOX.com/Games/606849621/Jailbreak")).toBe(606849621);
  });

  /**
   * O bug: o campo juntava todos os dígitos da URL, e o código do servidor
   * privado virava parte do place (`60684962198765`).
   */
  it("ignores the query string instead of gluing its digits to the place id", () => {
    expect(
      parsePlaceIdInput(
        "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=98765"
      )
    ).toBe(606849621);
    expect(parsePlaceIdInput("https://www.roblox.com/games/606849621/Jailbreak#play")).toBe(
      606849621
    );
  });

  it("reads the placeId query parameter", () => {
    expect(parsePlaceIdInput("https://www.roblox.com/games/start?placeId=606849621")).toBe(
      606849621
    );
    expect(
      parsePlaceIdInput("https://www.roblox.com/games/start?placeId=606849621&launchData=x9")
    ).toBe(606849621);
    expect(parsePlaceIdInput("roblox://placeId=606849621&gameInstanceId=job-1")).toBe(606849621);
  });

  it("refuses links that carry no place id", () => {
    expect(parsePlaceIdInput("https://www.roblox.com/share?code=abc123&type=Server")).toBeNull();
    expect(parsePlaceIdInput("https://ro.blox.com/Ebh5?pid=share_link")).toBeNull();
  });

  it("refuses text with no place id at all", () => {
    expect(parsePlaceIdInput("")).toBeNull();
    expect(parsePlaceIdInput("   ")).toBeNull();
    expect(parsePlaceIdInput("Jailbreak")).toBeNull();
    expect(parsePlaceIdInput("0")).toBeNull();
    expect(parsePlaceIdInput("606849621x")).toBeNull();
  });
});

describe("looksLikeJoinLink", () => {
  it("recognizes roblox links so the field can point at the Follow tab", () => {
    expect(looksLikeJoinLink("https://www.roblox.com/share?code=abc123&type=Server")).toBe(true);
    expect(looksLikeJoinLink("https://ro.blox.com/Ebh5?pid=share_link")).toBe(true);
    expect(looksLikeJoinLink("roblox.com/games/606849621")).toBe(true);
  });

  it("does not call plain typing a link", () => {
    expect(looksLikeJoinLink("Jailbreak")).toBe(false);
    expect(looksLikeJoinLink("606849621x")).toBe(false);
  });
});

// ── Servidores recentes (job ids) ─────────────────────────────────────────────

const STORAGE_KEY_RECENT_JOBS = "ram_recent_jobs";

const JOB_A = "11111111-2222-3333-4444-555555555555";
const JOB_B = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

describe("classifyJobInput", () => {
  it("calls a plain job id a job", () => {
    expect(classifyJobInput(JOB_A)).toBe("job");
  });

  it("recognizes the vip: prefix in any case", () => {
    expect(classifyJobInput("vip:abc123")).toBe("vip");
    expect(classifyJobInput("VIP:abc123")).toBe("vip");
  });

  it("recognizes a private server link by its code parameter", () => {
    expect(
      classifyJobInput("https://www.roblox.com/games/606849621/X?privateServerLinkCode=987")
    ).toBe("link");
    expect(classifyJobInput("https://www.roblox.com/share?code=abc123&type=Server")).toBe("link");
  });

  /**
   * Link curto do AppsFlyer: a query de verdade vem **codificada** dentro do
   * `af_dp`, então `code=` não aparece no texto cru. Classificado como `job`,
   * ele entraria na lista como alvo público — o código privado do dono à
   * mostra para qualquer conta. É formato real (mesmo caso de
   * `extract_query_param_value_recursive`, `launch_shared.rs`).
   */
  it("sees through a double-encoded share link", () => {
    expect(
      classifyJobInput(
        "https://ro.blox.com/Ebh5?af_dp=roblox%3A%2F%2Fnavigation%2Fshare_links%3Fcode%3DDEADBEEF%26type%3DServer"
      )
    ).toBe("link");
  });

  it("survives a broken percent-escape instead of throwing", () => {
    expect(classifyJobInput("job-%zz-%-id")).toBe("job");
  });
});

describe("recent jobs storage", () => {
  it("returns an empty list when nothing is stored", () => {
    expect(loadRecentJobs()).toEqual([]);
  });

  it("round-trips through localStorage", () => {
    const entries: RecentJobEntry[] = [
      { kind: "job", raw: JOB_A, placeId: 606849621, lastUsed: 5, userIds: [] },
      { kind: "vip", raw: "vip:abc123", placeId: 123, lastUsed: 4, userIds: [7] },
    ];
    saveRecentJobs(entries);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY_RECENT_JOBS) || "null")).toEqual(entries);
    expect(loadRecentJobs()).toEqual(entries);
  });

  it("returns an empty list for corrupt JSON", () => {
    localStorage.setItem(STORAGE_KEY_RECENT_JOBS, "<<<broken>>>");
    expect(loadRecentJobs()).toEqual([]);
  });
});

describe("addRecentJob", () => {
  it("ignores an empty job", () => {
    addRecentJob("", 606849621, 12, [1]);
    addRecentJob("   ", 606849621, 12, [1]);
    expect(localStorage.getItem(STORAGE_KEY_RECENT_JOBS)).toBeNull();
  });

  it("prepends the entry and stamps lastUsed", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2024-01-01T00:00:00Z"));

    addRecentJob(JOB_A, 606849621, 12, [1]);

    expect(loadRecentJobs()).toEqual([
      {
        kind: "job",
        raw: JOB_A,
        placeId: 606849621,
        lastUsed: Date.parse("2024-01-01T00:00:00Z"),
        userIds: [1],
      },
    ]);
  });

  it("moves a job it already has to the top without duplicating it", () => {
    addRecentJob(JOB_A, 606849621, 12, [1]);
    addRecentJob(JOB_B, 606849621, 12, [1]);
    addRecentJob(JOB_A, 606849621, 12, [1]);

    expect(loadRecentJobs().map((e) => e.raw)).toEqual([JOB_A, JOB_B]);
  });

  it("drops the oldest entry when the list passes the limit", () => {
    addRecentJob("job-1", 1, 2, [1]);
    addRecentJob("job-2", 1, 2, [1]);
    addRecentJob("job-3", 1, 2, [1]);

    expect(loadRecentJobs().map((e) => e.raw)).toEqual(["job-3", "job-2"]);
  });

  it("keeps at least one entry even with a broken limit", () => {
    addRecentJob("job-1", 1, 0, [1]);
    expect(loadRecentJobs().map((e) => e.raw)).toEqual(["job-1"]);
  });

  it("keeps the vip: prefix exactly as typed, so the meaning survives", () => {
    addRecentJob("  VIP:abc123  ", 606849621, 12, [1]);
    expect(loadRecentJobs()[0]).toMatchObject({ kind: "vip", raw: "VIP:abc123" });
  });

  it("remembers the place of an entry re-used without one", () => {
    addRecentJob(JOB_A, 606849621, 12, [1]);
    addRecentJob(JOB_A, null, 12, [1]);
    expect(loadRecentJobs()[0].placeId).toBe(606849621);
  });

  it("adds up the accounts that used the same private target", () => {
    addRecentJob("vip:abc123", 1, 12, [7]);
    addRecentJob("vip:abc123", 1, 12, [8, 7]);
    expect(loadRecentJobs()[0].userIds).toEqual([7, 8]);
  });
});

/**
 * Um link VIP/privado vale para quem o tem: deixá-lo aparecer na lista de outra
 * conta entrega o servidor privado de uma conta a outra sem o dono pedir. Job
 * id público não tem esse problema — é o mesmo servidor que a aba Servers lista
 * para todo mundo.
 */
describe("visibleRecentJobs", () => {
  it("shows a public job id to every account", () => {
    addRecentJob(JOB_A, 606849621, 12, [1]);
    expect(visibleRecentJobs(loadRecentJobs(), 2).map((e) => e.raw)).toEqual([JOB_A]);
    expect(visibleRecentJobs(loadRecentJobs(), null).map((e) => e.raw)).toEqual([JOB_A]);
  });

  it("hides a private target from an account that never used it", () => {
    addRecentJob("vip:abc123", 1, 12, [7]);
    expect(visibleRecentJobs(loadRecentJobs(), 8)).toEqual([]);
    expect(visibleRecentJobs(loadRecentJobs(), null)).toEqual([]);
  });

  it("shows a private target to each account that used it", () => {
    addRecentJob("vip:abc123", 1, 12, [7, 8]);
    expect(visibleRecentJobs(loadRecentJobs(), 7).map((e) => e.raw)).toEqual(["vip:abc123"]);
    expect(visibleRecentJobs(loadRecentJobs(), 8).map((e) => e.raw)).toEqual(["vip:abc123"]);
  });

  it("hides a private link entry the same way", () => {
    addRecentJob("https://www.roblox.com/games/1/X?privateServerLinkCode=987", 1, 12, [7]);
    expect(visibleRecentJobs(loadRecentJobs(), 9)).toEqual([]);
    expect(visibleRecentJobs(loadRecentJobs(), 7)).toHaveLength(1);
  });
});

describe("removeRecentJob", () => {
  it("removes only the entry asked for", () => {
    addRecentJob(JOB_A, 1, 12, [1]);
    addRecentJob(JOB_B, 1, 12, [1]);

    removeRecentJob(JOB_A);

    expect(loadRecentJobs().map((e) => e.raw)).toEqual([JOB_B]);
  });
});
