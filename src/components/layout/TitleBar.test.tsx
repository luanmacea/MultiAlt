import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/window", async () => (await import("../../test-utils/tauriMocks")).tauriWindowMock());

import { TitleBar } from "./TitleBar";
import { ModalWindowControls } from "./ModalWindowControls";
import { resetTauriMocks, setInvokeHandler, windowMock } from "../../test-utils/tauriMocks";

/** TitleBar's icon buttons carry no label; they render in a fixed order. */
const TITLEBAR_BUTTONS = ["github", "minimize", "maximize", "close"] as const;

function titleBarButton(which: (typeof TITLEBAR_BUTTONS)[number]): HTMLElement {
  return screen.getAllByRole("button")[TITLEBAR_BUTTONS.indexOf(which)];
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

describe("TitleBar", () => {
  it("shows the app name", () => {
    render(<TitleBar />);
    expect(screen.getByText("Roblox Account Manager")).toBeInTheDocument();
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
