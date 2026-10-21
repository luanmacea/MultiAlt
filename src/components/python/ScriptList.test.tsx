import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ScriptList, normalizeScriptName } from "./ScriptList";
import { ScriptEditor } from "./ScriptEditor";
import { SCRIPT_POLL_MS, usePythonScripts } from "./usePythonScripts";
import type { PythonScriptFile } from "./pythonApi";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, promptMock, resetPromptMocks } from "../../test-utils/promptMocks";

const FILES: PythonScriptFile[] = [
  { name: "farm.py", size: 1200, modifiedMs: 1 },
  { name: "afk.py", size: 80, modifiedMs: 2 },
];

function mountList(overrides: Partial<ComponentProps<typeof ScriptList>> = {}) {
  const props = {
    scripts: FILES,
    loaded: true,
    selected: "farm.py" as string | null,
    onSelect: vi.fn(),
    onChanged: vi.fn(async () => {}),
    onToast: vi.fn(),
    ...overrides,
  };
  render(<ScriptList {...props} />);
  return props;
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("normalizeScriptName", () => {
  it("adds .py and rejects anything the backend would refuse", () => {
    expect(normalizeScriptName("farm")).toBe("farm.py");
    expect(normalizeScriptName(" farm.py ")).toBe("farm.py");
    expect(normalizeScriptName("Farm.PY")).toBe("Farm.py");
    expect(normalizeScriptName("../evil")).toBeNull();
    expect(normalizeScriptName("a b")).toBeNull();
    expect(normalizeScriptName("")).toBeNull();
  });
});

describe("ScriptList", () => {
  it("lists the scripts and selects one", async () => {
    const props = mountList();
    expect(screen.getByText("farm.py")).toBeInTheDocument();
    await userEvent.click(screen.getByText("afk.py"));
    expect(props.onSelect).toHaveBeenCalledWith("afk.py");
  });

  it("explains the empty state", () => {
    mountList({ scripts: [], selected: null });
    expect(screen.getByText(/No scripts yet/)).toBeInTheDocument();
  });

  it("offers the four templates", async () => {
    mountList();
    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    for (const label of ["Blank", "Per account", "Controller", "Dashboard"]) {
      expect(screen.getByRole("menuitem", { name: new RegExp(label) })).toBeInTheDocument();
    }
  });

  it("creates a script from a template and selects it", async () => {
    setInvokeHandler(() => undefined);
    promptAnswers.prompt = "my_farm";
    const props = mountList();
    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: /Per account/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("python_create_script", { name: "my_farm.py", template: "per_account" })
    );
    await waitFor(() => expect(props.onSelect).toHaveBeenCalledWith("my_farm.py"));
    expect(props.onChanged).toHaveBeenCalled();
  });

  it("refuses an invalid name without calling the backend", async () => {
    promptAnswers.prompt = "../hack";
    const props = mountList();
    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: /Blank/ }));
    await waitFor(() => expect(props.onToast).toHaveBeenCalled());
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_create_script")).toBe(false);
  });

  it("refuses a name that already exists", async () => {
    promptAnswers.prompt = "FARM";
    const props = mountList();
    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: /Blank/ }));
    await waitFor(() => expect(props.onToast).toHaveBeenCalledWith(expect.stringContaining("already exists"), "warn"));
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_create_script")).toBe(false);
  });

  it("does nothing when the name prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    mountList();
    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: /Blank/ }));
    await waitFor(() => expect(promptMock).toHaveBeenCalled());
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_create_script")).toBe(false);
  });

  it("deletes only after confirmation", async () => {
    setInvokeHandler(() => undefined);
    const props = mountList();
    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: "Delete farm.py" }));
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_delete_script")).toBe(false);

    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Delete farm.py" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("python_delete_script", { name: "farm.py" }));
    expect(props.onSelect).toHaveBeenCalledWith(null);
  });

  it("opens the folder and the editor", async () => {
    setInvokeHandler(() => undefined);
    mountList();
    await userEvent.click(screen.getByRole("button", { name: /Open folder/ }));
    expect(invokeMock).toHaveBeenCalledWith("python_open_workspace_folder");
    await userEvent.click(screen.getByRole("button", { name: "Open afk.py in your editor" }));
    expect(invokeMock).toHaveBeenCalledWith("python_open_in_editor", { name: "afk.py" });
  });
});

describe("usePythonScripts", () => {
  function Probe() {
    const { scripts } = usePythonScripts();
    return <div data-testid="names">{scripts.map((s) => s.name).join(",")}</div>;
  }

  it("re-reads the folder every 2 seconds", async () => {
    vi.useFakeTimers();
    let files: PythonScriptFile[] = [FILES[0]];
    setInvokeHandler((cmd) => (cmd === "python_list_scripts" ? files : undefined));
    render(<Probe />);
    await act(async () => {});
    expect(screen.getByTestId("names")).toHaveTextContent("farm.py");

    files = [...FILES];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCRIPT_POLL_MS + 50);
    });
    expect(screen.getByTestId("names")).toHaveTextContent("afk.py,farm.py");
  });
});

describe("ScriptEditor", () => {
  let disk: string;
  let writes: string[];

  beforeEach(() => {
    disk = "print(1)\n";
    writes = [];
    setInvokeHandler((cmd, args) => {
      if (cmd === "python_read_script") return disk;
      if (cmd === "python_write_script") {
        disk = (args as { source: string }).source;
        writes.push(disk);
        return undefined;
      }
      return undefined;
    });
  });

  const editorBox = () => screen.getByRole("textbox", { name: "Source of farm.py" }) as HTMLTextAreaElement;

  it("loads the script and saves with Ctrl+S", async () => {
    render(<ScriptEditor name="farm.py" modifiedMs={1} />);
    await waitFor(() => expect(editorBox()).toHaveValue("print(1)\n"));

    await userEvent.type(editorBox(), "x");
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();
    await userEvent.keyboard("{Control>}s{/Control}");

    await waitFor(() => expect(writes).toEqual(["print(1)\nx"]));
    await waitFor(() => expect(screen.queryByText("Unsaved changes")).not.toBeInTheDocument());
  });

  it("reloads with a notice when the file changes on disk and nothing is unsaved", async () => {
    const { rerender } = render(<ScriptEditor name="farm.py" modifiedMs={1} />);
    await waitFor(() => expect(editorBox()).toHaveValue("print(1)\n"));

    disk = "print('from the AI')\n";
    rerender(<ScriptEditor name="farm.py" modifiedMs={2} />);

    await waitFor(() => expect(editorBox()).toHaveValue("print('from the AI')\n"));
    expect(screen.getByText(/Changed on disk - reloaded/)).toBeInTheDocument();
  });

  it("shows a conflict banner instead of overwriting unsaved edits", async () => {
    const { rerender } = render(<ScriptEditor name="farm.py" modifiedMs={1} />);
    await waitFor(() => expect(editorBox()).toHaveValue("print(1)\n"));
    await userEvent.type(editorBox(), "mine");

    disk = "print('external')\n";
    rerender(<ScriptEditor name="farm.py" modifiedMs={2} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(/changed on disk/);
    expect(editorBox()).toHaveValue("print(1)\nmine");

    await userEvent.click(screen.getByRole("button", { name: "Load the disk version" }));
    expect(editorBox()).toHaveValue("print('external')\n");
    expect(screen.queryByText("Unsaved changes")).not.toBeInTheDocument();
  });

  it("can keep the unsaved edits when the disk changed", async () => {
    const { rerender } = render(<ScriptEditor name="farm.py" modifiedMs={1} />);
    await waitFor(() => expect(editorBox()).toHaveValue("print(1)\n"));
    await userEvent.type(editorBox(), "mine");
    disk = "print('external')\n";
    rerender(<ScriptEditor name="farm.py" modifiedMs={2} />);
    await userEvent.click(await screen.findByRole("button", { name: "Keep my edits" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(editorBox()).toHaveValue("print(1)\nmine");
  });

  it("does not treat its own save as an external change", async () => {
    const { rerender } = render(<ScriptEditor name="farm.py" modifiedMs={1} />);
    await waitFor(() => expect(editorBox()).toHaveValue("print(1)\n"));
    await userEvent.type(editorBox(), "z");
    await userEvent.keyboard("{Control>}s{/Control}");
    await waitFor(() => expect(writes).toHaveLength(1));

    rerender(<ScriptEditor name="farm.py" modifiedMs={5} />);
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter((c) => c[0] === "python_read_script").length).toBeGreaterThan(1)
    );
    expect(screen.queryByText(/Changed on disk/)).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows a save error", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "python_read_script") return "a";
      if (cmd === "python_write_script") throw "disk is full";
      return undefined;
    });
    render(<ScriptEditor name="farm.py" modifiedMs={1} />);
    await waitFor(() => expect(editorBox()).toHaveValue("a"));
    await userEvent.type(editorBox(), "b");
    await userEvent.keyboard("{Control>}s{/Control}");
    expect(await screen.findByRole("alert")).toHaveTextContent("disk is full");
  });
});
