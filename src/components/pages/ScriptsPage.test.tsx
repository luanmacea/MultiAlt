import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ScriptsPage } from "./ScriptsPage";
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

function renderDialog(active = true) {
  const store = setStore({});
  const onLeave = vi.fn();
  render(<ScriptsPage active={active} onLeave={onLeave} />);
  return { store, onLeave };
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

describe("ScriptsPage — shell", () => {
  it("renders nothing while another page is open", () => {
    setStore({});
    const { container } = render(<ScriptsPage active={false} onLeave={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the header and the script search once open", async () => {
    renderDialog();
    expect(screen.getByRole("heading", { level: 1, name: "Scripts" })).toBeInTheDocument();
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

  it("leaves the page on Escape", async () => {
    const { onLeave } = renderDialog();
    await userEvent.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledTimes(1);
  });

  /** Com o menu New aberto, o primeiro Escape fecha o menu e a página fica. */
  it("closes the New menu on Escape before leaving the page", async () => {
    const { onLeave } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: /New/ }));
    expect(screen.getByText("New Script")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByText("New Script")).not.toBeInTheDocument();
    expect(onLeave).not.toHaveBeenCalled();
  });
});

describe("ScriptsPage — what the feature is", () => {
  it("says in the header that this is JavaScript inside RAM, not a Roblox executor", () => {
    renderDialog();

    expect(
      screen.getByText("JavaScript automation that runs inside MultiAlt, not a Roblox executor or injector")
    ).toBeInTheDocument();
  });

  it("says where the script runs and what it cannot reach, without opening a tab", () => {
    renderDialog();

    expect(
      screen.getByText(
        "Scripts run in a sandboxed Web Worker and talk to the app through the ram.* API. They cannot inject code into the Roblox client and never receive account cookies or passwords."
      )
    ).toBeInTheDocument();
  });

  it("keeps the explanation visible when a script is selected", async () => {
    scripts = [makeScript()];
    renderDialog();

    // Uma conta na biblioteca seleciona o primeiro script sozinha: a explicação
    // não pode sumir junto com o estado vazio.
    expect(await screen.findByText("Auto rejoin")).toBeInTheDocument();
    expect(
      screen.getByText("JavaScript automation that runs inside MultiAlt, not a Roblox executor or injector")
    ).toBeInTheDocument();
  });
});

describe("ScriptsPage — permission descriptions", () => {
  const PERMISSION_DESCRIPTIONS: Array<[string, string]> = [
    [
      "Invoke Rust Commands",
      "Runs app commands: add, edit or remove accounts, launch or kill clients, start Auto Rejoin, the generator and the local servers. Cookies and passwords are stripped from the results.",
    ],
    [
      "HTTP Requests",
      "Sends requests to any public http/https address, so data the script can read may leave this machine.",
    ],
    ["WebSocket Connections", "Keeps up to 8 live ws/wss connections open to an outside server."],
    [
      "Window Snapshot",
      "Reads app state on demand: account list, aliases, groups, selection, presence and the current Place/Job ID. No cookies or passwords.",
    ],
    ["Modal Access", "Opens alert, confirm and input dialogs on top of the app window."],
    ["Script Settings", "Reads and writes only this script's own section of RAMSettings.ini."],
    ["Custom UI", "Draws the script's own controls in the UI tab of this dialog."],
    [
      "Private Network (localhost/LAN)",
      "Lets HTTP and WebSocket calls reach localhost and your local network instead of only the public internet.",
    ],
  ];

  it("spells out the concrete risk under every permission toggle", async () => {
    scripts = [makeScript()];
    renderDialog();

    // O painel do editor só monta depois que o script carregado vira draft.
    expect(await screen.findByText("Invoke Rust Commands")).toBeInTheDocument();

    for (const [label, description] of PERMISSION_DESCRIPTIONS) {
      expect(screen.getByText(label)).toBeInTheDocument();
      expect(screen.getByText(description)).toBeInTheDocument();
    }
  });

  it("keeps the description inside the toggle button, so it reads with the label", async () => {
    scripts = [makeScript()];
    renderDialog();

    const toggle = (await screen.findByText("HTTP Requests")).closest("button");
    expect(toggle).not.toBeNull();
    expect(toggle).toHaveTextContent(
      "Sends requests to any public http/https address, so data the script can read may leave this machine."
    );
  });
});
