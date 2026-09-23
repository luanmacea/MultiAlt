import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ScriptsDialog } from "./ScriptsDialog";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";
import type { ManagedScript } from "../../scripting/types";

function makeScript(overrides: Partial<ManagedScript> = {}): ManagedScript {
  return {
    id: "s1",
    name: "Auto rejoin",
    description: "",
    language: "javascript",
    source: "// noop",
    enabled: false,
    trusted: false,
    autoStart: false,
    permissions: {
      allowInvoke: false,
      allowHttp: false,
      allowWebSocket: false,
      allowWindow: false,
      allowModal: false,
      allowSettings: false,
      allowUi: false,
    },
    createdAtMs: 0,
    updatedAtMs: 0,
    ...overrides,
  };
}

let scripts: ManagedScript[] = [];

function renderDialog(open = true) {
  const store = setStore({});
  const onClose = vi.fn();
  render(<ScriptsDialog open={open} onClose={onClose} />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  scripts = [];
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_scripts":
        return scripts;
      case "get_all_settings":
        return {};
      case "get_setting":
        return null;
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("ScriptsDialog — shell", () => {
  it("renders nothing while closed", () => {
    setStore({});
    const { container } = render(<ScriptsDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the header and the script search once open", async () => {
    renderDialog();
    expect(screen.getByText("Scripts")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Search scripts")).toBeInTheDocument();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_scripts"));
  });

  it("reports an empty library once the load finished", async () => {
    renderDialog();
    expect(await screen.findByText("No scripts found")).toBeInTheDocument();
  });

  it("lists the scripts returned by the backend and filters them", async () => {
    scripts = [makeScript(), makeScript({ id: "s2", name: "Robux watcher" })];
    renderDialog();

    expect(await screen.findByText("Auto rejoin")).toBeInTheDocument();
    expect(screen.getByText("Robux watcher")).toBeInTheDocument();

    await userEvent.type(screen.getByPlaceholderText("Search scripts"), "robux");
    await waitFor(() => expect(screen.queryByText("Auto rejoin")).not.toBeInTheDocument());
    expect(screen.getByText("Robux watcher")).toBeInTheDocument();
  });

  it("offers the New menu with its templates", async () => {
    renderDialog();
    expect(screen.queryByText("New Script")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    expect(screen.getByText("New Script")).toBeInTheDocument();
    expect(screen.getByText("Window Monitor Template")).toBeInTheDocument();
    expect(screen.getByText("Discord Bridge Template")).toBeInTheDocument();
  });

  it("closes from the header button", async () => {
    const { onClose } = renderDialog();
    // The header's X is the first button inside the dialog.
    await userEvent.click(screen.getAllByRole("button")[0]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
