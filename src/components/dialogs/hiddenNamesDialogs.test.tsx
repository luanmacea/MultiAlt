import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { AccountFieldsDialog } from "./AccountFieldsDialog";
import { MissingAssetsDialog } from "./MissingAssetsDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

/**
 * Diálogos pequenos que punham o nome da conta no título e ficavam de fora do
 * "Names hidden": o de Fields e o de itens que faltaram ao vestir um outfit.
 */
const SECRET = makeAccount({ UserID: 5, Username: "secretann", Alias: "AliasAnn" });
const HIDDEN = { hideUsernames: true, hiddenNameLetters: 0 };

beforeEach(() => {
  resetTauriMocks();
  setInvokeHandler(() => []);
});

afterEach(cleanup);

describe("diálogos com o nome da conta no título — nomes ocultos", () => {
  it("AccountFieldsDialog mascara o nome do título", () => {
    setStore({ ...HIDDEN, accounts: [SECRET], selectedAccounts: [SECRET], selectedIds: new Set([5]) });
    render(<AccountFieldsDialog open onClose={() => {}} />);
    expect(screen.getByText("Fields - ************")).toBeInTheDocument();
    expect(document.body.innerHTML).not.toMatch(/secretann|AliasAnn/);
  });

  it("MissingAssetsDialog mascara o nome do título", () => {
    setStore({
      ...HIDDEN,
      accounts: [SECRET],
      missingAssets: { userId: 5, username: "AliasAnn", assetIds: [1] },
    });
    render(<MissingAssetsDialog />);
    expect(screen.getByText("Missing Assets for ************")).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain("AliasAnn");
  });
});
