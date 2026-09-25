import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { Toolbar } from "./Toolbar";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { ENABLE_NEXUS } from "../../featureFlags";
import type { StoreValue } from "../../store";

const COOKIE =
  "_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items.|ABC";

function renderToolbar(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ accounts: [makeAccount({ UserID: 1 })], ...overrides });
  render(<Toolbar />);
  return store;
}

async function openAddMenu() {
  await userEvent.click(screen.getByRole("button", { name: /^Add/ }));
}

/**
 * The icon-only toolbar buttons carry no accessible name, so they are addressed
 * by their fixed render order. A "clear search" button is prepended whenever a
 * query is active, and the Nexus button only exists behind its feature flag.
 */
function iconButton(
  name:
    | "clear"
    | "selectAll"
    | "names"
    | "panel"
    | "add"
    | "session"
    | "theme"
    | "nexus"
    | "scripts"
    | "settings",
  hasQuery = false
): HTMLElement {
  const order: string[] = [];
  if (hasQuery) order.push("clear");
  order.push("selectAll", "names", "panel", "add", "session", "theme");
  if (ENABLE_NEXUS) order.push("nexus");
  order.push("scripts", "settings");
  return screen.getAllByRole("button")[order.indexOf(name)];
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});

afterEach(cleanup);

describe("Toolbar — search and toggles", () => {
  it("writes the search query to the store", async () => {
    const store = renderToolbar();
    await userEvent.type(screen.getByPlaceholderText("Filter accounts..."), "a");
    expect(store.setSearchQuery).toHaveBeenLastCalledWith("a");
  });

  it("offers a clear button only while a query is set", async () => {
    renderToolbar();
    const before = screen.getAllByRole("button").length;

    cleanup();
    const store = renderToolbar({ searchQuery: "ann" });
    expect(screen.getAllByRole("button").length).toBe(before + 1);

    await userEvent.click(iconButton("clear", true));
    expect(store.setSearchQuery).toHaveBeenCalledWith("");
  });

  it("toggles select-all", async () => {
    const store = renderToolbar();
    await userEvent.click(iconButton("selectAll"));
    expect(store.toggleSelectAll).toHaveBeenCalledTimes(1);
  });

  it("toggles name hiding and shows the current state", async () => {
    const store = renderToolbar({ hideUsernames: false });
    await userEvent.click(screen.getByRole("button", { name: "Names" }));
    expect(store.setHideUsernames).toHaveBeenCalledWith(true);

    cleanup();
    const store2 = renderToolbar({ hideUsernames: true });
    await userEvent.click(screen.getByRole("button", { name: "Hidden" }));
    expect(store2.setHideUsernames).toHaveBeenCalledWith(false);
  });
});

describe("Toolbar — Add menu", () => {
  it("opens and closes the Add menu", async () => {
    renderToolbar();
    expect(screen.queryByRole("button", { name: "Quick Add" })).not.toBeInTheDocument();

    await openAddMenu();
    expect(screen.getByRole("button", { name: "Quick Add" })).toBeInTheDocument();

    await openAddMenu();
    expect(screen.queryByRole("button", { name: "Quick Add" })).not.toBeInTheDocument();
  });

  it("routes Browser Login to the store", async () => {
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Browser Login" }));
    expect(store.openLoginBrowser).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["User:Pass Login", "userpass"],
    ["Import Cookie", "cookie"],
    ["Import Old Account Data", "legacy"],
  ])("opens the import dialog on the %s tab", async (label, tab) => {
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: label }));
    expect(store.setImportDialogTab).toHaveBeenCalledWith(tab);
    expect(store.setImportDialogOpen).toHaveBeenCalledWith(true);
  });

  /**
   * As duas formas de conseguir conta nova vivem no mesmo diálogo, mas cada
   * entrada do menu abre na sua aba — criar no navegador não pode exigir que o
   * usuário descubra uma aba escondida atrás do gerador por provedor.
   */
  it("opens the generator on the tab the menu entry asked for", async () => {
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Create Accounts" }));
    expect(store.openGeneratorDialog).toHaveBeenCalledWith("signup");

    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Account Generator" }));
    expect(store.openGeneratorDialog).toHaveBeenCalledWith("provider");
  });

  it("opens the versions dialog", async () => {
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Roblox Versions" }));
    expect(store.setVersionsDialogOpen).toHaveBeenCalledWith(true);
  });
});

describe("Toolbar — Quick Add", () => {
  it("treats a pasted cookie as a cookie add", async () => {
    promptAnswers.prompt = COOKIE;
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));

    await waitFor(() => expect(store.addAccountByCookie).toHaveBeenCalledWith(COOKIE));
    expect(invokeMock).not.toHaveBeenCalledWith("lookup_user", expect.anything());
  });

  it("looks a username up and adds it without a cookie", async () => {
    promptAnswers.prompt = "  roboduck  ";
    setInvokeMap({ lookup_user: { id: 77, name: "roboduck" } });
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("lookup_user", { username: "roboduck" })
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("add_account", {
        securityToken: "",
        username: "roboduck",
        userId: 77,
      })
    );
    expect(store.loadAccounts).toHaveBeenCalled();
  });

  /**
   * Conta criada por nome de usuário não tem cookie: não lança, não entra em
   * lugar nenhum. O aviso de sucesso não pode soar igual ao de um cookie
   * válido — tem que dizer que entrou sem sessão e o que falta fazer.
   */
  it("says the username-only account came in without a session", async () => {
    promptAnswers.prompt = "roboduck";
    setInvokeMap({ lookup_user: { id: 77, name: "roboduck" } });
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));

    await waitFor(() => expect(store.addToast).toHaveBeenCalled());
    const message = (store.addToast as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(message).toContain("roboduck");
    expect(message).toContain("no session");
    expect(message).toContain("Browser Login");
  });

  it("does nothing when the prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));
    await Promise.resolve();

    expect(store.addAccountByCookie).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reports a lookup failure as a toast", async () => {
    promptAnswers.prompt = "ghost";
    setInvokeMap({
      lookup_user: () => {
        throw new Error("user not found");
      },
    });
    const store = renderToolbar();
    await openAddMenu();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("user not found"))
    );
  });
});

describe("Toolbar — dialog shortcuts", () => {
  it("opens the theme editor, scripts and settings dialogs", async () => {
    const store = renderToolbar();
    await userEvent.click(iconButton("theme"));
    expect(store.setThemeEditorOpen).toHaveBeenCalledWith(true);

    await userEvent.click(iconButton("scripts"));
    expect(store.setScriptsOpen).toHaveBeenCalledWith(true);

    await userEvent.click(iconButton("settings"));
    expect(store.setSettingsOpen).toHaveBeenCalledWith(true);
  });

  it("toggles the detail sidebar", async () => {
    const store = renderToolbar({ sidebarOpen: false, selectedIds: new Set([1]) });
    await userEvent.click(iconButton("panel"));
    expect(store.setSidebarOpen).toHaveBeenCalledWith(true);
  });

  /**
   * O painel só existe para uma conta (App.tsx). Com 0 ou 2+ selecionadas o
   * botão acendia e nada aparecia — então ele tem que estar desabilitado,
   * dizendo por quê, em vez de mentir que ligou.
   */
  it.each([
    ["nothing selected", [] as number[]],
    ["two accounts selected", [1, 2]],
  ])("disables the panel button with %s", async (_label, ids) => {
    const store = renderToolbar({
      accounts: [makeAccount({ UserID: 1 }), makeAccount({ UserID: 2 })],
      sidebarOpen: false,
      selectedIds: new Set(ids),
    });

    const panel = iconButton("panel");
    expect(panel).toBeDisabled();
    await userEvent.click(panel);
    expect(store.setSidebarOpen).not.toHaveBeenCalled();
  });

  it("keeps the panel button enabled for exactly one selected account", () => {
    renderToolbar({ selectedIds: new Set([1]) });
    expect(iconButton("panel")).toBeEnabled();
  });
});
