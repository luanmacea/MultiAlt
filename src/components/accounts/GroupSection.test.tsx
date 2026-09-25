import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { GroupSection } from "./GroupSection";
import { AccountChip } from "./AccountChip";
import { groupAccounts, makeAccount, setStore } from "../../test-utils/renderWithStore";
import type { StoreValue } from "../../store";

const ACCOUNTS = [
  makeAccount({ UserID: 11, Username: "ann", Group: "Alts" }),
  makeAccount({ UserID: 12, Username: "bob", Group: "Alts" }),
];
const GROUP = groupAccounts(ACCOUNTS)[0];

function renderGroup(overrides: Partial<StoreValue> = {}, props: Partial<Parameters<typeof GroupSection>[0]> = {}) {
  const store = setStore({ accounts: ACCOUNTS, ...overrides });
  const onToggle = vi.fn();
  const onDrop = vi.fn();
  render(
    <GroupSection group={GROUP} collapsed={false} onToggle={onToggle} onDrop={onDrop} {...props} />
  );
  return { store, onToggle, onDrop };
}

function header(): HTMLElement {
  const el = document.querySelector("[data-group-header='true']");
  if (!el) throw new Error("group header not rendered");
  return el as HTMLElement;
}

afterEach(cleanup);

describe("GroupSection", () => {
  it("shows the group name, member count and its rows", () => {
    renderGroup();
    expect(screen.getByText("Alts")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("ann")).toBeInTheDocument();
    expect(screen.getByText("bob")).toBeInTheDocument();
  });

  it("toggles the group when the header is clicked", async () => {
    const { onToggle } = renderGroup();
    await userEvent.click(header());
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("reports the group key when something is dropped on the header", () => {
    const { onDrop } = renderGroup();
    fireEvent.drop(header(), { dataTransfer: { getData: () => "" } });
    expect(onDrop).toHaveBeenCalledWith("Alts");
  });

  it("hides the header when groups are turned off", () => {
    renderGroup({ showGroups: false });
    expect(document.querySelector("[data-group-header='true']")).toBeNull();
    // Rows are still rendered.
    expect(screen.getByText("ann")).toBeInTheDocument();
  });

  it("adds every member to the selection from the group checkbox", async () => {
    const { store } = renderGroup({ selectedIds: new Set([11, 99]) });
    const checkbox = header().firstElementChild?.firstElementChild as HTMLElement;
    await userEvent.click(checkbox);
    expect(store.setSelectedIds).toHaveBeenCalledWith(new Set([11, 99, 12]));
  });

  it("removes every member when the whole group is already selected", async () => {
    const { store } = renderGroup({ selectedIds: new Set([11, 12, 99]) });
    const checkbox = header().firstElementChild?.firstElementChild as HTMLElement;
    await userEvent.click(checkbox);
    expect(store.setSelectedIds).toHaveBeenCalledWith(new Set([99]));
  });

  /**
   * Cabeçalho e checkbox eram `<div onClick>`: colapsar o grupo e selecionar
   * o grupo inteiro não tinham equivalente por teclado.
   */
  describe("teclado", () => {
    it("exposes the header as a button and toggles it with the keyboard", async () => {
      const { onToggle } = renderGroup();
      const el = header();
      expect(el).toHaveAttribute("role", "button");
      expect(el.tabIndex).toBe(0);

      el.focus();
      await userEvent.keyboard("{Enter}");
      expect(onToggle).toHaveBeenCalledTimes(1);

      await userEvent.keyboard(" ");
      expect(onToggle).toHaveBeenCalledTimes(2);
    });

    it("exposes the group checkbox with role=checkbox and toggles it with Space", async () => {
      const { store } = renderGroup({ selectedIds: new Set([11, 99]) });
      const checkbox = header().firstElementChild?.firstElementChild as HTMLElement;
      expect(checkbox).toHaveAttribute("role", "checkbox");
      // 11 (do grupo) e 99 (fora) estão selecionados, 12 não: misto, não "false".
      expect(checkbox).toHaveAttribute("aria-checked", "mixed");

      checkbox.focus();
      await userEvent.keyboard(" ");
      expect(store.setSelectedIds).toHaveBeenCalledWith(new Set([11, 99, 12]));
    });

    it("reports aria-checked=true once the whole group is selected", () => {
      renderGroup({ selectedIds: new Set([11, 12]) });
      const checkbox = header().firstElementChild?.firstElementChild as HTMLElement;
      expect(checkbox).toHaveAttribute("aria-checked", "true");
    });

    it("reports aria-checked=mixed when only part of the group is selected", () => {
      renderGroup({ selectedIds: new Set([11]) });
      const checkbox = header().firstElementChild?.firstElementChild as HTMLElement;
      expect(checkbox).toHaveAttribute("aria-checked", "mixed");
    });
  });
});

describe("AccountChip", () => {
  it("shows the account name and removes it from the selection", async () => {
    setStore({ accounts: ACCOUNTS });
    const onRemove = vi.fn();
    render(<AccountChip account={ACCOUNTS[0]} onRemove={onRemove} />);

    expect(screen.getByText("ann")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button"));
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it("masks the name when usernames are hidden", () => {
    setStore({ accounts: ACCOUNTS, hideUsernames: true, hiddenNameLetters: 1 });
    render(<AccountChip account={ACCOUNTS[0]} onRemove={vi.fn()} />);
    expect(screen.getByText("a********")).toBeInTheDocument();
  });

  it("prefers the alias over the username", () => {
    const aliased = makeAccount({ UserID: 13, Username: "carl", Alias: "Carlito" });
    setStore({ accounts: [aliased] });
    render(<AccountChip account={aliased} onRemove={vi.fn()} />);
    expect(screen.getByText("Carlito")).toBeInTheDocument();
    expect(screen.queryByText("carl")).not.toBeInTheDocument();
  });
});
