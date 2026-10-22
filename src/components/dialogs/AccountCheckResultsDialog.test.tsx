import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { AccountCheckResultsDialog } from "./AccountCheckResultsDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import type { AccountCheckSummary } from "../../utils/accountCheck";
import type { ModerationStatus } from "../../types";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "fine" }),
  makeAccount({ UserID: 2, Username: "bannedone" }),
  makeAccount({ UserID: 3, Username: "stale" }),
  makeAccount({ UserID: 4, Username: "warnedone" }),
  makeAccount({ UserID: 5, Username: "nonet" }),
  makeAccount({ UserID: 6, Username: "bannedtwo" }),
];

const RESULTS: AccountCheckSummary["results"] = [
  { userId: 1, outcome: "ok" },
  { userId: 2, outcome: "banned" },
  { userId: 3, outcome: "invalid" },
  { userId: 4, outcome: "warned" },
  { userId: 5, outcome: "unknown" },
  { userId: 6, outcome: "banned" },
];

// Fim do ban no futuro distante, em horário local, para a legenda ser "Banned until 12/10".
const MODERATION = new Map<number, ModerationStatus>([
  [2, { state: "banned", until: "2099-10-12T12:00:00", note: "Exploiting", punishment: "Ban 7 Days" }],
  [4, { state: "warned", until: null, note: null, punishment: "Warn" }],
  [6, { state: "terminated", until: null, note: null, punishment: "Delete" }],
]);

function renderDialog(extra: Parameters<typeof setStore>[0] = {}, onClose = vi.fn()) {
  const store = setStore({
    accounts: ACCOUNTS,
    moderationByUserId: MODERATION,
    accountCheckReport: { results: RESULTS, at: 1 },
    ...extra,
  });
  render(<AccountCheckResultsDialog open onClose={onClose} />);
  return { store, onClose };
}

const rowIds = () =>
  screen
    .getAllByTestId(/^check-row-/)
    .map((el) => el.getAttribute("data-testid")?.replace("check-row-", ""));

afterEach(cleanup);

describe("AccountCheckResultsDialog", () => {
  it("lists every account, banned first, each with a labelled outcome pill", () => {
    renderDialog();
    expect(screen.getByRole("heading", { name: "Account check results" })).toBeInTheDocument();
    expect(rowIds()).toEqual(["2", "6", "4", "3", "5", "1"]);

    const banned = screen.getByTestId("check-row-2");
    expect(within(banned).getByText("bannedone")).toBeInTheDocument();
    // Texto + ícone, não só cor.
    expect(within(banned).getByText("Banned")).toBeInTheDocument();
    expect(within(screen.getByTestId("check-row-3")).getByText("Invalid session")).toBeInTheDocument();
    expect(within(screen.getByTestId("check-row-5")).getByText("Couldn't check")).toBeInTheDocument();
    expect(within(screen.getByTestId("check-row-1")).getByText("OK")).toBeInTheDocument();
    expect(within(screen.getByTestId("check-row-4")).getByText("Warned")).toBeInTheDocument();
  });

  it("shows the ban date and the moderator note for a banned account, and the label for a terminated one", () => {
    renderDialog();
    const banned = screen.getByTestId("check-row-2");
    expect(within(banned).getByText("Banned until 12/10")).toBeInTheDocument();
    expect(within(banned).getByText("Moderator note: Exploiting")).toBeInTheDocument();
    expect(within(screen.getByTestId("check-row-6")).getByText("Terminated")).toBeInTheDocument();
    // Conta boa não ganha linha de detalhe.
    expect(within(screen.getByTestId("check-row-1")).queryByText(/Moderator note/)).toBeNull();
  });

  it("hides the real usernames when names are hidden", () => {
    renderDialog({ hideUsernames: true });
    expect(screen.queryByText("bannedone")).toBeNull();
    expect(screen.getByTestId("check-row-2")).toBeInTheDocument();
  });

  it("shows the counts as chips that filter the list, and All resets it", async () => {
    renderDialog();
    const bannedChip = screen.getByRole("button", { name: /Banned.*2/ });
    expect(screen.getByRole("button", { name: /Warned.*1/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Invalid session.*1/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Couldn't check.*1/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /OK.*1/ })).toBeInTheDocument();

    await userEvent.click(bannedChip);
    expect(rowIds()).toEqual(["2", "6"]);
    expect(bannedChip).toHaveAttribute("aria-pressed", "true");

    // Clicar de novo desfaz o filtro.
    await userEvent.click(bannedChip);
    expect(rowIds()).toHaveLength(6);

    await userEvent.click(screen.getByRole("button", { name: /Invalid session/ }));
    expect(rowIds()).toEqual(["3"]);
    await userEvent.click(screen.getByRole("button", { name: /^All/ }));
    expect(rowIds()).toHaveLength(6);
  });

  it("Select banned selects exactly the banned accounts and closes", async () => {
    const { store, onClose } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Select banned" }));
    expect(store.setSelectedIds).toHaveBeenCalledTimes(1);
    expect([...(store.setSelectedIds as ReturnType<typeof vi.fn>).mock.calls[0][0]].sort()).toEqual([2, 6]);
    expect(onClose).toHaveBeenCalled();
  });

  it("hides Select banned when nobody was banned", () => {
    renderDialog({ accountCheckReport: { results: [{ userId: 1, outcome: "ok" }], at: 1 } });
    expect(screen.queryByRole("button", { name: "Select banned" })).toBeNull();
  });

  it("Retry checks again only the accounts that couldn't be checked, and merges the answer", async () => {
    const checkAccounts = vi.fn(async (): Promise<AccountCheckSummary | null> => ({
      total: 2,
      ok: 1,
      warned: 0,
      invalid: 0,
      banned: 1,
      unknown: 0,
      results: [
        { userId: 5, outcome: "banned" },
        { userId: 7, outcome: "ok" },
      ],
    }));
    const { store } = renderDialog({
      checkAccounts,
      accountCheckReport: {
        results: [...RESULTS, { userId: 7, outcome: "unknown" }],
        at: 1,
      },
    });
    await userEvent.click(screen.getByRole("button", { name: "Retry 2 that couldn't be checked" }));
    expect(checkAccounts).toHaveBeenCalledWith([5, 7]);

    const setReport = store.setAccountCheckReport as ReturnType<typeof vi.fn>;
    expect(setReport).toHaveBeenCalledTimes(1);
    const merged = setReport.mock.calls[0][0] as { results: AccountCheckSummary["results"] };
    // As contas que já tinham resultado não se perdem.
    expect(merged.results).toEqual([
      { userId: 1, outcome: "ok" },
      { userId: 2, outcome: "banned" },
      { userId: 3, outcome: "invalid" },
      { userId: 4, outcome: "warned" },
      { userId: 5, outcome: "banned" },
      { userId: 6, outcome: "banned" },
      { userId: 7, outcome: "ok" },
    ]);
  });

  it("hides Retry when every account was checked", () => {
    renderDialog({ accountCheckReport: { results: RESULTS.filter((r) => r.outcome !== "unknown"), at: 1 } });
    expect(screen.queryByRole("button", { name: /Retry/ })).toBeNull();
  });

  it("disables Retry while a check is running", () => {
    renderDialog({ accountCheckProgress: { done: 1, total: 2 } });
    expect(screen.getByRole("button", { name: /Retry/ })).toBeDisabled();
  });

  it("closes with the Close button, the backdrop and Escape", async () => {
    const { onClose } = renderDialog();
    // Dois botões "Close": o X do título e o do rodapé.
    const closeButtons = screen.getAllByRole("button", { name: "Close" });
    expect(closeButtons).toHaveLength(2);
    await userEvent.click(closeButtons[1]);
    expect(onClose).toHaveBeenCalledTimes(1);
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(2);
    await userEvent.click(screen.getByTestId("account-check-results-backdrop"));
    expect(onClose).toHaveBeenCalledTimes(3);
  });
});
