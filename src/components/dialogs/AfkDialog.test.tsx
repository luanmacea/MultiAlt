import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { AfkDialog, formatAfkElapsed } from "./AfkDialog";
import type { AfkStatus, StoreValue } from "../../store";
import { defaultSettings, makeAccount, setStore, storeRef } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks } from "../../test-utils/tauriMocks";
import i18n from "../../i18n";

const ACCOUNTS = [
  makeAccount({ UserID: 11, Username: "alpha" }),
  makeAccount({ UserID: 22, Username: "bravo" }),
];

/** A lista fechada que o backend entrega (`get_afk_keys`). */
const KEYS = ["Space", "W", "A", "S", "D", "E", "F", "R", "Q", "1", "2", "3", "4", "5"];

function makeAfkAccount(overrides: Partial<AfkStatus["accounts"][number]> = {}) {
  return {
    userId: 11,
    lastSendAtMs: 1_000,
    nextSendAtMs: 601_000,
    sends: 0,
    lastError: null,
    lastErrorCode: null,
    ...overrides,
  };
}

function makeAfkStatus(overrides: Partial<AfkStatus> = {}): AfkStatus {
  return {
    active: false,
    startedAtMs: null,
    intervalMinutes: 10,
    key: "",
    accounts: [],
    ...overrides,
  };
}

function renderDialog(overrides: Partial<StoreValue> = {}) {
  const store = setStore({
    accounts: ACCOUNTS,
    launchedByProgram: new Set([11, 22]),
    afkKeys: KEYS,
    afkStatus: makeAfkStatus(),
    ...overrides,
  });
  const onClose = vi.fn();
  render(<AfkDialog open onClose={onClose} />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
});

afterEach(cleanup);

/**
 * `SendInput` entrega na janela em **primeiro plano**, então o ciclo traz a
 * janela de cada conta para frente, uma depois da outra, e só devolve o foco
 * depois da última (`run_afk_cycle_blocking`: 150 ms de folga + 40 ms de tecla +
 * 250 ms entre contas, ~0,44 s por conta). Isso tira o foco de quem está usando
 * o PC, e é a primeira coisa que a tela tem de dizer — com os números de
 * verdade: "meio segundo e depois devolve" só valia com uma conta no modo.
 */
describe("AfkDialog — o preço do envio está na tela", () => {
  it("diz que o foco só volta depois da última conta do ciclo, e quanto tempo isso leva", () => {
    renderDialog();
    const aviso = screen.getByText(/takes the focus away from the window you are using/i);
    expect(aviso.textContent).toMatch(/about half a second each/i);
    expect(aviso.textContent).toMatch(/gives the focus back only after the last one/i);
    expect(aviso.textContent).toMatch(/about 4 seconds with 10 accounts/i);
  });

  it("avisa que, nesse meio-tempo, o que você digitar vai para a janela do Roblox", () => {
    renderDialog();
    expect(screen.getByText(/what you type goes to the Roblox window/i)).toBeInTheDocument();
    // A tecla do AFK só sai com a janela certa na frente (`afk_window_is_ready`):
    // "a tecla pode cair na janela errada" apontava o risco que o ciclo já elimina.
    expect(screen.queryByText(/the key can land in the wrong window/i)).not.toBeInTheDocument();
  });
});

/**
 * A lista de teclas é **fechada** e vem do backend (`get_afk_keys`): a tela não
 * pode oferecer tecla que o backend recusa, nem deixar digitar tecla arbitrária.
 */
describe("AfkDialog — só as teclas da lista", () => {
  it("oferece exatamente as teclas que o backend entregou", async () => {
    renderDialog();
    await userEvent.click(screen.getByLabelText("Key to send"));

    for (const key of KEYS) {
      expect(screen.getByRole("button", { name: key })).toBeInTheDocument();
    }
    // Teclas que fazem outra coisa no jogo não aparecem.
    for (const outside of ["Enter", "Tab", "Escape", "F4"]) {
      expect(screen.queryByRole("button", { name: outside })).not.toBeInTheDocument();
    }
  });

  it("não tem campo de texto para digitar uma tecla qualquer", () => {
    renderDialog();
    // A tecla se escolhe numa lista (botão que abre as opções); campo de texto
    // para tecla não existe, senão a lista fechada não seria fechada.
    expect(screen.queryByRole("textbox", { name: /key/i })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Key to send").tagName).toBe("BUTTON");
  });
});

/**
 * Sem tecla escolhida o modo não liga: inventar uma tecla padrão mexeria no
 * personagem sem o usuário ter pedido.
 */
describe("AfkDialog — o que impede o start", () => {
  it("não liga sem tecla escolhida", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));

    const start = screen.getByRole("button", { name: /Start AFK Mode/i });
    expect(start).toBeDisabled();
    await userEvent.click(start);
    expect(store.startAfkMode).not.toHaveBeenCalled();
  });

  it("não liga sem conta no modo, mesmo com tecla escolhida", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByLabelText("Key to send"));
    await userEvent.click(screen.getByRole("button", { name: "Space" }));

    expect(screen.getByRole("button", { name: /Start AFK Mode/i })).toBeDisabled();
    expect(store.startAfkMode).not.toHaveBeenCalled();
  });

  it("liga com uma tecla da lista e a conta escolhida", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByLabelText("Key to send"));
    await userEvent.click(screen.getByRole("button", { name: "Space" }));
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));
    await userEvent.click(screen.getByRole("button", { name: /Start AFK Mode/i }));

    expect(store.startAfkMode).toHaveBeenCalledWith({
      userIds: [11],
      intervalMinutes: 10,
      key: "Space",
    });
  });

  /** Conta que o usuário não marcou não pode entrar no modo por tabela. */
  it("manda só as contas marcadas", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByLabelText("Key to send"));
    await userEvent.click(screen.getByRole("button", { name: "W" }));
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[1].Username }));
    await userEvent.click(screen.getByRole("button", { name: /Start AFK Mode/i }));

    expect(store.startAfkMode).toHaveBeenCalledWith({
      userIds: [22],
      intervalMinutes: 10,
      key: "W",
    });
  });
});

describe("AfkDialog — sessão em andamento", () => {
  const RUNNING = makeAfkStatus({
    active: true,
    startedAtMs: 1_000,
    key: "Space",
    intervalMinutes: 10,
    accounts: [makeAfkAccount({ sends: 2 })],
  });

  it("mostra o botão de parar e não o de ligar", () => {
    renderDialog({ afkStatus: RUNNING });
    expect(screen.getByRole("button", { name: /Stop AFK Mode/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Start AFK Mode/i })).not.toBeInTheDocument();
  });

  it("parar é o comando de parar, e nada mais", async () => {
    const { store } = renderDialog({ afkStatus: RUNNING });
    await userEvent.click(screen.getByRole("button", { name: /Stop AFK Mode/i }));

    expect(store.stopAfkMode).toHaveBeenCalledTimes(1);
    // Parar o AFK mode não fecha nem reinicia cliente de conta nenhuma.
    expect(store.closeRobloxClients).not.toHaveBeenCalled();
    expect(store.killAllRobloxProcesses).not.toHaveBeenCalled();
    expect(store.restartRobloxClients).not.toHaveBeenCalled();
  });

  it("tirar uma conta do modo em andamento vai pelo set_afk_accounts", async () => {
    const { store } = renderDialog({ afkStatus: RUNNING });
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));

    expect(store.setAfkAccounts).toHaveBeenCalledWith([]);
    expect(store.closeRobloxClients).not.toHaveBeenCalled();
  });

  it("acrescentar uma conta ao modo em andamento mantém quem já estava", async () => {
    const { store } = renderDialog({ afkStatus: RUNNING });
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[1].Username }));

    expect(store.setAfkAccounts).toHaveBeenCalledWith([11, 22]);
  });

  it("com sessão ativa o intervalo e a tecla ficam travados", () => {
    renderDialog({ afkStatus: RUNNING });
    expect(screen.getByLabelText("Key to send")).toBeDisabled();
    expect(screen.getByLabelText("Send every")).toBeDisabled();
  });
});

/**
 * Intervalo e tecla só mudam com o modo parado, então "parar → mudar → ligar"
 * é o caminho normal. Parar não pode esquecer quem estava no modo: a seleção
 * voltava à de antes do start, e uma conta acrescentada com a sessão ligada
 * ficava de fora do próximo start sem aviso — e podia cair por inatividade.
 */
describe("AfkDialog — parar não esquece quem estava no modo", () => {
  const INI_WITH_KEY = { ...defaultSettings(), Afk: { IntervalMinutes: "10", Key: "Space" } };

  function runningWith(userIds: number[]): AfkStatus {
    return makeAfkStatus({
      active: true,
      startedAtMs: 1_000,
      key: "Space",
      accounts: userIds.map((userId) => makeAfkAccount({ userId })),
    });
  }

  /** O backend: `set_afk_accounts` devolve a sessão nova; `stop_afk_mode` a encerra. */
  function backendAnswers(store: StoreValue) {
    vi.mocked(store.setAfkAccounts).mockImplementation(async (userIds: number[]) => {
      storeRef.current = {
        ...storeRef.current,
        afkStatus: userIds.length > 0 ? runningWith(userIds) : makeAfkStatus(),
      };
    });
    vi.mocked(store.stopAfkMode).mockImplementation(async () => {
      storeRef.current = { ...storeRef.current, afkStatus: makeAfkStatus() };
    });
  }

  const row = (name: string) => screen.getByRole("button", { name });

  it("a conta que entrou com a sessão ligada continua marcada depois de parar, e religar a leva junto", async () => {
    const { store } = renderDialog({ afkStatus: runningWith([11]), settings: INI_WITH_KEY });
    backendAnswers(store);

    await userEvent.click(row(ACCOUNTS[1].Username));
    expect(store.setAfkAccounts).toHaveBeenCalledWith([11, 22]);
    await userEvent.click(screen.getByRole("button", { name: /Stop AFK Mode/i }));

    const start = await screen.findByRole("button", { name: /Start AFK Mode/i });
    expect(row(ACCOUNTS[0].Username)).toHaveAttribute("aria-pressed", "true");
    expect(row(ACCOUNTS[1].Username)).toHaveAttribute("aria-pressed", "true");

    await userEvent.click(start);
    expect(store.startAfkMode).toHaveBeenCalledWith({
      userIds: [11, 22],
      intervalMinutes: 10,
      key: "Space",
    });
  });

  it("com a tela aberta numa sessão que já rodava, parar deixa marcadas as contas dela", async () => {
    const { store } = renderDialog({ afkStatus: runningWith([11, 22]), settings: INI_WITH_KEY });
    backendAnswers(store);

    await userEvent.click(screen.getByRole("button", { name: /Stop AFK Mode/i }));

    expect(await screen.findByRole("button", { name: /Start AFK Mode/i })).toBeEnabled();
    expect(row(ACCOUNTS[0].Username)).toHaveAttribute("aria-pressed", "true");
    expect(row(ACCOUNTS[1].Username)).toHaveAttribute("aria-pressed", "true");
  });

  it("desmarcar a última conta desliga o modo, e ela fica desmarcada — foi o que o usuário pediu", async () => {
    const { store } = renderDialog({ afkStatus: runningWith([11]), settings: INI_WITH_KEY });
    backendAnswers(store);

    await userEvent.click(row(ACCOUNTS[0].Username));
    expect(store.setAfkAccounts).toHaveBeenCalledWith([]);

    expect(await screen.findByRole("button", { name: /Start AFK Mode/i })).toBeDisabled();
    expect(row(ACCOUNTS[0].Username)).toHaveAttribute("aria-pressed", "false");
  });
});

/**
 * "Isso está funcionando?" é a pergunta de quem acabou de ligar o modo com
 * intervalo de 10 minutos. Duas respostas na tela: o tempo decorrido da sessão e
 * o botão de enviar agora.
 */
describe("AfkDialog — dá para saber que está funcionando", () => {
  const RUNNING = makeAfkStatus({
    active: true,
    startedAtMs: 1_000,
    key: "Space",
    accounts: [makeAfkAccount()],
  });

  it("formata o tempo decorrido em minutos e horas", () => {
    expect(formatAfkElapsed(null, 10_000)).toBe("--");
    expect(formatAfkElapsed(1_000, 1_000)).toBe("<1m");
    expect(formatAfkElapsed(0, 59_999)).toBe("<1m");
    expect(formatAfkElapsed(0, 60_000)).toBe("1m");
    expect(formatAfkElapsed(0, 12 * 60_000)).toBe("12m");
    expect(formatAfkElapsed(0, 65 * 60_000)).toBe("1h 5m");
    // Relógio para trás não vira tempo negativo.
    expect(formatAfkElapsed(10_000, 0)).toBe("<1m");
  });

  it("mostra há quanto tempo a sessão está rodando", () => {
    vi.setSystemTime(new Date(1_000 + 12 * 60_000));
    try {
      renderDialog({ afkStatus: RUNNING });
      expect(screen.getByText(/Running for 12m/)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("enviar agora manda a tecla escolhida para as contas do modo", async () => {
    const { store } = renderDialog({ afkStatus: RUNNING });
    await userEvent.click(screen.getByRole("button", { name: /Send the key now/i }));

    expect(store.afkTriggerNow).toHaveBeenCalledWith([11], "Space");
  });

  it("não deixa enviar agora sem tecla escolhida", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));

    const sendNow = screen.getByRole("button", { name: /Send the key now/i });
    expect(sendNow).toBeDisabled();
    await userEvent.click(sendNow);
    expect(store.afkTriggerNow).not.toHaveBeenCalled();
  });

  /** O som explica o piscar de foco; ligado por quem quer, nunca por padrão. */
  it("o aviso sonoro nasce desligado e grava a escolha no INI", async () => {
    renderDialog();
    const toggle = screen.getByRole("button", { name: "Beep when a cycle finishes" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    await userEvent.click(toggle);
    expect(invokeMock).toHaveBeenCalledWith("update_setting", {
      section: "Afk",
      key: "BeepOnCycle",
      value: "true",
    });
  });
});


/**
 * O `SendInput` só alcança a janela em primeiro plano, e o Windows **recusa**
 * trazer janela para frente a pedido de processo que está em segundo plano — que
 * é o caso normal do AFK mode. Quando isso acontece o ciclo não manda nada, e a
 * tela tem de dizer as duas coisas: que não mandou, e por quê.
 */
describe("AfkDialog — quando o Windows não deixa a janela vir para frente", () => {
  const DENIED = makeAfkStatus({
    active: true,
    startedAtMs: 1_000,
    key: "Space",
    accounts: [
      makeAfkAccount({
        userId: 11,
        lastError: "Windows did not bring this account's Roblox window to the front, so nothing was sent",
        lastErrorCode: "focusDenied",
      }),
    ],
  });

  it("avisa, na configuração, que nada é enviado nesse caso", () => {
    renderDialog();
    expect(screen.getByText(/nothing is sent/i)).toBeInTheDocument();
  });

  it("diz qual conta foi pulada e por quê", () => {
    renderDialog({ afkStatus: DENIED });
    const aviso = screen.getByText(/did not let this account's window come to the front/i);
    expect(aviso).toBeInTheDocument();
    expect(aviso.textContent).toContain(ACCOUNTS[0].Username);
  });

  it("marca a linha da conta como não enviada", () => {
    renderDialog({ afkStatus: DENIED });
    expect(screen.getByText("not sent")).toBeInTheDocument();
  });

  it("explica que o envio manual passa porque o app acabou de receber o clique", () => {
    renderDialog({ afkStatus: DENIED });
    expect(screen.getByRole("button", { name: /Send the key now/i })).toBeEnabled();
    const dica = screen.getByText(/works because you just clicked/i);
    expect(dica.textContent).toMatch(/Send the key now/);
  });

  it("uma conta sem cliente aberto aparece com o motivo dela, não com o do foco", () => {
    renderDialog({
      afkStatus: makeAfkStatus({
        active: true,
        startedAtMs: 1_000,
        key: "Space",
        accounts: [
          makeAfkAccount({
            lastError: "No Roblox window for this account",
            lastErrorCode: "noWindow",
          }),
        ],
      }),
    });
    expect(screen.getByText(/has no Roblox client open/i)).toBeInTheDocument();
    expect(
      screen.queryByText(/did not let this account's window come to the front/i)
    ).not.toBeInTheDocument();
  });
});

/**
 * Envio manual é uma ação do usuário, mas continua valendo a regra de nunca
 * mexer em cliente de conta que não está no modo: sem sessão, não há a quem
 * enviar.
 */
describe("AfkDialog — envio manual exige sessão", () => {
  it("com o modo desligado, enviar agora fica indisponível mesmo com tecla e conta escolhidas", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByLabelText("Key to send"));
    await userEvent.click(screen.getByRole("button", { name: "Space" }));
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));

    const sendNow = screen.getByRole("button", { name: /Send the key now/i });
    expect(sendNow).toBeDisabled();
    await userEvent.click(sendNow);
    expect(store.afkTriggerNow).not.toHaveBeenCalled();
  });
});

/**
 * O AFK mode só alcança cliente que **este app** abriu (é o tracker que liga
 * conta a PID). Conta sem cliente aberto não tem o que receber tecla.
 */
describe("AfkDialog — contas que podem entrar no modo", () => {
  it("lista as contas com cliente aberto", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: ACCOUNTS[0].Username })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: ACCOUNTS[1].Username })).toBeInTheDocument();
  });

  it("não lista conta sem cliente aberto", () => {
    renderDialog({ launchedByProgram: new Set([11]) });
    expect(screen.getByRole("button", { name: ACCOUNTS[0].Username })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: ACCOUNTS[1].Username })).not.toBeInTheDocument();
  });

  it("explica o vazio em vez de mostrar uma lista vazia", () => {
    renderDialog({ launchedByProgram: new Set<number>() });
    expect(screen.getByText(/Open an account first/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start AFK Mode/i })).toBeDisabled();
  });
});

/**
 * Os detalhes baixos do checkup: nenhum quebra o modo, mas cada um faz a tela
 * dizer uma coisa que não é verdade — relógio acima do intervalo, "Enviando"
 * com nada saindo, "1 contas", o modo desligando calado, "Chave" onde é tecla.
 */
describe("AfkDialog — a tela diz a coisa certa", () => {
  const ONE_RUNNING = makeAfkStatus({
    active: true,
    startedAtMs: 1_000,
    key: "Space",
    accounts: [makeAfkAccount({ userId: 11 })],
  });

  /**
   * O prazo chega do backend (agora + intervalo) e era comparado com o relógio
   * da tela do último tique de 1 s — até 1 s atrás. A contagem nascia em
   * "10:01", mais que o intervalo, o que não existe; e de novo depois de cada
   * envio manual.
   */
  it("a contagem nunca começa maior que o intervalo", () => {
    vi.useFakeTimers({ now: 10_000_000 });
    try {
      const onClose = vi.fn();
      setStore({ accounts: ACCOUNTS, launchedByProgram: new Set([11, 22]), afkKeys: KEYS, afkStatus: makeAfkStatus() });
      const view = render(<AfkDialog open onClose={onClose} />);

      // 900 ms depois do último tique da tela, o start volta com o prazo do
      // primeiro envio.
      act(() => {
        vi.advanceTimersByTime(900);
      });
      const startedAt = Date.now();
      storeRef.current = {
        ...storeRef.current,
        afkStatus: makeAfkStatus({
          active: true,
          startedAtMs: startedAt,
          key: "Space",
          intervalMinutes: 10,
          accounts: [
            makeAfkAccount({ userId: 11, lastSendAtMs: startedAt, nextSendAtMs: startedAt + 10 * 60_000 }),
          ],
        }),
      };
      view.rerender(<AfkDialog open onClose={onClose} />);

      const linha = screen.getByRole("button", { name: ACCOUNTS[0].Username });
      expect(linha.textContent).toContain("10:00");
      expect(linha.textContent).not.toContain("10:01");
    } finally {
      vi.useRealTimers();
    }
  });

  /** "Enviando" com a sessão ligada mentia entre um ciclo e outro, e com o foco negado em todas. */
  it("com a sessão ligada a pílula diz o estado, 'On', e não 'Sending'", () => {
    renderDialog({ afkStatus: ONE_RUNNING });
    expect(screen.getByText("On")).toBeInTheDocument();
    expect(screen.queryByText("Sending")).not.toBeInTheDocument();
  });

  it("desmarcar a última conta avisa que o modo desligou", async () => {
    const { store } = renderDialog({ afkStatus: ONE_RUNNING });
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));

    expect(store.setAfkAccounts).toHaveBeenCalledWith([]);
    expect(store.addToast).toHaveBeenCalledWith("AFK mode off: no account is left in it");
  });

  it("tirar uma conta que não é a última não avisa nada", async () => {
    const { store } = renderDialog({
      afkStatus: makeAfkStatus({
        active: true,
        startedAtMs: 1_000,
        key: "Space",
        accounts: [makeAfkAccount({ userId: 11 }), makeAfkAccount({ userId: 22 })],
      }),
    });
    await userEvent.click(screen.getByRole("button", { name: ACCOUNTS[0].Username }));

    expect(store.setAfkAccounts).toHaveBeenCalledWith([22]);
    expect(store.addToast).not.toHaveBeenCalled();
  });

  it("enviar para uma conta só diz 'account', no singular", async () => {
    const { store } = renderDialog({ afkStatus: ONE_RUNNING });
    vi.mocked(store.afkTriggerNow).mockResolvedValue(1);
    await userEvent.click(screen.getByRole("button", { name: /Send the key now/i }));

    expect(store.addToast).toHaveBeenCalledWith("Sent Space to 1 account");
  });

  it("enviar para duas contas continua no plural", async () => {
    const { store } = renderDialog({
      afkStatus: makeAfkStatus({
        active: true,
        startedAtMs: 1_000,
        key: "Space",
        accounts: [makeAfkAccount({ userId: 11 }), makeAfkAccount({ userId: 22 })],
      }),
    });
    vi.mocked(store.afkTriggerNow).mockResolvedValue(2);
    await userEvent.click(screen.getByRole("button", { name: /Send the key now/i }));

    expect(store.addToast).toHaveBeenCalledWith("Sent Space to 2 accounts");
  });

  describe("em português", () => {
    beforeEach(async () => {
      await i18n.changeLanguage("pt");
    });
    afterEach(async () => {
      await i18n.changeLanguage("en");
    });

    /**
     * "Key" → "Chave" está certo na tela de campos da conta, que divide a mesma
     * chave do catálogo; aqui é tecla. E o campo do intervalo tinha nome
     * acessível em inglês, porque ia cru para o `NumericInput`.
     */
    it("o rótulo é 'Tecla a enviar', não 'Chave', e o campo do intervalo tem nome em português", () => {
      renderDialog();
      expect(screen.getByText("Tecla a enviar")).toBeInTheDocument();
      expect(screen.queryByText("Chave")).not.toBeInTheDocument();
      expect(screen.getByLabelText("Tecla a enviar").tagName).toBe("BUTTON");
      expect(screen.getByLabelText("Enviar a cada").tagName).toBe("INPUT");
    });

    it("a pílula diz 'Ligado' com a sessão ligada", () => {
      renderDialog({ afkStatus: ONE_RUNNING });
      expect(screen.getByText("Ligado")).toBeInTheDocument();
      expect(screen.queryByText("Enviando")).not.toBeInTheDocument();
    });
  });
});
