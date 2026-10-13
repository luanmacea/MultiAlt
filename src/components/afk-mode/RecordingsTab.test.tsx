import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { RecordingsTab } from "./RecordingsTab";
import { ClicksTab } from "./ClicksTab";
import type { StoreValue } from "../../store";
import { defaultSettings, makeAccount, setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import type { RecordingsPayload } from "../../recordings";

const ACCOUNTS = [
  makeAccount({ UserID: 11, Username: "alpha" }),
  makeAccount({ UserID: 22, Username: "bravo" }),
];

const KEYS = ["Space", "W", "A", "S", "D", "E"];

function library(overrides: Partial<RecordingsPayload> = {}): RecordingsPayload {
  return {
    recordings: [
      {
        id: "rec-1",
        name: "Walk forward",
        steps: [
          { type: "key", key: "W", holdMs: 800 },
          { type: "wait", ms: 200 },
          { type: "click", xPct: 37.5, yPct: 62.5 },
        ],
        createdAt: 1,
        updatedAt: 1,
      },
      { id: "rec-2", name: "Jump", steps: [{ type: "key", key: "Space", holdMs: 40 }], createdAt: 2, updatedAt: 2 },
    ],
    afterReconnect: { defaultId: null, accounts: {} },
    antiAfk: { defaultId: null, accounts: {} },
    keys: KEYS,
    ...overrides,
  };
}

let payload: RecordingsPayload;
let playing = false;

function route(extra: Record<string, (args: Record<string, unknown> | undefined) => unknown> = {}) {
  setInvokeHandler((cmd, args) => {
    if (cmd in extra) return extra[cmd](args);
    switch (cmd) {
      case "get_recordings":
        return payload;
      case "get_recording_playback":
        return { active: playing };
      case "save_recording": {
        const rec = (args as { recording: { id: string; name: string } }).recording;
        return { ...rec, id: rec.id || "rec-new", createdAt: 5, updatedAt: 5 };
      }
      default:
        return undefined;
    }
  });
}

function renderTab(overrides: Partial<StoreValue> = {}) {
  const settings = defaultSettings();
  (settings as Record<string, Record<string, string>>).Recordings = {
    AfterReconnect: "false",
    AfterReconnectDelaySeconds: "45",
  };
  const store = setStore({
    accounts: ACCOUNTS,
    launchedByProgram: new Set([11, 22]),
    afkKeys: KEYS,
    settings,
    ...overrides,
  });
  render(<RecordingsTab />);
  return { store };
}

/** Nada vem aberto: abre a gravação pela linha da biblioteca e espera o editor. */
async function openRecording(name = "Walk forward") {
  await userEvent.click(await screen.findByRole("button", { name: new RegExp(`^${name}`) }));
  return screen.findByLabelText("Recording name");
}

function calls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  payload = library();
  playing = false;
  route();
});

afterEach(cleanup);

describe("RecordingsTab — biblioteca e editor", () => {
  /**
   * A biblioteca vinha com a primeira gravação aberta, passo por passo, e
   * lotava a tela. Agora cada gravação é uma linha fechada; o editor abre só
   * quando se clica nela, e fecha clicando de novo.
   */
  it("lista as gravações fechadas; clicar abre o editor daquela, e clicar de novo fecha", async () => {
    renderTab();
    const row = await screen.findByRole("button", { name: /^Walk forward/ });
    expect(row).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByLabelText("Recording name")).not.toBeInTheDocument();
    expect(screen.queryByTestId("recording-step-1")).not.toBeInTheDocument();

    await userEvent.click(row);
    expect(await screen.findByLabelText("Recording name")).toHaveValue("Walk forward");
    expect(row).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("recording-step-1")).toHaveTextContent("W");
    expect(screen.getByTestId("recording-step-3")).toHaveTextContent("37.5% × 62.5%");
    // 800 ms segurada + 200 ms de espera + o clique estimado.
    expect(screen.getByText(/3 steps · about 1.8 s per window/)).toBeInTheDocument();

    await userEvent.click(row);
    expect(screen.queryByLabelText("Recording name")).not.toBeInTheDocument();
  });

  it("as contas e a escolha de quem toca vêm antes da biblioteca", async () => {
    renderTab();
    const slots = await screen.findByTestId("recording-slots");
    const library = screen.getByRole("region", { name: "Library" });
    expect(slots.compareDocumentPosition(library) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("acrescentar um passo deixa a gravação com mudança, e salvar manda os passos", async () => {
    const { store } = renderTab();
    await openRecording();
    const save = screen.getByRole("button", { name: "Save recording" });
    expect(save).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Wait" }));
    expect(screen.getByTestId("recording-step-4")).toBeInTheDocument();
    expect(save).toBeEnabled();
    await userEvent.click(save);

    const sent = calls("save_recording")[0][1] as { recording: { id: string; steps: unknown[] } };
    expect(sent.recording.id).toBe("rec-1");
    expect(sent.recording.steps).toHaveLength(4);
    expect(sent.recording.steps[3]).toEqual({ type: "wait", ms: 500 });
    expect(store.addToast).toHaveBeenCalledWith("Recording saved");
  });

  it("sem nome não salva, e diz por quê", async () => {
    renderTab();
    const name = await openRecording();
    await userEvent.clear(name);
    expect(screen.getByText("Give the recording a name.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save recording" })).toBeDisabled();
  });

  it("move e remove passos", async () => {
    renderTab();
    await openRecording();
    await userEvent.click(screen.getByRole("button", { name: "Move step 3 up" }));
    expect(screen.getByTestId("recording-step-2")).toHaveTextContent("37.5% × 62.5%");
    await userEvent.click(screen.getByRole("button", { name: "Remove step 1" }));
    expect(screen.queryByTestId("recording-step-3")).not.toBeInTheDocument();
  });

  it("nova gravação começa vazia e é criada ao salvar", async () => {
    renderTab();
    await openRecording();
    await userEvent.click(screen.getByRole("button", { name: "New recording" }));
    expect(screen.getByLabelText("Recording name")).toHaveValue("New recording");
    expect(screen.getByText(/No steps yet/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Key press" }));
    await userEvent.click(screen.getByRole("button", { name: "Save recording" }));
    const sent = calls("save_recording")[0][1] as { recording: { id: string; steps: unknown[] } };
    expect(sent.recording.id).toBe("");
    expect(sent.recording.steps).toEqual([{ type: "key", key: "Space", holdMs: 40 }]);
  });

  it("trocar de gravação com mudança pede confirmação", async () => {
    renderTab();
    await openRecording();
    await userEvent.click(screen.getByRole("button", { name: "Wait" }));
    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: /^Jump/ }));
    expect(confirmMock).toHaveBeenCalled();
    expect(screen.getByLabelText("Recording name")).toHaveValue("Walk forward");
  });

  it("renomear, duplicar e apagar passam pelo backend; apagar pede confirmação", async () => {
    renderTab();
    await openRecording();

    promptAnswers.prompt = "Walk back";
    await userEvent.click(screen.getByRole("button", { name: "Rename Walk forward" }));
    expect((calls("save_recording")[0][1] as { recording: { name: string } }).recording.name).toBe("Walk back");

    await userEvent.click(screen.getByRole("button", { name: "Duplicate Jump" }));
    expect(calls("duplicate_recording")[0][1]).toEqual({ id: "rec-2", name: "Jump (copy)" });

    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: "Delete Jump" }));
    expect(calls("delete_recording")).toHaveLength(0);
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Delete Jump" }));
    expect(calls("delete_recording")[0][1]).toEqual({ id: "rec-2" });
  });

  it("relê a biblioteca quando o backend avisa que mudou", async () => {
    renderTab();
    await openRecording();
    payload = library({ recordings: [...library().recordings, { id: "rec-3", name: "Spin", steps: [], createdAt: 3, updatedAt: 3 }] });
    await act(async () => emitTauriEvent("recordings-changed", null));
    expect(await screen.findByRole("button", { name: /^Spin/ })).toBeInTheDocument();
  });
});

/**
 * Pedido do dono (11/10/2026): a gravação de depois da reconexão (posicionar o
 * personagem) e a do anti-AFK (uma ação em outro lugar) são escolhas separadas.
 * Cada vaga tem a global e a escolha de cada conta, que vence.
 */
describe("RecordingsTab — qual gravação toca, por vaga", () => {
  it("escolhe a global de cada vaga na linha All accounts", async () => {
    renderTab();
    await openRecording();
    await userEvent.selectOptions(screen.getByLabelText("Global recording: After auto-reconnect"), "rec-2");
    expect(calls("set_default_recording")[0][1]).toEqual({ slot: "afterReconnect", id: "rec-2" });
    await userEvent.selectOptions(screen.getByLabelText("Global recording: On anti-AFK"), "");
    expect(calls("set_default_recording")[1][1]).toEqual({ slot: "antiAfk", id: null });
  });

  it("cada conta escolhe a própria ou nenhuma, em cada vaga", async () => {
    renderTab();
    await openRecording();
    await userEvent.selectOptions(screen.getByLabelText("After auto-reconnect for alpha"), "rec:rec-1");
    expect(calls("set_account_recording")[0][1]).toEqual({
      slot: "afterReconnect",
      userId: 11,
      choice: { kind: "recording", id: "rec-1" },
    });
    await userEvent.selectOptions(screen.getByLabelText("On anti-AFK for bravo"), "none");
    expect(calls("set_account_recording")[1][1]).toEqual({ slot: "antiAfk", userId: 22, choice: { kind: "none" } });
  });

  it("a célula herdada mostra a global, apagada; a própria fica destacada, com Use global", async () => {
    payload = library({
      afterReconnect: { defaultId: "rec-1", accounts: { "22": "rec-2" } },
      antiAfk: { defaultId: null, accounts: { "11": null } },
    });
    renderTab();
    await openRecording();
    const inherited = screen.getByLabelText("After auto-reconnect for alpha") as HTMLSelectElement;
    expect(inherited).toHaveAttribute("data-own", "false");
    expect(inherited.selectedOptions[0]).toHaveTextContent("Global: Walk forward");
    expect(screen.queryByRole("button", { name: "Use global for After auto-reconnect on alpha" })).not.toBeInTheDocument();

    const own = screen.getByLabelText("After auto-reconnect for bravo") as HTMLSelectElement;
    expect(own).toHaveAttribute("data-own", "true");
    expect(own.selectedOptions[0]).toHaveTextContent("Jump");
    // "None" escolhido é escolha da conta, mesmo sem global.
    const none = screen.getByLabelText("On anti-AFK for alpha") as HTMLSelectElement;
    expect(none).toHaveAttribute("data-own", "true");
    expect(none.selectedOptions[0]).toHaveTextContent("None for this account");
    expect((screen.getByLabelText("On anti-AFK for bravo") as HTMLSelectElement).selectedOptions[0]).toHaveTextContent(
      "Global: none"
    );

    await userEvent.click(screen.getByRole("button", { name: "Use global for After auto-reconnect on bravo" }));
    expect(calls("set_account_recording")[0][1]).toEqual({
      slot: "afterReconnect",
      userId: 22,
      choice: { kind: "inherit" },
    });
  });

  /**
   * Achado no harness (11/10/2026): a lista é controlada pelo arquivo, que só
   * volta depois do `recordings-changed`; descendo as opções com a seta, ela
   * voltava à escolha antiga entre uma tecla e outra e a seta seguinte mandava
   * a mesma escolha de novo. A escolha que acabou de sair fica na tela.
   */
  it("a escolha feita fica na lista enquanto o arquivo não volta, e a seguinte parte dela", async () => {
    renderTab();
    await openRecording();
    const cell = screen.getByLabelText("On anti-AFK for alpha") as HTMLSelectElement;
    await userEvent.selectOptions(cell, "none");
    expect(cell.value).toBe("none");
    expect(cell).toHaveAttribute("data-own", "true");
    await userEvent.selectOptions(cell, "rec:rec-1");
    expect(cell.value).toBe("rec:rec-1");
    expect(calls("set_account_recording").map((c) => (c[1] as { choice: unknown }).choice)).toEqual([
      { kind: "none" },
      { kind: "recording", id: "rec-1" },
    ]);
  });

  it("gravação recusada pelo backend devolve a lista à escolha salva", async () => {
    route({ set_account_recording: () => Promise.reject("That recording no longer exists.") });
    const { store } = renderTab();
    await openRecording();
    const cell = screen.getByLabelText("After auto-reconnect for bravo") as HTMLSelectElement;
    await userEvent.selectOptions(cell, "rec:rec-2");
    await waitFor(() => expect(cell.value).toBe("inherit"));
    expect(store.addToast).toHaveBeenCalledWith("That recording no longer exists.", "error");
  });
  it("lista as contas com cliente aberto e as com escolha própria em qualquer vaga", async () => {
    payload = library({ antiAfk: { defaultId: null, accounts: { "33": null } } });
    renderTab({
      accounts: [...ACCOUNTS, makeAccount({ UserID: 33, Username: "charlie" }), makeAccount({ UserID: 44, Username: "delta" })],
      launchedByProgram: new Set([11]),
    });
    await openRecording();
    const area = screen.getByTestId("recording-slots");
    expect(within(area).getByTestId("recording-slots-11")).toBeInTheDocument();
    expect(within(area).getByTestId("recording-slots-33")).toHaveTextContent("no client open");
    expect(within(area).queryByTestId("recording-slots-22")).not.toBeInTheDocument();
    expect(within(area).queryByTestId("recording-slots-44")).not.toBeInTheDocument();
  });

  it("sem gravação na biblioteca, a área diz o que fazer primeiro", async () => {
    payload = library({ recordings: [] });
    renderTab();
    const area = await screen.findByTestId("recording-slots");
    expect(area).toHaveTextContent("Create or import a recording first.");
    expect(within(area).queryByRole("combobox")).not.toBeInTheDocument();
  });
  it("tocar depois da reconexão grava no INI, com o tempo no jogo", async () => {
    const { store } = renderTab();
    await openRecording();
    expect(screen.getByLabelText("Seconds in the game before playing")).toHaveValue("45");
    await userEvent.click(screen.getByRole("button", { name: "Play after an automatic reconnect" }));
    expect(store.updateSetting).toHaveBeenCalledWith("Recordings", "AfterReconnect", "true");
  });

  /**
   * Pedido do dono: o Modo AFK repete a gravação de minutos em minutos — e isso
   * se liga daqui também, sem ter de achar "What to send" na outra aba.
   */
  /**
   * A aba Recordings tinha o seu próprio "Repeat in AFK mode" (com intervalo),
   * a mesma configuração do "What to send" da AFK clicks: configuração
   * duplicada. Agora ela só diz o estado e aponta para a AFK clicks.
   */
  it("o anti-AFK não se liga daqui: só diz o estado e aponta para AFK clicks", async () => {
    renderTab();
    await screen.findByTestId("recording-slots");
    expect(screen.queryByRole("switch", { name: "Repeat in AFK mode" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Repeat every: minutes")).not.toBeInTheDocument();
    expect(screen.getByTestId("recordings-afk-state")).toHaveTextContent(
      "Off. To play the anti-AFK recording on every AFK turn, pick Play the recording under What to send in AFK clicks."
    );
  });

  it("com o modo gravação nos AFK clicks, diz o intervalo de lá", async () => {
    const settings = defaultSettings() as Record<string, Record<string, string>>;
    settings.Afk = { Mode: "recording", IntervalMinutes: "2", IntervalSeconds: "30" };
    settings.Recordings = { AfterReconnect: "false", AfterReconnectDelaySeconds: "45" };
    renderTab({ settings: settings as never });
    expect(await screen.findByTestId("recordings-afk-state")).toHaveTextContent(
      "On: while AFK mode runs, each account plays its anti-AFK recording every 2 min 30 s (set in AFK clicks)."
    );
  });

  it("a gravação aberta diz para quem toca em cada vaga e vira a global de uma vaga num clique", async () => {
    payload = library({
      afterReconnect: { defaultId: "rec-1", accounts: {} },
      antiAfk: { defaultId: null, accounts: { "22": "rec-1" } },
    });
    renderTab();
    await openRecording();
    const box = screen.getByTestId("recording-use");
    expect(box).toHaveTextContent("After auto-reconnect: global, for every account without its own choice.");
    expect(box).toHaveTextContent("On anti-AFK: own choice of 1 account(s).");
    expect(
      within(box).queryByRole("button", { name: "Make Walk forward the global recording for After auto-reconnect" })
    ).not.toBeInTheDocument();
    await userEvent.click(
      within(box).getByRole("button", { name: "Make Walk forward the global recording for On anti-AFK" })
    );
    expect(calls("set_default_recording")[0][1]).toEqual({ slot: "antiAfk", id: "rec-1" });
  });

  it("gravação que nenhuma vaga usa diz isso", async () => {
    renderTab();
    await openRecording();
    const box = screen.getByTestId("recording-use");
    expect(box).toHaveTextContent("After auto-reconnect: not used.");
    expect(box).toHaveTextContent("On anti-AFK: not used.");
  });
});

describe("RecordingsTab — tocar agora e parar", () => {
  it("toca a gravação salva nas contas marcadas", async () => {
    route({ play_recording_now: () => [{ userId: 11, errorCode: null, error: null }] });
    const { store } = renderTab();
    await openRecording();
    const play = screen.getByRole("button", { name: "Play now" });
    expect(play).toBeDisabled();

    const tryIt = screen.getByText("Try it now").parentElement as HTMLElement;
    await userEvent.click(within(tryIt).getByRole("button", { name: "alpha" }));
    await userEvent.click(play);
    expect(calls("play_recording_now")[0][1]).toEqual({ userIds: [11], recordingId: "rec-1" });
    expect(store.addToast).toHaveBeenCalledWith("Played on 1 account");
  });

  it("com mudança sem salvar, não toca", async () => {
    renderTab();
    await openRecording();
    const tryIt = screen.getByText("Try it now").parentElement as HTMLElement;
    await userEvent.click(within(tryIt).getByRole("button", { name: "alpha" }));
    await userEvent.click(screen.getByRole("button", { name: "Wait" }));
    expect(screen.getByRole("button", { name: "Play now" })).toBeDisabled();
    expect(screen.getByText(/Not saved yet: pick exactly one account to test these steps/)).toBeInTheDocument();
  });

  it("diz por que uma conta não recebeu a gravação", async () => {
    route({ play_recording_now: () => [{ userId: 11, errorCode: "focusLost", error: "x" }] });
    renderTab();
    await openRecording();
    const tryIt = screen.getByText("Try it now").parentElement as HTMLElement;
    await userEvent.click(within(tryIt).getByRole("button", { name: "alpha" }));
    await userEvent.click(screen.getByRole("button", { name: "Play now" }));
    expect(
      await screen.findByText("alpha: another window came to the front, so the rest of the recording was not played.")
    ).toBeInTheDocument();
  });

  it("tocando, mostra o estado e o Parar, que não fecha nada", async () => {
    playing = true;
    renderTab();
    const stop = await screen.findByRole("button", { name: "Stop playing" });
    expect(screen.getByTestId("recordings-status")).toHaveAttribute("data-running", "true");
    await userEvent.click(stop);
    expect(calls("stop_recording_playback")).toHaveLength(1);
    expect(calls("cmd_kill_roblox")).toHaveLength(0);

    await act(async () => emitTauriEvent("recording-playback", { active: false }));
    await waitFor(() => expect(screen.getByTestId("recordings-status")).toHaveAttribute("data-running", "false"));
  });

  it("explica o preço: a janela fica na frente pela gravação inteira", async () => {
    renderTab();
    await openRecording();
    expect(screen.getByText(/Each window comes to the front in turn and plays the whole recording/)).toBeInTheDocument();
  });
});

/**
 * Importar do TinyTask (decisão do dono, 11/10/2026: é o jeito de criar
 * gravações). O arquivo e a janela em que ele foi gravado; o backend converte;
 * o rascunho abre no editor com o resumo, pode ser testado numa conta e só
 * então salvo.
 */
describe("RecordingsTab — importar do TinyTask", () => {
  const IMPORTED = {
    steps: [
      { type: "click", xPct: 53.11, yPct: 17.65 },
      { type: "wait", ms: 141 },
      { type: "key", key: "W", holdMs: 80 },
    ],
    summary: {
      events: 175,
      skippedKeys: [{ key: "Enter", count: 2 }],
      clicksOutside: 1,
      otherMouse: 0,
      cappedWaits: 1,
      drags: 4,
      truncated: false,
      stopKey: "F8",
    },
    sourceAspect: 1.7778,
  };
  const LEFT_WINDOW = { left: -1920, top: 0, width: 1920, height: 1080 };

  async function importSample(file = new File([new Uint8Array([1, 2, 3, 4])], "farm loop.rec"), name?: string) {
    await userEvent.click(await screen.findByRole("button", { name: "Import from TinyTask (.rec)" }));
    const panel = screen.getByTestId("tinytask-import");
    await userEvent.upload(within(panel).getByLabelText("TinyTask file"), file);
    if (name !== undefined) {
      const field = within(panel).getByLabelText("Name of the imported recording");
      await userEvent.clear(field);
      await userEvent.type(field, name);
    }
    await userEvent.click(within(panel).getByRole("button", { name: "Window it was recorded in" }));
    await userEvent.click(within(panel).getByRole("button", { name: "bravo" }));
    await userEvent.click(within(panel).getByRole("button", { name: "Import" }));
  }

  /**
   * O botão Import ficava apagado até achar o "Escolher arquivo" sem destaque,
   * e a gravação importada abria inteira no editor, sem salvar. Agora o
   * arquivo se escolhe por um botão, o nome vem do arquivo e pode mudar, e o
   * Import já salva: a gravação aparece fechada, com um resumo curto.
   */
  it("o Import converte com a janela escolhida e já salva com o nome dado, fechada e com resumo", async () => {
    route({
      recording_window_area: () => LEFT_WINDOW,
      import_tinytask_recording: () => IMPORTED,
      // Como o backend: a gravação salva entra na biblioteca relida.
      save_recording: (args) => {
        const rec = (args as { recording: { name: string; steps: never[] } }).recording;
        const out = { ...rec, id: "rec-new", createdAt: 5, updatedAt: 5 };
        payload = { ...payload, recordings: [...payload.recordings, out] };
        return out;
      },
    });
    renderTab();
    await userEvent.click(await screen.findByRole("button", { name: "Import from TinyTask (.rec)" }));
    const panel = screen.getByTestId("tinytask-import");
    expect(within(panel).getByRole("button", { name: "Choose a .rec file" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Import" })).toBeDisabled();
    await userEvent.click(within(panel).getByRole("button", { name: "Cancel" }));

    await importSample(undefined, "Farm route");

    expect(calls("recording_window_area")[0][1]).toEqual({ userId: 22 });
    expect(calls("import_tinytask_recording")[0][1]).toEqual({ bytes: [1, 2, 3, 4], area: LEFT_WINDOW });
    await waitFor(() => expect(calls("save_recording")).toHaveLength(1));
    const sent = calls("save_recording")[0][1] as { recording: Record<string, unknown> };
    expect(sent.recording).toMatchObject({ id: "", name: "Farm route", sourceAspect: 1.7778 });
    expect(sent.recording.steps).toEqual(IMPORTED.steps);

    // Fechada: nada do editor na tela, só o resumo curto da importação.
    expect(screen.queryByLabelText("Recording name")).not.toBeInTheDocument();
    expect(screen.queryByTestId("tinytask-import")).not.toBeInTheDocument();
    const summary = await screen.findByTestId("tinytask-summary");
    expect(summary).toHaveTextContent("Imported 3 steps from 175 TinyTask events.");
    // O detalhe do que ficou de fora está lá, recolhido.
    expect(summary).toHaveTextContent("Keys left out (not in the recordings' key list): Enter ×2.");
    expect(summary).toHaveTextContent("The key that stopped the TinyTask recording (F8) was left out.");
  });

  it("o nome do arquivo vira o nome sugerido", async () => {
    route({
      recording_window_area: () => LEFT_WINDOW,
      import_tinytask_recording: () => IMPORTED,
    });
    renderTab();
    await importSample();
    await waitFor(() => expect(calls("save_recording")).toHaveLength(1));
    expect((calls("save_recording")[0][1] as { recording: { name: string } }).recording.name).toBe("farm loop");
  });

  it("com uma janela aberta só, não pergunta a janela", async () => {
    renderTab({ launchedByProgram: new Set([11]) });
    await userEvent.click(await screen.findByRole("button", { name: "Import from TinyTask (.rec)" }));
    const panel = screen.getByTestId("tinytask-import");
    expect(within(panel).queryByRole("button", { name: "Window it was recorded in" })).not.toBeInTheDocument();
  });

  it("testa mudanças ainda não salvas numa conta só", async () => {
    route({ play_recording_draft: () => [{ userId: 11, errorCode: null, error: null }] });
    const { store } = renderTab();
    await openRecording();
    await userEvent.click(screen.getByRole("button", { name: "Wait" }));

    const test = screen.getByRole("button", { name: "Test on one account" });
    expect(test).toBeDisabled();
    const tryIt = screen.getByText("Try it now").parentElement as HTMLElement;
    await userEvent.click(within(tryIt).getByRole("button", { name: "alpha" }));
    expect(screen.getByRole("button", { name: "Play now" })).toBeDisabled();
    await userEvent.click(test);

    const sent = calls("play_recording_draft")[0][1] as { userId: number; recording: { steps: unknown[] } };
    expect(sent.userId).toBe(11);
    expect(sent.recording.steps).toHaveLength(4);
    expect(calls("save_recording")).toHaveLength(0);
    expect(store.addToast).toHaveBeenCalledWith("Played on 1 account");

    // Duas contas marcadas: o teste é numa só.
    await userEvent.click(within(tryIt).getByRole("button", { name: "bravo" }));
    expect(test).toBeDisabled();
  });

  it("o erro do arquivo vira frase, e nada é salvo", async () => {
    route({
      recording_window_area: () => LEFT_WINDOW,
      import_tinytask_recording: () => Promise.reject("notTinyTask"),
    });
    renderTab();
    // Extensão certa, conteúdo que não é do TinyTask: quem recusa é o backend.
    await importSample(new File([new Uint8Array([9])], "renamed photo.rec"));
    expect(await screen.findByText("This file is not a TinyTask recording (.rec).")).toBeInTheDocument();
    expect(calls("save_recording")).toHaveLength(0);
    expect(screen.queryByTestId("tinytask-summary")).not.toBeInTheDocument();
  });

  it("sem cliente aberto, pede para abrir a conta da janela gravada", async () => {
    renderTab({ launchedByProgram: new Set<number>() });
    await userEvent.click(await screen.findByRole("button", { name: "Import from TinyTask (.rec)" }));
    expect(screen.getByText(/Open the account whose Roblox window you recorded in first/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import" })).toBeDisabled();
  });

  /**
   * Os cliques são porcentagens da janela: numa janela de outro formato eles
   * caem em outro lugar. A tela avisa (e ainda deixa tocar).
   */
  it("avisa a conta cuja janela tem outro formato que a da gravação", async () => {
    payload = library({
      recordings: [
        { ...library().recordings[0], sourceAspect: 1.7778 },
        library().recordings[1],
      ],
    });
    route({
      recording_window_area: (args) =>
        (args as { userId: number }).userId === 11
          ? { left: 0, top: 0, width: 800, height: 600 }
          : { left: 0, top: 0, width: 1280, height: 720 },
    });
    renderTab();
    await openRecording();
    expect(await screen.findByTestId("recording-shape-warning-11")).toHaveTextContent(
      "alpha: this window has a different shape from the one the recording was made in"
    );
    expect(screen.queryByTestId("recording-shape-warning-22")).not.toBeInTheDocument();
    const tryIt = screen.getByText("Try it now").parentElement as HTMLElement;
    await userEvent.click(within(tryIt).getByRole("button", { name: "alpha" }));
    expect(screen.getByRole("button", { name: "Play now" })).toBeEnabled();
  });

  it("gravação escrita à mão (sem proporção) não lê janela nem avisa", async () => {
    renderTab();
    await openRecording();
    expect(calls("recording_window_area")).toHaveLength(0);
    expect(screen.queryByTestId(/recording-shape-warning/)).not.toBeInTheDocument();
  });
});

describe("ClicksTab — modo gravação", () => {
  function renderClicks(afk: Record<string, string>) {
    const settings = defaultSettings();
    (settings as Record<string, Record<string, string>>).Afk = { ...afk };
    const store = setStore({
      accounts: ACCOUNTS,
      launchedByProgram: new Set([11, 22]),
      afkKeys: KEYS,
      settings,
      afkStatus: null,
    });
    render(<ClicksTab targetUserIds={[11, 22]} />);
    return { store };
  }

  it("liga no modo gravação sem tecla e mostra a gravação de cada conta", async () => {
    // O Modo AFK lê a vaga "On anti-AFK"; a da reconexão não conta aqui.
    payload = library({
      antiAfk: { defaultId: "rec-1", accounts: { "22": "rec-2" } },
      afterReconnect: { defaultId: null, accounts: { "11": null } },
    });
    const { store } = renderClicks({ Mode: "recording" });
    expect(await screen.findByText("Walk forward")).toBeInTheDocument();
    expect(screen.getByText("Jump")).toBeInTheDocument();
    expect(screen.queryByLabelText("Key to send")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Start AFK Mode" }));
    expect(store.startAfkMode).toHaveBeenCalledWith(expect.objectContaining({ mode: "recording", userIds: [11, 22] }));
  });

  it("sem gravação de anti-AFK para nenhuma conta marcada, não liga e diz por quê", async () => {
    // Só a vaga da reconexão tem gravação: o Modo AFK não tem o que tocar.
    payload = library({ afterReconnect: { defaultId: "rec-1", accounts: {} } });
    renderClicks({ Mode: "recording" });
    expect(await screen.findAllByText("no recording")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Start AFK Mode" })).toBeDisabled();
    expect(screen.getByText("None of the ticked accounts has an anti-AFK recording to play")).toBeInTheDocument();
  });
});
