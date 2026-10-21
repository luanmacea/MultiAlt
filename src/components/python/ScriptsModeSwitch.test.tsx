import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, renderHook, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ScriptsPage } from "../pages/ScriptsPage";
import { ScriptsModeSwitch, SCRIPTS_MODE_STORAGE_KEY, useScriptsMode } from "./ScriptsModeSwitch";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  window.localStorage.clear();
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_scripts":
        return [];
      case "get_all_settings":
        return {};
      case "python_runtime_status":
        return { installed: false, version: null, pythonExe: null, packages: [], installing: false };
      case "python_list_scripts":
      case "python_list_runs":
      case "python_get_logs":
      case "python_get_dashboard":
      case "python_list_default_blocks":
        return [];
      case "python_get_run_configs":
        return {};
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("ScriptsModeSwitch", () => {
  it("offers both modes with a one-line description each", () => {
    render(<ScriptsModeSwitch mode="javascript" onChange={vi.fn()} />);
    expect(screen.getByRole("tab", { name: /Roblox Player automation \(Python\)/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /MultiAlt automation \(JavaScript\)/ })).toBeInTheDocument();
    expect(screen.getByText(/Controls the Roblox windows of your accounts/)).toBeInTheDocument();
    expect(screen.getByText(/Runs inside MultiAlt/)).toBeInTheDocument();
  });

  it("marks the current mode and reports a change", async () => {
    const onChange = vi.fn();
    render(<ScriptsModeSwitch mode="javascript" onChange={onChange} />);
    expect(screen.getByRole("tab", { name: /MultiAlt automation/ })).toHaveAttribute("aria-selected", "true");
    await userEvent.click(screen.getByRole("tab", { name: /Roblox Player automation/ }));
    expect(onChange).toHaveBeenCalledWith("python");
  });

  it("renders nothing when the feature flag is off", () => {
    const { container } = render(<ScriptsModeSwitch mode="javascript" onChange={vi.fn()} enabled={false} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe("useScriptsMode", () => {
  it("starts in JavaScript mode, so the page is unchanged until the user switches", () => {
    const { result } = renderHook(() => useScriptsMode());
    expect(result.current[0]).toBe("javascript");
  });

  it("remembers the choice in localStorage", () => {
    const { result } = renderHook(() => useScriptsMode());
    act(() => result.current[1]("python"));
    expect(result.current[0]).toBe("python");
    expect(window.localStorage.getItem(SCRIPTS_MODE_STORAGE_KEY)).toBe("python");
    expect(SCRIPTS_MODE_STORAGE_KEY).toBe("ram_scripts_mode");

    const again = renderHook(() => useScriptsMode());
    expect(again.result.current[0]).toBe("python");
  });

  it("ignores a garbage stored value", () => {
    window.localStorage.setItem(SCRIPTS_MODE_STORAGE_KEY, "perl");
    const { result } = renderHook(() => useScriptsMode());
    expect(result.current[0]).toBe("javascript");
  });

  it("stays in JavaScript mode when the flag is off, even with Python stored", () => {
    window.localStorage.setItem(SCRIPTS_MODE_STORAGE_KEY, "python");
    const { result } = renderHook(() => useScriptsMode(false));
    expect(result.current[0]).toBe("javascript");
  });

  it("survives a storage that throws", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const { result } = renderHook(() => useScriptsMode());
    expect(result.current[0]).toBe("javascript");
    spy.mockRestore();
  });
});

describe("ScriptsPage with the mode switch", () => {
  function renderPage() {
    setStore({});
    return render(<ScriptsPage active onLeave={vi.fn()} />);
  }

  it("keeps the JavaScript page untouched by default", async () => {
    renderPage();
    expect(screen.getByPlaceholderText("Search scripts")).toBeInTheDocument();
    expect(screen.queryByText(/Python scripts have full access to your PC/)).not.toBeInTheDocument();
  });

  it("switches to the Python view and back, remembering the choice", async () => {
    renderPage();
    await userEvent.click(screen.getByRole("tab", { name: /Roblox Player automation/ }));
    expect(await screen.findByText(/Python scripts have full access to your PC/)).toBeInTheDocument();
    expect(screen.queryByPlaceholderText("Search scripts")).not.toBeInTheDocument();
    expect(window.localStorage.getItem(SCRIPTS_MODE_STORAGE_KEY)).toBe("python");

    await userEvent.click(screen.getByRole("tab", { name: /MultiAlt automation/ }));
    await waitFor(() => expect(screen.getByPlaceholderText("Search scripts")).toBeInTheDocument());
    expect(screen.queryByText(/Python scripts have full access to your PC/)).not.toBeInTheDocument();
  });

  it("opens straight in Python mode when it was the last choice", async () => {
    window.localStorage.setItem(SCRIPTS_MODE_STORAGE_KEY, "python");
    renderPage();
    expect(await screen.findByText(/Python scripts have full access to your PC/)).toBeInTheDocument();
  });
});
