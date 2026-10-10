import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { AccountHistory } from "./AccountHistory";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import { resetPromptMocks } from "../../test-utils/promptMocks";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { clearGameIdentityCache } from "../../hooks/useGameIdentity";
import type { SessionRecord } from "../../types";

const ACCOUNT = makeAccount({ UserID: 7, Username: "BobAlt" });
const MIN = 60_000;

function session(overrides: Partial<SessionRecord> = {}): SessionRecord {
  const now = Date.now();
  return {
    startedAt: now - 60 * MIN,
    endedAt: now - 30 * MIN,
    placeId: 606849621,
    jobId: "job-a",
    end: "left",
    dropKind: null,
    reason: null,
    code: null,
    message: null,
    ...overrides,
  };
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  clearGameIdentityCache();
});
afterEach(cleanup);

describe("AccountHistory", () => {
  it("says there is nothing yet for an account without sessions", async () => {
    setInvokeMap({ get_session_history: [] });
    renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT] });
    expect(await screen.findByTestId("history-empty")).toBeInTheDocument();
    expect(callsFor("get_session_history")).toEqual([["get_session_history", { userId: 7 }]]);
  });

  it("lists the sessions with the game name and how each ended, and the 14-day bars", async () => {
    setInvokeMap({
      get_session_history: [
        session({ end: "dropped", dropKind: "disconnected", reason: "connectionLost", code: 277 }),
        session({ startedAt: Date.now() - 300 * MIN, endedAt: Date.now() - 240 * MIN, end: "teleported" }),
      ],
      batched_get_game_info: { placeId: 606849621, universeId: 1, name: "Jailbreak", iconUrl: null },
    });
    renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT] });

    const rows = await screen.findAllByTestId("history-row");
    expect(rows).toHaveLength(2);
    expect(within(rows[0]).getByText("Disconnected: lost connection")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Moved to another server")).toBeInTheDocument();
    await waitFor(() => expect(within(rows[0]).getByText("Jailbreak")).toBeInTheDocument());
    // Um jogo, 90 minutos somados.
    const bar = screen.getByTestId("playtime-row");
    expect(within(bar).getByText("1h 30m")).toBeInTheDocument();
  });

  it("Join again sends this account to the same place and server through the normal launch", async () => {
    setInvokeMap({ get_session_history: [session()] });
    const { store } = renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT] });
    await userEvent.click(await screen.findByRole("button", { name: /Join again/ }));
    await waitFor(() =>
      expect(store.joinServer).toHaveBeenCalledWith(7, { placeId: "606849621", jobId: "job-a" })
    );
  });

  it("no Join again while the account is still in that server", async () => {
    setInvokeMap({ get_session_history: [session({ end: "ongoing", endedAt: null })] });
    renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT] });
    expect(await screen.findByText("Playing now")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Join again/ })).not.toBeInTheDocument();
  });

  it("reloads when the backend records something for this account", async () => {
    setInvokeMap({ get_session_history: [] });
    renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT] });
    await screen.findByTestId("history-empty");
    setInvokeMap({ get_session_history: [session()] });
    emitTauriEvent("session-history-changed", { userIds: [8] });
    emitTauriEvent("session-history-changed", { userIds: [7] });
    expect(await screen.findAllByTestId("history-row")).toHaveLength(1);
  });

  it("exports a CSV built here and saved by the backend", async () => {
    setInvokeMap({ get_session_history: [session()], save_history_export: "C:/x.csv" });
    renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT] });
    await userEvent.click(await screen.findByRole("button", { name: /Export CSV/ }));
    await waitFor(() => expect(callsFor("save_history_export")).toHaveLength(1));
    const args = callsFor("save_history_export")[0][1] as { userId: number; csv: string };
    expect(args.userId).toBe(7);
    expect(args.csv.split("\r\n")[0]).toContain("How it ended");
  });

  it("with names hidden a kick message does not show the account name", async () => {
    setInvokeMap({
      get_session_history: [session({ end: "dropped", dropKind: "kicked", message: "Bye BobAlt" })],
    });
    renderWithStore(<AccountHistory account={ACCOUNT} />, { accounts: [ACCOUNT], hideUsernames: true });
    await screen.findAllByTestId("history-row");
    expect(screen.queryByText(/BobAlt/)).not.toBeInTheDocument();
  });
});
