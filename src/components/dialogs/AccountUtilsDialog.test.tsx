import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { AccountUtilsDialog } from "./AccountUtilsDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const ACCOUNT = makeAccount({ UserID: 42, Username: "ann", Alias: "Main" });

function renderDialog() {
  const store = setStore({
    accounts: [ACCOUNT],
    selectedIds: new Set([ACCOUNT.UserID]),
    selectedAccounts: [ACCOUNT],
  });
  render(<AccountUtilsDialog open onClose={() => {}} />);
  return store;
}

/** Preenche a senha atual + a nova senha e clica em Change Password. */
async function submitPasswordChange() {
  await userEvent.type(screen.getByPlaceholderText("Current Password"), "old-pw");
  await userEvent.type(screen.getByPlaceholderText("New Password"), "new-pw");
  await userEvent.click(screen.getByRole("button", { name: "Change Password" }));
}

/** Preenche a senha atual + o novo e-mail e clica em Change Email. */
async function submitEmailChange() {
  await userEvent.type(screen.getByPlaceholderText("Current Password"), "old-pw");
  await userEvent.type(
    screen.getByPlaceholderText("New Email (requires current password)"),
    "new@example.com"
  );
  await userEvent.click(screen.getByRole("button", { name: "Change Email" }));
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_robux":
        return 1234;
      case "validate_cookie":
        return { user_id: ACCOUNT.UserID, name: ACCOUNT.Username, is_email_verified: true };
      case "get_private_server_invite_privacy":
        return "AllUsers";
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("AccountUtilsDialog", () => {
  it("does not change the Roblox password without confirmation", async () => {
    renderDialog();
    await submitPasswordChange();

    expect(confirmMock).toHaveBeenCalledTimes(1);
    const [message, destructive] = confirmMock.mock.calls[0];
    expect(message).toContain("Main");
    expect(message).toContain("Roblox account");
    expect(destructive).toBe(true);

    expect(invokeMock).not.toHaveBeenCalledWith("change_password", expect.anything());
  });

  it("changes the Roblox password once confirmed", async () => {
    promptAnswers.confirm = true;
    renderDialog();
    await submitPasswordChange();

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("change_password", {
        userId: ACCOUNT.UserID,
        currentPassword: "old-pw",
        newPassword: "new-pw",
      })
    );
  });

  it("does not change the Roblox email without confirmation", async () => {
    renderDialog();
    await submitEmailChange();

    expect(confirmMock).toHaveBeenCalledTimes(1);
    const [message, destructive] = confirmMock.mock.calls[0];
    expect(message).toContain("Main");
    expect(message).toContain("new@example.com");
    expect(message).toContain("Roblox account");
    expect(destructive).toBe(true);

    expect(invokeMock).not.toHaveBeenCalledWith("change_email", expect.anything());
  });

  it("changes the Roblox email once confirmed", async () => {
    promptAnswers.confirm = true;
    renderDialog();
    await submitEmailChange();

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("change_email", {
        userId: ACCOUNT.UserID,
        password: "old-pw",
        newEmail: "new@example.com",
      })
    );
  });

  /**
   * O PIN de 4 dígitos é o da conta Roblox — não a senha do app nem a da
   * criptografia local. A tela tinha só um campo "PIN (4 digits)" e um botão
   * "Unlock", sem dizer o que destrava nem por quanto tempo.
   */
  it("explains which PIN this is and what unlocking it does", () => {
    renderDialog();

    const explanation = screen.getByText(/Roblox account PIN/i);
    expect(explanation).toBeInTheDocument();
    expect(explanation.textContent).toMatch(/not this app/i);
    expect(explanation.textContent).toMatch(/re-?locks/i);
  });

  it("marks both credential changes as a danger zone", async () => {
    renderDialog();
    expect(screen.getByText("Danger Zone")).toBeInTheDocument();

    for (const name of ["Change Password", "Change Email"]) {
      expect(screen.getByRole("button", { name })).toHaveClass("text-red-400");
    }
  });
});
