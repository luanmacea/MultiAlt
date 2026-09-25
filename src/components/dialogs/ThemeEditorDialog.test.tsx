import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ThemeEditorDialog } from "./ThemeEditorDialog";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { promptMock, resetPromptMocks } from "../../test-utils/promptMocks";
import { DEFAULT_THEME, DEFAULT_FONT_SANS } from "../../theme";

/**
 * Importar fonte/preset pedia um caminho de arquivo absoluto digitado de
 * cabeça (`window.prompt`), e um erro de digitação só voltava como
 * `Error: ...` do Rust. Estes testes travam o novo fluxo: um
 * `<input type="file">` manda os bytes escolhidos para os comandos
 * `import_theme_font_bytes` / `import_theme_preset_bytes`, sem prompt nenhum.
 */

function renderDialog() {
  const store = setStore({ theme: null });
  const onClose = vi.fn();
  render(<ThemeEditorDialog open onClose={onClose} />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeMap({ get_theme_presets: [] });
});

afterEach(cleanup);

async function openFontsTab() {
  await userEvent.click(screen.getByRole("button", { name: "Fonts" }));
}

describe("ThemeEditorDialog — importing a local font", () => {
  it("sends the chosen sans file's bytes instead of prompting for a path", async () => {
    setInvokeMap({
      get_theme_presets: [],
      import_theme_font_bytes: { file: "abc123.ttf", suggested_family: "My Font" },
    });
    const { store } = renderDialog();
    await openFontsTab();

    await userEvent.click(screen.getAllByRole("button", { name: "Import Local" })[0]);
    const file = new File([new Uint8Array([1, 2, 3, 4])], "My Font.ttf", { type: "font/ttf" });
    await userEvent.upload(screen.getByTestId("import-font-file-input"), file);

    expect(promptMock).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_theme_font_bytes", {
        fileName: "My Font.ttf",
        fileData: [1, 2, 3, 4],
      })
    );
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Imported font: My Font"));

    const previewCalls = (store.applyThemePreview as ReturnType<typeof vi.fn>).mock.calls;
    const lastPreview = previewCalls[previewCalls.length - 1][0];
    expect(lastPreview.font_sans.local.file).toBe("abc123.ttf");
  });

  it("targets the mono slot when the mono button was used", async () => {
    setInvokeMap({
      get_theme_presets: [],
      import_theme_font_bytes: { file: "def456.otf", suggested_family: "Mono Family" },
    });
    const { store } = renderDialog();
    await openFontsTab();

    await userEvent.click(screen.getAllByRole("button", { name: "Import Local" })[1]);
    const file = new File([new Uint8Array([9, 9])], "Mono Family.otf");
    await userEvent.upload(screen.getByTestId("import-font-file-input"), file);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_theme_font_bytes", {
        fileName: "Mono Family.otf",
        fileData: [9, 9],
      })
    );

    const previewCalls = (store.applyThemePreview as ReturnType<typeof vi.fn>).mock.calls;
    const lastPreview = previewCalls[previewCalls.length - 1][0];
    expect(lastPreview.font_mono.local.file).toBe("def456.otf");
    // The sans slot is untouched by a mono import.
    expect(lastPreview.font_sans).toEqual(DEFAULT_FONT_SANS);
  });

  it("shows the backend's rejection as a toast instead of crashing", async () => {
    setInvokeMap({
      get_theme_presets: [],
      import_theme_font_bytes: () => {
        // A wrong extension never reaches the backend at all: the picker's
        // `accept` filter keeps it from being selected in the first place.
        // What can still happen is a valid-looking file the backend rejects
        // on content (empty, oversized, ...).
        throw new Error("Font file is empty");
      },
    });
    const { store } = renderDialog();
    await openFontsTab();

    await userEvent.click(screen.getAllByRole("button", { name: "Import Local" })[0]);
    const file = new File([new Uint8Array([1])], "font.ttf");
    await userEvent.upload(screen.getByTestId("import-font-file-input"), file);

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("Font file is empty"))
    );
  });

  it("only accepts the documented font extensions", async () => {
    renderDialog();
    await openFontsTab();
    expect(screen.getByTestId("import-font-file-input")).toHaveAttribute(
      "accept",
      ".ttf,.otf,.woff,.woff2"
    );
  });
});

describe("ThemeEditorDialog — importing a preset file", () => {
  it("sends the chosen file's bytes instead of prompting for a path", async () => {
    setInvokeMap({
      get_theme_presets: [],
      import_theme_preset_bytes: {
        id: "uploaded-neon-1",
        name: "Uploaded Neon",
        theme: DEFAULT_THEME,
      },
    });
    const { store } = renderDialog();

    const file = new File([new Uint8Array([5, 6, 7])], "Uploaded Neon.ram-theme.json", {
      type: "application/json",
    });
    await userEvent.upload(screen.getByTestId("import-preset-file-input"), file);

    expect(promptMock).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_theme_preset_bytes", {
        fileName: "Uploaded Neon.ram-theme.json",
        fileData: [5, 6, 7],
      })
    );
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith("Imported preset: Uploaded Neon")
    );
  });

  it("shows the backend's rejection as a toast instead of crashing", async () => {
    setInvokeMap({
      get_theme_presets: [],
      import_theme_preset_bytes: () => {
        throw new Error("Preset file is empty");
      },
    });
    const { store } = renderDialog();

    const file = new File([], "empty.json", { type: "application/json" });
    await userEvent.upload(screen.getByTestId("import-preset-file-input"), file);

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("Preset file is empty"))
    );
  });

  it("only accepts the documented preset extensions", async () => {
    renderDialog();
    expect(screen.getByTestId("import-preset-file-input")).toHaveAttribute(
      "accept",
      ".json,.ram-theme.json,.zip,.ram-theme.zip"
    );
  });
});
