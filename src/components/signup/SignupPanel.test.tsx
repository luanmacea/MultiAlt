import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { SignupPanel, phaseLabel } from "./SignupPanel";
import { renderWithStore } from "../../test-utils/renderWithStore";
import {
  emitTauriEvent,
  invokeMock,
  resetTauriMocks,
  setInvokeMap,
} from "../../test-utils/tauriMocks";
import type { SignupIdentity, SignupStatus } from "../../types";

function identity(overrides: Partial<SignupIdentity> = {}): SignupIdentity {
  return {
    username: "Swift_Falcon412",
    password: "Rabcdefghijk7",
    day: "07",
    month: "Mar",
    year: "2001",
    gender: "male",
    ...overrides,
  };
}

function status(overrides: Partial<SignupStatus> = {}): SignupStatus {
  return {
    active: false,
    current: 0,
    total: 0,
    created: 0,
    phase: "idle",
    identity: null,
    lastError: null,
    createdUsernames: [],
    ...overrides,
  };
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((call) => call[0] === cmd);
}

beforeEach(() => {
  resetTauriMocks();
  setInvokeMap({ get_signup_status: status() });
});

afterEach(cleanup);

describe("SignupPanel — rótulos de fase", () => {
  const t = (s: string) => s;

  it("traduz cada fase do backend", () => {
    expect(phaseLabel("opening", t)).toMatch(/Opening/);
    expect(phaseLabel("filling", t)).toMatch(/Filling/);
    expect(phaseLabel("waiting-user", t)).toMatch(/CAPTCHA/);
    expect(phaseLabel("saving", t)).toMatch(/Saving/);
    expect(phaseLabel("done", t)).toMatch(/Finished/);
    expect(phaseLabel("error", t)).toMatch(/Error/);
    expect(phaseLabel("qualquer-coisa", t)).toMatch(/Idle/);
  });
});

describe("SignupPanel — sessão", () => {
  it("começa a sessão com a quantidade pedida", async () => {
    const user = userEvent.setup();
    setInvokeMap({
      get_signup_status: status(),
      start_signup_session: status({ active: true, total: 5, phase: "opening" }),
    });
    renderWithStore(<SignupPanel />);

    await user.click(await screen.findByRole("button", { name: "Start" }));

    await waitFor(() => expect(callsFor("start_signup_session")).toHaveLength(1));
    expect(callsFor("start_signup_session")[0][1]).toMatchObject({ count: 5 });
  });

  it("lê o estado inicial, porque a sessão pode já estar rodando", async () => {
    setInvokeMap({
      get_signup_status: status({ active: true, total: 3, created: 1, phase: "waiting-user" }),
    });
    renderWithStore(<SignupPanel />);

    expect(await screen.findByRole("button", { name: "Stop" })).toBeInTheDocument();
    expect(screen.getByText("1 of 3 created")).toBeInTheDocument();
  });

  it("acompanha o evento de progresso", async () => {
    renderWithStore(<SignupPanel />);
    await screen.findByRole("button", { name: "Start" });

    emitTauriEvent("signup-progress", status({ active: true, total: 2, phase: "filling" }));

    expect(await screen.findByText("Filling the form")).toBeInTheDocument();
  });

  it("para a sessão pelo backend", async () => {
    const user = userEvent.setup();
    setInvokeMap({ get_signup_status: status({ active: true, total: 2, phase: "waiting-user" }) });
    renderWithStore(<SignupPanel />);

    await user.click(await screen.findByRole("button", { name: "Stop" }));
    await waitFor(() => expect(callsFor("stop_signup_session")).toHaveLength(1));
  });
});

describe("SignupPanel — identidade gerada", () => {
  /**
   * A senha gerada só existe aqui: se ela não aparecer, a conta fica presa ao
   * cookie e o usuário nunca mais entra nela pelo site.
   */
  it("mostra usuário, senha e data de nascimento da conta em criação", async () => {
    setInvokeMap({
      get_signup_status: status({
        active: true,
        total: 1,
        phase: "waiting-user",
        identity: identity(),
      }),
    });
    renderWithStore(<SignupPanel />);

    expect(await screen.findByText("Swift_Falcon412")).toBeInTheDocument();
    expect(screen.getByText("Rabcdefghijk7")).toBeInTheDocument();
    expect(screen.getByText("07/Mar/2001")).toBeInTheDocument();
  });

  it("avisa que o CAPTCHA é com o usuário", async () => {
    setInvokeMap({
      get_signup_status: status({ active: true, total: 1, phase: "waiting-user", identity: identity() }),
    });
    renderWithStore(<SignupPanel />);

    expect(await screen.findByText(/Solve the CAPTCHA in the browser window/i)).toBeInTheDocument();
  });

  it("copia usuário e senha", async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    // `userEvent.setup()` instala a própria área de transferência; o dublê tem
    // que vir depois dela.
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: write },
      configurable: true,
    });
    setInvokeMap({
      get_signup_status: status({ active: true, total: 1, phase: "waiting-user", identity: identity() }),
    });
    renderWithStore(<SignupPanel />);

    await user.click(await screen.findByRole("button", { name: /Copy/i }));
    expect(write).toHaveBeenCalledWith("Swift_Falcon412\tRabcdefghijk7");
  });

  it("lista as contas já criadas e mostra o erro da última tentativa", async () => {
    setInvokeMap({
      get_signup_status: status({
        active: false,
        total: 3,
        created: 2,
        phase: "done",
        createdUsernames: ["Swift_Falcon412", "Brave_Otter88"],
        lastError: "Tempo esgotado esperando a conclusão do cadastro",
      }),
    });
    renderWithStore(<SignupPanel />);

    expect(await screen.findByText("Swift_Falcon412")).toBeInTheDocument();
    expect(screen.getByText("Brave_Otter88")).toBeInTheDocument();
    expect(screen.getByText(/Tempo esgotado/)).toBeInTheDocument();
    expect(screen.getByText("2 of 3 created")).toBeInTheDocument();
  });

  it("mostra o erro do backend ao tentar começar", async () => {
    const user = userEvent.setup();
    setInvokeMap({ get_signup_status: status() });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_signup_status") return status();
      if (cmd === "start_signup_session") throw new Error("Já existe uma criação de contas em andamento");
      return undefined;
    });
    const { store } = renderWithStore(<SignupPanel />);

    await user.click(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() => expect(store.addToast).toHaveBeenCalled());
  });
});
