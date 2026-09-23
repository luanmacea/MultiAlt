import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { AccountRow } from "./AccountRow";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import type { StoreValue } from "../../store";

const ACCOUNT = makeAccount({ UserID: 501, Username: "roboduck", Group: "Alts" });

function renderRow(overrides: Partial<StoreValue> = {}, account = ACCOUNT) {
  const store = setStore({ accounts: [account], ...overrides });
  render(<AccountRow account={account} />);
  return store;
}

function row(): HTMLElement {
  const el = document.querySelector("[data-account-row='true']");
  if (!el) throw new Error("account row not rendered");
  return el as HTMLElement;
}

function dragHandle(): HTMLElement {
  return screen.getByTitle("Drag to reorder");
}

function fakeDataTransfer() {
  return { setData: vi.fn(), effectAllowed: "", dropEffect: "" };
}

afterEach(cleanup);

describe("AccountRow", () => {
  it("shows the username and falls back to it when no alias is set", () => {
    renderRow();
    expect(screen.getByText("roboduck")).toBeInTheDocument();
  });

  it("prefers the alias and shows the @username underneath", () => {
    renderRow({}, makeAccount({ UserID: 501, Username: "roboduck", Alias: "Main" }));
    expect(screen.getByText("Main")).toBeInTheDocument();
    expect(screen.getByText("@roboduck")).toBeInTheDocument();
  });

  it("masks the name when hideUsernames is on", () => {
    renderRow({ hideUsernames: true, hiddenNameLetters: 3 });
    expect(screen.getByText("rob********")).toBeInTheDocument();
    expect(screen.queryByText("roboduck")).not.toBeInTheDocument();
  });

  it("delegates a click to handleSelect", async () => {
    const store = renderRow();
    await userEvent.click(row());
    expect(store.handleSelect).toHaveBeenCalledTimes(1);
    expect((store.handleSelect as ReturnType<typeof vi.fn>).mock.calls[0][0]).toBe(501);
  });

  it("selects the row and opens the context menu on right-click when unselected", () => {
    const store = renderRow();
    fireEvent.contextMenu(row(), { clientX: 120, clientY: 240 });
    expect(store.selectSingle).toHaveBeenCalledWith(501);
    expect(store.openContextMenu).toHaveBeenCalledWith(120, 240);
  });

  it("keeps an existing multi-selection when right-clicking a selected row", () => {
    const store = renderRow({ selectedIds: new Set([501, 502]) });
    fireEvent.contextMenu(row(), { clientX: 5, clientY: 6 });
    expect(store.selectSingle).not.toHaveBeenCalled();
    expect(store.openContextMenu).toHaveBeenCalledWith(5, 6);
  });

  it("starts a drag from the grip handle only", () => {
    const store = renderRow();
    const dataTransfer = fakeDataTransfer();
    fireEvent.dragStart(dragHandle(), { dataTransfer });

    expect(store.setDragState).toHaveBeenCalledWith({ userId: 501, sourceGroup: "Alts" });
    expect(dataTransfer.setData).toHaveBeenCalledWith("text/plain", "501");
  });

  it("clears dragState on dragEnd so a cancelled drag leaves no stale state", () => {
    const store = renderRow();
    fireEvent.dragStart(dragHandle(), { dataTransfer: fakeDataTransfer() });
    fireEvent.dragEnd(dragHandle());

    expect(store.setDragState).toHaveBeenLastCalledWith(null);
  });

  it("does not select the row when the grip handle is clicked", async () => {
    const store = renderRow();
    await userEvent.click(dragHandle());
    expect(store.handleSelect).not.toHaveBeenCalled();
  });

  it("reorders when a row from the same group is dropped on it", () => {
    const store = renderRow({ dragState: { userId: 777, sourceGroup: "Alts" } });
    fireEvent.drop(row(), { dataTransfer: fakeDataTransfer() });

    expect(store.setDragState).toHaveBeenCalledWith(null);
    expect(store.reorderAccounts).toHaveBeenCalledWith(777, 501);
    expect(store.moveToGroup).not.toHaveBeenCalled();
  });

  it("moves to this row's group when the drop comes from another group", () => {
    const store = renderRow({ dragState: { userId: 777, sourceGroup: "Default" } });
    fireEvent.drop(row(), { dataTransfer: fakeDataTransfer() });

    expect(store.moveToGroup).toHaveBeenCalledWith([777], "Alts");
    expect(store.reorderAccounts).not.toHaveBeenCalled();
  });

  it("ignores a drop when nothing is being dragged", () => {
    const store = renderRow({ dragState: null });
    fireEvent.drop(row(), { dataTransfer: fakeDataTransfer() });

    expect(store.reorderAccounts).not.toHaveBeenCalled();
    expect(store.moveToGroup).not.toHaveBeenCalled();
  });

  it("shows a spinner labelled Join while the account is joining", () => {
    renderRow({ joiningAccounts: new Set([501]) });
    expect(screen.getByText("Join")).toBeInTheDocument();
  });

  it("shows how long ago the account was last used", () => {
    const threeDaysAgo = new Date(Date.now() - 3 * 86400000).toISOString();
    renderRow({}, makeAccount({ UserID: 501, Username: "roboduck", LastUse: threeDaysAgo }));
    expect(screen.getByText("3d")).toBeInTheDocument();
  });

  it("marks the row as selected via the accent border", () => {
    renderRow({ selectedIds: new Set([501]) });
    expect(row().className).toContain("theme-row-selected");
  });
});
