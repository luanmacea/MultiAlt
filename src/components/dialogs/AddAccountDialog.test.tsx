import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { AddAccountDialog } from "./AddAccountDialog";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const COOKIE =
  "_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items.|TOKEN";

function renderDialog(open = true) {
  const store = setStore({});
  const onClose = vi.fn();
  render(<AddAccountDialog open={open} onClose={onClose} />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});

afterEach(cleanup);

describe("AddAccountDialog", () => {
  it("renders nothing while closed", () => {
    const { container } = render(<AddAccountDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("lists the four ways to add an account", () => {
    renderDialog();
    expect(screen.getByText("Add Account")).toBeInTheDocument();
    for (const label of ["Quick Add", "Browser Login", "Import Cookie", "Import Old Account Data"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("closes from the X button", async () => {
    const { onClose } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps the dialog open when the panel itself is clicked", async () => {
    const { onClose } = renderDialog();
    await userEvent.click(screen.getByText("Choose how to add an account"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("adds a pasted cookie without touching the user lookup", async () => {
    promptAnswers.prompt = COOKIE;
    const { store, onClose } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(store.addAccountByCookie).toHaveBeenCalledWith(COOKIE));
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("looks a plain username up and registers it without a cookie", async () => {
    promptAnswers.prompt = "  roboduck ";
    setInvokeMap({ lookup_user: { id: 77, name: "roboduck" } });
    const { store } = renderDialog();
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
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Added roboduck"));
    expect(store.loadAccounts).toHaveBeenCalled();
  });

  it("ignores a blank or cancelled Quick Add", async () => {
    promptAnswers.prompt = "   ";
    const { store } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));
    await Promise.resolve();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(store.addAccountByCookie).not.toHaveBeenCalled();
  });

  it("reports a failed lookup as a toast", async () => {
    promptAnswers.prompt = "ghost";
    setInvokeMap({
      lookup_user: () => {
        throw new Error("user not found");
      },
    });
    const { store } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Quick Add" }));
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("user not found"))
    );
  });

  it("opens the login browser", async () => {
    const { store, onClose } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Browser Login" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(store.openLoginBrowser).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["Import Cookie", "cookie"],
    ["Import Old Account Data", "legacy"],
  ])("routes %s to the import dialog", async (label, tab) => {
    const { store, onClose } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: label }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(store.setImportDialogTab).toHaveBeenCalledWith(tab);
    expect(store.setImportDialogOpen).toHaveBeenCalledWith(true);
  });
});
