import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { SessionPage } from "./SessionPage";
import {
  makeAccount,
  makeBottingStatus,
  makePlatformCapabilities,
  renderWithStore,
  setStore,
} from "../../test-utils/renderWithStore";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import type { ClientDrop, LaunchQueuePayload } from "../../types";
import type { StoreValue } from "../../store";
import { walkTour } from "../../test-utils/tourHelpers";

const ACCOUNTS = [
  makeAccount({ UserID: 10, Username: "alpha" }),
  makeAccount({ UserID: 20, Username: "bravo" }),
];

const QUEUE: LaunchQueuePayload = {
  entries: [
    { userId: 10, state: "launching", error: null, updatedAtMs: 1 },
    { userId: 20, state: "queued", error: null, updatedAtMs: 1 },
  ],
  active: true,
  placeId: 123,
  jobId: "",
};

/** Ações da store ligadas ao `invoke` mockado (ver SessionPanel.test.tsx). */
function storeActions(): Partial<StoreValue> {
  return {
    cancelAccountLaunch: vi.fn(
      async (userId: number) => (await invokeMock("cancel_account_launch", { userId })) as boolean
    ),
    stopLaunchQueue: vi.fn(async () => (await invokeMock("stop_launch_queue")) as number),
    focusRobloxClient: vi.fn(
      async (userId: number) => (await invokeMock("focus_roblox_window", { userId })) as boolean
    ),
    closeRobloxClients: vi.fn(async (userIds: number[]) => {
      for (const userId of userIds) await invokeMock("cmd_kill_roblox", { userId });
      return userIds.length;
    }),
  };
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeMap({
    cancel_account_launch: true,
    stop_launch_queue: 2,
    focus_roblox_window: true,
    cmd_kill_roblox: true,
  });
});

afterEach(cleanup);

describe("SessionPage", () => {
  it("renders nothing while another page is open", () => {
    setStore({});
    const { container } = render(<SessionPage active={false} onLeave={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the queue and the running clients", () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: QUEUE,
      launchedByProgram: new Set([10]),
      ...storeActions(),
    });

    expect(screen.getByRole("heading", { level: 1, name: "Session" })).toBeInTheDocument();
    expect(screen.getByTestId("session-queue-10")).toBeInTheDocument();
    expect(screen.getByTestId("session-queue-20")).toBeInTheDocument();
    expect(screen.getByTestId("session-running-10")).toBeInTheDocument();
    expect(screen.queryByTestId("session-running-20")).not.toBeInTheDocument();
  });

  it("drives the same actions as the Console panel", async () => {
    const user = userEvent.setup();
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: QUEUE,
      launchedByProgram: new Set([10, 20]),
      ...storeActions(),
    });

    await user.click(within(screen.getByTestId("session-queue-20")).getByRole("button"));
    await waitFor(() => expect(callsFor("cancel_account_launch")).toHaveLength(1));
    expect(callsFor("cancel_account_launch")[0][1]).toEqual({ userId: 20 });

    await user.click(within(screen.getByTestId("session-running-10")).getByRole("button", { name: "Focus" }));
    await waitFor(() => expect(callsFor("focus_roblox_window")).toHaveLength(1));
    expect(callsFor("focus_roblox_window")[0][1]).toEqual({ userId: 10 });
  });

  it("closes every selected client after one confirmation", async () => {
    const user = userEvent.setup();
    promptAnswers.confirm = true;
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set([10, 20]),
      ...storeActions(),
    });

    await user.click(screen.getByRole("checkbox", { name: "Select all running clients" }));
    await user.click(screen.getByRole("button", { name: "Close selected (2)" }));

    await waitFor(() => expect(callsFor("cmd_kill_roblox")).toHaveLength(2));
  });

  it("leaves the page on Escape", async () => {
    const user = userEvent.setup();
    const onLeave = vi.fn();
    renderWithStore(<SessionPage active onLeave={onLeave} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      ...storeActions(),
    });

    await user.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledTimes(1);
  });
});

/**
 * A página mostra primeiro as listas da sessão, em qualquer largura; em cima
 * delas, uma faixa de números tirada dos mesmos dados das linhas (jogando,
 * reconectando, não responde, entrando), cada um um filtro. Os cartões de
 * ajuste (manter no jogo, gravações, teto de memória) vêm depois.
 */
describe("SessionPage — order and stats", () => {
  const DROP: ClientDrop = { kind: "disconnected", reason: "connectionLost", code: 277, message: null, sinceMs: 0 };

  function renderStats(overrides: Partial<StoreValue> = {}) {
    setInvokeMap({
      get_nexus_accounts: [],
      get_current_sessions: [{ userId: 10, placeId: 606849621, jobId: "job-a", sinceMs: Date.now() - 60_000, privateServer: false }],
    });
    return renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: [...ACCOUNTS, makeAccount({ UserID: 30, Username: "charlie" })],
      launchQueue: QUEUE,
      launchedByProgram: new Set([10, 20, 30]),
      clientHealth: new Map([
        [20, { pid: 2, logFound: true, drop: null, notResponding: true }],
        [30, { pid: 3, logFound: true, drop: DROP }],
      ]),
      ...storeActions(),
      ...overrides,
    });
  }

  it("puts the session lists first and the settings after them", () => {
    renderStats();
    const panel = screen.getByTestId("session-panel");
    const settings = screen.getByRole("complementary", { name: "Session settings" });
    expect(panel.compareDocumentPosition(settings) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByTestId("session-layout").className).not.toMatch(/col-reverse/);
    // O cartão "Right now" virou a faixa de números em cima das listas.
    expect(screen.queryByText("Right now")).not.toBeInTheDocument();
  });

  it("counts from the same data as the rows, not from the presence setting", async () => {
    renderStats();
    const stats = within(screen.getByRole("group", { name: "Right now" }));
    expect(await stats.findByRole("button", { name: "1 Playing" })).toBeInTheDocument();
    expect(stats.getByRole("button", { name: "0 Reconnecting" })).toBeDisabled();
    expect(stats.getByRole("button", { name: "1 Not responding" })).toBeInTheDocument();
    expect(stats.getByRole("button", { name: "1 Dropped" })).toBeInTheDocument();
    expect(stats.getByRole("button", { name: "2 Joining" })).toBeInTheDocument();
  });

  it("each number filters the In game list, and a second click shows everyone again", async () => {
    const user = userEvent.setup();
    renderStats();
    const notResponding = screen.getByRole("button", { name: "1 Not responding" });
    await user.click(notResponding);
    expect(notResponding).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("session-running-20")).toBeInTheDocument();
    expect(screen.queryByTestId("session-running-10")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-running-30")).not.toBeInTheDocument();
    await user.click(notResponding);
    expect(notResponding).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByTestId("session-running-10")).toBeInTheDocument();
  });

  it("the Joining number brings the queue into view", async () => {
    const user = userEvent.setup();
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    renderStats();
    await user.click(screen.getByRole("button", { name: "2 Joining" }));
    expect(scroll).toHaveBeenCalled();
    expect(scroll.mock.contexts[0]).toBe(document.querySelector("[data-tour='session-joining']"));
  });

  /** A página rola inteira: uma lista com rolagem própria dentro dela escondia contas. */
  it("has no scroll box of its own inside the lists", () => {
    renderStats();
    const inGame = document.querySelector("[data-tour='session-in-game']") as HTMLElement;
    expect(inGame.querySelector("[class*='max-h-']")).toBeNull();
  });

  it("says when Auto Rejoin keeps accounts in game and links to AFK Mode", async () => {
    const user = userEvent.setup();
    const { store } = renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      bottingStatus: makeBottingStatus({ active: true, userIds: [10, 20] }),
      ...storeActions(),
    });
    expect(screen.getByText("Auto Rejoin is running for 2 accounts.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open AFK Mode" }));
    expect(store.setActivePage).toHaveBeenCalledWith("afk");
  });

  it("says when auto-reconnect is bringing accounts back (a stopped one does not count)", () => {
    const drop = { kind: "crashed" as const, reason: null, code: null, message: null, sinceMs: 0 };
    const base = { attempt: 1, maxAttempts: 5, nextAttemptAtMs: null, reason: null, error: null, drop };
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      autoReconnect: [
        { ...base, userId: 10, phase: "waiting" },
        { ...base, userId: 20, phase: "stopped", reason: "joinedElsewhere" },
      ],
      ...storeActions(),
    });
    const summary = screen.getByRole("complementary", { name: "Session settings" });
    expect(within(summary).getByText("Auto-reconnect is bringing back 1 account.")).toBeInTheDocument();
  });

  /**
   * O padrão de reconexão de todas as contas mora também aqui, no cartão de
   * quem mantém as contas no jogo: é a mesma setting de Settings › General
   * (`General.AutoReconnect`), gravada pela store para as duas telas verem o
   * mesmo valor.
   */
  it("turns the reconnect default on and off from the summary, same setting as Settings › General", async () => {
    const user = userEvent.setup();
    const { store } = renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      settings: { General: { AutoReconnect: "false" } },
      ...storeActions(),
    });
    const summary = within(screen.getByRole("complementary", { name: "Session settings" }));
    const toggle = summary.getByRole("switch", { name: /Reconnect accounts that drop/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    expect(toggle).toHaveTextContent("never ones opened from the website");
    await user.click(toggle);
    expect(store.updateSetting).toHaveBeenCalledWith("General", "AutoReconnect", "true");
  });

  it("shows the reconnect default as on when the setting is on", () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      settings: { General: { AutoReconnect: "true" } },
      ...storeActions(),
    });
    const summary = within(screen.getByRole("complementary", { name: "Session settings" }));
    expect(summary.getByRole("switch", { name: /Reconnect accounts that drop/ })).toHaveAttribute(
      "aria-checked",
      "true"
    );
  });

  /** Teto de memória: o padrão de todos os clientes também mora no resumo. */
  it("sets the memory limit of every client from the summary, off by default", async () => {
    const user = userEvent.setup();
    const { store } = renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      platformCapabilities: makePlatformCapabilities({ supportsMemoryTrim: true }),
      ...storeActions(),
    });
    const summary = within(screen.getByRole("complementary", { name: "Session settings" }));
    const select = summary.getByRole("combobox", { name: "Memory limit for every client" }) as HTMLSelectElement;
    expect(select.value).toBe("0");
    expect(summary.getByText(/asks Windows to free its memory first/)).toBeInTheDocument();
    await user.selectOptions(select, "2048");
    expect(store.updateSetting).toHaveBeenCalledWith("Optimization", "MemoryLimit", "2048");

    // Fechar é opção própria, ao lado do limite, desligada por padrão.
    const close = summary.getByRole("switch", { name: "Close a client that stays over its limit" });
    expect(close).toHaveAttribute("aria-checked", "false");
    expect(summary.queryByText(/Close If Memory Low/)).not.toBeInTheDocument();
    await user.click(close);
    expect(store.updateSetting).toHaveBeenCalledWith("Optimization", "CloseOverMemoryLimit", "true");
  });

  /**
   * Pedido do dono: o Modo AFK repetindo a gravação e a gravação depois da
   * reconexão ficam à vista na página Session, com o atalho para a aba.
   */
  it("shows when the recordings play and opens the Recordings tab", async () => {
    setInvokeMap({
      get_recordings: {
        recordings: [{ id: "a", name: "Farm loop", steps: [], createdAt: 0, updatedAt: 0 }],
        defaultId: "a",
        accountIds: { "20": "a" },
        keys: [],
      },
      get_recording_playback: { active: false },
    });
    const { store } = renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      platformCapabilities: makePlatformCapabilities({ os: "windows" }),
      settings: {
        Afk: { Mode: "recording", IntervalMinutes: "2", IntervalSeconds: "30" },
        Recordings: { AfterReconnect: "true", AfterReconnectDelaySeconds: "45" },
      },
      ...storeActions(),
    });
    const card = await screen.findByTestId("session-recordings");
    expect(await within(card).findByText(/All accounts: Farm loop\./)).toBeInTheDocument();
    expect(card).toHaveTextContent("AFK mode: plays it every 2 min 30 s.");
    expect(card).toHaveTextContent("After an automatic reconnect: plays once, 45 s after the account is back in the game.");
    await userEvent.click(within(card).getByRole("button", { name: "Open Recordings" }));
    expect(store.openAfkMode).toHaveBeenCalledWith({ tab: "recordings" });
  });

  it("has no recordings card outside Windows", () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      platformCapabilities: makePlatformCapabilities({ os: "macos" }),
      ...storeActions(),
    });
    expect(screen.queryByTestId("session-recordings")).not.toBeInTheDocument();
  });

  it("shows the close-over-limit switch on when it was turned on", () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      platformCapabilities: makePlatformCapabilities({ supportsMemoryTrim: true }),
      settings: { Optimization: { CloseOverMemoryLimit: "true" } },
      ...storeActions(),
    });
    expect(screen.getByRole("switch", { name: "Close a client that stays over its limit" })).toHaveAttribute(
      "aria-checked",
      "true"
    );
  });

  it("has no memory limit in the summary without the memory-trim build", () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      ...storeActions(),
    });
    expect(screen.queryByRole("combobox", { name: "Memory limit for every client" })).not.toBeInTheDocument();
  });

  it("has no reconnect default outside Windows", () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      platformCapabilities: makePlatformCapabilities({ os: "macos" }),
      ...storeActions(),
    });
    expect(screen.queryByRole("switch", { name: /Reconnect accounts that drop/ })).not.toBeInTheDocument();
  });

  /** "1 accounts" / "1 contas": uma conta sozinha é singular nas três frases. */
  it("says one account in the singular", () => {
    const keepAlive = (extra: Partial<StoreValue>) => {
      cleanup();
      renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
        accounts: ACCOUNTS,
        launchQueue: null,
        launchedByProgram: new Set<number>(),
        ...storeActions(),
        ...extra,
      });
      return within(screen.getByRole("complementary", { name: "Session settings" }));
    };
    expect(
      keepAlive({ bottingStatus: makeBottingStatus({ active: true, userIds: [10] }) }).getByText(
        "Auto Rejoin is running for 1 account."
      )
    ).toBeInTheDocument();
    expect(
      keepAlive({ afkStatus: { active: true, accounts: [{ userId: 10 }] } as unknown as StoreValue["afkStatus"] }).getByText(
        "AFK Mode is on for 1 account."
      )
    ).toBeInTheDocument();
    expect(
      keepAlive({ afkStatus: { active: true, accounts: [{ userId: 10 }, { userId: 20 }] } as unknown as StoreValue["afkStatus"] }).getByText(
        "AFK Mode is on for 2 accounts."
      )
    ).toBeInTheDocument();
  });
});

/**
 * Tutorial da página (botão Tutorial no cabeçalho): cada passo aponta uma
 * parte que existe na tela e nenhum passo mexe em cliente ou fila.
 */
describe("SessionPage — tutorial", () => {
  it("walks every step of the Session tutorial with its part on screen", async () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: QUEUE,
      launchedByProgram: new Set([10]),
      unidentifiedClients: [
        { pid: 4100, reason: "noLog", userId: null, placeId: null, jobId: null, startedAtMs: 1_700_000_000_000 },
      ],
      ...storeActions(),
    });
    await walkTour("session", { invoke: invokeMock });
  });

  it("still works with nothing running (missing parts fall back)", async () => {
    renderWithStore(<SessionPage active onLeave={vi.fn()} />, {
      accounts: ACCOUNTS,
      launchQueue: null,
      launchedByProgram: new Set<number>(),
      ...storeActions(),
    });
    await walkTour("session", { invoke: invokeMock });
  });
});
