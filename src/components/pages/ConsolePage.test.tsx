import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { ConsolePage } from "./ConsolePage";
import { makeAccount, renderWithStore, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";
import type { LaunchLogEntry } from "../../store";

const ACCOUNT = makeAccount({ UserID: 1001, Username: "alpha" });

const LOGS: LaunchLogEntry[] = [
  { id: 1, userId: 1001, level: "success", step: "rejoin", message: "Entrou no jogo pelo ciclo do Auto Rejoin", ts: Date.now() },
  { id: 2, userId: 1001, level: "warn", step: "watcher", message: "Cliente fechado pelo Watcher: sem conexao por 30s", ts: Date.now() },
  { id: 3, userId: null, level: "info", step: "rejoin", message: "Auto Rejoin parado", ts: Date.now() },
];

beforeEach(() => {
  resetTauriMocks();
});

afterEach(cleanup);

/**
 * O Console virou página da barra lateral (pedido do dono, 11/10/2026): o que
 * o app fez — launch, Auto Rejoin, Watcher, erros —, sem as listas da sessão,
 * que moram só na página Session.
 */
describe("ConsolePage", () => {
  it("renders nothing while another page is open", () => {
    setStore({});
    const { container } = render(<ConsolePage active={false} onLeave={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows every log line with where it came from", () => {
    renderWithStore(<ConsolePage active onLeave={vi.fn()} />, { accounts: [ACCOUNT], launchLogs: LOGS });

    expect(screen.getByRole("heading", { level: 1, name: "Console" })).toBeInTheDocument();
    expect(screen.getByText("Entrou no jogo pelo ciclo do Auto Rejoin")).toBeInTheDocument();
    expect(screen.getByText(/sem conexao por 30s/)).toBeInTheDocument();
    const origins = screen.getAllByTestId("log-step").map((el) => el.textContent);
    expect(origins).toEqual(["[rejoin]", "[watcher]", "[rejoin]"]);
    expect(screen.getByText("3 log lines")).toBeInTheDocument();
  });

  it("a session line does not pretend to belong to an account", () => {
    renderWithStore(<ConsolePage active onLeave={vi.fn()} />, { accounts: [ACCOUNT], launchLogs: LOGS });
    // `userId: null` desenha "—": passar 0 imprimiria "0" no lugar do nome.
    const line = screen.getByText("Auto Rejoin parado").closest("div") as HTMLElement;
    expect(line.textContent).toContain("—");
  });

  it("does not repeat the session lists (they live on the Session page)", () => {
    renderWithStore(<ConsolePage active onLeave={vi.fn()} />, {
      accounts: [ACCOUNT],
      launchLogs: LOGS,
      launchedByProgram: new Set([1001]),
    });
    expect(screen.queryByTestId("session-panel")).not.toBeInTheDocument();
    expect(screen.queryByText("In game")).not.toBeInTheDocument();
    expect(screen.queryByText("Joining")).not.toBeInTheDocument();
  });

  it("clears the log", async () => {
    const { store } = renderWithStore(<ConsolePage active onLeave={vi.fn()} />, { accounts: [ACCOUNT], launchLogs: LOGS });
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(store.clearLaunchLogs).toHaveBeenCalled();
  });

  it("explains the empty log", () => {
    renderWithStore(<ConsolePage active onLeave={vi.fn()} />, { accounts: [ACCOUNT], launchLogs: [] });
    expect(screen.getByText("No activity yet")).toBeInTheDocument();
    expect(screen.getByText("Launch a game or start Auto Rejoin to see the activity here")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear" })).toBeDisabled();
  });

  it("leaves the page on Escape", async () => {
    const onLeave = vi.fn();
    renderWithStore(<ConsolePage active onLeave={onLeave} />, { accounts: [ACCOUNT], launchLogs: [] });
    await userEvent.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalled();
  });
});
