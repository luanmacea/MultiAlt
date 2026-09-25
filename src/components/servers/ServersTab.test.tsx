import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { ServersTab, hasRoomFor, matchesRegion } from "./ServersTab";
import type { ServerRow } from "./ServersTab";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import {
  emitTauriEvent,
  invokeMock,
  resetTauriMocks,
  setInvokeMap,
} from "../../test-utils/tauriMocks";
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
function emitScan(rows: ServerRow[], extra: Partial<{ scanned: number; fitting: number; done: boolean }> = {}) {
  emitTauriEvent("server-scan", {
    scanId: SCAN_ID,
    placeId: 606849621,
    servers: rows,
    scanned: extra.scanned ?? rows.length,
    fitting: extra.fitting ?? rows.filter((r) => r.playing + 2 <= r.maxPlayers).length,
    done: extra.done ?? true,
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
    await user.click(screen.getByRole("button", { name: /Load regions/i }));

    await screen.findByText("São Paulo, BR");
    expect(screen.getByText("Ashburn, US")).toBeInTheDocument();

    const args = (callsFor("get_server_regions")[0][1] ?? {}) as Record<string, unknown>;
    expect(args.jobIds).toEqual(["job-a", "job-b"]);
    expect(args.userId).toBe(1001);
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
    await user.click(screen.getByRole("button", { name: /Load regions/i }));

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
    await user.click(screen.getByRole("button", { name: /Load regions/i }));

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
    await user.click(screen.getByRole("button", { name: /Load regions/i }));

    await screen.findByText("This game is full");
  });

  it("avisa em vez de chamar o backend quando não há conta selecionada", async () => {
    const user = userEvent.setup();
    const { store } = renderTab([row({ id: "job-a" })], {}, []);

    // Sem conta não dá para listar nem resolver região.
    await waitFor(() => expect(screen.getByRole("button", { name: /Load regions/i })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: /Load regions/i }));

    expect(callsFor("get_server_regions")).toHaveLength(0);
    expect(store.addToast).toHaveBeenCalled();
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

  it("aceita só dígitos no campo de Place ID", async () => {
    const user = userEvent.setup();
    renderTab([row({ id: "job-a" })]);
    await screen.findByText("job-a");

    await user.type(screen.getByLabelText("Place ID"), "a1");
    expect(setPlaceId).toHaveBeenCalledWith("6068496211");
  });
});

describe("ServersTab — região sem conta", () => {
  it("mantém o botão de região desabilitado enquanto não há servidor listado", async () => {
    renderTab([]);
    await screen.findByText(/No public server was found/i);
    expect(screen.getByRole("button", { name: /Load regions/i })).toBeDisabled();
  });
});
