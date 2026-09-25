import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
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
});
