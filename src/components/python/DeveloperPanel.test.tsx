import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { DeveloperPanel } from "./DeveloperPanel";
import { defaultSettings, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

const CLI = '"C:\\Data\\python-runtime\\current\\python.exe" -m multialt run scripts/farm.py --account Main_01 --mode one_at_a_time';

function storeWithMode(on: boolean) {
  return setStore({ settings: { ...defaultSettings(), Python: { DeveloperMode: on ? "true" : "false" } } });
}

beforeEach(() => {
  resetTauriMocks();
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "python_workspace_info":
        return { root: "C:/Data/python", scriptsDir: "C:/Data/python/scripts", assetsDir: "C:/Data/python/assets", pythonExe: "x", cliCommand: CLI };
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

describe("DeveloperPanel", () => {
  it("explains what developer mode unlocks and hides the terminal channel while it is off", () => {
    storeWithMode(false);
    render(<DeveloperPanel />);
    expect(screen.getByRole("switch", { name: "Developer mode" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/Editing the default blocks/)).toBeInTheDocument();
    expect(screen.getAllByText(/A terminal channel/).length).toBeGreaterThan(0);
    expect(screen.queryByTestId("cli-command")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_workspace_info")).toBe(false);
  });

  it("no longer lists the default blocks here: they live in Code and Global functions", async () => {
    storeWithMode(true);
    render(<DeveloperPanel />);
    await screen.findByTestId("cli-command");
    expect(screen.queryByText("Default blocks")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Restore default/ })).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_list_default_blocks")).toBe(false);
  });

  it("turns the mode on through the backend and then shows the CLI command", async () => {
    const store = storeWithMode(false);
    render(<DeveloperPanel />);
    await userEvent.click(screen.getByRole("switch", { name: "Developer mode" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_set_developer_mode", { enabled: true }));
    expect(await screen.findByTestId("cli-command")).toHaveTextContent("-m multialt run scripts/farm.py");
    expect(store.reloadSettings).toHaveBeenCalled();
  });

  it("turns the mode off and hides the terminal channel again", async () => {
    storeWithMode(true);
    render(<DeveloperPanel />);
    expect(await screen.findByTestId("cli-command")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "Developer mode" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_set_developer_mode", { enabled: false }));
    await waitFor(() => expect(screen.queryByTestId("cli-command")).not.toBeInTheDocument());
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
