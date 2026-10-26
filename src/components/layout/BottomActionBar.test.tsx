import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { BottomActionBar } from "./BottomActionBar";
import {
  defaultSettings,
  makeAccount,
  makeBottingStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import {
  invokeMock,
  resetTauriMocks,
  setInvokeHandler,
} from "../../test-utils/tauriMocks";
import {
  confirmMock,
  confirmWithOptOutMock,
  promptAnswers,
  resetPromptMocks,
} from "../../test-utils/promptMocks";
import type { StoreValue } from "../../store";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });
const C = makeAccount({ UserID: 3, Username: "cid", Group: "Farm" });

const writeText = vi.fn(async () => {});

function renderBar(selected = [A, B], overrides: Partial<StoreValue> = {}) {
  const store = setStore({
    accounts: [A, B, C],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    ...overrides,
  });
  render(<BottomActionBar />);
  return store;
}

async function openActions() {
  await userEvent.click(screen.getByRole("button", { name: /^Actions/ }));
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  writeText.mockClear();
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
});

afterEach(cleanup);

describe("BottomActionBar — selection-size rules", () => {
  it("names the single selected account and offers the Account button", () => {
    renderBar([A]);
    expect(screen.getByText("ann")).toBeInTheDocument();
    expect(screen.getByText("1 account selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Account$/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose Game" })).toBeInTheDocument();
  });

  it("hides the Account button and counts the selection when several are picked", () => {
    renderBar([A, B]);
    expect(screen.getByText("2 accounts selected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Account$/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose Game (2)" })).toBeInTheDocument();
  });

  it("offers Make Friends only from two accounts up", async () => {
    renderBar([A]);
    await openActions();
    expect(screen.queryByRole("button", { name: /Make Friends/ })).not.toBeInTheDocument();

    cleanup();
    renderBar([A, B]);
    await openActions();
    expect(screen.getByRole("button", { name: /Make Friends \(2\)/ })).toBeInTheDocument();
  });

  it("offers Restart Launched only for accounts this app launched", async () => {
    renderBar([A, B]);
    await openActions();
    expect(screen.queryByRole("button", { name: /Restart Launched/ })).not.toBeInTheDocument();

    cleanup();
    renderBar([A, B], { launchedByProgram: new Set([2]) });
    await openActions();
    expect(screen.getByRole("button", { name: /Restart Launched \(1\)/ })).toBeInTheDocument();
  });

  it("only offers to open the botting dialog once botting is enabled or running", async () => {
    renderBar([A, B]);
    await openActions();
    expect(screen.queryByRole("button", { name: /Open Auto Rejoin/ })).not.toBeInTheDocument();

    cleanup();
    const settings = defaultSettings();
    settings.General.BottingEnabled = "true";
    renderBar([A, B], { settings });
    await openActions();
    expect(screen.getByRole("button", { name: /Open Auto Rejoin/ })).toBeInTheDocument();
  });

  it("offers Add to Botting only for accounts not already in the loop", async () => {
    renderBar([A, B], { bottingStatus: makeBottingStatus({ active: true, userIds: [1] }) });
    await openActions();
    expect(screen.getByRole("button", { name: /Add to Auto Rejoin \(1\)/ })).toBeInTheDocument();

    cleanup();
    renderBar([A, B], { bottingStatus: makeBottingStatus({ active: true, userIds: [1, 2] }) });
    await openActions();
    expect(screen.queryByRole("button", { name: /Add to Auto Rejoin/ })).not.toBeInTheDocument();
  });
});

describe("BottomActionBar — actions", () => {
  it("clears the selection", async () => {
    const store = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(store.deselectAll).toHaveBeenCalledTimes(1);
  });

  it("opens the Choose Game screen", async () => {
    const store = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Choose Game (2)" }));
    expect(store.setChooseGameOpen).toHaveBeenCalledWith(true);
  });

  it("toggles the detail sidebar for a single account", async () => {
    const store = renderBar([A], { sidebarOpen: false });
    await userEvent.click(screen.getByRole("button", { name: /Account$/ }));
    expect(store.setSidebarOpen).toHaveBeenCalledWith(true);
  });

  /**
   * Ideia 26: o cookie vai pelo backend, que o marca fora do histórico do
   * Win+V e o apaga em 30 s. O texto sai do store do backend, então a tela só
   * manda os ids — nada de `navigator.clipboard` no Windows.
   */
  it("copies the selected cookies through the backend once the warning is accepted", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    setInvokeHandler((cmd) =>
      cmd === "copy_account_secret" ? { count: 2, clearsInSecs: 30 } : undefined
    );
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_account_secret", { userIds: [1, 2], kind: "cookie" })
    );
    expect(writeText).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(store.setActionReport).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: "copy-cookies",
          summary: "Copied 2 cookies. Cleared from the clipboard in 30 s.",
        })
      )
    );
  });

  it("falls back to the plain clipboard where the protected copy does not exist", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    setInvokeHandler((cmd) => {
      if (cmd === "copy_account_secret") throw "CLIPBOARD_UNSUPPORTED";
      return undefined;
    });
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("cookie-1\ncookie-2"));
    await waitFor(() =>
      expect(store.setActionReport).toHaveBeenCalledWith(
        expect.objectContaining({ kind: "copy-cookies", summary: "Copied 2 cookies" })
      )
    );
  });

  /** Erro do Windows (área de transferência ocupada) não cai no caminho sem proteção. */
  it("does not fall back to the unprotected clipboard on a real error", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    setInvokeHandler((cmd) => {
      if (cmd === "copy_account_secret") throw "The clipboard is busy";
      return undefined;
    });
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("Failed to copy"));
    expect(writeText).not.toHaveBeenCalled();
  });

  it("says in the warning that the copy is cleared in 30 s", async () => {
    renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() => expect(confirmWithOptOutMock).toHaveBeenCalledTimes(1));
    const [message] = confirmWithOptOutMock.mock.calls[0];
    expect(message).toContain("Cleared from the clipboard in 30 s");
    expect(message).toContain("Win+V");
  });

  /**
   * O botão punha o `.ROBLOSECURITY` de toda a seleção na área de transferência
   * num clique, sem dizer o que um cookie entrega nem quantas contas iam junto.
   */
  it("asks before copying every selected cookie, naming the count and what a cookie is", async () => {
    renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() => expect(confirmWithOptOutMock).toHaveBeenCalledTimes(1));
    const [message] = confirmWithOptOutMock.mock.calls[0];
    expect(message).toContain("2 accounts");
    expect(message).toMatch(/2-step verification/);
    expect(writeText).not.toHaveBeenCalled();
  });

  it("remembers the credential-copy opt-out in the settings", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: true };
    renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "General",
        key: "WarnOnCopyCredential",
        value: "false",
      })
    );
  });

  it("skips the warning once it has been turned off", async () => {
    const settings = defaultSettings();
    settings.General.WarnOnCopyCredential = "false";
    renderBar([A, B], { settings });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Copy All Cookies/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_account_secret", { userIds: [1, 2], kind: "cookie" })
    );
    expect(confirmWithOptOutMock).not.toHaveBeenCalled();
  });

  /** Ideia 9: conferir sessão e moderação da seleção, só leitura. */
  it("checks the selected accounts and says it is read-only", async () => {
    const store = renderBar();
    await openActions();
    const button = screen.getByRole("button", { name: /Check Accounts \(2\)/ });
    expect(button).toHaveTextContent(/doesn't sign anything out/);
    await userEvent.click(button);
    expect(store.checkAccounts).toHaveBeenCalledWith([1, 2]);
  });

  it("shows the progress of a running check on the Actions button", () => {
    renderBar([A, B], { accountCheckProgress: { done: 3, total: 20 } });
    expect(screen.getByRole("button", { name: /^Checking 3\/20/ })).toBeInTheDocument();
  });

  it("lists the existing groups and moves the selection into one", async () => {
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Move to Group/ }));
    await userEvent.click(screen.getByRole("button", { name: "Farm" }));
    expect(store.moveToGroup).toHaveBeenCalledWith([1, 2], "Farm", { report: true });
  });

  it("asks for a name before creating a new group", async () => {
    promptAnswers.prompt = "  Bots  ";
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Move to Group/ }));
    await userEvent.click(screen.getByRole("button", { name: /New Group/ }));
    await waitFor(() => expect(store.moveToGroup).toHaveBeenCalledWith([1, 2], "Bots", { report: true }));
  });

  it("does not create a group when the prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Move to Group/ }));
    await userEvent.click(screen.getByRole("button", { name: /New Group/ }));
    await Promise.resolve();
    expect(store.moveToGroup).not.toHaveBeenCalled();
  });

  it("requires typing REMOVE before deleting accounts", async () => {
    promptAnswers.prompt = "nope";
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Remove \(2\)/ }));
    await Promise.resolve();
    expect(store.removeAccounts).not.toHaveBeenCalled();

    cleanup();
    promptAnswers.prompt = "remove";
    const store2 = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Remove \(2\)/ }));
    await waitFor(() => expect(store2.removeAccounts).toHaveBeenCalledWith([1, 2], { report: true }));
  });

  it("closes every Roblox process", async () => {
    const store = renderBar();
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Close All Roblox/ }));
    expect(store.killAllRobloxProcesses).toHaveBeenCalledTimes(1);
  });

  it("opens the botting dialog and warns when botting is not running yet", async () => {
    const settings = defaultSettings();
    settings.General.BottingEnabled = "true";
    const store = renderBar([A, B], { settings, bottingStatus: makeBottingStatus({ active: false }) });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Open Auto Rejoin/ }));
    // Sem argumento de propósito: abrir pela barra não escolhe jogo nenhum, e
    // por isso limpa o jogo de uma abertura anterior vinda de um clique direito.
    expect(store.openBottingDialog).toHaveBeenCalledWith();
  });

  it("adds only the accounts missing from an active botting loop", async () => {
    const store = renderBar([A, B], {
      bottingStatus: makeBottingStatus({ active: true, userIds: [1] }),
    });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Add to Auto Rejoin \(1\)/ }));
    await waitFor(() => expect(store.addBottingAccounts).toHaveBeenCalledWith([2], { report: true }));
  });

  it("restarts only the clients this app launched", async () => {
    const store = renderBar([A, B], { launchedByProgram: new Set([2]) });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Restart Launched \(1\)/ }));
    // A seleção inteira vai: a que não tem cliente aberto pelo app aparece como "pulada".
    await waitFor(() => expect(store.restartRobloxClients).toHaveBeenCalledWith([1, 2], { report: true }));
  });

  /** Relato do dono (25/10/2026): "Refresh cookies" não mostrava o resultado de cada conta. */
  it("refreshes the cookies through the store, which opens the results window", async () => {
    const store = renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Refresh Cookies \(2\)/ }));
    await waitFor(() => expect(store.refreshCookies).toHaveBeenCalledWith([1, 2]));
    expect(store.addToast).not.toHaveBeenCalledWith(expect.stringMatching(/^Refreshed:/));
  });
});

describe("BottomActionBar — friend linking", () => {
  const FOUR = [A, B, C, makeAccount({ UserID: 4, Username: "dee" })];

  it("sends a mesh request for the whole selection", async () => {
    setInvokeHandler(() => ({ pairsTotal: 2, alreadyFriends: 0, verifiedOk: 2, failed: 0 }));
    renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("make_selected_friends", {
        userIds: [1, 2],
        mode: "mesh",
        mainUserId: null,
        // O campo de delay manda o valor (antes ia `null` e o backend caía no
        // setting — que só dava para editar no INI).
        delayMs: 2500,
      })
    );
  });

  it("sends a star request with the picked main account", async () => {
    setInvokeHandler(() => ({ pairsTotal: 1, alreadyFriends: 0, verifiedOk: 1, failed: 0 }));
    renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /⭐ bob/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "make_selected_friends",
        expect.objectContaining({ mode: "star", mainUserId: 2 })
      )
    );
  });

  it("skips the confirmation at exactly 30 requests", async () => {
    setInvokeHandler(() => ({ pairsTotal: 15, alreadyFriends: 0, verifiedOk: 15, failed: 0 }));
    const six = [...FOUR, makeAccount({ UserID: 5 }), makeAccount({ UserID: 6 })];
    renderBar(six);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(6\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all \(30 req\)/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("make_selected_friends", expect.anything())
    );
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("confirms above 30 requests and aborts when declined", async () => {
    const seven = [
      ...FOUR,
      makeAccount({ UserID: 5 }),
      makeAccount({ UserID: 6 }),
      makeAccount({ UserID: 7 }),
    ];
    renderBar(seven);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(7\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all \(42 req\)/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("make_selected_friends", expect.anything());
  });

  /**
   * O ritmo dos pedidos é o que decide se o Roblox aplica rate limit (ou pede
   * captcha) no lote inteiro. O controle existia só na barra lateral de
   * multi-seleção, que ninguém conseguia abrir e foi apagada: o valor tinha
   * virado editável apenas pelo INI.
   */
  describe("delay entre pedidos", () => {
    async function abrirSubmenu(settings?: Record<string, Record<string, string>>) {
      const store = renderBar([A, B], settings ? { settings } : {});
      await openActions();
      await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
      return store;
    }

    it("manda o delay escolhido, em milissegundos", async () => {
      await abrirSubmenu();
      const campo = screen.getByLabelText(/Delay between requests/i);

      await userEvent.clear(campo);
      await userEvent.type(campo, "5");
      await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all/ }));

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "make_selected_friends",
          expect.objectContaining({ delayMs: 5000 })
        )
      );
    });

    it("guarda o valor: quem ajustou uma vez não ajusta de novo a cada lote", async () => {
      await abrirSubmenu();
      const campo = screen.getByLabelText(/Delay between requests/i);

      await userEvent.clear(campo);
      await userEvent.type(campo, "5");
      await userEvent.tab();

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("update_setting", {
          section: "Friends",
          key: "RequestDelayMs",
          value: "5000",
        })
      );
    });

    it("abre com o valor salvo, em segundos", async () => {
      const settings = defaultSettings();
      settings.Friends = { RequestDelayMs: "4000" };
      await abrirSubmenu(settings);

      expect(screen.getByLabelText(/Delay between requests/i)).toHaveValue(4);
    });

    it("não mostra um número que o backend não vai respeitar", async () => {
      await abrirSubmenu();
      const campo = screen.getByLabelText(/Delay between requests/i);

      // O backend limita a 0,5–60 s: um campo mostrando 0,1 seria mentira.
      await userEvent.clear(campo);
      await userEvent.type(campo, "0.1");
      await userEvent.tab();
      expect(campo).toHaveValue(0.5);

      await userEvent.clear(campo);
      await userEvent.type(campo, "999");
      await userEvent.tab();
      expect(campo).toHaveValue(60);
    });

    it("texto sem número nenhum volta ao valor anterior", async () => {
      await abrirSubmenu();
      const campo = screen.getByLabelText(/Delay between requests/i);

      await userEvent.clear(campo);
      await userEvent.tab();

      expect(campo).toHaveValue(2.5);
    });
  });

  /**
   * O progresso vem da store (`friend-link-state`), não de um listener próprio
   * desta barra: a sidebar mantinha uma cópia do mesmo código, e quem remontava
   * no meio ficava sem progresso. O número é de **contas**, não de pares.
   */
  it("mostra no botão quantas contas o Make Friends já processou", async () => {
    let release: (value: unknown) => void = () => {};
    setInvokeHandler((cmd) => (cmd === "make_selected_friends" ? new Promise((resolve) => { release = resolve; }) : null));
    renderBar([A, B], {
      friendLinkState: {
        active: true,
        phase: "linking",
        processed: 2,
        total: 5,
        accounts: [],
        mode: "mesh",
        mainUserId: null,
      },
    });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all/ }));

    expect(await screen.findByRole("button", { name: /Friends 2\/5/ })).toBeInTheDocument();

    release({ pairsTotal: 2, alreadyFriends: 0, verifiedOk: 2, failed: 0 });
    await waitFor(() => expect(screen.getByRole("button", { name: /^Actions/ })).toBeInTheDocument());
  });

  it("sem operação ativa, o botão não finge progresso", async () => {
    let release: (value: unknown) => void = () => {};
    setInvokeHandler((cmd) => (cmd === "make_selected_friends" ? new Promise((resolve) => { release = resolve; }) : null));
    renderBar([A, B], {
      friendLinkState: {
        active: false,
        phase: "done",
        processed: 5,
        total: 5,
        accounts: [],
        mode: "mesh",
        mainUserId: null,
      },
    });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    await userEvent.click(screen.getByRole("button", { name: /Mesh - all friend all/ }));

    // Retrato de uma execução antiga não pode virar progresso da atual.
    expect(await screen.findByRole("button", { name: /Linking friends\.\.\./ })).toBeInTheDocument();
    release({ pairsTotal: 2, alreadyFriends: 0, verifiedOk: 2, failed: 0 });
    await waitFor(() => expect(screen.getByRole("button", { name: /^Actions/ })).toBeInTheDocument());
  });
});

/**
 * "Follow a user": as contas selecionadas seguem um alvo digitado (que não
 * precisa estar no app) ou escolhido na lista de contas.
 */
describe("BottomActionBar — follow a user", () => {
  const FOLLOW_RESULT = { total: 2, followed: 2, alreadyFollowing: 0, failed: 0, skippedTarget: false };

  async function openFollow(selected = [A, B], overrides: Partial<StoreValue> = {}) {
    const store = renderBar(selected, overrides);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Follow a user/ }));
    return store;
  }

  function routeFollow(resolved: unknown) {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_follow_target") {
        if (resolved instanceof Error) throw resolved.message;
        return resolved;
      }
      if (cmd === "follow_user_with_accounts") return FOLLOW_RESULT;
      return undefined;
    });
  }

  it("is offered even with a single account selected", async () => {
    renderBar([A]);
    await openActions();
    expect(screen.getByRole("button", { name: "Follow a user (1)" })).toBeInTheDocument();
  });

  it("looks up a typed name, confirms who it is and follows with the whole selection", async () => {
    routeFollow({ id: 9999, name: "starplayer", displayName: "Star" });
    promptAnswers.confirm = true;
    const store = await openFollow();
    await userEvent.type(screen.getByPlaceholderText("Username, user ID or profile link"), "starplayer{Enter}");

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("follow_user_with_accounts", {
        userIds: [1, 2],
        targetUserId: 9999,
        targetName: "starplayer",
        delayMs: 2500,
      })
    );
    expect(invokeMock).toHaveBeenCalledWith("resolve_follow_target", { query: "starplayer" });
    expect(confirmMock.mock.calls[0][0]).toContain("Star (@starplayer)");
    expect(confirmMock.mock.calls[0][0]).toContain("9999");
    await waitFor(() =>
      expect(store.finishActionReport).toHaveBeenCalledWith(
        expect.objectContaining({ summary: expect.stringContaining("2 followed") })
      )
    );
  });

  /**
   * Visto no harness: o mesmo Enter que dispara a busca chegava ao botão
   * "Confirm" (que abre com foco) e aceitava a confirmação sem o usuário ler.
   */
  it("swallows the Enter that submits, so it cannot accept the confirmation too", async () => {
    routeFollow({ id: 9999, name: "starplayer", displayName: "starplayer" });
    await openFollow();
    const input = screen.getByPlaceholderText("Username, user ID or profile link");
    await userEvent.type(input, "starplayer");
    // `fireEvent` devolve `false` quando o handler chamou `preventDefault`.
    expect(fireEvent.keyDown(input, { key: "Enter" })).toBe(false);
    expect(fireEvent.keyDown(input, { key: "a" })).toBe(true);
  });

  it("does not follow when the confirmation is declined", async () => {
    routeFollow({ id: 9999, name: "starplayer", displayName: "starplayer" });
    promptAnswers.confirm = false;
    await openFollow();
    await userEvent.type(screen.getByPlaceholderText("Username, user ID or profile link"), "starplayer");
    await userEvent.click(screen.getByRole("button", { name: "Follow" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("follow_user_with_accounts", expect.anything());
  });

  it("says when the typed user does not exist and follows nobody", async () => {
    routeFollow(new Error("User 'ghost' not found"));
    const store = await openFollow();
    await userEvent.type(screen.getByPlaceholderText("Username, user ID or profile link"), "ghost{Enter}");

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("not found"))
    );
    expect(confirmMock).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("follow_user_with_accounts", expect.anything());
  });

  it("counts only the accounts that will follow when the target is in the selection", async () => {
    routeFollow({ id: 2, name: "bob", displayName: "bob" });
    promptAnswers.confirm = true;
    await openFollow();
    await userEvent.type(screen.getByPlaceholderText("Username, user ID or profile link"), "2{Enter}");

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(confirmMock.mock.calls[0][0]).toMatch(/^1 account\(s\)/);
  });

  it("follows an account picked from the list, without asking", async () => {
    routeFollow(null);
    await openFollow();
    await userEvent.click(screen.getByRole("button", { name: /👤 cid/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "follow_user_with_accounts",
        expect.objectContaining({ userIds: [1, 2], targetUserId: 3, targetName: "cid" })
      )
    );
    expect(confirmMock).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("resolve_follow_target", expect.anything());
  });

  it("filters the account list by what was typed", async () => {
    await openFollow();
    await userEvent.type(screen.getByPlaceholderText("Username, user ID or profile link"), "ci");
    const menu = screen.getByTestId("follow-menu");
    expect(menu).toHaveTextContent("cid");
    expect(menu).not.toHaveTextContent("ann");
  });

  it("does not let the only selected account follow itself", async () => {
    await openFollow([A]);
    expect(screen.getByRole("button", { name: /👤 ann/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /👤 bob/ })).toBeEnabled();
  });

});

describe("BottomActionBar — Botting Mode discovery", () => {
  it("still names Botting Mode in the menu while the toggle is off", async () => {
    renderBar([A, B]);
    await openActions();
    expect(screen.getByRole("button", { name: /Auto Rejoin/ })).toBeInTheDocument();
  });

  it("says on screen what the mode does and how to turn it on", async () => {
    renderBar([A, B]);
    await openActions();
    const blurb = screen.getByText(/closing and relaunching each client/i);
    const text = blurb.textContent ?? "";
    expect(text).toMatch(/Multi Roblox/i);
    expect(text).toMatch(/Settings/i);
  });

  it("drops the explanation once botting is enabled", async () => {
    const settings = defaultSettings();
    settings.General.BottingEnabled = "true";
    renderBar([A, B], { settings });
    await openActions();
    expect(screen.queryByText(/turn it on in Settings › Keep in game/i)).not.toBeInTheDocument();
  });

  it("points at Settings instead of the dialog while the mode is off", async () => {
    const store = renderBar([A, B]);
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Auto Rejoin/ }));
    expect(store.openSettingsSection).toHaveBeenCalledWith("keep-in-game");
    expect(store.openBottingDialog).not.toHaveBeenCalled();
    expect(store.openAfkMode).not.toHaveBeenCalled();
  });
});

describe("BottomActionBar — Hidden mode", () => {
  it("masks the selected account name like the list does", () => {
    renderBar([A], { hideUsernames: true, hiddenNameLetters: 2 });
    expect(screen.queryByText("ann")).not.toBeInTheDocument();
    expect(screen.getByText("an********")).toBeInTheDocument();
  });

  it("hides the name entirely when no preview letters are configured", () => {
    renderBar([A], { hideUsernames: true, hiddenNameLetters: 0 });
    expect(screen.queryByText("ann")).not.toBeInTheDocument();
    expect(screen.getByText("************")).toBeInTheDocument();
  });

  it("masks the account's alias too", () => {
    const aliased = makeAccount({ UserID: 9, Username: "ann", Alias: "mainAccount" });
    renderBar([aliased], { hideUsernames: true, hiddenNameLetters: 4 });
    expect(screen.queryByText("mainAccount")).not.toBeInTheDocument();
    expect(screen.getByText("main********")).toBeInTheDocument();
  });

  it("masks the names listed in the Make Friends star picker", async () => {
    renderBar([A, B], { hideUsernames: true, hiddenNameLetters: 2 });
    await openActions();
    await userEvent.click(screen.getByRole("button", { name: /Make Friends \(2\)/ }));
    expect(screen.queryByRole("button", { name: /ann/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /an\*{8}/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /bo\*{8}/ })).toBeInTheDocument();
  });

  it("shows the real name while Hidden is off", () => {
    renderBar([A]);
    expect(screen.getByText("ann")).toBeInTheDocument();
  });
});
