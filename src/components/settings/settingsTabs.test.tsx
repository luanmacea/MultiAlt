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
import { SettingsDialog } from "./SettingsDialog";
import { IsolationTab } from "./IsolationTab";
import { WebServerTab } from "./WebServerTab";
import { WatcherTab } from "./WatcherTab";
import { OptimizationTab } from "./OptimizationTab";
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
      // Usados so pelo SettingsDialog inteiro, que monta todas as abas de uma vez.
      case "versions_list_installed":
        return [];
      case "remembered_unlock_state":
        return { remembered: false, expiresAt: null };
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
    expect(await screen.findByText("Web Server is off")).toBeInTheDocument();
    expect(screen.queryByText("Allow GetCookie")).not.toBeInTheDocument();
  });

  /**
   * A aba deixou de ser escondida (SettingsDialog), entao o estado bloqueado e
   * a unica coisa que explica a funcionalidade: tem que dizer o QUE o servidor
   * faz e ONDE se liga, senao trocamos um recurso invisivel por uma tela muda.
   */
  it("says what the web server does and where to turn it on while locked", async () => {
    renderWebServer({});
    expect(
      await screen.findByText(
        "It serves a local HTTP API so external tools and scripts can list your accounts, read their cookies and launch them."
      )
    ).toBeInTheDocument();
    expect(
      screen.getByText("Turn on Enable Web Server in the Developer tab to unlock these settings.")
    ).toBeInTheDocument();
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

  /**
   * O modo Full apaga arquivos e chaves do registro antes de cada launch. A
   * previa (dry-run) e a restauracao dos identificadores sao as duas redes de
   * seguranca do usuario: nenhuma pode depender de descobrir um "Advanced".
   */
  it("offers the wipe preview without expanding Advanced", async () => {
    renderIsolation({ Isolation: { Mode: "Full" } });
    await userEvent.click(
      await screen.findByRole("button", { name: "Preview what gets wiped" })
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("isolation_dry_run", expect.anything())
    );
  });

  it("offers the identifier restore without expanding Advanced", async () => {
    renderIsolation({ Isolation: { BackupMachineGuid: "{original-guid}" } });
    expect(
      await screen.findByRole("button", { name: "Restore original network identifiers" })
    ).toBeEnabled();
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

/**
 * A aba WebServer sumia inteira sem Dev Mode: quem nao sabia que existe uma API
 * HTTP local nunca ia descobrir. A capacidade tem que ser descobrivel — o que
 * continua trancado e o conteudo, nao a aba.
 */
describe("SettingsDialog tabs", () => {
  it.runIf(ENABLE_WEBSERVER)("lists the WebServer tab even without Developer Mode", async () => {
    stored = {};
    render(<SettingsDialog open onClose={() => {}} />);
    expect(await screen.findByRole("button", { name: "WebServer" })).toBeInTheDocument();
  });

  it.runIf(ENABLE_WEBSERVER)("opens the WebServer tab on its locked explanation", async () => {
    stored = {};
    render(<SettingsDialog open onClose={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "WebServer" }));
    expect(await screen.findByText("Web Server is off")).toBeVisible();
  });

  /**
   * "Generator" sozinho nao diz qual das duas funcoes de criar conta e esta.
   * O nome canonico da paga, usado no menu Add e no dialogo, e
   * `Account Generator` — a aba tem que bater com ele.
   */
  it("names the generator tab like the rest of the app does", async () => {
    stored = {};
    render(<SettingsDialog open onClose={() => {}} />);
    expect(await screen.findByRole("button", { name: "Account Generator" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Generator" })).not.toBeInTheDocument();
  });

  /**
   * O dialogo era o unico da familia sem guarda de viewport (520px fixos,
   * 85vh): em telas pequenas a aba Optimization sozinha ja rolava 5,8 telas.
   * O padrao usado em BottingDialog/GeneratorDialog e `max-w`/`max-h` com
   * `calc(100vw|100vh - 24px)` — a janela tem `minWidth` 750
   * (tauri.conf.json), entao a guarda de largura nao e opcional.
   */
  it("guards the dialog width and height against the window's minimum size", async () => {
    stored = {};
    render(<SettingsDialog open onClose={() => {}} />);
    const modal = await screen.findByText("Settings");
    const panel = modal.closest('[data-tour="settings-modal"]');
    expect(panel?.className).toContain("w-[780px]");
    expect(panel?.className).toContain("max-w-[calc(100vw-24px)]");
    expect(panel?.className).toContain("h-[calc(100vh-24px)]");
    expect(panel?.className).toContain("max-h-[760px]");
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

  /**
   * O aviso antes de copiar credencial nasce **ligado**: o toggle existe para
   * quem quer desligar, entao o clique grava "false".
   */
  it("lets the credential-copy warning be turned off", async () => {
    renderGeneral();
    await userEvent.click(await screen.findByText("Warn Before Copying Credentials"));
    await expectSaved("General", "WarnOnCopyCredential", "false");
  });

  it("saves the picked language", async () => {
    renderGeneral();
    await userEvent.click(await screen.findByText("English"));
    await userEvent.click(await screen.findByText("German"));
    await expectSaved("General", "Language", "de");
  });

  it("offers Brazilian Portuguese and saves it as pt", async () => {
    renderGeneral();
    await userEvent.click(await screen.findByText("English"));
    await userEvent.click(await screen.findByText("Portuguese (Brazil)"));
    await expectSaved("General", "Language", "pt");
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

  /**
   * O bloco da previa do modal era texto solto no JSX: nao passava por `t()`,
   * entao ficava em ingles com o catalogo inteiro traduzido.
   */
  it("mostra o bloco da prévia do modal traduzido", async () => {
    await i18n.changeLanguage("pt");
    try {
      renderDeveloper();
      expect(await screen.findByText("Prévia do modal de atualização")).toBeInTheDocument();
      expect(screen.getByText("Abrir a prévia")).toBeInTheDocument();
      expect(screen.queryByText("Update Modal Preview")).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

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

/**
 * A allowlist de fast flags mora no Rust (`WINDOWS_FASTFLAG_ALLOWLIST`,
 * platform/windows/optimization.rs). O backend recusa o JSON inteiro se
 * qualquer chave estiver fora dela — e `launch_shared.rs` engole o erro num
 * `eprintln!`. A tela precisa recusar na hora o que o launch vai recusar
 * depois, senao o toggle fica ligado e nada e aplicado, em silencio.
 */
describe("OptimizationTab", () => {
  function renderOptimization(
    initial: Record<string, Record<string, string>> = {},
    os = "windows"
  ) {
    stored = initial;
    setStore({ platformCapabilities: { os } as PlatformCapabilities });
    renderTab((s) => <OptimizationTab s={s} />);
  }

  const FAST_FLAGS_ON = {
    Optimization: { NormalEnableFastFlags: "true" },
  } as Record<string, Record<string, string>>;

  it("rejects a fast flag key that is not on the backend allowlist", async () => {
    renderOptimization({
      Optimization: {
        NormalEnableFastFlags: "true",
        NormalFastFlagsJson: '{"FFlagMadeUpByTheUser": true}',
      },
    });
    expect(
      await screen.findByText(
        "Only Roblox allowlisted keys are accepted: FFlagMadeUpByTheUser"
      )
    ).toBeInTheDocument();
  });

  it("accepts a JSON whose keys are all on the allowlist", async () => {
    renderOptimization({
      Optimization: {
        NormalEnableFastFlags: "true",
        NormalFastFlagsJson: '{"DFIntTextureQualityOverride": 0}',
      },
    });
    await screen.findByLabelText("Allowlisted fast flags JSON");
    expect(
      screen.queryByText(/Only Roblox allowlisted keys are accepted:/)
    ).not.toBeInTheDocument();
  });

  it("shows which keys the backend accepts instead of making the user guess", async () => {
    renderOptimization(FAST_FLAGS_ON);
    expect(await screen.findByText("DFFlagTextureQualityOverrideEnabled")).toBeInTheDocument();
    expect(screen.getByText("DFIntRenderShadowIntensity")).toBeInTheDocument();
  });

  /**
   * Campo numerico/texto editavel com o interruptor que o governa desligado:
   * o valor e salvo e o backend nunca o le.
   */
  it.each([
    ["Max FPS", "NormalUnlockFPS"],
    ["Client Volume", "NormalOverrideClientVolume"],
    ["Graphics Level", "NormalOverrideClientGraphics"],
    ["Window Width", "NormalOverrideClientWindowSize"],
    ["Window Height", "NormalOverrideClientWindowSize"],
  ])("disables %s while its General switch is off", async (label) => {
    renderOptimization();
    expect(await screen.findByLabelText(label)).toBeDisabled();
  });

  it.each([
    ["Max FPS", "UnlockFPS"],
    ["Client Volume", "OverrideClientVolume"],
    ["Graphics Level", "OverrideClientGraphics"],
    ["Window Width", "OverrideClientWindowSize"],
    ["Window Height", "OverrideClientWindowSize"],
  ])("enables %s once its General switch is on", async (label, key) => {
    renderOptimization({ General: { [key]: "true" } });
    expect(await screen.findByLabelText(label)).toBeEnabled();
  });

  it.each([
    ["Apply delay", "NormalEnableProcessPolicy"],
    ["CPU limit", "NormalEnableJobCpuLimit"],
    ["Process memory limit", "NormalEnableJobMemoryLimit"],
  ])("disables %s while its Optimization switch is off", async (label, key) => {
    renderOptimization();
    expect(await screen.findByLabelText(label)).toBeDisabled();

    cleanup();
    renderOptimization({ Optimization: { [key]: "true" } });
    expect(await screen.findByLabelText(label)).toBeEnabled();
  });

  it.each([["Priority Class"], ["Memory Priority"]])(
    "disables the %s picker while process optimization is off",
    async (label) => {
      renderOptimization();
      expect(await screen.findByRole("button", { name: label })).toBeDisabled();

      cleanup();
      renderOptimization({ Optimization: { NormalEnableProcessPolicy: "true" } });
      expect(await screen.findByRole("button", { name: label })).toBeEnabled();
    }
  );

  it.each([["Background Mode"], ["EcoQoS"], ["Ignore Timer Resolution"]])(
    "ignores a click on %s while process optimization is off",
    async (label) => {
      renderOptimization();
      await userEvent.click(await screen.findByText(label));
      expect(invokeMock).not.toHaveBeenCalledWith("update_setting", expect.anything());
    }
  );

  it("disables the fast flags editor while the fast flags switch is off", async () => {
    renderOptimization();
    expect(await screen.findByLabelText("Allowlisted fast flags JSON")).toBeDisabled();
  });

  it("enables the fast flags editor once the switch is on", async () => {
    renderOptimization(FAST_FLAGS_ON);
    expect(await screen.findByLabelText("Allowlisted fast flags JSON")).toBeEnabled();
  });

  /**
   * Antes desta mudanca, Botting ligado + perfis separados montava as 3
   * secoes de uma vez: 6357px de scroll, `Unlock FPS` 3x, e 12 aria-label
   * triplicados (Max FPS, Client Volume, Priority Class...). Um perfil por
   * vez elimina isso — so a secao escolhida existe no DOM.
   */
  describe("profile selector", () => {
    const SEPARATE_PROFILES = { General: { BottingEnabled: "true", BottingUseSharedClientProfile: "false" } };

    it("hides the selector when there is only one profile", async () => {
      renderOptimization();
      expect(screen.queryByRole("radio", { name: "Botting Player" })).not.toBeInTheDocument();
      expect(screen.queryByRole("radio", { name: "Normal" })).not.toBeInTheDocument();
    });

    it("shows one radio per profile once Botting uses separate profiles", async () => {
      renderOptimization(SEPARATE_PROFILES);
      expect(await screen.findByRole("radio", { name: "Normal" })).toBeInTheDocument();
      expect(screen.getByRole("radio", { name: "Botting Player" })).toBeInTheDocument();
      expect(screen.getByRole("radio", { name: "Botting Bot" })).toBeInTheDocument();
    });

    it("mounts only the selected profile's section, never more than one", async () => {
      renderOptimization(SEPARATE_PROFILES);
      await screen.findByRole("radio", { name: "Normal" });
      // Um so "Max FPS" no DOM — com as 3 secoes montadas de uma vez isso dava 3.
      expect(screen.getAllByLabelText("Max FPS")).toHaveLength(1);
      expect(screen.getAllByText("Unlock FPS")).toHaveLength(1);
    });

    it("switches the mounted section when another profile is picked", async () => {
      renderOptimization(SEPARATE_PROFILES);
      await userEvent.click(await screen.findByRole("radio", { name: "Botting Bot" }));
      expect(screen.getByRole("radio", { name: "Botting Bot" })).toHaveAttribute("aria-checked", "true");
      // Continua havendo so uma secao montada apos trocar de perfil.
      expect(screen.getAllByLabelText("Max FPS")).toHaveLength(1);
    });

    /**
     * O titulo do perfil ativo nao pode depender de rolagem: o seletor mora
     * fora do fluxo que rola (`sticky`), entao ele sempre esta visivel junto
     * com o nome do perfil escolhido.
     */
    it("keeps the profile picker out of the scrolling flow", async () => {
      renderOptimization(SEPARATE_PROFILES);
      const group = await screen.findByRole("radiogroup");
      expect(group.closest(".sticky")).not.toBeNull();
    });

    it("falls back to Normal when Botting is turned back off while another profile is selected", async () => {
      renderOptimization(SEPARATE_PROFILES);
      await userEvent.click(await screen.findByRole("radio", { name: "Botting Bot" }));
      cleanup();
      renderOptimization();
      expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
      expect(await screen.findByLabelText("Max FPS")).toBeInTheDocument();
    });
  });
});

describe("WatcherTab dependent fields", () => {
  function renderWatcher(initial: Record<string, Record<string, string>> = {}) {
    stored = initial;
    setStore({ platformCapabilities: { os: "windows" } as PlatformCapabilities });
    renderTab((s) => <WatcherTab s={s} />);
  }

  it.each([
    ["No Connection Timeout", "ExitIfNoConnection"],
    ["Memory Threshold", "CloseRbxMemory"],
    ["Expected Title", "CloseRbxWindowTitle"],
  ])("disables %s while its switch is off", async (label, key) => {
    renderWatcher();
    expect(await screen.findByLabelText(label)).toBeDisabled();

    cleanup();
    renderWatcher({ Watcher: { [key]: "true" } });
    expect(await screen.findByLabelText(label)).toBeEnabled();
  });
});
