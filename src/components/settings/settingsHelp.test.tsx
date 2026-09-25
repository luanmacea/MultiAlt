import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { useEffect, type ReactNode } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(async () => {}),
  disable: vi.fn(async () => {}),
}));

import { GeneralTab } from "./GeneralTab";
import { MiscellaneousTab } from "./MiscellaneousTab";
import { OptimizationTab } from "./OptimizationTab";
import { GeneratorTab } from "./GeneratorTab";
import { useSettings, type UseSettingsReturn } from "../../hooks/useSettings";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import type { PlatformCapabilities } from "../../types";
import i18n from "../../i18n";

/**
 * Faixa P2 do `docs/ux-checkup.md`: "esta na tela e nao se explica".
 *
 * Cada teste aqui fixa UM texto que so existe porque o ajuste ao lado dele e
 * indecifravel pelo nome — o que o backend faz de verdade esta citado no
 * comentario acima do teste, com arquivo e linha. Sem isso o texto vira chute,
 * e texto explicativo errado e pior que ausencia de texto.
 */
function SettingsHarness({ children }: { children: (s: UseSettingsReturn) => ReactNode }) {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!s.loaded) return <div>Loading settings...</div>;
  return <>{children(s)}</>;
}

let stored: Record<string, Record<string, string>> = {};

function renderTab(children: (s: UseSettingsReturn) => ReactNode) {
  render(<SettingsHarness>{children}</SettingsHarness>);
}

beforeEach(async () => {
  resetTauriMocks();
  await i18n.changeLanguage("en");
  stored = {};
  setStore({});
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return stored;
      case "remembered_unlock_state":
        return { supported: false, active: false };
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("GeneralTab explains what the fields do", () => {
  function renderGeneral(initial: Record<string, Record<string, string>> = {}) {
    stored = initial;
    renderTab((s) => <GeneralTab s={s} />);
  }

  /**
   * `Region Format` e um template: o backend troca `<city>`, `<region>`,
   * `<country>`, `<countryCode>` e `<ip>` e mantem o resto do texto
   * (api/roblox/server_regions.rs:75-91). A tela pedia o template sem dizer
   * nenhum token — nao havia como acertar sem ler o Rust.
   */
  it("lists the tokens the region template accepts", async () => {
    renderGeneral();
    expect(
      await screen.findByText(
        "Tokens: <city>, <region>, <country>, <countryCode>, <ip>. Any other text is kept as typed."
      )
    ).toBeInTheDocument();
  });

  /**
   * Um template que nao resolve nada cai no IP cru
   * (server_regions.rs:85-90) — o exemplo mostra o resultado real na coluna
   * de regiao da aba Servers.
   */
  it("shows what the region template produces", async () => {
    renderGeneral();
    expect(
      await screen.findByText(
        'Example: "<city>, <countryCode>" shows as "Ashburn, US" in the server list. A template that fills in empty falls back to the raw IP.'
      )
    ).toBeInTheDocument();
  });

  /**
   * `HiddenNameLetters` e o prefixo que sobra visivel no mascaramento
   * (`maskName`, components/accounts/AccountRow.tsx:8-13): `0` esconde o nome
   * inteiro. "Preview Letters" sozinho nao diz nada disso.
   */
  it("explains the hidden-name preview length", async () => {
    renderGeneral();
    expect(
      await screen.findByText(
        "First letters kept visible while names are hidden; the rest turns into asterisks. 0 hides the whole name."
      )
    ).toBeInTheDocument();
  });

  /**
   * `MaxRecentGames` corta a lista de recentes (`addRecentGame`,
   * components/server-list/types.ts:155-159) — nao tem relacao com o historico
   * do Roblox.
   */
  it("explains what the recent games cap applies to", async () => {
    renderGeneral();
    expect(
      await screen.findByText("How many games the Recent list keeps before the oldest one drops off.")
    ).toBeInTheDocument();
  });
});

describe("MiscellaneousTab explains what the fields do", () => {
  /**
   * O intervalo de presenca e `max(30s, minutos)` e so roda com
   * `ShowPresence` ligado (store.tsx:2064-2077).
   */
  it("explains the presence refresh interval", async () => {
    stored = {};
    renderTab((s) => <MiscellaneousTab s={s} />);
    expect(
      await screen.findByText(
        "How often the online status of every account is re-checked. Needs Show Presence on, and never runs faster than every 30 seconds."
      )
    ).toBeInTheDocument();
  });
});

describe("OptimizationTab explains what the fields do", () => {
  function renderOptimization(initial: Record<string, Record<string, string>> = {}) {
    stored = initial;
    setStore({ platformCapabilities: { os: "windows" } as PlatformCapabilities });
    renderTab((s) => <OptimizationTab s={s} />);
  }

  /**
   * `UnlockFPS` escreve `DFIntTaskSchedulerTargetFps` no
   * ClientAppSettings.json e `FramerateCap` no GlobalBasicSettings_13.xml
   * (platform/windows/client_settings.rs:36 e :155).
   */
  it("explains what Unlock FPS writes", async () => {
    renderOptimization();
    expect(
      await screen.findByText(
        "Lifts the client's frame cap to the Max FPS below by writing DFIntTaskSchedulerTargetFps before each launch."
      )
    ).toBeInTheDocument();
  });

  /**
   * O arquivo apontado aqui e COPIADO por cima do ClientAppSettings.json da
   * instalacao (platform/windows/client_settings.rs:221-235) e desliga o
   * Unlock FPS e as fast flags geradas (launch_shared.rs:274-288).
   */
  it("explains that Custom ClientSettings overwrites the client file", async () => {
    renderOptimization();
    expect(
      await screen.findByText(
        "Your own ClientAppSettings.json: the file is copied over the installed client's one at launch and takes over from Unlock FPS and the fast flags below."
      )
    ).toBeInTheDocument();
  });

  /**
   * `PriorityClass` vira `SetPriorityClass` no processo do cliente, e
   * `BackgroundMode` ignora a escolha forcando IDLE
   * (platform/windows/optimization.rs:278-292).
   */
  it("explains the Windows priority class picker", async () => {
    renderOptimization({ Optimization: { NormalEnableProcessPolicy: "true" } });
    expect(
      await screen.findByText(
        "Windows CPU scheduling priority for the Roblox process. Background Mode overrides this with Idle."
      )
    ).toBeInTheDocument();
  });

  /**
   * `MemoryPriority` vira `ProcessMemoryPriority`
   * (platform/windows/optimization.rs:297-315): decide de quem o Windows tira
   * memoria primeiro quando a RAM aperta.
   */
  it("explains the Windows memory priority picker", async () => {
    renderOptimization({ Optimization: { NormalEnableProcessPolicy: "true" } });
    expect(
      await screen.findByText(
        "How readily Windows takes memory away from this client before other processes when RAM runs short."
      )
    ).toBeInTheDocument();
  });

  /**
   * A espera acontece depois de o PID aparecer e antes de aplicar a politica
   * (launch_shared.rs:497-503).
   */
  it("explains what the process policy delay waits for", async () => {
    renderOptimization({ Optimization: { NormalEnableProcessPolicy: "true" } });
    expect(
      await screen.findByText(
        "Waits this long after the Roblox process shows up before applying the policy to it."
      )
    ).toBeInTheDocument();
  });
});

describe("GeneratorTab says who BloxGen is and what it costs", () => {
  /**
   * BloxGen e um servico externo (`https://core.bloxgen.net`) com chave de API
   * e saldo em dinheiro: `generator_test_key` le `/api/balance`
   * (commands/generators.rs:1, :705-741). A tela pedia endpoint e API key sem
   * dizer que era de terceiro nem que era pago.
   */
  it("says the provider is a paid third-party service", async () => {
    stored = {};
    renderTab((s) => <GeneratorTab s={s} />);
    expect(
      await screen.findByText(
        "BloxGen is a paid service run by someone else, not part of this app: you buy credit on their site, paste the API key below, and the accounts come from them. Test API key shows the credit left."
      )
    ).toBeInTheDocument();
  });

  /**
   * A alternativa gratuita mora na outra aba do mesmo dialogo e na entrada
   * "Create Accounts" do menu `Add` (docs/features/account-creation.md): o app
   * abre e preenche o formulario de cadastro do Roblox, o usuario resolve o
   * CAPTCHA e confirma. O nome usado aqui e o do menu/doc, nao o rotulo da
   * aba — o rotulo esta sendo unificado noutro lugar.
   */
  it("points at the free alternative next to it", async () => {
    stored = {};
    renderTab((s) => <GeneratorTab s={s} />);
    expect(
      await screen.findByText(
        "Free alternative: Create Accounts fills Roblox's own signup form for you — you solve the CAPTCHA and confirm, and nothing is charged."
      )
    ).toBeInTheDocument();
  });
});
