import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ContextMenu } from "./ContextMenu";
import { MenuItemView, type MenuItem } from "./MenuItemView";
import {
  defaultSettings,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import {
  confirmMock,
  confirmWithOptOutMock,
  promptAnswers,
  promptMock,
  resetPromptMocks,
} from "../../test-utils/promptMocks";
import type { StoreValue } from "../../store";

const A = makeAccount({ UserID: 1, Username: "ann", Group: "Alts" });
const B = makeAccount({ UserID: 2, Username: "bob", Group: "Farm" });

const writeText = vi.fn(async () => {});

function renderMenu(overrides: Partial<StoreValue> = {}, selected = [A]) {
  const store = setStore({
    accounts: [A, B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    contextMenu: { x: 20, y: 30 },
    ...overrides,
  });
  render(<ContextMenu />);
  return store;
}

const item = (name: string | RegExp) => screen.getByText(name);

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  writeText.mockClear();
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
});

afterEach(cleanup);

describe("ContextMenu — visibility", () => {
  it("renders nothing while it is closed", () => {
    setStore({ accounts: [A], contextMenu: null });
    const { container } = render(<ContextMenu />);
    expect(container).toBeEmptyDOMElement();
  });

  it("closes on Escape", () => {
    const store = renderMenu();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(store.closeContextMenu).toHaveBeenCalledTimes(1);
  });

  it("closes when clicking outside", () => {
    const store = renderMenu();
    fireEvent.mouseDown(document.body);
    expect(store.closeContextMenu).toHaveBeenCalledTimes(1);
  });

  it("closes after an item is picked", async () => {
    promptAnswers.prompt = null;
    const store = renderMenu();
    await userEvent.click(item("Set Alias"));
    expect(store.closeContextMenu).toHaveBeenCalledTimes(1);
  });
});

describe("ContextMenu — account actions", () => {
  it("writes an alias to every selected account", async () => {
    promptAnswers.prompt = "Main";
    const store = renderMenu({}, [A, B]);
    await userEvent.click(item("Set Alias"));
    await waitFor(() => expect(store.updateAccount).toHaveBeenCalledTimes(2));
    expect(store.updateAccount).toHaveBeenCalledWith(expect.objectContaining({ UserID: 1, Alias: "Main" }));
    expect(store.updateAccount).toHaveBeenCalledWith(expect.objectContaining({ UserID: 2, Alias: "Main" }));
  });

  it("caps the alias at 240 characters", async () => {
    promptAnswers.prompt = "x".repeat(260);
    const store = renderMenu();
    await userEvent.click(item("Set Alias"));
    await waitFor(() =>
      expect(store.updateAccount).toHaveBeenCalledWith(
        expect.objectContaining({ Alias: "x".repeat(240) })
      )
    );
  });

  /**
   * Colar 300 caracteres no "Set Alias" do menu gravava 240 e avisava só
   * "Alias updated": o corte era calado (medido no harness — o campo do prompt
   * não tinha `maxlength`). O campo agora trava no mesmo limite da sidebar.
   */
  it("limita o campo do prompt a 240 caracteres, como a sidebar", async () => {
    promptAnswers.prompt = null;
    renderMenu();
    await userEvent.click(item("Set Alias"));
    await waitFor(() => expect(promptMock).toHaveBeenCalled());
    expect(promptMock.mock.calls[0][2]).toEqual({ maxLength: 240 });
  });

  it("se o alias chegar maior que o limite, o aviso diz que ele foi cortado", async () => {
    // O campo já trava em 240; isto é a rede para quem chegar por fora dele.
    promptAnswers.prompt = "x".repeat(260);
    const store = renderMenu();
    await userEvent.click(item("Set Alias"));
    await waitFor(() => expect(store.addToast).toHaveBeenCalled());
    const aviso = (store.addToast as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(aviso).toContain("240");
    expect(aviso).not.toBe("Alias updated");
  });

  it("um alias dentro do limite segue com o aviso de sempre", async () => {
    promptAnswers.prompt = "Main";
    const store = renderMenu();
    await userEvent.click(item("Set Alias"));
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Alias updated"));
  });

  it("leaves the alias alone when the prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    const store = renderMenu();
    await userEvent.click(item("Set Alias"));
    await Promise.resolve();
    expect(store.updateAccount).not.toHaveBeenCalled();
  });

  it("writes a description to every selected account", async () => {
    promptAnswers.prompt = "farm alt";
    const store = renderMenu({}, [A, B]);
    await userEvent.click(item("Set Description"));
    await waitFor(() => expect(store.updateAccount).toHaveBeenCalledTimes(2));
    expect(store.updateAccount).toHaveBeenCalledWith(
      expect.objectContaining({ UserID: 1, Description: "farm alt" })
    );
  });

  it("removes the selected accounts after confirmation", async () => {
    promptAnswers.confirm = true;
    const store = renderMenu({}, [A, B]);
    await userEvent.click(item(/Remove Account/));
    await waitFor(() => expect(store.removeAccounts).toHaveBeenCalledWith([1, 2]));
  });

  it("keeps the accounts when the removal is declined", async () => {
    promptAnswers.confirm = false;
    const store = renderMenu({}, [A, B]);
    await userEvent.click(item(/Remove Account/));
    await Promise.resolve();
    expect(store.removeAccounts).not.toHaveBeenCalled();
  });
});

describe("ContextMenu — copy submenu", () => {
  it.each([
    ["Username", "ann\nbob"],
    ["User ID", "1\n2"],
    ["Profile Link", "https://www.roblox.com/users/1/profile\nhttps://www.roblox.com/users/2/profile"],
  ])("copies the %s of every selected account", async (label, expected) => {
    renderMenu({}, [A, B]);
    await userEvent.click(item(label));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(expected));
  });

  it("copies the cookie of every selected account through the backend once the warning is accepted", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    setInvokeMap({ copy_account_secret: { count: 2, clearsInSecs: 30 } });
    const store = renderMenu({}, [A, B]);
    await userEvent.click(item("Cookie"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_account_secret", { userIds: [1, 2], kind: "cookie" })
    );
    expect(writeText).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith("Copied cookie. Cleared from the clipboard in 30 s.")
    );
  });

  it.each([
    ["Password", "password"],
    ["User:Pass", "userpass"],
  ])("copies the %s through the backend too", async (label, kind) => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    setInvokeMap({ copy_account_secret: { count: 2, clearsInSecs: 30 } });
    renderMenu({}, [A, B]);
    await userEvent.click(item(label));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_account_secret", { userIds: [1, 2], kind })
    );
    expect(writeText).not.toHaveBeenCalled();
  });

  it("falls back to the plain clipboard where the protected copy does not exist", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    setInvokeMap({
      copy_account_secret: () => {
        throw "CLIPBOARD_UNSUPPORTED";
      },
    });
    renderMenu({}, [A, B]);
    await userEvent.click(item("Cookie"));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("cookie-1\ncookie-2"));
  });

  it("hides the developer-only entries unless dev mode is on", () => {
    renderMenu();
    expect(screen.queryByText("rbx-player Link")).not.toBeInTheDocument();
    expect(screen.queryByText("Get Auth Ticket")).not.toBeInTheDocument();

    cleanup();
    renderMenu({ devMode: true });
    expect(screen.getByText("rbx-player Link")).toBeInTheDocument();
    expect(screen.getByText("Get Auth Ticket")).toBeInTheDocument();
  });

  it("copies an auth ticket from the backend in dev mode", async () => {
    promptAnswers.confirm = true;
    setInvokeMap({ get_auth_ticket: "ticket-123" });
    renderMenu({ devMode: true });
    await userEvent.click(item("Get Auth Ticket"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_auth_ticket", { userId: 1 }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("ticket-123"));
  });
});

/**
 * Copiar credencial saía sem aviso: um clique em Cookie punha o
 * `.ROBLOSECURITY` de todas as contas selecionadas na área de transferência —
 * a sessão inteira de cada conta, sem senha e sem 2 etapas para quem pegar.
 */
describe("ContextMenu — copiar credencial avisa antes", () => {
  const P = makeAccount({ UserID: 1, Username: "ann", Password: "pw-ann", Group: "Alts" });
  const Q = makeAccount({ UserID: 2, Username: "bob", Password: "pw-bob", Group: "Farm" });

  function renderWithPasswords(overrides: Partial<StoreValue> = {}) {
    return renderMenu({ accounts: [P, Q], ...overrides }, [P, Q]);
  }

  it.each([
    ["Cookie"],
    ["Password"],
    ["User:Pass"],
  ])("asks before putting the %s of the selection on the clipboard", async (label) => {
    renderWithPasswords();
    await userEvent.click(item(label));
    await waitFor(() => expect(confirmWithOptOutMock).toHaveBeenCalledTimes(1));
    expect(writeText).not.toHaveBeenCalled();
  });

  it("says what a cookie hands over and how many accounts are in the copy", async () => {
    renderWithPasswords();
    await userEvent.click(item("Cookie"));
    await waitFor(() => expect(confirmWithOptOutMock).toHaveBeenCalledTimes(1));
    const [message] = confirmWithOptOutMock.mock.calls[0];
    expect(message).toContain("2 accounts");
    expect(message).toMatch(/2-step verification/);
    expect(message).toMatch(/clipboard/i);
  });

  it("does not ask for the username on its own — it is not a credential", async () => {
    renderWithPasswords();
    await userEvent.click(item("Username"));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("ann\nbob"));
    expect(confirmWithOptOutMock).not.toHaveBeenCalled();
  });

  it("remembers the opt-out in the settings instead of only this session", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: true };
    const store = renderWithPasswords();
    await userEvent.click(item("Cookie"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "General",
        key: "WarnOnCopyCredential",
        value: "false",
      })
    );
    await waitFor(() => expect(store.reloadSettings).toHaveBeenCalled());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_account_secret", { userIds: [1, 2], kind: "cookie" })
    );
  });

  it("skips the warning once it has been turned off", async () => {
    const settings = defaultSettings();
    settings.General.WarnOnCopyCredential = "false";
    renderWithPasswords({ settings });
    await userEvent.click(item("Cookie"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_account_secret", { userIds: [1, 2], kind: "cookie" })
    );
    expect(confirmWithOptOutMock).not.toHaveBeenCalled();
  });

  it.each([
    ["rbx-player Link"],
    ["App Link"],
  ])("says the %s costs a fresh auth ticket before asking Roblox for one", async (label) => {
    setInvokeMap({ get_auth_ticket: "ticket-123" });
    renderMenu({ devMode: true });
    await userEvent.click(item(label));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    const [message] = confirmMock.mock.calls[0];
    expect(message).toMatch(/auth ticket/i);
    expect(message).toContain("ann");
    expect(invokeMock).not.toHaveBeenCalledWith("get_auth_ticket", { userId: 1 });
    expect(writeText).not.toHaveBeenCalled();
  });

  /**
   * O ticket cru entra na area de transferencia igual aos dois links: quem o tiver
   * entra como a conta. Faltava so este item.
   */
  it("says what the raw auth ticket hands over before asking Roblox for one", async () => {
    setInvokeMap({ get_auth_ticket: "ticket-123" });
    renderMenu({ devMode: true });
    await userEvent.click(item("Get Auth Ticket"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    const [message] = confirmMock.mock.calls[0];
    expect(message).toMatch(/auth ticket/i);
    expect(message).toContain("ann");
    expect(invokeMock).not.toHaveBeenCalledWith("get_auth_ticket", { userId: 1 });
    expect(writeText).not.toHaveBeenCalled();
  });

  it("copies the launch link once the ticket warning is accepted", async () => {
    promptAnswers.confirm = true;
    setInvokeMap({ get_auth_ticket: "ticket-123" });
    renderMenu({ devMode: true });
    await userEvent.click(item("rbx-player Link"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_auth_ticket", { userId: 1 }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining("gameinfo:ticket-123"))
    );
  });
});

describe("ContextMenu — group, botting and client entries", () => {
  it("lists the existing groups and moves the selection", async () => {
    const store = renderMenu();
    await userEvent.click(item("Farm"));
    expect(store.moveToGroup).toHaveBeenCalledWith([1], "Farm");
  });

  it("creates a new group from the prompt", async () => {
    promptAnswers.prompt = " Bots ";
    const store = renderMenu();
    await userEvent.click(item("New Group..."));
    await waitFor(() => expect(store.moveToGroup).toHaveBeenCalledWith([1], "Bots"));
  });

  it("hides the botting entry while botting is off", () => {
    renderMenu();
    expect(screen.queryByText(/Auto Rejoin/)).not.toBeInTheDocument();
  });

  it("adds the missing accounts to a running botting loop", async () => {
    const store = renderMenu(
      { bottingStatus: makeBottingStatus({ active: true, userIds: [1] }) },
      [A, B]
    );
    await userEvent.click(item("Add 1 account to Auto Rejoin"));
    await waitFor(() => expect(store.addBottingAccounts).toHaveBeenCalledWith([2]));
  });

  it("says when every selected account is already botting", async () => {
    const store = renderMenu(
      { bottingStatus: makeBottingStatus({ active: true, userIds: [1, 2] }) },
      [A, B]
    );
    await userEvent.click(item("Already in Auto Rejoin"));
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith("Selected accounts are already in Auto Rejoin")
    );
    expect(store.addBottingAccounts).not.toHaveBeenCalled();
  });

  it("offers Focus and Restart only for launched clients", async () => {
    renderMenu();
    expect(screen.queryByText("Focus client")).not.toBeInTheDocument();

    cleanup();
    const store = renderMenu({ launchedByProgram: new Set([1]) });
    await userEvent.click(item("Focus client"));
    await waitFor(() => expect(store.focusRobloxClient).toHaveBeenCalledWith(1));

    cleanup();
    const store2 = renderMenu({ launchedByProgram: new Set([1, 2]) }, [A, B]);
    await userEvent.click(item("Restart clients (2)"));
    await waitFor(() => expect(store2.restartRobloxClients).toHaveBeenCalledWith([1, 2]));
  });

  it("warns when no Roblox window can be focused", async () => {
    const store = renderMenu({ launchedByProgram: new Set([1]) });
    (store.focusRobloxClient as ReturnType<typeof vi.fn>).mockResolvedValue(false);
    await userEvent.click(item("Focus client"));
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith("No active Roblox window found for this account")
    );
  });
});

describe("ContextMenu — quick login", () => {
  /**
   * O prompt só dizia "Enter 6-digit code:" e não contava de onde vem o código
   * nem o que o app faz com ele: o Roblox mostra o código em roblox.com/login
   * no aparelho que vai entrar, e o app o manda com a sessão da conta
   * selecionada.
   */
  it("says where the 6-digit code comes from and whose session sends it", async () => {
    promptAnswers.prompt = "123456";
    renderMenu();
    await userEvent.click(item("Quick Login"));

    await waitFor(() => expect(promptMock).toHaveBeenCalledTimes(1));
    const [message] = promptMock.mock.calls[0];
    expect(message).toContain("roblox.com/login");
    expect(message).toContain("ann");
    expect(message).toMatch(/6-digit/);
  });

  it("names the account in the toast after the code goes through", async () => {
    promptAnswers.prompt = "123456";
    const store = renderMenu();
    await userEvent.click(item("Quick Login"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("quick_login_enter_code", { userId: 1, code: "123456" })
    );
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("ann"))
    );
  });

  /**
   * Com o código já copiado o app nem pergunta — manda em silêncio. O toast é o
   * único lugar onde dá para contar que o código saiu da área de transferência.
   */
  it("says out loud when the code came from the clipboard", async () => {
    const readText = vi.fn(async () => "123 456");
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText, readText },
      configurable: true,
    });

    const store = renderMenu();
    await userEvent.click(item("Quick Login"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("quick_login_enter_code", { userId: 1, code: "123456" })
    );
    expect(promptMock).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringMatching(/clipboard/i))
    );
  });
});

/**
 * Com "Names hidden" o menu ainda punha o nome real nas confirmações, nos
 * prompts, nos toasts, no título e no corpo do "Show Details" e no campo do
 * "Set Alias".
 */
describe("ContextMenu — nomes ocultos", () => {
  const SECRET = makeAccount({ UserID: 1, Username: "secretann", Alias: "AliasAnn" });
  const HIDDEN = { accounts: [SECRET, B], hideUsernames: true, hiddenNameLetters: 0 };
  const realName = /secretann|AliasAnn/;

  it("a confirmação de remover não diz o nome", async () => {
    promptAnswers.confirm = false;
    renderMenu(HIDDEN, [SECRET]);
    await userEvent.click(item(/Remove Account/));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(confirmMock.mock.calls[0][0]).toBe("Remove ************?");
  });

  it("o Quick Login não diz o nome no prompt nem no toast", async () => {
    promptAnswers.prompt = "123456";
    const store = renderMenu(HIDDEN, [SECRET]);
    await userEvent.click(item("Quick Login"));
    await waitFor(() => expect(store.addToast).toHaveBeenCalled());
    expect(promptMock.mock.calls[0][0]).not.toMatch(realName);
    expect(promptMock.mock.calls[0][0]).toContain("************");
    expect(String(vi.mocked(store.addToast).mock.calls[0][0])).not.toMatch(realName);
  });

  it("as confirmações de link e ticket não dizem o nome", async () => {
    setInvokeMap({ get_auth_ticket: "ticket-123" });
    renderMenu({ ...HIDDEN, devMode: true }, [SECRET]);
    await userEvent.click(item("Get Auth Ticket"));
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(confirmMock.mock.calls[0][0]).not.toMatch(realName);
  });

  it("o Show Details mascara título e nomes", async () => {
    const store = renderMenu(HIDDEN, [SECRET]);
    await userEvent.click(item("Show Details"));
    expect(store.showModal).toHaveBeenCalledTimes(1);
    const [title, body] = vi.mocked(store.showModal).mock.calls[0];
    expect(title).toBe("************");
    expect(String(body)).not.toMatch(realName);
  });

  it("o Set Alias abre o campo em bolinhas", async () => {
    renderMenu(HIDDEN, [SECRET]);
    await userEvent.click(item("Set Alias"));
    await waitFor(() => expect(promptMock).toHaveBeenCalled());
    expect(promptMock.mock.calls[0][2]).toEqual({ maxLength: 240, masked: true });
  });
});

describe("MenuItemView", () => {
  it("runs the action and closes the menu", async () => {
    const action = vi.fn();
    const close = vi.fn();
    render(<MenuItemView item={{ label: "Do it", action }} close={close} />);
    await userEvent.click(screen.getByText("Do it"));
    expect(action).toHaveBeenCalledTimes(1);
    expect(close).toHaveBeenCalledTimes(1);
  });

  it("renders a separator without a label", () => {
    const { container } = render(<MenuItemView item={{ label: "", separator: true }} close={vi.fn()} />);
    expect(container.textContent).toBe("");
  });

  it("renders a submenu's children", () => {
    const submenu: MenuItem[] = [{ label: "Child A" }, { label: "Child B" }];
    render(<MenuItemView item={{ label: "Parent", submenu }} close={vi.fn()} />);
    expect(screen.getByText("Parent")).toBeInTheDocument();
    expect(screen.getByText("Child A")).toBeInTheDocument();
    expect(screen.getByText("Child B")).toBeInTheDocument();
  });

  it("still closes an item that has no action", async () => {
    const close = vi.fn();
    render(<MenuItemView item={{ label: "Inert" }} close={close} />);
    await userEvent.click(screen.getByText("Inert"));
    expect(close).toHaveBeenCalledTimes(1);
  });

  /**
   * O menu de contexto é a superfície principal de ação do app e era só
   * `<div onClick>`: sem `role`, sem foco, sem teclado. Estes testes cobrem a
   * semântica e a navegação por teclado que a onda 2 deu a ele.
   */
  describe("teclado", () => {
    it("is reachable by tab and activates with Enter", async () => {
      const action = vi.fn();
      const close = vi.fn();
      render(<MenuItemView item={{ label: "Do it", action }} close={close} />);
      const menuitem = screen.getByRole("menuitem", { name: "Do it" });
      expect(menuitem.tabIndex).toBe(0);

      menuitem.focus();
      await userEvent.keyboard("{Enter}");
      expect(action).toHaveBeenCalledTimes(1);
      expect(close).toHaveBeenCalledTimes(1);
    });

    it("activates with Space too", async () => {
      const action = vi.fn();
      render(<MenuItemView item={{ label: "Do it", action }} close={vi.fn()} />);
      screen.getByRole("menuitem", { name: "Do it" }).focus();
      await userEvent.keyboard(" ");
      expect(action).toHaveBeenCalledTimes(1);
    });

    it("moves focus between siblings with the arrow keys", async () => {
      render(
        <div>
          <MenuItemView item={{ label: "First" }} close={vi.fn()} />
          <MenuItemView item={{ label: "Second" }} close={vi.fn()} />
        </div>
      );
      const first = screen.getByRole("menuitem", { name: "First" });
      const second = screen.getByRole("menuitem", { name: "Second" });

      first.focus();
      await userEvent.keyboard("{ArrowDown}");
      expect(second).toHaveFocus();

      await userEvent.keyboard("{ArrowUp}");
      expect(first).toHaveFocus();
    });

    it("skips separators while navigating with the arrow keys", async () => {
      render(
        <div>
          <MenuItemView item={{ label: "First" }} close={vi.fn()} />
          <MenuItemView item={{ label: "", separator: true }} close={vi.fn()} />
          <MenuItemView item={{ label: "Second" }} close={vi.fn()} />
        </div>
      );
      screen.getByRole("menuitem", { name: "First" }).focus();
      await userEvent.keyboard("{ArrowDown}");
      expect(screen.getByRole("menuitem", { name: "Second" })).toHaveFocus();
    });

    it("opens a submenu with ArrowRight and focuses its first child; ArrowLeft returns focus", async () => {
      const submenu: MenuItem[] = [{ label: "Child A" }, { label: "Child B" }];
      render(<MenuItemView item={{ label: "Parent", submenu }} close={vi.fn()} />);
      const trigger = screen.getByRole("menuitem", { name: "Parent" });

      trigger.focus();
      await userEvent.keyboard("{ArrowRight}");
      expect(screen.getByRole("menuitem", { name: "Child A" })).toHaveFocus();

      await userEvent.keyboard("{ArrowLeft}");
      expect(trigger).toHaveFocus();
    });

    it("also opens the submenu with Enter", async () => {
      const submenu: MenuItem[] = [{ label: "Child A" }];
      render(<MenuItemView item={{ label: "Parent", submenu }} close={vi.fn()} />);
      screen.getByRole("menuitem", { name: "Parent" }).focus();
      await userEvent.keyboard("{Enter}");
      expect(screen.getByRole("menuitem", { name: "Child A" })).toHaveFocus();
    });
  });
});
