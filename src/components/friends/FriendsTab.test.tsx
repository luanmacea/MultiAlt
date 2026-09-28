import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { FriendsTab } from "./FriendsTab";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import {
  emitTauriEvent,
  invokeMock,
  resetTauriMocks,
  setInvokeHandler,
  setInvokeMap,
} from "../../test-utils/tauriMocks";
import type { Account, AccountFriends, OnlineFriend, ThumbnailData } from "../../types";
import type { StoreValue } from "../../store";

const ACCOUNT_A = makeAccount({ UserID: 1001, Username: "alpha" });
const ACCOUNT_B = makeAccount({ UserID: 1002, Username: "bravo", Alias: "Bravo Alt" });

function friend(overrides: Partial<OnlineFriend> = {}): OnlineFriend {
  return {
    userId: 7001,
    name: "friendo",
    displayName: "Friendo",
    presenceType: 2,
    lastLocation: "Natural Disaster Survival",
    placeId: 189707,
    rootPlaceId: 189707,
    gameId: "job-a",
    ...overrides,
  };
}

function group(userId: number, friends: OnlineFriend[], error: string | null = null): AccountFriends {
  return { userId, friends, error };
}

/**
 * Dublê de `useLauncher().launchAll`. A aba **não pode** ter caminho de launch
 * próprio, então o contrato que estes testes travam é: um único `launchAll`
 * com todas as contas e o alvo já resolvido.
 */
const launchAll = vi.fn(
  async (
    _userIds: number[],
    _placeId: number,
    _jobId?: string,
    _onStarted?: () => void
  ): Promise<{ ok: boolean; error?: string }> => ({ ok: true })
);

/** Rota padrão: amigos vindos do backend + avatares em lote. */
function setFriends(rows: AccountFriends[], avatars: ThumbnailData[] = []) {
  setInvokeMap({
    get_online_friends_for_accounts: rows,
    batched_get_avatar_headshots: avatars,
  });
}

function renderTab(selected: Account[] = [ACCOUNT_A, ACCOUNT_B], overrides: Partial<StoreValue> = {}) {
  const userIds = selected.map((a) => a.UserID);
  return renderWithStore(<FriendsTab userIds={userIds} launchAll={launchAll} />, {
    accounts: [ACCOUNT_A, ACCOUNT_B],
    selectedIds: new Set(userIds),
    selectedAccounts: selected,
    ...overrides,
  });
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((call) => call[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  launchAll.mockReset();
  launchAll.mockResolvedValue({ ok: true });
});

afterEach(cleanup);

describe("FriendsTab — grouping", () => {
  it("lists the online friends of each selected account under that account", async () => {
    setFriends([
      group(1001, [friend({ userId: 7001, displayName: "Friendo" })]),
      group(1002, [friend({ userId: 7002, displayName: "Buddy" })]),
    ]);
    renderTab();

    const first = await screen.findByTestId("friends-group-1001");
    const second = screen.getByTestId("friends-group-1002");

    expect(within(first).getByText("alpha")).toBeInTheDocument();
    expect(within(first).getByText("Friendo")).toBeInTheDocument();
    expect(within(first).queryByText("Buddy")).not.toBeInTheDocument();

    expect(within(second).getByText("Bravo Alt")).toBeInTheDocument();
    expect(within(second).getByText("Buddy")).toBeInTheDocument();
  });

  it("requests the friends of every selected account in one batched call", async () => {
    setFriends([group(1001, []), group(1002, [])]);
    renderTab();

    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));
    const args = (callsFor("get_online_friends_for_accounts")[0][1] ?? {}) as Record<string, unknown>;
    expect(args.userIds).toEqual([1001, 1002]);
    expect(typeof args.delayMs).toBe("number");
  });

  it("masks the user's own account names but never the friends' names", async () => {
    setFriends([group(1002, [friend({ userId: 7002, displayName: "Buddy" })])]);
    renderTab([ACCOUNT_B], { hideUsernames: true, hiddenNameLetters: 2 });

    const block = await screen.findByTestId("friends-group-1002");
    expect(within(block).queryByText("Bravo Alt")).not.toBeInTheDocument();
    expect(within(block).getByText("Br********")).toBeInTheDocument();
    // Nome de terceiro fica legível: o usuário precisa reconhecer o amigo.
    expect(within(block).getByText("Buddy")).toBeInTheDocument();
  });

  it("loads the friends' avatars through the batched thumbnail command", async () => {
    setFriends(
      [group(1001, [friend({ userId: 7001 }), friend({ userId: 7002 })])],
      [{ targetId: 7001, imageUrl: "https://img/7001.png", state: "Completed" }]
    );
    renderTab([ACCOUNT_A]);

    await waitFor(() => expect(callsFor("batched_get_avatar_headshots")).toHaveLength(1));
    expect(callsFor("batched_get_avatar_headshots")[0][1]).toMatchObject({
      userIds: [7001, 7002],
      size: "48x48",
    });
  });
});

describe("FriendsTab — joinable vs. not joinable", () => {
  it("makes a friend in a visible server clickable", async () => {
    setFriends([group(1001, [friend({ userId: 7001 })])]);
    renderTab([ACCOUNT_A]);

    const row = await screen.findByTestId("friend-1001-7001");
    expect(within(row).getByRole("button")).toBeEnabled();
    expect(within(row).getByText("Natural Disaster Survival")).toBeInTheDocument();
  });

  it.each([
    [
      "online on the website",
      { presenceType: 1, gameId: null, placeId: null, rootPlaceId: null },
      "On the website",
    ],
    ["in Studio", { presenceType: 3, gameId: null, placeId: null, rootPlaceId: null }, "In Studio"],
    ["in a hidden server", { presenceType: 2, gameId: null }, "Server not visible"],
    [
      "in a game with no place id",
      { presenceType: 2, gameId: "job-a", placeId: null, rootPlaceId: null },
      "Server not visible",
    ],
  ])("dims a friend %s and shows the reason", async (_case, overrides, reason) => {
    setFriends([group(1001, [friend({ userId: 7001, ...overrides })])]);
    renderTab([ACCOUNT_A]);

    const row = await screen.findByTestId("friend-1001-7001");
    expect(within(row).queryByRole("button")).not.toBeInTheDocument();
    expect(within(row).getAllByText(reason).length).toBeGreaterThan(0);
    expect(row.querySelector(".opacity-50")).not.toBeNull();
  });

  it("keeps joinable and non-joinable friends in the same list", async () => {
    setFriends([
      group(1001, [
        friend({ userId: 7001, displayName: "Joinable" }),
        friend({ userId: 7002, displayName: "Website", presenceType: 1, gameId: null }),
      ]),
    ]);
    renderTab([ACCOUNT_A]);

    expect(await screen.findByText("Joinable")).toBeInTheDocument();
    expect(screen.getByText("Website")).toBeInTheDocument();
  });
});

describe("FriendsTab — joining", () => {
  it("sends every selected account to the friend's server in a single launch call", async () => {
    setFriends([group(1001, [friend({ userId: 7001 })]), group(1002, [])]);
    renderTab();

    await userEvent.click(within(await screen.findByTestId("friend-1001-7001")).getByRole("button"));

    await waitFor(() => expect(launchAll).toHaveBeenCalledTimes(1));
    expect(launchAll).toHaveBeenCalledWith([1001, 1002], 189707, "job-a", undefined);
    // Nunca um laço próprio de launch: o piso anti-captcha vive no backend.
    expect(callsFor("launch_roblox")).toHaveLength(0);
  });

  /**
   * O Job ID é de um servidor **do place em que o amigo está**. Num jogo com
   * sub-places (Life Sentence tem 6: o raiz distribui para "VC Only", "Pro
   * Players"...), mandar o raiz com o Job ID do sub-place pede um servidor que
   * não existe ali — o cliente abria em "This experience has ended, or the
   * server became unavailable" (relato do dono, 28/09/2026).
   */
  it("joins the place the friend's server is in, not the root place", async () => {
    setFriends([group(1001, [friend({ userId: 7001, placeId: 111, rootPlaceId: 999, gameId: "job-b" })])]);
    renderTab([ACCOUNT_A]);

    await userEvent.click(within(await screen.findByTestId("friend-1001-7001")).getByRole("button"));

    await waitFor(() => expect(launchAll).toHaveBeenCalledWith([1001], 111, "job-b", undefined));
  });

  it("falls back to rootPlaceId when placeId is missing", async () => {
    setFriends([group(1001, [friend({ userId: 7001, placeId: null, rootPlaceId: 999, gameId: "job-c" })])]);
    renderTab([ACCOUNT_A]);

    await userEvent.click(within(await screen.findByTestId("friend-1001-7001")).getByRole("button"));

    await waitFor(() => expect(launchAll).toHaveBeenCalledWith([1001], 999, "job-c", undefined));
  });

  it("does not launch twice while a join is in flight", async () => {
    let release: (value: { ok: boolean }) => void = () => {};
    launchAll.mockImplementation(
      () => new Promise<{ ok: boolean }>((resolve) => { release = resolve; })
    );
    setFriends([group(1001, [friend({ userId: 7001 })])]);
    renderTab([ACCOUNT_A]);

    const button = within(await screen.findByTestId("friend-1001-7001")).getByRole("button");
    await userEvent.click(button);
    await waitFor(() => expect(button).toBeDisabled());
    expect(launchAll).toHaveBeenCalledTimes(1);

    await act(async () => release({ ok: true }));
    await waitFor(() => expect(button).toBeEnabled());
  });
});

describe("FriendsTab — loading, errors and reload", () => {
  it("shows the per-account progress while the backend walks the accounts", async () => {
    let resolveRows: (rows: AccountFriends[]) => void = () => {};
    setInvokeHandler((cmd) => {
      if (cmd === "get_online_friends_for_accounts") {
        return new Promise<AccountFriends[]>((resolve) => { resolveRows = resolve; });
      }
      return [];
    });
    renderTab();

    // Começa em 0/N e só então o backend reporta cada conta concluída.
    expect(await screen.findByText("Checking friends 0/2...")).toBeInTheDocument();

    act(() => emitTauriEvent("friends-online-progress", { done: 1, total: 2 }));
    expect(await screen.findByText("Checking friends 1/2...")).toBeInTheDocument();

    await act(async () => resolveRows([group(1001, [friend({ userId: 7001 })]), group(1002, [])]));
    expect(await screen.findByTestId("friend-1001-7001")).toBeInTheDocument();
  });

  it("keeps the other accounts' friends when one account fails", async () => {
    setFriends([
      group(1001, [], "Cookie expired"),
      group(1002, [friend({ userId: 7002, displayName: "Buddy" })]),
    ]);
    renderTab();

    const failed = await screen.findByTestId("friends-group-1001");
    expect(within(failed).getByText("Cookie expired")).toBeInTheDocument();
    // A conta boa continua listada com os amigos dela.
    expect(within(screen.getByTestId("friends-group-1002")).getByText("Buddy")).toBeInTheDocument();
    expect(screen.queryByTestId("friends-empty")).not.toBeInTheDocument();
  });

  it("shows the empty state when nobody is online", async () => {
    setFriends([group(1001, []), group(1002, [])]);
    renderTab();

    expect(await screen.findByTestId("friends-empty")).toHaveTextContent("No friends online right now");
  });

  it("surfaces a global failure without blowing up the tab", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_online_friends_for_accounts") throw new Error("friends endpoint is down");
      return [];
    });
    renderTab();

    expect(await screen.findByRole("alert")).toHaveTextContent("friends endpoint is down");
  });

  it("refetches when Reload is clicked", async () => {
    setFriends([group(1001, []), group(1002, [])]);
    renderTab();

    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(2));
  });

  it("does not call the backend when no account is selected", async () => {
    setFriends([]);
    renderTab([]);

    expect(await screen.findByTestId("friends-empty")).toBeInTheDocument();
    expect(callsFor("get_online_friends_for_accounts")).toHaveLength(0);
  });
});
