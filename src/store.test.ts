import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account, PlatformCapabilities } from "./types";

const invokeMock = vi.fn();
const recordRecentGameMock = vi.fn(async () => {});
const unlistenMock = vi.fn();
const listenHandlers = new Map<string, Array<(event: { payload: unknown }) => void>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
  isTauri: () => false,
  convertFileSrc: (p: string) => p,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) => {
    const handlers = listenHandlers.get(event) ?? [];
    handlers.push(handler);
    listenHandlers.set(event, handlers);
    return Promise.resolve(() => unlistenMock(event));
  },
}));

vi.mock("./components/server-list/types", () => ({
  recordRecentGame: (...args: unknown[]) => recordRecentGameMock(...(args as [])),
}));

import { StoreProvider, useStore, type StoreValue } from "./store";

function account(overrides: Partial<Account> & { UserID: number }): Account {
  return {
    Valid: true,
    SecurityToken: "token",
    Username: `user${overrides.UserID}`,
    LastUse: new Date().toISOString(),
    Alias: "",
    Description: "",
    Password: "",
    Group: "",
    Fields: {},
    LastAttemptedRefresh: new Date().toISOString(),
    BrowserTrackerID: "",
    ...overrides,
  };
}

function caps(overrides: Partial<PlatformCapabilities> = {}): PlatformCapabilities {
  return {
    os: "windows",
    sessionType: "",
    preferredRunner: "",
    detectedRunner: "",
    runnerPath: null,
    supportsSingleLaunch: true,
    supportsMultiLaunch: true,
    supportsWatcher: true,
    supportsWatcherMemory: true,
    supportsWindowControls: true,
    supportsBotting: true,
    supportsUpdater: true,
    supportsClientSettings: true,
    reasons: [],
    warnings: [],
    ...overrides,
  };
}

let accountsData: Account[] = [];
let settingsData: Record<string, Record<string, string>> = {};
let capabilitiesData: PlatformCapabilities = caps();
let presenceRows: unknown[] = [];
let runningInstances: unknown[] = [];
let needsPasswordValue = false;
let updateResult: unknown = null;
const failures = new Map<string, unknown>();
const results = new Map<string, unknown>();

function defaultInvoke(cmd: string): unknown {
  switch (cmd) {
    case "needs_password":
      return needsPasswordValue;
    case "get_accounts":
      return accountsData;
    case "get_all_settings":
      return settingsData;
    case "get_platform_capabilities":
      return capabilitiesData;
    case "is_accounts_encrypted":
      return false;
    case "batched_get_avatar_headshots":
      return [];
    case "get_presence":
      return presenceRows;
    case "get_running_instances":
      return runningInstances;
    case "check_for_updates_with_channels":
      return updateResult;
    case "cmd_kill_all_roblox":
      return 0;
    case "get_theme":
      return { accounts_background: "#101010" };
    default:
      return null;
  }
}

function invokeCalls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

function lastArgs(cmd: string): Record<string, unknown> {
  const calls = invokeCalls(cmd);
  if (calls.length === 0) throw new Error(`no invoke("${cmd}") calls`);
  return calls[calls.length - 1][1] as Record<string, unknown>;
}

function emit(event: string, payload: unknown) {
  const handlers = listenHandlers.get(event) ?? [];
  for (const handler of handlers) handler({ payload });
}

async function renderStore() {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(StoreProvider, null, children);
  const view = renderHook(() => useStore(), { wrapper });
  await waitFor(() => expect(view.result.current.initialized).toBe(true));
  return view;
}

beforeEach(() => {
  invokeMock.mockReset();
  recordRecentGameMock.mockClear();
  unlistenMock.mockClear();
  listenHandlers.clear();
  failures.clear();
  results.clear();
  accountsData = [];
  settingsData = {};
  capabilitiesData = caps();
  presenceRows = [];
  runningInstances = [];
  needsPasswordValue = false;
  updateResult = null;
  localStorage.clear();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (failures.has(cmd)) throw failures.get(cmd);
    if (results.has(cmd)) return results.get(cmd);
    return defaultInvoke(cmd);
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useStore", () => {
  it("throws when used outside the provider", () => {
    expect(() => renderHook(() => useStore())).toThrow(/StoreProvider/);
  });
});

describe("store bootstrap", () => {
  it("loads accounts, settings and saved launch fields", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 })];
    settingsData = {
      General: {
        SavedPlaceId: "111",
        SavedJobId: "job-1",
        SavedLaunchData: "ld",
        ShuffleJobId: "true",
        HideUsernames: "true",
        HiddenNameLetters: "3",
        ShowAvatarsWhenHidden: "true",
        HideRobuxWhenHidden: "true",
      },
      Developer: { DevMode: "true" },
    };

    const { result } = await renderStore();

    expect(result.current.accounts).toHaveLength(2);
    expect(result.current.placeId).toBe("111");
    expect(result.current.jobId).toBe("job-1");
    expect(result.current.launchData).toBe("ld");
    expect(result.current.shuffleJobId).toBe(true);
    expect(result.current.hideUsernames).toBe(true);
    expect(result.current.hiddenNameLetters).toBe(3);
    expect(result.current.showAvatarsWhenHidden).toBe(true);
    expect(result.current.hideRobuxWhenHidden).toBe(true);
    expect(result.current.devMode).toBe(true);
  });

  it("falls back to 0 hidden letters for a non-numeric setting", async () => {
    settingsData = { General: { HiddenNameLetters: "abc" } };
    const { result } = await renderStore();
    expect(result.current.hiddenNameLetters).toBe(0);
  });

  it("keeps accounts empty and surfaces needsPassword when locked", async () => {
    needsPasswordValue = true;
    accountsData = [account({ UserID: 1 })];

    const { result } = await renderStore();

    expect(result.current.needsPassword).toBe(true);
    expect(result.current.accounts).toEqual([]);
    expect(invokeCalls("get_accounts")).toHaveLength(0);
  });

  it("opens the encryption onboarding on a fresh install", async () => {
    settingsData = { General: { EncryptionOnboardingState: "pending" } };
    const { result } = await renderStore();
    expect(result.current.encryptionSetupOpen).toBe(true);
    expect(result.current.encryptionSetupMode).toBe("firstRun");
    // firstRun mode cannot be dismissed
    act(() => result.current.closeEncryptionSetup());
    expect(result.current.encryptionSetupOpen).toBe(true);
  });

  it("opens the first-run walkthrough once encryption onboarding is done", async () => {
    settingsData = {
      General: { FirstRunWalkthroughState: "pending", EncryptionOnboardingState: "completed" },
    };
    const { result } = await renderStore();
    expect(result.current.firstRunWalkthroughOpen).toBe(true);
    expect(result.current.firstRunWalkthroughMode).toBe("firstRun");
  });

  it("records an error when loading accounts fails", async () => {
    failures.set("get_accounts", "boom");
    const { result } = await renderStore();
    expect(result.current.error).toBe("boom");
  });
});

describe("groups and filtering", () => {
  it("collapses a single Default group into a flat list", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2, Group: "Default" })];
    const { result } = await renderStore();

    expect(result.current.groups).toHaveLength(1);
    expect(result.current.groups[0].key).toBe("__all__");
    expect(result.current.groups[0].accounts).toHaveLength(2);
  });

  it("sorts named groups by their numeric prefix and strips it from the label", async () => {
    accountsData = [
      account({ UserID: 1, Group: "20 Bots" }),
      account({ UserID: 2, Group: "5 Mains" }),
      account({ UserID: 3, Group: "Zeta" }),
    ];
    const { result } = await renderStore();

    expect(result.current.groups.map((g) => g.key)).toEqual(["5 Mains", "20 Bots", "Zeta"]);
    expect(result.current.groups.map((g) => g.displayName)).toEqual(["Mains", "Bots", "Zeta"]);
    expect(result.current.groups[2].sortKey).toBe(999999);
  });

  it("returns one synthetic group when grouping is disabled", async () => {
    accountsData = [account({ UserID: 1, Group: "A" }), account({ UserID: 2, Group: "B" })];
    const { result } = await renderStore();

    act(() => result.current.setShowGroups(false));

    expect(result.current.groups).toHaveLength(1);
    expect(result.current.groups[0].key).toBe("__all__");
    expect(result.current.orderedUserIds).toEqual([1, 2]);
  });

  it("filters on username, alias, description and group", async () => {
    accountsData = [
      account({ UserID: 1, Username: "alpha" }),
      account({ UserID: 2, Username: "beta", Alias: "NEEDLE" }),
      account({ UserID: 3, Username: "gamma", Description: "has needle inside" }),
      account({ UserID: 4, Username: "delta", Group: "Needles" }),
      account({ UserID: 5, Username: "epsilon" }),
    ];
    const { result } = await renderStore();

    act(() => result.current.setSearchQuery("needle"));

    const ids = result.current.groups.flatMap((g) => g.accounts.map((a) => a.UserID));
    expect(ids.sort()).toEqual([2, 3, 4]);
  });

  it("omits collapsed groups from orderedUserIds", async () => {
    accountsData = [
      account({ UserID: 1, Group: "A" }),
      account({ UserID: 2, Group: "B" }),
    ];
    const { result } = await renderStore();

    expect(result.current.orderedUserIds).toEqual([1, 2]);
    act(() => result.current.toggleGroup("A"));
    expect(result.current.collapsedGroups.has("A")).toBe(true);
    expect(result.current.orderedUserIds).toEqual([2]);
    act(() => result.current.toggleGroup("A"));
    expect(result.current.orderedUserIds).toEqual([1, 2]);
  });
});

describe("selection", () => {
  async function withFive() {
    accountsData = [1, 2, 3, 4, 5].map((id) => account({ UserID: id }));
    return renderStore();
  }

  function mouse(init: Partial<React.MouseEvent> = {}): React.MouseEvent {
    return { altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...init } as React.MouseEvent;
  }

  it("selects a single account on a plain click", async () => {
    const { result } = await withFive();
    act(() => result.current.handleSelect(3, mouse()));
    expect([...result.current.selectedIds]).toEqual([3]);
    expect(result.current.selectedAccount?.UserID).toBe(3);
  });

  it("toggles with ctrl/alt/meta and reports multi selection", async () => {
    const { result } = await withFive();
    act(() => result.current.handleSelect(1, mouse()));
    act(() => result.current.handleSelect(3, mouse({ ctrlKey: true })));
    expect([...result.current.selectedIds].sort()).toEqual([1, 3]);
    // selectedAccount is only set for exactly one selected row
    expect(result.current.selectedAccount).toBeNull();
    expect(result.current.selectedAccounts.map((a) => a.UserID)).toEqual([1, 3]);

    act(() => result.current.handleSelect(3, mouse({ altKey: true })));
    expect([...result.current.selectedIds]).toEqual([1]);
  });

  it("selects a range with shift and extends it when combined with ctrl", async () => {
    const { result } = await withFive();
    act(() => result.current.handleSelect(2, mouse()));
    act(() => result.current.handleSelect(4, mouse({ shiftKey: true })));
    expect([...result.current.selectedIds].sort()).toEqual([2, 3, 4]);

    act(() => result.current.handleSelect(1, mouse({ shiftKey: true, ctrlKey: true })));
    expect([...result.current.selectedIds].sort()).toEqual([1, 2, 3, 4]);
  });

  it("selects all, deselects all and toggles", async () => {
    const { result } = await withFive();
    act(() => result.current.selectAll());
    expect(result.current.selectedIds.size).toBe(5);
    act(() => result.current.deselectAll());
    expect(result.current.selectedIds.size).toBe(0);
    act(() => result.current.toggleSelectAll());
    expect(result.current.selectedIds.size).toBe(5);
    act(() => result.current.toggleSelectAll());
    expect(result.current.selectedIds.size).toBe(0);
  });

  it("selectAll only covers the filtered accounts", async () => {
    accountsData = [
      account({ UserID: 1, Username: "keep-me" }),
      account({ UserID: 2, Username: "other" }),
    ];
    const { result } = await renderStore();
    act(() => result.current.setSearchQuery("keep"));
    act(() => result.current.selectAll());
    expect([...result.current.selectedIds]).toEqual([1]);
  });

  it("navigates the selection with and without shift and clamps at the edges", async () => {
    const { result } = await withFive();
    act(() => result.current.selectSingle(1));
    act(() => result.current.navigateSelection("up", false));
    expect([...result.current.selectedIds]).toEqual([1]);

    act(() => result.current.navigateSelection("down", false));
    expect([...result.current.selectedIds]).toEqual([2]);

    act(() => result.current.navigateSelection("down", true));
    expect([...result.current.selectedIds].sort()).toEqual([2, 3]);

    act(() => result.current.selectSingle(5));
    act(() => result.current.navigateSelection("down", false));
    expect([...result.current.selectedIds]).toEqual([5]);
  });

  it("starts from the first row when nothing was clicked yet", async () => {
    const { result } = await withFive();
    act(() => result.current.navigateSelection("down", false));
    expect([...result.current.selectedIds]).toEqual([2]);
  });
});

describe("joinServer", () => {
  async function setup(general: Record<string, string> = {}) {
    accountsData = [account({ UserID: 1, Alias: "Main" }), account({ UserID: 2 })];
    settingsData = { General: { ...general } };
    return renderStore();
  }

  it("sends the store's place/job/launchData and shuffle flag", async () => {
    const { result } = await setup({
      SavedPlaceId: "606849621",
      SavedJobId: "job-abc",
      SavedLaunchData: "payload",
      ShuffleJobId: "true",
    });

    await act(async () => {
      await result.current.joinServer(1);
    });

    expect(lastArgs("launch_roblox")).toEqual({
      userId: 1,
      placeId: 606849621,
      jobId: "job-abc",
      launchData: "payload",
      followUser: false,
      joinVip: false,
      linkCode: "",
      shuffleJob: true,
    });
  });

  it("prefers the explicit target over store state", async () => {
    const { result } = await setup({ SavedPlaceId: "1", SavedJobId: "stale", SavedLaunchData: "old" });

    await act(async () => {
      await result.current.joinServer(1, { placeId: "222", jobId: "fresh", launchData: "new" });
    });

    expect(lastArgs("launch_roblox")).toMatchObject({
      placeId: 222,
      jobId: "fresh",
      launchData: "new",
    });
  });

  it("falls back to placeId 5315046213 when place is empty or not a number", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.joinServer(1);
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ placeId: 5315046213 });

    await act(async () => {
      await result.current.joinServer(1, { placeId: "not-a-number" });
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ placeId: 5315046213 });

    // parseInt("0") is falsy, so an explicit 0 also falls back
    await act(async () => {
      await result.current.joinServer(1, { placeId: "0" });
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ placeId: 5315046213 });
  });

  it("parses the vip: job prefix into joinVip + linkCode", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.joinServer(1, { jobId: "vip:  abc-123  " });
    });

    expect(lastArgs("launch_roblox")).toMatchObject({
      jobId: "",
      joinVip: true,
      linkCode: "abc-123",
    });
  });

  it("accepts the vip: prefix case-insensitively", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.joinServer(1, { jobId: "VIP:Code42" });
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ joinVip: true, linkCode: "Code42" });
  });

  it("extracts linkCode from a pasted private-server URL and decodes it", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.joinServer(1, {
        jobId: "https://www.roblox.com/games/123/x?privateServerLinkCode=a%2Fb",
      });
    });

    expect(lastArgs("launch_roblox")).toMatchObject({
      jobId: "",
      linkCode: "a/b",
      // NOTE: joinVip stays false for this branch; the backend resolves by link code.
      joinVip: false,
    });
  });

  it("keeps the raw link code when it is not valid percent-encoding", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.joinServer(1, { jobId: "?linkCode=100%bad" });
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ linkCode: "100%bad", jobId: "" });
  });

  it("lets an explicit target.joinVip/linkCode win over the parsed job string", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.joinServer(1, {
        jobId: "plain-job-id",
        joinVip: true,
        linkCode: "resolved-code",
      });
    });

    expect(lastArgs("launch_roblox")).toMatchObject({
      jobId: "",
      joinVip: true,
      linkCode: "resolved-code",
    });
  });

  it("lets target.joinVip=false override a vip: prefix while keeping the parsed code", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.joinServer(1, { jobId: "vip:code", joinVip: false });
    });

    expect(lastArgs("launch_roblox")).toMatchObject({
      joinVip: false,
      linkCode: "code",
      jobId: "",
    });
  });

  it("trims the job id before sending it", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.joinServer(1, { jobId: "   job-xyz   " });
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ jobId: "job-xyz" });
  });

  it("uses an empty target.launchData instead of the store value", async () => {
    const { result } = await setup({ SavedLaunchData: "store-data" });
    await act(async () => {
      await result.current.joinServer(1, { launchData: "" });
    });
    expect(lastArgs("launch_roblox")).toMatchObject({ launchData: "" });
  });

  it("records the launched place as a recent game with the configured cap", async () => {
    const { result } = await setup({ SavedPlaceId: "42", MaxRecentGames: "3" });
    await act(async () => {
      await result.current.joinServer(1);
    });
    expect(recordRecentGameMock).toHaveBeenCalledWith(42, 1, 3);
  });

  it("defaults the recent-games cap to 8 when the setting is missing or invalid", async () => {
    const { result } = await setup({ SavedPlaceId: "42", MaxRecentGames: "zero" });
    await act(async () => {
      await result.current.joinServer(1);
    });
    expect(recordRecentGameMock).toHaveBeenCalledWith(42, 1, 8);
  });

  it("tracks joining state and progress, then clears it after 7s", async () => {
    const { result } = await setup({ SavedPlaceId: "42" });
    vi.useFakeTimers();

    await act(async () => {
      await result.current.joinServer(1);
    });

    expect([...result.current.joiningAccounts]).toEqual([1]);
    expect(result.current.launchProgress).toMatchObject({
      mode: "single",
      current: 1,
      total: 1,
      userId: 1,
    });

    await act(async () => {
      vi.advanceTimersByTime(7000);
    });

    expect(result.current.joiningAccounts.size).toBe(0);
    expect(result.current.launchProgress).toBeNull();
  });

  it("reports a launch failure without throwing and clears the joining state", async () => {
    const { result } = await setup();
    failures.set("launch_roblox", "backend exploded");

    await act(async () => {
      await result.current.joinServer(1);
    });

    expect(result.current.error).toBe("backend exploded");
    expect(result.current.joiningAccounts.size).toBe(0);
    expect(result.current.launchProgress).toBeNull();
    expect(result.current.actionStatus?.tone).toBe("error");
    expect(recordRecentGameMock).not.toHaveBeenCalled();
  });

  it("announces the account alias in the action status while launching", async () => {
    const { result } = await setup();
    let releaseLaunch = () => {};
    results.set(
      "launch_roblox",
      new Promise<null>((resolve) => {
        releaseLaunch = () => resolve(null);
      })
    );

    let pending: Promise<void> | null = null;
    await act(async () => {
      pending = result.current.joinServer(1);
      await Promise.resolve();
    });

    expect(result.current.actionStatus?.message).toContain("Main");

    await act(async () => {
      releaseLaunch();
      await pending;
    });

    // the success toast replaces the per-account message once the launch returns
    expect(result.current.actionStatus?.message).toBe("Launching game...");
  });
});

describe("launchMultiple", () => {
  async function setup(general: Record<string, string> = {}) {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 }), account({ UserID: 3 })];
    settingsData = { General: { ...general } };
    return renderStore();
  }

  it("does nothing for an empty selection", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.launchMultiple([]);
    });
    expect(invokeCalls("launch_multiple")).toHaveLength(0);
  });

  it("sends userIds, place, job and launchData", async () => {
    const { result } = await setup({
      SavedPlaceId: "777",
      SavedJobId: "job-1",
      SavedLaunchData: "data",
    });

    await act(async () => {
      await result.current.launchMultiple([1, 2]);
    });

    expect(lastArgs("launch_multiple")).toEqual({
      userIds: [1, 2],
      placeId: 777,
      jobId: "job-1",
      launchData: "data",
    });
  });

  it("encodes a VIP target as a vip:<code> job id", async () => {
    const { result } = await setup({ SavedPlaceId: "777" });

    await act(async () => {
      await result.current.launchMultiple([1, 2], {
        joinVip: true,
        linkCode: "  code-9  ",
        jobId: "ignored",
      });
    });

    expect(lastArgs("launch_multiple")).toMatchObject({ jobId: "vip:code-9" });
  });

  it("falls back to the job field's own code when joinVip has no link code", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.launchMultiple([1], {
        joinVip: true,
        linkCode: "",
        jobId: "vip:fallback-code",
      });
    });

    expect(lastArgs("launch_multiple")).toMatchObject({ jobId: "vip:fallback-code" });
  });

  it("sends the plain job when joinVip has no code anywhere", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.launchMultiple([1], { joinVip: true, linkCode: "", jobId: "raw-job" });
    });

    // Nothing to join privately with; a plain job is the only sane request.
    expect(lastArgs("launch_multiple")).toMatchObject({ jobId: "raw-job" });
  });

  it("parses a private-server link pasted into the job field, like joinServer", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.launchMultiple([1], { jobId: "?linkCode=abc" });
    });

    expect(lastArgs("launch_multiple")).toMatchObject({ jobId: "vip:abc" });
  });

  it("parses a full private-server URL pasted into the job field", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.launchMultiple([1], {
        jobId: "https://www.roblox.com/games/606849621/X?privateServerLinkCode=99887766",
      });
    });

    expect(lastArgs("launch_multiple")).toMatchObject({ jobId: "vip:99887766" });
  });

  it("keeps a plain job id untouched", async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.launchMultiple([1], { jobId: "abc-123-def" });
    });

    expect(lastArgs("launch_multiple")).toMatchObject({ jobId: "abc-123-def" });
  });

  it("falls back to placeId 5315046213", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.launchMultiple([1]);
    });
    expect(lastArgs("launch_multiple")).toMatchObject({ placeId: 5315046213 });
  });

  it("records the recent game for the first account", async () => {
    const { result } = await setup({ SavedPlaceId: "99", MaxRecentGames: "5" });
    await act(async () => {
      await result.current.launchMultiple([3, 1]);
    });
    expect(recordRecentGameMock).toHaveBeenCalledWith(99, 3, 5);
  });

  it("refuses multi-launch on an unsupported Linux runner and reports the reason", async () => {
    capabilitiesData = caps({
      os: "linux",
      supportsMultiLaunch: false,
      reasons: ["runner incompatible"],
    });
    const { result } = await setup();

    await act(async () => {
      await expect(result.current.launchMultiple([1, 2])).rejects.toThrow("runner incompatible");
    });

    expect(invokeCalls("launch_multiple")).toHaveLength(0);
    expect(result.current.error).toBe("runner incompatible");
  });

  it("still allows a single account on that Linux runner", async () => {
    capabilitiesData = caps({ os: "linux", supportsMultiLaunch: false, reasons: ["nope"] });
    const { result } = await setup();

    await act(async () => {
      await result.current.launchMultiple([1]);
    });

    expect(invokeCalls("launch_multiple")).toHaveLength(1);
  });

  it("rethrows backend failures and clears progress", async () => {
    const { result } = await setup();
    failures.set("launch_multiple", "multi failed");

    await act(async () => {
      await expect(result.current.launchMultiple([1, 2])).rejects.toBeTruthy();
    });

    expect(result.current.error).toBe("multi failed");
    expect(result.current.joiningAccounts.size).toBe(0);
    expect(result.current.launchProgress).toBeNull();
  });
});

describe("restartRobloxClients", () => {
  it("does nothing when no selected account was launched by the app", async () => {
    accountsData = [account({ UserID: 1 })];
    const { result } = await renderStore();

    await act(async () => {
      await result.current.restartRobloxClients([1]);
    });

    expect(invokeCalls("cmd_kill_roblox")).toHaveLength(0);
    expect(result.current.toasts.join(" ")).toMatch(/No launched Roblox clients/i);
  });

  it("closes and relaunches the launched clients", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 })];
    runningInstances = [{ userId: 1 }, { user_id: 2 }];
    const { result } = await renderStore();

    await waitFor(() => expect(result.current.launchedByProgram.size).toBe(2));

    await act(async () => {
      await result.current.restartRobloxClients([1, 2, 2]);
    });

    expect(invokeCalls("cmd_kill_roblox")).toHaveLength(2);
    expect(invokeCalls("launch_multiple")).toHaveLength(1);
    expect(lastArgs("launch_multiple")).toMatchObject({ userIds: [1, 2] });
  });

  it("uses joinServer for a single launched client", async () => {
    accountsData = [account({ UserID: 1 })];
    runningInstances = [{ userId: 1 }];
    const { result } = await renderStore();
    await waitFor(() => expect(result.current.launchedByProgram.size).toBe(1));

    await act(async () => {
      await result.current.restartRobloxClients([1]);
    });

    expect(invokeCalls("launch_roblox")).toHaveLength(1);
  });
});

describe("account mutations", () => {
  it("adds an account by cookie and reports whether it was new", async () => {
    results.set("validate_cookie", { user_id: 7, name: "Cookie" });
    const { result } = await renderStore();

    await act(async () => {
      await result.current.addAccountByCookie("_|WARNING:-token");
    });

    expect(lastArgs("add_account")).toEqual({
      securityToken: "_|WARNING:-token",
      username: "Cookie",
      userId: 7,
    });
    expect(result.current.toasts.join(" ")).toContain("Added Cookie");
  });

  it("says 'Updated' when the account already exists", async () => {
    accountsData = [account({ UserID: 7 })];
    results.set("validate_cookie", { user_id: 7, name: "Cookie" });
    const { result } = await renderStore();

    await act(async () => {
      await result.current.addAccountByCookie("token");
    });

    expect(result.current.toasts.join(" ")).toContain("Updated Cookie");
  });

  it("removes accounts and drops them from the selection", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 })];
    const { result } = await renderStore();
    act(() => result.current.setSelectedIds(new Set([1, 2])));

    accountsData = [account({ UserID: 2 })];
    await act(async () => {
      await result.current.removeAccounts([1]);
    });

    expect(lastArgs("remove_account")).toEqual({ userId: 1 });
    expect([...result.current.selectedIds]).toEqual([2]);
    expect(result.current.accounts.map((a) => a.UserID)).toEqual([2]);
  });

  it("updates an account in place", async () => {
    accountsData = [account({ UserID: 1, Alias: "old" })];
    const { result } = await renderStore();

    await act(async () => {
      await result.current.updateAccount({ ...result.current.accounts[0], Alias: "new" });
    });

    expect(result.current.accounts[0].Alias).toBe("new");
    expect(invokeCalls("update_account")).toHaveLength(1);
  });

  it("moves accounts to a group and persists each one", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 })];
    const { result } = await renderStore();

    await act(async () => {
      await result.current.moveToGroup([1], "10 Bots");
    });

    expect(result.current.accounts[0].Group).toBe("10 Bots");
    expect(invokeCalls("update_account")).toHaveLength(1);
    expect(result.current.toasts.join(" ")).toContain("Bots");
  });

  it("sorts a group alphabetically by alias or username and persists the order", async () => {
    accountsData = [
      account({ UserID: 1, Username: "zeta", Group: "G" }),
      account({ UserID: 2, Username: "alpha", Group: "G" }),
      account({ UserID: 3, Username: "mid", Group: "Other" }),
    ];
    const { result } = await renderStore();

    act(() => result.current.sortGroupAlphabetically("G"));

    expect(result.current.accounts.map((a) => a.UserID)).toEqual([2, 1, 3]);
    expect(lastArgs("reorder_accounts")).toEqual({ userIds: [2, 1, 3] });
  });

  it("leaves a one-account group untouched", async () => {
    accountsData = [account({ UserID: 1, Group: "G" })];
    const { result } = await renderStore();

    act(() => result.current.sortGroupAlphabetically("G"));

    expect(invokeCalls("reorder_accounts")).toHaveLength(0);
  });

  it("reorders accounts by drag and drop", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 }), account({ UserID: 3 })];
    const { result } = await renderStore();

    await act(async () => {
      await result.current.reorderAccounts(3, 1);
    });

    expect(result.current.accounts.map((a) => a.UserID)).toEqual([3, 1, 2]);
    expect(lastArgs("reorder_accounts")).toEqual({ userIds: [3, 1, 2] });
  });

  it("ignores a reorder onto itself or onto an unknown account", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 })];
    const { result } = await renderStore();

    await act(async () => {
      await result.current.reorderAccounts(1, 1);
      await result.current.reorderAccounts(1, 99);
    });

    expect(invokeCalls("reorder_accounts")).toHaveLength(0);
  });
});

describe("saved launch fields", () => {
  it("persists placeId, jobId and launchData as they change", async () => {
    const { result } = await renderStore();

    act(() => result.current.setPlaceId("123"));
    act(() => result.current.setJobId("job"));
    act(() => result.current.setLaunchData("data"));
    act(() => result.current.setHideUsernames(true));

    const saved = invokeCalls("update_setting").map((c) => c[1] as Record<string, string>);
    expect(saved).toEqual(
      expect.arrayContaining([
        { section: "General", key: "SavedPlaceId", value: "123" },
        { section: "General", key: "SavedJobId", value: "job" },
        { section: "General", key: "SavedLaunchData", value: "data" },
        { section: "General", key: "HideUsernames", value: "true" },
      ])
    );
    expect(result.current.placeId).toBe("123");
  });
});

describe("toasts and action status", () => {
  it("derives an error tone from the message and clears after the timeout", async () => {
    const { result } = await renderStore();
    vi.useFakeTimers();

    act(() => result.current.addToast("Something failed"));
    expect(result.current.toasts).toEqual(["Something failed"]);
    expect(result.current.actionStatus).toMatchObject({ tone: "error" });

    await act(async () => {
      vi.advanceTimersByTime(2500);
    });
    expect(result.current.toasts).toEqual([]);

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(result.current.actionStatus).toBeNull();
  });

  it("uses a success tone for saved/updated/launched and a warn tone for warnings", async () => {
    const { result } = await renderStore();

    act(() => result.current.addToast("Accounts saved"));
    expect(result.current.actionStatus?.tone).toBe("success");

    act(() => result.current.addToast("Some warning here"));
    expect(result.current.actionStatus?.tone).toBe("warn");

    act(() => result.current.addToast("Neutral message"));
    expect(result.current.actionStatus?.tone).toBe("info");
  });

  it("reacts to the ram-action-status window event", async () => {
    const { result } = await renderStore();

    act(() => {
      window.dispatchEvent(
        new CustomEvent("ram-action-status", { detail: { message: "Settings saved" } })
      );
    });

    expect(result.current.actionStatus).toMatchObject({ message: "Settings saved", tone: "success" });
  });

  it("ignores a ram-action-status event without a message", async () => {
    const { result } = await renderStore();
    act(() => {
      window.dispatchEvent(new CustomEvent("ram-action-status", { detail: {} }));
    });
    expect(result.current.actionStatus).toBeNull();
  });

  it("opens and closes the modal", async () => {
    const { result } = await renderStore();
    act(() => result.current.showModal("Title", "Body"));
    expect(result.current.modal).toEqual({ title: "Title", content: "Body" });
    act(() => result.current.closeModal());
    expect(result.current.modal).toBeNull();
  });

  it("opens and closes the context menu", async () => {
    const { result } = await renderStore();
    act(() => result.current.openContextMenu(12, 34));
    expect(result.current.contextMenu).toEqual({ x: 12, y: 34 });
    act(() => result.current.closeContextMenu());
    expect(result.current.contextMenu).toBeNull();
  });
});

describe("backend events", () => {
  it("appends launch logs and caps the buffer at 500 entries", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("launch-log")).toBe(true));

    act(() => {
      emit("launch-log", { userId: 1, level: "warn", step: "prep", message: "hello" });
    });
    expect(result.current.launchLogs).toHaveLength(1);
    expect(result.current.launchLogs[0]).toMatchObject({
      userId: 1,
      level: "warn",
      step: "prep",
      message: "hello",
    });

    act(() => {
      for (let i = 0; i < 520; i++) emit("launch-log", { message: `m${i}` });
    });
    expect(result.current.launchLogs).toHaveLength(500);
    expect(result.current.launchLogs[499].message).toBe("m519");
    // defaults for a payload without level/step/userId
    expect(result.current.launchLogs[499]).toMatchObject({ level: "info", step: "", userId: null });

    act(() => result.current.clearLaunchLogs());
    expect(result.current.launchLogs).toEqual([]);
  });

  it("tracks multi-launch progress and completion", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("launch-progress")).toBe(true));

    act(() => emit("launch-progress", { userId: 5, index: 1, total: 3 }));
    expect(result.current.launchProgress).toEqual({
      mode: "multi",
      current: 2,
      total: 3,
      userId: 5,
    });
    expect([...result.current.joiningAccounts]).toEqual([5]);

    act(() => emit("launch-complete", {}));
    expect(result.current.joiningAccounts.size).toBe(0);
    expect(result.current.launchProgress).toMatchObject({ current: 3, total: 3 });
    expect(result.current.actionStatus?.tone).toBe("success");
  });

  it("clamps the reported progress index to the total", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("launch-progress")).toBe(true));
    act(() => emit("launch-progress", { userId: 1, index: 9, total: 2 }));
    expect(result.current.launchProgress).toMatchObject({ current: 2, total: 2 });
  });

  it("reloads accounts and warns when an account gets moderated", async () => {
    accountsData = [account({ UserID: 1, Alias: "Alpha" })];
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("account-moderated")).toBe(true));

    await act(async () => {
      emit("account-moderated", { userId: 1 });
    });

    expect(result.current.toasts.join(" ")).toContain("Alpha");
    expect(result.current.toasts.join(" ")).toContain("moderadas");
  });

  it("surfaces roblox build install progress", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("roblox-build-install")).toBe(true));

    act(() => emit("roblox-build-install", { stage: "starting", current: 0, total: 0 }));
    expect(result.current.actionStatus?.message).toMatch(/Downloading the new Roblox version/);

    act(() => emit("roblox-build-install", { stage: "ready", current: 1, total: 1 }));
    expect(result.current.actionStatus).toMatchObject({ tone: "success" });

    act(() => emit("roblox-build-install", { stage: "error", current: 0, total: 0, message: "nope" }));
    expect(result.current.toasts.join(" ")).toContain("nope");
  });

  it("turns a chromium download event into a percentage status", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("chromium-download-progress")).toBe(true));

    act(() => emit("chromium-download-progress", { stage: "downloading", downloaded: 50, total: 200 }));
    expect(result.current.actionStatus?.message).toContain("25");

    act(() => emit("chromium-download-progress", { stage: "ready", downloaded: 0, total: 0 }));
    expect(result.current.actionStatus).toMatchObject({ tone: "success" });
  });

  it("marks botting as inactive when the backend stops it without a prior status", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("botting-stopped")).toBe(true));

    act(() => emit("botting-stopped", {}));

    expect(result.current.bottingStatus).toMatchObject({
      active: false,
      intervalMinutes: 19,
      launchDelaySeconds: 20,
      playerGraceMinutes: 15,
    });
  });

  it("stores botting and generator status payloads", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("botting-status")).toBe(true));

    act(() => emit("botting-status", { active: true, userIds: [1] }));
    expect(result.current.bottingStatus).toMatchObject({ active: true });

    act(() => emit("generator-status", { active: true, totalGenerated: 3 }));
    expect(result.current.generatorStatus).toMatchObject({ totalGenerated: 3 });

    act(() => emit("generator-stopped", {}));
    expect(result.current.generatorStatus).toMatchObject({ active: false });
  });

  it("warns when a botting rejoin cycle fails", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("botting-account-cycle")).toBe(true));

    act(() => emit("botting-account-cycle", { userId: 4, ok: false, error: "timeout" }));
    expect(result.current.actionStatus).toMatchObject({ tone: "warn" });
    expect(result.current.actionStatus?.message).toContain("timeout");

    const before = result.current.actionStatus;
    act(() => emit("botting-account-cycle", { userId: 4, ok: true }));
    expect(result.current.actionStatus).toBe(before);
  });

  it("formats optimization warnings with and without a pid", async () => {
    const { result } = await renderStore();
    await waitFor(() => expect(listenHandlers.has("roblox-optimization-warning")).toBe(true));

    act(() => emit("roblox-optimization-warning", { pid: 4242, message: "high cpu" }));
    expect(result.current.toasts.join(" ")).toContain("4242");

    act(() => emit("roblox-optimization-warning", { message: "  " }));
    expect(result.current.toasts.join(" ")).toContain("Unknown");
  });
});

describe("presence and running instances", () => {
  it("stays empty while ShowPresence is off", async () => {
    accountsData = [account({ UserID: 1 })];
    presenceRows = [{ userId: 1, userPresenceType: 2 }];
    const { result } = await renderStore();

    expect(invokeCalls("get_presence")).toHaveLength(0);
    expect(result.current.presenceByUserId.size).toBe(0);
  });

  it("maps camelCase and snake_case presence rows", async () => {
    accountsData = [account({ UserID: 1 }), account({ UserID: 2 }), account({ UserID: 3 })];
    settingsData = { General: { ShowPresence: "true" } };
    presenceRows = [
      { userId: 1, userPresenceType: 2 },
      { user_id: 2, user_presence_type: 1 },
      { userId: 3 },
    ];
    const { result } = await renderStore();

    await waitFor(() => expect(result.current.presenceByUserId.size).toBe(3));
    expect(result.current.presenceByUserId.get(1)).toBe(2);
    expect(result.current.presenceByUserId.get(2)).toBe(1);
    expect(result.current.presenceByUserId.get(3)).toBe(0);
  });

  it("tracks program-launched clients from both key spellings", async () => {
    accountsData = [account({ UserID: 1 })];
    runningInstances = [{ userId: 1 }, { user_id: 2 }, { pid: 3 }];
    const { result } = await renderStore();

    await waitFor(() => expect(result.current.launchedByProgram.size).toBe(2));
    expect([...result.current.launchedByProgram].sort()).toEqual([1, 2]);
  });
});

describe("botting and generator commands", () => {
  it("maps the botting start config onto the backend arguments", async () => {
    const { result } = await renderStore();
    results.set("start_botting_mode", { active: true });

    await act(async () => {
      await result.current.startBottingMode({
        userIds: [1, 2],
        placeId: 5,
        jobId: "job",
        launchData: "ld",
        playerUserIds: [1],
        intervalMinutes: 10,
        launchDelaySeconds: 20,
        playerGraceMinutes: 30,
      });
    });

    expect(lastArgs("start_botting_mode")).toEqual({
      userIds: [1, 2],
      placeId: 5,
      jobId: "job",
      launchData: "ld",
      playerUserIds: [1],
      intervalMinutes: 10,
      launchDelaySeconds: 20,
      playerGraceMinutes: 30,
    });
    expect(result.current.bottingStatus).toMatchObject({ active: true });
  });

  it("refuses to start botting on an unsupported Linux runner", async () => {
    capabilitiesData = caps({ os: "linux", supportsBotting: false, warnings: ["no botting"] });
    const { result } = await renderStore();

    await act(async () => {
      await expect(
        result.current.startBottingMode({
          userIds: [1],
          placeId: 1,
          jobId: "",
          launchData: "",
          playerUserIds: [],
          intervalMinutes: 1,
          launchDelaySeconds: 1,
          playerGraceMinutes: 1,
        })
      ).rejects.toThrow("no botting");
    });
    expect(invokeCalls("start_botting_mode")).toHaveLength(0);
  });

  it("stops botting and refreshes the status", async () => {
    const { result } = await renderStore();
    await act(async () => {
      await result.current.stopBottingMode(true);
    });
    expect(lastArgs("stop_botting_mode")).toEqual({ closeBotAccounts: true });
    expect(result.current.toasts.join(" ")).toMatch(/bot accounts closed/i);
  });

  it("adds botting accounts and ignores an empty list", async () => {
    const { result } = await renderStore();
    await act(async () => {
      await result.current.addBottingAccounts([]);
    });
    expect(invokeCalls("add_botting_accounts")).toHaveLength(0);

    await act(async () => {
      await result.current.addBottingAccounts([1, 2]);
    });
    expect(lastArgs("add_botting_accounts")).toEqual({ userIds: [1, 2] });
  });

  it("sets the player accounts and reports cleared vs updated", async () => {
    const { result } = await renderStore();

    await act(async () => {
      await result.current.setBottingPlayerAccounts([]);
    });
    expect(lastArgs("set_botting_player_accounts")).toEqual({ playerUserIds: [] });
    expect(result.current.toasts.join(" ")).toMatch(/cleared/i);

    await act(async () => {
      await result.current.setBottingPlayerAccounts([1]);
    });
    expect(result.current.toasts.join(" ")).toMatch(/updated/i);
  });

  it("forwards per-account botting actions", async () => {
    const { result } = await renderStore();
    await act(async () => {
      await result.current.bottingAccountAction(3, "restartLoop");
    });
    expect(lastArgs("botting_account_action")).toEqual({ userId: 3, action: "restartLoop" });
  });

  it("starts and stops the account generator", async () => {
    results.set("start_generator", { active: true, totalGenerated: 0 });
    const { result } = await renderStore();

    await act(async () => {
      await result.current.startGenerator({
        provider: "p",
        endpoint: "e",
        apiKey: "secret",
        accountType: "t",
        extraDelaySeconds: 2,
        targetGroup: "g",
        maxAccounts: 4,
      });
    });

    expect(lastArgs("start_generator")).toEqual({
      provider: "p",
      endpoint: "e",
      apiKey: "secret",
      accountType: "t",
      extraDelaySeconds: 2,
      targetGroup: "g",
      maxAccounts: 4,
    });

    await act(async () => {
      await result.current.stopGenerator();
    });
    expect(invokeCalls("stop_generator")).toHaveLength(1);
  });

  it("propagates generator failures", async () => {
    failures.set("start_generator", "gen boom");
    const { result } = await renderStore();

    await act(async () => {
      await expect(
        result.current.startGenerator({
          provider: "p",
          endpoint: "",
          apiKey: "",
          accountType: "",
          extraDelaySeconds: 0,
          targetGroup: "",
          maxAccounts: 1,
        })
      ).rejects.toBeTruthy();
    });
    expect(result.current.error).toBe("gen boom");
  });
});

describe("process control", () => {
  it("reports how many Roblox processes were closed", async () => {
    results.set("cmd_kill_all_roblox", 2);
    const { result } = await renderStore();

    await act(async () => {
      await result.current.killAllRobloxProcesses();
    });

    expect(result.current.toasts.join(" ")).toContain("2");
    expect(result.current.error).toBeNull();
  });

  it("reports when nothing was open", async () => {
    results.set("cmd_kill_all_roblox", 0);
    const { result } = await renderStore();

    await act(async () => {
      await result.current.killAllRobloxProcesses();
    });

    expect(result.current.toasts.join(" ")).toMatch(/No open Roblox processes/i);
  });

  it("surfaces focus failures", async () => {
    failures.set("focus_roblox_window", "no window");
    const { result } = await renderStore();

    await act(async () => {
      await expect(result.current.focusRobloxClient(1)).rejects.toBeTruthy();
    });
    expect(result.current.error).toBe("no window");
  });
});

describe("updates", () => {
  it("skips the automatic check when updates are disabled", async () => {
    settingsData = { General: { CheckForUpdates: "false" } };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.checkForUpdates();
    });

    expect(invokeCalls("check_for_updates_with_channels")).toHaveLength(0);
  });

  it("still runs a manual check with normalized channels", async () => {
    settingsData = {
      General: {
        CheckForUpdates: "false",
        UpdaterReleaseChannel: "STABLE",
        UpdaterFeatureChannel: "nexus",
      },
    };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.checkForUpdates(true);
    });

    expect(lastArgs("check_for_updates_with_channels")).toEqual({
      releaseChannel: "stable",
      featureChannel: "nexus-ws",
    });
    expect(result.current.toasts.join(" ")).toMatch(/No updates available/i);
  });

  it("opens the update dialog when an update is returned", async () => {
    updateResult = {
      version: "4.3.0",
      currentVersion: "4.2.0",
      date: "",
      body: "",
      releaseChannel: "garbage",
      featureChannel: "full",
    };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.checkForUpdates(true);
    });

    expect(result.current.updateDialogOpen).toBe(true);
    expect(result.current.updateInfo).toMatchObject({
      version: "4.3.0",
      releaseChannel: "beta",
      featureChannel: "nexus-ws",
    });
  });

  it("honours a skipped version for automatic checks only", async () => {
    updateResult = {
      version: "4.3.0",
      currentVersion: "4.2.0",
      date: "",
      body: "",
      releaseChannel: "beta",
      featureChannel: "standard",
    };
    localStorage.setItem("skipped-update-version:beta:standard", "4.3.0");
    const { result } = await renderStore();

    await act(async () => {
      await result.current.checkForUpdates();
    });
    expect(result.current.updateDialogOpen).toBe(false);

    await act(async () => {
      await result.current.checkForUpdates(true);
    });
    expect(result.current.updateDialogOpen).toBe(true);
  });

  it("reports a failed manual check", async () => {
    failures.set("check_for_updates_with_channels", "network down");
    const { result } = await renderStore();

    await act(async () => {
      await result.current.checkForUpdates(true);
    });

    expect(result.current.toasts.join(" ")).toMatch(/Update check failed/i);
  });

  it("fills a preview release for the update dialog", async () => {
    const { result } = await renderStore();
    act(() => result.current.openUpdatePreviewDialog());
    expect(result.current.updateDialogOpen).toBe(true);
    expect(result.current.updateInfo?.version).toBe("4.2.6-beta");
  });
});

describe("versions, encryption and walkthrough", () => {
  it("applies the default version optimistically", async () => {
    settingsData = { Versions: { DefaultVersion: "old" } };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.setDefaultVersion("new");
    });

    expect(lastArgs("versions_set_default")).toEqual({ versionId: "new" });
    expect(result.current.settings?.Versions?.DefaultVersion).toBe("new");
  });

  it("rolls the default version back when the backend refuses", async () => {
    settingsData = { Versions: { DefaultVersion: "old" } };
    failures.set("versions_set_default", "nope");
    const { result } = await renderStore();

    await act(async () => {
      await result.current.setDefaultVersion("new");
    });

    expect(result.current.settings?.Versions?.DefaultVersion).toBe("old");
    expect(result.current.toasts.join(" ")).toMatch(/Failed to set version/i);
  });

  it("stores an empty string when clearing the default version", async () => {
    settingsData = { Versions: { DefaultVersion: "old" } };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.setDefaultVersion(null);
    });

    expect(result.current.settings?.Versions?.DefaultVersion).toBe("");
    expect(lastArgs("versions_set_default")).toEqual({ versionId: null });
  });

  it("applies password encryption and persists the onboarding state", async () => {
    const { result } = await renderStore();

    await act(async () => {
      await result.current.applyEncryptionMethod("password", "hunter2");
    });

    expect(lastArgs("set_encryption_password")).toEqual({ password: "hunter2" });
    const settingWrites = invokeCalls("update_setting").map((c) => c[1] as Record<string, string>);
    expect(settingWrites).toEqual(
      expect.arrayContaining([
        { section: "General", key: "EncryptionOnboardingState", value: "completed" },
        { section: "General", key: "EncryptionMethod", value: "password" },
      ])
    );
    expect(result.current.encryptionSetupOpen).toBe(false);
    expect(result.current.applyingEncryption).toBe(false);
  });

  it("sends a null password for default encryption", async () => {
    const { result } = await renderStore();
    await act(async () => {
      await result.current.applyEncryptionMethod("default");
    });
    expect(lastArgs("set_encryption_password")).toEqual({ password: null });
  });

  it("keeps the dialog open and records the error when encryption fails", async () => {
    failures.set("set_encryption_password", "bad password");
    const { result } = await renderStore();

    act(() => result.current.openEncryptionSetupFromSettings());
    await act(async () => {
      await expect(result.current.applyEncryptionMethod("password", "x")).rejects.toBeTruthy();
    });

    expect(result.current.encryptionSetupError).toBe("bad password");
    expect(result.current.encryptionSetupOpen).toBe(true);
  });

  it("unlocks with a password and reloads the accounts", async () => {
    needsPasswordValue = true;
    const { result } = await renderStore();
    expect(result.current.needsPassword).toBe(true);

    accountsData = [account({ UserID: 1 })];
    await act(async () => {
      await result.current.unlock("secret");
    });

    expect(lastArgs("unlock_accounts")).toEqual({ password: "secret" });
    expect(result.current.needsPassword).toBe(false);
    expect(result.current.accounts).toHaveLength(1);
    expect(result.current.unlocking).toBe(false);
  });

  it("keeps the lock screen up on a wrong password", async () => {
    needsPasswordValue = true;
    failures.set("unlock_accounts", "wrong password");
    const { result } = await renderStore();

    await act(async () => {
      await result.current.unlock("nope");
    });

    expect(result.current.needsPassword).toBe(true);
    expect(result.current.error).toBe("wrong password");
  });

  it("persists the walkthrough state only once it was pending", async () => {
    settingsData = { General: { FirstRunWalkthroughState: "completed" } };
    const { result } = await renderStore();

    act(() => result.current.openFirstRunWalkthroughFromSettings());
    await waitFor(() => expect(result.current.firstRunWalkthroughOpen).toBe(true));

    await act(async () => {
      await result.current.completeFirstRunWalkthrough();
    });

    expect(result.current.firstRunWalkthroughOpen).toBe(false);
    const writes = invokeCalls("update_setting").map((c) => c[1] as Record<string, string>);
    expect(writes.some((w) => w.key === "FirstRunWalkthroughState")).toBe(false);
  });

  it("persists completion for a pending first run", async () => {
    settingsData = {
      General: { FirstRunWalkthroughState: "pending", EncryptionOnboardingState: "completed" },
    };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.completeFirstRunWalkthrough();
    });

    const writes = invokeCalls("update_setting").map((c) => c[1] as Record<string, string>);
    expect(writes).toEqual(
      expect.arrayContaining([
        { section: "General", key: "FirstRunWalkthroughState", value: "completed" },
      ])
    );
    expect(result.current.settings?.General?.FirstRunWalkthroughState).toBe("completed");
  });

  it("persists a skip for a pending first run", async () => {
    settingsData = {
      General: { FirstRunWalkthroughState: "pending", EncryptionOnboardingState: "completed" },
    };
    const { result } = await renderStore();

    await act(async () => {
      await result.current.skipFirstRunWalkthrough();
    });

    const writes = invokeCalls("update_setting").map((c) => c[1] as Record<string, string>);
    expect(writes).toEqual(
      expect.arrayContaining([
        { section: "General", key: "FirstRunWalkthroughState", value: "skipped" },
      ])
    );
  });

  it("closes the walkthrough without persisting anything", async () => {
    settingsData = { General: { FirstRunWalkthroughState: "skipped" } };
    const { result } = await renderStore();

    act(() => result.current.openFirstRunWalkthroughFromSettings());
    await waitFor(() => expect(result.current.firstRunWalkthroughOpen).toBe(true));
    act(() => result.current.closeFirstRunWalkthrough());

    expect(result.current.firstRunWalkthroughOpen).toBe(false);
    expect(result.current.firstRunWalkthroughMode).toBe("manual");
  });
});

describe("theme and settings reload", () => {
  it("normalizes a theme preview and writes CSS variables", async () => {
    const { result } = await renderStore();

    act(() =>
      result.current.applyThemePreview({
        ...(result.current.theme as StoreValue["theme"])!,
        accounts_background: "not-a-color",
        buttons_background: "#123456",
      })
    );

    expect(result.current.theme?.buttons_background).toBe("#123456");
    // invalid colors fall back to the default theme value
    expect(result.current.theme?.accounts_background).toBe("#09090B");
    expect(document.documentElement.style.getPropertyValue("--buttons-bg")).toBe("#123456");
  });

  it("saves a normalized theme to the backend", async () => {
    const { result } = await renderStore();

    await act(async () => {
      await result.current.saveTheme({
        ...(result.current.theme as StoreValue["theme"])!,
        button_style: "Popup",
      });
    });

    const args = lastArgs("update_theme") as { theme: Record<string, unknown> };
    expect(args.theme.button_style).toBe("Popup");
    expect(result.current.theme?.button_style).toBe("Popup");
  });

  it("reloads settings and platform capabilities on demand", async () => {
    const { result } = await renderStore();
    settingsData = { General: { Language: "de-DE" } };

    await act(async () => {
      await result.current.reloadSettings();
    });

    expect(result.current.settings?.General?.Language).toBe("de-DE");
  });

  it("themes the titlebar from the forms colors for a light top bar", async () => {
    settingsData = { General: { ThemeWindowsNavbar: "false" } };
    results.set("get_theme", { forms_background: "#123456", dark_top_bar: false });
    await renderStore();
    expect(document.documentElement.style.getPropertyValue("--titlebar-bg")).toBe("#123456");
  });

  it("uses the dark titlebar colors when dark_top_bar is set", async () => {
    settingsData = { General: { ThemeWindowsNavbar: "false" } };
    results.set("get_theme", { forms_background: "#123456", dark_top_bar: true });
    await renderStore();
    expect(document.documentElement.style.getPropertyValue("--titlebar-bg")).toBe("#09090b");
    expect(document.documentElement.style.getPropertyValue("--titlebar-fg")).toBe("#a1a1aa");
  });

  it("uses the theme colors for the titlebar when the navbar option is on", async () => {
    settingsData = { General: { ThemeWindowsNavbar: "true" } };
    results.set("get_theme", { forms_background: "#123456", forms_foreground: "#abcdef", dark_top_bar: true });
    await renderStore();
    expect(document.documentElement.style.getPropertyValue("--titlebar-bg")).toBe("#123456");
    expect(document.documentElement.style.getPropertyValue("--titlebar-fg")).toBe("#abcdef");
  });
});

describe("browser helpers", () => {
  it("opens the login and per-account browsers", async () => {
    const { result } = await renderStore();

    await act(async () => {
      await result.current.openLoginBrowser();
      await result.current.openAccountBrowser(9);
    });

    expect(invokeCalls("open_login_browser")).toHaveLength(1);
    expect(lastArgs("open_account_browser")).toEqual({ userId: 9 });
  });

  it("records an error when the login browser cannot open", async () => {
    failures.set("open_login_browser", "no chromium");
    const { result } = await renderStore();

    await act(async () => {
      await result.current.openLoginBrowser();
    });

    expect(result.current.error).toBe("no chromium");
  });

  it("refreshes a cookie and reloads the accounts", async () => {
    results.set("refresh_cookie", true);
    const { result } = await renderStore();

    let ok = false;
    await act(async () => {
      ok = await result.current.refreshCookie(3);
    });

    expect(ok).toBe(true);
    expect(lastArgs("refresh_cookie")).toEqual({ userId: 3 });
  });
});
