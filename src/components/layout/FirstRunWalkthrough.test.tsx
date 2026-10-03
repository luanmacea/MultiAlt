import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { FirstRunWalkthrough } from "./FirstRunWalkthrough";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";

/**
 * Abre o tour em modo "manual" (o passo de idioma ja vem liberado) e anda ate
 * o passo que explica as bolinhas da lista.
 */
async function openSignalsStep() {
  setStore({
    accounts: [makeAccount({ UserID: 1, Username: "ann" })],
    firstRunWalkthroughOpen: true,
    firstRunWalkthroughMode: "manual",
  });
  render(<FirstRunWalkthrough />);
  await userEvent.keyboard("{ArrowRight}{ArrowRight}");
  expect(await screen.findByText("Learn the account list signals")).toBeInTheDocument();
}

/** Abre o tour em modo "manual" (o passo de idioma ja vem liberado). */
function openTour(overrides: Record<string, unknown> = {}) {
  const store = setStore({
    accounts: [makeAccount({ UserID: 1, Username: "ann" })],
    firstRunWalkthroughOpen: true,
    firstRunWalkthroughMode: "manual",
    ...overrides,
  });
  render(<FirstRunWalkthrough />);
  return store;
}

/** Avanca o tour ate o passo cujo titulo casa com `title`. */
async function gotoStep(title: RegExp) {
  for (let i = 0; i < 10; i++) {
    if (screen.queryAllByText(title).length > 0) return;
    await userEvent.keyboard("{ArrowRight}");
  }
  throw new Error(`walkthrough step not reached: ${title}`);
}

beforeEach(() => {
  resetTauriMocks();
});

afterEach(cleanup);

describe("FirstRunWalkthrough — status dot lesson", () => {
  it("teaches the same colors the status bar legend draws", async () => {
    await openSignalsStep();
    const lesson = screen.getByText(/Red means invalid session/);
    const text = lesson.textContent ?? "";
    // Ambar e "launched", nao "aged" — o tour ensinava o contrario.
    expect(text).toMatch(/amber means launched/i);
    expect(text).toMatch(/orange means idle 20d\+/i);
    expect(text).toMatch(/sky.*green.*violet/i);
  });

  it("no longer calls the aging dot 'aged' without a criterion", async () => {
    await openSignalsStep();
    expect(screen.queryByText(/amber means aged/i)).not.toBeInTheDocument();
  });
});

describe("FirstRunWalkthrough — Session panel", () => {
  it("teaches the Session panel somewhere in the tour", async () => {
    openTour();
    await gotoStep(/Session panel/i);
    expect(screen.getByText(/cancel an account that is still joining/i)).toBeInTheDocument();
    expect(screen.getByText(/close a client that is already running/i)).toBeInTheDocument();
  });

  it("opens the Session page from the step's action button", async () => {
    const store = openTour();
    await gotoStep(/Session panel/i);
    await userEvent.click(screen.getByRole("button", { name: /Open Session Panel/i }));
    expect(store.setActivePage).toHaveBeenCalledWith("session");
  });

  /** A barra de ícones do topo saiu: o tour aponta o item da barra lateral. */
  it("points at the Session item in the sidebar", async () => {
    openTour();
    await gotoStep(/Session panel/i);
    expect(screen.getAllByText(/sidebar/i).length).toBeGreaterThan(0);
    expect(screen.queryByText(/top toolbar/i)).not.toBeInTheDocument();
  });
});

/**
 * Os passos da lista de contas (Add, bolinhas, painel) só têm alvo na página de
 * contas: aberto de outra página, o tour leva o usuário até lá.
 */
describe("FirstRunWalkthrough — pages", () => {
  it("brings the account page back for the account steps", async () => {
    const store = openTour({ activePage: "settings" });
    await gotoStep(/Add your first account/i);
    expect(store.setActivePage).toHaveBeenCalledWith("accounts");
  });

  it("points at the Settings item in the sidebar", async () => {
    openTour();
    await gotoStep(/Power settings/i);
    expect(screen.queryByText(/top toolbar/i)).not.toBeInTheDocument();
  });

  it("opens the Settings page from the safety step", async () => {
    const store = openTour();
    await gotoStep(/Power settings/i);
    await userEvent.click(screen.getByRole("button", { name: /Open Settings/i }));
    expect(store.setActivePage).toHaveBeenCalledWith("settings");
  });
});

describe("FirstRunWalkthrough — final step", () => {
  it("talks about the status bar it is highlighting", async () => {
    openTour();
    await gotoStep(/ready to roll/i);
    expect(screen.getByText(/status bar/i)).toBeInTheDocument();
  });

  it("goes back to the account list before finishing", async () => {
    const store = openTour({ activePage: "session" });
    await gotoStep(/ready to roll/i);
    expect(store.setActivePage).toHaveBeenLastCalledWith("accounts");
  });
});

describe("FirstRunWalkthrough — passo de idioma", () => {
  /**
   * O tour tinha o seu proprio seletor de idioma, com so `en` e `de`. Com
   * `Language=pt` o controle mostrava o literal "pt" e, em modo primeira
   * execucao, o botao Avancar fica travado ate a pessoa escolher um idioma da
   * lista — ou seja, quem estava em portugues nao passava do primeiro passo.
   */
  it("oferece os mesmos idiomas que o app suporta", async () => {
    openTour({ firstRunWalkthroughMode: "firstRun" });
    await userEvent.click(screen.getByText("English"));
    expect(screen.getByText("German")).toBeInTheDocument();
    expect(screen.getByText("Portuguese (Brazil)")).toBeInTheDocument();
  });

  it("mostra o idioma salvo em vez do codigo cru quando ele e portugues", async () => {
    openTour({
      firstRunWalkthroughMode: "firstRun",
      settings: { General: { Language: "pt" } },
    });
    expect(screen.getByText("Portuguese (Brazil)")).toBeInTheDocument();
    expect(screen.queryByText("pt")).not.toBeInTheDocument();
  });
});
