import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("@tauri-apps/api/window", async () => (await import("../../test-utils/tauriMocks")).tauriWindowMock());
vi.mock("@tauri-apps/api/webview", async () => (await import("../../test-utils/tauriMocks")).tauriWebviewMock());
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(async () => {}),
  disable: vi.fn(async () => {}),
}));

import { SettingsPage } from "../pages/SettingsPage";
import { SETTINGS_TABS, TAB_ORDER, type TabId } from "./tabs";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import i18n from "../../i18n";

/**
 * Settings › General era um paredão de ~40 opções (crítica de design P1,
 * 11/10/2026). Ele e a Misc viraram seis seções de nome simples. A divisão é
 * só de lugar: cada controle continua gravando a mesma chave do INI.
 *
 * O inventário abaixo é a lista de tudo que morava em General e em Misc. Cada
 * linha diz para que seção foi e, quando é um interruptor, o que um clique
 * grava (com o INI vazio). A página monta todas as seções de uma vez (as não
 * escolhidas ficam escondidas), então dá para conferir que cada controle
 * aparece **uma vez só** na página inteira, e dentro da seção certa.
 */
type Entry = {
  label: string;
  tab: TabId;
  /** O que um clique no rótulo grava, com o INI vazio. */
  save?: [section: string, key: string, value: string];
};

const INVENTORY: Entry[] = [
  // Launch
  { label: "Launch does nothing?", tab: "launch" },
  { label: "Multi Roblox", tab: "launch", save: ["General", "EnableMultiRbx", "true"] },
  { label: "Experimental: keep clients open across teleports", tab: "launch" },
  { label: "Auto Close Roblox for Multi Roblox", tab: "launch", save: ["General", "AutoCloseRobloxForMultiRbx", "true"] },
  { label: "Launch one account at a time", tab: "launch", save: ["General", "AsyncJoin", "true"] },
  { label: "Account Join Delay", tab: "launch" },
  {
    label: "Start the next account once the previous one is in the game",
    tab: "launch",
    save: ["General", "WaitForGameJoin", "false"],
  },
  { label: "Check Bans Before Launch", tab: "launch", save: ["General", "CheckModerationBeforeLaunch", "false"] },
  { label: "Warn Before Joining Online Accounts", tab: "launch", save: ["General", "WarnOnOnlineJoin", "false"] },
  { label: "Auto Close Last Process", tab: "launch", save: ["General", "AutoCloseLastProcess", "true"] },
  { label: "Shuffle Job ID", tab: "launch", save: ["General", "ShuffleJobId", "true"] },
  // Keep in game
  { label: "Auto Rejoin", tab: "keep-in-game", save: ["General", "BottingEnabled", "true"] },
  { label: "Reconnect accounts that drop", tab: "keep-in-game" },
  {
    label: "Keep the PC awake while accounts are kept in game",
    tab: "keep-in-game",
    save: ["General", "KeepPcAwake", "false"],
  },
  { label: "Stop AFK and recordings when I shake the mouse", tab: "keep-in-game", save: ["Afk", "ShakeToStop", "true"] },
  // Accounts
  { label: "Show Presence", tab: "accounts", save: ["General", "ShowPresence", "true"] },
  { label: "Presence Refresh", tab: "accounts" },
  { label: "Wrap Long Names", tab: "accounts", save: ["General", "WrapLongNames", "true"] },
  { label: "Disable Aging Alert", tab: "accounts", save: ["General", "DisableAgingAlert", "true"] },
  { label: "Disable Image Loading", tab: "accounts", save: ["General", "DisableImages", "true"] },
  { label: "Auto Cookie Refresh", tab: "accounts", save: ["General", "AutoCookieRefresh", "true"] },
  // Pedido do dono (12/10/2026): "Show Robux" (desligado) no lugar do "Hide Robux When Hidden".
  { label: "Show Robux", tab: "accounts", save: ["General", "ShowRobux", "true"] },
  { label: "Persistent login profile", tab: "accounts", save: ["Login", "PersistentProfile", "false"] },
  { label: "Reduce automation signals", tab: "accounts", save: ["Login", "StealthMode", "false"] },
  { label: "Custom browser executable", tab: "accounts" },
  { label: "Bundled Browser", tab: "accounts" },
  // Privacy & security
  { label: "Preview Letters", tab: "privacy" },
  { label: "Show Avatars When Hidden", tab: "privacy", save: ["General", "ShowAvatarsWhenHidden", "true"] },
  {
    label: "Show Account Name on Roblox Window",
    tab: "privacy",
    save: ["General", "ShowAccountNameOnWindow", "false"],
  },
  { label: "Change Encryption Method", tab: "privacy" },
  { label: "Warn Before Copying Credentials", tab: "privacy", save: ["General", "WarnOnCopyCredential", "false"] },
  { label: "Lock after inactivity", tab: "privacy" },
  { label: "Lock after", tab: "privacy" },
  { label: "Stay signed in on this computer", tab: "privacy" },
  { label: "Restricted Screen Style", tab: "privacy" },
  // Updates
  { label: "Auto Check for Updates", tab: "updates", save: ["General", "CheckForUpdates", "true"] },
  { label: "Update Release Channel", tab: "updates" },
  { label: "Update Feature Channel", tab: "updates" },
  { label: "Manual Update Check", tab: "updates" },
  // App
  { label: "Language", tab: "app" },
  { label: "Interface size", tab: "app" },
  { label: "Theme Windows window navbar", tab: "app", save: ["General", "ThemeWindowsNavbar", "true"] },
  { label: "Run on Windows Startup", tab: "app", save: ["General", "StartOnPCStartup", "true"] },
  { label: "Minimize to Tray", tab: "app", save: ["General", "MinimizeToTray", "true"] },
  { label: "First-Time Walkthrough", tab: "app" },
  { label: "Max Recent Games", tab: "app" },
  { label: "Max Recent Servers", tab: "app" },
  { label: "Region Format", tab: "app" },
];

const TAB_LABEL: Record<string, string> = Object.fromEntries(SETTINGS_TABS.map((tab) => [tab.id, tab.label]));

let stored: Record<string, Record<string, string>> = {};
let userAgent: { mockRestore: () => void } | null = null;

beforeEach(async () => {
  resetTauriMocks();
  await i18n.changeLanguage("en");
  stored = {};
  setStore({});
  // Metade dos controles de launch e de "manter no jogo" só existe no Windows.
  userAgent = vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return stored;
      case "get_web_server_status":
        return { running: false, port: 0 };
      case "isolation_list_adapters":
      case "versions_list_installed":
      case "list_backups":
        return [];
      case "remembered_unlock_state":
        return { supported: true, active: false, defaultHours: 24 };
      case "backups_info":
        return { dir: "C:/data", portable: false, totalBytes: 0, count: 0 };
      default:
        return undefined;
    }
  });
});

afterEach(() => {
  userAgent?.mockRestore();
  cleanup();
});

async function renderPage() {
  render(<SettingsPage active onLeave={() => {}} />);
  await screen.findByRole("navigation", { name: "Settings sections" });
  // Espera as seções montarem (só montam com as settings carregadas).
  await screen.findAllByText("Launch does nothing?");
}

describe("Settings split (General and Misc became plain sections)", () => {
  it("lists the everyday sections first, then the advanced ones, with no General or Misc", async () => {
    await renderPage();
    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    const names = within(nav)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(names.slice(0, 8)).toEqual([
      "Launch",
      "Keep in game",
      "Notifications",
      "Accounts",
      "Privacy & security",
      "Updates",
      "App",
      "Backups",
    ]);
    expect(names).not.toContain("General");
    expect(names).not.toContain("Misc");
    expect(within(nav).getByText("Advanced")).toBeInTheDocument();
    expect(TAB_ORDER).not.toContain("general" as TabId);
    expect(TAB_ORDER).not.toContain("miscellaneous" as TabId);
  });

  it("opens on Launch", async () => {
    await renderPage();
    expect(screen.getByRole("heading", { level: 2, name: "Launch" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Launch" })).toHaveAttribute("aria-current", "true");
  });

  it.each(INVENTORY.map((e) => [e.label, e.tab] as const))(
    "shows %s exactly once, in the %s section",
    async (label, tab) => {
      await renderPage();
      const matches = await screen.findAllByText(label);
      expect(matches).toHaveLength(1);
      expect(matches[0].closest("[data-settings-pane]")).toHaveAttribute("data-settings-pane", tab);
    }
  );

  it.each(INVENTORY.filter((e) => e.save).map((e) => [e.label, e.tab, ...e.save!] as const))(
    "%s (%s) still saves %s.%s",
    async (label, tab, section, key, value) => {
      await renderPage();
      await userEvent.click(screen.getByRole("button", { name: TAB_LABEL[tab] }));
      expect(screen.getByRole("heading", { level: 2, name: TAB_LABEL[tab] })).toBeInTheDocument();
      await userEvent.click(screen.getByText(label));
      await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_setting", { section, key, value }));
    }
  );
});

describe("Settings › Keep in game links to where each option is changed", () => {
  /**
   * O padrão da reconexão tem casa na página Session, ao lado das contas em
   * jogo, onde cada conta também tem a sua. Aqui fica o estado e o caminho até
   * lá, sem um segundo interruptor para a mesma chave.
   */
  it("has no second reconnect switch and sends you to the Session page", async () => {
    const store = setStore({});
    await renderPage();
    await userEvent.click(screen.getByRole("button", { name: "Keep in game" }));
    expect(screen.queryByRole("switch", { name: "Reconnect accounts that drop" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Change it on the Session page" }));
    expect(store.setActivePage).toHaveBeenCalledWith("session");
  });

  it("says whether reconnecting is on for every account", async () => {
    stored = { General: { AutoReconnect: "true" } };
    await renderPage();
    expect(
      screen.getByText("On for every account that doesn't set its own. It's changed next to the accounts in game.")
    ).toBeInTheDocument();
  });

  it("sends you to AFK Mode for who rejoins, where and how often", async () => {
    const store = setStore({});
    await renderPage();
    await userEvent.click(screen.getByRole("button", { name: "Keep in game" }));
    await userEvent.click(screen.getByRole("button", { name: "Open AFK Mode" }));
    expect(store.setActivePage).toHaveBeenCalledWith("afk");
  });
});

describe("Settings deep links", () => {
  /** Outra tela pede uma seção (`store.openSettingsSection`): a página abre nela. */
  it("opens the section another screen asked for", async () => {
    setStore({ settingsSectionRequest: { section: "accounts", seq: 1 } });
    await renderPage();
    expect(screen.getByRole("heading", { level: 2, name: "Accounts" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accounts" })).toHaveAttribute("aria-current", "true");
  });
});
