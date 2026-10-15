import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

/** O Help está atrás de `ENABLE_HELP_BUTTON` (desligado); os testes ligam quando precisam. */
const helpFlag = vi.hoisted(() => ({ on: false }));
/** Grupos está atrás de `ENABLE_GROUPS` (ligado desde 13/10/2026); os testes escolhem. */
const groupsFlag = vi.hoisted(() => ({ on: false }));
vi.mock("../../featureFlags", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../featureFlags")>();
  return {
    ...actual,
    get ENABLE_HELP_BUTTON() {
      return helpFlag.on;
    },
    get ENABLE_GROUPS() {
      return groupsFlag.on;
    },
  };
});

import { NavSidebar, NAV_COLLAPSED_KEY } from "./NavSidebar";
import { makeAccount, makeBottingStatus, setStore } from "../../test-utils/renderWithStore";
import { ENABLE_NEXUS } from "../../featureFlags";
import { invokeMock } from "../../test-utils/tauriMocks";
import type { AfkStatus, StoreValue } from "../../store";

function renderNav(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ accounts: [makeAccount({ UserID: 1 }), makeAccount({ UserID: 2 })], ...overrides });
  render(<NavSidebar />);
  return store;
}

function item(name: string): HTMLElement {
  return screen.getByRole("button", { name: new RegExp(`^${name}`) });
}

function afk(active: boolean): AfkStatus {
  return {
    active,
    startedAtMs: active ? 1 : null,
    intervalSeconds: 600,
    key: "space",
    mode: "key",
    clickX: 50,
    clickY: 50,
    accounts: [],
  } as AfkStatus;
}

beforeEach(() => {
  localStorage.clear();
  helpFlag.on = false;
  groupsFlag.on = false;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/**
 * A barra de ícones do topo só dizia o que cada botão fazia com o mouse
 * parado em cima. A barra lateral mostra os nomes sempre, na ordem combinada
 * com o dono, e cada item leva a uma página.
 */
describe("NavSidebar — items", () => {
  it("is a named navigation landmark with every page, in order", () => {
    renderNav();
    const nav = screen.getByRole("navigation", { name: "Main navigation" });
    const names = within(nav)
      .getAllByRole("button")
      .map((b) => b.getAttribute("data-nav"))
      .filter(Boolean);
    // Console logo abaixo de Session: o que o app fez, ao lado do que roda agora.
    const expected = ["accounts", "session", "console", "afk", "avatars", "scripts", "theme"];
    if (ENABLE_NEXUS) expected.push("nexus");
    expected.push("settings");
    // "What's new" mora no rodapé, ao lado do recolher: é sobre o app, não trabalho do dia.
    expected.push("changelog");
    expect(names).toEqual(expected);
  });

  it("shows the labels without hovering", () => {
    renderNav();
    for (const label of ["Accounts", "Session", "Console", "AFK Mode", "Avatars", "Scripts", "Theme", "Settings", "What's new"]) {
      expect(screen.getByText(label)).not.toHaveClass("sr-only");
    }
  });

  it("marks the current page with aria-current", () => {
    renderNav({ activePage: "scripts" });
    expect(item("Scripts")).toHaveAttribute("aria-current", "page");
    expect(item("Accounts")).not.toHaveAttribute("aria-current");
  });

  it.each([
    ["Accounts", "accounts"],
    ["Session", "session"],
    ["Console", "console"],
    ["AFK Mode", "afk"],
    ["Avatars", "avatars"],
    ["Scripts", "scripts"],
    ["Theme", "theme"],
    ["Settings", "settings"],
    ["What's new", "changelog"],
  ] as const)("opens the %s page", async (label, page) => {
    const store = renderNav();
    await userEvent.click(item(label));
    expect(store.setActivePage).toHaveBeenCalledWith(page);
  });

  it.runIf(ENABLE_NEXUS)("opens the Nexus page", async () => {
    const store = renderNav();
    await userEvent.click(item("Nexus"));
    expect(store.setActivePage).toHaveBeenCalledWith("nexus");
  });

  it("hides Groups until the flag turns it on", async () => {
    renderNav();
    expect(screen.queryByText("Groups")).not.toBeInTheDocument();
    cleanup();
    groupsFlag.on = true;
    const store = renderNav();
    await userEvent.click(item("Groups"));
    expect(store.setActivePage).toHaveBeenCalledWith("groups");
  });

  /**
   * Pedido do dono (08/10/2026): "What's new" desce para a última linha, ao
   * lado do botão de recolher; "Send feedback" fica logo acima.
   */
  it("puts What's new on the last row next to the collapse button, under Send feedback", async () => {
    renderNav({ activePage: "changelog" });
    const whatsNew = item("What's new");
    expect(whatsNew).toHaveAttribute("aria-current", "page");
    const row = screen.getByTestId("nav-footer-actions");
    expect(within(row).getByRole("button", { name: /^What's new/ })).toBe(whatsNew);
    expect(within(row).getByRole("button", { name: "Collapse sidebar" })).toBeInTheDocument();
    expect(row).toHaveClass("flex", "items-center");
    const feedback = screen.getByRole("button", { name: "Send feedback" });
    expect(feedback.compareDocumentPosition(whatsNew) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(item("Settings").compareDocumentPosition(whatsNew) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // Recolhida, os dois empilham no centro.
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    const collapsedRow = screen.getByTestId("nav-footer-actions");
    expect(collapsedRow).toHaveClass("flex-col", "items-center");
    expect(within(collapsedRow).getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
  });

  /**
   * O app ganhou um servidor no Discord (12/10/2026): o botão fica logo acima
   * do "Send feedback" e abre o convite no navegador. O endereço mora no
   * backend (`open_discord_invite`), numa constante — a tela só pede.
   */
  it("has a Discord button right above Send feedback that opens the invite", async () => {
    renderNav();
    const discord = screen.getByRole("button", { name: "Discord" });
    const feedback = screen.getByRole("button", { name: "Send feedback" });
    expect(discord.compareDocumentPosition(feedback) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await userEvent.click(discord);
    expect(invokeMock).toHaveBeenCalledWith("open_discord_invite");
  });

  it("keeps What's new named when the sidebar is collapsed", async () => {
    renderNav();
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(screen.getByText("What's new")).toHaveClass("sr-only");
    expect(item("What's new")).toBeInTheDocument();
  });

  it("replays the walkthrough from Help when the flag turns it on", async () => {
    helpFlag.on = true;
    const store = renderNav();
    expect(screen.getByText("Help")).not.toHaveClass("sr-only");
    await userEvent.click(screen.getByRole("button", { name: /^Help/ }));
    expect(store.openFirstRunWalkthroughFromSettings).toHaveBeenCalledTimes(1);
  });

  /**
   * Pedido do dono (08/10/2026): o Help some por enquanto (pode virar FAQ). O
   * tutorial continua em Settings › App. O recolher fica sozinho na linha,
   * à direita com a barra aberta e no centro recolhida.
   */
  it("hides Help by default", async () => {
    renderNav();
    expect(screen.queryByRole("button", { name: /^Help/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Help")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(screen.queryByRole("button", { name: /^Help/ })).not.toBeInTheDocument();
  });

  /** O tour destaca os itens pelo `data-tour`, que não muda com o idioma. */
  it("anchors the walkthrough steps", () => {
    renderNav();
    expect(document.querySelector("[data-tour='nav-session']")).toBe(item("Session"));
    expect(document.querySelector("[data-tour='nav-settings']")).toBe(item("Settings"));
  });
});

describe("NavSidebar — live state", () => {
  it("counts the running clients on the Session item", () => {
    renderNav({ launchedByProgram: new Set([1, 2]) });
    expect(screen.getByTestId("nav-session-count")).toHaveTextContent("2");
    expect(item("Session")).toHaveAccessibleName(/2 running/);
  });

  it("shows no counter when nothing is running", () => {
    renderNav({ launchedByProgram: new Set() });
    expect(screen.queryByTestId("nav-session-count")).not.toBeInTheDocument();
  });

  it.each([
    ["AFK", { afkStatus: afk(true) }],
    ["Auto Rejoin", { bottingStatus: makeBottingStatus({ active: true }) }],
  ] as const)("flags AFK Mode while %s is on", (_label, overrides) => {
    renderNav(overrides as Partial<StoreValue>);
    expect(screen.getByTestId("nav-afk-active")).toBeInTheDocument();
    expect(item("AFK Mode")).toHaveAccessibleName(/on/i);
    // Só a luz: um "Ligado" escrito cortava o rótulo ("Modo A...") na largura da barra.
    expect(screen.getByTestId("nav-afk-active").textContent).toBe("");
  });

  it("shows no AFK flag while nothing runs", () => {
    renderNav({ afkStatus: afk(false), bottingStatus: makeBottingStatus({ active: false }) });
    expect(screen.queryByTestId("nav-afk-active")).not.toBeInTheDocument();
  });

  it("counts the accounts on the Accounts item", () => {
    renderNav();
    expect(item("Accounts")).toHaveTextContent("2");
  });
});

describe("NavSidebar — collapse", () => {
  it("collapses to icons and keeps every item named", async () => {
    renderNav();
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    const nav = screen.getByRole("navigation", { name: "Main navigation" });
    expect(nav).toHaveAttribute("data-collapsed", "true");
    expect(screen.getByText("Settings")).toHaveClass("sr-only");
    expect(item("Settings")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
  });

  it("remembers the choice for the next start", async () => {
    renderNav();
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(localStorage.getItem(NAV_COLLAPSED_KEY)).toBe("1");

    cleanup();
    renderNav();
    expect(screen.getByRole("navigation", { name: "Main navigation" })).toHaveAttribute("data-collapsed", "true");

    await userEvent.click(screen.getByRole("button", { name: "Expand sidebar" }));
    expect(localStorage.getItem(NAV_COLLAPSED_KEY)).toBe("0");
  });

  /** Armazenamento bloqueado não pode derrubar a barra: ela abre expandida. */
  it("works when storage is unavailable", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    renderNav();
    const nav = screen.getByRole("navigation", { name: "Main navigation" });
    expect(nav).toHaveAttribute("data-collapsed", "false");
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(nav).toHaveAttribute("data-collapsed", "true");
  });

  /** Tooltip só no modo recolhido: com o nome visível ele repetiria o rótulo. */
  it("shows a tooltip only while collapsed", async () => {
    renderNav();
    item("Scripts").focus();
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    item("Scripts").focus();
    expect(await screen.findByRole("tooltip")).toHaveTextContent("Scripts");
  });

  it("opens the feedback dialog from the footer", async () => {
    renderNav();
    expect(screen.queryByRole("dialog", { name: "Send feedback" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Send feedback" }));
    expect(screen.getByRole("dialog", { name: "Send feedback" })).toBeInTheDocument();
  });});

/**
 * Estrela no GitHub (pedido do dono, 15/10/2026): botão fixo no rodapé e um
 * cartão que só pede depois de uns dias de uso, com "Agora não" e "nunca mais".
 */
describe("NavSidebar — GitHub star", () => {
  const READY = JSON.stringify({ days: ["2026-10-01", "2026-10-02", "2026-10-03"] });

  beforeEach(() => invokeMock.mockClear());

  it("always has a Star on GitHub button that opens the repository", async () => {
    renderNav();
    await userEvent.click(screen.getByRole("button", { name: "Star on GitHub" }));
    expect(invokeMock).toHaveBeenCalledWith("open_repo_url");
  });

  it("does not ask on the first days of use", () => {
    renderNav();
    expect(screen.queryByTestId("star-prompt")).not.toBeInTheDocument();
  });

  it("asks after three days of use, and starring ends it for good", async () => {
    localStorage.setItem("ram_star_prompt", READY);
    renderNav();
    const card = await screen.findByTestId("star-prompt");
    await userEvent.click(within(card).getByRole("button", { name: "Give a star" }));
    expect(invokeMock).toHaveBeenCalledWith("open_repo_url");
    expect(screen.queryByTestId("star-prompt")).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("ram_star_prompt")!).done).toBe(true);
  });

  it("'Not now' hides it and remembers when to ask again", async () => {
    localStorage.setItem("ram_star_prompt", READY);
    renderNav();
    await userEvent.click(within(await screen.findByTestId("star-prompt")).getByRole("button", { name: "Not now" }));
    expect(screen.queryByTestId("star-prompt")).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("ram_star_prompt")!).snoozeUntil).toBeGreaterThan(Date.now());
    expect(invokeMock).not.toHaveBeenCalledWith("open_repo_url");
  });

  it("the X dismisses it for good without opening anything", async () => {
    localStorage.setItem("ram_star_prompt", READY);
    renderNav();
    await userEvent.click(within(await screen.findByTestId("star-prompt")).getByRole("button", { name: "Don't show again" }));
    expect(screen.queryByTestId("star-prompt")).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("ram_star_prompt")!).done).toBe(true);
    expect(invokeMock).not.toHaveBeenCalledWith("open_repo_url");
  });

  it("is not shown with the sidebar collapsed (only the star button stays)", () => {
    localStorage.setItem("ram_star_prompt", READY);
    localStorage.setItem(NAV_COLLAPSED_KEY, "1");
    renderNav();
    expect(screen.queryByTestId("star-prompt")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Star on GitHub" })).toBeInTheDocument();
  });
});
