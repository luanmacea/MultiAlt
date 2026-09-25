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
