import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ImportDialog } from "./ImportDialog";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";

const COOKIE =
  "_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items.|TOKEN";

function renderDialog() {
  const store = setStore({ accounts: [] });
  const onClose = vi.fn();
  render(<ImportDialog open onClose={onClose} defaultTab="cookie" />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});

afterEach(cleanup);

/**
 * O cookie `.ROBLOSECURITY` é a sessão inteira da conta: quem o tem entra como
 * ela, sem senha e sem verificação em duas etapas (`api/auth.rs:45`,
 * `validate_cookie` manda só o cookie e recebe a conta). A tela pedia o dado
 * sem dizer o que era, onde achar nem o que ele entrega.
 */
describe("ImportDialog — the cookie tab explains the cookie", () => {
  it("says the cookie signs in as the account, with no password", () => {
    renderDialog();
    expect(screen.getByText(/signed in as that account/i)).toBeInTheDocument();
  });

  it("says where to find it in the browser", () => {
    renderDialog();
    const where = screen.getByText(/DevTools/);
    expect(where).toHaveTextContent(/Application/);
    expect(where).toHaveTextContent(/Cookies/);
  });

  /**
   * A caixa passou a aceitar `username:password:cookie`, e a senha é mais do
   * que a sessão: troca e-mail e senha, e sair de todas as sessões não a
   * revoga. O aviso que falava só do cookie subestimava o que está em jogo.
   */
  it("says the password raises the stakes over the cookie alone", () => {
    renderDialog();
    const warning = screen.getByText(/also saves the password/i);
    expect(warning).toHaveTextContent(/email and password/i);
    expect(warning).toHaveTextContent(/AccountData\.json/);
  });

  it("offers the username:password:cookie format in the hint", () => {
    renderDialog();
    expect(
      screen.getByText(/Paste one \.ROBLOSECURITY cookie per line, or one username:password:cookie/)
    ).toBeInTheDocument();
  });
});

describe("ImportDialog — importing cookies", () => {
  it("validates and adds one account per pasted line", async () => {
    setInvokeMap({ validate_cookie: { user_id: 5, name: "alt_one" } });
    const { store } = renderDialog();

    await userEvent.type(screen.getByRole("textbox"), COOKIE);
    await userEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_account", {
        securityToken: COOKIE,
        username: "alt_one",
        userId: 5,
      })
    );
    expect(store.loadAccounts).toHaveBeenCalled();
  });

  /**
   * O cookie tem `:` dentro dele (`_|WARNING:-DO-NOT-SHARE...`): um
   * `split(":")` ingênuo manda `_|WARNING` como cookie e grava credencial
   * quebrada. O corte é pelo lugar onde o cookie começa, não pelos dois-pontos.
   */
  it("imports username:password:cookie without cutting the cookie in half", async () => {
    setInvokeMap({ validate_cookie: { user_id: 7, name: "name_from_roblox" } });
    renderDialog();

    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: `alt_two:hunter2:${COOKIE}` },
    });
    await userEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_account", {
        securityToken: COOKIE,
        username: "name_from_roblox",
        userId: 7,
        password: "hunter2",
      })
    );
    expect(invokeMock).toHaveBeenCalledWith("validate_cookie", { cookie: COOKIE });
  });

  it("skips a line that has only username:password, instead of importing half a credential", async () => {
    // O nome gravado é o que `validate_cookie` devolve, não o da linha.
    setInvokeMap({ validate_cookie: { user_id: 9, name: "name_from_roblox" } });
    renderDialog();

    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: `alt_three:hunter2\nalt_four:hunter2:${COOKIE}` },
    });
    await userEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() => expect(screen.getByText(/no cookie in this line/i)).toBeInTheDocument());
    // A linha completa depois dela continua entrando: pular não é abortar.
    expect(invokeMock).toHaveBeenCalledWith("add_account", {
      securityToken: COOKIE,
      username: "name_from_roblox",
      userId: 9,
      password: "hunter2",
    });
    const validated = invokeMock.mock.calls.filter((c) => c[0] === "validate_cookie");
    expect(validated).toHaveLength(1);
  });
});
