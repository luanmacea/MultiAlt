import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { NexusDialog } from "./NexusDialog";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

/** Estado do servidor devolvido pelo backend em cada teste. */
let nexusStatus = { running: false, port: null as number | null, connected_count: 0 };

function renderDialog() {
  const store = setStore({});
  const onClose = vi.fn();
  render(<NexusDialog open onClose={onClose} />);
  return { store, onClose };
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

describe("NexusDialog — identidade", () => {
  it("renders nothing while closed", () => {
    setStore({});
    const { container } = render(<NexusDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("names itself Nexus in the header, matching the toolbar icon", () => {
    renderDialog();
    expect(screen.getByRole("heading", { name: "Nexus" })).toBeInTheDocument();
  });

  it("keeps Account Control as a subtitle instead of the title", () => {
    renderDialog();
    const subtitle = screen.getByText("Account Control");
    expect(subtitle).toBeInTheDocument();
    expect(subtitle.tagName).not.toBe("H2");
  });
});

describe("NexusDialog — Help explica o pré-requisito", () => {
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

  it("warns that only accounts already in the list may connect", async () => {
    renderDialog();
    await openHelp();

    expect(screen.getByText(/exact Roblox username/i)).toBeInTheDocument();
  });
});
