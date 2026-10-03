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
import { MAX_ALIAS_LENGTH } from "../../types";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });

/** Auto Rejoin refuses to start unless Multi Roblox is on. */
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
  render(<RejoinTab initialPlaceId={initialPlaceId} />);
  return { store };
}

const startButton = () => screen.getByRole("button", { name: "Start Auto Rejoin" });
const placeIdField = () => screen.getByPlaceholderText("Place ID");

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  // The dialog re-reads settings from the backend when it opens.
  setInvokeHandler((cmd) => (cmd === "get_all_settings" ? {} : undefined));
});

afterEach(cleanup);

describe("RejoinTab — start guards", () => {
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
      screen.getByText("Auto Rejoin currently requires Multi Roblox to be enabled")
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

describe("RejoinTab — stop controls", () => {
  it("stops the loop, optionally closing the bot clients", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop Auto Rejoin" }));
    expect(store.stopBottingMode).toHaveBeenCalledWith(false);

    // Fechar clientes é destrutivo: passa pelo confirm.
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Alt Accounts" }));
    await waitFor(() => expect(store.stopBottingMode).toHaveBeenCalledWith(true));
  });
});

/**
 * Fechar cliente é irreversível para quem está jogando: a tela tem que dizer
 * quantos clientes fecham e o que sobrevive, antes de fechar.
 */
describe("RejoinTab — confirma antes de fechar clientes", () => {
  it("Stop + Close diz quantos clientes bot fecha e o que fica aberto", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop + Close Alt Accounts" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    const [message, destructive] = confirmMock.mock.calls[0];
    expect(message).toContain("2 alt accounts");
    expect(message).toContain("Main accounts keep their client");
    expect(message).toContain("outside this session are left alone");
    expect(destructive).toBe(true);
    // Recusado: nada fecha.
    expect(store.stopBottingMode).not.toHaveBeenCalled();
  });

  it("Stop Botting Mode (sem fechar) não pergunta nada", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Stop Auto Rejoin" }));

    expect(confirmMock).not.toHaveBeenCalled();
    expect(store.stopBottingMode).toHaveBeenCalledWith(false);
  });

  it("o lote Close client diz quantos clientes fecha e não fecha se recusado", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });

    await userEvent.click(screen.getByRole("button", { name: "Select alts" }));
    await userEvent.click(screen.getByRole("button", { name: "Close client (2)" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toContain("2 alt accounts");
    expect(confirmMock.mock.calls[0][1]).toBe(true);
    expect(store.bottingAccountAction).not.toHaveBeenCalled();
  });

  it("o lote Close + Disconnect diz que as contas saem do ciclo", async () => {
    const { store } = renderDialog({ bottingStatus: activeSession() });
    promptAnswers.confirm = true;

    await userEvent.click(screen.getByRole("button", { name: "Select alts" }));
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

    await userEvent.click(screen.getByRole("button", { name: "Select alts" }));
    await userEvent.click(screen.getByRole("button", { name: "Restart loop (2)" }));

    await waitFor(() => expect(store.bottingAccountAction).toHaveBeenCalledTimes(2));
    expect(confirmMock).not.toHaveBeenCalled();
  });
});

describe("RejoinTab — explains the cycle", () => {
  // O ciclo em `src-tauri/src/commands/botting.rs` fecha o cliente da conta
  // (`kill_for_user_graceful_async`) e relanca em seguida a cada intervalo. A
  // tela precisa dizer isso, e dizer que so as contas bot da sessao fecham.
  it("says each cycle closes and reopens the bot client", async () => {
    renderDialog();

    expect(screen.getByText("How each cycle works")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Every rejoin closes that alt account's Roblox client and opens it again, so the account leaves the server and joins back."
      )
    ).toBeInTheDocument();
  });

  it("says only this session's bot accounts are closed", async () => {
    renderDialog();

    expect(
      screen.getByText(
        "Only the alt accounts in this session are closed. Main accounts keep their client, and clients of accounts outside this session are left alone."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Stop + Close Alt Accounts closes those same alt clients; Stop Auto Rejoin leaves every client open."
      )
    ).toBeInTheDocument();
  });

  it("keeps the explanation in the classic view", async () => {
    renderDialog({ settings: settings(true, { BottingDualPanelDialog: "false" }) });

    await waitFor(() => expect(screen.getByText("How each cycle works")).toBeInTheDocument());
    expect(
      screen.getByText(
        "Every rejoin closes that alt account's Roblox client and opens it again, so the account leaves the server and joins back."
      )
    ).toBeInTheDocument();
  });
});

describe("RejoinTab — timing units", () => {
  const unitLabels = [
    "Rejoin Interval (minutes)",
    "Launch Delay (seconds)",
    "Main Grace (minutes)",
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
        "Rejoin Interval: minutes an alt account stays in the server before its client is closed and reopened (10-480)."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Launch Delay: seconds between two launches, so the accounts do not all start at once (5-120)."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Main Grace: minutes a main account keeps its client after you remove it from Main Accounts, before it joins the cycle (1-90)."
      )
    ).toBeInTheDocument();
  });
});

describe("RejoinTab — long alias chips", () => {
  // Task 10 subiu o alias de 30 para MAX_ALIAS_LENGTH (240) caracteres. Os
  // chips de "Targets" mostram `Alias || Username` sem limite de largura —
  // sem truncar, um alias no teto estoura o layout do diálogo.
  //
  // O mesmo nome tambem aparece no dropdown fechado de "Main Accounts"
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

/**
 * O chip de Targets já mostrava o nome inteiro no `title`; os outros lugares do
 * diálogo que cortam o nome não. Medido no harness a 1100x700: na lista ao vivo
 * da New View o nome tinha 556 de 1765 px, no menu Main Accounts 192 px, no
 * botão Main Accounts 240 px, e no Live Cycle da Classic uma caixa de **90 px**
 * — que corta até nome comum (`MyFarmAccount01` mede 109 px), e as alts
 * numeradas viravam todas "MyFarmAccou…" sem jeito de ler o resto.
 */
describe("RejoinTab — nome cortado tem o nome inteiro no title", () => {
  const longAlias = "a".repeat(MAX_ALIAS_LENGTH);
  const longAccount = makeAccount({ UserID: 1, Username: "ann", Alias: longAlias });

  /** O `title` que o navegador mostra ao passar o mouse: o do ancestral mais próximo. */
  function tituloDe(el: Element): string | null {
    return el.closest("[title]")?.getAttribute("title") ?? null;
  }

  function comMainAccount() {
    // O rascunho salvo traz a conta 1 como Main Account.
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlayerAccountIds: "1" } } : undefined
    );
  }

  it("na lista ao vivo da New View", () => {
    renderDialog({ accounts: [longAccount, B] }, [longAccount, B]);
    const secao = screen.getByText("Live Auto Rejoin List").closest("section") as HTMLElement;
    expect(tituloDe(within(secao).getByText(longAlias))).toBe(longAlias);
  });

  it("nos itens do menu Main Accounts", () => {
    renderDialog({ accounts: [longAccount, B] }, [longAccount, B]);
    const gatilho = document.querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    const menu = gatilho.nextElementSibling as HTMLElement;
    expect(tituloDe(within(menu).getByText(longAlias))).toBe(longAlias);
  });

  it("no botão Main Accounts com uma conta escolhida", async () => {
    comMainAccount();
    renderDialog({ accounts: [longAccount, B] }, [longAccount, B]);
    const botao = document.querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    await waitFor(() => expect(botao).toHaveTextContent(longAlias));
    expect(tituloDe(within(botao).getByText(longAlias))).toBe(longAlias);
  });

  it("no botão Main Accounts com várias contas, o title diz quais são", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlayerAccountIds: "1,2" } } : undefined
    );
    renderDialog({ accounts: [longAccount, B] }, [longAccount, B]);
    const botao = document.querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    await waitFor(() => expect(botao).toHaveTextContent("2 selected"));
    expect(tituloDe(within(botao).getByText("2 selected"))).toBe(`${longAlias}, bob`);
  });

  it("no Live Cycle da visão Classic", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDualPanelDialog: "false" } } : undefined
    );
    renderDialog({ accounts: [longAccount, B] }, [longAccount, B]);
    const titulo = await screen.findByText("Live Cycle");
    const secao = titulo.closest("section") as HTMLElement;
    expect(tituloDe(within(secao).getByText(longAlias))).toBe(longAlias);
  });
});

describe("RejoinTab — New View em janela estreita", () => {
  /**
   * Numa área estreita (o modal numa janela pequena; a decisão é pela largura do
   * contêiner, não da janela) o grid de duas colunas vira duas linhas, e elas
   * dividiam a altura fixa do diálogo: a 900x560 a "Live Auto Rejoin List"
   * ficava com 0 px e o conteúdo (`overflow-hidden`) não rolava — nenhuma ação
   * por conta alcançável, e a 750x450 nem as ações em lote. Medido no harness
   * (relatório da Frente C do checkup).
   *
   * O jsdom não calcula layout, então isto trava a estrutura de que o conserto
   * depende: quem rola é o **conteúdo** do diálogo, e do conteúdo até a lista
   * (e até os controles da coluna esquerda) nada limita a altura fora de `lg:`.
   * A prova de que cabe é a medida no harness, não este teste.
   */
  const LIMITA_ALTURA = ["overflow-hidden", "overflow-y-auto", "min-h-0", "h-full", "flex-1"];

  function conteudoDoDialogo(): HTMLElement {
    const secao = screen.getByText("How each cycle works").closest("section");
    if (!secao?.parentElement) throw new Error("conteúdo do diálogo não encontrado");
    return secao.parentElement;
  }

  /** Classes que limitam a altura sem `lg:`, do elemento até o conteúdo. */
  function limitesForaDoLg(de: HTMLElement): string[] {
    const conteudo = conteudoDoDialogo();
    const achados: string[] = [];
    for (let el: HTMLElement | null = de; el && el !== conteudo; el = el.parentElement) {
      for (const classe of el.className.split(/\s+/)) {
        if (LIMITA_ALTURA.includes(classe)) {
          achados.push(`${classe} em <${el.tagName.toLowerCase()} class="${el.className.slice(0, 48)}">`);
        }
      }
    }
    return achados;
  }

  it("o conteúdo da New View rola em qualquer largura", () => {
    renderDialog();
    const classes = conteudoDoDialogo().className.split(/\s+/);
    expect(classes).toContain("overflow-y-auto");
    expect(classes).not.toContain("overflow-hidden");
  });

  it("do conteúdo até a lista ao vivo, só `lg:` limita a altura", () => {
    renderDialog();
    const secao = screen.getByText("Live Auto Rejoin List").closest("section");
    const lista = secao?.lastElementChild as HTMLElement | null;
    if (!lista) throw new Error("lista ao vivo não encontrada");
    expect(limitesForaDoLg(lista)).toEqual([]);
  });

  /**
   * Queixa do dono: o Auto Rejoin ligava e ele não achava onde parar. Ligar e
   * parar ficam na barra de estado, **fora** da área que rola — à vista em
   * qualquer altura de janela.
   */
  it("Start e Stop ficam fora da área que rola", () => {
    renderDialog();
    expect(conteudoDoDialogo().contains(startButton())).toBe(false);
    cleanup();
    renderDialog({ bottingStatus: activeSession() });
    const stop = screen.getByRole("button", { name: "Stop Auto Rejoin" });
    expect(conteudoDoDialogo().contains(stop)).toBe(false);
  });
});

describe("RejoinTab — draft persistence", () => {
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
   * Clique direito num jogo → "Auto Rejoin" abre esta tela para **aquele**
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


describe("RejoinTab — barra de estado", () => {
  it("diz que o Auto Rejoin está parado, e o que falta para ligar", () => {
    renderDialog();
    const bar = screen.getByTestId("rejoin-status");
    expect(bar).toHaveTextContent("Auto Rejoin is stopped");
    expect(bar).toHaveTextContent("Place ID is required");
    expect(within(bar).getByRole("button", { name: "Start Auto Rejoin" })).toBeInTheDocument();
  });

  /**
   * Aberto pela barra com o ciclo rodando e nada selecionado na lista, a tela
   * dizia "nenhuma conta" ao lado da lista ao vivo, e o menu de mains ficava
   * vazio — não dava para trocar a main com o ciclo ligado.
   */
  it("sem seleção e com o ciclo rodando, os alvos são as contas do ciclo", () => {
    renderDialog({ bottingStatus: activeSession() }, []);
    const card = screen.getByText("Targets").closest("section") as HTMLElement;
    expect(Array.from(card.querySelectorAll("span.theme-soft")).map((el) => el.textContent)).toEqual([
      "ann",
      "bob",
    ]);
  });

  it("diz que está rodando, e só oferece parar", () => {
    renderDialog({ bottingStatus: activeSession() });
    const bar = screen.getByTestId("rejoin-status");
    expect(bar).toHaveTextContent("Auto Rejoin is running");
    expect(bar).toHaveTextContent("2 accounts in the cycle");
    expect(within(bar).queryByRole("button", { name: "Start Auto Rejoin" })).not.toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "Stop Auto Rejoin" })).toBeInTheDocument();
  });
});

/**
 * Aberto pelo "Em jogo" do Painel de Sessão: as contas são as que estão em
 * jogo (não a seleção da lista), o place vem do jogo em que elas estão, e o
 * Start **adota** os clientes abertos — nada fecha, nada relança. Antes o botão
 * ligava direto, sem mostrar o tempo do ciclo nem as contas main.
 */
describe("RejoinTab — adotando as contas em jogo", () => {
  const C = makeAccount({ UserID: 3, Username: "cid" });

  function renderAdopt(overrides: Partial<StoreValue> = {}, targets = [1, 3]) {
    const store = setStore({
      accounts: [A, B, C],
      // A seleção da lista é outra: quem manda são as contas do Em jogo.
      selectedIds: new Set([2]),
      selectedAccounts: [B],
      settings: settings(true),
      detectRunningGamePlace: vi.fn(async () => 606849621),
      ...overrides,
    });
    render(<RejoinTab targetUserIds={targets} adoptRunning />);
    return { store };
  }

  function targetChips(): string[] {
    const titulo = screen.getByText("Targets");
    const card = titulo.closest("section") as HTMLElement;
    return Array.from(card.querySelectorAll("span.theme-soft")).map((el) => el.textContent ?? "");
  }

  it("os alvos são as contas em jogo, não a seleção da lista", async () => {
    renderAdopt();
    await waitFor(() => expect(targetChips()).toEqual(["ann", "cid"]));
  });

  it("o place vem do jogo em que as contas estão, não do rascunho", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlaceId: "1234" } } : undefined
    );
    const { store } = renderAdopt();

    await waitFor(() => expect(placeIdField()).toHaveValue("606849621"));
    expect(store.detectRunningGamePlace).toHaveBeenCalledWith([1, 3]);
    // Job e JoinData não entram no ciclo adotado.
    expect(screen.queryByPlaceholderText("Job ID (optional)")).not.toBeInTheDocument();
  });

  it("sem saber o jogo, deixa o campo vazio e pede o place em vez de usar o rascunho", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlaceId: "1234" } } : undefined
    );
    renderAdopt({ detectRunningGamePlace: vi.fn(async () => null) });

    expect(
      await screen.findByText(/Could not tell which game these accounts are in/)
    ).toBeInTheDocument();
    expect(placeIdField()).toHaveValue("");
    expect(startButton()).toBeDisabled();
  });

  it("o Start adota com o tempo e as mains escolhidos, sem relançar ninguém", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings"
        ? { General: { BottingDefaultIntervalMinutes: "25", BottingDraftPlayerAccountIds: "1" } }
        : undefined
    );
    const { store } = renderAdopt();
    await waitFor(() => expect(placeIdField()).toHaveValue("606849621"));
    await waitFor(() => expect(startButton()).toBeEnabled());

    await userEvent.click(startButton());

    await waitFor(() =>
      expect(store.adoptRunningIntoBotting).toHaveBeenCalledWith(
        [1, 3],
        expect.objectContaining({ placeId: 606849621, intervalMinutes: 25, playerUserIds: [1] })
      )
    );
    expect(store.startBottingMode).not.toHaveBeenCalled();
    expect(store.closeRobloxClients).not.toHaveBeenCalled();
  });

  it("adotando, não grava place nem job no rascunho de quem usa pela lista", async () => {
    const saved: Array<Record<string, unknown>> = [];
    setInvokeHandler((cmd, args) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "update_setting") saved.push(args as Record<string, unknown>);
      return undefined;
    });
    renderAdopt();
    await waitFor(() => expect(placeIdField()).toHaveValue("606849621"));
    await waitFor(() => expect(startButton()).toBeEnabled());
    await userEvent.click(startButton());

    await waitFor(() => expect(saved.length).toBeGreaterThan(0));
    const keys = saved.map((it) => it.key);
    expect(keys).not.toContain("BottingDraftPlaceId");
    expect(keys).not.toContain("BottingDraftJobId");
    expect(keys).toContain("BottingDefaultIntervalMinutes");
  });

  it("uma conta só, sem sessão, não liga", async () => {
    renderAdopt({}, [1]);
    await waitFor(() => expect(placeIdField()).toHaveValue("606849621"));
    expect(startButton()).toBeDisabled();
    expect(screen.getByTestId("rejoin-status")).toHaveTextContent("Select at least 2 accounts");
  });

  it("com sessão ligada, oferece só acrescentar quem ainda não está nela", async () => {
    const { store } = renderAdopt({
      bottingStatus: makeBottingStatus({
        active: true,
        userIds: [1, 2],
        accounts: [botRow({ userId: 1 }), botRow({ userId: 2 })],
      }),
    });

    await userEvent.click(screen.getByRole("button", { name: "Add to Auto Rejoin (1)" }));

    expect(store.adoptRunningIntoBotting).toHaveBeenCalledWith([3]);
    expect(store.startBottingMode).not.toHaveBeenCalled();
  });
});

/**
 * Com "Names hidden" na toolbar, o Auto Rejoin mostrava os nomes reais nos
 * chips de Targets, no menu e no botão de Main Accounts e na lista ao vivo (com
 * a foto da conta) — confirmado no app de verdade.
 */
describe("RejoinTab — nomes ocultos", () => {
  const SA = makeAccount({ UserID: 1, Username: "secretann", Alias: "AliasAnn" });
  const SB = makeAccount({ UserID: 2, Username: "secretbob" });
  const HIDDEN = {
    accounts: [SA, SB],
    hideUsernames: true,
    hiddenNameLetters: 0,
    showAvatarsWhenHidden: false,
    avatarUrls: new Map([
      [1, "https://avatar.test/one.png"],
      [2, "https://avatar.test/two.png"],
    ]),
  };

  function expectNoRealName() {
    const html = document.body.innerHTML;
    for (const leak of ["secretann", "AliasAnn", "secretbob", "avatar.test"]) expect(html).not.toContain(leak);
  }

  it("New View: chips, menu e botão de Main Accounts e lista ao vivo", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlayerAccountIds: "1" } } : undefined
    );
    renderDialog({ ...HIDDEN, bottingStatus: activeSession() }, [SA, SB]);
    const botao = document.querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    await waitFor(() => expect(botao).toHaveTextContent("************"));
    expect(screen.getByText("Live Auto Rejoin List")).toBeInTheDocument();
    expectNoRealName();
  });

  it("o title do botão de Main Accounts com várias contas", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDraftPlayerAccountIds: "1,2" } } : undefined
    );
    renderDialog(HIDDEN, [SA, SB]);
    const botao = document.querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    await waitFor(() => expect(botao).toHaveTextContent("2 selected"));
    expectNoRealName();
  });

  it("Classic: Live Cycle", async () => {
    setInvokeHandler((cmd) =>
      cmd === "get_all_settings" ? { General: { BottingDualPanelDialog: "false" } } : undefined
    );
    renderDialog({ ...HIDDEN, bottingStatus: activeSession() }, [SA, SB]);
    await screen.findByText("Live Cycle");
    expectNoRealName();
  });

  it("com a opção de manter avatares, a foto volta mas o nome não", () => {
    renderDialog({ ...HIDDEN, showAvatarsWhenHidden: true, bottingStatus: activeSession() }, [SA, SB]);
    expect(document.body.innerHTML).toContain("avatar.test/one.png");
    expect(document.body.innerHTML).not.toContain("secretann");
  });
});
