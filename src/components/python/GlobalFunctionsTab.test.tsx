import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());
vi.mock("./copyText", () => ({ copyText: vi.fn(async () => true) }));

import { GlobalFunctionsTab, importLine, newFunctionTemplate, restoreConfirmText } from "./GlobalFunctionsTab";
import { copyText } from "./copyText";
import type { GlobalModule } from "./pythonApi";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const MODULES: GlobalModule[] = [
  {
    path: "lib/helpers.py",
    module: "lib.helpers",
    pinned: false,
    modified: false,
    newDefaultAvailable: false,
    functions: [{ name: "walk_forward", signature: "walk_forward(account, seconds=1.0)", doc: "Hold W for a while." }],
  },
  {
    path: "multialt/defaults/vision.py",
    module: "multialt.defaults.vision",
    pinned: true,
    modified: true,
    newDefaultAvailable: true,
    functions: [{ name: "wait_pixel", signature: "wait_pixel(account, x, y, color, timeout=10)", doc: "Wait for a pixel color." }],
  },
  {
    path: "multialt/defaults/focus.py",
    module: "multialt.defaults.focus",
    pinned: true,
    modified: false,
    newDefaultAvailable: false,
    functions: [],
  },
];

const onOpenFile = vi.fn();
const onOpenDeveloper = vi.fn();
const onToast = vi.fn();
const calls = (cmd: string) => invokeMock.mock.calls.filter((call) => call[0] === cmd).map((call) => call[1]);

function draw(developerMode = false) {
  return render(
    <GlobalFunctionsTab
      visible
      developerMode={developerMode}
      onOpenFile={onOpenFile}
      onOpenDeveloper={onOpenDeveloper}
      onToast={onToast}
    />
  );
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  vi.mocked(copyText).mockClear();
  for (const fn of [onOpenFile, onOpenDeveloper, onToast]) fn.mockClear();
  setInvokeHandler((cmd) => (cmd === "python_list_global_functions" ? MODULES : undefined));
});
afterEach(cleanup);

describe("GlobalFunctionsTab", () => {
  it("lists the locked default blocks first, then the modules of lib/", async () => {
    draw();
    const blocks = await screen.findByRole("region", { name: "Default blocks" });
    expect(within(blocks).getByText("multialt.defaults.vision")).toBeInTheDocument();
    expect(within(blocks).getAllByLabelText("Locked default block").length).toBeGreaterThanOrEqual(2);
    expect(within(blocks).getByText("Modified")).toBeInTheDocument();
    expect(within(blocks).getByText("New default available")).toBeInTheDocument();
    const mine = screen.getByRole("region", { name: "Your modules" });
    expect(within(mine).getByText("lib.helpers")).toBeInTheDocument();
    expect(within(mine).getByText("walk_forward(account, seconds=1.0)")).toBeInTheDocument();
    expect(within(mine).getByText("Hold W for a while.")).toBeInTheDocument();
    expect(blocks.compareDocumentPosition(mine) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("shows a ready-to-copy import line for each function and copies it", async () => {
    const user = userEvent.setup();
    draw();
    expect(await screen.findByText("from lib.helpers import walk_forward")).toBeInTheDocument();
    expect(screen.getByText("from multialt.defaults.vision import wait_pixel")).toBeInTheDocument();
    expect(importLine("lib.helpers", MODULES[0].functions[0])).toBe("from lib.helpers import walk_forward");

    await user.click(screen.getByRole("button", { name: "Copy import of walk_forward" }));
    expect(copyText).toHaveBeenCalledWith("from lib.helpers import walk_forward");
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  it("opens the file in the Code tab when a function is clicked", async () => {
    const user = userEvent.setup();
    draw();
    await user.click(await screen.findByText("walk_forward(account, seconds=1.0)"));
    expect(onOpenFile).toHaveBeenCalledWith("lib/helpers.py");
    await user.click(screen.getByRole("button", { name: "multialt.defaults.vision" }));
    expect(onOpenFile).toHaveBeenCalledWith("multialt/defaults/vision.py");
  });

  it("explains that Restore default needs Developer mode and keeps it off without it", async () => {
    const user = userEvent.setup();
    draw(false);
    const restore = await screen.findByRole("button", { name: "Restore default of vision.py" });
    expect(restore).toBeDisabled();
    expect(screen.getByText(/Editing or restoring them needs Developer mode/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open the Developer tab" }));
    expect(onOpenDeveloper).toHaveBeenCalled();
  });

  it("asks before restoring a block and restores it only when confirmed", async () => {
    const user = userEvent.setup();
    draw(true);
    const restore = await screen.findByRole("button", { name: "Restore default of vision.py" });
    promptAnswers.confirm = false;
    await user.click(restore);
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(confirmMock.mock.calls[0][0]).toBe(restoreConfirmText("vision.py"));
    expect(calls("python_restore_default_block")).toEqual([]);

    promptAnswers.confirm = true;
    await user.click(restore);
    await waitFor(() => expect(calls("python_restore_default_block")).toEqual([{ name: "vision.py" }]));
  });

  it("creates a module in lib/ from New function and opens it in the Code tab", async () => {
    const user = userEvent.setup();
    draw();
    await screen.findByText("lib.helpers");
    promptAnswers.prompt = "moves";
    await user.click(screen.getByRole("button", { name: "New function" }));
    await waitFor(() => expect(calls("python_fs_create")).toEqual([{ path: "lib/moves.py", kind: "file" }]));
    const [write] = calls("python_fs_write") as { path: string; content: string }[];
    expect(write.path).toBe("lib/moves.py");
    expect(write.content).toBe(newFunctionTemplate("moves"));
    expect(write.content).toContain("def moves(account, seconds=1.0):");
    expect(onOpenFile).toHaveBeenCalledWith("lib/moves.py");
  });

  it("refuses a module name that cannot be imported", async () => {
    const user = userEvent.setup();
    draw();
    await screen.findByText("lib.helpers");
    for (const bad of ["2fast", "my-module", "a b"]) {
      promptAnswers.prompt = bad;
      await user.click(screen.getByRole("button", { name: "New function" }));
    }
    await waitFor(() => expect(onToast).toHaveBeenCalledTimes(3));
    expect(calls("python_fs_create")).toEqual([]);

    promptAnswers.prompt = "helpers";
    await user.click(screen.getByRole("button", { name: "New function" }));
    await waitFor(() => expect(onToast).toHaveBeenCalledTimes(4));
    expect(onToast).toHaveBeenLastCalledWith("lib/helpers.py already exists.", "warn");
  });
});
