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
