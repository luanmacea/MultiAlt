import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../store", async () => (await import("../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());

import { ChooseGameScreen } from "./ChooseGameScreen";
import { makeAccount, setStore } from "../test-utils/renderWithStore";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeHandler } from "../test-utils/tauriMocks";
import type { JoinTarget } from "../types";
import type { StoreValue } from "../store";

function joinTarget(overrides: Partial<JoinTarget> = {}): JoinTarget {
  return {
    kind: "place",
    placeId: 606849621,
    jobId: "",
    accessCode: "",
    linkCode: "",
    launchData: "",
    inviterId: null,
    note: null,
    ...overrides,
  };
}

const ACCOUNT_A = makeAccount({ UserID: 1001, Username: "alpha" });
const ACCOUNT_B = makeAccount({ UserID: 1002, Username: "bravo" });

/** Renders the screen with `selected` accounts and opens the Follow tab. */
async function renderFollowTab(selected = [ACCOUNT_A, ACCOUNT_B]): Promise<StoreValue> {
  const store = setStore({
    accounts: [ACCOUNT_A, ACCOUNT_B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
  });
  render(<ChooseGameScreen />);
  await userEvent.click(screen.getByRole("button", { name: "Follow" }));
  return store;
}

function linkInput(): HTMLInputElement {
  return screen.getByPlaceholderText(/ExperienceInvite/) as HTMLInputElement;
}

function joinButton(): HTMLButtonElement {
  return screen.getByRole("button", { name: "Join" }) as HTMLButtonElement;
}

beforeEach(() => {
  resetTauriMocks();
  localStorage.clear();
});

afterEach(cleanup);

describe("ChooseGameScreen — JoinLinkSection", () => {
  it("resolves the pasted link with the first selected account and launches everyone", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "job", jobId: "job-1" });
      return undefined;
    });
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/games/606849621");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_join_link", {
        userId: 1001,
        link: "https://www.roblox.com/games/606849621",
      })
    );
    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith([1001, 1002], {
        placeId: "606849621",
        jobId: "job-1",
        launchData: undefined,
        joinVip: undefined,
        linkCode: undefined,
      })
    );
  });

  it("uses joinServer when exactly one account is selected", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place" }));
    const store = await renderFollowTab([ACCOUNT_A]);

    await userEvent.type(linkInput(), "roblox://placeId=606849621");
    await userEvent.click(joinButton());

    await waitFor(() => expect(store.joinServer).toHaveBeenCalledTimes(1));
    expect(store.joinServer).toHaveBeenCalledWith(1001, expect.objectContaining({ placeId: "606849621" }));
    expect(store.launchMultiple).not.toHaveBeenCalled();
  });

  it("sends a private server as joinVip + linkCode and mirrors it into the Job ID field", async () => {
    setInvokeHandler(() =>
      joinTarget({ kind: "private", linkCode: "abc123", jobId: "ignored-job" })
    );
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/share?code=abc123&type=Server");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith([1001, 1002], {
        placeId: "606849621",
        jobId: "",
        launchData: undefined,
        joinVip: true,
        linkCode: "abc123",
      })
    );
    expect(store.setJobId).toHaveBeenCalledWith("vip:abc123");
  });

  it("falls back to accessCode when the private link carries no linkCode", async () => {
    setInvokeHandler(() => joinTarget({ kind: "private", accessCode: "access-9" }));
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "vip-link");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ joinVip: true, linkCode: "access-9" })
      )
    );
  });

  it("forwards launchData from an invite", async () => {
    setInvokeHandler(() =>
      joinTarget({ kind: "invite", jobId: "job-7", launchData: "payload" })
    );
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://ro.blox.com/Ebh5");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ jobId: "job-7", launchData: "payload" })
      )
    );
  });

  it.each([
    ["invite", { kind: "invite" as const, jobId: "job-1" }, "Invite · place 606849621 · server job-1"],
    ["private", { kind: "private" as const, linkCode: "abc" }, "Private server · place 606849621"],
    ["job", { kind: "job" as const, jobId: "job-2" }, "Server · place 606849621 · server job-2"],
    ["place", { kind: "place" as const }, "Game · place 606849621"],
  ])("renders the resolved summary for a %s link", async (_kind, overrides, expected) => {
    setInvokeHandler(() => joinTarget(overrides));
    await renderFollowTab();

    await userEvent.type(linkInput(), "some-link");
    await userEvent.click(joinButton());

    expect(await screen.findByText(expected)).toBeInTheDocument();
  });

  it("shows the expiry warning but still launches when the target carries a note", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place", note: "Expired" }));
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/share?code=old");
    await userEvent.click(joinButton());

    expect(await screen.findByText(/no longer valid \(Expired\)/i)).toBeInTheDocument();
    await waitFor(() => expect(store.launchMultiple).toHaveBeenCalledTimes(1));
  });

  it("shows a backend error inline and keeps the typed link", async () => {
    setInvokeHandler(() => {
      throw new Error("link expired or invalid");
    });
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://bad.link/x");
    await userEvent.click(joinButton());

    expect(await screen.findByText(/link expired or invalid/i)).toBeInTheDocument();
    expect(linkInput()).toHaveValue("https://bad.link/x");
    expect(store.launchMultiple).not.toHaveBeenCalled();
    // Re-enabled after the failure so the user can retry.
    await waitFor(() => expect(joinButton()).toBeEnabled());
  });

  it("surfaces a launch failure inline without clearing the link", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place" }));
    const store = await renderFollowTab();
    (store.launchMultiple as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("multi roblox off"));

    await userEvent.type(linkInput(), "https://www.roblox.com/games/1");
    await userEvent.click(joinButton());

    expect(await screen.findByText(/multi roblox off/i)).toBeInTheDocument();
    expect(linkInput()).toHaveValue("https://www.roblox.com/games/1");
  });

  it("submits on Enter", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place" }));
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/games/606849621{Enter}");

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("resolve_join_link", expect.anything()));
    await waitFor(() => expect(store.launchMultiple).toHaveBeenCalledTimes(1));
  });

  it("keeps Join disabled until a link is typed", async () => {
    await renderFollowTab();
    expect(joinButton()).toBeDisabled();

    await userEvent.type(linkInput(), "  ");
    expect(joinButton()).toBeDisabled();

    await userEvent.type(linkInput(), "link");
    expect(joinButton()).toBeEnabled();
  });

  it("keeps Join disabled while no account is selected", async () => {
    await renderFollowTab([]);
    await userEvent.type(linkInput(), "link");
    expect(joinButton()).toBeDisabled();
  });

  it("disables the input and the button while the link is resolving", async () => {
    let release: (value: JoinTarget) => void = () => {};
    setInvokeHandler(
      () => new Promise<JoinTarget>((resolve) => { release = resolve; })
    );
    await renderFollowTab();

    await userEvent.type(linkInput(), "link");
    await userEvent.click(joinButton());

    expect(await screen.findByRole("button", { name: "Resolving link..." })).toBeDisabled();
    expect(linkInput()).toBeDisabled();

    release(joinTarget());
    await waitFor(() => expect(joinButton()).toBeEnabled());
  });

  it("labels the section with the selected account count", async () => {
    await renderFollowTab();
    expect(screen.getAllByText("2 accounts").length).toBeGreaterThan(0);

    cleanup();
    await renderFollowTab([ACCOUNT_A]);
    expect(screen.getAllByText("1 account").length).toBeGreaterThan(0);
  });
});

describe("ChooseGameScreen — FollowTab", () => {
  /** O botão "Follow" do formulário (o primeiro com esse nome é a aba). */
  function followButton(): HTMLButtonElement {
    const buttons = screen.getAllByRole("button", { name: "Follow" });
    return buttons[buttons.length - 1] as HTMLButtonElement;
  }

  function presence(overrides: Record<string, unknown> = {}) {
    return [{ userPresenceType: 2, placeId: 111, rootPlaceId: 606849621, gameId: "job-x", ...overrides }];
  }

  it("resolves the target once and launches every account through the batch launcher", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "lookup_user") return { id: 42 };
      if (cmd === "get_presence") return presence();
      return undefined;
    });
    const store = await renderFollowTab();

    await userEvent.type(screen.getByPlaceholderText("e.g. Builderman"), "Builderman");
    await userEvent.click(followButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ placeId: "606849621", jobId: "job-x" })
      )
    );
    expect(store.launchMultiple).toHaveBeenCalledTimes(1);
  });

  /**
   * Regressão: o Follow já lançou conta por conta com `launch_roblox` e
   * `sleep(3000)`, furando o piso anti-captcha de 8 s que `launch_multiple`
   * aplica no backend. Nada aqui pode voltar a lançar em laço.
   */
  it("never calls launch_roblox in a loop", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "lookup_user") return { id: 42 };
      if (cmd === "get_presence") return presence();
      return undefined;
    });
    await renderFollowTab();

    await userEvent.type(screen.getByPlaceholderText("e.g. Builderman"), "Builderman");
    await userEvent.click(followButton());

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_presence", expect.anything()));
    expect(invokeMock.mock.calls.filter((call) => call[0] === "launch_roblox")).toHaveLength(0);
  });

  it("does not launch when the target is not in a game", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "lookup_user") return { id: 42 };
      if (cmd === "get_presence") return presence({ userPresenceType: 1, gameId: null, rootPlaceId: null, placeId: null });
      return undefined;
    });
    const store = await renderFollowTab();

    await userEvent.type(screen.getByPlaceholderText("e.g. Builderman"), "Builderman");
    await userEvent.click(followButton());

    await waitFor(() => expect(store.addToast).toHaveBeenCalled());
    expect(store.launchMultiple).not.toHaveBeenCalled();
    expect(store.joinServer).not.toHaveBeenCalled();
    expect(invokeMock.mock.calls.filter((call) => call[0] === "launch_roblox")).toHaveLength(0);
  });

  it("uses rootPlaceId over placeId", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "lookup_user") return { id: 42 };
      if (cmd === "get_presence") return presence({ rootPlaceId: 999, placeId: 111 });
      return undefined;
    });
    const store = await renderFollowTab();

    await userEvent.type(screen.getByPlaceholderText("e.g. Builderman"), "Builderman");
    await userEvent.click(followButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith([1001, 1002], expect.objectContaining({ placeId: "999" }))
    );
  });
});

describe("ChooseGameScreen — Friends tab", () => {
  it("shows a Friends tab that groups the online friends per selected account", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_online_friends_for_accounts") {
        return [
          {
            userId: 1001,
            error: null,
            friends: [
              {
                userId: 7001,
                name: "friendo",
                displayName: "Friendo",
                presenceType: 2,
                lastLocation: "Some Game",
                placeId: 189707,
                rootPlaceId: 189707,
                gameId: "job-a",
              },
            ],
          },
          { userId: 1002, error: null, friends: [] },
        ];
      }
      return [];
    });
    setStore({
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set([1001, 1002]),
      selectedAccounts: [ACCOUNT_A, ACCOUNT_B],
    });
    render(<ChooseGameScreen />);

    await userEvent.click(screen.getByRole("button", { name: "Friends" }));

    expect(await screen.findByTestId("friends-group-1001")).toBeInTheDocument();
    expect(screen.getByTestId("friends-group-1002")).toBeInTheDocument();
    expect(screen.getByTestId("friend-1001-7001")).toBeInTheDocument();
  });
});

/**
 * Preferência de servidor (Random / Emptiest / Fullest, com filtro de país).
 *
 * Contrato que estes testes travam: o servidor é resolvido **uma vez** para o
 * lote — se cada conta resolvesse o seu, o lote não jogaria junto — e só quando
 * o usuário não escolheu servidor.
 */
describe("ChooseGameScreen — preferência de servidor", () => {
  function pickStore(overrides: Partial<StoreValue> = {}) {
    return setStore({
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set([1001, 1002]),
      selectedAccounts: [ACCOUNT_A, ACCOUNT_B],
      ...overrides,
    });
  }

  async function joinPlainLink() {
    render(<ChooseGameScreen />);
    await userEvent.click(screen.getByRole("button", { name: "Follow" }));
    await userEvent.type(linkInput(), "https://www.roblox.com/games/606849621");
    await userEvent.click(joinButton());
  }

  it("resolve o servidor uma vez para o lote e lança todo mundo nele", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "place" });
      if (cmd === "pick_server") {
        return { jobId: "job-vazio", playing: 2, maxPlayers: 30, region: null, regionFallback: false };
      }
      return undefined;
    });
    const store = pickStore({ serverPreference: "emptiest", serverRegionFilter: "" });

    await joinPlainLink();

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ jobId: "job-vazio" })
      )
    );
    const picks = invokeMock.mock.calls.filter((call) => call[0] === "pick_server");
    expect(picks).toHaveLength(1);
    expect(picks[0][1]).toMatchObject({
      userId: 1001,
      placeId: 606849621,
      preference: "emptiest",
      accounts: 2,
    });
  });

  it("manda o país escolhido junto com a preferência", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "place" });
      if (cmd === "pick_server") {
        return { jobId: "job-br", playing: 4, maxPlayers: 30, region: null, regionFallback: false };
      }
      return undefined;
    });
    pickStore({ serverPreference: "emptiest", serverRegionFilter: "BR" });

    await joinPlainLink();

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "pick_server",
        expect.objectContaining({ countryCode: "BR" })
      )
    );
  });

  it("não resolve nada quando o usuário pede para o Roblox escolher", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "place" });
      return undefined;
    });
    const store = pickStore({ serverPreference: "none" });

    await joinPlainLink();

    await waitFor(() => expect(store.launchMultiple).toHaveBeenCalled());
    expect(invokeMock.mock.calls.filter((call) => call[0] === "pick_server")).toHaveLength(0);
  });

  /** Job ID explícito vence a preferência: o usuário já escolheu. */
  it("não sobrepõe um servidor que o usuário escolheu", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "job", jobId: "job-do-usuario" });
      return undefined;
    });
    const store = pickStore({ serverPreference: "fullest" });

    await joinPlainLink();

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ jobId: "job-do-usuario" })
      )
    );
    expect(invokeMock.mock.calls.filter((call) => call[0] === "pick_server")).toHaveLength(0);
  });

  /**
   * Sem servidor no país pedido o backend devolve o melhor disponível marcado
   * como fallback; entrar nele sem perguntar seria justamente o que o usuário
   * quer evitar. Sem `PromptProvider`, `confirm()` resolve `false`.
   */
  it("não entra no servidor de outro país sem confirmação", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "place" });
      if (cmd === "pick_server") {
        return { jobId: "job-us", playing: 2, maxPlayers: 30, region: null, regionFallback: true };
      }
      return undefined;
    });
    const store = pickStore({ serverPreference: "emptiest", serverRegionFilter: "BR" });

    await joinPlainLink();

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("pick_server", expect.anything()));
    expect(store.launchMultiple).not.toHaveBeenCalled();
    expect(store.joinServer).not.toHaveBeenCalled();
  });

  /** Falha ao escolher servidor não pode cancelar o launch. */
  it("cai no comportamento antigo quando a escolha falha", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "place" });
      if (cmd === "pick_server") throw new Error("429 Too Many Requests");
      return undefined;
    });
    const store = pickStore({ serverPreference: "emptiest" });

    await joinPlainLink();

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ jobId: "" })
      )
    );
    expect(store.addToast).toHaveBeenCalled();
  });
});

describe("ChooseGameScreen — Servers tab", () => {
  it("mostra a aba de servidores com o que a varredura já achou", async () => {
    setInvokeHandler((cmd) => (cmd === "start_server_scan" ? 3 : undefined));
    setStore({
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set([1001, 1002]),
      selectedAccounts: [ACCOUNT_A, ACCOUNT_B],
      placeId: "606849621",
    });
    render(<ChooseGameScreen />);

    await userEvent.click(screen.getByRole("button", { name: "Servers" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("start_server_scan", expect.anything()));

    emitTauriEvent("server-scan", {
      scanId: 3,
      placeId: 606849621,
      servers: [{ id: "job-listado", playing: 5, maxPlayers: 30, ping: 30 }],
      scanned: 1,
      fitting: 1,
      done: true,
      error: null,
    });

    expect(await screen.findByText("job-listado")).toBeInTheDocument();
    expect(screen.getByText("/ 30")).toBeInTheDocument();
  });

  /** Item do usuário: do jogo direto para os servidores dele. */
  it("abre os servidores do jogo pelo botão da aba Games", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "search_games") {
        return {
          sorts: [
            { games: [{ rootPlaceId: 606849621, universeId: 1, name: "Jailbreak", playerCount: 10 }] },
          ],
        };
      }
      if (cmd === "start_server_scan") return 4;
      return undefined;
    });
    const store = setStore({
      accounts: [ACCOUNT_A],
      selectedIds: new Set([1001]),
      selectedAccounts: [ACCOUNT_A],
    });
    render(<ChooseGameScreen />);

    await userEvent.click(screen.getByRole("button", { name: "Games" }));
    const browse = await screen.findByRole("button", { name: /Browse servers/i });
    await userEvent.click(browse);

    // Guarda o place e troca de aba; quem lista é a própria aba Servers, com
    // testes seus em src/components/servers.
    expect(store.setPlaceId).toHaveBeenCalledWith("606849621");
    expect(await screen.findByLabelText("Place ID")).toBeInTheDocument();
  });
});

/**
 * Descoberta: toda lista de jogos leva aos servidores daquele jogo, a dica de
 * cada aba diz o que a aba faz, e a grade de janelas não divide espaço com o
 * log de lançamento.
 */
describe("ChooseGameScreen — descoberta", () => {
  function renderScreen(overrides: Partial<StoreValue> = {}) {
    const store = setStore({
      accounts: [ACCOUNT_A],
      selectedIds: new Set([1001]),
      selectedAccounts: [ACCOUNT_A],
      ...overrides,
    });
    render(<ChooseGameScreen />);
    return store;
  }

  function seedFavorite() {
    localStorage.setItem(
      "ram_favorite_games",
      JSON.stringify([
        { placeId: 606849621, name: "Jailbreak", iconUrl: null, addedAt: 0, vipServers: [] },
      ])
    );
  }

  function seedRecent() {
    localStorage.setItem(
      "ram_recent_games",
      JSON.stringify([
        { placeId: 920587237, name: "Adopt Me", iconUrl: null, lastPlayed: Date.now() },
      ])
    );
  }

  /** Item do dono: ver servidores de um favorito exigia pesquisar na aba Games. */
  it("abre os servidores de um jogo favorito", async () => {
    setInvokeHandler((cmd) => (cmd === "start_server_scan" ? 7 : undefined));
    seedFavorite();
    const store = renderScreen();

    await userEvent.click(await screen.findByRole("button", { name: "Browse servers" }));

    expect(store.setPlaceId).toHaveBeenCalledWith("606849621");
    expect(await screen.findByLabelText("Place ID")).toBeInTheDocument();
    expect(store.joinServer).not.toHaveBeenCalled();
  });

  it("abre os servidores de um jogo recente", async () => {
    setInvokeHandler((cmd) => (cmd === "start_server_scan" ? 8 : undefined));
    seedRecent();
    const store = renderScreen();

    await userEvent.click(screen.getByRole("button", { name: "Recent" }));
    await userEvent.click(await screen.findByRole("button", { name: "Browse servers" }));

    expect(store.setPlaceId).toHaveBeenCalledWith("920587237");
    expect(await screen.findByLabelText("Place ID")).toBeInTheDocument();
    expect(store.joinServer).not.toHaveBeenCalled();
  });

  /** A aba do Join link era a única sem dica: ninguém sabia o que havia nela. */
  it("explica a aba Follow na dica", async () => {
    renderScreen();

    await userEvent.click(screen.getByRole("button", { name: "Follow" }));

    expect(screen.getByText(/Paste any Roblox link/i)).toBeInTheDocument();
  });

  /** Quem colou link no campo de Job ID precisa de um caminho curto até lá. */
  it("leva da aba Servers ao Join link em um clique", async () => {
    setInvokeHandler((cmd) => (cmd === "start_server_scan" ? 9 : undefined));
    renderScreen();

    await userEvent.click(screen.getByRole("button", { name: "Servers" }));
    await userEvent.click(screen.getByRole("button", { name: /Paste a join link/i }));

    expect(screen.getByPlaceholderText(/ExperienceInvite/)).toBeInTheDocument();
  });

  /** A grade de janelas espremia o log do Console em 35 px. */
  it("tira a grade de janelas da aba Console", async () => {
    renderScreen();

    await userEvent.click(screen.getByRole("button", { name: "Console" }));
    expect(screen.queryByText("Window layout")).not.toBeInTheDocument();
    expect(screen.getByText("No launch activity yet")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Windows" }));
    expect(await screen.findByText("Window layout")).toBeInTheDocument();
  });
});

describe("ChooseGameScreen — chips das contas", () => {
  it("tira a conta do lote pelo x do chip", async () => {
    const store = setStore({
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set([1001, 1002]),
      selectedAccounts: [ACCOUNT_A, ACCOUNT_B],
    });
    render(<ChooseGameScreen />);

    await userEvent.click(screen.getByRole("button", { name: /Remove alpha from this launch/i }));
    expect(store.setSelectedIds).toHaveBeenCalledWith(new Set([1002]));
  });

  /** Um lote vazio deixaria a tela sem nada para lançar. */
  it("não deixa remover a última conta", async () => {
    const store = setStore({
      accounts: [ACCOUNT_A],
      selectedIds: new Set([1001]),
      selectedAccounts: [ACCOUNT_A],
    });
    render(<ChooseGameScreen />);

    const remove = screen.getByRole("button", { name: /Remove alpha from this launch/i });
    expect(remove).toBeDisabled();
    await userEvent.click(remove);
    expect(store.setSelectedIds).not.toHaveBeenCalled();
  });
});

describe("ChooseGameScreen — shell", () => {
  it("closes on Back and on Escape", async () => {
    const store = setStore({
      accounts: [ACCOUNT_A],
      selectedIds: new Set([1001]),
      selectedAccounts: [ACCOUNT_A],
    });
    render(<ChooseGameScreen />);

    await userEvent.click(screen.getByRole("button", { name: /Back/ }));
    expect(store.setChooseGameOpen).toHaveBeenCalledWith(false);

    await userEvent.keyboard("{Escape}");
    expect(store.setChooseGameOpen).toHaveBeenCalledTimes(2);
  });

  it("summarises how many accounts will launch", async () => {
    setStore({
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set([1001, 1002]),
      selectedAccounts: [ACCOUNT_A, ACCOUNT_B],
    });
    render(<ChooseGameScreen />);
    expect(screen.getByText("2 accounts will be launched together")).toBeInTheDocument();
    expect(screen.getByText("alpha")).toBeInTheDocument();
    expect(screen.getByText("bravo")).toBeInTheDocument();
  });
});
