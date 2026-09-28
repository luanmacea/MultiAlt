import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { StatusBar, generatorChipTooltip } from "./StatusBar";
import {
  defaultSettings,
  groupAccounts,
  makeAccount,
  makeBottingStatus,
  makeGeneratorStatus,
  setStore,
} from "../../test-utils/renderWithStore";
import { TONE_STYLES } from "../../utils/toastTone";
import type { StoreValue } from "../../store";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "ann" }),
  makeAccount({ UserID: 2, Username: "bob" }),
  makeAccount({ UserID: 3, Username: "cid" }),
];

function renderBar(overrides: Partial<StoreValue> = {}) {
  setStore({ accounts: ACCOUNTS, ...overrides });
  render(<StatusBar />);
}

/** Reads the text of the element that directly holds `value`. */
function lineWith(value: string): string {
  const el = screen.getAllByText(value)[0];
  return (el.parentElement?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** Texto do contador que termina exatamente com `label` (ex.: "2 in game"). */
function counter(label: string): string {
  const exact = new RegExp(String.raw`^\d+ ${label}$`);
  const matches = screen.getAllByText((_, node) =>
    exact.test((node?.textContent ?? "").replace(/\s+/g, " ").trim())
  );
  // O span do contador e o unico com esse texto exato; ancestrais tem mais.
  return (matches[0]?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** Texto do indicador do gerador, pedaco a pedaco, na ordem em que aparece. */
function generatorLine(): string {
  const host = screen.getByText("generating").closest("button");
  return [...(host?.querySelectorAll("span") ?? [])]
    .map((span) => (span.textContent ?? "").replace(/\s+/g, " ").trim())
    .filter(Boolean)
    .join(" ");
}

/** Liga ShowPresence e monta a barra com o mapa de presenca informado. */
function renderPresence(presence: Array<[number, number]>) {
  const settings = defaultSettings();
  settings.General.ShowPresence = "true";
  renderBar({ settings, presenceByUserId: new Map(presence) });
}

afterEach(cleanup);

describe("StatusBar", () => {
  it("shows the total account count", () => {
    renderBar();
    expect(lineWith("3")).toContain("3 accounts");
  });

  it("uses the singular form for one account", () => {
    setStore({ accounts: [ACCOUNTS[0]] });
    render(<StatusBar />);
    expect(lineWith("1")).toContain("1 account");
  });

  it("shows filtered / total while a search is active", () => {
    renderBar({ searchQuery: "an", groups: groupAccounts([ACCOUNTS[0]]) });
    expect(lineWith("1")).toContain("1 / 3 accounts");
  });

  it("shows the selected count only when something is selected", () => {
    renderBar({ selectedIds: new Set([1, 2]) });
    expect(screen.getByText("selected")).toBeInTheDocument();
    expect(lineWith("2")).toContain("2 selected");
  });

  it("hides the online/in-game counters when ShowPresence is off", () => {
    renderBar({ presenceByUserId: new Map([[1, 2]]) });
    // Only the legend entry remains — no counter.
    expect(screen.getAllByText("online")).toHaveLength(1);
    expect(screen.getAllByText("in game")).toHaveLength(1);
  });

  it("counts each presence state once, without overlap", () => {
    renderPresence([
      [1, 1],
      [2, 2],
      [3, 0],
    ]);
    // A conta em jogo nao pode ser contada tambem como online.
    expect(counter("online")).toBe("1 online");
    expect(counter("in game")).toBe("1 in game");
    expect(counter("studio")).toBe("0 studio");
  });

  it("does not fold a Studio session into the in-game counter", () => {
    renderPresence([
      [1, 3],
      [2, 3],
      [3, 2],
    ]);
    expect(counter("studio")).toBe("2 studio");
    expect(counter("in game")).toBe("1 in game");
    expect(counter("online")).toBe("0 online");
  });

  it("gives the studio legend entry a counter of its own", () => {
    renderPresence([[1, 3]]);
    expect(counter("studio")).toBe("1 studio");
  });

  it("reports how many clients this app launched", () => {
    renderBar({ launchedByProgram: new Set([1, 2]) });
    expect(lineWith("2")).toContain("2 launched");
  });

  it("stays silent about botting while it is not running", () => {
    renderBar({ bottingStatus: makeBottingStatus({ active: false }) });
    expect(screen.queryByText("auto rejoin")).not.toBeInTheDocument();
  });

  it("counts down to the next botting restart", () => {
    renderBar({
      bottingStatus: makeBottingStatus({
        active: true,
        userIds: [1],
        accounts: [
          {
            userId: 1,
            isPlayer: false,
            disconnected: false,
            phase: "waiting",
            retryCount: 0,
            nextRestartAtMs: Date.now() + 90_000,
            playerGraceUntilMs: null,
            lastError: null,
          },
        ],
      }),
    });
    expect(screen.getByText("auto rejoin")).toBeInTheDocument();
    expect(screen.getByText(/next 1:(29|30)/)).toBeInTheDocument();
  });

  it("shows a dash when botting has no scheduled restart", () => {
    renderBar({ bottingStatus: makeBottingStatus({ active: true, userIds: [1] }) });
    expect(screen.getByText("next -")).toBeInTheDocument();
  });

  it("stays silent about the generator while it is not running", () => {
    renderBar({ generatorStatus: makeGeneratorStatus({ active: false, totalGenerated: 4 }) });
    expect(screen.queryByText("generating")).not.toBeInTheDocument();
  });

  it("keeps the paid generator visible in the footer while it runs", () => {
    renderBar({
      generatorStatus: makeGeneratorStatus({
        active: true,
        provider: "acme",
        totalGenerated: 3,
        maxAccounts: 10,
        nextAttemptAtMs: Date.now() + 90_000,
      }),
    });
    expect(generatorLine()).toMatch(/^generating 3\/10 next 1:(29|30)$/);
  });

  it("shows the running total when the generator has no target", () => {
    renderBar({
      generatorStatus: makeGeneratorStatus({ active: true, totalGenerated: 2, maxAccounts: 0 }),
    });
    expect(generatorLine()).toBe("generating 2 next -");
  });

  it("reopens the generator dialog from the footer indicator", async () => {
    const store = setStore({
      accounts: ACCOUNTS,
      generatorStatus: makeGeneratorStatus({ active: true, provider: "acme" }),
    });
    render(<StatusBar />);
    await userEvent.click(screen.getByRole("button", { name: /generating/ }));
    expect(store.openGeneratorDialog).toHaveBeenCalledWith("provider");
  });

  it("always shows the status-dot legend", () => {
    renderBar();
    for (const label of ["invalid", "idle 20d+", "launched", "online", "in game", "studio"]) {
      expect(screen.getAllByText(label).length).toBeGreaterThan(0);
    }
  });

  it("names the aging criterion instead of the bare word 'aged'", () => {
    renderBar();
    expect(screen.queryByText("aged")).not.toBeInTheDocument();
    expect(screen.getByText("idle 20d+")).toBeInTheDocument();
  });
});

/**
 * `actionStatus` é o canal do "está acontecendo agora" — progresso de download,
 * conta N de M, Settings saved. Dezesseis chamadas escreviam nele e nenhum
 * componente lia: a mensagem nunca chegava à tela.
 */
describe("StatusBar — actionStatus", () => {
  const status = (overrides: Partial<NonNullable<StoreValue["actionStatus"]>> = {}) => ({
    message: "Downloading the new Roblox version...",
    tone: "info" as const,
    at: Date.now(),
    ...overrides,
  });

  it("stays out of the way while there is no action running", () => {
    renderBar();
    expect(screen.queryByTestId("action-status")).not.toBeInTheDocument();
  });

  it("shows the current action message", () => {
    renderBar({ actionStatus: status({ message: "Launching account 2/5..." }) });
    expect(screen.getByTestId("action-status")).toHaveTextContent("Launching account 2/5...");
  });

  it("paints the dot with the tone of the message", () => {
    renderBar({ actionStatus: status({ message: "New Roblox version installed", tone: "success" }) });
    expect(screen.getByTestId("action-status").className).toContain(TONE_STYLES.success.text);
    expect(
      screen.getByTestId("action-status").querySelector(`.${CSS.escape(TONE_STYLES.success.dot)}`)
    ).not.toBeNull();

    cleanup();
    renderBar({ actionStatus: status({ message: "Auto Rejoin failed for 4", tone: "warn" }) });
    expect(
      screen.getByTestId("action-status").querySelector(`.${CSS.escape(TONE_STYLES.warn.dot)}`)
    ).not.toBeNull();
  });
});

/**
 * O mesmo chip cobre as duas funcoes de criar conta: o gerador pago (que gasta
 * saldo) e a criacao gratuita no formulario do Roblox. O texto unico dizia que
 * estava gastando saldo mesmo quando o que rodava era a gratuita.
 */
describe("StatusBar — chip do gerador", () => {
  it("says the paid generator is spending the provider balance", () => {
    expect(generatorChipTooltip("bloxgen", (s: string) => s)).toMatch(/balance/);
  });

  it("does not claim the free signup flow is spending money", () => {
    const text = generatorChipTooltip("", (s: string) => s);
    expect(text).not.toMatch(/balance/);
    expect(text).toMatch(/browser/);
  });
});
