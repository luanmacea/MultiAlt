import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { AccountAutoReconnect } from "./AccountAutoReconnect";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
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

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("AccountAutoReconnect", () => {
  it("follows the default from Settings until the account changes it", () => {
    renderSection(makeAccount({ UserID: 7 }), { settings: { General: { AutoReconnect: "true" } } });
    expect(screen.getByRole("checkbox", { name: "Reconnect automatically if it drops" })).toBeChecked();
    expect(screen.getByText("Following the default in Settings › General.")).toBeInTheDocument();
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
    await userEvent.click(screen.getByText("Use the default from Settings › General"));
    expect(saved(store).Fields).not.toHaveProperty("AutoReconnect");
  });

  it("shows how the reconnect of this account is going", () => {
    const entry: AutoReconnectEntry = {
      userId: 7,
      phase: "gaveUp",
      attempt: 5,
      maxAttempts: 5,
      nextAttemptAtMs: null,
      reason: null,
      error: null,
      drop: { kind: "crashed", reason: null, code: null, message: null, sinceMs: 0 },
    };
    renderSection(makeAccount({ UserID: 7 }), { autoReconnect: [entry] });
    expect(screen.getByTestId("account-reconnect-status")).toHaveTextContent("Gave up after 5 tries");
  });

  it("is not shown outside Windows, where the Roblox log is not read", () => {
    const account = makeAccount({ UserID: 7 });
    setStore({ accounts: [account], platformCapabilities: { os: "macos" } as PlatformCapabilities });
    render(<AccountAutoReconnect account={account} />);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });
});
