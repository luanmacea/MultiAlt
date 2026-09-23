import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ContextMenu } from "./ContextMenu";
import { MenuItemView, type MenuItem } from "./MenuItemView";
import { makeAccount, makeBottingStatus, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
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

  it("caps the alias at 30 characters", async () => {
    promptAnswers.prompt = "x".repeat(50);
    const store = renderMenu();
    await userEvent.click(item("Set Alias"));
    await waitFor(() =>
      expect(store.updateAccount).toHaveBeenCalledWith(
        expect.objectContaining({ Alias: "x".repeat(30) })
      )
    );
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
    ["Cookie", "cookie-1\ncookie-2"],
    ["Username", "ann\nbob"],
    ["User ID", "1\n2"],
    ["Profile Link", "https://www.roblox.com/users/1/profile\nhttps://www.roblox.com/users/2/profile"],
  ])("copies the %s of every selected account", async (label, expected) => {
    renderMenu({}, [A, B]);
    await userEvent.click(item(label));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(expected));
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
    setInvokeMap({ get_auth_ticket: "ticket-123" });
    renderMenu({ devMode: true });
    await userEvent.click(item("Get Auth Ticket"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_auth_ticket", { userId: 1 }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("ticket-123"));
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
    expect(screen.queryByText(/Botting Mode/)).not.toBeInTheDocument();
  });

  it("adds the missing accounts to a running botting loop", async () => {
    const store = renderMenu(
      { bottingStatus: makeBottingStatus({ active: true, userIds: [1] }) },
      [A, B]
    );
    await userEvent.click(item("Add 1 account to Botting Mode"));
    await waitFor(() => expect(store.addBottingAccounts).toHaveBeenCalledWith([2]));
  });

  it("says when every selected account is already botting", async () => {
    const store = renderMenu(
      { bottingStatus: makeBottingStatus({ active: true, userIds: [1, 2] }) },
      [A, B]
    );
    await userEvent.click(item("Already in Botting Mode"));
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith("Selected accounts are already in Botting Mode")
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
});
