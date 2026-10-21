import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { PythonTerminal } from "./PythonTerminal";
import { MAX_LOG_LINES, createPythonLogStore, type PythonLogStore } from "./pythonLogStore";
import type { PythonLogLine } from "./pythonApi";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

function line(overrides: Partial<PythonLogLine> = {}): PythonLogLine {
  return {
    runId: "r1",
    userId: 1001,
    level: "log",
    message: "hello",
    code: null,
    hint: null,
    ts: 1_700_000_000_000,
    ...overrides,
  };
}

let backlog: PythonLogLine[] = [];
let logStore: PythonLogStore;

function renderTerminal() {
  return render(<PythonTerminal store={logStore} runs={[]} />);
}

beforeEach(() => {
  resetTauriMocks();
  backlog = [];
  logStore = createPythonLogStore();
  setStore({ accounts: [makeAccount({ UserID: 1001, Username: "Main_01" }), makeAccount({ UserID: 1002, Username: "Alt_02" })] });
  setInvokeHandler((cmd) => (cmd === "python_get_logs" ? backlog : undefined));
});

afterEach(cleanup);

describe("PythonTerminal", () => {
  it("shows the backlog and the lines that arrive live", async () => {
    backlog = [line({ message: "from the backlog" })];
    renderTerminal();
    expect(await screen.findByText("from the backlog")).toBeInTheDocument();

    act(() => emitTauriEvent("python-log", line({ message: "arrived live", ts: 1_700_000_001_000 })));
    expect(await screen.findByText("arrived live")).toBeInTheDocument();
  });

  it("names the account on each line", async () => {
    backlog = [line({ message: "clicked", userId: 1002 })];
    renderTerminal();
    await screen.findByText("clicked");
    expect(screen.getAllByText(/Alt_02/).length).toBeGreaterThan(0);
  });

  it("filters by level", async () => {
    backlog = [line({ message: "plain output" }), line({ level: "error", message: "it exploded" })];
    renderTerminal();
    await screen.findByText("it exploded");

    await userEvent.click(screen.getByRole("button", { name: "error" }));
    expect(screen.queryByText("it exploded")).not.toBeInTheDocument();
    expect(screen.getByText("plain output")).toBeInTheDocument();
  });

  it("searches the message text", async () => {
    backlog = [line({ message: "found the button" }), line({ message: "waiting for pixel" })];
    renderTerminal();
    await screen.findByText("found the button");

    await userEvent.type(screen.getByLabelText("Search the log"), "PIXEL");
    expect(screen.queryByText("found the button")).not.toBeInTheDocument();
    expect(screen.getByText("waiting for pixel")).toBeInTheDocument();
  });

  it("filters by account", async () => {
    backlog = [line({ message: "one", userId: 1001 }), line({ message: "two", userId: 1002 })];
    renderTerminal();
    await screen.findByText("two");

    await userEvent.click(screen.getByRole("button", { name: "Filter by account" }));
    await userEvent.click(screen.getByRole("button", { name: "Alt_02" }));
    expect(screen.queryByText("one")).not.toBeInTheDocument();
    expect(screen.getByText("two")).toBeInTheDocument();
  });

  it("filters by run", async () => {
    backlog = [line({ message: "first run", runId: "r1" }), line({ message: "second run", runId: "r2" })];
    renderTerminal();
    await screen.findByText("second run");

    await userEvent.click(screen.getByRole("button", { name: "Filter by run" }));
    await userEvent.click(screen.getByRole("button", { name: "r2" }));
    expect(screen.queryByText("first run")).not.toBeInTheDocument();
    expect(screen.getByText("second run")).toBeInTheDocument();
  });

  it("highlights a blocked line with its code and hint", async () => {
    backlog = [
      line({
        level: "blocked",
        message: "Blocked: Main_01's window is not in front. Input only goes to the focused window.",
        code: "not_foreground",
        hint: 'Wrap input in "with account.focused():".',
      }),
    ];
    renderTerminal();
    const message = await screen.findByText(/window is not in front/);
    const row = message.closest("[data-level]") as HTMLElement;
    expect(row).toHaveAttribute("data-level", "blocked");
    expect(within(row).getByText("not_foreground")).toBeInTheDocument();
    expect(within(row).getByText(/Wrap input in "with account.focused\(\):"/)).toBeInTheDocument();
  });

  it("pauses following when scrolled up and offers Jump to latest", async () => {
    backlog = [line({ message: "a" }), line({ message: "b" })];
    renderTerminal();
    await screen.findByText("b");
    expect(screen.queryByRole("button", { name: /Jump to latest/ })).not.toBeInTheDocument();

    const log = screen.getByRole("log");
    Object.defineProperty(log, "scrollHeight", { configurable: true, value: 1000 });
    Object.defineProperty(log, "clientHeight", { configurable: true, value: 200 });
    log.scrollTop = 100;
    fireEvent.scroll(log);
    const jump = await screen.findByRole("button", { name: /Jump to latest/ });

    await userEvent.click(jump);
    expect(screen.queryByRole("button", { name: /Jump to latest/ })).not.toBeInTheDocument();
    expect(log.scrollTop).toBe(1000);
  });

  it("copies only the visible lines", async () => {
    backlog = [line({ message: "keep me" }), line({ level: "warn", message: "drop me" })];
    renderTerminal();
    await screen.findByText("drop me");
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });

    await userEvent.click(screen.getByRole("button", { name: "warn" }));
    await userEvent.click(screen.getByRole("button", { name: /Copy visible/ }));

    expect(writeText).toHaveBeenCalledTimes(1);
    const copied = (writeText.mock.calls[0] as unknown as [string])[0];
    expect(copied).toContain("keep me");
    expect(copied).not.toContain("drop me");
  });

  it("clears the terminal", async () => {
    backlog = [line({ message: "soon gone" })];
    renderTerminal();
    await screen.findByText("soon gone");
    await userEvent.click(screen.getByRole("button", { name: /Clear/ }));
    await waitFor(() => expect(screen.queryByText("soon gone")).not.toBeInTheDocument());
  });
});

describe("pythonLogStore", () => {
  it("keeps only the last 5000 lines", () => {
    const store = createPythonLogStore();
    store.appendMany(Array.from({ length: MAX_LOG_LINES + 25 }, (_, i) => line({ message: `m${i}`, ts: i })));
    const lines = store.getLines();
    expect(lines).toHaveLength(MAX_LOG_LINES);
    expect(lines[0].message).toBe("m25");
    expect(lines[lines.length - 1].message).toBe(`m${MAX_LOG_LINES + 24}`);

    store.append(line({ message: "newest", ts: 99_999 }));
    expect(store.getLines()).toHaveLength(MAX_LOG_LINES);
    expect(store.getLines()[0].message).toBe("m26");
  });

  it("does not repeat a line that is in the backlog and also arrived live", async () => {
    const dup = line({ message: "dup", ts: 5 });
    backlog = [dup];
    const store = createPythonLogStore();
    const off = store.connect();
    emitTauriEvent("python-log", dup);
    await waitFor(() => expect(store.getLines().length).toBeGreaterThan(0));
    expect(store.getLines().filter((l) => l.message === "dup")).toHaveLength(1);
    off();
  });
});

describe("isolation from the app Console", () => {
  it("never imports anything from components/console", () => {
    const sources = import.meta.glob(["./*.ts", "./*.tsx", "./widgets/*.tsx"], {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>;
    const files = Object.entries(sources).filter(([path]) => !path.endsWith(".test.ts") && !path.endsWith(".test.tsx"));
    expect(files.length).toBeGreaterThan(5);
    for (const [path, source] of files) {
      const imports = source.match(/from\s+["'][^"']+["']/g) ?? [];
      for (const statement of imports) {
        expect(statement, `${path} must not import from the app Console`).not.toMatch(/console/i);
      }
    }
  });
});
