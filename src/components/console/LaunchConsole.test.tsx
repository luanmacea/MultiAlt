import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { LaunchConsole } from "./LaunchConsole";
import { launchLogText } from "../../utils/launchLog";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";
import i18n from "../../i18n";
import { tr } from "../../i18n/text";
import type { LaunchLogEntry } from "../../store";

const ACCOUNT = makeAccount({ UserID: 1001, Username: "alpha" });

/** Uma linha como o backend a manda: `message` em inglês + `key`/`params`. */
function line(id: number, key: string, params: Record<string, unknown>, message: string): LaunchLogEntry {
  return { id, userId: 1001, level: "info", step: "pid", message, key, params, ts: 1_700_000_000_000 };
}

const STARTED = line(1, "Client started (PID {{pid}})", { pid: 4242 }, "Client started (PID 4242)");
const WATCHER = line(
  2,
  "Client closed by the Watcher: no connection for {{seconds}}s",
  { seconds: 30 },
  "Client closed by the Watcher: no connection for 30s",
);

beforeEach(() => {
  resetTauriMocks();
});

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage("en");
});

/**
 * O Console traduz na hora de desenhar, a partir de `key` + `params`: era
 * `log.message` cru, e o backend mandava português a quem usa o app em inglês
 * (pedido do dono, 31/10/2026).
 */
describe("LaunchConsole — idioma das linhas", () => {
  it.each([
    ["en", "Client started (PID 4242)"],
    ["pt", "Cliente iniciado (PID 4242)"],
    ["es", "Cliente iniciado (PID 4242)"],
    ["de", "Client gestartet (PID 4242)"],
  ])("desenha a frase com os valores em %s", async (language, expected) => {
    await i18n.changeLanguage(language);
    renderWithStore(<LaunchConsole />, { accounts: [ACCOUNT], launchLogs: [STARTED] });
    expect(screen.getByText(expected)).toBeInTheDocument();
  });

  it("trocar o idioma traduz as linhas que já estão na tela", async () => {
    const { rerender } = renderWithStore(<LaunchConsole />, { accounts: [ACCOUNT], launchLogs: [STARTED, WATCHER] });
    expect(screen.getByText("Client closed by the Watcher: no connection for 30s")).toBeInTheDocument();
    await i18n.changeLanguage("pt");
    rerender(<LaunchConsole />);
    expect(screen.getByText("Cliente fechado pelo Watcher: sem conexão há 30s")).toBeInTheDocument();
  });

  it("linha sem key (texto de fora) mostra a mensagem como veio, em qualquer idioma", async () => {
    await i18n.changeLanguage("pt");
    const raw: LaunchLogEntry = { id: 9, userId: 1001, level: "warn", step: "version", message: "Roblox said: HTTP 429", ts: 1 };
    renderWithStore(<LaunchConsole />, { accounts: [ACCOUNT], launchLogs: [raw] });
    expect(screen.getByText("Roblox said: HTTP 429")).toBeInTheDocument();
  });
});

describe("launchLogText", () => {
  const t = tr;

  it("o texto de fora entra como parâmetro e não é traduzido", async () => {
    await i18n.changeLanguage("pt");
    const entry = line(
      3,
      "Authentication ticket failed: {{error}}",
      { error: "HTTP 401 Unauthorized" },
      "Authentication ticket failed: HTTP 401 Unauthorized",
    );
    expect(launchLogText(entry, t)).toBe("Falha ao obter o ticket de autenticação: HTTP 401 Unauthorized");
  });

  it("uma frase dentro de outra é traduzida antes de entrar", async () => {
    await i18n.changeLanguage("es");
    const entry = line(
      4,
      "Starting launch — place {{placeId}} ({{target}})",
      { placeId: 606849621, target: { key: "public server", params: {} } },
      "Starting launch — place 606849621 (public server)",
    );
    expect(launchLogText(entry, t)).toBe("Iniciando el lanzamiento — place 606849621 (servidor público)");
  });

  it("uma lista de frases vira 'a, b' no idioma de agora", async () => {
    await i18n.changeLanguage("pt");
    const entry = line(
      5,
      "PC kept awake while it runs: {{reasons}} (the screen may turn off)",
      { reasons: [{ key: "AFK mode", params: {} }, { key: "Auto Rejoin", params: {} }] },
      "PC kept awake while it runs: AFK mode, Auto Rejoin (the screen may turn off)",
    );
    expect(launchLogText(entry, t)).toBe("PC mantido acordado enquanto roda: Modo AFK, Auto Rejoin (a tela pode apagar)");
  });

  it("uma frase que ainda não tem tradução cai no inglês, nunca em branco", async () => {
    await i18n.changeLanguage("de");
    const entry = line(6, "A brand new sentence for {{name}}", { name: "alpha" }, "A brand new sentence for alpha");
    expect(launchLogText(entry, t)).toBe("A brand new sentence for alpha");
  });

  it("sem key devolve o message", () => {
    expect(launchLogText({ message: "plain" }, t)).toBe("plain");
  });
});
