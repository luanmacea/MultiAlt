import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { UpdateDialog, notesForUpdateDialog } from "./UpdateDialog";
import i18n, { DEFAULT_LANGUAGE } from "../../i18n/index";
import { setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import { UPDATE_HANDOFF_KEY } from "../../updateHandoff";

const INFO = {
  version: "0.1.7",
  currentVersion: "0.1.6",
  date: "",
  body: "",
  releaseChannel: "beta" as const,
  featureChannel: "standard" as const,
};

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

describe("UpdateDialog", () => {
  beforeEach(async () => {
    await i18n.changeLanguage(DEFAULT_LANGUAGE);
    resetTauriMocks();
    localStorage.clear();
    // As notas vêm do GitHub; aqui não importam.
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, json: async () => ({}) })));
    setStore({ updateDialogOpen: true, updateInfo: INFO });
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("a barra do download acompanha o progresso que o backend manda", async () => {
    const download = deferred();
    setInvokeMap({ download_selected_update: () => download.promise });
    render(<UpdateDialog />);

    await userEvent.click(screen.getByRole("button", { name: "Download Update" }));
    act(() => emitTauriEvent("update-download-progress", { downloaded: 512 * 1024, total: 1024 * 1024 }));

    expect(await screen.findByText(/^50%/)).toBeInTheDocument();

    await act(async () => download.resolve());
    expect(await screen.findByRole("button", { name: "Install & Restart" })).toBeInTheDocument();
  });

  it("instalar mostra a tela de instalação, anota a versão e só então instala", async () => {
    setInvokeMap({ download_selected_update: undefined, install_selected_update: undefined });
    render(<UpdateDialog />);

    await userEvent.click(screen.getByRole("button", { name: "Download Update" }));
    await userEvent.click(await screen.findByRole("button", { name: "Install & Restart" }));

    expect(screen.getByText("Installing v0.1.7")).toBeInTheDocument();
    expect(screen.getByText("MultiAlt will close and open again by itself in a few seconds.")).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem(UPDATE_HANDOFF_KEY) ?? "{}")).toMatchObject({
      from: "0.1.6",
      to: "0.1.7",
    });
    // Dá tempo de ler a tela antes de o app fechar.
    const installCalls = () => invokeMock.mock.calls.filter((c) => c[0] === "install_selected_update").length;
    expect(installCalls()).toBe(0);

    await waitFor(() => expect(installCalls()).toBe(1), { timeout: 3000 });
  });

  it("uma falha ao instalar apaga a anotação e mostra o erro", async () => {
    setInvokeMap({
      download_selected_update: undefined,
      install_selected_update: () => Promise.reject("Failed to install update: boom"),
    });
    render(<UpdateDialog />);

    await userEvent.click(screen.getByRole("button", { name: "Download Update" }));
    await userEvent.click(await screen.findByRole("button", { name: "Install & Restart" }));

    expect(await screen.findByText("Failed to install update: boom", {}, { timeout: 3000 })).toBeInTheDocument();
    expect(localStorage.getItem(UPDATE_HANDOFF_KEY)).toBeNull();
  });
});

/**
 * A página da release abre com "## Download" (o botão do instalador, para quem
 * chega pelo GitHub). Dentro do app isso não serve — a atualização já baixa
 * sozinha —, então a janela de atualização tira essa seção e mostra a partir
 * do que mudou.
 */
describe("notas da atualização dentro do app", () => {
  it("tiram a seção de download da página da release", () => {
    const body = [
      "## Download",
      "",
      "### [⬇ Download MultiAlt-Setup.msi](https://example.test/MultiAlt-Setup.msi)",
      "",
      "Open the file and follow the installer. That's it.",
      "",
      "<details>",
      "<summary>Other files (you can ignore these)</summary>",
      "",
      "- `.sig` files: used by the automatic update.",
      "",
      "</details>",
      "",
      "## What's Changed",
      "* New sidebar",
      "",
      "## Contributors",
      "[@luanmacea](https://github.com/luanmacea)",
    ].join("\n");
    const notes = notesForUpdateDialog(body);
    expect(notes).not.toMatch(/Download MultiAlt-Setup|Other files|## Download/);
    expect(notes.startsWith("## What's Changed")).toBe(true);
    expect(notes).toMatch(/## Contributors/);
  });

  it("deixam como estão as notas sem seção de download (releases antigas)", () => {
    const body = "> [!WARNING]\n> Beta.\n\n## What's Changed\n* Fix";
    expect(notesForUpdateDialog(body)).toBe(body);
  });
});
