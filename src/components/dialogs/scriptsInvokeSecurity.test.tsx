import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import {
  SCRIPT_INVOKE_COMMANDS,
  SCRIPT_INVOKE_COMMAND_LIST,
  ScriptsDialog,
  loadPrivateNetworkGrants,
} from "./ScriptsDialog";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
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
let settings: Record<string, Record<string, string>> = {};

function routeInvoke() {
  setInvokeHandler((cmd, args) => {
    switch (cmd) {
      case "get_scripts":
        return scripts;
      case "get_all_settings":
        return settings;
      case "get_setting":
        return null;
      case "update_setting":
        return null;
      case "save_script": {
        // O backend descarta campos que não conhece — inclusive
        // `allowPrivateNetwork`, que é justamente o que o grant no INI cobre.
        const sent = (args as { script: ManagedScript }).script;
        const stored = JSON.parse(JSON.stringify(sent)) as ManagedScript;
        delete (stored.permissions as unknown as Record<string, unknown>).allowPrivateNetwork;
        return stored;
      }
      default:
        return undefined;
    }
  });
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  scripts = [];
  settings = {};
  routeInvoke();
});

afterEach(cleanup);

describe("ram.invoke allow-list", () => {
  // Regressão: `get_accounts` devolve SecurityToken e Password de todas as
  // contas. Ele não pode voltar para a lista de passagem direta.
  it("does not let get_accounts through unfiltered", () => {
    expect(SCRIPT_INVOKE_COMMANDS).not.toContain("get_accounts");
    expect([...SCRIPT_INVOKE_COMMANDS]).toEqual(
      expect.not.arrayContaining(["get_accounts", "unlock_accounts", "set_encryption_password"])
    );
  });

  it("still offers get_accounts to scripts, through the filtered route", () => {
    expect(SCRIPT_INVOKE_COMMAND_LIST).toContain("get_accounts");
    expect(SCRIPT_INVOKE_COMMAND_LIST).toContain("launch_roblox");
  });
});

describe("private-network grant persistence", () => {
  it("reads the grants from the settings section scripts cannot write", async () => {
    settings = { ScriptPrivateNetwork: { s1: "true", s2: "false", s3: "TRUE" } };
    await expect(loadPrivateNetworkGrants()).resolves.toEqual({
      s1: true,
      s2: false,
      s3: true,
    });
  });

  it("denies everything when the settings read fails", async () => {
    setInvokeHandler(() => {
      throw new Error("settings unavailable");
    });
    await expect(loadPrivateNetworkGrants()).resolves.toEqual({});
  });

  it("restores the granted permission when the dialog loads", async () => {
    scripts = [makeScript({ trusted: true })];
    settings = { ScriptPrivateNetwork: { s1: "true" } };
    setStore({});
    render(<ScriptsDialog open onClose={vi.fn()} />);

    const toggle = await screen.findByRole("button", {
      name: /Private Network \(localhost\/LAN\)/,
    });
    await waitFor(() => expect(toggle.querySelector(".bg-emerald-400")).not.toBeNull());
  });

  it("leaves the permission off when there is no grant", async () => {
    scripts = [makeScript({ trusted: true })];
    setStore({});
    render(<ScriptsDialog open onClose={vi.fn()} />);

    const toggle = await screen.findByRole("button", {
      name: /Private Network \(localhost\/LAN\)/,
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_all_settings"));
    expect(toggle.querySelector(".bg-emerald-400")).toBeNull();
  });

  it("writes the grant to the INI when the script is saved", async () => {
    scripts = [makeScript({ trusted: true })];
    setStore({});
    render(<ScriptsDialog open onClose={vi.fn()} />);

    const toggle = await screen.findByRole("button", {
      name: /Private Network \(localhost\/LAN\)/,
    });
    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole("button", { name: "Save Script" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "ScriptPrivateNetwork",
        key: "s1",
        value: "true",
      })
    );
    // E o toggle continua ligado mesmo com o backend devolvendo o script sem
    // o campo (é o grant do INI que manda).
    await waitFor(() => expect(toggle.querySelector(".bg-emerald-400")).not.toBeNull());
  });

  it("clears the grant when the script is deleted", async () => {
    scripts = [makeScript({ trusted: true })];
    settings = { ScriptPrivateNetwork: { s1: "true" } };
    promptAnswers.confirm = true;
    setStore({});
    render(<ScriptsDialog open onClose={vi.fn()} />);

    await screen.findByText("Auto rejoin");
    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "ScriptPrivateNetwork",
        key: "s1",
        value: "false",
      })
    );
  });
});
