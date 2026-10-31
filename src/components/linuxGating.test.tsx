import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect, type ReactNode } from "react";

vi.mock("../store", async () => (await import("../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../hooks/usePrompt", async () => (await import("../test-utils/promptMocks")).promptModuleMock());

import { NavSidebar } from "./layout/NavSidebar";
import { SessionPage } from "./pages/SessionPage";
import { ScriptsPage } from "./pages/ScriptsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { LaunchTab } from "./settings/LaunchTab";
import { LinuxTab } from "./settings/LinuxTab";
import { tabsFor } from "./settings/tabs";
import { useSettings } from "../hooks/useSettings";
import { makePlatformCapabilities, setStore } from "../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../test-utils/tauriMocks";
import { resetPromptMocks } from "../test-utils/promptMocks";
import type { PlatformCapabilities } from "../types";

/** Exatamente o que `commands/platform_info.rs` devolve para o Linux. */
function linux(): PlatformCapabilities {
  return makePlatformCapabilities({
    os: "linux",
    sessionType: "wayland",
    preferredRunner: "sober",
    detectedRunner: "sober",
    supportsWatcher: false,
    supportsWatcherMemory: false,
    supportsWindowControls: false,
    supportsBotting: false,
    supportsClientSettings: false,
    supportsLiveView: false,
    supportsAfkInput: false,
    supportsIsolation: false,
    supportsMultiRobloxMutex: false,
    supportsVersions: false,
    supportsOptimization: false,
    supportsExternalClients: false,
    supportsPython: false,
    reasons: ["Auto Rejoin is not available on Linux yet"],
    warnings: ["Linux runs Roblox through Sober"],
  });
}

let settingsStore: Record<string, Record<string, string>> = {};

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  settingsStore = {};
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return settingsStore;
      case "get_scripts":
        return [];
      case "get_setting":
        return null;
      case "remembered_unlock_state":
        return { remembered: false, expiresAt: null };
      case "list_backups":
        return [];
      case "backups_info":
        return { dir: "C:/data", portable: false, totalBytes: 0, count: 0 };
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

function navPages(): string[] {
  return screen
    .getAllByRole("button")
    .map((b) => b.getAttribute("data-nav") ?? "")
    .filter(Boolean);
}

describe("navigation on Linux", () => {
  it("hides Live and AFK Mode, keeps the pages that work everywhere", () => {
    setStore({ platformCapabilities: linux() });
    render(<NavSidebar />);
    const pages = navPages();
    expect(pages).not.toContain("live");
    expect(pages).not.toContain("afk");
    for (const page of ["accounts", "session", "console", "discord", "avatars", "scripts", "theme", "settings", "changelog"]) {
      expect(pages).toContain(page);
    }
  });

  it("shows Live and AFK Mode on Windows", () => {
    setStore({ platformCapabilities: makePlatformCapabilities() });
    render(<NavSidebar />);
    expect(navPages()).toEqual(expect.arrayContaining(["live", "afk"]));
  });

  it("shows everything until the capabilities arrive", () => {
    setStore({ platformCapabilities: null });
    render(<NavSidebar />);
    expect(navPages()).toEqual(expect.arrayContaining(["live", "afk"]));
  });
});

describe("Settings sections", () => {
  const ids = (caps: PlatformCapabilities | null) => tabsFor(caps).map((tab) => tab.id);

  it("drops the Windows-only sections on Linux and adds Linux", () => {
    const list = ids(linux());
    for (const gone of ["keep-in-game", "watcher", "isolation", "versions", "optimization"]) {
      expect(list).not.toContain(gone);
    }
    expect(list).toContain("linux");
    for (const kept of ["launch", "accounts", "privacy", "updates", "app", "backups", "checks", "developer"]) {
      expect(list).toContain(kept);
    }
  });

  it("keeps every Windows section and has no Linux section there", () => {
    const list = ids(makePlatformCapabilities());
    for (const shown of ["keep-in-game", "watcher", "isolation", "versions", "optimization"]) {
      expect(list).toContain(shown);
    }
    expect(list).not.toContain("linux");
  });

  it("does not mount the hidden sections in the page", async () => {
    setStore({ platformCapabilities: linux() });
    const { container } = render(<SettingsPage active onLeave={vi.fn()} />);
    await screen.findByRole("heading", { level: 2, name: "Launch" });
    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    expect(nav).toHaveTextContent("Linux");
    expect(nav).not.toHaveTextContent("Isolation");
    expect(nav).not.toHaveTextContent("Versions");
    expect(nav).not.toHaveTextContent("Optimization");
    expect(nav).not.toHaveTextContent("Keep in game");
    expect(container.querySelector('[data-settings-pane="isolation"]')).toBeNull();
    expect(container.querySelector('[data-settings-pane="linux"]')).not.toBeNull();
  });
});

function SettingsLoaded({ children }: { children: (s: ReturnType<typeof useSettings>) => ReactNode }) {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return s.loaded ? children(s) : null;
}

describe("Settings › Launch", () => {
  it("has no Multi Roblox switches on Linux but keeps the launch timing", async () => {
    setStore({ platformCapabilities: linux() });
    render(<SettingsLoaded>{(s) => <LaunchTab s={s} />}</SettingsLoaded>);
    expect(await screen.findByText("Launch one account at a time")).toBeInTheDocument();
    expect(screen.queryByText("Multi Roblox")).not.toBeInTheDocument();
    expect(screen.queryByText("Auto Close Roblox for Multi Roblox")).not.toBeInTheDocument();
  });

  it("keeps the Multi Roblox switches on Windows", async () => {
    setStore({ platformCapabilities: makePlatformCapabilities() });
    render(<SettingsLoaded>{(s) => <LaunchTab s={s} />}</SettingsLoaded>);
    expect(await screen.findByText("Multi Roblox")).toBeInTheDocument();
    expect(screen.getByText("Auto Close Roblox for Multi Roblox")).toBeInTheDocument();
  });
});

describe("Settings › Linux", () => {
  const renderLinux = () =>
    render(<SettingsLoaded>{(s) => <LinuxTab s={s} />}</SettingsLoaded>);

  it("explains that Roblox runs through Sober and what to install", async () => {
    setStore({ platformCapabilities: linux() });
    renderLinux();
    expect(
      await screen.findByText("Runs Roblox through Sober (Flatpak). Install Sober and passt, then launch as usual.")
    ).toBeInTheDocument();
  });

  it("shows the stored values of the four [Linux] keys", async () => {
    settingsStore = {
      Linux: {
        PreferredRunner: "custom",
        CustomLaunchCommand: "my-launcher --play",
        CustomProcessMatch: "sober,roblox",
        CustomLogDir: "/home/me/logs",
      },
    };
    setStore({ platformCapabilities: linux() });
    renderLinux();
    expect(await screen.findByRole("textbox", { name: "Custom launch command" })).toHaveValue("my-launcher --play");
    expect(screen.getByRole("textbox", { name: "Process names" })).toHaveValue("sober,roblox");
    expect(screen.getByRole("textbox", { name: "Log folder" })).toHaveValue("/home/me/logs");
    expect(screen.getByRole("button", { name: "Preferred runner" })).toHaveTextContent("Custom command");
  });

  it("saves edits to the right [Linux] key", async () => {
    settingsStore = { Linux: { PreferredRunner: "custom" } };
    setStore({ platformCapabilities: linux() });
    renderLinux();
    const user = userEvent.setup();
    await user.type(await screen.findByRole("textbox", { name: "Log folder" }), "/x");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "Linux",
        key: "CustomLogDir",
        value: "/x",
      })
    );
  });

  it("only lets you type a custom command when the runner is Custom", async () => {
    settingsStore = { Linux: { PreferredRunner: "sober" } };
    setStore({ platformCapabilities: linux() });
    renderLinux();
    expect(await screen.findByRole("textbox", { name: "Custom launch command" })).toBeDisabled();
  });

  it("saves the runner choice", async () => {
    settingsStore = { Linux: { PreferredRunner: "sober" } };
    setStore({ platformCapabilities: linux() });
    renderLinux();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Preferred runner" }));
    await user.click(await screen.findByText("Custom command"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "Linux",
        key: "PreferredRunner",
        value: "custom",
      })
    );
  });
});

describe("Session page", () => {
  it("drops the Keep accounts in game card on Linux, where AFK Mode does not exist", () => {
    setStore({ platformCapabilities: linux() });
    render(<SessionPage active onLeave={vi.fn()} />);
    expect(screen.queryByText("Keep accounts in game")).not.toBeInTheDocument();
    expect(screen.queryByText("Open AFK Mode")).not.toBeInTheDocument();
  });

  it("keeps it on Windows", () => {
    setStore({ platformCapabilities: makePlatformCapabilities() });
    render(<SessionPage active onLeave={vi.fn()} />);
    expect(screen.getByText("Keep accounts in game")).toBeInTheDocument();
  });
});

describe("Scripts page", () => {
  it("has no Python mode switch on Linux", async () => {
    setStore({ platformCapabilities: linux() });
    render(<ScriptsPage active onLeave={vi.fn()} />);
    await screen.findByRole("heading", { level: 1, name: "Scripts" });
    expect(screen.queryByRole("tablist", { name: "Scripts mode" })).not.toBeInTheDocument();
  });

  it("has it on Windows", async () => {
    setStore({ platformCapabilities: makePlatformCapabilities() });
    render(<ScriptsPage active onLeave={vi.fn()} />);
    expect(await screen.findByRole("tablist", { name: "Scripts mode" })).toBeInTheDocument();
  });
});
