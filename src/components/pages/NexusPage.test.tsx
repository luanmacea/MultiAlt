import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { NexusPage } from "./NexusPage";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

/** Estado do servidor devolvido pelo backend em cada teste. */
let nexusStatus = { running: false, port: null as number | null, connected_count: 0 };

function renderDialog() {
  const store = setStore({});
  const onLeave = vi.fn();
  render(<NexusPage active onLeave={onLeave} />);
  return { store, onLeave };
}

/** Abre a aba Help, onde mora o texto explicativo. */
async function openHelp() {
  await userEvent.click(screen.getByRole("button", { name: "Help" }));
}

beforeEach(() => {
  resetTauriMocks();
  nexusStatus = { running: false, port: null, connected_count: 0 };
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_nexus_status":
        return nexusStatus;
      case "get_nexus_accounts":
      case "get_nexus_elements":
      case "get_nexus_log":
        return [];
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("NexusPage — identidade", () => {
  it("renders nothing while another page is open", () => {
    setStore({});
    const { container } = render(<NexusPage active={false} onLeave={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("names itself Nexus in the header, matching the sidebar item", () => {
    renderDialog();
    expect(screen.getByRole("heading", { name: "Nexus" })).toBeInTheDocument();
  });

  it("keeps Account Control as a subtitle instead of the title", () => {
    renderDialog();
    const subtitle = screen.getByText("Account Control");
    expect(subtitle).toBeInTheDocument();
    expect(subtitle.tagName).not.toMatch(/^H[12]$/);
  });
});

describe("NexusPage — Help explica o pré-requisito", () => {
  it("states that a third-party script executor is required", async () => {
    renderDialog();
    await openHelp();

    expect(screen.getByRole("heading", { name: "Requirements" })).toBeInTheDocument();
    expect(screen.getByText(/third-party script executor/i)).toBeInTheDocument();
    // O executor precisa oferecer WebSocket: o Nexus.lua aborta sem isso.
    expect(screen.getByText(/WebSocket/i)).toBeInTheDocument();
  });

  it("says the app only listens and the client connects back to it", async () => {
    renderDialog();
    await openHelp();

    expect(screen.getByText(/does not inject/i)).toBeInTheDocument();
  });

  it("shows the address the clients connect to, using the live port", async () => {
    nexusStatus = { running: true, port: 5300, connected_count: 0 };
    renderDialog();
    await openHelp();

    expect(await screen.findByText("ws://localhost:5300/Nexus")).toBeInTheDocument();
  });

  it("ties the Nexus.lua step to the button that saves the file", async () => {
    renderDialog();
    await openHelp();

    const saveButton = screen.getByRole("button", { name: "Save Nexus.lua" });
    expect(saveButton).toBeInTheDocument();
    // O passo tem que citar o botão pelo nome, senão o usuário não liga um ao outro.
    expect(screen.getByText(/Save Nexus\.lua.*below/i)).toBeInTheDocument();
    // O botão grava o arquivo ao lado do app e copia o caminho — sem isso o nome engana.
    expect(screen.getByText(/copies its path to the clipboard/i)).toBeInTheDocument();
  });

  /**
   * O clique gravava o arquivo, copiava o caminho e nao dizia nada: sucesso era
   * silencio e erro era engolido por um `catch {}` vazio. Quem clicava nao sabia
   * se algo aconteceu — e, com a feature `nexus` desligada no build, o comando
   * falha e o clique parecia nao fazer nada.
   */
  it("says where the file went, and says when it failed", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "export_nexus_lua") return "C:/RAM/Nexus.lua";
      if (cmd === "get_nexus_status") return { running: false, port: null, connected_count: 0 };
      return [];
    });
    const { store } = renderDialog();
    await openHelp();

    await userEvent.click(screen.getByRole("button", { name: "Save Nexus.lua" }));
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("C:/RAM/Nexus.lua"))
    );

    setInvokeHandler((cmd) => {
      if (cmd === "export_nexus_lua") throw new Error("feature nexus desligada");
      if (cmd === "get_nexus_status") return { running: false, port: null, connected_count: 0 };
      return [];
    });
    await userEvent.click(screen.getByRole("button", { name: "Save Nexus.lua" }));
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringMatching(/Não foi possível|Could not|failed/i))
    );
  });

  /**
   * Se o comando devolver vazio (ou algo que nao e string), a frase "salvo em
   * {{path}}" fica pendurada sem caminho — foi o que apareceu ao dirigir a tela.
   */
  it("does not leave a dangling path when the command returns nothing", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "export_nexus_lua") return "";
      if (cmd === "get_nexus_status") return { running: false, port: null, connected_count: 0 };
      return [];
    });
    const { store } = renderDialog();
    await openHelp();

    await userEvent.click(screen.getByRole("button", { name: "Save Nexus.lua" }));
    await waitFor(() => expect(store.addToast).toHaveBeenCalled());
    const chamadas = vi.mocked(store.addToast).mock.calls;
    const mensagem = String(chamadas[chamadas.length - 1][0]);
    // "Nexus.lua saved in  (path copied)" era o que saia: "em" sem caminho nenhum.
    expect(mensagem).not.toMatch(/in\s*(\(|$)/);
    expect(mensagem).not.toMatch(/\s{2}/);
    expect(mensagem).toMatch(/Nexus\.lua/);
  });

  it("warns that only accounts already in the list may connect", async () => {
    renderDialog();
    await openHelp();

    expect(screen.getByText(/exact Roblox username/i)).toBeInTheDocument();
  });
});

describe("NexusPage — página", () => {
  it("keeps Start/Stop in the page header", () => {
    renderDialog();
    const header = screen.getByRole("heading", { level: 1, name: "Nexus" }).closest("header") as HTMLElement;
    expect(header).toContainElement(screen.getByRole("button", { name: "Start" }));
  });

  it("volta para a lista de contas com Escape", async () => {
    const { onLeave } = renderDialog();
    await userEvent.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledTimes(1);
  });
});
