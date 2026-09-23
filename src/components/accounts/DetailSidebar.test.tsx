import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
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
    expect(screen.queryByText("Roblox Version")).not.toBeInTheDocument();
  });

  it("offers the installed versions as a launch override", async () => {
    setInvokeHandler((cmd) => (cmd === "versions_list_installed" ? [VERSION] : undefined));
    const store = renderSidebar();

    expect(await screen.findByText("Roblox Version")).toBeInTheDocument();
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
