import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());
// Edição padrão: o build sai com `VITE_ENABLE_AVATAR_BATCH=false`.
vi.mock("../../featureFlags", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../featureFlags")>()),
  ENABLE_AVATAR_BATCH: false,
}));

import { AvatarsPage } from "./AvatarsPage";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";
import { walkTour } from "../../test-utils/tourHelpers";

const SAVED = {
  id: "av_1",
  name: "Ninja",
  items: [{ id: 101, kind: "Asset" as const, typeId: 11, name: "Shirt", collectibleItemId: "c-101" }],
  skinColor: 1030,
};

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeMap({
    avatar_free_catalog: [],
    avatar_list_saved: [SAVED],
    batch_thumbnails: [],
  });
});

afterEach(cleanup);

function renderPage() {
  const store = setStore({ accounts: [makeAccount({ UserID: 11, Username: "alpha" })] });
  render(<AvatarsPage active onLeave={() => {}} />);
  return store;
}

describe("AvatarsPage — edição padrão (sem distribuição em lote)", () => {
  it("a aba Distribute mostra o cartão da edição completa no lugar do lote", async () => {
    renderPage();
    await userEvent.click(screen.getByRole("tab", { name: /Distribute/ }));

    expect(screen.getByRole("heading", { name: "Distributing avatars is an extra" })).toBeInTheDocument();
    expect(screen.getByText(/picks up the free catalog items for each account automatically/)).toBeInTheDocument();
    expect(screen.getByText(/Your accounts and settings are kept\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Get the complete edition" })).toBeInTheDocument();
    // Nada do lote: nem a lista de avatares para sortear, nem o botão de aplicar.
    expect(screen.queryByRole("button", { name: "Apply avatars" })).not.toBeInTheDocument();
    expect(screen.queryByText("Avatars to hand out")).not.toBeInTheDocument();
  });

  it("não consulta o estado do lote, que esta edição não tem", async () => {
    renderPage();
    await screen.findByRole("tab", { name: /Distribute/ });
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "get_avatar_batch_state")).toBe(false);
  });

  it("o botão troca para a edição completa pelo updater do app", async () => {
    const store = renderPage();
    await userEvent.click(screen.getByRole("tab", { name: /Distribute/ }));
    await userEvent.click(screen.getByRole("button", { name: "Get the complete edition" }));

    expect(store.switchToCompleteEdition).toHaveBeenCalledTimes(1);
    // Nada de abrir o navegador: o download é do próprio app.
    expect(invokeMock.mock.calls.some(([cmd]) => /open|browser|url/i.test(String(cmd)))).toBe(false);
  });

  it("o tutorial da página ainda chega até a aba Distribute", async () => {
    renderPage();
    await walkTour("avatars", { invoke: invokeMock });
    expect(screen.getByRole("tab", { name: /Distribute/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { name: "Distributing avatars is an extra" })).toBeInTheDocument();
  });
});
