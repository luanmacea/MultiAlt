import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { DashboardView, WidgetView } from "./DashboardView";
import type { DashboardSection } from "./pythonApi";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const NOW = 1_700_000_000_000;

function section(overrides: Partial<DashboardSection> = {}): DashboardSection {
  return {
    script: "farm.py",
    layout: [
      { id: "accounts", type: "accounts", title: "Accounts", columns: ["Status", "Loops"] },
      { id: "total", type: "stat", title: "Total loops" },
      { id: "rate", type: "line", title: "Loops per hour" },
      { id: "state", type: "badge", title: "State" },
      { id: "goal", type: "progress", title: "Goal", max: 50 },
      { id: "notes", type: "text", title: "Notes" },
      { id: "recent", type: "table", title: "Recent", columns: ["Time", "Event"] },
      { id: "events", type: "log", title: "Events", max: 2 },
    ],
    values: {
      "accounts@1001": { Status: "Working", Loops: 3 },
      "accounts@1002": { Status: "Waiting", Loops: 1 },
      total: { value: 42, delta: 5 },
      state: { level: "warn", text: "Slow" },
      goal: 20,
      notes: "all fine",
      recent: [
        ["10:00", "joined"],
        ["10:05", "clicked"],
      ],
    },
    series: {
      rate: [
        [NOW, 10],
        [NOW + 60_000, 14],
        [NOW + 120_000, 12],
      ],
      events: [
        [NOW, "first"],
        [NOW + 1, "second"],
        [NOW + 2, "third"],
      ],
    },
    updatedMs: NOW,
    ...overrides,
  };
}

let sections: DashboardSection[];

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  sections = [section()];
  setStore({ accounts: [makeAccount({ UserID: 1001, Username: "Main_01" }), makeAccount({ UserID: 1002, Username: "Alt_02" })] });
  setInvokeHandler((cmd) => (cmd === "python_get_dashboard" ? sections : undefined));
});
afterEach(cleanup);

const widget = (id: string) => screen.getByTestId(`widget-${id}`);

describe("DashboardView", () => {
  it("explains farm_report() and the example when nothing was reported", async () => {
    sections = [];
    render(<DashboardView />);
    expect(await screen.findByText("Nothing reported yet")).toBeInTheDocument();
    expect(screen.getByText("dashboard.farm_report()")).toBeInTheDocument();
    expect(screen.getByText("examples/farm_dashboard.py")).toBeInTheDocument();
  });

  it("draws one section per script with every widget type", async () => {
    render(<DashboardView />);
    expect(await screen.findByRole("heading", { name: "farm.py" })).toBeInTheDocument();

    const accounts = within(widget("accounts"));
    expect(accounts.getByText("Main_01")).toBeInTheDocument();
    expect(accounts.getByText("Working")).toBeInTheDocument();
    expect(accounts.getByText("Alt_02")).toBeInTheDocument();
    expect(accounts.getByRole("columnheader", { name: "Loops" })).toBeInTheDocument();

    expect(within(widget("total")).getByText("42")).toBeInTheDocument();
    expect(within(widget("total")).getByLabelText("Change +5")).toBeInTheDocument();

    expect(within(widget("state")).getByText("Slow")).toHaveAttribute("data-level", "warn");

    const progress = within(widget("goal")).getByRole("progressbar");
    expect(progress).toHaveAttribute("aria-valuenow", "20");
    expect(within(widget("goal")).getByText("20 / 50")).toBeInTheDocument();

    expect(within(widget("notes")).getByText("all fine")).toBeInTheDocument();

    const table = within(widget("recent"));
    expect(table.getByRole("columnheader", { name: "Event" })).toBeInTheDocument();
    expect(table.getByText("clicked")).toBeInTheDocument();

    expect(within(widget("rate")).getByRole("img")).toHaveAccessibleName(/3 points, last value 12/);
    expect(within(widget("rate")).getByText("max 14")).toBeInTheDocument();
  });

  it("shows only the newest lines of a log widget", async () => {
    render(<DashboardView />);
    await screen.findByRole("heading", { name: "farm.py" });
    const log = within(widget("events"));
    expect(log.queryByText("first")).not.toBeInTheDocument();
    expect(log.getByText(/second/)).toBeInTheDocument();
    expect(log.getByText(/third/)).toBeInTheDocument();
  });

  it("updates a section when the backend pushes new values", async () => {
    render(<DashboardView />);
    await screen.findByRole("heading", { name: "farm.py" });
    act(() => emitTauriEvent("python-dashboard-update", section({ values: { ...section().values, total: { value: 99 } } })));
    expect(await within(widget("total")).findByText("99")).toBeInTheDocument();
  });

  it("adds a section for a new script", async () => {
    render(<DashboardView />);
    await screen.findByRole("heading", { name: "farm.py" });
    act(() => emitTauriEvent("python-dashboard-update", section({ script: "afk.py", layout: [], values: {}, series: {} })));
    expect(await screen.findByRole("heading", { name: "afk.py" })).toBeInTheDocument();
    expect(screen.getByText(/has not defined any widgets/)).toBeInTheDocument();
  });

  it("clears one section after confirmation", async () => {
    render(<DashboardView />);
    await screen.findByRole("heading", { name: "farm.py" });

    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: /Clear/ }));
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_clear_dashboard")).toBe(false);

    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: /Clear/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_clear_dashboard", { script: "farm.py" }));
    expect(await screen.findByText("Nothing reported yet")).toBeInTheDocument();
  });
});

describe("WidgetView", () => {
  const base = { values: {}, series: {}, accountName: (id: number) => `U${id}` };

  it("shows a small box for a widget type it does not know", () => {
    render(<WidgetView {...base} widget={{ id: "x", type: "hologram", title: "Mystery" }} />);
    expect(screen.getByText("Mystery")).toBeInTheDocument();
    expect(screen.getByText(/Unsupported widget \(hologram\)/)).toBeInTheDocument();
  });

  it("renders script strings as plain text, never as HTML", () => {
    const evil = '<img src=x onerror="alert(1)"><b>bold</b>';
    const { container } = render(
      <>
        <WidgetView {...base} widget={{ id: "t", type: "text", title: evil }} values={{ t: evil }} />
        <WidgetView {...base} widget={{ id: "s", type: "stat" }} values={{ s: evil }} />
        <WidgetView {...base} widget={{ id: "b", type: "badge" }} values={{ b: { level: "ok", text: evil } }} />
        <WidgetView
          {...base}
          widget={{ id: "tb", type: "table", columns: [evil] }}
          values={{ tb: [[evil]] }}
        />
      </>
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(screen.getAllByText(evil).length).toBeGreaterThanOrEqual(4);
  });

  it("treats unexpected values defensively", () => {
    render(
      <>
        <WidgetView {...base} widget={{ id: "p", type: "progress" }} values={{ p: "lots" }} />
        <WidgetView {...base} widget={{ id: "b", type: "badge" }} values={{ b: "fine" }} />
        <WidgetView {...base} widget={{ id: "tb", type: "table" }} values={{ tb: "nope" }} />
        <WidgetView {...base} widget={{ id: "l", type: "line" }} series={{ l: "bad" }} />
      </>
    );
    expect(screen.getAllByText(/Unsupported value/).length).toBe(3);
    expect(screen.getByText("No points yet")).toBeInTheDocument();
  });

  it("caps a table at 200 rows", () => {
    const rows = Array.from({ length: 300 }, (_, i) => [`row${i}`]);
    render(<WidgetView {...base} widget={{ id: "tb", type: "table", columns: ["A"] }} values={{ tb: rows }} />);
    expect(screen.getAllByRole("row")).toHaveLength(201);
  });

  it("clamps progress to its range", () => {
    render(<WidgetView {...base} widget={{ id: "p", type: "progress" }} values={{ p: 250 }} />);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  });
});
