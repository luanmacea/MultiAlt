import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect, type ReactNode } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(async () => {}),
  disable: vi.fn(async () => {}),
}));

import { GeneralTab } from "./GeneralTab";
import { DeveloperTab } from "./DeveloperTab";
import { IsolationTab } from "./IsolationTab";
import { WebServerTab } from "./WebServerTab";
import { WatcherTab } from "./WatcherTab";
import { useSettings, type UseSettingsReturn } from "../../hooks/useSettings";
import { setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { ENABLE_WEBSERVER } from "../../featureFlags";
import type { PlatformCapabilities } from "../../types";
import i18n from "../../i18n";

/**
 * Settings tabs receive the real `useSettings()` object, so every assertion
 * below goes through the same debounce + `update_setting` path the app uses.
 */
function SettingsHarness({ children }: { children: (s: UseSettingsReturn) => ReactNode }) {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // SettingsDialog mounts the tabs only once settings are loaded, and some tabs
  // (IsolationTab) read settings in a mount effect — mirror that here.
  if (!s.loaded) return <div>Loading settings...</div>;
  return <>{children(s)}</>;
}

let stored: Record<string, Record<string, string>> = {};

function renderTab(children: (s: UseSettingsReturn) => ReactNode) {
  render(<SettingsHarness>{children}</SettingsHarness>);
}

/** Waits for the debounced save and asserts the exact backend payload. */
async function expectSaved(section: string, key: string, value: string) {
  await waitFor(() =>
    expect(invokeMock).toHaveBeenCalledWith("update_setting", { section, key, value })
  );
}

beforeEach(async () => {
  resetTauriMocks();
  // The GeneralTab language picker switches i18n globally; reset it per test.
  await i18n.changeLanguage("en");
  stored = {};
  setStore({});
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return stored;
      case "get_web_server_status":
        return { running: false, port: 0 };
      case "isolation_list_adapters":
        return [];
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("WebServerTab", () => {
  function renderWebServer(initial: Record<string, Record<string, string>> = { Developer: { DevMode: "true" } }) {
    stored = initial;
    renderTab((s) => <WebServerTab s={s} />);
  }

  it("stays locked until Developer Mode or the web server is enabled", async () => {
    renderWebServer({});
    expect(
      await screen.findByText("Enable Developer Mode or Web Server first")
    ).toBeInTheDocument();
    expect(screen.queryByText("Allow GetCookie")).not.toBeInTheDocument();
  });

  it("unlocks from the EnableWebServer flag alone", async () => {
    renderWebServer({ Developer: { EnableWebServer: "true" } });
    expect(await screen.findByText("Allow GetCookie")).toBeInTheDocument();
  });

  it.each([
    ["Every Request Requires Password", "EveryRequestRequiresPassword"],
    ["Allow GetCookie", "AllowGetCookie"],
    ["Allow GetAccounts", "AllowGetAccounts"],
    ["Allow LaunchAccount", "AllowLaunchAccount"],
    ["Allow Account Editing", "AllowAccountEditing"],
    ["Allow External Connections", "AllowExternalConnections"],
  ])("saves WebServer.%s as a boolean", async (label, key) => {
    renderWebServer();
    await userEvent.click(await screen.findByText(label));
    await expectSaved("WebServer", key, "true");
  });

  it("turns a permission back off", async () => {
    renderWebServer({ Developer: { DevMode: "true" }, WebServer: { AllowGetCookie: "true" } });
    await userEvent.click(await screen.findByText("Allow GetCookie"));
    await expectSaved("WebServer", "AllowGetCookie", "false");
  });

  it("strips non-alphanumeric characters from the password", async () => {
    renderWebServer();
    const field = (await screen.findByText("Password")).parentElement?.querySelector("input");
    await userEvent.type(field as HTMLInputElement, "a");
    await expectSaved("WebServer", "Password", "a");

    await userEvent.type(field as HTMLInputElement, "!");
    // The invalid character never reaches the backend.
    expect(invokeMock).not.toHaveBeenCalledWith("update_setting", {
      section: "WebServer",
      key: "Password",
      value: "a!",
    });
  });

  /**
   * O middleware devolve 401 para QUALQUER requisicao quando a senha tem menos
   * de 6 caracteres (api/server/middleware.rs:53). A tela deixava salvar "a" e
   * o usuario ficava com um servidor que recusa tudo, sem pista do motivo.
   */
  it("warns that a password under 6 characters blocks every request", async () => {
    renderWebServer({ Developer: { DevMode: "true" }, WebServer: { Password: "abc" } });
    expect(
      await screen.findByText("Too short: the server answers 401 to everything until it has 6 characters.")
    ).toBeInTheDocument();
  });

  it("drops the warning once the password is long enough", async () => {
    renderWebServer({ Developer: { DevMode: "true" }, WebServer: { Password: "abcdef" } });
    await screen.findByText("Password");
    expect(
      screen.queryByText("Too short: the server answers 401 to everything until it has 6 characters.")
    ).not.toBeInTheDocument();
  });

  it("starts and stops the server", async () => {
    renderWebServer();
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("start_web_server"));
  });

  it("reports the running port", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_all_settings") return { Developer: { DevMode: "true" } };
      if (cmd === "get_web_server_status") return { running: true, port: 7963 };
      return undefined;
    });
    renderTab((s) => <WebServerTab s={s} />);
    expect(await screen.findByText("Running on port 7963")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Stop" })).toBeInTheDocument();
  });
});

describe("IsolationTab", () => {
  function renderIsolation(initial: Record<string, Record<string, string>> = {}) {
    stored = initial;
    renderTab((s) => <IsolationTab s={s} />);
  }

  it("switches the isolation mode to Full", async () => {
    renderIsolation();
    await userEvent.click(await screen.findByText("Full"));
    await expectSaved("Isolation", "Mode", "Full");
  });

  it("switches the isolation mode back to Off", async () => {
    renderIsolation({ Isolation: { Mode: "Full" } });
    await userEvent.click(await screen.findByText("Off"));
    await expectSaved("Isolation", "Mode", "Off");
  });

  it("migrates a legacy light/medium mode to Full on mount", async () => {
    renderIsolation({ Isolation: { Mode: "medium" } });
    await expectSaved("Isolation", "Mode", "Full");
  });

  it("leaves a valid mode untouched on mount", async () => {
    renderIsolation({ Isolation: { Mode: "Off" } });
    await screen.findByText("Full");
    expect(invokeMock).not.toHaveBeenCalledWith("update_setting", expect.anything());
  });

  it.each([
    ["Rotate MachineGuid", "SpoofMachineGuid"],
    ["Rotate MAC address", "SpoofMacAddress"],
  ])("saves Isolation.%s", async (label, key) => {
    renderIsolation();
    await userEvent.click(await screen.findByText(label));
    await expectSaved("Isolation", key, "true");
  });

  it("keeps the preservation switches behind Advanced", async () => {
    renderIsolation();
    expect(screen.queryByText("Preserve fast flags")).not.toBeInTheDocument();

    await userEvent.click(await screen.findByRole("button", { name: "Advanced" }));
    // Defaults to on, so the first click turns it off.
    await userEvent.click(await screen.findByText("Preserve fast flags"));
    await expectSaved("Isolation", "PreserveFastFlags", "false");
  });
});

describe("GeneralTab", () => {
  function renderGeneral(initial: Record<string, Record<string, string>> = {}) {
    stored = initial;
    renderTab((s) => <GeneralTab s={s} />);
  }

  it.each([
    ["Auto Check for Updates", "General", "CheckForUpdates"],
    ["Launch one account at a time", "General", "AsyncJoin"],
    ["Disable Image Loading", "General", "DisableImages"],
    ["Multi Roblox", "General", "EnableMultiRbx"],
    ["Botting Mode", "General", "BottingEnabled"],
    ["Show Presence", "General", "ShowPresence"],
    ["Auto Cookie Refresh", "General", "AutoCookieRefresh"],
    ["Minimize to Tray", "General", "MinimizeToTray"],
  ])("saves the %s toggle", async (label, section, key) => {
    renderGeneral();
    await userEvent.click(await screen.findByText(label));
    await expectSaved(section, key, "true");
  });

  it.each([
    ["Persistent login profile", "PersistentProfile"],
    ["Reduce automation signals", "StealthMode"],
  ])("turns the %s login option off (it defaults to on)", async (label, key) => {
    renderGeneral();
    await userEvent.click(await screen.findByText(label));
    await expectSaved("Login", key, "false");
  });

  it("turns the aging alert off and on", async () => {
    renderGeneral({ General: { DisableAgingAlert: "true" } });
    await userEvent.click(await screen.findByText("Disable Aging Alert"));
    await expectSaved("General", "DisableAgingAlert", "false");
  });

  it("saves the picked language", async () => {
    renderGeneral();
    await userEvent.click(await screen.findByText("English"));
    await userEvent.click(await screen.findByText("German"));
    await expectSaved("General", "Language", "de");
  });

  it("saves the updater release channel", async () => {
    renderGeneral();
    await userEvent.click(await screen.findByText("Beta"));
    await userEvent.click(await screen.findByText("Stable"));
    await expectSaved("General", "UpdaterReleaseChannel", "stable");
  });

  it("saves the updater feature channel", async () => {
    renderGeneral();
    await userEvent.click(await screen.findByText("Standard (Non-Nexus/WebServer)"));
    await userEvent.click(await screen.findByText("Nexus + WebServer"));
    await expectSaved("General", "UpdaterFeatureChannel", "nexus-ws");
  });

  /**
   * `AsyncJoin` serializa a fila (launch.rs espera a conta anterior). O rotulo
   * "Async Launching" prometia o contrario e a descricao dizia o certo — duas
   * frases brigando na mesma linha.
   */
  it("names the serial launch toggle after what it does", async () => {
    renderGeneral();
    expect(await screen.findByText("Launch one account at a time")).toBeInTheDocument();
    expect(screen.queryByText("Async Launching")).not.toBeInTheDocument();
  });

  /** O backend nunca desce de MIN_JOIN_GAP_SECS = 8; o campo aceitava 0. */
  it("does not save a join delay the backend will ignore", async () => {
    renderGeneral({ General: { AccountJoinDelay: "20" } });
    const delay = await screen.findByLabelText("Account Join Delay");
    await userEvent.clear(delay);
    await userEvent.type(delay, "3");
    await userEvent.tab();
    await expectSaved("General", "AccountJoinDelay", "8");
  });

  /** Com o lote em serie o delay nem e lido: o campo tem que dizer isso. */
  it("disables the join delay while accounts launch one at a time", async () => {
    renderGeneral({ General: { AsyncJoin: "true" } });
    expect(await screen.findByLabelText("Account Join Delay")).toBeDisabled();
    expect(
      screen.getByText("Not used while accounts launch one at a time.")
    ).toBeInTheDocument();
  });

  it("registers the app with the OS autostart when Run on Windows Startup is turned on", async () => {
    const autostart = await import("@tauri-apps/plugin-autostart");
    renderGeneral();
    await userEvent.click(await screen.findByText("Run on Windows Startup"));
    await expectSaved("General", "StartOnPCStartup", "true");
    expect(autostart.enable).toHaveBeenCalledTimes(1);
  });
});

describe("WatcherTab", () => {
  function renderWatcher(os: string) {
    stored = {};
    setStore({ platformCapabilities: { os } as PlatformCapabilities });
    renderTab((s) => <WatcherTab s={s} />);
  }

  /**
   * `ReadInterval` so e lido dentro de `#[cfg(target_os = "macos")]`
   * (commands/watcher.rs). No Windows o campo era decoracao.
   */
  it("hides Read Interval on Windows, where nothing reads it", async () => {
    renderWatcher("windows");
    expect(await screen.findByText("Scan Interval")).toBeInTheDocument();
    expect(screen.queryByText("Read Interval")).not.toBeInTheDocument();
  });

  it("keeps Read Interval on macOS", async () => {
    renderWatcher("macos");
    expect(await screen.findByText("Read Interval")).toBeInTheDocument();
  });
});

describe("DeveloperTab", () => {
  function renderDeveloper(initial: Record<string, Record<string, string>> = {}) {
    stored = initial;
    renderTab((s) => <DeveloperTab s={s} />);
  }

  it("saves the Developer Mode toggle", async () => {
    renderDeveloper();
    await userEvent.click(await screen.findByText("Enable Developer Mode"));
    await expectSaved("Developer", "DevMode", "true");
  });

  it.runIf(ENABLE_WEBSERVER)("saves the web server toggle", async () => {
    renderDeveloper();
    await userEvent.click(await screen.findByText("Enable Web Server"));
    await expectSaved("Developer", "EnableWebServer", "true");
  });

  it("opens the update preview from the store", async () => {
    const store = setStore({});
    renderDeveloper();
    await userEvent.click(await screen.findByRole("button", { name: "Open Preview" }));
    expect(store.openUpdatePreviewDialog).toHaveBeenCalledTimes(1);
  });

  it.each([
    [{ holder: "free", robloxPids: [], legacyRamPids: [] }, "Free (no process holds the mutex)"],
    [
      { holder: "thisProcess", robloxPids: [], legacyRamPids: [] },
      "This Account Manager (Multi-Roblox active)",
    ],
    [
      { holder: "roblox", robloxPids: [11, 22], legacyRamPids: [] },
      "Running Roblox client(s): 11, 22",
    ],
    [
      { holder: "legacyRam", robloxPids: [], legacyRamPids: [99] },
      "Legacy Roblox Account Manager: PID 99",
    ],
  ])("explains who holds the Roblox mutex (%#)", async (diagnosis, expected) => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "diagnose_mutex_holder") return { ...diagnosis, thisProcessHolds: false };
      return undefined;
    });
    renderTab((s) => <DeveloperTab s={s} />);

    await userEvent.click(await screen.findByRole("button", { name: "Diagnose mutex holder" }));
    expect(await screen.findByText(expected)).toBeInTheDocument();
  });

  it("offers to close a legacy Account Manager that holds the mutex", async () => {
    let killed = false;
    setInvokeHandler((cmd) => {
      if (cmd === "get_all_settings") return {};
      if (cmd === "diagnose_mutex_holder")
        return killed
          ? { holder: "free", robloxPids: [], legacyRamPids: [], thisProcessHolds: false }
          : { holder: "legacyRam", robloxPids: [], legacyRamPids: [42], thisProcessHolds: false };
      if (cmd === "kill_legacy_ram_processes") {
        killed = true;
        return 1;
      }
      return undefined;
    });
    renderTab((s) => <DeveloperTab s={s} />);

    await userEvent.click(await screen.findByRole("button", { name: "Diagnose mutex holder" }));
    await userEvent.click(
      await screen.findByRole("button", { name: "Close legacy Account Manager" })
    );

    expect(await screen.findByText("Free (no process holds the mutex)")).toBeInTheDocument();
  });
});
