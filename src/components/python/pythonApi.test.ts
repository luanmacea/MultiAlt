import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { PYTHON_EVENTS, errorText, isValidScriptName, pythonApi } from "./pythonApi";
import { invokeMock, resetTauriMocks } from "../../test-utils/tauriMocks";

beforeEach(() => resetTauriMocks());

describe("isValidScriptName", () => {
  it("accepts plain .py names", () => {
    for (const name of ["farm.py", "a.py", "My_script-2.py", `${"x".repeat(64)}.py`]) {
      expect(isValidScriptName(name)).toBe(true);
    }
  });

  it("rejects paths, other extensions, empty and too long names", () => {
    for (const name of [
      "",
      ".py",
      "farm",
      "farm.txt",
      "farm.PY",
      "../farm.py",
      "sub/farm.py",
      "sub\farm.py",
      "my script.py",
      "farm.py.py",
      `${"x".repeat(65)}.py`,
    ]) {
      expect(isValidScriptName(name)).toBe(false);
    }
  });
});

describe("pythonApi", () => {
  it("uses the command and argument names of the contract", async () => {
    await pythonApi.runtimeStatus();
    await pythonApi.readScript("a.py");
    await pythonApi.writeScript("a.py", "x");
    await pythonApi.createScript("a.py", "per_account");
    await pythonApi.stopRun("r1");
    await pythonApi.inspectorCapture(7);
    await pythonApi.saveAsset(7, { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }, "btn.png", 4);
    await pythonApi.setDeveloperMode(true);
    await pythonApi.clearDashboard("farm.py");
    await pythonApi.restoreDefaultBlock("runner.py");
    const config = { script: "a.py", accounts: [1], mode: "one_at_a_time" as const, trigger: { kind: "loop" as const } };
    await pythonApi.startRun(config);
    await pythonApi.saveRunConfig(config);

    expect(invokeMock.mock.calls.map((call) => [call[0], call[1]])).toEqual([
      ["python_runtime_status", undefined],
      ["python_read_script", { name: "a.py" }],
      ["python_write_script", { name: "a.py", source: "x" }],
      ["python_create_script", { name: "a.py", template: "per_account" }],
      ["python_stop_run", { runId: "r1" }],
      ["python_inspector_capture", { userId: 7 }],
      ["python_save_asset", { userId: 7, rect: { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }, name: "btn.png", frameId: 4 }],
      ["python_set_developer_mode", { enabled: true }],
      ["python_clear_dashboard", { script: "farm.py" }],
      ["python_restore_default_block", { name: "runner.py" }],
      ["python_start_run", { config }],
      ["python_save_run_config", { config }],
    ]);
  });

  it("exposes the event names of the contract", () => {
    expect(PYTHON_EVENTS).toEqual({
      installProgress: "python-install-progress",
      log: "python-log",
      runUpdate: "python-run-update",
      dashboardUpdate: "python-dashboard-update",
    });
  });
});

describe("errorText", () => {
  it("turns strings, errors and objects into text", () => {
    expect(errorText("boom")).toBe("boom");
    expect(errorText(new Error("bad"))).toBe("bad");
    expect(errorText({ a: 1 })).toBe('{"a":1}');
  });
});
