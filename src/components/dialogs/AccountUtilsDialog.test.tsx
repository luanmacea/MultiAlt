import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

  /**
   * O campo aceitava só dígitos crus via `parseInt` e, se a pessoa colasse um
   * link, falhava em silêncio (`return` sem toast nenhum). Agora aceita um
   * link colado e sempre diz por que não deu, quando não dá.
   */
  describe("Universe ID", () => {
    it("loads places from a plain numeric Universe ID", async () => {
      setInvokeHandler((cmd) => {
        switch (cmd) {
          case "get_robux":
            return 1234;
          case "validate_cookie":
            return { user_id: ACCOUNT.UserID, name: ACCOUNT.Username, is_email_verified: true };
          case "get_private_server_invite_privacy":
            return "AllUsers";
          case "get_universe_places":
            return [{ id: 606849621, name: "Jailbreak" }];
          default:
            return undefined;
        }
      });
      renderDialog();

      await userEvent.type(screen.getByPlaceholderText("Universe ID"), "555");
      await userEvent.click(screen.getByRole("button", { name: "Load Places" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("get_universe_places", {
          universeId: 555,
          userId: ACCOUNT.UserID,
        })
      );
      expect(await screen.findByText(/Jailbreak/)).toBeInTheDocument();
    });

    it("extracts the Universe ID from a pasted link instead of failing silently", async () => {
      setInvokeHandler((cmd) => {
        switch (cmd) {
          case "get_robux":
            return 1234;
          case "validate_cookie":
            return { user_id: ACCOUNT.UserID, name: ACCOUNT.Username, is_email_verified: true };
          case "get_private_server_invite_privacy":
            return "AllUsers";
          case "get_universe_places":
            return [{ id: 1, name: "Some Place" }];
          default:
            return undefined;
        }
      });
      renderDialog();

      await userEvent.type(
        screen.getByPlaceholderText("Universe ID"),
        "https://www.roblox.com/places/1818/universes/configure?universeId=987654"
      );
      await userEvent.click(screen.getByRole("button", { name: "Load Places" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("get_universe_places", {
          universeId: 987654,
          userId: ACCOUNT.UserID,
        })
      );
    });

    it("explains why it could not resolve a Universe ID instead of doing nothing", async () => {
      const { addToast } = renderDialog();

      await userEvent.type(screen.getByPlaceholderText("Universe ID"), "not a universe id");
      await userEvent.click(screen.getByRole("button", { name: "Load Places" }));

      await waitFor(() => expect(addToast).toHaveBeenCalled());
      const [message] = vi.mocked(addToast).mock.calls[0];
      expect(message).toMatch(/universe id/i);
      expect(invokeMock).not.toHaveBeenCalledWith("get_universe_places", expect.anything());
    });
  });

  /**
   * `JSON.parse` corria dentro do `try` do `set_avatar`: um JSON errado só
   * aparecia como o toast genérico "Error: ...", depois de já ter tentado
   * chamar o backend. Agora valida antes, no onChange, e aponta para o
   * caminho que já monta o JSON sozinho (Wear Outfit).
   */
  describe("Custom Avatar JSON", () => {
    function jsonField() {
      return screen.getByPlaceholderText('{"assets":[{"id":12345}]}');
    }

    it("shows an inline error for invalid JSON without calling set_avatar", async () => {
      renderDialog();

      // userEvent.type interpreta `{` como início de tecla especial; um textarea
      // JSON precisa de fireEvent.change para colar chaves literais.
      fireEvent.change(jsonField(), { target: { value: "{not valid json" } });
      expect(await screen.findByText(/check the commas, braces and quotes/i)).toBeInTheDocument();
      // A mensagem do motor JS ("Expected property name or '}' in JSON at
      // position 1") e inglesa e cripta para quem joga: nao pode chegar a tela.
      expect(screen.queryByText(/Expected property name/i)).not.toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "Apply Avatar JSON" }));
      expect(invokeMock).not.toHaveBeenCalledWith("set_avatar", expect.anything());
    });

    it("points to Wear Outfit as the easier path for named items", () => {
      renderDialog();
      expect(screen.getByText(/Wear Outfit/)).toBeInTheDocument();
    });

    it("applies valid avatar JSON once it parses cleanly", async () => {
      setInvokeHandler((cmd) => {
        switch (cmd) {
          case "get_robux":
            return 1234;
          case "validate_cookie":
            return { user_id: ACCOUNT.UserID, name: ACCOUNT.Username, is_email_verified: true };
          case "get_private_server_invite_privacy":
            return "AllUsers";
          case "set_avatar":
            return [];
          default:
            return undefined;
        }
      });
      renderDialog();

      fireEvent.change(jsonField(), { target: { value: '{"assets":[{"id":12345}]}' } });
      await userEvent.click(screen.getByRole("button", { name: "Apply Avatar JSON" }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("set_avatar", {
          userId: ACCOUNT.UserID,
          avatarJson: { assets: [{ id: 12345 }] },
        })
      );
    });
  });
});
