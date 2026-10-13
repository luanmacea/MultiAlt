import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { AccountList } from "./AccountList";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { clearGameIdentityCache } from "../../hooks/useGameIdentity";
import type { AutoReconnectEntry, ClientHealth, ClosedClient, CurrentSession } from "../../types";
import type { StoreValue } from "../../store";

/**
 * A lista de contas com clientes abertos (crítica de design P0, 11/10/2026):
 * com 7 clientes a linha só tinha a bolinha laranja e "now" — não dizia em que
 * jogo, nem se a conta caiu, está reconectando ou travou. A linha agora diz,
 * com as mesmas palavras da página Session.
 */

const JAILBREAK = 606849621;

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "alpha" }),
  makeAccount({ UserID: 2, Username: "bravo" }),
  makeAccount({ UserID: 3, Username: "charlie" }),
];

function session(userId: number, placeId: number, minutesAgo: number): CurrentSession {
  return { userId, placeId, jobId: `job-${userId}`, sinceMs: Date.now() - minutesAgo * 60_000, privateServer: false };
}

function health(overrides: Partial<ClientHealth> = {}): ClientHealth {
  return { pid: 1, logFound: true, drop: null, inGame: true, ...overrides };
}

function renderList(sessions: CurrentSession[], overrides: Partial<StoreValue> = {}) {
  setInvokeMap({
    get_current_sessions: () => sessions,
    batched_get_game_info: (args: Record<string, unknown> | undefined) => ({
      placeId: Number(args?.placeId),
      universeId: 1,
      name: Number(args?.placeId) === JAILBREAK ? "Jailbreak" : null,
      iconUrl: null,
    }),
  });
  const store = setStore({ accounts: ACCOUNTS, ...overrides });
  render(<AccountList />);
  return store;
}

function row(userId: number): HTMLElement {
  const el = document.querySelector(`[data-account-row='true'][data-user-id='${userId}']`);
  if (!el) throw new Error(`row ${userId} not rendered`);
  return el as HTMLElement;
}

beforeEach(() => {
  resetTauriMocks();
  clearGameIdentityCache();
});
afterEach(cleanup);

describe("Accounts list — live state of running accounts", () => {
  it("a running account shows Playing and the game; an idle one shows nothing extra", async () => {
    renderList([session(1, JAILBREAK, 47)], {
      launchedByProgram: new Set([1]),
      clientHealth: new Map([[1, health()]]),
    });

    expect(await within(row(1)).findByText("Playing")).toBeInTheDocument();
    expect(await within(row(1)).findByText("Jailbreak", {}, { timeout: 2000 })).toBeInTheDocument();
    expect(within(row(1)).getByTestId("account-live-1")).toHaveAttribute("data-state", "playing");

    expect(screen.queryByTestId("account-live-2")).not.toBeInTheDocument();
    expect(screen.queryByTestId("account-live-3")).not.toBeInTheDocument();
  });

  it("a dropped account says Dropped, with the reason", async () => {
    renderList([], {
      launchedByProgram: new Set([2]),
      clientHealth: new Map([
        [2, health({ inGame: false, drop: { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: 1 } })],
      ]),
    });

    const live = await within(row(2)).findByTestId("account-live-2");
    expect(live).toHaveAttribute("data-state", "dropped");
    expect(within(live).getByText("Dropped")).toBeInTheDocument();
    expect(within(live).getByText("Disconnected: lost connection")).toBeInTheDocument();
  });

  it("not responding and reconnecting use the Session words", async () => {
    const reconnect: AutoReconnectEntry = {
      userId: 3,
      phase: "waiting",
      attempt: 1,
      maxAttempts: 5,
      nextAttemptAtMs: Date.now() + 30_000,
      reason: null,
      error: null,
      drop: { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: 1 },
    };
    renderList([session(1, JAILBREAK, 5)], {
      launchedByProgram: new Set([1]),
      clientHealth: new Map([[1, health({ notResponding: true })]]),
      // A 3ª fechou o cliente sozinha e está esperando a próxima tentativa.
      autoReconnect: [reconnect],
    });

    expect(await within(row(1)).findByText("Not responding")).toBeInTheDocument();
    expect(within(row(3)).getByTestId("account-live-3")).toHaveAttribute("data-state", "reconnecting");
    expect(within(row(3)).getByText("Reconnecting")).toBeInTheDocument();
  });

  it("an account the launch queue is opening says Joining", () => {
    renderList([], {
      launchQueue: {
        entries: [
          { userId: 2, state: "launching", error: null, updatedAtMs: 1, placeId: JAILBREAK, jobId: "" },
          { userId: 3, state: "queued", error: null, updatedAtMs: 1, placeId: JAILBREAK, jobId: "" },
        ],
        active: true,
        placeId: JAILBREAK,
        jobId: "",
      },
    });

    expect(within(row(2)).getByTestId("account-live-2")).toHaveAttribute("data-state", "joining");
    expect(within(row(2)).getByText("Joining")).toBeInTheDocument();
    // Na fila, mas ainda não abrindo: nada novo na linha.
    expect(screen.queryByTestId("account-live-3")).not.toBeInTheDocument();
  });

  it("does not read sessions while nothing is running", async () => {
    renderList([]);
    await waitFor(() => expect(screen.getAllByText(/alpha|bravo|charlie/).length).toBe(3));
    expect(invokeMock.mock.calls.filter((c) => c[0] === "get_current_sessions")).toHaveLength(0);
  });

  it("keeps the masked names when names are hidden", async () => {
    renderList([session(1, JAILBREAK, 3)], {
      launchedByProgram: new Set([1]),
      clientHealth: new Map([[1, health()]]),
      hideUsernames: true,
      hiddenNameLetters: 2,
    });
    expect(await within(row(1)).findByText("Playing")).toBeInTheDocument();
    expect(within(row(1)).queryByText("alpha")).not.toBeInTheDocument();
  });

  describe("an account whose client closed (owner, 12/10/2026: it just vanished from the row)", () => {
    function closedClient(overrides: Partial<ClosedClient> = {}): ClosedClient {
      return {
        userId: 2,
        closedAtMs: Date.now() - 60_000,
        drop: { kind: "kicked", reason: null, code: 267, message: "AFK too long", sinceMs: 1 },
        placeId: JAILBREAK,
        rejoin: { placeId: JAILBREAK, jobId: "job-b", launchData: "", joinVip: false, linkCode: "" },
        ...overrides,
      };
    }

    it("says Closed, with the reason and the game", async () => {
      renderList([], { closedClients: new Map([[2, closedClient()]]) });

      const live = within(row(2)).getByTestId("account-live-2");
      expect(live).toHaveAttribute("data-state", "closed");
      expect(within(live).getByText("Closed")).toBeInTheDocument();
      expect(within(live).getByText("Kicked: AFK too long")).toBeInTheDocument();
      expect(await within(live).findByText("Jailbreak", {}, { timeout: 2000 })).toBeInTheDocument();
    });

    it("Rejoin launches it back into the same server through the normal launch", async () => {
      const store = renderList([], { closedClients: new Map([[2, closedClient()]]) });

      await userEvent.click(within(row(2)).getByRole("button", { name: "Rejoin" }));
      expect(store.joinServer).toHaveBeenCalledWith(2, {
        placeId: String(JAILBREAK),
        jobId: "job-b",
        launchData: "",
      });
      // O clique no botão não seleciona a linha.
      expect(store.handleSelect).not.toHaveBeenCalled();
    });

    it("Rejoin to a private server carries the VIP join and its code", async () => {
      const store = renderList([], {
        closedClients: new Map([
          [2, closedClient({ rejoin: { placeId: JAILBREAK, jobId: "", launchData: "", joinVip: true, linkCode: "CODE" } })],
        ]),
      });

      await userEvent.click(within(row(2)).getByRole("button", { name: "Rejoin" }));
      expect(store.joinServer).toHaveBeenCalledWith(2, {
        placeId: String(JAILBREAK),
        jobId: "",
        launchData: "",
        joinVip: true,
        linkCode: "CODE",
      });
    });

    it("the × dismisses it", async () => {
      const store = renderList([], { closedClients: new Map([[2, closedClient()]]) });

      await userEvent.click(within(row(2)).getByRole("button", { name: "Dismiss" }));
      expect(store.dismissClosedClient).toHaveBeenCalledWith(2);
    });

    it("waiting in the launch queue: Rejoin is off (it is already going back)", () => {
      renderList([], {
        closedClients: new Map([[2, closedClient()]]),
        launchQueue: {
          entries: [{ userId: 2, state: "queued", error: null, updatedAtMs: 1, placeId: JAILBREAK, jobId: "job-b" }],
          active: true,
          placeId: JAILBREAK,
          jobId: "",
        },
      });
      expect(within(row(2)).getByRole("button", { name: "Rejoin" })).toBeDisabled();
    });

    it("joining again replaces it", () => {
      renderList([], {
        closedClients: new Map([[2, closedClient()]]),
        joiningAccounts: new Set([2]),
      });
      expect(within(row(2)).getByTestId("account-live-2")).toHaveAttribute("data-state", "joining");
      expect(within(row(2)).queryByRole("button", { name: "Rejoin" })).not.toBeInTheDocument();
    });

    it("no destination known: no Rejoin, the state still shows", () => {
      renderList([], { closedClients: new Map([[2, closedClient({ rejoin: null })]]) });
      expect(within(row(2)).getByTestId("account-live-2")).toHaveAttribute("data-state", "closed");
      expect(within(row(2)).queryByRole("button", { name: "Rejoin" })).not.toBeInTheDocument();
    });
  });
});
