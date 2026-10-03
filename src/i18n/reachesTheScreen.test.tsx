import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../store", async () => (await import("../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../hooks/usePrompt", async () => (await import("../test-utils/promptMocks")).promptModuleMock());

import i18n, { DEFAULT_LANGUAGE } from "./index";
import { AccountUtilsDialog } from "../components/dialogs/AccountUtilsDialog";
import { ThemePage } from "../components/pages/ThemePage";
import { AccountRow } from "../components/accounts/AccountRow";
import { makeAccount, setStore } from "../test-utils/renderWithStore";
import { resetTauriMocks } from "../test-utils/tauriMocks";
import { resetPromptMocks } from "../test-utils/promptMocks";

/**
 * Traduzir o catálogo não basta: se a string nunca chega ao `t()` — texto solto
 * no JSX, chave montada dentro da chamada, prop com ternário — a tela continua
 * em inglês com o catálogo inteiro traduzido. Foi o que aconteceu com os botões
 * de segurança do diálogo de utilitários, as abas do editor de temas e a coluna
 * de último uso. Este arquivo renderiza cada um desses pontos **em português**.
 */
const ACCOUNT = makeAccount({ UserID: 42, Username: "ann", Alias: "Main" });

beforeEach(async () => {
  resetTauriMocks();
  resetPromptMocks();
  await i18n.changeLanguage("pt");
});

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage(DEFAULT_LANGUAGE);
});

describe("a tradução chega à tela", () => {
  it("traduz os botões de sessão e a Zona de Perigo dos utilitários da conta", () => {
    setStore({ accounts: [ACCOUNT], selectedIds: new Set([ACCOUNT.UserID]), selectedAccounts: [ACCOUNT] });
    render(<AccountUtilsDialog open onClose={() => {}} />);

    expect(screen.getByText("Sair das outras sessões")).toBeInTheDocument();
    expect(screen.getByText("Trocar a senha")).toBeInTheDocument();
    expect(screen.getByText("Trocar o e-mail")).toBeInTheDocument();
    expect(screen.getByText("Ver os usuários bloqueados")).toBeInTheDocument();
    expect(screen.queryByText("Sign out of other sessions")).not.toBeInTheDocument();
  });

  it("traduz as seções da página de tema", () => {
    setStore({ accounts: [ACCOUNT] });
    render(<ThemePage active onLeave={() => {}} />);

    const abas = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    for (const aba of ["Contas", "Botões", "Formulários", "Campos de texto", "Rótulos", "Fontes"]) {
      expect(abas).toContain(aba);
    }
    expect(abas).not.toContain("Buttons");
  });

  it("traduz a coluna de último uso da linha da conta", () => {
    const agora = makeAccount({ UserID: 7, Username: "bob", LastUse: new Date().toISOString() });
    const nunca = makeAccount({ UserID: 8, Username: "carol", LastUse: "" });
    setStore({ accounts: [agora, nunca] });

    render(<AccountRow account={agora} />);
    expect(screen.getByText("agora")).toBeInTheDocument();

    cleanup();
    render(<AccountRow account={nunca} />);
    expect(screen.getByText("nunca")).toBeInTheDocument();
  });
});
