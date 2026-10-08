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

  it("hides the user's own account photo with the names, unless avatars are kept", async () => {
    setFriends([group(1002, [friend({ userId: 7002, displayName: "Buddy" })])]);
    renderTab([ACCOUNT_B], {
      hideUsernames: true,
      showAvatarsWhenHidden: false,
      avatarUrls: new Map([[1002, "https://avatar.test/1002.png"]]),
    });
    const block = await screen.findByTestId("friends-group-1002");
    expect(block.innerHTML).not.toContain("avatar.test/1002.png");
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

  it("says 'no friends online' inside each account, without a second notice on top", async () => {
    // O aviso grande no topo repetia o que cada conta já diz (pedido do dono, 08/10/2026).
    setFriends([group(1001, []), group(1002, [])]);
    renderTab();

    expect(await screen.findByTestId("friends-group-1001")).toHaveTextContent("No friends online");
    expect(screen.getByTestId("friends-group-1002")).toHaveTextContent("No friends online");
    expect(screen.queryByTestId("friends-empty")).not.toBeInTheDocument();
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

/**
 * O backend percorre as contas uma a uma, com pausa entre elas (rate limit).
 * Esperar o lote inteiro deixava a aba vazia por segundos com 6+ contas; agora
 * cada conta aparece quando o evento dela chega.
 */
describe("FriendsTab — progressive loading", () => {
  /** Backend pendurado: devolve o `requestId` mandado e o gatilho do fim. */
  function hangingBackend() {
    const control = { finish: (_rows: AccountFriends[]) => {} };
    setInvokeHandler((cmd) => {
      if (cmd === "get_online_friends_for_accounts") {
        return new Promise<AccountFriends[]>((resolve) => {
          control.finish = resolve;
        });
      }
      return [];
    });
    return control;
  }

  function lastRequestId(): number {
    const calls = callsFor("get_online_friends_for_accounts");
    const args = (calls[calls.length - 1]?.[1] ?? {}) as Record<string, unknown>;
    return args.requestId as number;
  }

  it("sends a request id so the progress events can be told apart", async () => {
    hangingBackend();
    renderTab();
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));
    expect(typeof lastRequestId()).toBe("number");
  });

  it("shows an account's friends as soon as its progress event arrives", async () => {
    hangingBackend();
    renderTab();
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));

    // Antes de qualquer conta voltar, as duas aparecem como pendentes.
    expect(screen.getByTestId("friends-pending-1001")).toBeInTheDocument();
    expect(screen.getByTestId("friends-pending-1002")).toBeInTheDocument();

    act(() =>
      emitTauriEvent("friends-online-progress", {
        done: 1,
        total: 2,
        requestId: lastRequestId(),
        entry: group(1001, [friend({ userId: 7001, displayName: "Friendo" })]),
      })
    );

    expect(await screen.findByTestId("friend-1001-7001")).toBeInTheDocument();
    // A segunda conta continua esperando, com o próprio indicador.
    expect(screen.getByTestId("friends-pending-1002")).toBeInTheDocument();
    expect(screen.queryByTestId("friends-group-1002")).not.toBeInTheDocument();
  });

  it("keeps the selection order no matter which account answers first", async () => {
    hangingBackend();
    renderTab();
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));

    act(() =>
      emitTauriEvent("friends-online-progress", {
        done: 1,
        total: 2,
        requestId: lastRequestId(),
        entry: group(1002, [friend({ userId: 7002, displayName: "Buddy" })]),
      })
    );
    await screen.findByTestId("friend-1002-7002");

    const sections = [...screen.getByTestId("friends-tab").querySelectorAll("section")];
    expect(sections.map((s) => s.getAttribute("data-testid"))).toEqual([
      "friends-pending-1001",
      "friends-group-1002",
    ]);
  });

  it("ignores progress entries from another request", async () => {
    hangingBackend();
    renderTab();
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));

    act(() =>
      emitTauriEvent("friends-online-progress", {
        done: 1,
        total: 2,
        requestId: lastRequestId() + 1000,
        entry: group(1001, [friend({ userId: 7001 })]),
      })
    );

    expect(screen.queryByTestId("friend-1001-7001")).not.toBeInTheDocument();
    expect(screen.getByTestId("friends-pending-1001")).toBeInTheDocument();
  });

  it("the final answer replaces what was streamed", async () => {
    const backend = hangingBackend();
    renderTab();
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));

    act(() =>
      emitTauriEvent("friends-online-progress", {
        done: 1,
        total: 2,
        requestId: lastRequestId(),
        entry: group(1001, [friend({ userId: 7001 })]),
      })
    );
    await screen.findByTestId("friend-1001-7001");

    await act(async () =>
      backend.finish([group(1001, [friend({ userId: 7009 })]), group(1002, [])])
    );
    expect(await screen.findByTestId("friend-1001-7009")).toBeInTheDocument();
    expect(screen.queryByTestId("friend-1001-7001")).not.toBeInTheDocument();
    expect(screen.queryByTestId("friends-pending-1002")).not.toBeInTheDocument();
  });
});

/**
 * Sair da aba e voltar não pode recomeçar do zero: a aba mostra o que já tinha
 * na hora e atualiza por trás.
 */
describe("FriendsTab — cache across tab switches", () => {
  it("coming back shows the last list at once and refreshes in the background", async () => {
    setFriends([group(1001, [friend({ userId: 7001 })]), group(1002, [])]);
    const first = renderTab();
    await screen.findByTestId("friend-1001-7001");
    first.unmount();

    let finish: (rows: AccountFriends[]) => void = () => {};
    setInvokeHandler((cmd) => {
      if (cmd === "get_online_friends_for_accounts") {
        return new Promise<AccountFriends[]>((resolve) => {
          finish = resolve;
        });
      }
      return [];
    });
    renderTab();

    // Já no primeiro desenho, sem esperar o backend.
    expect(screen.getByTestId("friend-1001-7001")).toBeInTheDocument();
    expect(screen.queryByTestId("friends-pending-1001")).not.toBeInTheDocument();
    // E a atualização roda por trás, com o indicador por conta.
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(2));
    expect(screen.getByTestId("friends-updating-1001")).toBeInTheDocument();

    await act(async () => finish([group(1001, [friend({ userId: 7005 })]), group(1002, [])]));
    expect(await screen.findByTestId("friend-1001-7005")).toBeInTheDocument();
    expect(screen.queryByTestId("friend-1001-7001")).not.toBeInTheDocument();
    expect(screen.queryByTestId("friends-updating-1001")).not.toBeInTheDocument();
  });

  it("does not reuse the list of a different selection", async () => {
    setFriends([group(1001, [friend({ userId: 7001 })]), group(1002, [])]);
    const first = renderTab();
    await screen.findByTestId("friend-1001-7001");
    first.unmount();

    setInvokeHandler((cmd) =>
      cmd === "get_online_friends_for_accounts" ? new Promise(() => {}) : []
    );
    renderTab([ACCOUNT_A]);

    expect(screen.queryByTestId("friend-1001-7001")).not.toBeInTheDocument();
    expect(screen.getByTestId("friends-pending-1001")).toBeInTheDocument();
  });

  it("re-entering while a load is still running reuses it instead of asking again", async () => {
    let finish: (rows: AccountFriends[]) => void = () => {};
    setInvokeHandler((cmd) => {
      if (cmd === "get_online_friends_for_accounts") {
        return new Promise<AccountFriends[]>((resolve) => {
          finish = resolve;
        });
      }
      return [];
    });
    const first = renderTab();
    await waitFor(() => expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1));
    first.unmount();

    renderTab();
    await act(async () => finish([group(1001, [friend({ userId: 7001 })]), group(1002, [])]));

    expect(await screen.findByTestId("friend-1001-7001")).toBeInTheDocument();
    // O backend já estava percorrendo as contas: uma segunda rodada dobraria
    // as chamadas à API de amigos (rate limit por IP).
    expect(callsFor("get_online_friends_for_accounts")).toHaveLength(1);
  });
});
