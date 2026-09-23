import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { StatusBar } from "./StatusBar";
import {
  defaultSettings,
  groupAccounts,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import type { StoreValue } from "../../store";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "ann" }),
  makeAccount({ UserID: 2, Username: "bob" }),
  makeAccount({ UserID: 3, Username: "cid" }),
];

function renderBar(overrides: Partial<StoreValue> = {}) {
  setStore({ accounts: ACCOUNTS, ...overrides });
  render(<StatusBar />);
}

/** Reads the text of the element that directly holds `value`. */
function lineWith(value: string): string {
  const el = screen.getAllByText(value)[0];
  return (el.parentElement?.textContent ?? "").replace(/\s+/g, " ").trim();
}

afterEach(cleanup);

describe("StatusBar", () => {
  it("shows the total account count", () => {
    renderBar();
    expect(lineWith("3")).toContain("3 accounts");
  });

  it("uses the singular form for one account", () => {
    setStore({ accounts: [ACCOUNTS[0]] });
    render(<StatusBar />);
    expect(lineWith("1")).toContain("1 account");
  });

  it("shows filtered / total while a search is active", () => {
    renderBar({ searchQuery: "an", groups: groupAccounts([ACCOUNTS[0]]) });
    expect(lineWith("1")).toContain("1 / 3 accounts");
  });

  it("shows the selected count only when something is selected", () => {
    renderBar({ selectedIds: new Set([1, 2]) });
    expect(screen.getByText("selected")).toBeInTheDocument();
    expect(lineWith("2")).toContain("2 selected");
  });

  it("hides the online/in-game counters when ShowPresence is off", () => {
    renderBar({ presenceByUserId: new Map([[1, 2]]) });
    // Only the legend entry remains — no counter.
    expect(screen.getAllByText("online")).toHaveLength(1);
    expect(screen.getAllByText("in game")).toHaveLength(1);
  });

  it("counts online and in-game accounts when ShowPresence is on", () => {
    const settings = defaultSettings();
    settings.General.ShowPresence = "true";
    renderBar({
      settings,
      presenceByUserId: new Map([
        [1, 1],
        [2, 2],
        [3, 0],
      ]),
    });
    // 2 accounts are at least online, 1 of them is in game.
    expect(lineWith("2")).toContain("2 online");
    expect(lineWith("1")).toContain("1 in game");
  });

  it("reports how many clients this app launched", () => {
    renderBar({ launchedByProgram: new Set([1, 2]) });
    expect(lineWith("2")).toContain("2 launched");
  });

  it("stays silent about botting while it is not running", () => {
    renderBar({ bottingStatus: makeBottingStatus({ active: false }) });
    expect(screen.queryByText("botting")).not.toBeInTheDocument();
  });

  it("counts down to the next botting restart", () => {
    renderBar({
      bottingStatus: makeBottingStatus({
        active: true,
        userIds: [1],
        accounts: [
          {
            userId: 1,
            isPlayer: false,
            disconnected: false,
            phase: "waiting",
            retryCount: 0,
            nextRestartAtMs: Date.now() + 90_000,
            playerGraceUntilMs: null,
            lastError: null,
          },
        ],
      }),
    });
    expect(screen.getByText("botting")).toBeInTheDocument();
    expect(screen.getByText(/next 1:(29|30)/)).toBeInTheDocument();
  });

  it("shows a dash when botting has no scheduled restart", () => {
    renderBar({ bottingStatus: makeBottingStatus({ active: true, userIds: [1] }) });
    expect(screen.getByText("next -")).toBeInTheDocument();
  });

  it("always shows the status-dot legend", () => {
    renderBar();
    for (const label of ["invalid", "aged", "launched", "online", "in game", "studio"]) {
      expect(screen.getAllByText(label).length).toBeGreaterThan(0);
    }
  });
});
