import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { LivePage } from "./LivePage";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

let stored: Record<string, Record<string, string>> = {};
let accounts = [
  { userId: 1, label: "Farm_01", inGame: true, selected: true },
  { userId: 2, label: "Farm_02", inGame: false, selected: true },
];

beforeEach(() => {
  resetTauriMocks();
  stored = { Live: { ShowNames: "true", KeepMinimized: "true", Background: "dark" } };
  setInvokeHandler((cmd) => {
    if (cmd === "get_all_settings") return stored;
    if (cmd === "live_accounts") return accounts;
    if (cmd === "live_window_open") return false;
    return undefined;
  });
});
afterEach(cleanup);

function calls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);
}

describe("Live page", () => {
  it("lists the open accounts and says every open account is shown by default", async () => {
    render(<LivePage active onLeave={() => {}} />);
    expect(await screen.findByLabelText("Farm_01")).toBeChecked();
    expect(screen.getByText("Every open account is shown, including the ones you open later.")).toBeInTheDocument();
    expect(screen.getByText("Not in a game")).toBeInTheDocument();
  });

  it("unticking an account keeps an explicit list, so unticking the last one shows none", async () => {
    render(<LivePage active onLeave={() => {}} />);
    await userEvent.click(await screen.findByLabelText("Farm_02"));
    await waitFor(() => expect(calls("set_live_accounts")).toContainEqual({ userIds: [1], all: false }));
  });

  it("opens the Live window", async () => {
    render(<LivePage active onLeave={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "Open the Live window" }));
    await waitFor(() => expect(calls("open_live_window")).toHaveLength(1));
  });

  it("says what to do when no client is open", async () => {
    accounts = [];
    render(<LivePage active onLeave={() => {}} />);
    expect(await screen.findByText(/No Roblox client open/)).toBeInTheDocument();
    accounts = [
      { userId: 1, label: "Farm_01", inGame: true, selected: true },
      { userId: 2, label: "Farm_02", inGame: false, selected: true },
    ];
  });
});
