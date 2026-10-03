import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { RejoinTab } from "./RejoinTab";
import {
  defaultSettings,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { BottingAccountStatus, StoreValue } from "../../store";
import { saveFavorites, type FavoriteGame } from "../server-list/types";
import { MAX_ALIAS_LENGTH } from "../../types";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });
const C = makeAccount({ UserID: 3, Username: "cid" });
/** Sem cliente aberto: não entra na lista do Auto Rejoin. */
const D = makeAccount({ UserID: 4, Username: "dan" });

const PLACE = 606849621;

/** Auto Rejoin refuses to start unless Multi Roblox is on. */
function settings(multiRbx: boolean, extra: Record<string, string> = {}) {
  const s = defaultSettings();
  s.General.EnableMultiRbx = multiRbx ? "true" : "false";
  s.General.BottingEnabled = "true";
  Object.assign(s.General, extra);
  return s;
}

function renderTab(
  overrides: Partial<StoreValue> = {},
  props: { targetUserIds?: number[]; initialPlaceId?: string | null; adoptRunning?: boolean } = {}
) {
  const store = setStore({
    accounts: [A, B, C, D],
    selectedIds: new Set<number>(),
    selectedAccounts: [],
    launchedByProgram: new Set([1, 2, 3]),
    settings: settings(true),
    detectRunningGamePlace: vi.fn(async () => PLACE),
    ...overrides,
  });
  render(
    <RejoinTab
      targetUserIds={props.targetUserIds ?? [1, 2]}
      initialPlaceId={props.initialPlaceId ?? null}
      adoptRunning={props.adoptRunning}
    />
  );
  return { store };
}

const startButton = () => screen.getByRole("button", { name: "Start Auto Rejoin" });
const accountButton = (name: string) => screen.getByRole("button", { name });
const currentOption = () => screen.getByRole("radio", { name: /The game they are playing now/ });
const gameOption = () => screen.getByRole("radio", { name: /A game I pick/ });

function favorite(overrides: Partial<FavoriteGame> = {}): FavoriteGame {
  return {
    placeId: 920587237,
    name: "Adopt Me!",
    iconUrl: null,
    addedAt: 0,
    vipServers: [],
    ...overrides,
  };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  localStorage.clear();
  // A aba relê o INI do backend ao abrir.
  setInvokeHandler((cmd) => (cmd === "get_all_settings" ? {} : undefined));
});

afterEach(cleanup);

/**
 * Pedido do dono (03/10/2026): escolher as contas como nos cliques AFK — uma
 * lista das contas com cliente aberto, marca quem vai e dá Start. Sem cartão
 * "Targets", sem "Main accounts", e só dois botões de lote.
 */
describe("RejoinTab — escolher as contas", () => {
  it("lista só as contas com cliente aberto", () => {
    renderTab();
    expect(accountButton("ann")).toBeInTheDocument();
    expect(accountButton("bob")).toBeInTheDocument();
    expect(accountButton("cid")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "dan" })).not.toBeInTheDocument();
  });

  it("chega com as contas de quem abriu marcadas", () => {
    renderTab({}, { targetUserIds: [1, 3] });
    expect(accountButton("ann")).toHaveAttribute("aria-pressed", "true");
    expect(accountButton("bob")).toHaveAttribute("aria-pressed", "false");
    expect(accountButton("cid")).toHaveAttribute("aria-pressed", "true");
  });

  it("sem contas de quem abriu, marca a seleção da lista principal", () => {
    setStore({
      accounts: [A, B, C, D],
      selectedIds: new Set([2, 4]),
      selectedAccounts: [B, D],
      launchedByProgram: new Set([1, 2, 3]),
      settings: settings(true),
    });
    render(<RejoinTab />);
    expect(accountButton("bob")).toHaveAttribute("aria-pressed", "true");
    expect(accountButton("ann")).toHaveAttribute("aria-pressed", "false");
  });

  it("marcar e desmarcar uma conta muda quem vai", async () => {
    renderTab();
    await userEvent.click(accountButton("cid"));
    expect(accountButton("cid")).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(accountButton("ann"));
    expect(accountButton("ann")).toHaveAttribute("aria-pressed", "false");
  });

  it("Select all e Clear são os únicos botões de lote", async () => {
    renderTab({}, { targetUserIds: [] });
    await userEvent.click(screen.getByRole("button", { name: "Select all" }));
    for (const name of ["ann", "bob", "cid"]) {
      expect(accountButton(name)).toHaveAttribute("aria-pressed", "true");
    }
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    for (const name of ["ann", "bob", "cid"]) {
      expect(accountButton(name)).toHaveAttribute("aria-pressed", "false");
    }
    expect(screen.queryByRole("button", { name: "Select alts" })).not.toBeInTheDocument();
    expect(screen.queryByText("All visible")).not.toBeInTheDocument();
  });

  it("não tem Targets nem contas main", () => {
    renderTab();
    expect(screen.queryByText("Targets")).not.toBeInTheDocument();
    expect(screen.queryByText("Main Accounts")).not.toBeInTheDocument();
    expect(screen.queryByText(/Main Grace/)).not.toBeInTheDocument();
  });

  it("sem cliente aberto, diz o que fazer e não liga", () => {
    renderTab({ launchedByProgram: new Set() }, { targetUserIds: [] });
    expect(screen.getByTestId("rejoin-status")).toHaveTextContent("No Roblox client is open yet");
    expect(startButton()).toBeDisabled();
  });

  it("com uma conta só marcada, não liga", async () => {
    renderTab({}, { targetUserIds: [1] });
    await waitFor(() =>
      expect(screen.getByTestId("rejoin-status")).toHaveTextContent("Select at least 2 accounts")
    );
    expect(startButton()).toBeDisabled();
  });
});

/**
 * O servidor padrão é onde as contas já estão: nada de Place ID para digitar.
 * O Start adota os clientes abertos (nada fecha agora) e cada rejoin volta ao
 * jogo detectado pela presença.
 */
describe("RejoinTab — o jogo em que as contas estão (padrão)", () => {
  it("é a opção marcada ao abrir", () => {
    renderTab();
    expect(currentOption()).toBeChecked();
    expect(gameOption()).not.toBeChecked();
  });

  it("descobre o jogo de cada conta marcada", async () => {
    const { store } = renderTab();
    await waitFor(() => expect(startButton()).toBeEnabled());
    expect(store.detectRunningGamePlace).toHaveBeenCalledWith([1]);
    expect(store.detectRunningGamePlace).toHaveBeenCalledWith([2]);
  });

  it("o Start adota as contas no jogo detectado, sem main e sem relançar ninguém", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings"
        ? {
            General: {
              BottingDefaultIntervalMinutes: "25",
              BottingLaunchDelaySeconds: "30",
              BottingDraftPlaceId: "1234",
              BottingDraftPlayerAccountIds: "1",
            },
          }
        : undefined
    );
    const { store } = renderTab();
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.adoptRunningIntoBotting).toHaveBeenCalledWith(
        [1, 2],
        expect.objectContaining({
          placeId: PLACE,
          intervalMinutes: 25,
          launchDelaySeconds: 30,
          playerUserIds: [],
        })
      )
    );
    expect(store.startBottingMode).not.toHaveBeenCalled();
    expect(store.closeRobloxClients).not.toHaveBeenCalled();
  });

  it("contas em jogos diferentes: diz isso e pede um jogo", async () => {
    renderTab({
      detectRunningGamePlace: vi.fn(async (ids: number[]) => (ids[0] === 1 ? 111 : 222)),
    });
    await waitFor(() =>
      expect(screen.getByTestId("rejoin-status")).toHaveTextContent(
        "The selected accounts are in different games"
      )
    );
    expect(startButton()).toBeDisabled();
  });

  it("sem descobrir o jogo: diz isso e pede um jogo, sem usar o rascunho", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlaceId: "1234" } } : undefined
    );
    renderTab({ detectRunningGamePlace: vi.fn(async () => null) });
    await waitFor(() =>
      expect(screen.getByTestId("rejoin-status")).toHaveTextContent(
        "Could not tell which game these accounts are in"
      )
    );
    expect(startButton()).toBeDisabled();
  });

  it("uma conta sem presença não impede quando as outras dizem o jogo", async () => {
    const { store } = renderTab({
      detectRunningGamePlace: vi.fn(async (ids: number[]) => (ids[0] === 1 ? PLACE : null)),
    });
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());
    await waitFor(() =>
      expect(store.adoptRunningIntoBotting).toHaveBeenCalledWith(
        [1, 2],
        expect.objectContaining({ placeId: PLACE })
      )
    );
  });

  it("não grava place nem job no rascunho", async () => {
    const saved: Array<Record<string, unknown>> = [];
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "update_setting") saved.push(args as Record<string, unknown>);
      return undefined;
    });
    renderTab();
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());

    await waitFor(() => expect(saved.length).toBeGreaterThan(0));
    const keys = saved.map((it) => it.key);
    expect(keys).not.toContain("BottingDraftPlaceId");
    expect(keys).not.toContain("BottingDraftJobId");
    expect(keys).toContain("BottingDefaultIntervalMinutes");
  });
});

/** Segunda opção: um jogo dos Favoritos, ou um dos servidores VIP salvos nele. */
describe("RejoinTab — um jogo escolhido", () => {
  it("mostra os favoritos e liga no favorito escolhido, relançando as contas", async () => {
    saveFavorites([favorite()]);
    const { store } = renderTab();

    await userEvent.click(gameOption());
    await userEvent.click(screen.getByRole("button", { name: /Adopt Me!/ }));
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.startBottingMode).toHaveBeenCalledWith(
        expect.objectContaining({
          userIds: [1, 2],
          placeId: 920587237,
          jobId: "",
          launchData: "",
          playerUserIds: [],
        })
      )
    );
    expect(store.adoptRunningIntoBotting).not.toHaveBeenCalled();
  });

  it("um servidor VIP do favorito vai como Job ID", async () => {
    saveFavorites([
      favorite({ vipServers: [{ id: "v1", name: "My VIP", link: "https://www.roblox.com/share?code=abc&type=Server" }] }),
    ]);
    const { store } = renderTab();

    await userEvent.click(gameOption());
    await userEvent.click(screen.getByRole("button", { name: /My VIP/ }));
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.startBottingMode).toHaveBeenCalledWith(
        expect.objectContaining({
          placeId: 920587237,
          jobId: "https://www.roblox.com/share?code=abc&type=Server",
        })
      )
    );
  });

  it("escolher um favorito marca a opção de jogo escolhido", async () => {
    saveFavorites([favorite()]);
    renderTab();
    await userEvent.click(gameOption());
    await userEvent.click(screen.getByRole("button", { name: /Adopt Me!/ }));
    expect(gameOption()).toBeChecked();
    expect(screen.getByRole("button", { name: /Adopt Me!/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("sem favoritos, diz onde criar um", async () => {
    renderTab();
    await userEvent.click(gameOption());
    expect(screen.getByText(/No favorite games yet/)).toBeInTheDocument();
    expect(startButton()).toBeDisabled();
    expect(screen.getByTestId("rejoin-status")).toHaveTextContent("Pick a game");
  });

  it("o Place ID digitado fica guardado num Avançado fechado", async () => {
    const { store } = renderTab();
    await userEvent.click(gameOption());
    const advanced = screen.getByText("Type a Place ID").closest("details") as HTMLDetailsElement;
    expect(advanced.open).toBe(false);

    await userEvent.click(screen.getByText("Type a Place ID"));
    await userEvent.type(screen.getByPlaceholderText("Place ID"), "606849621");
    await userEvent.type(screen.getByPlaceholderText("Job ID or private server link (optional)"), "job-1");
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.startBottingMode).toHaveBeenCalledWith(
        expect.objectContaining({ placeId: 606849621, jobId: "job-1" })
      )
    );
  });

  it("o jogo escolhido na abertura já vem marcado", async () => {
    const { store } = renderTab({}, { initialPlaceId: "606849621" });
    expect(gameOption()).toBeChecked();
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());
    await waitFor(() =>
      expect(store.startBottingMode).toHaveBeenCalledWith(expect.objectContaining({ placeId: 606849621 }))
    );
  });

  it("aberto pelo Em jogo, o padrão continua sendo onde as contas estão", () => {
    renderTab({}, { initialPlaceId: "606849621", adoptRunning: true });
    expect(currentOption()).toBeChecked();
  });

  it("o último jogo escolhido volta pré-selecionado, mas a opção padrão não muda", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings"
        ? { General: { BottingDraftPlaceId: "920587237", BottingDraftJobId: "" } }
        : undefined
    );
    saveFavorites([favorite()]);
    renderTab();
    expect(currentOption()).toBeChecked();
    await userEvent.click(gameOption());
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Adopt Me!/ })).toHaveAttribute("aria-pressed", "true")
    );
  });

  it("grava o jogo escolhido no rascunho ao ligar", async () => {
    const saved: Array<Record<string, unknown>> = [];
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "update_setting") saved.push(args as Record<string, unknown>);
      return undefined;
    });
    saveFavorites([favorite()]);
    renderTab();
    await userEvent.click(gameOption());
    await userEvent.click(screen.getByRole("button", { name: /Adopt Me!/ }));
    await userEvent.click(startButton());
    await waitFor(() =>
      expect(saved).toContainEqual(
        expect.objectContaining({ key: "BottingDraftPlaceId", value: "920587237" })
      )
    );
  });
});

describe("RejoinTab — start guards", () => {
  it("explica e não liga com o Multi Roblox desligado", async () => {
    renderTab({ settings: settings(false) });
    expect(screen.getByTestId("rejoin-status")).toHaveTextContent(
      "Auto Rejoin currently requires Multi Roblox to be enabled"
    );
    expect(startButton()).toBeDisabled();
  });

  it("mostra a falha do Start", async () => {
    const { store } = renderTab();
    (store.adoptRunningIntoBotting as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("failed to enable multi roblox")
    );
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("failed to enable multi roblox"))
    );
  });
});

/** Só a visão nova: o Classic e a troca de visão saíram (pedido do dono). */
describe("RejoinTab — uma visão só", () => {
  it("não oferece New View nem Classic, mesmo com o INI antigo", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDualPanelDialog: "false" } } : undefined
    );
    renderTab({ settings: settings(true, { BottingDualPanelDialog: "false" }) });
    await waitFor(() => expect(startButton()).toBeEnabled());
    expect(screen.queryByRole("button", { name: /New View/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Classic/ })).not.toBeInTheDocument();
  });

  it("mantém o tempo do ciclo, com a unidade de cada campo", () => {
    renderTab();
    expect(screen.getByRole("textbox", { name: "Rejoin every (minutes)" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Time between launches (seconds)" })).toBeInTheDocument();
  });

  it("diz em uma linha o que cada rejoin faz", () => {
    renderTab();
    expect(
      screen.getByText(
        "Each rejoin closes that account's Roblox client and opens it again. Clients of other accounts are never touched."
      )
    ).toBeInTheDocument();
  });
});

/** Uma linha do painel ao vivo, como o backend a manda em `status.accounts`. */
function botRow(overrides: Partial<BottingAccountStatus> = {}): BottingAccountStatus {
  return {
    userId: 1,
    isPlayer: false,
    disconnected: false,
    phase: "waiting-rejoin",
    retryCount: 0,
    nextRestartAtMs: null,
    playerGraceUntilMs: null,
    lastError: null,
    ...overrides,
  };
}

/** Sessão ativa com as duas contas. */
function activeSession() {
  return makeBottingStatus({
    active: true,
    placeId: PLACE,
    userIds: [1, 2],
    accounts: [botRow({ userId: 1 }), botRow({ userId: 2 })],
  });
}

describe("RejoinTab — barra de estado", () => {
  it("diz que o Auto Rejoin está parado, com o Start", () => {
    renderTab();
    const bar = screen.getByTestId("rejoin-status");
    expect(bar).toHaveTextContent("Auto Rejoin is stopped");
    expect(bar).toHaveTextContent("2 accounts selected");
    expect(within(bar).getByRole("button", { name: "Start Auto Rejoin" })).toBeInTheDocument();
  });

  it("diz que está rodando, e só oferece parar", () => {
    renderTab({ bottingStatus: activeSession() });
    const bar = screen.getByTestId("rejoin-status");
    expect(bar).toHaveTextContent("Auto Rejoin is running");
    expect(bar).toHaveTextContent("2 accounts in the cycle");
    expect(within(bar).queryByRole("button", { name: "Start Auto Rejoin" })).not.toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "Stop Auto Rejoin" })).toBeInTheDocument();
  });

  /**
   * Queixa do dono: o Auto Rejoin ligava e ele não achava onde parar. Ligar e
   * parar ficam na barra de estado, **fora** da área que rola.
   */
  it("Start e Stop ficam fora da área que rola", () => {
    renderTab();
    const scroll = screen.getByTestId("rejoin-scroll");
    expect(scroll.className).toContain("overflow-y-auto");
    expect(scroll.contains(startButton())).toBe(false);
    cleanup();
    renderTab({ bottingStatus: activeSession() });
    const stop = screen.getByRole("button", { name: "Stop Auto Rejoin" });
    expect(screen.getByTestId("rejoin-scroll").contains(stop)).toBe(false);
  });

  it("com sessão ligada, oferece acrescentar quem está marcado e ainda não está nela", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() }, { targetUserIds: [1, 3] });

    await userEvent.click(screen.getByRole("button", { name: "Add to Auto Rejoin (1)" }));

    expect(store.adoptRunningIntoBotting).toHaveBeenCalledWith([3]);
    expect(store.startBottingMode).not.toHaveBeenCalled();
  });

  /**
   * Achado no harness: parar o ciclo deixava marcada só a conta acrescentada
   * por último, e religar levava uma conta só. Como nos cliques AFK, quem
   * estava no ciclo continua marcado depois do Stop.
   */
  it("depois de parar, as contas do ciclo continuam marcadas", async () => {
    const store = setStore({
      accounts: [A, B, C],
      launchedByProgram: new Set([1, 2, 3]),
      settings: settings(true),
      bottingStatus: activeSession(),
      detectRunningGamePlace: vi.fn(async () => PLACE),
    });
    const { rerender } = render(<RejoinTab targetUserIds={[]} />);
    setStore({ ...store, bottingStatus: makeBottingStatus({ active: false }) });
    rerender(<RejoinTab targetUserIds={[]} />);

    expect(accountButton("ann")).toHaveAttribute("aria-pressed", "true");
    expect(accountButton("bob")).toHaveAttribute("aria-pressed", "true");
    expect(accountButton("cid")).toHaveAttribute("aria-pressed", "false");
  });

  it("com sessão ligada, as contas abertas fora dela aparecem para marcar", async () => {
    renderTab({ bottingStatus: activeSession() }, { targetUserIds: [] });
    await userEvent.click(accountButton("cid"));
    expect(screen.getByRole("button", { name: "Add to Auto Rejoin (1)" })).toBeInTheDocument();
  });
});

describe("RejoinTab — stop controls", () => {
  it("Stop não pergunta nada e não fecha cliente", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop Auto Rejoin" }));

    expect(confirmMock).not.toHaveBeenCalled();
    expect(store.stopBottingMode).toHaveBeenCalledWith(false);
  });

  it("Stop + Close diz quantos clientes fecha e o que fica aberto", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Clients" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    const [message, destructive] = confirmMock.mock.calls[0];
    expect(message).toContain("2 accounts");
    expect(message).toContain("outside this session are left alone");
    expect(destructive).toBe(true);
    // Recusado: nada fecha.
    expect(store.stopBottingMode).not.toHaveBeenCalled();

    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Clients" }));
    await waitFor(() => expect(store.stopBottingMode).toHaveBeenCalledWith(true));
  });
});

/**
 * Fechar cliente é irreversível para quem está jogando: a tela diz quantos
 * clientes fecham antes de fechar. Na lista ao vivo, também só Select all e
 * Clear.
 */
describe("RejoinTab — lista ao vivo", () => {
  function liveList(): HTMLElement {
    return screen.getByTestId("rejoin-live");
  }

  it("só tem Select all e Clear como seleção em lote", () => {
    renderTab({ bottingStatus: activeSession() });
    const list = liveList();
    expect(within(list).getByRole("button", { name: "Select all" })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: "Clear" })).toBeInTheDocument();
    expect(within(list).queryByRole("button", { name: "Select alts" })).not.toBeInTheDocument();
    expect(within(list).queryByText("All visible")).not.toBeInTheDocument();
    expect(within(list).queryByText(/Mains/)).not.toBeInTheDocument();
  });

  it("o lote Close client diz quantos clientes fecha e não fecha se recusado", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });

    await userEvent.click(within(liveList()).getByRole("button", { name: "Select all" }));
    await userEvent.click(screen.getByRole("button", { name: "Close client (2)" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("2 accounts");
    expect(confirmMock.mock.calls[0][1]).toBe(true);
    expect(store.bottingAccountAction).not.toHaveBeenCalled();
  });

  it("o lote Close + Disconnect diz que as contas saem do ciclo", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });
    promptAnswers.confirm = true;

    await userEvent.click(within(liveList()).getByRole("button", { name: "Select all" }));
    await userEvent.click(screen.getByRole("button", { name: "Close + Disconnect (2)" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("rejoin cycle");
    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledTimes(2));
    expect(store.bottingAccountAction).toHaveBeenCalledWith(1, "closeDisconnect");
    expect(store.bottingAccountAction).toHaveBeenCalledWith(2, "closeDisconnect");
  });

  it("Clear tira a seleção e esconde as ações em lote", async () => {
    renderTab({ bottingStatus: activeSession() });
    await userEvent.click(within(liveList()).getByRole("button", { name: "Select all" }));
    expect(screen.getByRole("button", { name: "Restart loop (2)" })).toBeInTheDocument();
    await userEvent.click(within(liveList()).getByRole("button", { name: "Clear" }));
    expect(screen.queryByRole("button", { name: "Restart loop (2)" })).not.toBeInTheDocument();
  });

  /**
   * `closeDisconnect` tira a conta do ciclo até alguém reconectar
   * (`botting_action_flags`); `close` sozinho é transitório, porque o loop
   * reabre no próximo restart.
   */
  it("a linha Close + Disconnect pergunta antes de tirar a conta do ciclo", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });

    await userEvent.click(screen.getAllByRole("button", { name: "Close + Disconnect" })[0]);

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("rejoin cycle");
    expect(store.bottingAccountAction).not.toHaveBeenCalled();
  });

  it("a linha Close + Disconnect roda quando a pergunta é aceita", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });
    promptAnswers.confirm = true;

    await userEvent.click(screen.getAllByRole("button", { name: "Close + Disconnect" })[0]);

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledWith(1, "closeDisconnect"));
  });

  it("a linha Close client segue sem pergunta: o loop reabre o cliente", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });

    await userEvent.click(screen.getAllByRole("button", { name: "Close client" })[0]);

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledWith(1, "close"));
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("os lotes não destrutivos seguem sem pergunta", async () => {
    const { store } = renderTab({ bottingStatus: activeSession() });

    await userEvent.click(within(liveList()).getByRole("button", { name: "Select all" }));
    await userEvent.click(screen.getByRole("button", { name: "Restart loop (2)" }));

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledTimes(2));
    expect(confirmMock).not.toHaveBeenCalled();
  });
});

/** Nome comprido cortado na tela: o `title` traz o nome inteiro. */
describe("RejoinTab — nomes compridos", () => {
  const LONG = "L".repeat(MAX_ALIAS_LENGTH);
  const LA = makeAccount({ UserID: 1, Username: "ann", Alias: LONG });

  it("na lista de contas e na lista ao vivo", () => {
    renderTab({ accounts: [LA, B, C], bottingStatus: activeSession() }, { targetUserIds: [] });
    expect(within(screen.getByTestId("rejoin-live")).getByTitle(LONG)).toHaveClass("truncate");
    cleanup();
    renderTab({ accounts: [LA, B, C] });
    expect(screen.getByTitle(LONG)).toHaveClass("truncate");
  });
});

/**
 * Com "Names hidden" na toolbar, nada do Auto Rejoin pode mostrar o nome real
 * nem a foto da conta.
 */
describe("RejoinTab — nomes ocultos", () => {
  const SA = makeAccount({ UserID: 1, Username: "secretann", Alias: "AliasAnn" });
  const SB = makeAccount({ UserID: 2, Username: "secretbob" });
  const HIDDEN = {
    accounts: [SA, SB],
    hideUsernames: true,
    hiddenNameLetters: 0,
    showAvatarsWhenHidden: false,
    launchedByProgram: new Set([1, 2]),
    avatarUrls: new Map([
      [1, "https://avatar.test/one.png"],
      [2, "https://avatar.test/two.png"],
    ]),
  };

  function expectNoRealName() {
    const html = document.body.innerHTML;
    for (const leak of ["secretann", "AliasAnn", "secretbob", "avatar.test"]) expect(html).not.toContain(leak);
  }

  it("lista de contas", () => {
    renderTab(HIDDEN);
    expectNoRealName();
  });

  it("lista ao vivo", () => {
    renderTab({ ...HIDDEN, bottingStatus: activeSession() });
    expect(screen.getByTestId("rejoin-live")).toBeInTheDocument();
    expectNoRealName();
  });

  it("com a opção de manter avatares, a foto volta mas o nome não", () => {
    renderTab({ ...HIDDEN, showAvatarsWhenHidden: true, bottingStatus: activeSession() });
    expect(document.body.innerHTML).toContain("avatar.test/one.png");
    expect(document.body.innerHTML).not.toContain("secretann");
  });
});
