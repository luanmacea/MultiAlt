import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { DeveloperPanel, restoreConfirmText } from "./DeveloperPanel";
import type { DefaultBlockInfo } from "./pythonApi";
import { defaultSettings, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const CLI = '"C:\\Data\\python-runtime\\current\\python.exe" -m multialt run scripts/farm.py --account Main_01 --mode one_at_a_time';
let blocks: DefaultBlockInfo[];

function storeWithMode(on: boolean) {
  return setStore({ settings: { ...defaultSettings(), Python: { DeveloperMode: on ? "true" : "false" } } });
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  blocks = [
    { name: "runner.py", modified: true, newDefaultAvailable: false },
    { name: "focus.py", modified: false, newDefaultAvailable: false },
    { name: "vision.py", modified: true, newDefaultAvailable: true },
  ];
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "python_list_default_blocks":
        return blocks;
      case "python_workspace_info":
        return { root: "C:/Data/python", scriptsDir: "C:/Data/python/scripts", assetsDir: "C:/Data/python/assets", pythonExe: "x", cliCommand: CLI };
      case "python_read_default_block":
        return "def run(): pass\n";
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

describe("DeveloperPanel", () => {
  it("explains what developer mode unlocks and hides the rest while it is off", () => {
    storeWithMode(false);
    render(<DeveloperPanel />);
    expect(screen.getByRole("switch", { name: "Developer mode" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/Editing the default blocks/)).toBeInTheDocument();
    expect(screen.getByText(/A terminal channel/)).toBeInTheDocument();
    expect(screen.queryByText("Default blocks")).not.toBeInTheDocument();
    expect(screen.queryByText("runner.py")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_list_default_blocks")).toBe(false);
  });

  it("turns the mode on through the backend and then shows the blocks and the CLI command", async () => {
    const store = storeWithMode(false);
    render(<DeveloperPanel />);
    await userEvent.click(screen.getByRole("switch", { name: "Developer mode" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_set_developer_mode", { enabled: true }));
    expect(await screen.findByText("runner.py")).toBeInTheDocument();
    expect(await screen.findByTestId("cli-command")).toHaveTextContent("-m multialt run scripts/farm.py");
    expect(store.reloadSettings).toHaveBeenCalled();
  });

  it("turns the mode off and hides the blocks again", async () => {
    storeWithMode(true);
    render(<DeveloperPanel />);
    expect(await screen.findByText("runner.py")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "Developer mode" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_set_developer_mode", { enabled: false }));
    await waitFor(() => expect(screen.queryByText("runner.py")).not.toBeInTheDocument());
    expect(screen.queryByTestId("cli-command")).not.toBeInTheDocument();
  });

  it("stays off and reports the error when the backend refuses", async () => {
    const store = storeWithMode(false);
    setInvokeHandler((cmd) => {
      if (cmd === "python_set_developer_mode") throw "Python automation is not available in this build";
      return undefined;
    });
    render(<DeveloperPanel />);
    await userEvent.click(screen.getByRole("switch", { name: "Developer mode" }));
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Python automation is not available in this build", "error"));
    expect(screen.getByRole("switch", { name: "Developer mode" })).toHaveAttribute("aria-checked", "false");
  });

  it("badges Modified and New default available per block", async () => {
    storeWithMode(true);
    render(<DeveloperPanel />);
    await screen.findByText("runner.py");
    const rows = screen.getAllByRole("listitem");
    const row = (name: string) => within(rows.find((r) => within(r).queryByText(name)) as HTMLElement);
    expect(row("runner.py").getByText("Modified")).toBeInTheDocument();
    expect(row("runner.py").queryByText("New default available")).not.toBeInTheDocument();
    expect(row("focus.py").queryByText("Modified")).not.toBeInTheDocument();
    expect(row("vision.py").getByText("New default available")).toBeInTheDocument();
  });

  it("asks before restoring, names the block, and restores only that block", async () => {
    storeWithMode(true);
    render(<DeveloperPanel />);
    await screen.findByText("runner.py");

    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: "Restore default of runner.py" }));
    expect(confirmMock).toHaveBeenCalledWith(
      "Restore runner.py to the default? Your changes to this block will be lost. Your scripts and other blocks are not touched.",
      true,
      expect.anything()
    );
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_restore_default_block")).toBe(false);

    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Restore default of vision.py" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_restore_default_block", { name: "vision.py" }));
    expect(invokeMock.mock.calls.filter((c) => c[0] === "python_restore_default_block")).toHaveLength(1);
  });

  it("has the exact confirmation text", () => {
    expect(restoreConfirmText("focus.py")).toBe(
      "Restore focus.py to the default? Your changes to this block will be lost. Your scripts and other blocks are not touched."
    );
  });

  it("opens a block inline and saves it", async () => {
    storeWithMode(true);
    render(<DeveloperPanel />);
    await screen.findByText("focus.py");
    await userEvent.click(screen.getByRole("button", { name: "Open focus.py" }));

    const box = await screen.findByRole("textbox", { name: "Source of block focus.py" });
    await waitFor(() => expect(box).toHaveValue("def run(): pass\n"));
    await userEvent.type(box, "# edited");
    await userEvent.click(screen.getByRole("button", { name: /Save/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("python_write_default_block", {
        name: "focus.py",
        source: "def run(): pass\n# edited",
      })
    );
  });

  it("copies the CLI command and opens the workspace folder", async () => {
    storeWithMode(true);
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    render(<DeveloperPanel />);
    await screen.findByText(/-m multialt run/);
    await userEvent.click(screen.getByRole("button", { name: /^Copy$/ }));
    expect(writeText).toHaveBeenCalledWith(CLI);

    await userEvent.click(screen.getByRole("button", { name: /Open workspace folder/ }));
    expect(invokeMock).toHaveBeenCalledWith("python_open_workspace_folder");
    expect(screen.getByText(/AGENTS.md/)).toBeInTheDocument();
  });
});
