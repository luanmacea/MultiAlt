import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { PresetsDialog } from "./PresetsDialog";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import type { LaunchPresetView } from "../../types";
import { newPresetDraft } from "../../utils/presets";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "alpha_main" }),
  makeAccount({ UserID: 2, Username: "bravo_alt" }),
  makeAccount({ UserID: 3, Username: "charlie_alt" }),
];

function view(overrides: Partial<LaunchPresetView> = {}): LaunchPresetView {
  return {
    id: "preset-1",
    name: "Morning farm",
    userIds: [1, 2],
    placeId: 920587237,
    jobId: "",
    gameName: "Adopt Me!",
    vipName: null,
    arrangeGrid: false,
    schedule: null,
    nextOpenAt: null,
    nextCloseAt: null,
    openClients: 0,
    ...overrides,
  };
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  window.localStorage.clear();
});
afterEach(cleanup);

describe("PresetsDialog — lista", () => {
  it("shows each preset with its accounts and game, and Launch goes through the store", async () => {
    setInvokeMap({ get_launch_presets: [view()] });
    const { store } = renderWithStore(<PresetsDialog />, {
      accounts: ACCOUNTS,
      presetsDialog: { draft: null },
    });

    const row = await screen.findByTestId("preset-row");
    expect(within(row).getByText("Morning farm")).toBeInTheDocument();
    expect(within(row).getByText(/2 accounts · Adopt Me!/)).toBeInTheDocument();

    await userEvent.click(within(row).getByRole("button", { name: /Launch/ }));
    await waitFor(() => expect(store.launchPreset).toHaveBeenCalledWith(expect.objectContaining({ id: "preset-1" })));
    // Começou: o diálogo fecha para a pessoa ver o launch andando.
    await waitFor(() => expect(store.closePresetsDialog).toHaveBeenCalled());
  });

  it("closing a preset's windows asks the backend for that preset only and never closes everything", async () => {
    setInvokeMap({ get_launch_presets: [view({ openClients: 2 })], close_preset_clients: 2 });
    renderWithStore(<PresetsDialog />, { accounts: ACCOUNTS, presetsDialog: { draft: null } });

    await userEvent.click(await screen.findByRole("button", { name: "Close them" }));
    await waitFor(() => expect(callsFor("close_preset_clients")).toEqual([["close_preset_clients", { id: "preset-1" }]]));
    expect(callsFor("cmd_kill_all_roblox")).toHaveLength(0);
    expect(callsFor("cmd_kill_roblox")).toHaveLength(0);
  });

  it("an account that left the app is counted out and flagged", async () => {
    setInvokeMap({ get_launch_presets: [view({ userIds: [1, 99] })] });
    renderWithStore(<PresetsDialog />, { accounts: ACCOUNTS, presetsDialog: { draft: null } });
    const row = await screen.findByTestId("preset-row");
    expect(within(row).getByText(/1 account · Adopt Me!/)).toBeInTheDocument();
    expect(within(row).getByText(/no longer in the app/)).toBeInTheDocument();
  });

  it("shows the next scheduled run", async () => {
    const soon = Date.now() + 60_000;
    setInvokeMap({ get_launch_presets: [view({ nextOpenAt: soon })] });
    renderWithStore(<PresetsDialog />, { accounts: ACCOUNTS, presetsDialog: { draft: null } });
    expect(await screen.findByText(/Opens (Today|Tomorrow)/)).toBeInTheDocument();
  });
});

describe("PresetsDialog — editor", () => {
  it("saves the accounts, the typed place and the schedule", async () => {
    setInvokeMap({ get_launch_presets: [], save_launch_preset: (args: unknown) => (args as { preset: unknown }).preset });
    renderWithStore(<PresetsDialog />, { accounts: ACCOUNTS, presetsDialog: { draft: newPresetDraft([2]) } });

    await userEvent.type(await screen.findByLabelText("Name"), "Night");
    await userEvent.type(screen.getByLabelText("Place ID"), "606849621");
    await userEvent.click(screen.getByRole("switch", { name: /Open at a set time/ }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(callsFor("save_launch_preset")).toHaveLength(1));
    const preset = (callsFor("save_launch_preset")[0][1] as { preset: Record<string, unknown> }).preset;
    expect(preset).toMatchObject({ name: "Night", userIds: [2], placeId: 606849621, jobId: "" });
    expect(preset.schedule).toMatchObject({ openEnabled: true, openAt: "08:00", closeEnabled: false });
  });

  it("does not save without a name and says why", async () => {
    setInvokeMap({ get_launch_presets: [] });
    renderWithStore(<PresetsDialog />, { accounts: ACCOUNTS, presetsDialog: { draft: newPresetDraft([1], 42) } });
    await userEvent.click(await screen.findByRole("button", { name: "Save" }));
    expect(await screen.findByText("Give the preset a name.")).toBeInTheDocument();
    expect(callsFor("save_launch_preset")).toHaveLength(0);
  });

  it("deleting asks first", async () => {
    setInvokeMap({ get_launch_presets: [], delete_launch_preset: true });
    promptAnswers.confirm = true;
    renderWithStore(<PresetsDialog />, {
      accounts: ACCOUNTS,
      presetsDialog: { draft: { ...view(), name: "Old" } },
    });
    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
    await waitFor(() => expect(callsFor("delete_launch_preset")).toEqual([["delete_launch_preset", { id: "preset-1" }]]));
  });

  it("with names hidden the account list does not show the real names", async () => {
    setInvokeMap({ get_launch_presets: [] });
    renderWithStore(<PresetsDialog />, {
      accounts: ACCOUNTS,
      hideUsernames: true,
      presetsDialog: { draft: newPresetDraft([1]) },
    });
    await screen.findByLabelText("Name");
    expect(screen.queryByText("alpha_main")).not.toBeInTheDocument();
    expect(screen.queryByText("bravo_alt")).not.toBeInTheDocument();
  });
});
