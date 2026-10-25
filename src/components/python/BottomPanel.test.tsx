import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { BottomPanel, PANEL_DEFAULT_HEIGHT, PANEL_MIN_HEIGHT, PANEL_STATE_KEY } from "./BottomPanel";
import type { PythonRunInfo } from "./pythonApi";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

const running: PythonRunInfo = {
  id: "r1",
  script: "farm.py",
  accounts: [1],
  mode: "one_at_a_time",
  trigger: { kind: "loop" },
  source: "app",
  state: "running",
  startedMs: 1,
  endedMs: null,
  nextRunMs: null,
  error: null,
};

beforeEach(() => {
  resetTauriMocks();
  window.localStorage.clear();
  setStore({ accounts: [] });
  setInvokeHandler((cmd) => (cmd === "python_get_logs" ? [] : undefined));
});
afterEach(cleanup);

describe("BottomPanel", () => {
  it("shows the terminal first and the running scripts on the other tab, with the count", async () => {
    const user = userEvent.setup();
    render(<BottomPanel runs={[running]} onToast={() => {}} />);
    expect(screen.getByRole("tab", { name: "Terminal" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: /Running/ })).toHaveTextContent("1");
    expect(screen.getByRole("region", { name: "Python terminal" })).toBeVisible();

    await user.click(screen.getByRole("tab", { name: /Running/ }));
    expect(screen.getByRole("region", { name: "Running" })).toBeVisible();
    expect(screen.getByRole("region", { name: "Python terminal", hidden: true })).not.toBeVisible();
  });

  it("collapses, expands and remembers it", async () => {
    const user = userEvent.setup();
    const first = render(<BottomPanel runs={[]} onToast={() => {}} />);
    await user.click(screen.getByRole("button", { name: "Collapse the panel" }));
    expect(screen.getByTestId("panel-body")).not.toBeVisible();
    expect(JSON.parse(window.localStorage.getItem(PANEL_STATE_KEY) ?? "{}").collapsed).toBe(true);
    first.unmount();

    render(<BottomPanel runs={[]} onToast={() => {}} />);
    expect(screen.getByTestId("panel-body")).not.toBeVisible();
    await userEvent.setup().click(screen.getByRole("button", { name: "Expand the panel" }));
    expect(screen.getByTestId("panel-body")).toBeVisible();
  });

  it("resizes with the keyboard inside its limits and remembers the height", () => {
    render(<BottomPanel runs={[]} onToast={() => {}} />);
    const handle = screen.getByRole("separator", { name: "Resize the panel" });
    expect(handle).toHaveAttribute("aria-valuenow", String(PANEL_DEFAULT_HEIGHT));
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    expect(Number(handle.getAttribute("aria-valuenow"))).toBeGreaterThan(PANEL_DEFAULT_HEIGHT);
    for (let i = 0; i < 40; i += 1) fireEvent.keyDown(handle, { key: "ArrowDown" });
    expect(handle).toHaveAttribute("aria-valuenow", String(PANEL_MIN_HEIGHT));
    expect(JSON.parse(window.localStorage.getItem(PANEL_STATE_KEY) ?? "{}").height).toBe(PANEL_MIN_HEIGHT);
  });
});
