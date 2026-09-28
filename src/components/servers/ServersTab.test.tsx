import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { ServersTab, dedupeRows, fitScore, hasRoomFor, matchesRegion, rankRows } from "./ServersTab";
import type { ServerRow } from "./ServersTab";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import {
  emitTauriEvent,
  invokeMock,
  resetTauriMocks,
  setInvokeMap,
} from "../../test-utils/tauriMocks";
import { clearGameIdentityCache } from "../../hooks/useGameIdentity";
import type { ServerRegion } from "../../types";
import type { StoreValue } from "../../store";

const ACCOUNT_A = makeAccount({ UserID: 1001, Username: "alpha" });
const ACCOUNT_B = makeAccount({ UserID: 1002, Username: "bravo" });

function row(overrides: Partial<ServerRow> = {}): ServerRow {
  return { id: "job-a", playing: 5, maxPlayers: 30, ping: 40, ...overrides };
}

function region(jobId: string, countryCode: string, city = "Cidade"): ServerRegion {
  return {
    jobId,
    label: `${city}, ${countryCode}`,
    region: { ip: "1.2.3.4", city, region: "", country: countryCode, countryCode },
    error: null,
  };
}

/**
 * Dublê de `useLauncher().launchAll`. A aba **não pode** ter caminho de launch
 * próprio: o contrato travado aqui é um único `launchAll` com todas as contas
 * e o Job ID escolhido.
 */
const launchAll = vi.fn(
  async (
    _userIds: number[],
    _placeId: number,
    _jobId?: string,
    _onStarted?: () => void
  ): Promise<{ ok: boolean; error?: string }> => ({ ok: true })
);

const setPlaceId = vi.fn();

const SCAN_ID = 7;

/**
 * A lista chega por evento, página a página: a varredura roda em background no
 * backend e publica o que já achou, sempre reordenado.
 */
function emitScan(
  rows: ServerRow[],
  extra: Partial<{ scanned: number; fitting: number; done: boolean; stoppedAtLimit: boolean }> = {}
) {
  emitTauriEvent("server-scan", {
    scanId: SCAN_ID,
    placeId: 606849621,
    servers: rows,
    scanned: extra.scanned ?? rows.length,
    fitting: extra.fitting ?? rows.filter((r) => r.playing + 2 <= r.maxPlayers).length,
    done: extra.done ?? true,
    stoppedAtLimit: extra.stoppedAtLimit ?? false,
    error: null,
  });
}

function renderTab(
  rows: ServerRow[],
  overrides: Partial<StoreValue> = {},
  selected = [ACCOUNT_A, ACCOUNT_B]
) {
  setInvokeMap({
    start_server_scan: SCAN_ID,
    stop_server_scan: null,
    get_server_regions: [],
  });
  const userIds = selected.map((a) => a.UserID);
  const result = renderWithStore(
    <ServersTab
      userIds={userIds}
      placeId="606849621"
      setPlaceId={setPlaceId}
      launchAll={launchAll}
    />,
    {
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set(userIds),
      selectedAccounts: selected,
      ...overrides,
    }
  );
  emitScan(rows);
  return result;
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((call) => call[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  // O cache de identidade do jogo é de módulo (vive a sessão inteira do app):
  // sem zerar, um teste entrega o nome ao seguinte e esconde a regressão.
  clearGameIdentityCache();
  launchAll.mockReset();
  launchAll.mockResolvedValue({ ok: true });
  setPlaceId.mockReset();
});

afterEach(cleanup);

describe("ServersTab — vagas (puro)", () => {

  it("só considera com vaga o servidor que cabe o lote inteiro", () => {
    expect(hasRoomFor(row({ playing: 28, maxPlayers: 30 }), 8)).toBe(false);
    expect(hasRoomFor(row({ playing: 28, maxPlayers: 30 }), 2)).toBe(true);
    expect(hasRoomFor(row({ playing: 30, maxPlayers: 30 }), 1)).toBe(false);
    expect(hasRoomFor(row({ playing: 0, maxPlayers: 0 }), 1)).toBe(false);
  });

  /**
   * Servidor ainda sem região resolvida continua listado: escondê-lo daria a
   * impressão falsa de que não há servidor naquele país.
   */
  it("filtra por país sem esconder quem ainda não teve a região resolvida", () => {
    const regions = new Map([["a", region("a", "US")], ["b", region("b", "BR")]]);
    expect(matchesRegion(row({ id: "a" }), regions, "BR")).toBe(false);
    expect(matchesRegion(row({ id: "b" }), regions, "BR")).toBe(true);
    expect(matchesRegion(row({ id: "sem-regiao" }), regions, "BR")).toBe(true);
    // Filtro desligado aceita tudo.
    expect(matchesRegion(row({ id: "a" }), regions, "")).toBe(true);
  });
});

describe("ServersTab — ordem por encaixe (puro)", () => {
  /**
   * O topo é sempre o melhor encaixe para o lote **desta tela**. Era o que
   * faltava: com 6 contas a lista abria com servidores de 3 vagas e os que
   * levavam todo mundo ficavam escondidos na rolagem.
   */
  it("põe quem cabe o lote na frente de quem não cabe", () => {
    const rows = [
      row({ id: "nao-cabe", playing: 10, maxPlayers: 13 }),
      row({ id: "cabe", playing: 6, maxPlayers: 13 }),
      row({ id: "nao-cabe-2", playing: 9, maxPlayers: 13 }),
    ];
    expect(rankRows(rows, 6).map((r) => r.id)).toEqual(["cabe", "nao-cabe-2", "nao-cabe"]);
  });

  /**
   * A regra que o usuário pediu: a folga ideal depois do lote entrar é **uma
   * vaga**, e a nota piora conforme se afasta disso para qualquer lado.
   * Validado na UI real pelo cenário `servers-big-game` do harness.
   */
  it("põe no topo o servidor que deixa exatamente uma vaga de folga", () => {
    const rows = [
      row({ id: "vazio", playing: 0, maxPlayers: 13 }),
      row({ id: "sobra-2", playing: 5, maxPlayers: 13 }),
      row({ id: "ideal", playing: 6, maxPlayers: 13 }),
      row({ id: "lota", playing: 7, maxPlayers: 13 }),
    ];
    expect(rankRows(rows, 6).map((r) => r.id)).toEqual([
      "ideal",
      "lota",
      "sobra-2",
      "vazio",
    ]);
  });

  it("empata pela folga e desempata pelo mais cheio", () => {
    const lota = row({ id: "lota", playing: 7, maxPlayers: 13 });
    const sobra = row({ id: "sobra", playing: 5, maxPlayers: 13 });
    expect(fitScore(lota, 6)[1]).toBe(fitScore(sobra, 6)[1]);
    expect(rankRows([sobra, lota], 6).map((r) => r.id)).toEqual(["lota", "sobra"]);
  });

  it("quem cabe sempre pontua melhor que quem não cabe", () => {
    const cabe = row({ id: "cabe", playing: 0, maxPlayers: 13 });
    const quase = row({ id: "quase", playing: 8, maxPlayers: 13 });
    expect(fitScore(cabe, 6)[0]).toBeLessThan(fitScore(quase, 6)[0]);
  });

  it("servidor sem tamanho fica por último", () => {
    const rows = [row({ id: "sem", playing: 0, maxPlayers: 0 }), row({ id: "ok", playing: 6, maxPlayers: 13 })];
    expect(rankRows(rows, 6).map((r) => r.id)).toEqual(["ok", "sem"]);
  });

  it("sem ninguém que caiba, ordena por quantas contas levam", () => {
    const rows = [
      row({ id: "uma-vaga", playing: 12, maxPlayers: 13 }),
      row({ id: "quatro-vagas", playing: 9, maxPlayers: 13 }),
      row({ id: "duas-vagas", playing: 11, maxPlayers: 13 }),
    ];
    expect(rankRows(rows, 6).map((r) => r.id)).toEqual([
      "quatro-vagas",
      "duas-vagas",
      "uma-vaga",
    ]);
  });

  it("com as mesmas vagas, o mais cheio vem primeiro", () => {
    const rows = [
      row({ id: "pequeno", playing: 1, maxPlayers: 4 }),
      row({ id: "grande", playing: 20, maxPlayers: 23 }),
    ];
    expect(rankRows(rows, 6).map((r) => r.id)).toEqual(["grande", "pequeno"]);
  });

  it("não altera a lista original", () => {
    const rows = [row({ id: "a", playing: 10 }), row({ id: "b", playing: 1 })];
    rankRows(rows, 6);
    expect(rows.map((r) => r.id)).toEqual(["a", "b"]);
  });
});

describe("ServersTab — lista", () => {
  it("lista os servidores do place com jogadores e ping", async () => {
    renderTab([row({ id: "job-a", playing: 5 }), row({ id: "job-b", playing: 12 })]);

    await screen.findByText("job-a");
    expect(screen.getByText("5")).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();

    const args = (callsFor("start_server_scan")[0][1] ?? {}) as Record<string, unknown>;
    expect(args.placeId).toBe(606849621);
  });

  /**
   * A ordenação e a paginação moram no backend: a resposta da Roblox traz 100
   * servidores de milhares, e reordenar a página local mostrava "o mais cheio
   * entre os mais vazios" — era o 3/13 em tudo com "Fullest" ligado.
   */
  it("pede a lista já ordenada ao backend, com a preferência e o tamanho do lote", async () => {
    renderTab([row({ id: "cheio", playing: 12, maxPlayers: 13 })], {
      serverPreference: "fullest",
    });

    await screen.findByText("cheio");
    const args = (callsFor("start_server_scan")[0][1] ?? {}) as Record<string, unknown>;
    expect(args.preference).toBe("fullest");
    expect(args.accounts).toBe(2);
    expect(args.placeId).toBe(606849621);
  });

  /** O lote inteiro precisa caber; senão as contas se espalham. */
  it("desabilita o Join de um servidor sem vaga para todas as contas", async () => {
    renderTab([row({ id: "apertado", playing: 29, maxPlayers: 30 })]);

    await screen.findByText("apertado");
    expect(screen.getByRole("button", { name: "Join" })).toBeDisabled();
  });

  /**
   * O que o usuário viu: servidores de poucas vagas no topo, e os que levavam
   * o lote inteiro escondidos na rolagem. A ordem final é da UI, que é quem
   * sabe quantas contas estão selecionadas agora.
   */
  it("mostra no topo o servidor que cabe o lote, mesmo se a lista chegar fora de ordem", async () => {
    renderTab([]);
    emitScan(
      [
        row({ id: "nao-cabe", playing: 12, maxPlayers: 13 }),
        row({ id: "cabe", playing: 5, maxPlayers: 13 }),
      ],
      { scanned: 1800, fitting: 1, done: true }
    );

    await screen.findByText("cabe");
    const ids = screen.getAllByText(/^(cabe|nao-cabe)$/).map((el) => el.textContent);
    expect(ids).toEqual(["cabe", "nao-cabe"]);
  });

  it("manda TODAS as contas selecionadas para o servidor clicado, numa chamada só", async () => {
    const user = userEvent.setup();
    renderTab([row({ id: "job-escolhido", playing: 3 })]);

    await screen.findByText("job-escolhido");
    await user.click(screen.getByRole("button", { name: "Join" }));

    await waitFor(() => expect(launchAll).toHaveBeenCalledTimes(1));
    expect(launchAll).toHaveBeenCalledWith(
      [1001, 1002],
      606849621,
      "job-escolhido",
      undefined
    );
  });
});

/**
 * Job ID: antes era um `<code>` dentro de uma coluna `flex-1` (528px de
 * 1056, ~290px vazios num Job ID real de ~238px) sem `onClick`, `title` nem
 * menu de contexto — a região é que devia ganhar aquele espaço, e o Job ID
 * precisava ser copiável.
 */
describe("ServersTab — Job ID copiável", () => {
  const writeText = vi.fn(async () => {});

  beforeEach(() => {
    writeText.mockClear();
  });

  /**
   * `userEvent.setup()` instala o próprio stub de clipboard (com getter
   * `configurable: true`) na hora em que roda — depois disso é que dá para
   * sobrescrever com o mock e ter certeza de que é ele quem o componente vê.
   * Fazer isso num `beforeEach`, antes do `setup()`, perde a corrida: o stub
   * do user-event substitui o mock, e `writeText` nunca é chamado.
   */
  function mockClipboard() {
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  }

  it("copia o Job ID ao clicar, sem disparar o Join", async () => {
    const user = userEvent.setup();
    mockClipboard();
    const { store } = renderTab([row({ id: "job-copy" })]);
    await screen.findByText("job-copy");

    await user.click(screen.getByRole("button", { name: "Copy Job ID" }));

    expect(writeText).toHaveBeenCalledWith("job-copy");
    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringMatching(/copied/i))
    );
    expect(launchAll).not.toHaveBeenCalled();
  });

  it("avisa quando o clipboard falha", async () => {
    writeText.mockRejectedValueOnce(new Error("denied"));
    const user = userEvent.setup();
    mockClipboard();
    const { store } = renderTab([row({ id: "job-copy" })]);
    await screen.findByText("job-copy");

    await user.click(screen.getByRole("button", { name: "Copy Job ID" }));

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringMatching(/fail/i))
    );
  });

  /**
   * Coluna fixa: o Job ID não pode mais esticar com `flex-1` — é a região
   * (antes travada em 150px) que agora recebe o espaço sobrando.
   */
  it("mantém o Job ID numa coluna de largura fixa", async () => {
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    const jobIdButton = screen.getByRole("button", { name: "Copy Job ID" });
    // 264px: o Job ID inteiro na fonte mono de 12px mede 259px (medido na
    // tela). A largura fixa é o que impede a coluna de dançar entre linhas.
    expect(jobIdButton.className).toMatch(/w-\[264px\]/);
    expect(jobIdButton.className).not.toMatch(/flex-1/);
  });

  /**
   * Armadilha do relato: o botão de copiar não pode ganhar um nome acessível
   * que bate com `/Join/`, senão os testes que buscam "Join" no singular
   * quebram.
   */
  it("não interfere no botão Join singular", async () => {
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    expect(screen.getByRole("button", { name: "Join" })).toBeInTheDocument();
    expect(screen.queryAllByRole("button", { name: /Join/i })).toHaveLength(1);
  });

  it("mostra o Job ID completo no title, para o que a coluna truncar", async () => {
    renderTab([row({ id: "job-completo-1234567890" })]);
    await screen.findByText("job-completo-1234567890");

    const jobIdButton = screen.getByRole("button", { name: "Copy Job ID" });
    expect(jobIdButton.title).toContain("job-completo-1234567890");
  });
});

describe("ServersTab — região", () => {
  it("resolve a região só quando pedido, em lote, e mostra o rótulo", async () => {
    const user = userEvent.setup();
    renderTab([row({ id: "job-a" }), row({ id: "job-b" })]);
    await screen.findByText("job-a");

    // Nada de região sem o usuário pedir: é uma chamada de join por servidor.
    expect(callsFor("get_server_regions")).toHaveLength(0);

    setInvokeMap({
      start_server_scan: SCAN_ID,
      get_server_regions: [region("job-a", "BR", "São Paulo"), region("job-b", "US", "Ashburn")],
    });
    await user.click(screen.getByRole("button", { name: /Check servers/i }));

    await screen.findByText("São Paulo, BR");
    expect(screen.getByText("Ashburn, US")).toBeInTheDocument();

    const args = (callsFor("get_server_regions")[0][1] ?? {}) as Record<string, unknown>;
    expect(args.jobIds).toEqual(["job-a", "job-b"]);
    expect(args.userId).toBe(1001);
  });

  /**
   * A mesma chamada da região já traz a recusa do Roblox (o erro 524, "You do
   * not have permission to join this experience"). Relato do dono
   * (28/09/2026): as alts só descobriam isso tentando entrar. A linha diz, e o
   * Entrar daquele servidor fica desativado.
   */
  it("marca o servidor que recusa a conta e desativa o Entrar dele", async () => {
    const user = userEvent.setup();
    renderTab([row({ id: "job-ok" }), row({ id: "job-negado" })]);
    await screen.findByText("job-negado");

    setInvokeMap({
      start_server_scan: SCAN_ID,
      get_server_regions: [
        region("job-ok", "BR", "São Paulo"),
        {
          jobId: "job-negado",
          region: null,
          label: "",
          error: "You do not have permission to join this experience.",
          denied: true,
        },
      ],
    });
    await user.click(screen.getByRole("button", { name: /Check servers/i }));

    const negado = (await screen.findByText("No permission")).closest("li") as HTMLElement;
    expect(within(negado).getByText("job-negado")).toBeInTheDocument();
    expect(within(negado).getByRole("button", { name: "Join" })).toBeDisabled();

    const liberado = screen.getByText("job-ok").closest("li") as HTMLElement;
    expect(within(liberado).queryByText("No permission")).not.toBeInTheDocument();
    expect(within(liberado).getByRole("button", { name: "Join" })).toBeEnabled();
  });

  it("filtra a lista pelo país assim que a região é resolvida", async () => {
    const user = userEvent.setup();
    const rows = [row({ id: "job-br" }), row({ id: "job-us" })];
    renderTab(rows, { serverRegionFilter: "BR" });

    // Antes de resolver, os dois aparecem: esconder o que ainda é desconhecido
    // faria parecer que não existe servidor no país.
    await screen.findByText("job-br");
    expect(screen.getByText("job-us")).toBeInTheDocument();

    setInvokeMap({
      start_server_scan: SCAN_ID,
      get_server_regions: [region("job-br", "BR"), region("job-us", "US")],
    });
    await user.click(screen.getByRole("button", { name: /Check servers/i }));

    await screen.findByText("Cidade, BR");
    expect(screen.queryByText("job-us")).not.toBeInTheDocument();
  });

  it("guarda no store o país escolhido no filtro", async () => {
    const user = userEvent.setup();
    const { store } = renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    await user.selectOptions(screen.getByLabelText("Region"), "BR");
    expect(store.setServerRegionFilter).toHaveBeenCalledWith("BR");
  });

  it("explica quando o filtro não deixou nenhum servidor visível", async () => {
    const user = userEvent.setup();
    const rows = [row({ id: "job-us" })];
    renderTab(rows, { serverRegionFilter: "BR" });
    await screen.findByText("job-us");

    setInvokeMap({
      start_server_scan: SCAN_ID,
      get_server_regions: [region("job-us", "US")],
    });
    await user.click(screen.getByRole("button", { name: /Check servers/i }));

    await screen.findByText(/No server matched BR/i);
  });

  it("mostra o erro do Roblox quando o join daquele servidor não devolve endereço", async () => {
    const user = userEvent.setup();
    const rows = [row({ id: "job-cheio" })];
    renderTab(rows);
    await screen.findByText("job-cheio");

    setInvokeMap({
      start_server_scan: SCAN_ID,
      get_server_regions: [
        { jobId: "job-cheio", region: null, label: "", error: "This game is full" },
      ],
    });
    await user.click(screen.getByRole("button", { name: /Check servers/i }));

    await screen.findByText("This game is full");
  });

  it("avisa em vez de chamar o backend quando não há conta selecionada", async () => {
    const user = userEvent.setup();
    const { store } = renderTab([row({ id: "job-a" })], {}, []);

    // Sem conta não dá para listar nem resolver região.
    await waitFor(() => expect(screen.getByRole("button", { name: /Check servers/i })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: /Check servers/i }));

    expect(callsFor("get_server_regions")).toHaveLength(0);
    expect(store.addToast).toHaveBeenCalled();
  });
});

describe("ServersTab — servidores repetidos", () => {
  /**
   * A lista do Roblox se mexe entre uma página e outra, então o mesmo Job ID
   * volta em páginas diferentes — nos dados reais do jogo do relato, 50
   * repetidos em 400. Cada repetido virava uma **chave duplicada** no React,
   * que então parava de reordenar a tabela: a lista subia os servidores bons e
   * travava no meio do caminho.
   */
  it("descarta o servidor repetido, mantendo o primeiro", () => {
    const rows = [
      row({ id: "a", playing: 1 }),
      row({ id: "b", playing: 2 }),
      row({ id: "a", playing: 9 }),
    ];
    const out = dedupeRows(rows);
    expect(out.map((r) => r.id)).toEqual(["a", "b"]);
    expect(out[0].playing).toBe(1);
  });

  it("descarta linha sem id", () => {
    expect(dedupeRows([row({ id: "" }), row({ id: "ok" })]).map((r) => r.id)).toEqual(["ok"]);
  });

  it("a ordenação nunca devolve o mesmo servidor duas vezes", () => {
    const rows = [
      row({ id: "cheio", playing: 12, maxPlayers: 13 }),
      row({ id: "cabe", playing: 2, maxPlayers: 13 }),
      row({ id: "cheio", playing: 12, maxPlayers: 13 }),
    ];
    expect(rankRows(rows, 6).map((r) => r.id)).toEqual(["cabe", "cheio"]);
  });

  it("renderiza uma linha por servidor mesmo com repetidos no evento", async () => {
    renderTab([]);
    emitScan([
      row({ id: "dup", playing: 2, maxPlayers: 13 }),
      row({ id: "dup", playing: 2, maxPlayers: 13 }),
      row({ id: "outro", playing: 3, maxPlayers: 13 }),
    ]);

    await screen.findByText("dup");
    expect(screen.getAllByText("dup")).toHaveLength(1);
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
  });
});

describe("ServersTab — lista cortada", () => {
  /**
   * O caso que o usuário viu: o resumo diz que existem servidores que cabem,
   * mas nenhum deles está na página recebida. A tela não pode deixar parecer
   * que o topo serve.
   */
  it("avisa quando os servidores que cabem não chegaram nesta página", async () => {
    renderTab([]);
    emitScan(
      Array.from({ length: 5 }, (_, i) => row({ id: `cheio-${i}`, playing: 12, maxPlayers: 13 })),
      { scanned: 1700, fitting: 68, done: true }
    );

    expect(await screen.findByText(/The ones that fit are not in this page yet/i)).toBeInTheDocument();
  });

  it("não avisa nada quando os que cabem estão na lista", async () => {
    renderTab([]);
    emitScan([row({ id: "cabe", playing: 2, maxPlayers: 13 })], {
      scanned: 1700,
      fitting: 68,
      done: true,
    });

    await screen.findByText("cabe");
    expect(screen.queryByText(/not in this page yet/i)).not.toBeInTheDocument();
  });
});

describe("ServersTab — profundidade da varredura", () => {
  it("manda o limite de páginas escolhido para a varredura", async () => {
    renderTab([row({ id: "job-a" })], { serverScanPages: 80 });
    await screen.findByText("job-a");

    const args = (callsFor("start_server_scan")[0][1] ?? {}) as Record<string, unknown>;
    expect(args.maxPages).toBe(80);
  });

  it("guarda no store o novo limite digitado", async () => {
    const user = userEvent.setup();
    const { store } = renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    const field = screen.getByLabelText("Pages to scan");
    await user.clear(field);
    await user.type(field, "60");
    expect(store.setServerScanPages).toHaveBeenCalled();
  });

  /**
   * Parar no limite não é "acabaram os servidores": o jogo tem mais, e o
   * usuário precisa de um jeito óbvio de continuar procurando.
   */
  it("oferece dobrar o limite quando a varredura para por causa dele", async () => {
    const user = userEvent.setup();
    const { store } = renderTab([row({ id: "job-a" })], { serverScanPages: 30 });
    emitScan([row({ id: "job-a" })], { scanned: 3000, fitting: 0, done: true, stoppedAtLimit: true });

    await screen.findByText(/Stopped after 30 pages/i);
    await user.click(screen.getByRole("button", { name: /Scan 60 pages/i }));
    expect(store.setServerScanPages).toHaveBeenCalledWith(60);
  });

  it("não oferece nada quando a varredura acabou por falta de servidores", async () => {
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");
    expect(screen.queryByText(/Stopped after/i)).not.toBeInTheDocument();
  });
});

describe("ServersTab — preferência", () => {
  it("guarda a preferência escolhida no store", async () => {
    const user = userEvent.setup();
    const { store } = renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    await user.selectOptions(screen.getByLabelText("Server preference"), "fullest");
    expect(store.setServerPreference).toHaveBeenCalledWith("fullest");
  });

  it("mostra o erro do backend em vez de uma lista vazia sem explicação", async () => {
    setInvokeMap({ start_server_scan: SCAN_ID });
    renderWithStore(
      <ServersTab
        userIds={[1001]}
        placeId="606849621"
        setPlaceId={setPlaceId}
        launchAll={launchAll}
      />,
      { accounts: [ACCOUNT_A], selectedIds: new Set([1001]), selectedAccounts: [ACCOUNT_A] }
    );

    await waitFor(() => expect(callsFor("start_server_scan")).toHaveLength(1));
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });
});

describe("ServersTab — lista vazia", () => {
  it("diz que o place não tem servidor público", async () => {
    renderTab([]);
    await screen.findByText(/No public server was found/i);
  });
});

describe("ServersTab — place inválido", () => {
  it("não chama o backend com um Place ID vazio", async () => {
    renderWithStore(
      <ServersTab userIds={[1001]} placeId="" setPlaceId={setPlaceId} launchAll={launchAll} />,
      { accounts: [ACCOUNT_A], selectedIds: new Set([1001]), selectedAccounts: [ACCOUNT_A] }
    );

    await screen.findByText(/Enter a Place ID to list its servers/i);
    expect(callsFor("start_server_scan")).toHaveLength(0);
  });

  it("aceita dígitos digitados no campo de Place ID", async () => {
    const user = userEvent.setup();
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    await user.type(screen.getByLabelText("Place ID"), "1");
    expect(setPlaceId).toHaveBeenCalledWith("6068496211");
  });

  /**
   * Colar a URL do jogo juntava **todos** os dígitos da URL: o código do
   * servidor privado virava parte do place (`60684962198765`) e a busca ia
   * atrás de um place que não existe, sem nenhum aviso.
   */
  it("tira o place da URL colada em vez de juntar os dígitos dela", async () => {
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    fireEvent.change(screen.getByLabelText("Place ID"), {
      target: {
        value: "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=98765",
      },
    });
    expect(setPlaceId).toHaveBeenLastCalledWith("606849621");
  });

  /** Link de convite não carrega place — quem resolve isso é a aba Follow. */
  it("recusa link de convite e manda para a aba Follow", async () => {
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    fireEvent.change(screen.getByLabelText("Place ID"), {
      target: { value: "https://www.roblox.com/share?code=abc123&type=Server" },
    });
    expect(setPlaceId).not.toHaveBeenCalled();
    expect(await screen.findByText(/Follow tab/i)).toBeInTheDocument();
  });
});

describe("ServersTab — região sem conta", () => {
  it("mantém o botão de região desabilitado enquanto não há servidor listado", async () => {
    renderTab([]);
    await screen.findByText(/No public server was found/i);
    expect(screen.getByRole("button", { name: /Check servers/i })).toBeDisabled();
  });
});

/**
 * Um Place ID de 10 dígitos não diz a ninguém que jogo é aquele. A aba manda em
 * todas as contas selecionadas de uma vez: entrar no jogo errado por causa de um
 * número copiado torto é o erro caro que essa identificação evita.
 */
describe("ServersTab — qual jogo é este place", () => {
  function knowsJailbreak() {
    setInvokeMap({
      start_server_scan: SCAN_ID,
      stop_server_scan: null,
      get_server_regions: [],
      batched_get_game_info: {
        placeId: 606849621,
        universeId: 245662005,
        name: "Jailbreak",
        iconUrl: "https://tr.rbxcdn.com/jailbreak.png",
      },
    });
  }

  it("mostra nome e ícone do jogo ao lado do campo de Place ID", async () => {
    renderTab([row({ id: "job-a" })]);
    knowsJailbreak();

    expect(await screen.findByText("Jailbreak", {}, { timeout: 3000 })).toBeInTheDocument();
    const badge = screen.getByTestId("game-badge");
    expect(badge.querySelector("img")).toHaveAttribute(
      "src",
      "https://tr.rbxcdn.com/jailbreak.png"
    );
    expect(callsFor("batched_get_game_info")[0][1]).toEqual({
      placeId: 606849621,
      userId: ACCOUNT_A.UserID,
    });
  });

  it("não inventa nome quando o backend não sabe que jogo é", async () => {
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(screen.queryByTestId("game-badge")).not.toBeInTheDocument();
    expect(screen.queryByText(/Place 606849621/)).not.toBeInTheDocument();
  });
});
