import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { BULK_CREDENTIALS_GAP_MS, BulkCredentialsDialog } from "./BulkCredentialsDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, resetPromptMocks } from "../../test-utils/promptMocks";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "withpass", Password: "stored1" }),
  makeAccount({ UserID: 2, Username: "nopass", Password: "" }),
];

function renderDialog(accounts = ACCOUNTS) {
  const store = setStore({
    accounts,
    selectedIds: new Set(accounts.map((a) => a.UserID)),
    selectedAccounts: accounts,
  });
  render(<BulkCredentialsDialog open onClose={() => {}} />);
  return store;
}

beforeEach(() => {
  BULK_CREDENTIALS_GAP_MS.value = 0;
  resetTauriMocks();
  resetPromptMocks();
  confirmMock.mockResolvedValue(true);
});
afterEach(cleanup);

describe("BulkCredentialsDialog", () => {
  it("disables the button until a change is chosen and valid", async () => {
    renderDialog();
    const run = screen.getByRole("button", { name: /Change .* account/ });
    expect(run).toBeDisabled();
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "short");
    expect(screen.getByText(/at least 8 characters/)).toBeInTheDocument();
    expect(run).toBeDisabled();
    await userEvent.type(screen.getByPlaceholderText(/New password/), "enough99");
    expect(run).toBeEnabled();
  });

  it("skips accounts with no current password, runs the one that has it", async () => {
    setInvokeHandler((cmd) => (cmd === "change_password" ? null : undefined));
    renderDialog();
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    expect(screen.getByText(/1 will change, 1 will be skipped/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Change 1 account/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("change_password", {
        userId: 1,
        currentPassword: "stored1",
        newPassword: "newpass12",
      })
    );
    // conta 2 nunca foi chamada
    expect(invokeMock).not.toHaveBeenCalledWith("change_password", expect.objectContaining({ userId: 2 }));
    expect(within(screen.getByTestId("bulk-cred-row-2")).getByText(/skipped/)).toBeInTheDocument();
  });

  it("uses a shared current password so the account with none can change too", async () => {
    setInvokeHandler((cmd) => (cmd === "change_password" ? null : undefined));
    renderDialog();
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    await userEvent.type(screen.getByPlaceholderText(/Leave empty/), "common99");
    await userEvent.click(screen.getByRole("button", { name: /Change 2 account/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("change_password", {
        userId: 2,
        currentPassword: "common99",
        newPassword: "newpass12",
      })
    );
    // a conta com senha guardada ainda usa a dela, não a compartilhada
    expect(invokeMock).toHaveBeenCalledWith("change_password", {
      userId: 1,
      currentPassword: "stored1",
      newPassword: "newpass12",
    });
  });

  it("shows a per-account error without stopping the others", async () => {
    setInvokeHandler((cmd, args) => {
      if (cmd !== "change_password") return undefined;
      if ((args as { userId: number }).userId === 1) throw new Error("Roblox asked to verify");
      return null;
    });
    renderDialog();
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    await userEvent.type(screen.getByPlaceholderText(/Leave empty/), "common99");
    await userEvent.click(screen.getByRole("button", { name: /Change 2 account/ }));
    await waitFor(() =>
      expect(within(screen.getByTestId("bulk-cred-row-1")).getByText(/asked to verify/)).toBeInTheDocument()
    );
    expect(within(screen.getByTestId("bulk-cred-row-2")).getByText("done")).toBeInTheDocument();
  });

  it("asks for confirmation before changing the real accounts", async () => {
    confirmMock.mockResolvedValue(false);
    renderDialog();
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    await userEvent.click(screen.getByRole("button", { name: /Change 1 account/ }));
    expect(confirmMock).toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("change_password", expect.anything());
  });

  it("changes the email with the current password, and after a password change uses the new one", async () => {
    setInvokeHandler((cmd) => (cmd === "change_password" ? null : cmd === "change_email" ? null : undefined));
    renderDialog([ACCOUNTS[0]]);
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    await userEvent.click(screen.getByLabelText("Change the email"));
    await userEvent.type(screen.getByPlaceholderText(/New email/), "new@example.com");
    await userEvent.click(screen.getByRole("button", { name: /Change 1 account/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("change_email", {
        userId: 1,
        password: "newpass12",
        newEmail: "new@example.com",
      })
    );
  });

  /**
   * Relato do dono (15/10/2026): no lote, umas trocavam e outras diziam só
   * "Failed to change password". Agora a linha diz o motivo que o Roblox deu.
   */
  it("says why Roblox refused each account, in plain words", async () => {
    setInvokeHandler((cmd, args) => {
      if (cmd !== "change_password") return undefined;
      const id = (args as { userId: number }).userId;
      if (id === 1) throw "credential-error:rate-limited:429:";
      throw "credential-error:wrong-current-password:403:Incorrect password";
    });
    renderDialog();
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    await userEvent.type(screen.getByPlaceholderText(/Leave empty/), "common99");
    await userEvent.click(screen.getByRole("button", { name: /Change 2 account/ }));

    await waitFor(() =>
      expect(within(screen.getByTestId("bulk-cred-row-1")).getByText(/too many changes in a row/)).toBeInTheDocument()
    );
    expect(within(screen.getByTestId("bulk-cred-row-2")).getByText(/current password is wrong/)).toBeInTheDocument();
    expect(within(screen.getByTestId("bulk-cred-row-2")).getByText(/Incorrect password/)).toBeInTheDocument();
    expect(screen.getByText(/1 were blocked for now by Roblox/)).toBeInTheDocument();
  });

  it("Retry failed only runs the failed accounts, and never changes a password twice", async () => {
    let failEmail = true;
    setInvokeHandler((cmd, args) => {
      const id = (args as { userId: number }).userId;
      if (cmd === "change_password") return null;
      if (cmd === "change_email" && id === 1 && failEmail) throw "credential-error:verification-captcha:403:";
      return null;
    });
    renderDialog([ACCOUNTS[0], { ...ACCOUNTS[1], Password: "stored2" }]);
    await userEvent.click(screen.getByLabelText("Change the password"));
    await userEvent.type(screen.getByPlaceholderText(/New password/), "newpass12");
    await userEvent.click(screen.getByLabelText("Change the email"));
    await userEvent.type(screen.getByPlaceholderText(/New email/), "new@example.com");
    await userEvent.click(screen.getByRole("button", { name: /Change 2 account/ }));
    await waitFor(() => expect(screen.getByTestId("bulk-cred-retry")).toHaveTextContent("Retry failed (1)"));

    invokeMock.mockClear();
    failEmail = false;
    await userEvent.click(screen.getByTestId("bulk-cred-retry"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("change_email", {
        userId: 1,
        password: "newpass12",
        newEmail: "new@example.com",
      })
    );
    // A senha da conta 1 já tinha trocado; a conta 2 já estava feita.
    expect(invokeMock).not.toHaveBeenCalledWith("change_password", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("change_email", expect.objectContaining({ userId: 2 }));
  });
});
