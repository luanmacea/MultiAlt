import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { AccountRecentGames, clearAccountRecentGamesCache, type AccountRecentGamesEntry } from "./AccountRecentGames";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "alpha" }),
  makeAccount({ UserID: 2, Username: "bravo" }),
];

const ENTRIES: AccountRecentGamesEntry[] = [
  {
    userId: 1,
    games: [
      { placeId: 79546208627805, universeId: 7326934954, name: "99 Nights", iconUrl: "https://icon/99" },
      { placeId: 606849621, universeId: 245662005, name: "Jailbreak", iconUrl: null },
    ],
    error: null,
  },
  { userId: 2, games: [], error: "Failed to read recent games (status 401)" },
];

function renderList(props: Partial<Parameters<typeof AccountRecentGames>[0]> = {}) {
  const handlers = {
    onSelectGame: vi.fn(),
    onJoinGame: vi.fn(),
    onBrowseServers: vi.fn(),
    onAddFavorite: vi.fn(),
  };
  renderWithStore(<AccountRecentGames userIds={[1, 2]} {...handlers} {...props} />, { accounts: ACCOUNTS });
  return handlers;
}

beforeEach(() => {
  resetTauriMocks();
  clearAccountRecentGamesCache();
  setInvokeHandler((cmd) => (cmd === "get_recent_games_for_accounts" ? ENTRIES : undefined));
});

afterEach(cleanup);

describe("AccountRecentGames", () => {
  it("shows each account's recent games under its own name, like the Friends tab", async () => {
    renderList();
    const alpha = await screen.findByTestId("account-recent-1");
    expect(within(alpha).getByText("alpha")).toBeInTheDocument();
    expect(within(alpha).getByText("99 Nights")).toBeInTheDocument();
    expect(within(alpha).getByText("Jailbreak")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_recent_games_for_accounts", { userIds: [1, 2] });
  });

  it("puts an account's error on that account only", async () => {
    renderList();
    const bravo = await screen.findByTestId("account-recent-2");
    expect(within(bravo).getByText("Failed to read recent games (status 401)")).toBeInTheDocument();
    expect(within(screen.getByTestId("account-recent-1")).queryByText(/status 401/)).not.toBeInTheDocument();
  });

  it("opens the servers on click and joins from the row button", async () => {
    const handlers = renderList();
    const row = await screen.findByTestId("account-recent-1-79546208627805");
    await userEvent.click(within(row).getByText("99 Nights"));
    expect(handlers.onSelectGame).toHaveBeenCalledWith(79546208627805, "99 Nights", "https://icon/99");
    await userEvent.click(within(row).getByRole("button", { name: "Join Game" }));
    expect(handlers.onJoinGame).toHaveBeenCalledWith(79546208627805);
  });

  it("does not ask Roblox again when the tab is reopened within a minute", async () => {
    renderList();
    await screen.findByTestId("account-recent-1");
    cleanup();
    renderList();
    expect(await screen.findByTestId("account-recent-1")).toBeInTheDocument();
    expect(invokeMock.mock.calls.filter((c) => c[0] === "get_recent_games_for_accounts")).toHaveLength(1);
  });

  it("refresh asks again", async () => {
    renderList();
    await screen.findByTestId("account-recent-1");
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter((c) => c[0] === "get_recent_games_for_accounts")).toHaveLength(2)
    );
  });

  it("says when an account has nothing recent", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_recent_games_for_accounts" ? [{ userId: 1, games: [], error: null }] : undefined
    );
    renderList({ userIds: [1] });
    expect(await screen.findByText("No recent games on Roblox for this account.")).toBeInTheDocument();
  });
});
