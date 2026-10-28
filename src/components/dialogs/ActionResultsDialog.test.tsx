import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ActionResultsDialog } from "./ActionResultsDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { ActionReport } from "../../utils/actionReport";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "fine" }),
  makeAccount({ UserID: 2, Username: "dead" }),
  makeAccount({ UserID: 3, Username: "limited" }),
];

const REFRESH: ActionReport = {
  kind: "refresh-cookies",
  title: "Refresh cookies",
  running: false,
  at: 1,
  rows: [
    { userId: 1, outcome: "ok" },
    { userId: 2, outcome: "login", detail: "Roblox ended this session. Sign in again to use the account." },
    { userId: 3, outcome: "failed", detail: "Too many requests" },
  ],
};

function renderDialog(report: ActionReport, extra: Parameters<typeof setStore>[0] = {}) {
  const store = setStore({ accounts: ACCOUNTS, actionReport: report, ...extra });
  render(<ActionResultsDialog open onClose={vi.fn()} />);
  return store;
}

const rowIds = () =>
  screen.getAllByTestId(/^action-row-/).map((el) => el.getAttribute("data-testid")?.replace("action-row-", ""));

beforeEach(resetPromptMocks);
afterEach(cleanup);

/**
 * Pedido do dono (25/10/2026): toda ação do menu "Actions" mostra o resultado
 * conta a conta, como o "Check accounts" — o "Refresh cookies" com sessões
 * inválidas não dizia nada, nem oferecia logar de novo.
 */
describe("ActionResultsDialog", () => {
  it("lista cada conta com o resultado e o motivo, o que pede ação primeiro", () => {
    renderDialog(REFRESH);
    expect(screen.getByRole("heading", { name: "Refresh cookies" })).toBeInTheDocument();
    expect(rowIds()).toEqual(["2", "3", "1"]);
    expect(screen.getByTestId("action-row-2")).toHaveTextContent("Needs sign-in");
    expect(screen.getByTestId("action-row-2")).toHaveTextContent("Roblox ended this session");
    expect(screen.getByTestId("action-row-3")).toHaveTextContent("Too many requests");
    expect(screen.getByTestId("action-row-1")).toHaveTextContent("Refreshed");
  });

  it("os números do topo filtram a lista", async () => {
    renderDialog(REFRESH);
    await userEvent.click(screen.getByRole("button", { name: /Failed 1/ }));
    expect(rowIds()).toEqual(["3"]);
    await userEvent.click(screen.getByRole("button", { name: /All 3/ }));
    expect(rowIds()).toEqual(["2", "3", "1"]);
  });

  it("oferece logar de novo nas contas de sessão morta, depois de confirmar", async () => {
    const store = renderDialog(REFRESH);
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Sign in again (1)" }));
    expect(confirmMock.mock.calls[0][0]).toMatch(/You solve the CAPTCHA/);
    await waitFor(() => expect(store.reloginAccounts).toHaveBeenCalledWith([2]));
  });

  it("não loga de novo quando a pessoa cancela", async () => {
    const store = renderDialog(REFRESH);
    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: "Sign in again (1)" }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(store.reloginAccounts).not.toHaveBeenCalled();
  });

  it("seleciona as que falharam (inclusive as de sessão morta)", async () => {
    const store = renderDialog(REFRESH);
    await userEvent.click(screen.getByRole("button", { name: "Select the ones that failed" }));
    expect(store.setSelectedIds).toHaveBeenCalledWith(new Set([2, 3]));
  });

  it("enquanto roda mostra o progresso e esconde as ações", () => {
    renderDialog({
      ...REFRESH,
      running: true,
      rows: [
        { userId: 1, outcome: "ok" },
        { userId: 2, outcome: "running" },
        { userId: 3, outcome: "pending" },
      ],
    });
    expect(screen.getByText("Working 1/3...")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Sign in again/ })).toBeNull();
  });

  it("Make Friends em andamento mostra o estado ao vivo de cada conta", () => {
    renderDialog(
      { kind: "make-friends", title: "Make friends", running: true, at: 1, rows: [] },
      {
        friendLinkState: {
          active: true,
          phase: "linking",
          processed: 1,
          total: 3,
          mode: "mesh",
          mainUserId: null,
          accounts: [
            { userId: 1, state: "done", error: null },
            { userId: 2, state: "failed", error: "status 401" },
            { userId: 3, state: "processing", error: null },
          ],
        },
      }
    );
    expect(screen.getByTestId("action-row-1")).toHaveTextContent("Linked");
    expect(screen.getByTestId("action-row-2")).toHaveTextContent("Needs sign-in");
    expect(screen.getByTestId("action-row-3")).toHaveTextContent("Working");
  });

  it("ação sem linhas (fechar todos sem cliente aberto) mostra só o resumo", () => {
    renderDialog({
      kind: "close-all",
      title: "Close all Roblox clients",
      running: false,
      at: 1,
      rows: [],
      summary: "No open Roblox processes found",
    });
    expect(screen.getByTestId("action-results-summary")).toHaveTextContent("No open Roblox processes found");
  });
});
