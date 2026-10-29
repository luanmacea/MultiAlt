import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EditorView } from "@codemirror/view";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { CodeTab, discardConfirmText, type OpenRequest } from "./CodeTab";
import { NOT_RUNNABLE_TEXT } from "./RunControls";
import { TREE_POLL_MS } from "./useWorkspaceTree";
import { findNode } from "./workspaceTree";
import type { FsNode, PythonRunConfig, PythonRunInfo } from "./pythonApi";
import { makeWorkspaceTree } from "../../test-utils/pythonWorkspace";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

let tree: FsNode;
let contents: Record<string, string>;
let savedConfigs: Record<string, PythonRunConfig>;

const onToast = vi.fn();
const onOpenDeveloper = vi.fn();

function draw(options: { runs?: PythonRunInfo[]; installed?: boolean; request?: OpenRequest | null; developerMode?: boolean } = {}) {
  const element = (request: OpenRequest | null) => (
    <CodeTab
      visible
      openRequest={request}
      runtimeInstalled={options.installed ?? true}
      runs={options.runs ?? []}
      developerMode={options.developerMode ?? false}
      onOpenDeveloper={onOpenDeveloper}
      onToast={onToast}
    />
  );
  const view = render(element(options.request ?? null));
  return { ...view, request: (next: OpenRequest) => view.rerender(element(next)) };
}

function editorView(): EditorView {
  const dom = document.querySelector<HTMLElement>(".cm-editor");
  const found = dom ? EditorView.findFromDOM(dom) : null;
  if (!found) throw new Error("no editor");
  return found;
}

function type(text: string) {
  const view = editorView();
  act(() => {
    view.dispatch({ changes: { from: view.state.doc.length, insert: text }, userEvent: "input.type" });
  });
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((call) => call[0] === cmd).map((call) => call[1]);

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  window.localStorage.clear();
  onToast.mockClear();
  onOpenDeveloper.mockClear();
  tree = makeWorkspaceTree();
  contents = {
    "scripts/farm.py": "print('farm')\n",
    "scripts/example_walk_jump.py": "# example\n",
    "lib/helpers.py": "def walk_forward(account, seconds=1.0):\n    pass\n",
    "examples/walk_and_jump.py": "# example\n",
    "multialt/defaults/runner.py": "def run(): pass\n",
  };
  savedConfigs = {};
  setStore({ accounts: [makeAccount({ UserID: 1001, Username: "Main_01" })] });
  setInvokeHandler((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "python_fs_tree":
        return structuredClone(tree);
      case "python_fs_read":
        return contents[String(a.path)] ?? "";
      case "python_fs_read_image":
        return "data:image/png;base64,AAAA";
      case "python_fs_write":
        contents[String(a.path)] = String(a.content);
        return undefined;
      case "python_get_run_configs":
        return savedConfigs;
      case "python_save_run_config":
        savedConfigs[(a.config as PythonRunConfig).script] = a.config as PythonRunConfig;
        return undefined;
      case "python_start_run":
        return "r9";
      case "python_get_logs":
        return [];
      default:
        return undefined;
    }
  });
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

async function openFromExplorer(user: ReturnType<typeof userEvent.setup>, name: string | RegExp) {
  await user.click(await screen.findByRole("treeitem", { name }));
}

describe("CodeTab editor and tabs", () => {
  it("opens a file from the Explorer into a tab and shows its text", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toBe("print('farm')\n"));
    expect(screen.getByRole("tab", { name: /farm\.py/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("textbox", { name: "Source of farm.py" })).toBeInTheDocument();
  });

  it("opens the file asked for from outside, once", async () => {
    const { request } = draw();
    request({ path: "lib/helpers.py", seq: 1 });
    await waitFor(() => expect(screen.getByRole("tab", { name: /helpers\.py/ })).toBeInTheDocument());
    request({ path: "lib/helpers.py", seq: 2 });
    expect(screen.getAllByRole("tab", { name: /helpers\.py/ })).toHaveLength(1);
  });

  it("marks a file as unsaved when edited and saves it with the Save button", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("farm"));
    expect(screen.queryByRole("img", { name: "Unsaved changes" })).not.toBeInTheDocument();

    type("# more\n");
    expect(screen.getByRole("img", { name: "Unsaved changes" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^Save/ }));
    await waitFor(() => expect(calls("python_fs_write")).toEqual([{ path: "scripts/farm.py", content: "print('farm')\n# more\n" }]));
    await waitFor(() => expect(screen.queryByRole("img", { name: "Unsaved changes" })).not.toBeInTheDocument());
  });

  it("saves with Ctrl+S inside the editor", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("farm"));
    type("# x\n");
    fireEvent.keyDown(editorView().contentDOM, { key: "s", code: "KeyS", ctrlKey: true });
    await waitFor(() => expect(calls("python_fs_write")).toHaveLength(1));
  });

  it("asks before closing a tab with unsaved edits and keeps it when declined", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("farm"));
    type("# edit\n");

    promptAnswers.confirm = false;
    await user.click(screen.getByRole("button", { name: "Close farm.py" }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(confirmMock.mock.calls[0][0]).toBe(discardConfirmText("farm.py"));
    expect(screen.getByRole("tab", { name: /farm\.py/ })).toBeInTheDocument();

    promptAnswers.confirm = true;
    await user.click(screen.getByRole("button", { name: "Close farm.py" }));
    await waitFor(() => expect(screen.queryByRole("tab", { name: /farm\.py/ })).not.toBeInTheDocument());
  });

  it("closes a clean tab without asking", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(screen.getByRole("tab", { name: /farm\.py/ })).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Close farm.py" }));
    expect(confirmMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("tab", { name: /farm\.py/ })).not.toBeInTheDocument();
  });

  it("keeps edits per file when switching between tabs", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("farm"));
    type("# kept\n");
    await openFromExplorer(user, "helpers.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("walk_forward"));
    await user.click(screen.getByRole("tab", { name: /farm\.py/ }));
    expect(editorView().state.doc.toString()).toBe("print('farm')\n# kept\n");
  });

  it("previews an image from assets/", async () => {
    const user = userEvent.setup();
    draw();
    await user.click(await screen.findByRole("treeitem", { name: "assets" }));
    await user.click(await screen.findByRole("treeitem", { name: "play_button.png" }));
    const image = await screen.findByRole("img", { name: "play_button.png" });
    expect(image).toHaveAttribute("src", "data:image/png;base64,AAAA");
    expect(document.querySelector(".cm-editor")).toBeNull();
  });
});

describe("CodeTab read-only files", () => {
  it("explains an example is read-only and offers to copy it to scripts", async () => {
    const user = userEvent.setup();
    draw();
    await user.click(await screen.findByRole("treeitem", { name: "examples" }));
    await user.click(await screen.findByRole("treeitem", { name: /walk_and_jump\.py/ }));
    const banner = await screen.findByRole("note");
    expect(banner).toHaveTextContent("cannot be edited");
    expect(screen.getByRole("textbox", { name: "Source of walk_and_jump.py" })).toHaveAttribute("aria-readonly", "true");
    expect(screen.queryByRole("button", { name: /^Save/ })).not.toBeInTheDocument();

    await user.click(within(banner).getByRole("button", { name: "Copy to scripts" }));
    await waitFor(() => expect(calls("python_fs_create")).toEqual([{ path: "scripts/walk_and_jump.py", kind: "file" }]));
    expect(calls("python_fs_write")).toEqual([{ path: "scripts/walk_and_jump.py", content: "# example\n" }]);
    await waitFor(() => expect(document.querySelector('[role="tab"][title="scripts/walk_and_jump.py"]')).not.toBeNull());
  });

  it("explains a locked default block and points to the Developer tab", async () => {
    const user = userEvent.setup();
    draw();
    await user.click(await screen.findByRole("treeitem", { name: "multialt" }));
    await user.click(await screen.findByRole("treeitem", { name: "defaults" }));
    await user.click(await screen.findByRole("treeitem", { name: /runner\.py/ }));
    const banner = await screen.findByRole("note");
    expect(banner).toHaveTextContent("locked");
    await user.click(within(banner).getByRole("button", { name: "Open the Developer tab" }));
    expect(onOpenDeveloper).toHaveBeenCalled();
  });

  it("lets a default block be edited when Developer mode makes it writable", async () => {
    const user = userEvent.setup();
    tree = makeWorkspaceTree({ developerMode: true });
    draw({ developerMode: true });
    await user.click(await screen.findByRole("treeitem", { name: "multialt" }));
    await user.click(await screen.findByRole("treeitem", { name: "defaults" }));
    await user.click(await screen.findByRole("treeitem", { name: /runner\.py/ }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Source of runner.py" })).toHaveAttribute("aria-readonly", "false"));
    expect(screen.getByRole("note")).toHaveTextContent("Developer mode is on");
  });
});

describe("CodeTab Run", () => {
  it("enables Run for a script directly in scripts/", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(screen.getByRole("button", { name: "Run" })).toBeEnabled());
    expect(screen.queryByText(NOT_RUNNABLE_TEXT)).not.toBeInTheDocument();
  });

  it("disables Run and says why for a file that is not directly in scripts/", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "helpers.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("walk_forward"));
    expect(screen.getByRole("button", { name: "Run" })).toBeDisabled();
    expect(screen.getByText(NOT_RUNNABLE_TEXT)).toBeInTheDocument();
  });

  it("asks for accounts when none are picked instead of starting", async () => {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await user.click(await screen.findByRole("button", { name: "Run" }));
    await waitFor(() => expect(screen.getByRole("dialog", { name: "Run settings" })).toBeInTheDocument());
    expect(calls("python_start_run")).toEqual([]);
  });

  it("starts the run with the saved accounts", async () => {
    const user = userEvent.setup();
    savedConfigs = { "farm.py": { script: "farm.py", accounts: [1001], mode: "one_at_a_time", trigger: { kind: "loop" } } };
    draw();
    await openFromExplorer(user, "farm.py");
    await user.click(await screen.findByRole("button", { name: "Run" }));
    await waitFor(() => expect(calls("python_start_run")).toHaveLength(1));
    expect(calls("python_start_run")[0]).toEqual({ config: savedConfigs["farm.py"] });
  });

  it("shows Stop while the script runs and blocks a second Run", async () => {
    const user = userEvent.setup();
    const running: PythonRunInfo = {
      id: "r1",
      script: "farm.py",
      accounts: [1001],
      mode: "one_at_a_time",
      trigger: { kind: "loop" },
      source: "app",
      state: "running",
      startedMs: 1,
      endedMs: null,
      nextRunMs: null,
      error: null,
    };
    draw({ runs: [running] });
    await openFromExplorer(user, "farm.py");
    await user.click(await screen.findByRole("button", { name: "Stop" }));
    await waitFor(() => expect(calls("python_stop_run")).toEqual([{ runId: "r1" }]));
    expect(screen.getByRole("button", { name: "Run" })).toBeDisabled();
  });

  it("keeps Run off until Python is installed", async () => {
    const user = userEvent.setup();
    draw({ installed: false });
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(screen.getByRole("tab", { name: /farm\.py/ })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Run" })).toBeDisabled();
  });
});

describe("CodeTab external changes", () => {
  const SLOW = { timeout: TREE_POLL_MS * 3 };

  async function setup() {
    const user = userEvent.setup();
    draw();
    await openFromExplorer(user, "farm.py");
    await waitFor(() => expect(editorView().state.doc.toString()).toContain("farm"));
  }

  function touchOnDisk(text: string) {
    contents["scripts/farm.py"] = text;
    findNode(tree, "scripts/farm.py")!.modifiedMs += 500;
  }

  it("reloads a clean file that an outside tool changed", async () => {
    await setup();
    touchOnDisk("print('by ai')\n");
    await waitFor(() => expect(editorView().state.doc.toString()).toBe("print('by ai')\n"), SLOW);
    expect(screen.getByText("Changed on disk - reloaded")).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "Unsaved changes" })).not.toBeInTheDocument();
  }, 15000);

  it("asks which side wins when the file changed while there are unsaved edits", async () => {
    await setup();
    type("# mine\n");
    touchOnDisk("print('by ai')\n");
    const alert = await screen.findByRole("alert", {}, SLOW);
    expect(alert).toHaveTextContent("changed on disk");
    expect(editorView().state.doc.toString()).toBe("print('farm')\n# mine\n");

    fireEvent.click(within(alert).getByRole("button", { name: "Load the disk version" }));
    await waitFor(() => expect(editorView().state.doc.toString()).toBe("print('by ai')\n"));
  }, 15000);

  it("does not report its own save as an outside change", async () => {
    await setup();
    type("# mine\n");
    fireEvent.click(screen.getByRole("button", { name: /^Save/ }));
    await waitFor(() => expect(calls("python_fs_write")).toHaveLength(1));
    findNode(tree, "scripts/farm.py")!.modifiedMs += 500;
    const reads = calls("python_fs_read").length;
    await waitFor(() => expect(calls("python_fs_read").length).toBeGreaterThan(reads), SLOW);
    expect(screen.queryByText("Changed on disk - reloaded")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  }, 15000);
});
