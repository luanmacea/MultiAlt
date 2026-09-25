import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { BottomActionBar } from "./BottomActionBar";
import {
  defaultSettings,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import {
  emitTauriEvent,
  invokeMock,
  resetTauriMocks,
  setInvokeHandler,
} from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { StoreValue } from "../../store";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });
const C = makeAccount({ UserID: 3, Username: "cid", Group: "Farm" });

const writeText = vi.fn(async () => {});

function renderBar(selected = [A, B], overrides: Partial<StoreValue> = {}) {
  const store = setStore({
    accounts: [A, B, C],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    ...overrides,
  });
  render(<BottomActionBar />);
  return store;
}

async function openActions() {
  await userEvent.click(screen.getByRole("button", { name: /^Actions/ }));
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  writeText.mockClear();
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
});

afterEach(cleanup);

describe("BottomActionBar — selection-size rules", () => {
  it("names the single selected account and offers the Account button", () => {
    renderBar([A]);
    expect(screen.getByText("ann")).toBeInTheDocument();
    expect(screen.getByText("1 account selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Account$/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose Game" })).toBeInTheDocument();
  });

  it("hides the Account button and counts the selection when several are picked", () => {
    renderBar([A, B]);
    expect(screen.getByText("2 accounts selected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Account$/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose Game (2)" })).toBeInTheDocument();
  });

  it("offers Make Friends only from two accounts up", async () => {
    renderBar([A]);
    await openActions();
    expect(screen.queryByRole("button", { name: /Make Friends/ })).not.toBeInTheDocument();

    cleanup();
    renderBar([A, B]);
    await openActions();
    expect(screen.getByRole("button", { name: /Make Friends \(2\)/ })).toBeInTheDocument();
  });

  it("offers Restart Launched only for accounts this app launched", async () => {
    renderBar([A, B]);
    await openActions();
    expect(screen.queryByRole("button", { name: /Restart Launched/ })).not.toBeInTheDocument();

    cleanup();
    renderBar([A, B], { launchedByProgram: new Set([2]) });
    await openActions();
    expect(screen.getByRole("button", { name: /Restart Launched \(1\)/ })).toBeInTheDocument();
  });

  it("hides the botting entries until botting is enabled or running", async () => {
    renderBar([A, B]);
    await openActions();
    expect(screen.queryByRole("button", { name: /Open Botting Mode/ })).not.toBeInTheDocument();

    cleanup();
    const settings = defaultSettings();
    settings.General.BottingEnabled = "true";
    renderBar([A, B], { settings });
    await openActions();
    expect(screen.getByRole("button", { name: /Open Botting Mode/ })).toBeInTheDocument();
  });

  it("offers Add to Botting only for accounts not already in the loop", async () => {
    renderBar([A, B], { bottingStatus: makeBottingStatus({ active: true, userIds: [1] }) });
    await openActions();
    expect(screen.getByRole("button", { name: /Add to Botting \(1\)/ })).toBeInTheDocument();

    cleanup();
    renderBar([A, B], { bottingStatus: makeBottingStatus({ active: true, userIds: [1, 2] }) });
    await openActions();
    expect(screen.queryByRole("button", { name: /Add to Botting/ })).not.toBeInTheDocument();
  });
});

describe("BottomActionBar — actions", () => {
  it("clears the selection", async () => {
    const store = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(store.deselectAll).toHaveBeenCalledTimes(1);
  });

  it("opens the Choose Game screen", async () => {
    const store = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Choose Game (2)" }));
    expect(store.setChooseGameOpen).toHaveBeenCalledWith(true);
  });

  it("toggles the detail sidebar for a single account", async () => {
    const store = renderBar([A], { sidebarOpen: false });
    await userEvent.click(screen.getByRole("button", { name: /Account$/ }));
    expect(store.setSidebarOpen).toHaveBeenCalledWith(true);
  });

  it("copies the selected cookies one per line", async () => {
    renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("cookie-1\ncookie-2"));
  });

  it("lists the existing groups and moves the selection into one", async () => {
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Move to Group/ }));
    await userEvent.click(screen.getByRole("button", { name: "Farm" }));
    expect(store.moveToGroup).toHaveBeenCalledWith([1, 2], "Farm");
  });

  it("asks for a name before creating a new group", async () => {
    promptAnswers.prompt = "  Bots  ";
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Move to Group/ }));
    await userEvent.click(screen.getByRole("button", { name: /New Group/ }));
    await waitFor(() => expect(store.moveToGroup).toHaveBeenCalledWith([1, 2], "Bots"));
  });

  it("does not create a group when the prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Move to Group/ }));
    await userEvent.click(screen.getByRole("button", { name: /New Group/ }));
    await Promise.resolve();
    expect(store.moveToGroup).not.toHaveBeenCalled();
  });

  it("requires typing REMOVE before deleting accounts", async () => {
    promptAnswers.prompt = "nope";
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Remove \(2\)/ }));
    await Promise.resolve();
    expect(store.removeAccounts).not.toHaveBeenCalled();

    cleanup();
    promptAnswers.prompt = "remove";
    const store2 = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Remove \(2\)/ }));
    await waitFor(() => expect(store2.removeAccounts).toHaveBeenCalledWith([1, 2]));
  });

  it("closes every Roblox process", async () => {
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Close All Roblox/ }));
    expect(store.killAllRobloxProcesses).toHaveBeenCalledTimes(1);
  });

  it("opens the botting dialog and warns when botting is not running yet", async () => {
    const settings = defaultSettings();
    settings.General.BottingEnabled = "true";
    const store = renderBar([A, B], { settings, bottingStatus: makeBottingStatus({ active: false }) });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Open Botting Mode/ }));
    expect(store.setBottingDialogOpen).toHaveBeenCalledWith(true);
  });

  it("adds only the accounts missing from an active botting loop", async () => {
    const store = renderBar([A, B], {
      bottingStatus: makeBottingStatus({ active: true, userIds: [1] }),
    });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Add to Botting \(1\)/ }));
    await waitFor(() => expect(store.addBottingAccounts).toHaveBeenCalledWith([2]));
  });

  it("restarts only the clients this app launched", async () => {
    const store = renderBar([A, B], { launchedByProgram: new Set([2]) });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Restart Launched \(1\)/ }));
    await waitFor(() => expect(store.restartRobloxClients).toHaveBeenCalledWith([2]));
  });
});

describe("BottomActionBar — friend linking", () => {
  const FOUR = [A, B, C, makeAccount({ UserID: 4, Username: "dee" })];

  it("sends a mesh request for the whole selection", async () => {
    setInvokeHandler(() => ({ pairsTotal: 2, alreadyFriends: 0, verifiedOk: 2, failed: 0 }));
    renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("make_selected_friends", {
        userIds: [1, 2],
        mode: "mesh",
        mainUserId: null,
        delayMs: null,
      })
    );
  });

  it("sends a star request with the picked main account", async () => {
    setInvokeHandler(() => ({ pairsTotal: 1, alreadyFriends: 0, verifiedOk: 1, failed: 0 }));
    renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /⭐ bob/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "make_selected_friends",
        expect.objectContaining({ mode: "star", mainUserId: 2 })
      )
    );
  });

  it("skips the confirmation at exactly 30 requests", async () => {
    setInvokeHandler(() => ({ pairsTotal: 15, alreadyFriends: 0, verifiedOk: 15, failed: 0 }));
    const six = [...FOUR, makeAccount({ UserID: 5 }), makeAccount({ UserID: 6 })];
    renderBar(six);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(6\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all \(30 req\)/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("make_selected_friends", expect.anything())
    );
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("confirms above 30 requests and aborts when declined", async () => {
    const seven = [
      ...FOUR,
      makeAccount({ UserID: 5 }),
      makeAccount({ UserID: 6 }),
      makeAccount({ UserID: 7 }),
    ];
    renderBar(seven);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(7\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all \(42 req\)/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("make_selected_friends", expect.anything());
  });

  it("shows the live friend-link progress on the Actions button", async () => {
    let release: (value: unknown) => void = () => {};
    setInvokeHandler(() => new Promise((resolve) => { release = resolve; }));
    renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all/ }));

    emitTauriEvent("friend-link-progress", { phase: "checking", done: 1, total: 4 });
    expect(await screen.findByRole("button", { name: /Checking 1\/4/ })).toBeInTheDocument();

    emitTauriEvent("friend-link-progress", { phase: "verifying", done: 3, total: 4 });
    expect(await screen.findByRole("button", { name: /Verifying 3\/4/ })).toBeInTheDocument();

    emitTauriEvent("friend-link-progress", { phase: "linking", done: 4, total: 4 });
    expect(await screen.findByRole("button", { name: /Linking 4\/4/ })).toBeInTheDocument();

    emitTauriEvent("friend-link-progress", { phase: "done", done: 4, total: 4 });
    expect(await screen.findByRole("button", { name: /Linking friends\.\.\./ })).toBeInTheDocument();

    release({ pairsTotal: 2, alreadyFriends: 0, verifiedOk: 2, failed: 0 });
    await waitFor(() => expect(screen.getByRole("button", { name: /^Actions/ })).toBeInTheDocument());
  });
});

describe("BottomActionBar — Hidden mode", () => {
  it("masks the selected account name like the list does", () => {
    renderBar([A], { hideUsernames: true, hiddenNameLetters: 2 });
    expect(screen.queryByText("ann")).not.toBeInTheDocument();
    expect(screen.getByText("an********")).toBeInTheDocument();
  });

  it("hides the name entirely when no preview letters are configured", () => {
    renderBar([A], { hideUsernames: true, hiddenNameLetters: 0 });
    expect(screen.queryByText("ann")).not.toBeInTheDocument();
    expect(screen.getByText("************")).toBeInTheDocument();
  });

  it("masks the account's alias too", () => {
    const aliased = makeAccount({ UserID: 9, Username: "ann", Alias: "mainAccount" });
    renderBar([aliased], { hideUsernames: true, hiddenNameLetters: 4 });
    expect(screen.queryByText("mainAccount")).not.toBeInTheDocument();
    expect(screen.getByText("main********")).toBeInTheDocument();
  });

  it("masks the names listed in the Make Friends star picker", async () => {
    renderBar([A, B], { hideUsernames: true, hiddenNameLetters: 2 });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    expect(screen.queryByRole("button", { name: /ann/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /an\*{8}/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /bo\*{8}/ })).toBeInTheDocument();
  });

  it("shows the real name while Hidden is off", () => {
    renderBar([A]);
    expect(screen.getByText("ann")).toBeInTheDocument();
  });
});
