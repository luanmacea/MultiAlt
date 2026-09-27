import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { BottingDialog } from "./BottingDialog";
import {
  defaultSettings,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { BottingAccountStatus, StoreValue } from "../../store";
import { MAX_ALIAS_LENGTH } from "../../types";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });

/** Botting refuses to start unless Multi Roblox is on. */
function settings(multiRbx: boolean, draft: Record<string, string> = {}) {
  const s = defaultSettings();
  s.General.EnableMultiRbx = multiRbx ? "true" : "false";
  s.General.BottingEnabled = "true";
  Object.assign(s.General, draft);
  return s;
}

function renderDialog(
  overrides: Partial<StoreValue> = {},
  selected = [A, B],
  initialPlaceId: string | null = null
) {
  const store = setStore({
    accounts: [A, B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    settings: settings(true),
    ...overrides,
  });
  const onClose = vi.fn();
  render(<BottingDialog open onClose={onClose} initialPlaceId={initialPlaceId} />);
  return { store, onClose };
}

const startButton = () => screen.getByRole("button", { name: "Start Botting Mode" });
const placeIdField = () => screen.getByPlaceholderText("Place ID");

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  // The dialog re-reads settings from the backend when it opens.
  setInvokeHandler((cmd) => (cmd === "get_all_settings" ? {} : undefined));
});

afterEach(cleanup);

describe("BottingDialog — start guards", () => {
  it("renders nothing while closed", () => {
    setStore({ accounts: [A, B] });
    const { container } = render(<BottingDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("keeps Start disabled without a Place ID", async () => {
    renderDialog();
    expect(startButton()).toBeDisabled();

    await userEvent.type(placeIdField(), "606849621");
    await waitFor(() => expect(startButton()).toBeEnabled());
  });

  it("keeps Start disabled with fewer than two accounts", async () => {
    renderDialog({}, [A]);
    await userEvent.type(placeIdField(), "606849621");
    expect(startButton()).toBeDisabled();
  });

  it("keeps Start disabled and explains why when Multi Roblox is off", async () => {
    renderDialog({ settings: settings(false) });
    await userEvent.type(placeIdField(), "606849621");

    expect(
      screen.getByText("Botting Mode currently requires Multi Roblox to be enabled")
    ).toBeInTheDocument();
    expect(startButton()).toBeDisabled();
  });

  it("starts the loop with the typed configuration", async () => {
    const { store } = renderDialog();
    await userEvent.type(placeIdField(), "606849621");
    await userEvent.type(screen.getByPlaceholderText("Job ID (optional)"), "job-1");
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.startBottingMode).toHaveBeenCalledWith(
        expect.objectContaining({
          userIds: [1, 2],
          placeId: 606849621,
          jobId: "job-1",
          playerUserIds: [],
        })
      )
    );
  });

  it("surfaces a start failure inline", async () => {
    const { store } = renderDialog();
    (store.startBottingMode as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("failed to enable multi roblox")
    );
    await userEvent.type(placeIdField(), "606849621");
    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(
        expect.stringContaining("failed to enable multi roblox")
      )
    );
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

/** Sessão ativa com as duas contas como bot. */
function activeSession() {
  return makeBottingStatus({
    active: true,
    userIds: [1, 2],
    accounts: [botRow({ userId: 1 }), botRow({ userId: 2 })],
  });
}

describe("BottingDialog — stop controls", () => {
  it("stops the loop, optionally closing the bot clients", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop Botting Mode" }));
    expect(store.stopBottingMode).toHaveBeenCalledWith(false);

    // Fechar clientes é destrutivo: passa pelo confirm.
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Bot Accounts" }));
    await waitFor(() => expect(store.stopBottingMode).toHaveBeenCalledWith(true));
  });
});

/**
 * Fechar cliente é irreversível para quem está jogando: a tela tem que dizer
 * quantos clientes fecham e o que sobrevive, antes de fechar.
 */
describe("BottingDialog — confirma antes de fechar clientes", () => {
  it("Stop + Close diz quantos clientes bot fecha e o que fica aberto", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Bot Accounts" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    const [message, destructive] = confirmMock.mock.calls[0];
    expect(message).toContain("2 bot accounts");
    expect(message).toContain("Player accounts keep their client");
    expect(message).toContain("outside this session are left alone");
    expect(destructive).toBe(true);
    // Recusado: nada fecha.
    expect(store.stopBottingMode).not.toHaveBeenCalled();
  });

  it("Stop Botting Mode (sem fechar) não pergunta nada", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop Botting Mode" }));

    expect(confirmMock).not.toHaveBeenCalled();
    expect(store.stopBottingMode).toHaveBeenCalledWith(false);
  });

  it("o lote Close client diz quantos clientes fecha e não fecha se recusado", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Select bots" }));
    await userEvent.click(screen.getByRole("button", { name: "Close client (2)" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("2 bot accounts");
    expect(confirmMock.mock.calls[0][1]).toBe(true);
    expect(store.bottingAccountAction).not.toHaveBeenCalled();
  });

  it("o lote Close + Disconnect diz que as contas saem do ciclo", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });
    promptAnswers.confirm = true;

    await userEvent.click(screen.getByRole("button", { name: "Select bots" }));
    await userEvent.click(screen.getByRole("button", { name: "Close + Disconnect (2)" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("rejoin cycle");
    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledTimes(2));
    expect(store.bottingAccountAction).toHaveBeenCalledWith(1, "closeDisconnect");
    expect(store.bottingAccountAction).toHaveBeenCalledWith(2, "closeDisconnect");
  });

  /**
   * O lote perguntava e a linha nao: mesma acao, cobertura diferente. A divisao
   * certa nao e lote vs. linha, e o que a acao custa — `closeDisconnect` tira a
   * conta do ciclo de rejoin ate alguem reconectar (`botting_action_flags`), e
   * `close` sozinho e transitorio, porque o loop reabre no proximo restart.
   */
  it("a linha Close + Disconnect pergunta antes de tirar a conta do ciclo", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getAllByRole("button", { name: "Close + Disconnect" })[0]);

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("rejoin cycle");
    expect(store.bottingAccountAction).not.toHaveBeenCalled();
  });

  it("a linha Close + Disconnect roda quando a pergunta e aceita", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });
    promptAnswers.confirm = true;

    await userEvent.click(screen.getAllByRole("button", { name: "Close + Disconnect" })[0]);

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledWith(1, "closeDisconnect"));
  });

  it("a linha Close client segue sem pergunta: o loop reabre o cliente", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getAllByRole("button", { name: "Close client" })[0]);

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledWith(1, "close"));
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("os lotes não destrutivos seguem sem pergunta", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Select bots" }));
    await userEvent.click(screen.getByRole("button", { name: "Restart loop (2)" }));

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledTimes(2));
    expect(confirmMock).not.toHaveBeenCalled();
  });
});

describe("BottingDialog — explains the cycle", () => {
  // O ciclo em `src-tauri/src/commands/botting.rs` fecha o cliente da conta
  // (`kill_for_user_graceful_async`) e relanca em seguida a cada intervalo. A
  // tela precisa dizer isso, e dizer que so as contas bot da sessao fecham.
  it("says each cycle closes and reopens the bot client", async () => {
    renderDialog();

    expect(screen.getByText("How each cycle works")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Every rejoin closes that bot account's Roblox client and opens it again, so the account leaves the server and joins back."
      )
    ).toBeInTheDocument();
  });

  it("says only this session's bot accounts are closed", async () => {
    renderDialog();

    expect(
      screen.getByText(
        "Only the bot accounts in this session are closed. Player accounts keep their client, and clients of accounts outside this session are left alone."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Stop + Close Bot Accounts closes those same bot clients; Stop Botting Mode leaves every client open."
      )
    ).toBeInTheDocument();
  });

  it("keeps the explanation in the classic view", async () => {
    renderDialog({ settings: settings(true, { BottingDualPanelDialog: "false" }) });

    await waitFor(() => expect(screen.getByText("How each cycle works")).toBeInTheDocument());
    expect(
      screen.getByText(
        "Every rejoin closes that bot account's Roblox client and opens it again, so the account leaves the server and joins back."
      )
    ).toBeInTheDocument();
  });
});

describe("BottingDialog — timing units", () => {
  const unitLabels = [
    "Rejoin Interval (minutes)",
    "Launch Delay (seconds)",
    "Player Grace (minutes)",
  ];

  it("shows the unit of every timing field in the default view", async () => {
    renderDialog();

    for (const label of unitLabels) {
      expect(screen.getByText(label)).toBeInTheDocument();
      // O campo e um input de texto: sem nome acessivel a unidade nao chega
      // a quem navega por teclado/leitor de tela.
      expect(screen.getByRole("textbox", { name: label })).toBeInTheDocument();
    }
  });

  it("shows the unit of every timing field in the classic view", async () => {
    renderDialog({ settings: settings(true, { BottingDualPanelDialog: "false" }) });

    await waitFor(() => expect(screen.getByText(unitLabels[0])).toBeInTheDocument());
    for (const label of unitLabels) {
      expect(screen.getByText(label)).toBeInTheDocument();
      expect(screen.getByRole("textbox", { name: label })).toBeInTheDocument();
    }
  });

  it("states the accepted range of each timing field", async () => {
    renderDialog();

    // Limites reais: botting.rs `clamp_botting_interval_minutes` (10..480),
    // `clamp_botting_launch_delay_seconds` (5..120) e
    // `resolve_player_grace_minutes` (1..90).
    expect(
      screen.getByText(
        "Rejoin Interval: minutes a bot account stays in the server before its client is closed and reopened (10-480)."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Launch Delay: seconds between two launches, so the accounts do not all start at once (5-120)."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Player Grace: minutes a player account keeps its client after you remove it from Player Accounts, before it joins the cycle (1-90)."
      )
    ).toBeInTheDocument();
  });
});

describe("BottingDialog — long alias chips", () => {
  // Task 10 subiu o alias de 30 para MAX_ALIAS_LENGTH (240) caracteres. Os
  // chips de "Targets" mostram `Alias || Username` sem limite de largura —
  // sem truncar, um alias no teto estoura o layout do diálogo.
  //
  // O mesmo nome tambem aparece no dropdown fechado de "Player Accounts"
  // (fica no DOM, só oculto por opacidade), entao a busca por texto precisa
  // filtrar pelo chip de verdade (`theme-soft`) em vez do primeiro que achar.
  const longAlias = "a".repeat(MAX_ALIAS_LENGTH);
  const longAccount = makeAccount({ UserID: 1, Username: "ann", Alias: longAlias });

  function targetsChip(): HTMLElement {
    const matches = screen.getAllByText(longAlias);
    const chip = matches.find((el) => el.className.includes("theme-soft"));
    if (!chip) throw new Error("Targets chip not found among matches");
    return chip;
  }

  it("truncates a long alias chip in the split view", () => {
    renderDialog({}, [longAccount]);
    const chip = targetsChip();
    expect(chip.className).toContain("truncate");
    expect(chip.className).toMatch(/max-w-\[\d+px\]/);
  });

  it("truncates a long alias chip in the classic view", async () => {
    renderDialog(
      { settings: settings(true, { BottingDualPanelDialog: "false" }) },
      [longAccount]
    );
    await screen.findAllByText(longAlias);
    const chip = targetsChip();
    expect(chip.className).toContain("truncate");
    expect(chip.className).toMatch(/max-w-\[\d+px\]/);
  });

  it("keeps the full name reachable via title when the chip is truncated", () => {
    renderDialog({}, [longAccount]);
    expect(targetsChip()).toHaveAttribute("title", longAlias);
  });
});

describe("BottingDialog — draft persistence", () => {
  it("restores the saved draft place/job when it opens", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings"
        ? { General: { BottingDraftPlaceId: "1234", BottingDraftJobId: "saved-job" } }
        : undefined
    );
    renderDialog();

    await waitFor(() => expect(placeIdField()).toHaveValue("1234"));
    expect(screen.getByPlaceholderText("Job ID (optional)")).toHaveValue("saved-job");
  });

  it("persists the place id on blur", async () => {
    const saved: Array<Record<string, unknown>> = [];
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "update_setting") saved.push(args as Record<string, unknown>);
      return undefined;
    });
    renderDialog();

    await userEvent.type(placeIdField(), "606849621");
    await userEvent.tab();

    await waitFor(() =>
      expect(saved).toContainEqual({
        section: "General",
        key: "BottingDraftPlaceId",
        value: "606849621",
      })
    );
  });

  /**
   * Clique direito num jogo → "Botting Mode" abre esta tela para **aquele**
   * jogo. O rascunho salvo vence a store, então o place escolhido tem que vir
   * explícito na abertura, senão o usuário escolhe um jogo e vê outro.
   */
  it("o jogo escolhido na abertura vence o rascunho salvo", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings"
        ? { General: { BottingDraftPlaceId: "1234", BottingDraftJobId: "saved-job" } }
        : undefined
    );
    renderDialog({}, [A, B], "606849621");

    await waitFor(() => expect(placeIdField()).toHaveValue("606849621"));
    // O resto do rascunho continua valendo: só o jogo foi escolhido de fora.
    expect(screen.getByPlaceholderText("Job ID (optional)")).toHaveValue("saved-job");
  });

  it("sem jogo na abertura, o rascunho continua mandando", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlaceId: "1234" } } : undefined
    );
    renderDialog({}, [A, B], null);

    await waitFor(() => expect(placeIdField()).toHaveValue("1234"));
  });
});

