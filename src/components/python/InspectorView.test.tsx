import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { InspectorView, normalizeAssetName } from "./InspectorView";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const IMG_RECT = { left: 0, top: 0, width: 200, height: 100, right: 200, bottom: 100, x: 0, y: 0, toJSON: () => ({}) };
let writeText: ReturnType<typeof vi.fn>;
let toast: ReturnType<typeof vi.fn>;

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  writeText = vi.fn(async () => {});
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  vi.spyOn(HTMLImageElement.prototype, "getBoundingClientRect").mockReturnValue(IMG_RECT as DOMRect);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage: () => {},
    getImageData: () => ({ data: [255, 200, 0, 255] }),
  } as unknown as CanvasRenderingContext2D);
  const store = setStore({
    accounts: [makeAccount({ UserID: 1001, Username: "Main_01" }), makeAccount({ UserID: 1002, Username: "Alt_02" })],
    launchedByProgram: new Set([1001]),
  });
  toast = store.addToast as unknown as ReturnType<typeof vi.fn>;
  setInvokeHandler((cmd) => {
    if (cmd === "python_inspector_capture") return { width: 800, height: 400, pngBase64: "AAAA", frameId: 12 };
    if (cmd === "python_save_asset") return "C:/Data/python/assets/play_button.png";
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function loadedImage() {
  const img = (await screen.findByAltText("Capture of the Roblox window")) as HTMLImageElement;
  fireEvent.load(img);
  return { img, area: img.parentElement as HTMLElement };
}

describe("normalizeAssetName", () => {
  it("adds .png and refuses unsafe names", () => {
    expect(normalizeAssetName("play_button")).toBe("play_button.png");
    expect(normalizeAssetName("Play-1.PNG")).toBe("Play-1.png");
    expect(normalizeAssetName("../x")).toBeNull();
    expect(normalizeAssetName("a b")).toBeNull();
    expect(normalizeAssetName("")).toBeNull();
  });
});

describe("InspectorView", () => {
  it("asks to launch an account when no client is running", () => {
    setStore({ accounts: [makeAccount({ UserID: 1001 })] });
    render(<InspectorView />);
    expect(screen.getByText("No running clients")).toBeInTheDocument();
  });

  it("captures the first running client and offers only running clients", async () => {
    render(<InspectorView />);
    await loadedImage();
    expect(invokeMock).toHaveBeenCalledWith("python_inspector_capture", { userId: 1001 });
    expect(screen.getByText("800 x 400 px")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Account to inspect" })).toHaveTextContent("Main_01");
  });

  it("captures again on Refresh", async () => {
    render(<InspectorView />);
    await loadedImage();
    fireEvent.click(screen.getByRole("button", { name: /Refresh/ }));
    await waitFor(() => expect(invokeMock.mock.calls.filter((c) => c[0] === "python_inspector_capture")).toHaveLength(2));
  });

  it("shows a capture error", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "python_inspector_capture") throw "Could not capture Main_01's window (minimized).";
      return undefined;
    });
    render(<InspectorView />);
    expect(await screen.findByRole("alert")).toHaveTextContent(/minimized/);
  });

  it("shows the position as fractions with three decimals, and the color", async () => {
    render(<InspectorView />);
    const { area } = await loadedImage();
    fireEvent.mouseMove(area, { clientX: 104.6, clientY: 81.1 });
    expect(screen.getByTestId("hover-position")).toHaveTextContent("x 0.523, y 0.811");
    expect(screen.getByTestId("hover-color")).toHaveTextContent("RGB 255, 200, 0");
  });

  it("copies a click snippet on click", async () => {
    render(<InspectorView />);
    const { area } = await loadedImage();
    fireEvent.mouseDown(area, { clientX: 104.6, clientY: 81.1, button: 0 });
    fireEvent.mouseUp(area, { clientX: 104.6, clientY: 81.1 });
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("account.click(0.523, 0.811)"));
  });

  it("copies a wait_pixel snippet on Shift+click", async () => {
    render(<InspectorView />);
    const { area } = await loadedImage();
    fireEvent.mouseDown(area, { clientX: 104.6, clientY: 81.1, button: 0 });
    fireEvent.mouseUp(area, { clientX: 104.6, clientY: 81.1, shiftKey: true });
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("account.wait_pixel(0.523, 0.811, (255, 200, 0))"));
  });

  it("saves a dragged rectangle as an asset and gives the find_image snippet", async () => {
    promptAnswers.prompt = "play_button";
    render(<InspectorView />);
    const { area } = await loadedImage();
    fireEvent.mouseDown(area, { clientX: 40, clientY: 20, button: 0 });
    fireEvent.mouseMove(area, { clientX: 100, clientY: 60 });
    expect(screen.getByTestId("selection")).toBeInTheDocument();
    fireEvent.mouseUp(area, { clientX: 100, clientY: 60 });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("python_save_asset", {
        userId: 1001,
        rect: { x: 0.2, y: 0.2, w: 0.3, h: 0.4 },
        name: "play_button.png",
        // The crop comes from the frame on screen, not from a new capture.
        frameId: 12,
      })
    );
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('account.find_image("play_button.png")'));
    expect(toast).toHaveBeenCalledWith(expect.stringContaining('account.find_image("play_button.png")'), "success");
    expect(toast).toHaveBeenCalledWith(expect.stringContaining("play_button.png"), "success");
  });

  it("saves nothing when the name prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    render(<InspectorView />);
    const { area } = await loadedImage();
    fireEvent.mouseDown(area, { clientX: 40, clientY: 20, button: 0 });
    fireEvent.mouseMove(area, { clientX: 100, clientY: 60 });
    fireEvent.mouseUp(area, { clientX: 100, clientY: 60 });
    await waitFor(() => expect(screen.queryByTestId("selection")).not.toBeInTheDocument());
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_save_asset")).toBe(false);
  });

  it("does not save a tiny drag as an image", async () => {
    promptAnswers.prompt = "x";
    render(<InspectorView />);
    const { area } = await loadedImage();
    fireEvent.mouseDown(area, { clientX: 40, clientY: 20, button: 0 });
    fireEvent.mouseUp(area, { clientX: 45, clientY: 20 });
    await waitFor(() => expect(screen.queryByTestId("selection")).not.toBeInTheDocument());
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_save_asset")).toBe(false);
  });
});
