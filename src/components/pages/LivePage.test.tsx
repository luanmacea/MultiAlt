import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

const DEFAULT_ACCOUNTS = [
  { userId: 1, label: "Farm_01", inGame: true, selected: true },
  { userId: 2, label: "Farm_02", inGame: false, selected: true },
];

beforeEach(() => {
  resetTauriMocks();
  accounts = DEFAULT_ACCOUNTS.map((a) => ({ ...a }));
  stored = { Live: { ShowNames: "true", KeepMinimized: "true", Background: "dark" } };
  setInvokeHandler((cmd, args) => {
    if (cmd === "set_live_accounts") {
      // O backend guarda a escolha: a próxima leitura da lista a devolve.
      const ids = (args as { userIds: number[] }).userIds;
      accounts = accounts.map((a) => ({ ...a, selected: ids.includes(a.userId) }));
      return undefined;
    }
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

  it("Shift+click and dragging over the rows change several accounts in one go", async () => {
    accounts = [
      { userId: 1, label: "Farm_01", inGame: true, selected: false },
      { userId: 2, label: "Farm_02", inGame: true, selected: false },
      { userId: 3, label: "Farm_03", inGame: true, selected: false },
      { userId: 4, label: "Farm_04", inGame: true, selected: false },
    ];
    stored.Live.Accounts = "2";
    render(<LivePage active onLeave={() => {}} />);
    fireEvent.click(await screen.findByLabelText("Farm_01"));
    fireEvent.click(screen.getByLabelText("Farm_03"), { shiftKey: true });
    await waitFor(() => expect(screen.getByLabelText("Farm_02")).toBeChecked());
    expect(screen.getByLabelText("Farm_03")).toBeChecked();
    expect(screen.getByLabelText("Farm_04")).not.toBeChecked();
    await waitFor(() => expect(calls("set_live_accounts")).toContainEqual({ userIds: [1, 2, 3], all: false }));

    // Drag from the unticked Farm_04 upwards: ticks what it passes.
    fireEvent.mouseDown(screen.getByLabelText("Farm_04"), { button: 0 });
    fireEvent.mouseMove(screen.getByLabelText("Farm_03").closest("li") as HTMLElement);
    fireEvent.mouseUp(screen.getByLabelText("Farm_03"));
    await waitFor(() => expect(calls("set_live_accounts").slice(-1)[0]).toEqual({ userIds: [1, 2, 3, 4], all: false }));
  });

  it("Clear unticks every account", async () => {
    render(<LivePage active onLeave={() => {}} />);
    await screen.findByLabelText("Farm_01");
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    await waitFor(() => expect(calls("set_live_accounts")).toContainEqual({ userIds: [], all: false }));
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
  });
});
