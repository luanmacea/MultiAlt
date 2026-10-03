import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/window", async () => (await import("../../test-utils/tauriMocks")).tauriWindowMock());

import { TitleBar } from "./TitleBar";
import { ModalWindowControls } from "./ModalWindowControls";
import { resetTauriMocks, setInvokeHandler, windowMock } from "../../test-utils/tauriMocks";

/** Ordem fixa de render; os nomes acessíveis são checados em teste próprio. */
const TITLEBAR_BUTTONS = ["github", "minimize", "maximize", "close"] as const;

function titleBarButton(which: (typeof TITLEBAR_BUTTONS)[number]): HTMLElement {
  return screen.getAllByRole("button")[TITLEBAR_BUTTONS.indexOf(which)];
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

describe("TitleBar", () => {
  it("shows the app name", () => {
    render(<TitleBar />);
    expect(screen.getByText("MultiAlt")).toBeInTheDocument();
  });

  it("keeps the old name as a subtitle so people still recognize the app", () => {
    // Até a v0.1.8 o app se chamava Roblox Account Manager (docs/rebrand-multialt.md).
    render(<TitleBar />);
    expect(screen.getByText("Roblox Account Manager")).toBeInTheDocument();
  });

  /**
   * Os quatro botões da barra de título são só ícone: sem nome acessível eles
   * não existem para leitor de tela. O do GitHub é, na prática, a porta da
   * documentação (docs/ mora no repositório), então o nome precisa dizer isso.
   */
  it("names every icon-only button", async () => {
    setInvokeHandler(() => "false");
    render(<TitleBar />);

    expect(screen.getByRole("button", { name: /docs/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Minimize" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Maximize" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument());
  });

  it("minimizes and maximizes the window", async () => {
    render(<TitleBar />);
    await userEvent.click(titleBarButton("minimize"));
    expect(windowMock.minimize).toHaveBeenCalledTimes(1);

    await userEvent.click(titleBarButton("maximize"));
    expect(windowMock.toggleMaximize).toHaveBeenCalledTimes(1);
  });

  it("closes the window when MinimizeToTray is off", async () => {
    setInvokeHandler(() => "false");
    render(<TitleBar />);
    await waitFor(() => expect(windowMock.isMaximized).toHaveBeenCalled());

    await userEvent.click(titleBarButton("close"));
    expect(windowMock.close).toHaveBeenCalledTimes(1);
    expect(windowMock.hide).not.toHaveBeenCalled();
  });

  it("hides to the tray instead of closing when MinimizeToTray is on", async () => {
    setInvokeHandler((cmd, args) =>
      cmd === "get_setting" && (args as { key: string }).key === "MinimizeToTray" ? "true" : ""
    );
    render(<TitleBar />);

    await waitFor(async () => {
      await userEvent.click(titleBarButton("close"));
      expect(windowMock.hide).toHaveBeenCalled();
    });
    expect(windowMock.close).not.toHaveBeenCalled();
  });

  it("opens the repository through the backend", async () => {
    const opened: string[] = [];
    setInvokeHandler((cmd) => {
      opened.push(cmd);
      return undefined;
    });
    render(<TitleBar />);
    await userEvent.click(titleBarButton("github"));
    await waitFor(() => expect(opened).toContain("open_repo_url"));
  });

  it("keeps the controls mounted but inert when they are hidden", () => {
    render(<TitleBar controlsHidden />);
    const controls = titleBarButton("minimize").parentElement?.parentElement;
    expect(controls?.className).toContain("pointer-events-none");
  });
});

describe("ModalWindowControls", () => {
  it("exposes labelled window controls while a modal is open", async () => {
    setInvokeHandler(() => "false");
    render(<ModalWindowControls visible />);

    await userEvent.click(screen.getByTitle("Minimize"));
    expect(windowMock.minimize).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByTitle("Maximize"));
    expect(windowMock.toggleMaximize).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getByTitle("Close"));
    expect(windowMock.close).toHaveBeenCalledTimes(1);
  });

  it("does not query the window while hidden", () => {
    render(<ModalWindowControls visible={false} />);
    expect(windowMock.isMaximized).not.toHaveBeenCalled();
  });

  it("switches the close control to tray mode", async () => {
    setInvokeHandler(() => "true");
    render(<ModalWindowControls visible />);
    expect(await screen.findByTitle("Minimize to tray")).toBeInTheDocument();
  });
});
