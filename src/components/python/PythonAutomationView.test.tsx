import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EditorView } from "@codemirror/view";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { PythonAutomationView, WARNING_DISMISSED_KEY } from "./PythonAutomationView";
import type { GlobalModule, PythonRunInfo } from "./pythonApi";
import { makeWorkspaceTree } from "../../test-utils/pythonWorkspace";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";

const MODULES: GlobalModule[] = [
  {
    path: "lib/helpers.py",
    module: "lib.helpers",
    pinned: false,
    modified: false,
    newDefaultAvailable: false,
    functions: [{ name: "walk_forward", signature: "walk_forward(account, seconds=1.0)", doc: "Hold W." }],
  },
];

function run(overrides: Partial<PythonRunInfo> = {}): PythonRunInfo {
  return {
    id: "r1",
    script: "farm.py",
    accounts: [1001],
    mode: "one_at_a_time",
    trigger: { kind: "manual" },
    source: "app",
    state: "running",
    startedMs: 1000,
    endedMs: null,
    nextRunMs: null,
    error: null,
    ...overrides,
  };
}

function editorView(): EditorView {
  const dom = document.querySelector<HTMLElement>(".cm-editor");
  const found = dom ? EditorView.findFromDOM(dom) : null;
  if (!found) throw new Error("no editor");
  return found;
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  window.localStorage.clear();
  setStore({ accounts: [makeAccount({ UserID: 1001, Username: "Main_01" })] });
  setInvokeHandler((cmd, args) => {
    switch (cmd) {
      case "python_runtime_status":
        return { installed: true, version: "3.12.7", pythonExe: "python.exe", packages: [], installing: false };
      case "python_fs_tree":
        return makeWorkspaceTree();
      case "python_fs_read":
        return `# ${(args as { path: string }).path}\n`;
      case "python_list_global_functions":
        return MODULES;
      case "python_list_runs":
      case "python_get_logs":
      case "python_get_dashboard":
        return [];
      case "python_get_run_configs":
        return {};
      default:
        return undefined;
    }
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  cleanup();
});

describe("PythonAutomationView warning", () => {
  it("is a yellow alert, not a red error", () => {
    render(<PythonAutomationView />);
    const warning = screen.getByTestId("python-warning");
    expect(warning).toHaveTextContent("Python scripts have full access to your PC. Only run scripts you trust.");
    expect(warning.className).toContain("amber");
    expect(warning.className).not.toContain("red");
  });

  it("goes away with the X and does not come back", async () => {
    const user = userEvent.setup();
    const first = render(<PythonAutomationView />);
    await user.click(screen.getByRole("button", { name: "Dismiss the warning" }));
    expect(screen.queryByTestId("python-warning")).not.toBeInTheDocument();
    expect(window.localStorage.getItem(WARNING_DISMISSED_KEY)).toBe("1");
    first.unmount();

    render(<PythonAutomationView />);
    expect(screen.queryByTestId("python-warning")).not.toBeInTheDocument();
  });

  it("still works when the browser storage is not available", async () => {
    const user = userEvent.setup();
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    render(<PythonAutomationView />);
    expect(screen.getByTestId("python-warning")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Dismiss the warning" }));
    expect(screen.queryByTestId("python-warning")).not.toBeInTheDocument();
  });
});

describe("PythonAutomationView tabs", () => {
  it("has the five tabs in order and starts on Code with the Explorer", async () => {
    render(<PythonAutomationView />);
    expect(screen.getAllByRole("tab").slice(0, 5).map((tab) => tab.textContent)).toEqual([
      "Code",
      "Global functions",
      "Dashboard",
      "Inspector",
      "Developer",
    ]);
    expect(screen.getByRole("tab", { name: "Code" })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText(/Python 3.12.7 installed/)).toBeInTheDocument();
    expect(await screen.findByRole("tree", { name: "Workspace files" })).toBeInTheDocument();
    expect(screen.getByRole("log")).toBeInTheDocument();
  });

  it("opens a function's file in the Code tab when it is clicked in Global functions", async () => {
    const user = userEvent.setup();
    render(<PythonAutomationView />);
    await user.click(screen.getByRole("tab", { name: "Global functions" }));
    await user.click(await screen.findByText("walk_forward(account, seconds=1.0)"));

    expect(screen.getByRole("tab", { name: "Code" })).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(editorView().state.doc.toString()).toBe("# lib/helpers.py\n"));
    expect(screen.getByRole("tab", { name: /helpers\.py/ })).toBeInTheDocument();
  });

  it("keeps unsaved edits when another tab is shown and Code comes back", async () => {
    const user = userEvent.setup();
    render(<PythonAutomationView />);
    await user.click(await screen.findByRole("treeitem", { name: "farm.py" }));
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("farm.py"));
    const view = editorView();
    act(() => {
      view.dispatch({ changes: { from: view.state.doc.length, insert: "# kept\n" }, userEvent: "input.type" });
    });
    expect(screen.getByRole("img", { name: "Unsaved changes" })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Developer" }));
    await user.click(screen.getByRole("tab", { name: "Code" }));
    expect(screen.getByRole("img", { name: "Unsaved changes" })).toBeInTheDocument();
    expect(editorView().state.doc.toString()).toContain("# kept");
  });

  it("keeps the Developer tab to the toggle and the terminal channel", async () => {
    const user = userEvent.setup();
    render(<PythonAutomationView />);
    await user.click(screen.getByRole("tab", { name: "Developer" }));
    expect(screen.getByRole("switch", { name: "Developer mode" })).toBeInTheDocument();
    expect(screen.queryByText("Default blocks")).not.toBeInTheDocument();
  });

  it("adds a run started from a terminal as soon as the backend announces it", async () => {
    render(<PythonAutomationView />);
    await screen.findByText(/Python 3.12.7 installed/);
    act(() => emitTauriEvent("python-run-update", run({ id: "t1", script: "ai_test.py", source: "terminal" })));
    expect(await screen.findByText("ai_test.py")).toBeInTheDocument();
    expect(screen.getByText("terminal")).toBeInTheDocument();
  });
});
