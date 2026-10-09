import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { AccountRow } from "./AccountRow";
import { defaultSettings, makeAccount, setStore } from "../../test-utils/renderWithStore";
import type { StoreValue } from "../../store";

/** Settings com o alerta de envelhecimento ligado (o default do mock o desliga). */
function agingAlertOn(): Record<string, Record<string, string>> {
  const settings = defaultSettings();
  settings.General.DisableAgingAlert = "false";
  return settings;
}

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

/**
 * O dono pediu um caminho de ordenar que não dependa de arrastar. As setas
 * fazem o mesmo `reorderAccounts` do arrasto, uma posição por clique, e são a
 * única forma de reordenar por teclado.
 */
const TRIO = [
  makeAccount({ UserID: 1, Username: "um", Group: "Alts" }),
  makeAccount({ UserID: 2, Username: "dois", Group: "Alts" }),
  makeAccount({ UserID: 3, Username: "tres", Group: "Alts" }),
  makeAccount({ UserID: 9, Username: "outro", Group: "Mains" }),
];

function renderNoTrio(userId: number) {
  const conta = TRIO.find((a) => a.UserID === userId)!;
  const store = setStore({ accounts: TRIO });
  render(<AccountRow account={conta} />);
  return store;
}

afterEach(cleanup);

describe("AccountRow — setas de ordem", () => {
  it("sobe a conta uma posição, trocando com a de cima", async () => {
    const store = renderNoTrio(2);

    await userEvent.click(screen.getByRole("button", { name: /Move up/i }));

    expect(store.reorderAccounts).toHaveBeenCalledWith(2, 1);
  });

  it("desce a conta uma posição, trocando com a de baixo", async () => {
    const store = renderNoTrio(2);

    await userEvent.click(screen.getByRole("button", { name: /Move down/i }));

    expect(store.reorderAccounts).toHaveBeenCalledWith(2, 3);
  });

  /** Subir a primeira do grupo não pode empurrá-la para dentro de outro grupo. */
  it("não oferece subir na primeira nem descer na última do grupo", () => {
    renderNoTrio(1);
    expect(screen.getByRole("button", { name: /Move up/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Move down/i })).toBeEnabled();

    cleanup();
    renderNoTrio(3);
    expect(screen.getByRole("button", { name: /Move up/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /Move down/i })).toBeDisabled();
  });

  it("conta sozinha no grupo tem as duas setas desligadas", () => {
    renderNoTrio(9);
    expect(screen.getByRole("button", { name: /Move up/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Move down/i })).toBeDisabled();
  });

  it("clicar na seta não seleciona a conta nem abre o menu", async () => {
    const store = renderNoTrio(2);

    await userEvent.click(screen.getByRole("button", { name: /Move up/i }));

    expect(store.handleSelect).not.toHaveBeenCalled();
    expect(store.openContextMenu).not.toHaveBeenCalled();
  });
});

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

  /**
   * O backend cria a conta com Valid=true mesmo sem cookie (Quick Add por nome
   * de usuário), então quem distingue é a linha: sem token não há sessão, e o
   * usuário precisa ver isso com a mesma marca de sessão inválida.
   */
  it("flags an account added without a cookie as having no session", () => {
    renderRow({}, makeAccount({ UserID: 501, Username: "roboduck", SecurityToken: "" }));
    expect(screen.getByLabelText(/No session/)).toBeInTheDocument();
  });

  it("keeps the invalid-session mark for an account whose cookie died", () => {
    renderRow({}, makeAccount({ UserID: 501, Valid: false }));
    expect(screen.getByLabelText("Invalid session")).toBeInTheDocument();
  });

  it("shows no session mark for a healthy account", () => {
    renderRow();
    expect(screen.queryByLabelText(/session/i)).not.toBeInTheDocument();
  });

  /**
   * A bolinha de envelhecimento não dizia de *que* a conta envelheceu: só
   * "Aged account". Tem que dizer quantos dias se passaram desde a data de Last
   * Use (a mesma da coluna da direita) e que isso não é a bolinha vermelha de
   * sessão inválida.
   */
  it("says how many days old the aging dot is counting", () => {
    const lastUse = new Date(Date.now() - 25 * 86400000).toISOString();
    renderRow(
      { settings: agingAlertOn() },
      makeAccount({ UserID: 501, Username: "roboduck", LastUse: lastUse })
    );

    const dot = screen.getByLabelText(/Aged: 25 days/);
    expect(dot).toBeInTheDocument();
    expect(dot.getAttribute("aria-label")).toContain("Last Use");
    expect(dot.getAttribute("aria-label")).toMatch(/red dot/i);
  });

  it("marks each state with its own icon, not only a color", () => {
    const lastUse = new Date(Date.now() - 25 * 86400000).toISOString();
    renderRow(
      { settings: agingAlertOn() },
      makeAccount({ UserID: 501, Username: "roboduck", LastUse: lastUse, Valid: false })
    );
    expect(screen.getByLabelText("Invalid session")).toHaveAttribute("data-status", "invalid");
    expect(screen.getByLabelText(/Aged: 25 days/)).toHaveAttribute("data-status", "aged");
  });

  /** Ideia 8: a moderação lida do Roblox aparece na linha, com ícone próprio. */
  it("shows a banned account with its end date and the moderator note", () => {
    const until = new Date(Date.now() + 3 * 86400000);
    const dd = String(until.getDate()).padStart(2, "0");
    const mm = String(until.getMonth() + 1).padStart(2, "0");
    renderRow({
      moderationByUserId: new Map([
        [501, { state: "banned", until: until.toISOString(), note: "Spam", punishment: "Ban 3 Days" }],
      ]),
    });
    const dot = screen.getByLabelText(new RegExp(`Banned until ${dd}/${mm}`));
    expect(dot).toHaveAttribute("data-status", "banned");
    expect(dot.getAttribute("aria-label")).toContain("Spam");
  });

  it("shows warned and terminated accounts", () => {
    renderRow({
      moderationByUserId: new Map([[501, { state: "warned", until: null, note: null, punishment: "Warn" }]]),
    });
    expect(screen.getByLabelText(/Warned/)).toHaveAttribute("data-status", "warned");
    cleanup();
    renderRow({
      moderationByUserId: new Map([[501, { state: "terminated", until: null, note: null, punishment: "Delete" }]]),
    });
    expect(screen.getByLabelText(/Terminated/)).toHaveAttribute("data-status", "banned");
  });

  it("shows nothing for a clean account", () => {
    renderRow({
      moderationByUserId: new Map([[501, { state: "clean", until: null, note: null, punishment: null }]]),
    });
    expect(document.querySelector("[data-status='banned'], [data-status='warned']")).toBeNull();
  });

  it("keeps the aging dot quiet for a recently used account", () => {
    const lastUse = new Date(Date.now() - 3 * 86400000).toISOString();
    renderRow(
      { settings: agingAlertOn() },
      makeAccount({ UserID: 501, Username: "roboduck", LastUse: lastUse })
    );
    expect(screen.queryByLabelText(/Aged:/)).not.toBeInTheDocument();
  });

  it("marks the row as selected via the accent border", () => {
    renderRow({ selectedIds: new Set([501]) });
    expect(row().className).toContain("theme-row-selected");
  });

  /**
   * O alias agora aceita ate 240 caracteres, o que estoura a linha da conta.
   * Sem `WrapLongNames` o nome continua sendo cortado (comportamento de
   * sempre); ligado, o nome quebra em vez de truncar. As duas classes sao
   * mutuamente exclusivas para nao truncar E quebrar ao mesmo tempo.
   */
  describe("long names", () => {
    const longAlias = "a".repeat(240);
    const longAccount = makeAccount({ UserID: 501, Username: "roboduck", Alias: longAlias });

    it("truncates a long alias by default (WrapLongNames off)", () => {
      renderRow({}, longAccount);
      const nameEl = screen.getByText(longAlias);
      expect(nameEl.className).toContain("truncate");
      expect(nameEl.className).not.toContain("wrap-anywhere");
    });

    it("wraps a long alias instead of truncating when WrapLongNames is on", () => {
      const settings = defaultSettings();
      settings.General.WrapLongNames = "true";
      renderRow({ settings }, longAccount);
      const nameEl = screen.getByText(longAlias);
      expect(nameEl.className).toContain("wrap-anywhere");
      expect(nameEl.className).not.toContain("truncate");
    });

    it("wraps the @username line too when WrapLongNames is on", () => {
      const settings = defaultSettings();
      settings.General.WrapLongNames = "true";
      const longUsername = makeAccount({
        UserID: 501,
        Username: longAlias,
        Alias: "Main",
      });
      renderRow({ settings }, longUsername);
      const usernameEl = screen.getByText(`@${longAlias}`);
      expect(usernameEl.className).toContain("wrap-anywhere");
      expect(usernameEl.className).not.toContain("truncate");
    });

    it("still masks a long alias correctly when names are hidden", () => {
      renderRow({ hideUsernames: true, hiddenNameLetters: 3 }, longAccount);
      expect(screen.getByText("aaa********")).toBeInTheDocument();
      expect(screen.queryByText(longAlias)).not.toBeInTheDocument();
    });

    /**
     * O teste acima usa um alias **sem espaço** e passava com a tela quebrada:
     * `break-words` num item flex com `min-width: auto` não encolhe abaixo do
     * trecho sem espaço (as quebras de `break-word` não contam no tamanho
     * mínimo), então o nome ficava numa linha de 1765 px passando por baixo do
     * carimbo e das setas — medido no harness nos três tamanhos de janela. O que
     * resolve (validado num clone) é o nome poder encolher (`min-w-0`) e quebrar
     * em qualquer ponto também no tamanho mínimo (`overflow-wrap: anywhere`).
     * O jsdom não calcula layout; a medida de verdade está no relatório.
     */
    it("um alias sem espaço também quebra: o nome encolhe abaixo do trecho inteiro", () => {
      const settings = defaultSettings();
      settings.General.WrapLongNames = "true";
      const semEspaco = "A1B2C3D4E5".repeat(23);
      renderRow({ settings }, makeAccount({ UserID: 502, Username: "roboduck", Alias: semEspaco }));
      const classes = screen.getByText(semEspaco).className.split(/\s+/);
      expect(classes).toContain("min-w-0");
      expect(classes).toContain("wrap-anywhere");
    });
  });
});
