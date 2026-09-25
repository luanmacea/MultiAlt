import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { BottingDialog } from "./BottingDialog";
import {
  defaultSettings,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";
import type { StoreValue } from "../../store";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });

/** Botting refuses to start unless Multi Roblox is on. */
function settings(multiRbx: boolean, draft: Record<string, string> = {}) {
  const s = defaultSettings();
  s.General.EnableMultiRbx = multiRbx ? "true" : "false";
  s.General.BottingEnabled = "true";
  Object.assign(s.General, draft);
  return s;
}

function renderDialog(overrides: Partial<StoreValue> = {}, selected = [A, B]) {
  const store = setStore({
    accounts: [A, B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    settings: settings(true),
    ...overrides,
  });
  const onClose = vi.fn();
  render(<BottingDialog open onClose={onClose} />);
  return { store, onClose };
}

const startButton = () => screen.getByRole("button", { name: "Start Botting Mode" });
const placeIdField = () => screen.getByPlaceholderText("Place ID");

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  // The dialog re-reads settings from the backend when it opens.
  setInvokeHandler((cmd) => (cmd === "get_all_settings" ? {} : undefined));
});

afterEach(cleanup);

describe("BottingDialog — start guards", () => {
  it("renders nothing while closed", () => {
    setStore({ accounts: [A, B] });
    const { container } = render(<BottingDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("keeps Start disabled without a Place ID", async () => {
    renderDialog();
    expect(startButton()).toBeDisabled();

    await userEvent.type(placeIdField(), "606849621");
    await waitFor(() => expect(startButton()).toBeEnabled());
  });

  it("keeps Start disabled with fewer than two accounts", async () => {
    renderDialog({}, [A]);
    await userEvent.type(placeIdField(), "606849621");
    expect(startButton()).toBeDisabled();
  });

  it("keeps Start disabled and explains why when Multi Roblox is off", async () => {
    renderDialog({ settings: settings(false) });
    await userEvent.type(placeIdField(), "606849621");

    expect(
      screen.getByText("Botting Mode currently requires Multi Roblox to be enabled")
    ).toBeInTheDocument();
    expect(startButton()).toBeDisabled();
  });

  it("starts the loop with the typed configuration", async () => {
    const { store } = renderDialog();
    await userEvent.type(placeIdField(), "606849621");
    await userEvent.type(screen.getByPlaceholderText("Job ID (optional)"), "job-1");
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.startBottingMode).toHaveBeenCalledWith(
        expect.objectContaining({
          userIds: [1, 2],
          placeId: 606849621,
          jobId: "job-1",
          playerUserIds: [],
        })
      )
    );
  });

  it("surfaces a start failure inline", async () => {
    const { store } = renderDialog();
    (store.startBottingMode as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("failed to enable multi roblox")
    );
    await userEvent.type(placeIdField(), "606849621");
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(
        expect.stringContaining("failed to enable multi roblox")
      )
    );
  });
});

describe("BottingDialog — stop controls", () => {
  it("stops the loop, optionally closing the bot clients", async () => {
    const { store } = renderDialog({
      bottingStatus: makeBottingStatus({ active: true, userIds: [1, 2] }),
    });

    await userEvent.click(screen.getByRole("button", { name: "Stop Botting Mode" }));
    expect(store.stopBottingMode).toHaveBeenCalledWith(false);

    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Bot Accounts" }));
    expect(store.stopBottingMode).toHaveBeenCalledWith(true);
  });
});

describe("BottingDialog — explains the cycle", () => {
  // O ciclo em `src-tauri/src/commands/botting.rs` fecha o cliente da conta
  // (`kill_for_user_graceful_async`) e relanca em seguida a cada intervalo. A
  // tela precisa dizer isso, e dizer que so as contas bot da sessao fecham.
  it("says each cycle closes and reopens the bot client", async () => {
    renderDialog();

    expect(screen.getByText("How each cycle works")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Every rejoin closes that bot account's Roblox client and opens it again, so the account leaves the server and joins back."
      )
    ).toBeInTheDocument();
  });

  it("says only this session's bot accounts are closed", async () => {
    renderDialog();

    expect(
      screen.getByText(
        "Only the bot accounts in this session are closed. Player accounts keep their client, and clients of accounts outside this session are left alone."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Stop + Close Bot Accounts closes those same bot clients; Stop Botting Mode leaves every client open."
      )
    ).toBeInTheDocument();
  });

  it("keeps the explanation in the classic view", async () => {
    renderDialog({ settings: settings(true, { BottingDualPanelDialog: "false" }) });

    await waitFor(() => expect(screen.getByText("How each cycle works")).toBeInTheDocument());
    expect(
      screen.getByText(
        "Every rejoin closes that bot account's Roblox client and opens it again, so the account leaves the server and joins back."
      )
    ).toBeInTheDocument();
  });
});

describe("BottingDialog — timing units", () => {
  const unitLabels = [
    "Rejoin Interval (minutes)",
    "Launch Delay (seconds)",
    "Player Grace (minutes)",
  ];

  it("shows the unit of every timing field in the default view", async () => {
    renderDialog();

    for (const label of unitLabels) {
      expect(screen.getByText(label)).toBeInTheDocument();
      // O campo e um input de texto: sem nome acessivel a unidade nao chega
      // a quem navega por teclado/leitor de tela.
      expect(screen.getByRole("textbox", { name: label })).toBeInTheDocument();
    }
  });

  it("shows the unit of every timing field in the classic view", async () => {
    renderDialog({ settings: settings(true, { BottingDualPanelDialog: "false" }) });

    await waitFor(() => expect(screen.getByText(unitLabels[0])).toBeInTheDocument());
    for (const label of unitLabels) {
      expect(screen.getByText(label)).toBeInTheDocument();
      expect(screen.getByRole("textbox", { name: label })).toBeInTheDocument();
    }
  });

  it("states the accepted range of each timing field", async () => {
    renderDialog();

    // Limites reais: botting.rs `clamp_botting_interval_minutes` (10..120),
    // `clamp_botting_launch_delay_seconds` (5..120) e
    // `resolve_player_grace_minutes` (1..90).
    expect(
      screen.getByText(
        "Rejoin Interval: minutes a bot account stays in the server before its client is closed and reopened (10-120)."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Launch Delay: seconds between two launches, so the accounts do not all start at once (5-120)."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Player Grace: minutes a player account keeps its client after you remove it from Player Accounts, before it joins the cycle (1-90)."
      )
    ).toBeInTheDocument();
  });
});

describe("BottingDialog — draft persistence", () => {
  it("restores the saved draft place/job when it opens", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings"
        ? { General: { BottingDraftPlaceId: "1234", BottingDraftJobId: "saved-job" } }
        : undefined
    );
    renderDialog();

    await waitFor(() => expect(placeIdField()).toHaveValue("1234"));
    expect(screen.getByPlaceholderText("Job ID (optional)")).toHaveValue("saved-job");
  });

  it("persists the place id on blur", async () => {
    const saved: Array<Record<string, unknown>> = [];
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "update_setting") saved.push(args as Record<string, unknown>);
      return undefined;
    });
    renderDialog();

    await userEvent.type(placeIdField(), "606849621");
    await userEvent.tab();

    await waitFor(() =>
      expect(saved).toContainEqual({
        section: "General",
        key: "BottingDraftPlaceId",
        value: "606849621",
      })
    );
  });
});
