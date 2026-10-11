import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { DetailSidebar } from "./DetailSidebar";
import { defaultSettings, makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { StoreValue } from "../../store";

const A = makeAccount({ UserID: 1, Username: "ann", Description: "farm alt" });
const B = makeAccount({ UserID: 2, Username: "bob" });

const VERSION = {
  channel: "LIVE",
  versionHash: "version-abcdef0123456789",
  displayVersion: "1.2.3",
  userLabel: null as string | null,
};

function renderSidebar(overrides: Partial<StoreValue> = {}, selected = [A]) {
  const store = setStore({
    accounts: [A, B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    ...overrides,
  });
  render(<DetailSidebar />);
  return store;
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeHandler((cmd) => (cmd === "versions_list_installed" ? [] : undefined));
});

afterEach(cleanup);

describe("DetailSidebar", () => {
  it("renders nothing without a selection", () => {
    setStore({ accounts: [A, B] });
    const { container } = render(<DetailSidebar />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing for a multi-account selection", () => {
    setStore({
      accounts: [A, B],
      selectedIds: new Set([1, 2]),
      selectedAccounts: [A, B],
    });
    const { container } = render(<DetailSidebar />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the account identity and validity", () => {
    renderSidebar();
    expect(screen.getByText("ann")).toBeInTheDocument();
    expect(screen.getByText("Valid")).toBeInTheDocument();
    expect(screen.getByText("ID: 1")).toBeInTheDocument();
  });

  it("marks an invalid session", () => {
    const invalid = makeAccount({ UserID: 3, Username: "cid", Valid: false });
    renderSidebar({ accounts: [invalid] }, [invalid]);
    expect(screen.getByText("Invalid")).toBeInTheDocument();
  });

  it("hides the user id and masks the name when names are hidden", () => {
    renderSidebar({ hideUsernames: true, hiddenNameLetters: 1 });
    expect(screen.getByText("a********")).toBeInTheDocument();
    expect(screen.queryByText("ID: 1")).not.toBeInTheDocument();
  });

  /**
   * Achado ao tirar as fotos do README (03/10/2026): com os nomes ocultos, o
   * campo de apelido ainda mostrava o nome de usuário como texto de exemplo (e
   * o apelido salvo, como valor) — o nome vazava na tela que devia escondê-lo.
   */
  it("does not leak the username or the alias through the alias field when names are hidden", () => {
    const withAlias = makeAccount({ UserID: 5, Username: "ann", Alias: "Main" });
    renderSidebar({ accounts: [withAlias], hideUsernames: true, hiddenNameLetters: 1 }, [withAlias]);
    expect(screen.queryByPlaceholderText("ann")).not.toBeInTheDocument();
    const field = screen.getByPlaceholderText("Alias");
    expect(field).toHaveClass("masked-input");
  });

  it("saves an alias from the Set button and from Enter", async () => {
    const store = renderSidebar();
    const aliasField = screen.getByPlaceholderText("ann");

    await userEvent.type(aliasField, "Main");
    await userEvent.click(screen.getByRole("button", { name: "Set" }));
    expect(store.updateAccount).toHaveBeenCalledWith(expect.objectContaining({ Alias: "Main" }));

    await userEvent.type(aliasField, "!{Enter}");
    expect(store.updateAccount).toHaveBeenLastCalledWith(
      expect.objectContaining({ Alias: "Main!" })
    );
  });

  it("accepts an alias up to 240 characters and cuts anything past that", async () => {
    const store = renderSidebar();
    const aliasField = screen.getByPlaceholderText("ann");

    // maxLength on the input already stops typing past 240; paste bypasses
    // that, so handleSetAlias must enforce the same limit on its own.
    fireEvent.change(aliasField, { target: { value: "x".repeat(260) } });
    await userEvent.click(screen.getByRole("button", { name: "Set" }));
    expect(store.updateAccount).toHaveBeenCalledWith(
      expect.objectContaining({ Alias: "x".repeat(240) })
    );
  });

  it("masks a 240-character alias the same way as a short one", () => {
    const longAlias = "a".repeat(240);
    const long = makeAccount({ UserID: 4, Username: "dee", Alias: longAlias });
    renderSidebar({ accounts: [long], hideUsernames: true, hiddenNameLetters: 3 }, [long]);
    expect(screen.getByText("aaa********")).toBeInTheDocument();
  });

  it("saves the description", async () => {
    const store = renderSidebar();
    const notes = screen.getByPlaceholderText("Notes...");
    expect(notes).toHaveValue("farm alt");

    await userEvent.clear(notes);
    await userEvent.type(notes, "main account");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(store.updateAccount).toHaveBeenCalledWith(
      expect.objectContaining({ Description: "main account" })
    );
  });

  it("hides the version override when nothing is installed", async () => {
    renderSidebar();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("versions_list_installed"));
    expect(screen.queryByText(/Roblox Version/)).not.toBeInTheDocument();
  });

  it("says the version override is global instead of per account", async () => {
    setInvokeHandler((cmd) => (cmd === "versions_list_installed" ? [VERSION] : undefined));
    renderSidebar();

    expect(await screen.findByText("Roblox Version (all accounts)")).toBeInTheDocument();
    expect(
      screen.getByText("Global setting — every account launches with this version, not just this one.")
    ).toBeInTheDocument();
    expect(screen.queryByText(/this account launches/i)).not.toBeInTheDocument();
  });

  it("offers the installed versions as a launch override", async () => {
    setInvokeHandler((cmd) => (cmd === "versions_list_installed" ? [VERSION] : undefined));
    const store = renderSidebar();

    expect(await screen.findByText("Roblox Version (all accounts)")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Latest installed" }));
    await userEvent.click(screen.getByRole("button", { name: "LIVE · 1.2.3" }));
    expect(store.setDefaultVersion).toHaveBeenCalledWith("LIVE:version-abcdef0123456789");
  });

  it("clears the override back to the latest installed version", async () => {
    setInvokeHandler((cmd) => (cmd === "versions_list_installed" ? [VERSION] : undefined));
    const settings = defaultSettings();
    settings.Versions = { DefaultVersion: "LIVE:version-abcdef0123456789" };
    const store = renderSidebar({ settings });

    await userEvent.click(await screen.findByRole("button", { name: "LIVE · 1.2.3" }));
    await userEvent.click(screen.getByRole("button", { name: "Latest installed" }));
    expect(store.setDefaultVersion).toHaveBeenCalledWith(null);
  });

  /**
   * A reconexão automática saiu do painel de uma conta (10/10/2026): quem tem
   * muitas contas não abre o painel de cada uma. Ela mora na página Session
   * (padrão no resumo, uma chave por conta na lista "Em jogo").
   */
  it("has no auto-reconnect settings: they live on the Session page", () => {
    renderSidebar({ settings: { General: { AutoReconnect: "true" } } });
    expect(screen.getByText("Description")).toBeInTheDocument();
    expect(screen.queryByText("Auto-reconnect")).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Reconnect automatically/ })).not.toBeInTheDocument();
  });

  it("opens the tools it delegates to the store", async () => {
    const store = renderSidebar();
    await userEvent.click(screen.getByRole("button", { name: "Server List" }));
    expect(store.setServerListOpen).toHaveBeenCalledWith(true);

    await userEvent.click(screen.getByRole("button", { name: "Utilities" }));
    expect(store.setAccountUtilsOpen).toHaveBeenCalledWith(true);

    await userEvent.click(screen.getByRole("button", { name: "Browser" }));
    expect(store.openAccountBrowser).toHaveBeenCalledWith(1);
  });

  it("joins a group by id", async () => {
    promptAnswers.prompt = " 7654321 ";
    const store = renderSidebar();
    await userEvent.click(screen.getByRole("button", { name: "Join Group" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("join_group", { userId: 1, groupId: 7654321 })
    );
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Joined group 7654321"));
  });

  it("rejects a non-numeric group id", async () => {
    promptAnswers.prompt = "not-a-group";
    const store = renderSidebar();
    await userEvent.click(screen.getByRole("button", { name: "Join Group" }));

    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Invalid group ID"));
    expect(invokeMock).not.toHaveBeenCalledWith("join_group", expect.anything());
  });
});

/** Ideia 8: moderação no painel da conta, com consulta sob demanda. */
describe("DetailSidebar — moderation", () => {
  it("checks the ban status on demand", async () => {
    const store = renderSidebar();
    await userEvent.click(screen.getByRole("button", { name: "Check ban status" }));
    expect(store.checkModeration).toHaveBeenCalledWith(1);
  });

  it("says nothing was checked yet before the first check", () => {
    renderSidebar();
    expect(screen.getByText("Ban status not checked yet")).toBeInTheDocument();
  });

  it("shows the state and the moderator note once known", () => {
    renderSidebar({
      moderationByUserId: new Map([
        [1, { state: "warned", until: null, note: "Be nice in chat", punishment: "Warn" }],
      ]),
    });
    expect(screen.getByText("Warned")).toBeInTheDocument();
    expect(screen.getByText(/Be nice in chat/)).toBeInTheDocument();
  });

  it("shows a clean account as such", () => {
    renderSidebar({
      moderationByUserId: new Map([[1, { state: "clean", until: null, note: null, punishment: null }]]),
    });
    expect(screen.getByText("No moderation")).toBeInTheDocument();
  });
});

/** Revisão da UI (pacote Organização): o painel cresceu e o uso diário ficou lá embaixo. */
describe("DetailSidebar — layout", () => {
  beforeEach(() => {
    try {
      window.localStorage.clear();
    } catch {
      // sem storage: os testes abaixo não dependem dele existir
    }
  });

  function sectionOrder(): string[] {
    return [...document.querySelectorAll<HTMLElement>("[data-sidebar-section]")].map(
      (el) => el.dataset.sidebarSection ?? ""
    );
  }

  it("puts the everyday controls first and the history last", async () => {
    renderSidebar();
    await screen.findByText(/Nothing yet/);
    expect(sectionOrder()).toEqual(["Tools", "Launch Exceptions", "Alias", "Description", "History"]);
  });

  it("history can be collapsed, and stays collapsed next time", async () => {
    renderSidebar();
    const toggle = await screen.findByRole("button", { name: "History" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(await screen.findByText(/Nothing yet/)).toBeInTheDocument();

    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText(/Nothing yet/)).not.toBeInTheDocument();

    cleanup();
    renderSidebar();
    expect(await screen.findByRole("button", { name: "History" })).toHaveAttribute("aria-expanded", "false");
  });

  it("opens the next account at the top, not where the last one was scrolled", () => {
    setStore({ accounts: [A, B], selectedIds: new Set([1]), selectedAccounts: [A], selectedAccount: A });
    const { rerender } = render(<DetailSidebar />);
    const scroller = screen.getByTestId("account-sidebar-scroll");
    scroller.scrollTop = 900;

    setStore({ accounts: [A, B], selectedIds: new Set([2]), selectedAccounts: [B], selectedAccount: B });
    rerender(<DetailSidebar />);
    expect(screen.getByTestId("account-sidebar-scroll").scrollTop).toBe(0);
  });

  it("a long kick message wraps (up to 3 lines) with the full text in the tooltip", () => {
    const message = "You have been kicked for being AFK too long in this server, please rejoin later";
    renderSidebar({
      launchedByProgram: new Set([1]),
      clientHealth: new Map([
        [1, { pid: 10, logFound: true, drop: { kind: "kicked", reason: null, code: 267, message, sinceMs: 0 } }],
      ]),
    });
    const note = screen.getByTestId("client-health-note");
    expect(note.getAttribute("title")).toContain(message);
    const text = within(note).getByText(new RegExp(message.slice(0, 20)));
    expect(text).toHaveClass("line-clamp-3");
    expect(text).not.toHaveClass("truncate");
  });
});
