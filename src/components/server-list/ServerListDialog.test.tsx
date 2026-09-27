import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ServerListDialog } from "./ServerListDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";

const ACCOUNT = makeAccount({ UserID: 1001, Username: "alpha" });

function renderDialog() {
  const store = setStore({
    accounts: [ACCOUNT],
    selectedIds: new Set([1001]),
    selectedAccounts: [ACCOUNT],
    selectedAccount: ACCOUNT,
  });
  render(<ServerListDialog open onClose={vi.fn()} />);
  return store;
}

/** O quadro do diálogo: o painel que contém o título. */
function quadro(): HTMLElement {
  const painel = screen.getByRole("heading", { name: "Server List" }).closest(".rounded-2xl");
  if (!painel) throw new Error("painel do Server List não encontrado");
  return painel as HTMLElement;
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  localStorage.clear();
  setInvokeHandler(() => null);
});

afterEach(cleanup);

describe("ServerListDialog — cabe na janela", () => {
  /**
   * O diálogo tinha tamanho fixo (`w-[680px] h-[560px]`) e nada que o limitasse
   * à janela. Na janela mínima do app (750x450, `tauri.conf.json`) sobravam
   * 110 px divididos acima e abaixo: título e X ficavam **fora da tela** (o X em
   * y = -38) e, na base, os campos Teleport e Find player também — sem rolagem
   * que os alcançasse (`body` é `overflow: hidden`). Medido no harness.
   *
   * O jsdom não calcula layout: isto trava o teto que o conserto depende, o
   * mesmo dos outros diálogos grandes (Auto Rejoin, AFK, Generator). A prova de
   * que cabe está nas medidas do relatório da Frente C.
   */
  it("o quadro nunca passa do tamanho da janela", () => {
    renderDialog();
    const classes = quadro().className.split(/\s+/);
    expect(classes).toContain("max-h-[calc(100vh-24px)]");
    expect(classes).toContain("max-w-[calc(100vw-24px)]");
  });

  it("o miolo encolhe com o quadro em vez de empurrar a base para fora", () => {
    renderDialog();
    // Entre o quadro e a aba: o miolo é `flex-1 min-h-0`, então a lista de
    // servidores (que rola por dentro) é quem cede altura, não os campos.
    const miolo = quadro().lastElementChild as HTMLElement;
    const classes = miolo.className.split(/\s+/);
    expect(classes).toContain("flex-1");
    expect(classes).toContain("min-h-0");
  });
});
