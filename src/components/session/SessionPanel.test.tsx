import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { SessionPanel } from "./SessionPanel";
import { makeAccount, makeBottingStatus, renderWithStore, setStore } from "../../test-utils/renderWithStore";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import type {
  FriendLinkState,
  LaunchQueueEntry,
  LaunchQueuePayload,
  LaunchQueueState,
} from "../../types";
import type { StoreValue } from "../../store";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "alpha" }),
  makeAccount({ UserID: 2, Username: "bravo", Alias: "Bravo Alt" }),
  makeAccount({ UserID: 3, Username: "charlie" }),
];

function entry(userId: number, state: LaunchQueueState, error: string | null = null): LaunchQueueEntry {
  return { userId, state, error, updatedAtMs: 1_700_000_000_000 };
}

function queue(entries: LaunchQueueEntry[], active = true): LaunchQueuePayload {
  return { entries, active, placeId: 5315046213, jobId: "" };
}

/**
 * A store é substituída por um mock, então as ações são reimplementadas aqui
 * exatamente como em `store.tsx`. Assim os testes travam o **contrato com o
 * backend** (nome do comando e argumentos que chegam ao `invoke`) e não apenas
 * o fato de o componente ter chamado a store.
 */
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
      let closed = 0;
      for (const userId of userIds) {
        if (await invokeMock("cmd_kill_roblox", { userId })) closed += 1;
      }
      return closed;
    }),
  };
}

function renderPanel(overrides: Partial<StoreValue> = {}) {
  return renderWithStore(<SessionPanel />, {
    accounts: ACCOUNTS,
    ...storeActions(),
    ...overrides,
  });
}

/** Chamadas do `invoke` mockado para um comando. */
function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  setInvokeMap({
    cancel_account_launch: true,
    stop_launch_queue: 3,
    focus_roblox_window: true,
    cmd_kill_roblox: true,
  });
});

afterEach(cleanup);

describe("SessionPanel — rendering", () => {
  it("lists the launch queue and the running clients", () => {
    renderPanel({
      launchQueue: queue([entry(1, "launching"), entry(2, "queued"), entry(3, "done")]),
      launchedByProgram: new Set([1, 3]),
    });

    // Fila: uma linha por conta, com o estado de cada uma.
    expect(within(screen.getByTestId("session-queue-1")).getByText("Joining")).toBeInTheDocument();
    expect(within(screen.getByTestId("session-queue-2")).getByText("Queued")).toBeInTheDocument();
    expect(within(screen.getByTestId("session-queue-3")).getByText("Joined")).toBeInTheDocument();

    // Em jogo: só as contas com cliente rodando.
    expect(screen.getByTestId("session-running-1")).toBeInTheDocument();
    expect(screen.getByTestId("session-running-3")).toBeInTheDocument();
    expect(screen.queryByTestId("session-running-2")).not.toBeInTheDocument();
    expect(screen.getByText("2 running")).toBeInTheDocument();
  });

  it("shows the alias and masks names when the app hides usernames", () => {
    renderPanel({
      launchQueue: queue([entry(2, "queued")]),
      launchedByProgram: new Set<number>(),
    });
    expect(within(screen.getByTestId("session-queue-2")).getByText("Bravo Alt")).toBeInTheDocument();

    cleanup();
    renderPanel({
      launchQueue: queue([entry(2, "queued")]),
      launchedByProgram: new Set<number>(),
      hideUsernames: true,
      hiddenNameLetters: 2,
    });
    const row = screen.getByTestId("session-queue-2");
    expect(within(row).queryByText("Bravo Alt")).not.toBeInTheDocument();
    expect(within(row).getByText("Br********")).toBeInTheDocument();
  });

  it("surfaces a failed entry's backend error", () => {
    // Fixture com frase, não com código: esta linha desenha `entry.error` cru, e
    // o backend manda frase justamente por isso (`version_conflict_message`).
    const erro =
      "A Roblox client is already running on a different Roblox version. Open now: system install.";
    renderPanel({
      launchQueue: queue([entry(1, "failed", erro)]),
      launchedByProgram: new Set<number>(),
    });
    const row = screen.getByTestId("session-queue-1");
    expect(within(row).getByText("Failed")).toBeInTheDocument();
    expect(within(row).getByText(erro)).toBeInTheDocument();
  });

  it("explains both empty states and disables Stop queue", () => {
    renderPanel({ launchQueue: null, launchedByProgram: new Set<number>() });

    expect(
      screen.getByText("Nothing in the launch queue. Pick a game to start joining accounts.")
    ).toBeInTheDocument();
    expect(
      screen.getByText("No Roblox client is running. Accounts you launch show up here.")
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Stop queue/ })).toBeDisabled();
  });
});

describe("SessionPanel — queue actions", () => {
  it("cancels only the clicked account", async () => {
    const user = userEvent.setup();
    renderPanel({
      launchQueue: queue([entry(1, "queued"), entry(2, "queued")]),
      launchedByProgram: new Set<number>(),
    });

    await user.click(within(screen.getByTestId("session-queue-2")).getByRole("button"));

    await waitFor(() => expect(callsFor("cancel_account_launch")).toHaveLength(1));
    expect(callsFor("cancel_account_launch")[0][1]).toEqual({ userId: 2 });
  });

  it("never closes a client when cancelling (cancel !== kill)", async () => {
    const user = userEvent.setup();
    // Conta 1 já está com cliente aberto E ainda na fila: cancelar não pode
    // encostar nesse cliente.
    renderPanel({
      launchQueue: queue([entry(1, "launching")]),
      launchedByProgram: new Set([1]),
    });

    await user.click(within(screen.getByTestId("session-queue-1")).getByRole("button"));
    await waitFor(() => expect(callsFor("cancel_account_launch")).toHaveLength(1));

    expect(callsFor("cmd_kill_roblox")).toHaveLength(0);
    expect(callsFor("cmd_kill_all_roblox")).toHaveLength(0);
  });

  it("offers no cancel button for finished entries", () => {
    renderPanel({
      launchQueue: queue([entry(1, "done"), entry(2, "failed", "boom"), entry(3, "cancelled")]),
      launchedByProgram: new Set<number>(),
    });
    for (const id of [1, 2, 3]) {
      expect(within(screen.getByTestId(`session-queue-${id}`)).queryByRole("button")).toBeNull();
    }
  });

  it("stops the whole queue from the header", async () => {
    const user = userEvent.setup();
    renderPanel({
      launchQueue: queue([entry(1, "queued"), entry(2, "queued")]),
      launchedByProgram: new Set<number>(),
    });

    await user.click(screen.getByRole("button", { name: /Stop queue/ }));

    await waitFor(() => expect(callsFor("stop_launch_queue")).toHaveLength(1));
    expect(callsFor("cmd_kill_roblox")).toHaveLength(0);
  });

  it("shows a backend failure instead of swallowing it", async () => {
    const user = userEvent.setup();
    setInvokeMap({
      stop_launch_queue: () => {
        throw new Error("queue is gone");
      },
    });
    renderPanel({
      launchQueue: queue([entry(1, "queued")]),
      launchedByProgram: new Set<number>(),
    });

    await user.click(screen.getByRole("button", { name: /Stop queue/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent("queue is gone");
  });
});

describe("SessionPanel — running clients", () => {
  it("focuses the clicked client's window", async () => {
    const user = userEvent.setup();
    renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 3]) });

    await user.click(within(screen.getByTestId("session-running-3")).getByRole("button", { name: "Focus" }));

    await waitFor(() => expect(callsFor("focus_roblox_window")).toHaveLength(1));
    expect(callsFor("focus_roblox_window")[0][1]).toEqual({ userId: 3 });
  });

  it("reports a window that could not be raised", async () => {
    const user = userEvent.setup();
    setInvokeMap({ focus_roblox_window: false });
    renderPanel({ launchQueue: null, launchedByProgram: new Set([1]) });

    await user.click(within(screen.getByTestId("session-running-1")).getByRole("button", { name: "Focus" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not bring that Roblox window to the front."
    );
  });

  it("closes a single client without asking for confirmation", async () => {
    const user = userEvent.setup();
    renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 2]) });

    await user.click(within(screen.getByTestId("session-running-2")).getByRole("button", { name: "Close" }));

    await waitFor(() => expect(callsFor("cmd_kill_roblox")).toHaveLength(1));
    expect(callsFor("cmd_kill_roblox")[0][1]).toEqual({ userId: 2 });
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("closes a multi-selection after a single confirmation", async () => {
    const user = userEvent.setup();
    promptAnswers.confirm = true;
    renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 2, 3]) });

    await user.click(screen.getByRole("checkbox", { name: "Select alpha" }));
    await user.click(screen.getByRole("checkbox", { name: "Select Bravo Alt" }));
    await user.click(screen.getByRole("checkbox", { name: "Select charlie" }));

    await user.click(screen.getByRole("button", { name: /Close accounts \(3\)/ }));

    await waitFor(() => expect(callsFor("cmd_kill_roblox")).toHaveLength(3));
    expect(confirmMock).toHaveBeenCalledTimes(1);
    expect(confirmMock.mock.calls[0][0]).toContain("3");
    expect(callsFor("cmd_kill_roblox").map((c) => c[1])).toEqual([
      { userId: 1 },
      { userId: 2 },
      { userId: 3 },
    ]);
  });

  it("closes nothing when the confirmation is declined", async () => {
    const user = userEvent.setup();
    promptAnswers.confirm = false;
    renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 2]) });

    await user.click(screen.getByRole("checkbox", { name: "Select all running clients" }));
    await user.click(screen.getByRole("button", { name: /Close accounts \(2\)/ }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(callsFor("cmd_kill_roblox")).toHaveLength(0);
  });
});

describe("SessionPanel — live updates", () => {
  it("follows the store's queue state without a reload", async () => {
    // O evento `launch-queue` é ouvido pela store; o painel só reflete o
    // estado. Re-renderizar com o payload novo é o que o listener faz.
    const { rerender } = renderPanel({
      launchQueue: queue([entry(1, "queued"), entry(2, "queued")]),
      launchedByProgram: new Set<number>(),
    });
    expect(within(screen.getByTestId("session-queue-1")).getByText("Queued")).toBeInTheDocument();

    setStore({
      accounts: ACCOUNTS,
      ...storeActions(),
      launchQueue: queue([entry(1, "done"), entry(2, "launching")]),
      launchedByProgram: new Set([1]),
    });
    rerender(<SessionPanel />);

    await waitFor(() =>
      expect(within(screen.getByTestId("session-queue-1")).getByText("Joined")).toBeInTheDocument()
    );
    expect(within(screen.getByTestId("session-queue-2")).getByText("Joining")).toBeInTheDocument();
    expect(screen.getByTestId("session-running-1")).toBeInTheDocument();
  });

  it("drops a client that stopped running from the selection", async () => {
    const user = userEvent.setup();
    const { rerender } = renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 2]) });

    await user.click(screen.getByRole("checkbox", { name: "Select all running clients" }));
    expect(screen.getByRole("button", { name: /Close accounts \(2\)/ })).toBeInTheDocument();

    // O polling deixa de ver a conta 2: ela some da lista e do lote.
    setStore({
      accounts: ACCOUNTS,
      ...storeActions(),
      launchQueue: null,
      launchedByProgram: new Set([1]),
    });
    rerender(<SessionPanel />);

    expect(screen.queryByTestId("session-running-2")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Close accounts \(1\)/ })).toBeInTheDocument();
  });
});

/**
 * Fechar as contas em jogo: sem marcação, o botão vale para **todas as que
 * estão na lista** — nunca para clientes fora dela (`closeRobloxClients`, não o
 * `killAllRobloxProcesses`). Mais de uma sempre pergunta antes.
 */
describe("SessionPanel — fechar contas", () => {
  it("sem marcar ninguém, fecha todas as da lista depois de uma confirmação", async () => {
    const user = userEvent.setup();
    promptAnswers.confirm = true;
    const { store } = renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 2]) });

    await user.click(screen.getByRole("button", { name: "Close accounts" }));

    await waitFor(() => expect(callsFor("cmd_kill_roblox")).toHaveLength(2));
    expect(confirmMock).toHaveBeenCalledTimes(1);
    expect(confirmMock.mock.calls[0][0]).toContain("2");
    expect(store.killAllRobloxProcesses).not.toHaveBeenCalled();
  });

  it("recusada a confirmação, nada fecha", async () => {
    const user = userEvent.setup();
    promptAnswers.confirm = false;
    renderPanel({ launchQueue: null, launchedByProgram: new Set([1, 2]) });

    await user.click(screen.getByRole("button", { name: "Close accounts" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(callsFor("cmd_kill_roblox")).toHaveLength(0);
  });

  it("com uma conta só em jogo, fecha sem perguntar", async () => {
    const user = userEvent.setup();
    renderPanel({ launchQueue: null, launchedByProgram: new Set([3]) });

    await user.click(screen.getByRole("button", { name: "Close accounts" }));

    await waitFor(() => expect(callsFor("cmd_kill_roblox")).toHaveLength(1));
    expect(callsFor("cmd_kill_roblox")[0][1]).toEqual({ userId: 3 });
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("não aparece sem cliente rodando", () => {
    renderPanel({ launchQueue: null, launchedByProgram: new Set() });
    expect(screen.queryByRole("button", { name: /Close accounts/ })).not.toBeInTheDocument();
  });
});

/**
 * O botão de Auto Rejoin daqui ligava o ciclo na hora, sem tela: quem clicava
 * não via o tempo do ciclo, nem as contas main, nem onde parar. Agora ele abre o
 * Modo AFK com as contas em jogo — o Start de lá é que adota, sem fechar nada.
 */
describe("SessionPanel — Modo AFK com as contas em jogo", () => {
  function comRodando(overrides: Partial<StoreValue> = {}) {
    return renderPanel({
      launchedByProgram: new Set([1, 2]),
      launchQueue: queue([]),
      ...overrides,
    });
  }

  it("abre o Modo AFK com as contas marcadas, sem ligar nada nem fechar cliente", async () => {
    const { store } = comRodando({ launchedByProgram: new Set([1, 2, 3]) });

    await userEvent.click(screen.getByRole("checkbox", { name: "Select alpha" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "Select charlie" }));
    await userEvent.click(screen.getByRole("button", { name: /AFK Mode/ }));

    expect(store.openAfkMode).toHaveBeenCalledWith({
      tab: "clicks",
      targetUserIds: [1, 3],
      adoptRunning: true,
    });
    expect(store.adoptRunningIntoBotting).not.toHaveBeenCalled();
    expect(store.startBottingMode).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("cmd_kill_roblox", expect.anything());
  });

  it("sem marcar ninguém, leva todas as que estão em jogo", async () => {
    const { store } = comRodando();

    await userEvent.click(screen.getByRole("button", { name: /AFK Mode/ }));

    expect(store.openAfkMode).toHaveBeenCalledWith({
      tab: "clicks",
      targetUserIds: [1, 2],
      adoptRunning: true,
    });
  });

  it("com só os cliques AFK ligados, abre na aba deles", async () => {
    const { store } = comRodando({
      afkStatus: {
        active: true,
        startedAtMs: 1,
        intervalMinutes: 10,
        key: "Space",
        mode: "key",
        clickX: 50,
        clickY: 50,
        accounts: [],
      },
    });

    await userEvent.click(screen.getByRole("button", { name: /AFK Mode/ }));

    expect(store.openAfkMode).toHaveBeenCalledWith(expect.objectContaining({ tab: "clicks" }));
  });

  /** Os cliques AFK são o padrão (pedido do dono, 03/10/2026); o Auto Rejoin só
   * abre direto quando é ele que está rodando. */
  it("com só o Auto Rejoin ligado, abre na aba dele", async () => {
    const { store } = comRodando({ bottingStatus: makeBottingStatus({ active: true, userIds: [1, 2] }) });

    await userEvent.click(screen.getByRole("button", { name: /AFK Mode/ }));

    expect(store.openAfkMode).toHaveBeenCalledWith(expect.objectContaining({ tab: "rejoin" }));
  });

  it("não oferece o botão quando não há cliente rodando", () => {
    renderPanel({ launchedByProgram: new Set(), launchQueue: queue([]) });
    expect(screen.queryByRole("button", { name: /AFK Mode/ })).not.toBeInTheDocument();
  });
});

/**
 * Make Friends não tinha como ser acompanhado: o progresso era `{phase, done,
 * total}` num `useState` de dois componentes, e na fase de envio o `done`
 * contava **pares**. Agora o painel mostra conta por conta, no mesmo lugar em
 * que se acompanha a fila de launch.
 */
describe("SessionPanel — Make Friends", () => {
  function friendLink(overrides: Partial<FriendLinkState> = {}): FriendLinkState {
    return {
      active: true,
      phase: "linking",
      processed: 1,
      total: 3,
      mode: "star",
      mainUserId: 1,
      accounts: [
        { userId: 1, state: "processing", error: null },
        { userId: 2, state: "done", error: null },
        { userId: 3, state: "pending", error: null },
      ],
      ...overrides,
    };
  }

  it("não ocupa espaço no painel enquanto ninguém rodou Make Friends", () => {
    renderPanel({ launchQueue: queue([entry(1, "queued")]) });
    expect(screen.queryByTestId("friend-link-panel")).not.toBeInTheDocument();
  });

  it("diz quantas contas já foram processadas e o estado de cada uma", () => {
    renderPanel({ friendLinkState: friendLink() });

    const painel = within(screen.getByTestId("friend-link-panel"));
    expect(painel.getByText("1 / 3 accounts processed")).toBeInTheDocument();
    expect(painel.getByText("Sending friend requests")).toBeInTheDocument();

    expect(within(screen.getByTestId("friend-link-1")).getByText("Processing")).toBeInTheDocument();
    expect(within(screen.getByTestId("friend-link-2")).getByText("Linked")).toBeInTheDocument();
    expect(within(screen.getByTestId("friend-link-3")).getByText("Waiting")).toBeInTheDocument();
  });

  it("marca qual é a conta principal do modo star", () => {
    renderPanel({ friendLinkState: friendLink() });
    expect(within(screen.getByTestId("friend-link-1")).getByText("main")).toBeInTheDocument();
    expect(within(screen.getByTestId("friend-link-2")).queryByText("main")).not.toBeInTheDocument();
  });

  it("mostra o erro na conta que falhou, e não num texto agregado", () => {
    renderPanel({
      friendLinkState: friendLink({
        accounts: [
          { userId: 1, state: "failed", error: "cookie inválido" },
          { userId: 2, state: "done", error: null },
          { userId: 3, state: "done", error: null },
        ],
      }),
    });

    const linha = within(screen.getByTestId("friend-link-1"));
    expect(linha.getByText("Failed")).toBeInTheDocument();
    expect(linha.getByText("cookie inválido")).toBeInTheDocument();
  });

  it("continua mostrando o resultado depois que a operação termina", () => {
    renderPanel({
      friendLinkState: friendLink({ active: false, phase: "done", processed: 3 }),
    });

    const painel = within(screen.getByTestId("friend-link-panel"));
    expect(painel.getByText("3 / 3 accounts processed")).toBeInTheDocument();
    expect(painel.getByText("Finished")).toBeInTheDocument();
  });

  it("usa o alias mascarado, como o resto do painel", () => {
    renderPanel({
      friendLinkState: friendLink(),
      hideUsernames: true,
      hiddenNameLetters: 2,
    });
    expect(within(screen.getByTestId("friend-link-2")).getByText("Br********")).toBeInTheDocument();
  });
});
