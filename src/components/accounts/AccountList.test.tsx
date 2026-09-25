import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { AccountList } from "./AccountList";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";
import type { StoreValue } from "../../store";

const COOKIE_PREFIX =
  "_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items.|";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "one", Group: "Alts" }),
  makeAccount({ UserID: 2, Username: "two", Group: "Alts" }),
];

function renderList(overrides: Partial<StoreValue> = {}) {
  const store = setStore(overrides);
  render(<AccountList />);
  return store;
}

function listEl(): HTMLElement {
  const el = document.querySelector("[data-tour='accounts-list']");
  if (!el) throw new Error("list not rendered");
  return el as HTMLElement;
}

function dropText(target: HTMLElement, text: string) {
  fireEvent.drop(target, { dataTransfer: { getData: () => text } });
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

describe("AccountList — empty and filtered states", () => {
  it("offers to add an account when the list is empty", async () => {
    renderList({ accounts: [] });
    expect(screen.getByText("No accounts yet")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText("Add Account")).toBeInTheDocument();
  });

  /**
   * Quem tem zero conta é justamente quem precisa criar uma. O diálogo do
   * estado vazio escondia as duas entradas que criam conta nova, que só
   * existiam no menu `Add` da toolbar.
   */
  it("reaches both ways of creating a new account from the empty state", async () => {
    renderList({ accounts: [] });
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    expect(await screen.findByRole("button", { name: /^Create Accounts/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Account Generator/ })).toBeInTheDocument();
  });

  it("reports an empty search result instead of the empty-state", () => {
    renderList({ accounts: ACCOUNTS, groups: [], searchQuery: "zzz" });
    expect(screen.getByText(/No matches for/)).toBeInTheDocument();
    expect(screen.queryByText("No accounts yet")).not.toBeInTheDocument();
  });

  it("renders a group header with its rows", () => {
    renderList({ accounts: ACCOUNTS });
    expect(screen.getByText("Alts")).toBeInTheDocument();
    expect(screen.getByText("one")).toBeInTheDocument();
    expect(screen.getByText("two")).toBeInTheDocument();
  });
});

describe("AccountList — cookie text drop", () => {
  it("adds every cookie found in the dropped text", async () => {
    const store = renderList({ accounts: ACCOUNTS });
    dropText(listEl(), `${COOKIE_PREFIX}AAA\nsome noise\n${COOKIE_PREFIX}BBB`);

    await waitFor(() => expect(store.addAccountByCookie).toHaveBeenCalledTimes(2));
    expect(store.addAccountByCookie).toHaveBeenNthCalledWith(1, `${COOKIE_PREFIX}AAA`);
    expect(store.addAccountByCookie).toHaveBeenNthCalledWith(2, `${COOKIE_PREFIX}BBB`);
  });

  it("ignores dropped text that holds no cookie", async () => {
    const store = renderList({ accounts: ACCOUNTS });
    dropText(listEl(), "just some text");
    await Promise.resolve();
    expect(store.addAccountByCookie).not.toHaveBeenCalled();
  });

  it("accepts a cookie drop on the empty state too", async () => {
    const store = renderList({ accounts: [] });
    dropText(screen.getByText("No accounts yet").closest("[data-tour='accounts-list']") as HTMLElement, `${COOKIE_PREFIX}CCC`);
    await waitFor(() => expect(store.addAccountByCookie).toHaveBeenCalledWith(`${COOKIE_PREFIX}CCC`));
  });
});

describe("AccountList — keyboard and background interaction", () => {
  it("moves the selection with the arrow keys", () => {
    const store = renderList({ accounts: ACCOUNTS });
    fireEvent.keyDown(listEl(), { key: "ArrowDown" });
    expect(store.navigateSelection).toHaveBeenCalledWith("down", false);

    fireEvent.keyDown(listEl(), { key: "ArrowUp", shiftKey: true });
    expect(store.navigateSelection).toHaveBeenCalledWith("up", true);
  });

  it("selects all with Ctrl+A and clears with Escape", () => {
    const store = renderList({ accounts: ACCOUNTS });
    fireEvent.keyDown(listEl(), { key: "a", ctrlKey: true });
    expect(store.selectAll).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(listEl(), { key: "Escape" });
    expect(store.deselectAll).toHaveBeenCalledTimes(1);
  });

  it("jumps to the first and last account with Home/End", () => {
    const store = renderList({ accounts: ACCOUNTS });
    fireEvent.keyDown(listEl(), { key: "Home" });
    expect(store.selectSingle).toHaveBeenCalledWith(1);

    fireEvent.keyDown(listEl(), { key: "End" });
    expect(store.selectSingle).toHaveBeenCalledWith(2);
  });

  it("clears the selection when the list background is clicked", () => {
    const store = renderList({ accounts: ACCOUNTS });
    fireEvent.click(listEl());
    expect(store.deselectAll).toHaveBeenCalledTimes(1);
  });

  it("opens the background context menu at the cursor", () => {
    const store = renderList({ accounts: ACCOUNTS });
    fireEvent.contextMenu(listEl(), { clientX: 42, clientY: 84 });
    expect(store.deselectAll).toHaveBeenCalledTimes(1);
    expect(store.openContextMenu).toHaveBeenCalledWith(42, 84);
  });
});
