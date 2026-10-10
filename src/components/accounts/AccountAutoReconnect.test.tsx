import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { AccountAutoReconnect } from "./AccountAutoReconnect";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import type { Account, AutoReconnectEntry, PlatformCapabilities } from "../../types";

const WINDOWS = { os: "windows" } as PlatformCapabilities;

function renderSection(account: Account, extra: Record<string, unknown> = {}) {
  const store = setStore({
    accounts: [account],
    selectedAccounts: [account],
    platformCapabilities: WINDOWS,
    ...extra,
  });
  render(<AccountAutoReconnect account={account} />);
  return store;
}

function saved(store: ReturnType<typeof setStore>): Account {
  return (store.updateAccount as unknown as { mock: { calls: [Account][] } }).mock.calls[0][0];
}

function reconnectEntry(partial: Partial<AutoReconnectEntry> = {}): AutoReconnectEntry {
  return {
    userId: 7,
    phase: "gaveUp",
    attempt: 5,
    maxAttempts: 5,
    nextAttemptAtMs: null,
    reason: null,
    error: null,
    drop: { kind: "crashed", reason: null, code: null, message: null, sinceMs: 0 },
    ...partial,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  resetTauriMocks();
  setInvokeMap({ get_nexus_accounts: [] });
});

afterEach(cleanup);

describe("AccountAutoReconnect", () => {
  it("follows the default from Settings until the account changes it", () => {
    renderSection(makeAccount({ UserID: 7 }), { settings: { General: { AutoReconnect: "true" } } });
    expect(screen.getByRole("checkbox", { name: "Reconnect automatically if it drops" })).toBeChecked();
    expect(screen.getByText("Following the default from Settings › General (on).")).toBeInTheDocument();
  });

  /** "Seguindo o padrão" sem dizer qual era o padrão: não dava para saber se estava ligado. */
  it("says whether the default it follows is on or off", () => {
    renderSection(makeAccount({ UserID: 7 }));
    expect(screen.getByText("Following the default from Settings › General (off).")).toBeInTheDocument();
  });

  it("turning it on writes the account field and keeps the other fields", async () => {
    const store = renderSection(makeAccount({ UserID: 7, Fields: { RobloxVersion: "LIVE:abc" } }));
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    await userEvent.click(screen.getByRole("checkbox"));
    const account = saved(store);
    expect(account.Fields.AutoReconnect).toBe("true");
    expect(account.Fields.RobloxVersion).toBe("LIVE:abc");
  });

  it("the account's own choice wins over the default, and can go back to it", async () => {
    const store = renderSection(makeAccount({ UserID: 7, Fields: { AutoReconnect: "false" } }), {
      settings: { General: { AutoReconnect: "true" } },
    });
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    await userEvent.click(screen.getByText("Use the default from Settings › General (on)"));
    expect(saved(store).Fields).not.toHaveProperty("AutoReconnect");
  });

  /**
   * `reconnect_enabled` (commands/reconnect.rs) também liga a conta com o
   * AutoRelaunch do Nexus, mesmo com o campo da conta em "false": o painel
   * dizia "desligado" com a conta reconectando.
   */
  it("says when Nexus AutoRelaunch keeps it on, even with the account set to off", async () => {
    setInvokeMap({ get_nexus_accounts: [{ username: "Seven", auto_relaunch: true }] });
    renderSection(makeAccount({ UserID: 7, Username: "seven", Fields: { AutoReconnect: "false" } }), {
      autoReconnect: [reconnectEntry({ phase: "waiting", attempt: 1, nextAttemptAtMs: Date.now() + 30_000 })],
    });
    expect(
      await screen.findByText("Nexus AutoRelaunch is on for this account, so it reconnects even with this off.")
    ).toBeInTheDocument();
    expect(screen.getByRole("checkbox")).toBeChecked();
    expect(screen.getByRole("checkbox")).toBeDisabled();
  });

  it("does not blame Nexus when its AutoRelaunch is off for the account", async () => {
    setInvokeMap({ get_nexus_accounts: [{ username: "seven", auto_relaunch: false }] });
    renderSection(makeAccount({ UserID: 7, Username: "seven", Fields: { AutoReconnect: "false" } }));
    await screen.findByRole("checkbox");
    expect(screen.queryByText(/Nexus AutoRelaunch/)).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox")).not.toBeChecked();
  });

  it("explains a reconnect still running after it was turned off", () => {
    renderSection(makeAccount({ UserID: 7, Fields: { AutoReconnect: "false" } }), {
      autoReconnect: [reconnectEntry({ phase: "launching", attempt: 2 })],
    });
    expect(screen.getByText("Turned off: it stops after the attempt in progress.")).toBeInTheDocument();
  });

  it("shows how the reconnect of this account is going, with the cause under it", () => {
    renderSection(makeAccount({ UserID: 7 }), {
      settings: { General: { AutoReconnect: "true" } },
      autoReconnect: [reconnectEntry({ error: "It did not get into the game in 2 minutes" })],
    });
    expect(screen.getByTestId("account-reconnect-status")).toHaveTextContent("Gave up after 5 tries");
    expect(screen.getByTestId("account-reconnect-detail")).toHaveTextContent(
      "It did not get into the game in 2 minutes"
    );
  });

  it("says windows opened from the website are never reconnected", () => {
    renderSection(makeAccount({ UserID: 7 }));
    expect(screen.getByText(/never .*opened from the website/i)).toBeInTheDocument();
  });

  it("is not shown outside Windows, where the Roblox log is not read", () => {
    const account = makeAccount({ UserID: 7 });
    setStore({ accounts: [account], platformCapabilities: { os: "macos" } as PlatformCapabilities });
    render(<AccountAutoReconnect account={account} />);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });
});
