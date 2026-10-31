import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { RunPanel } from "./RunPanel";
import { RunningList, formatCountdown } from "./RunningList";
import type { PythonRunConfig, PythonRunInfo } from "./pythonApi";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";

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

let savedConfigs: Record<string, PythonRunConfig>;

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  savedConfigs = {};
  setStore({
    accounts: [
      makeAccount({ UserID: 1001, Username: "Main_01", Group: "Default" }),
      makeAccount({ UserID: 1002, Username: "Alt_02", Group: "Default" }),
      makeAccount({ UserID: 1003, Username: "Alt_03", Group: "Squad" }),
    ],
    launchedByProgram: new Set([1001]),
  });
  setInvokeHandler((cmd, args) => {
    switch (cmd) {
      case "python_get_run_configs":
        return savedConfigs;
      case "python_save_run_config":
        savedConfigs[(args as { config: PythonRunConfig }).config.script] = (args as { config: PythonRunConfig }).config;
        return undefined;
      case "python_start_run":
        return "r9";
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

const lastSaved = () => {
  const calls = invokeMock.mock.calls.filter((c) => c[0] === "python_save_run_config");
  return (calls[calls.length - 1][1] as { config: PythonRunConfig }).config;
};

describe("RunPanel", () => {
  it("starts with nothing selected and the Run button off", async () => {
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={vi.fn()} />);
    expect(await screen.findByText("0 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Run/ })).toBeDisabled();
    expect(screen.getByText("Pick at least one account.")).toBeInTheDocument();
  });

  it("keeps Run off until Python is installed", async () => {
    render(<RunPanel script="farm.py" runtimeInstalled={false} runs={[]} onToast={vi.fn()} />);
    await userEvent.click(await screen.findByRole("button", { name: "All" }));
    expect(screen.getByRole("button", { name: /Run/ })).toBeDisabled();
    expect(screen.getByText("Install Python to run scripts.")).toBeInTheDocument();
  });

  it("saves the accounts, mode and trigger as they change", async () => {
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={vi.fn()} />);
    await screen.findByText("0 selected");

    await userEvent.click(screen.getByRole("checkbox", { name: /Main_01/ }));
    expect(lastSaved()).toMatchObject({ script: "farm.py", accounts: [1001], mode: "one_at_a_time", trigger: { kind: "manual" } });

    await userEvent.click(screen.getByRole("radio", { name: "All together" }));
    expect(lastSaved().mode).toBe("all_together");

    await userEvent.click(screen.getByRole("button", { name: "Trigger" }));
    await userEvent.click(screen.getByRole("button", { name: "Every N minutes" }));
    expect(lastSaved().trigger).toEqual({ kind: "interval", minutes: 30 });

    const minutes = screen.getByRole("spinbutton", { name: "Minutes between runs" });
    await userEvent.clear(minutes);
    await userEvent.type(minutes, "5");
    expect(lastSaved().trigger).toEqual({ kind: "interval", minutes: 5 });
  });

  it("restores the saved configuration of the script", async () => {
    savedConfigs["farm.py"] = {
      script: "farm.py",
      accounts: [1002, 1003, 9999],
      mode: "all_together",
      trigger: { kind: "loop" },
    };
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={vi.fn()} />);
    // 9999 is not an account anymore: it is not counted.
    expect(await screen.findByText("2 selected")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "All together" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Trigger" })).toHaveTextContent(/Loop/);
  });

  it("selects a whole group and the open clients", async () => {
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={vi.fn()} />);
    await screen.findByText("0 selected");
    await userEvent.click(screen.getByRole("checkbox", { name: "Select the group Squad" }));
    expect(lastSaved().accounts).toEqual([1003]);

    await userEvent.click(screen.getByRole("button", { name: "Open clients" }));
    expect(lastSaved().accounts).toEqual([1001]);
  });

  it("Shift+click selects the range across groups and dragging over the rows adds the ones it passes", async () => {
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={vi.fn()} />);
    const box = async (name: string) => (await screen.findByText(name)).closest("label")!.querySelector("input")!;
    fireEvent.click(await box("Main_01"));
    fireEvent.click(await box("Alt_03"), { shiftKey: true });
    await waitFor(() => expect(lastSaved().accounts.sort()).toEqual([1001, 1002, 1003]));

    // Drag from the ticked Alt_03 up to Alt_02: unticks what it passes.
    fireEvent.mouseDown(await box("Alt_03"), { button: 0 });
    fireEvent.mouseMove((await box("Alt_02")).closest("label")!);
    fireEvent.mouseUp(await box("Alt_02"));
    await waitFor(() => expect(lastSaved().accounts).toEqual([1001]));
  });

  it("starts a run with the selected accounts", async () => {
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={vi.fn()} />);
    await screen.findByText("0 selected");
    await userEvent.click(screen.getByRole("checkbox", { name: /Main_01/ }));
    await userEvent.click(screen.getByRole("checkbox", { name: /Alt_02/ }));
    await userEvent.click(screen.getByRole("button", { name: /Run/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("python_start_run", {
        config: { script: "farm.py", accounts: [1001, 1002], mode: "one_at_a_time", trigger: { kind: "manual" } },
      })
    );
  });

  it("reports a start error", async () => {
    const onToast = vi.fn();
    setInvokeHandler((cmd) => {
      if (cmd === "python_get_run_configs") return {};
      if (cmd === "python_start_run") throw "Python automation is not available in this build";
      return undefined;
    });
    render(<RunPanel script="farm.py" runtimeInstalled runs={[]} onToast={onToast} />);
    await screen.findByText("0 selected");
    await userEvent.click(screen.getByRole("button", { name: "All" }));
    await userEvent.click(screen.getByRole("button", { name: /Run/ }));
    await waitFor(() => expect(onToast).toHaveBeenCalledWith("Python automation is not available in this build", "error"));
  });

  it("keeps Run off while this script is running", async () => {
    savedConfigs["farm.py"] = { script: "farm.py", accounts: [1001], mode: "one_at_a_time", trigger: { kind: "manual" } };
    const { rerender } = render(
      <RunPanel script="farm.py" runtimeInstalled runs={[run({ id: "r1", source: "terminal" })]} onToast={vi.fn()} />
    );
    expect(await screen.findByText("1 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Run/ })).toBeDisabled();
    expect(screen.getByText("This script is already running. Stop it first.")).toBeInTheDocument();
    // Another script running does not count; a finished run of this one neither.
    rerender(
      <RunPanel
        script="farm.py"
        runtimeInstalled
        runs={[run({ id: "r1", state: "finished" }), run({ id: "r2", script: "other.py" })]}
        onToast={vi.fn()}
      />
    );
    expect(screen.getByRole("button", { name: /Run/ })).toBeEnabled();
  });

  it("stops the active runs of this script only", async () => {
    render(
      <RunPanel
        script="farm.py"
        runtimeInstalled
        runs={[run({ id: "r1" }), run({ id: "r2", script: "other.py" }), run({ id: "r3", state: "finished" })]}
        onToast={vi.fn()}
      />
    );
    await userEvent.click(await screen.findByRole("button", { name: "Stop" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_stop_run", { runId: "r1" }));
    expect(invokeMock.mock.calls.filter((c) => c[0] === "python_stop_run")).toHaveLength(1);
  });
});

describe("RunningList", () => {
  it("shows where each run came from and its state", () => {
    render(
      <RunningList
        runs={[run({ id: "r1" }), run({ id: "r2", script: "claude_test.py", source: "terminal", state: "starting" })]}
        onToast={vi.fn()}
      />
    );
    expect(screen.getByText("claude_test.py")).toBeInTheDocument();
    expect(screen.getByText("terminal")).toBeInTheDocument();
    expect(screen.getByText("app")).toBeInTheDocument();
    expect(screen.getByText("starting")).toBeInTheDocument();
    expect(screen.getByText("2 active")).toBeInTheDocument();
  });

  it("stops one run, or all of them", async () => {
    setInvokeHandler(() => undefined);
    render(<RunningList runs={[run({ id: "r1" }), run({ id: "r2", script: "b.py" })]} onToast={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Stop b.py (r2)" }));
    expect(invokeMock).toHaveBeenCalledWith("python_stop_run", { runId: "r2" });
    await userEvent.click(screen.getByRole("button", { name: /Stop all/ }));
    expect(invokeMock).toHaveBeenCalledWith("python_stop_all_runs");
  });

  it("disables Stop all when nothing is active and hides Stop on finished runs", () => {
    render(<RunningList runs={[run({ state: "finished", endedMs: 5 }), run({ id: "r2", state: "failed", error: "Traceback: boom" })]} onToast={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Stop all/ })).toBeDisabled();
    expect(screen.queryByRole("button", { name: /^Stop farm/ })).not.toBeInTheDocument();
    expect(screen.getByText("Traceback: boom")).toBeInTheDocument();
  });

  it("shows when the next interval run starts", () => {
    const now = Date.now();
    render(
      <RunningList
        runs={[run({ state: "waiting", trigger: { kind: "interval", minutes: 5 }, nextRunMs: now + 4 * 60_000 + 12_000 })]}
        onToast={vi.fn()}
      />
    );
    expect(screen.getByText(/next run in 4m/)).toBeInTheDocument();
  });

  it("formats countdowns", () => {
    expect(formatCountdown(null, 0)).toBe("");
    expect(formatCountdown(45_000, 0)).toBe("in 45s");
    expect(formatCountdown(125_000, 0)).toBe("in 2m 05s");
    expect(formatCountdown(0, 10_000)).toBe("in 0s");
  });
});
