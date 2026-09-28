import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { AccountLaunchOverrides } from "./AccountLaunchOverrides";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import type { Account } from "../../types";

function renderSection(account: Account) {
  const store = setStore({ accounts: [account], selectedAccounts: [account] });
  render(<AccountLaunchOverrides account={account} />);
  return store;
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("AccountLaunchOverrides", () => {
  it("keeps the fields hidden until the exception is turned on", () => {
    renderSection(makeAccount({ UserID: 7 }));
    expect(screen.queryByLabelText("FPS")).not.toBeInTheDocument();
  });

  it("turning it on writes the switch into the account fields", async () => {
    const account = makeAccount({ UserID: 7, Fields: { RobloxVersion: "LIVE:abc" } });
    const store = renderSection(account);

    await userEvent.click(screen.getByRole("checkbox"));

    expect(store.updateAccount).toHaveBeenCalledTimes(1);
    const enviada = (store.updateAccount as unknown as { mock: { calls: [Account][] } }).mock
      .calls[0][0];
    expect(enviada.Fields.ClientOverridesEnabled).toBe("true");
    // Um campo que já estava lá não pode ser levado embora pela gravação.
    expect(enviada.Fields.RobloxVersion).toBe("LIVE:abc");
  });

  it("shows the stored exception when the account already has one", () => {
    renderSection(
      makeAccount({
        UserID: 7,
        Fields: {
          ClientOverridesEnabled: "true",
          ClientOverrideMaxFPS: "240",
          ClientOverrideVolume: "0.200",
        },
      })
    );
    expect(screen.getByLabelText("FPS")).toHaveValue("240");
    // O arquivo guarda fração; a tela mostra a escala do jogo.
    expect(screen.getByLabelText("Volume")).toHaveValue("2");
  });

  it("saves the FPS typed into the field", async () => {
    const account = makeAccount({ UserID: 7, Fields: { ClientOverridesEnabled: "true" } });
    const store = renderSection(account);

    await userEvent.type(screen.getByLabelText("FPS"), "240{Enter}");

    const calls = (store.updateAccount as unknown as { mock: { calls: [Account][] } }).mock.calls;
    expect(calls[calls.length - 1][0].Fields.ClientOverrideMaxFPS).toBe("240");
  });

  /**
   * O Volume gravava certo (15 vira 10: o clamp de `writeAccountLaunchOverrides`)
   * mas o campo continuava **mostrando** 15 depois de sair dele, sem aviso do
   * ajuste, até a conta ser reselecionada — validado no harness. O campo tem
   * que mostrar o que foi gravado.
   */
  it("depois de sair do campo, o Volume mostra o valor gravado, não o de fora da faixa", async () => {
    const account = makeAccount({ UserID: 7, Fields: { ClientOverridesEnabled: "true" } });
    const store = renderSection(account);
    const volume = screen.getByLabelText("Volume");

    await userEvent.type(volume, "15");
    await userEvent.tab();

    const calls = (store.updateAccount as unknown as { mock: { calls: [Account][] } }).mock.calls;
    expect(calls[calls.length - 1][0].Fields.ClientOverrideVolume).toBe("1.000");
    expect(volume).toHaveValue("10");
  });

  it("um número malformado aparece como foi entendido, não como foi digitado", async () => {
    // `1.2.3` passa pelo filtro de caracteres e é gravado como 1.2 (0.120).
    const account = makeAccount({ UserID: 7, Fields: { ClientOverridesEnabled: "true" } });
    const store = renderSection(account);
    const volume = screen.getByLabelText("Volume");

    await userEvent.type(volume, "1.2.3{Enter}");

    const calls = (store.updateAccount as unknown as { mock: { calls: [Account][] } }).mock.calls;
    expect(calls[calls.length - 1][0].Fields.ClientOverrideVolume).toBe("0.120");
    expect(volume).toHaveValue("1.2");
  });

  it("only asks for a window size when the account is set to windowed", async () => {
    const account = makeAccount({
      UserID: 7,
      Fields: { ClientOverridesEnabled: "true", ClientOverrideFullscreen: "true" },
    });
    renderSection(account);
    expect(screen.queryByLabelText("Window width")).not.toBeInTheDocument();

    cleanup();
    renderSection(
      makeAccount({
        UserID: 7,
        Fields: { ClientOverridesEnabled: "true", ClientOverrideFullscreen: "false" },
      })
    );
    expect(screen.getByLabelText("Window width")).toBeInTheDocument();
  });
});
