import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { EXPLORER_OPEN_KEY, FileExplorer, deleteConfirmText } from "./FileExplorer";
import { makeWorkspaceTree } from "../../test-utils/pythonWorkspace";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const onOpen = vi.fn();
const onChanged = vi.fn(async () => {});
const onRenamed = vi.fn();
const onRemoved = vi.fn();
const onToast = vi.fn();

function renderExplorer(activePath: string | null = null) {
  return render(
    <FileExplorer
      tree={makeWorkspaceTree()}
      loaded
      activePath={activePath}
      onOpen={onOpen}
      onChanged={onChanged}
      onRenamed={onRenamed}
      onRemoved={onRemoved}
      onToast={onToast}
    />
  );
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((call) => call[0] === cmd).map((call) => call[1]);

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  window.localStorage.clear();
  for (const fn of [onOpen, onChanged, onRenamed, onRemoved, onToast]) fn.mockClear();
  setInvokeHandler((cmd) => (cmd === "python_fs_read" ? "print(1)\n" : undefined));
});
afterEach(cleanup);

describe("FileExplorer tree", () => {
  it("opens scripts and lib by default and keeps the others closed", () => {
    renderExplorer();
    expect(screen.getByRole("treeitem", { name: "farm.py" })).toBeInTheDocument();
    expect(screen.getByRole("treeitem", { name: "helpers.py" })).toBeInTheDocument();
    expect(screen.queryByRole("treeitem", { name: "play_button.png" })).not.toBeInTheDocument();
    expect(screen.queryByRole("treeitem", { name: "walk_and_jump.py" })).not.toBeInTheDocument();
  });

  it("expands and collapses a folder and remembers it", async () => {
    const user = userEvent.setup();
    const first = renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "assets" }));
    expect(screen.getByRole("treeitem", { name: "play_button.png" })).toBeInTheDocument();
    await user.click(screen.getByRole("treeitem", { name: "scripts" }));
    expect(screen.queryByRole("treeitem", { name: "farm.py" })).not.toBeInTheDocument();
    first.unmount();

    renderExplorer();
    expect(screen.getByRole("treeitem", { name: "play_button.png" })).toBeInTheDocument();
    expect(screen.queryByRole("treeitem", { name: "farm.py" })).not.toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem(EXPLORER_OPEN_KEY) ?? "[]")).toContain("assets");
  });

  it("opens a file on click", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "farm.py" }));
    expect(onOpen).toHaveBeenCalledWith("scripts/farm.py");
  });

  it("marks pinned blocks with a lock and examples as read-only", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "multialt" }));
    await user.click(screen.getByRole("treeitem", { name: "defaults" }));
    await user.click(screen.getByRole("treeitem", { name: "examples" }));
    expect(within(screen.getByRole("treeitem", { name: /runner\.py/ })).getByLabelText("Locked default block")).toBeInTheDocument();
    expect(within(screen.getByRole("treeitem", { name: /walk_and_jump\.py/ })).getByLabelText("Read-only")).toBeInTheDocument();
    expect(within(screen.getByRole("treeitem", { name: "farm.py" })).queryByLabelText("Read-only")).not.toBeInTheDocument();
    // The pinned folders are marked by the backend too, but only the block files get the lock.
    expect(within(screen.getByRole("treeitem", { name: "defaults" })).queryByLabelText("Locked default block")).not.toBeInTheDocument();
  });
});

describe("FileExplorer actions", () => {
  it("creates a file in the selected folder, adds .py and opens it", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "lib" }));
    await user.click(screen.getByRole("treeitem", { name: "lib" }));
    promptAnswers.prompt = "moves";
    await user.click(screen.getByRole("button", { name: "New file" }));
    await waitFor(() => expect(calls("python_fs_create")).toEqual([{ path: "lib/moves.py", kind: "file" }]));
    expect(onOpen).toHaveBeenCalledWith("lib/moves.py");
    expect(onChanged).toHaveBeenCalled();
  });

  it("starts a new script in scripts/ with a runnable starter", async () => {
    const user = userEvent.setup();
    renderExplorer();
    promptAnswers.prompt = "fresh";
    await user.click(screen.getByRole("button", { name: "New file" }));
    await waitFor(() => expect(calls("python_fs_create")).toEqual([{ path: "scripts/fresh.py", kind: "file" }]));
    const [write] = calls("python_fs_write") as { path: string; content: string }[];
    expect(write.path).toBe("scripts/fresh.py");
    expect(write.content).toContain("def main(account)");
  });

  it("creates a folder", async () => {
    const user = userEvent.setup();
    renderExplorer();
    promptAnswers.prompt = "moves";
    await user.click(screen.getByRole("button", { name: "New folder" }));
    await waitFor(() => expect(calls("python_fs_create")).toEqual([{ path: "scripts/moves", kind: "dir" }]));
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("refuses a name the backend would refuse, without calling it", async () => {
    const user = userEvent.setup();
    renderExplorer();
    promptAnswers.prompt = "../evil";
    await user.click(screen.getByRole("button", { name: "New file" }));
    await waitFor(() => expect(onToast).toHaveBeenCalledWith(expect.stringContaining("letters, numbers"), "warn"));
    expect(calls("python_fs_create")).toEqual([]);
  });

  it("renames a file inside its folder", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "farm.py" }));
    promptAnswers.prompt = "farm2.py";
    await user.click(screen.getByRole("button", { name: "Rename" }));
    await waitFor(() => expect(calls("python_fs_rename")).toEqual([{ from: "scripts/farm.py", to: "scripts/farm2.py" }]));
    expect(onRenamed).toHaveBeenCalledWith("scripts/farm.py", "scripts/farm2.py");
  });

  it("asks before moving a file to the trash, and does nothing when declined", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "farm.py" }));
    promptAnswers.confirm = false;
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(confirmMock.mock.calls[0][0]).toBe("Move farm.py to the workspace trash?");
    expect(deleteConfirmText("farm.py")).toBe("Move farm.py to the workspace trash?");
    expect(calls("python_fs_delete")).toEqual([]);

    promptAnswers.confirm = true;
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(calls("python_fs_delete")).toEqual([{ path: "scripts/farm.py" }]));
    expect(onRemoved).toHaveBeenCalledWith("scripts/farm.py");
  });

  it("does not allow renaming or deleting the top-level folders or the pinned blocks", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "scripts" }));
    expect(screen.getByRole("button", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete" })).toBeDisabled();

    await user.click(screen.getByRole("treeitem", { name: "multialt" }));
    await user.click(screen.getByRole("treeitem", { name: "defaults" }));
    await user.click(screen.getByRole("treeitem", { name: /runner\.py/ }));
    expect(screen.getByRole("button", { name: "Delete" })).toBeDisabled();
  });

  it("shows a context menu with the actions of the node", async () => {
    const user = userEvent.setup();
    renderExplorer();
    fireEvent.contextMenu(screen.getByRole("treeitem", { name: "scripts" }));
    const menu = screen.getByRole("menu", { name: "Actions for scripts" });
    expect(within(menu).getByRole("menuitem", { name: "New file here" })).toBeInTheDocument();
    expect(within(menu).getByRole("menuitem", { name: "Rename" })).toBeDisabled();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    fireEvent.contextMenu(screen.getByRole("treeitem", { name: "farm.py" }));
    promptAnswers.confirm = true;
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    await waitFor(() => expect(calls("python_fs_delete")).toEqual([{ path: "scripts/farm.py" }]));
  });

  it("copies an example to scripts from the context menu", async () => {
    const user = userEvent.setup();
    renderExplorer();
    await user.click(screen.getByRole("treeitem", { name: "examples" }));
    fireEvent.contextMenu(screen.getByRole("treeitem", { name: /walk_and_jump\.py/ }));
    await user.click(screen.getByRole("menuitem", { name: "Copy to scripts" }));
    await waitFor(() => expect(calls("python_fs_create")).toEqual([{ path: "scripts/walk_and_jump.py", kind: "file" }]));
    expect(calls("python_fs_write")).toEqual([{ path: "scripts/walk_and_jump.py", content: "print(1)\n" }]);
    expect(onOpen).toHaveBeenCalledWith("scripts/walk_and_jump.py");
  });
});
