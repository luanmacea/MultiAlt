import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { SessionDialog, SessionToolbarButton } from "./SessionDialog";
import { makeAccount, renderWithStore, setStore } from "../../test-utils/renderWithStore";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import type { LaunchQueuePayload } from "../../types";
import type { StoreValue } from "../../store";

const ACCOUNTS = [
  makeAccount({ UserID: 10, Username: "alpha" }),
  makeAccount({ UserID: 20, Username: "bravo" }),
];

const QUEUE: LaunchQueuePayload = {
  entries: [
    { userId: 10, state: "launching", error: null, updatedAtMs: 1 },
    { userId: 20, state: "queued", error: null, updatedAtMs: 1 },
  ],
  active: true,
  placeId: 123,
  jobId: "",
};

/** Ações da store ligadas ao `invoke` mockado (ver SessionPanel.test.tsx). */
function storeActions(): Partial<StoreValue> {
  return {
    cancelAccountLaunch: vi.fn(
      async (userId: number) => (await invokeMock("cancel_account_launch", { userId })) as boolean
    ),
    stopLaunchQueue: vi.fn(async () => (await invokeMock("stop_launch_queue")) as number),
    focusRobloxClient: vi.fn(
      async (userId: number) => (await invokeMock("focus_roblox_window", { userId })) as boolean
    ),
    closeRobloxClients: vi.fn(async (userIds: number[]) => {
      for (const userId of userIds) await invokeMock("cmd_kill_roblox", { userId });
      return userIds.length;
    }),
  };
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeMap({
    cancel_account_launch: true,
    stop_launch_queue: 2,
    focus_roblox_window: true,
    cmd_kill_roblox: true,
  });
});

afterEach(cleanup);

describe("SessionDialog", () => {
  it("renders nothing while closed", () => {
    setStore({});
    const { container } = render(<SessionDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the queue and the running clients when open", () => {
    renderWithStore(<SessionDialog open onClose={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: QUEUE,
      launchedByProgram: new Set([10]),
      ...storeActions(),
    });

    expect(screen.getByRole("dialog", { name: "Session" })).toBeInTheDocument();
    expect(screen.getByTestId("session-queue-10")).toBeInTheDocument();
    expect(screen.getByTestId("session-queue-20")).toBeInTheDocument();
    expect(screen.getByTestId("session-running-10")).toBeInTheDocument();
    expect(screen.queryByTestId("session-running-20")).not.toBeInTheDocument();
  });

  it("drives the same actions as the Console panel", async () => {
    const user = userEvent.setup();
    renderWithStore(<SessionDialog open onClose={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: QUEUE,
      launchedByProgram: new Set([10, 20]),
      ...storeActions(),
    });

    await user.click(within(screen.getByTestId("session-queue-20")).getByRole("button"));
    await waitFor(() => expect(callsFor("cancel_account_launch")).toHaveLength(1));
    expect(callsFor("cancel_account_launch")[0][1]).toEqual({ userId: 20 });

    await user.click(within(screen.getByTestId("session-running-10")).getByRole("button", { name: "Focus" }));
    await waitFor(() => expect(callsFor("focus_roblox_window")).toHaveLength(1));
    expect(callsFor("focus_roblox_window")[0][1]).toEqual({ userId: 10 });
  });

  it("closes every selected client after one confirmation", async () => {
    const user = userEvent.setup();
    promptAnswers.confirm = true;
    renderWithStore(<SessionDialog open onClose={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set([10, 20]),
      ...storeActions(),
    });

    await user.click(screen.getByRole("checkbox", { name: "Select all running clients" }));
    await user.click(screen.getByRole("button", { name: /Close selected \(2\)/ }));

    await waitFor(() => expect(callsFor("cmd_kill_roblox")).toHaveLength(2));
  });

  it("closes on the X button and on Escape", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    renderWithStore(<SessionDialog open onClose={onClose} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      ...storeActions(),
    });

    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

describe("SessionToolbarButton", () => {
  it("opens the dialog through the store", async () => {
    const user = userEvent.setup();
    const { store } = renderWithStore(<SessionToolbarButton />, {
      launchedByProgram: new Set<number>(),
    });

    await user.click(screen.getByRole("button", { name: "Session" }));
    expect(store.setSessionDialogOpen).toHaveBeenCalledWith(true);
  });

  it("hides the badge with no client running", () => {
    renderWithStore(<SessionToolbarButton />, { launchedByProgram: new Set<number>() });
    expect(screen.queryByTestId("session-button-count")).not.toBeInTheDocument();
  });

  it("counts the clients that are in game", () => {
    renderWithStore(<SessionToolbarButton />, { launchedByProgram: new Set([1, 2, 3, 4]) });
    expect(screen.getByTestId("session-button-count")).toHaveTextContent("4");
  });
});
