import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { NavSidebar, NAV_COLLAPSED_KEY } from "./NavSidebar";
import { makeAccount, makeBottingStatus, setStore } from "../../test-utils/renderWithStore";
import { ENABLE_NEXUS } from "../../featureFlags";
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
    intervalMinutes: 10,
    key: "space",
    mode: "key",
    clickX: 50,
    clickY: 50,
    accounts: [],
  } as AfkStatus;
}

beforeEach(() => {
  localStorage.clear();
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
    const expected = ["accounts", "session", "afk", "avatars", "scripts", "theme"];
    if (ENABLE_NEXUS) expected.push("nexus");
    expected.push("settings");
    expect(names).toEqual(expected);
  });

  it("shows the labels without hovering", () => {
    renderNav();
    for (const label of ["Accounts", "Session", "AFK Mode", "Avatars", "Scripts", "Theme", "Settings", "Help"]) {
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
    ["AFK Mode", "afk"],
    ["Avatars", "avatars"],
    ["Scripts", "scripts"],
    ["Theme", "theme"],
    ["Settings", "settings"],
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

  it("replays the walkthrough from Help", async () => {
    const store = renderNav();
    await userEvent.click(screen.getByRole("button", { name: /^Help/ }));
    expect(store.openFirstRunWalkthroughFromSettings).toHaveBeenCalledTimes(1);
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
});
