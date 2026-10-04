import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ThemePage } from "./ThemePage";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { promptMock, resetPromptMocks } from "../../test-utils/promptMocks";
import { walkTour } from "../../test-utils/tourHelpers";
import { ScreenTourHost } from "../tour/ScreenTour";
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
  const onLeave = vi.fn();
  const view = render(<ThemePage active onLeave={onLeave} />);
  return { store, onLeave, view };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeMap({ get_theme_presets: [] });
});

afterEach(cleanup);

/** Na página as fontes já estão à vista; o passo fica para o teste dizer o que exercita. */
async function openFontsTab() {
  expect(screen.getByRole("heading", { name: "Fonts" })).toBeInTheDocument();
}

describe("ThemePage — importing a local font", () => {
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

describe("ThemePage — importing a preset file", () => {
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

/**
 * O modal mostrava uma categoria por vez (duas a quatro cores cada). A página
 * tem largura para todas, mais a prévia. E sair sem salvar continua devolvendo
 * o tema salvo, como o Escape/Cancel do modal faziam.
 */
describe("ThemePage — page behaviour", () => {
  function lastPreview(store: ReturnType<typeof setStore>) {
    const calls = (store.applyThemePreview as ReturnType<typeof vi.fn>).mock.calls;
    return calls[calls.length - 1][0];
  }

  it("shows every category at once, with a preview", () => {
    renderDialog();
    for (const category of ["Accounts", "Buttons", "Forms", "Text Boxes", "Labels", "Fonts"]) {
      expect(screen.getByRole("heading", { level: 2, name: category })).toBeInTheDocument();
    }
    expect(screen.getByRole("complementary", { name: "Preview" })).toBeInTheDocument();
  });

  it("renders nothing while another page is open", () => {
    setStore({ theme: null });
    render(<ThemePage active={false} onLeave={() => {}} />);
    expect(screen.queryByRole("heading", { name: "Theme" })).not.toBeInTheDocument();
  });

  it("puts the saved theme back when the page is left unsaved", async () => {
    const { store, view, onLeave } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Light Avatars" }));
    expect(lastPreview(store).light_images).toBe(!DEFAULT_THEME.light_images);

    view.rerender(<ThemePage active={false} onLeave={onLeave} />);
    expect(lastPreview(store).light_images).toBe(DEFAULT_THEME.light_images);
  });

  it("discards the changes without leaving the page", async () => {
    const { store, onLeave } = renderDialog();
    const discard = screen.getByRole("button", { name: "Discard changes" });
    expect(discard).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Light Avatars" }));
    expect(discard).toBeEnabled();
    await userEvent.click(discard);

    expect(lastPreview(store).light_images).toBe(DEFAULT_THEME.light_images);
    expect(discard).toBeDisabled();
    expect(onLeave).not.toHaveBeenCalled();
  });

  it("saves and stays on the page", async () => {
    const { store, onLeave } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Light Avatars" }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(store.saveTheme).toHaveBeenCalledTimes(1));
    expect((store.saveTheme as ReturnType<typeof vi.fn>).mock.calls[0][0].light_images).toBe(
      !DEFAULT_THEME.light_images
    );
    expect(onLeave).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole("button", { name: "Discard changes" })).toBeDisabled());
  });

  it("closes the preset list on Escape before leaving the page", async () => {
    const { onLeave } = renderDialog();
    const presetButton = screen.getByRole("button", { expanded: false });
    await userEvent.click(presetButton);
    expect(presetButton).toHaveAttribute("aria-expanded", "true");

    await userEvent.keyboard("{Escape}");
    expect(presetButton).toHaveAttribute("aria-expanded", "false");
    expect(onLeave).not.toHaveBeenCalled();

    await userEvent.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledTimes(1);
  });
});

describe("ThemePage — tutorial", () => {
  it("walks the Theme tutorial without saving or leaving the page", async () => {
    const { onLeave } = renderDialog();
    await walkTour("theme", { invoke: invokeMock });
    expect(onLeave).not.toHaveBeenCalled();
  });

  it("Escape closes the tutorial first, and only the next one leaves the page", async () => {
    const { onLeave } = renderDialog();
    render(<ScreenTourHost />);
    await userEvent.click(screen.getByRole("button", { name: /Tutorial/ }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(onLeave).not.toHaveBeenCalled();
    await userEvent.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledTimes(1);
  });
});
