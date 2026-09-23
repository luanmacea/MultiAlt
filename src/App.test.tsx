import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("./store", async () => (await import("./test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("./test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("./test-utils/tauriMocks")).tauriEventMock());
vi.mock("@tauri-apps/api/window", async () => (await import("./test-utils/tauriMocks")).tauriWindowMock());
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(async () => {}),
  disable: vi.fn(async () => {}),
}));

import App from "./App";
import { makeAccount, setStore } from "./test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "./test-utils/tauriMocks";
import type { StoreValue } from "./store";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });

function renderApp(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ accounts: [A, B], ...overrides });
  render(<App />);
  return store;
}

function selecting(ids: number[]) {
  const selectedAccounts = [A, B].filter((a) => ids.includes(a.UserID));
  return { selectedIds: new Set(ids), selectedAccounts };
}

beforeEach(() => {
  resetTauriMocks();
  localStorage.clear();
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return {};
      case "get_setting":
        return "";
      case "versions_list_installed":
        return [];
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("App — blocking screens", () => {
  it("waits while the store initialises", () => {
    renderApp({ initialized: false });
    expect(screen.getByText("Loading...")).toBeInTheDocument();
    expect(screen.queryByText("Roblox Account Manager")).not.toBeInTheDocument();
  });

  it("asks for the password before anything else", () => {
    renderApp({ needsPassword: true, encryptionSetupOpen: true });
    expect(screen.getByText("Restricted Access")).toBeInTheDocument();
    expect(screen.queryByText("Set Up Encryption")).not.toBeInTheDocument();
  });

  it("shows the encryption setup once unlocked", () => {
    renderApp({ encryptionSetupOpen: true });
    expect(screen.getByText("Set Up Encryption")).toBeInTheDocument();
    expect(screen.queryByText("Restricted Access")).not.toBeInTheDocument();
  });

  it("checks for updates once the app is usable", () => {
    const store = renderApp();
    expect(store.checkForUpdates).toHaveBeenCalledTimes(1);
  });

  it("does not check for updates behind the password screen", () => {
    const store = renderApp({ needsPassword: true });
    expect(store.checkForUpdates).not.toHaveBeenCalled();
  });
});

describe("App — main layout", () => {
  it("renders the window chrome, account list and status bar", () => {
    renderApp();
    expect(screen.getByText("Roblox Account Manager")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Filter accounts...")).toBeInTheDocument();
    expect(screen.getByText("ann")).toBeInTheDocument();
    expect(screen.getByText("Legend:")).toBeInTheDocument();
  });

  it("swaps the account list for the Choose Game screen", () => {
    renderApp({ chooseGameOpen: true, ...selecting([1]) });
    expect(screen.getByText("Choose Game")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Back/ })).toBeInTheDocument();
  });

  it("shows the batch action bar only with a selection and no Choose Game", () => {
    renderApp();
    expect(screen.queryByRole("button", { name: "Clear" })).not.toBeInTheDocument();

    cleanup();
    renderApp(selecting([1, 2]));
    expect(screen.getByRole("button", { name: "Clear" })).toBeInTheDocument();

    cleanup();
    renderApp({ chooseGameOpen: true, ...selecting([1, 2]) });
    expect(screen.queryByRole("button", { name: "Clear" })).not.toBeInTheDocument();
  });

  it("opens the detail sidebar only for exactly one selected account", () => {
    renderApp({ sidebarOpen: true, ...selecting([1, 2]) });
    expect(screen.queryByText("Account Details")).not.toBeInTheDocument();

    cleanup();
    renderApp({ sidebarOpen: false, ...selecting([1]) });
    expect(screen.queryByText("Account Details")).not.toBeInTheDocument();
  });
});

describe("App — error banner, toasts and the generic modal", () => {
  it("shows a dismissible error banner", async () => {
    const store = renderApp({ error: "Something went wrong" });
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close Roblox" })).not.toBeInTheDocument();

    const banner = screen.getByText("Something went wrong").parentElement as HTMLElement;
    await userEvent.click(banner.querySelectorAll("button")[0]);
    expect(store.setError).toHaveBeenCalledWith(null);
  });

  it("offers Close Roblox for a multi-Roblox failure", async () => {
    const store = renderApp({ error: "Failed to enable Multi Roblox" });
    await userEvent.click(screen.getByRole("button", { name: "Close Roblox" }));
    expect(store.killAllRobloxProcesses).toHaveBeenCalledTimes(1);
  });

  it("stacks the toasts", () => {
    renderApp({ toasts: ["Copied 2 cookies", "Added roboduck"] });
    expect(screen.getByText("Copied 2 cookies")).toBeInTheDocument();
    expect(screen.getByText("Added roboduck")).toBeInTheDocument();
  });

  it("renders the generic text modal and closes it", async () => {
    const store = renderApp({ modal: { title: "Auth ticket", content: "ticket-body" } });
    expect(screen.getByText("Auth ticket")).toBeInTheDocument();
    expect(screen.getByText("ticket-body")).toBeInTheDocument();

    await userEvent.click(screen.getByText("Auth ticket").parentElement!.querySelector("button")!);
    expect(store.closeModal).toHaveBeenCalledTimes(1);
  });
});

describe("App — dialog routing", () => {
  it("keeps every dialog closed by default", () => {
    renderApp();
    expect(screen.queryByText("Settings")).not.toBeInTheDocument();
    expect(screen.queryByText("Roblox Versions")).not.toBeInTheDocument();
  });

  it("opens the versions dialog from store state", async () => {
    renderApp({ versionsDialogOpen: true });
    expect(await screen.findByText("Roblox Versions")).toBeInTheDocument();
  });

  it("opens the first-run walkthrough from store state", () => {
    renderApp({ firstRunWalkthroughOpen: true });
    // The walkthrough owns the screen, so the update check is skipped.
    expect(screen.queryByText("Loading...")).not.toBeInTheDocument();
  });
});
