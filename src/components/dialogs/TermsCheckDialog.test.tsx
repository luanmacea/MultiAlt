import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { TermsCheckDialog } from "./TermsCheckDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { TermsState } from "../../types";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "clean" }),
  makeAccount({ UserID: 2, Username: "pendingone" }),
  makeAccount({ UserID: 3, Username: "failed" }),
  makeAccount({ UserID: 4, Username: "pendingtwo" }),
  makeAccount({ UserID: 5, Username: "noid" }),
  makeAccount({ UserID: 6, Username: "slow" }),
];

const TERMS = new Map<number, TermsState>([
  [1, { kind: "none" }],
  [2, { kind: "pending", pending: [{ id: "a-1", title: "Terms of Use" }] }],
  [3, { kind: "error", message: "Roblox is limiting requests right now. Try again in a minute." }],
  [4, { kind: "pending", pending: [{ id: "a-2", title: "Privacy Policy" }] }],
  [5, { kind: "pending", pending: [{ id: "", title: "Agreement" }] }],
  // 6 ainda não respondeu
]);

function renderDialog(extra: Parameters<typeof setStore>[0] = {}, onClose = vi.fn()) {
  const store = setStore({
    accounts: ACCOUNTS,
    termsByUserId: TERMS,
    termsDialogIds: [1, 2, 3, 4, 5, 6],
    ...extra,
  });
  render(<TermsCheckDialog open onClose={onClose} />);
  return { store, onClose };
}

const rowIds = () =>
  screen.getAllByTestId(/^terms-row-/).map((el) => el.getAttribute("data-testid")?.replace("terms-row-", ""));

beforeEach(() => resetPromptMocks());
afterEach(cleanup);

describe("TermsCheckDialog", () => {
  it("puts the pending accounts first, then failures, waiting and clean ones", () => {
    renderDialog();
    expect(rowIds()).toEqual(["2", "4", "5", "3", "6", "1"]);
  });

  it("tells a failed check apart from a clean account", () => {
    renderDialog();
    const failed = within(screen.getByTestId("terms-row-3"));
    expect(failed.getByText("Couldn't check")).toBeInTheDocument();
    expect(failed.getByText(/Try again in a minute/)).toBeInTheDocument();
    expect(within(screen.getByTestId("terms-row-1")).getByText("No pending terms")).toBeInTheDocument();
    expect(within(screen.getByTestId("terms-row-6")).getByText("Checking...")).toBeInTheDocument();
  });

  it("accepts one account after the confirmation, and not without it", async () => {
    const { store } = renderDialog();
    promptAnswers.confirm = false;
    await userEvent.click(within(screen.getByTestId("terms-row-2")).getByRole("button", { name: "Accept terms" }));
    expect(store.acceptTerms).not.toHaveBeenCalled();

    promptAnswers.confirm = true;
    await userEvent.click(within(screen.getByTestId("terms-row-2")).getByRole("button", { name: "Accept terms" }));
    await waitFor(() => expect(store.acceptTerms).toHaveBeenCalledWith(2));
    expect(store.acceptTerms).toHaveBeenCalledTimes(1);
  });

  it("accepts all the pending accounts that have an id, one at a time", async () => {
    const { store } = renderDialog({ termsChecking: false });
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Accept all (2)" }));
    await waitFor(() => expect(store.acceptTerms).toHaveBeenCalledTimes(2));
    expect(vi.mocked(store.acceptTerms).mock.calls.map((c) => c[0])).toEqual([2, 4]);
  });

  it("never offers the API accept for an agreement without an id, only the browser", async () => {
    const { store } = renderDialog();
    const noId = within(screen.getByTestId("terms-row-5"));
    expect(noId.queryByRole("button", { name: "Accept terms" })).not.toBeInTheDocument();
    await userEvent.click(noId.getByRole("button", { name: "Open in browser" }));
    expect(store.openAccountBrowser).toHaveBeenCalledWith(5);
  });

  it("does not offer Accept all while the check is still running", () => {
    renderDialog({ termsChecking: true });
    expect(screen.getByRole("button", { name: "Accept all (2)" })).toBeDisabled();
    expect(screen.getByText("Checking 5/6...")).toBeInTheDocument();
  });
});
