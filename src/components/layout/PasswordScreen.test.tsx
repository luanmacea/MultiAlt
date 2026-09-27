import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/window", async () => (await import("../../test-utils/tauriMocks")).tauriWindowMock());

import { PasswordScreen } from "./PasswordScreen";
import { EncryptionSetupScreen } from "./EncryptionSetupScreen";
import { defaultSettings, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";
import type { StoreValue } from "../../store";

/** The WebGL backgrounds need a GL context; the SVG "waves" style does not. */
function settingsWithWavesBackground() {
  const settings = defaultSettings();
  settings.General.RestrictedBackgroundStyle = "waves";
  return settings;
}

function renderPasswordScreen(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ settings: settingsWithWavesBackground(), ...overrides });
  render(<PasswordScreen />);
  return store;
}

function passwordBox(): HTMLInputElement {
  return screen.getByPlaceholderText("Password") as HTMLInputElement;
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

describe("PasswordScreen", () => {
  it("asks for the password", () => {
    renderPasswordScreen();
    expect(screen.getByText("Restricted Access")).toBeInTheDocument();
    expect(screen.getByText("Enter your password to continue")).toBeInTheDocument();
  });

  it("keeps Continue disabled until something is typed", async () => {
    renderPasswordScreen();
    const button = screen.getByRole("button", { name: "Continue" });
    expect(button).toBeDisabled();

    await userEvent.type(passwordBox(), "hunter2");
    expect(button).toBeEnabled();
  });

  it("unlocks with the typed password", async () => {
    const store = renderPasswordScreen();
    await userEvent.type(passwordBox(), "hunter2");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(store.unlock).toHaveBeenCalledWith("hunter2", undefined);
  });

  it("unlocks on Enter", async () => {
    const store = renderPasswordScreen();
    await userEvent.type(passwordBox(), "hunter2{Enter}");
    expect(store.unlock).toHaveBeenCalledWith("hunter2", undefined);
  });

  it("ignores Enter while the field is empty", async () => {
    const store = renderPasswordScreen();
    passwordBox().focus();
    await userEvent.keyboard("{Enter}");
    expect(store.unlock).not.toHaveBeenCalled();
  });

  /**
   * "Lembrar de mim" guarda a senha protegida pelo SO. A caixa só existe onde
   * há essa proteção (DPAPI do Windows) — sem ela, guardar a senha em disco
   * seria pior que digitá-la.
   */
  describe("lembrar de mim", () => {
    function checkbox() {
      return screen.getByRole("checkbox", {
        name: /Keep me signed in for 24 hours/i,
      }) as HTMLInputElement;
    }

    it("oferece a caixa quando o sistema guarda a senha com proteção", async () => {
      setInvokeMap({
        remembered_unlock_state: { supported: true, active: false, defaultHours: 24 },
      });
      renderPasswordScreen();
      expect(await screen.findByRole("checkbox")).not.toBeChecked();
    });

    it("não oferece a caixa onde não há proteção do sistema", async () => {
      setInvokeMap({
        remembered_unlock_state: { supported: false, active: false, defaultHours: 24 },
      });
      renderPasswordScreen();
      await userEvent.type(passwordBox(), "hunter2");
      expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    });

    it("manda o prazo junto ao destrancar com a caixa marcada", async () => {
      setInvokeMap({
        remembered_unlock_state: { supported: true, active: false, defaultHours: 24 },
      });
      const store = renderPasswordScreen();
      await screen.findByRole("checkbox");

      await userEvent.click(checkbox());
      await userEvent.type(passwordBox(), "hunter2{Enter}");
      expect(store.unlock).toHaveBeenCalledWith("hunter2", 24);
    });

    /**
     * Chegar nesta tela com um lembrete guardado significa que ele não serviu
     * (senha trocada, prazo vencido): a caixa não pode vir marcada dando a
     * entender que está tudo certo.
     */
    it("começa desmarcada mesmo com um lembrete guardado", async () => {
      setInvokeMap({
        remembered_unlock_state: { supported: true, active: true, defaultHours: 24 },
      });
      renderPasswordScreen();
      expect(await screen.findByRole("checkbox")).not.toBeChecked();
    });
  });

  it("shows the unlock error from the store", () => {
    renderPasswordScreen({ error: "Wrong password" });
    expect(screen.getByText("Wrong password")).toBeInTheDocument();
  });

  it("disables the button and shows progress while unlocking", () => {
    renderPasswordScreen({ unlocking: true });
    expect(screen.getByRole("button", { name: "Unlocking..." })).toBeDisabled();
  });
});

describe("EncryptionSetupScreen", () => {
  function renderSetup(overrides: Partial<StoreValue> = {}) {
    const store = setStore({ encryptionSetupMode: "firstRun", ...overrides });
    render(<EncryptionSetupScreen />);
    return store;
  }

  const newPassword = () => screen.getByPlaceholderText("Create Encryption Password");
  const confirmPassword = () => screen.getByPlaceholderText("Confirm Encryption Password");
  const applyButton = () => screen.getByRole("button", { name: /Continue|Apply Encryption/ });

  it("defaults to the password method on first run and hides Cancel", () => {
    renderSetup();
    expect(screen.getByText("Set Up Encryption")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Pass Lock/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("offers Cancel when opened from settings", async () => {
    const store = renderSetup({ encryptionSetupMode: "settings", accountsEncrypted: true });
    expect(screen.getByText("Change Encryption Method")).toBeInTheDocument();
    expect(screen.getByText("Current method: Password Lock")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(store.closeEncryptionSetup).toHaveBeenCalledTimes(1);
  });

  it("rejects an empty password", async () => {
    const store = renderSetup();
    await userEvent.click(applyButton());
    expect(await screen.findByText("Please enter an encryption password")).toBeInTheDocument();
    expect(store.applyEncryptionMethod).not.toHaveBeenCalled();
  });

  it("rejects a password under 8 characters", async () => {
    const store = renderSetup();
    await userEvent.type(newPassword(), "short");
    await userEvent.type(confirmPassword(), "short");
    await userEvent.click(applyButton());
    expect(await screen.findByText("Password must be at least 8 characters")).toBeInTheDocument();
    expect(store.applyEncryptionMethod).not.toHaveBeenCalled();
  });

  it("rejects a mismatched confirmation", async () => {
    const store = renderSetup();
    await userEvent.type(newPassword(), "longenough1");
    await userEvent.type(confirmPassword(), "longenough2");
    await userEvent.click(applyButton());
    expect(await screen.findByText("Passwords do not match")).toBeInTheDocument();
    expect(store.applyEncryptionMethod).not.toHaveBeenCalled();
  });

  it("applies a valid password", async () => {
    const store = renderSetup();
    await userEvent.type(newPassword(), "longenough1");
    await userEvent.type(confirmPassword(), "longenough1");
    await userEvent.click(applyButton());
    expect(store.applyEncryptionMethod).toHaveBeenCalledWith("password", "longenough1");
  });

  it("submits on Enter from the confirmation field", async () => {
    const store = renderSetup();
    await userEvent.type(newPassword(), "longenough1");
    await userEvent.type(confirmPassword(), "longenough1{Enter}");
    expect(store.applyEncryptionMethod).toHaveBeenCalledWith("password", "longenough1");
  });

  it("applies the no-password method without asking for a password", async () => {
    const store = renderSetup();
    await userEvent.click(screen.getByRole("button", { name: /No Password/ }));
    expect(screen.queryByPlaceholderText("Create Encryption Password")).not.toBeInTheDocument();

    await userEvent.click(applyButton());
    expect(store.applyEncryptionMethod).toHaveBeenCalledWith("default", undefined);
  });

  // Regressão, duas vezes no mesmo texto: primeiro a opção sem senha se chamava
  // "Default Encryption" e gravava JSON puro (o rótulo escondia isso); agora ela
  // **é** criptografada, pela chave do aparelho, e o rótulo antigo ("Not
  // Encrypted", "plain JSON") passou a ser a mentira oposta. O texto tem que
  // dizer onde a chave fica, senão o usuário escolhe sem saber o que ganhou.
  it("says the no-password option is locked with a device key, not plain JSON", async () => {
    renderSetup();
    expect(screen.queryByText(/Default Encryption/)).not.toBeInTheDocument();
    expect(screen.queryByText(/local default protection/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/plain JSON/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Not Encrypted/i)).not.toBeInTheDocument();

    const plainOption = screen.getByRole("button", { name: /No Password/ });
    expect(plainOption).toHaveTextContent(/Device Key/i);
    expect(plainOption).toHaveTextContent(/encrypted with a key stored on this device/i);
  });

  // O limite da proteção tem que estar na tela: ela para arquivo copiado,
  // backup vazado e outro usuário do PC, e **não** para malware rodando como o
  // próprio usuário. Vender mais que isso é vender proteção que não existe.
  it("states the real limit of the device key once the no-password option is picked", async () => {
    renderSetup();
    await userEvent.click(screen.getByRole("button", { name: /No Password/ }));
    const warning = screen.getByText(/the key sits in a file next to AccountData\.json/i);
    expect(warning).toBeInTheDocument();
    expect(warning).toHaveTextContent(/copied file/i);
    expect(warning).toHaveTextContent(/leaked backup/i);
    expect(warning).toHaveTextContent(/another user on this PC/i);
    expect(warning).toHaveTextContent(/but not a program running as you/i);
    expect(
      screen.getByText(
        "You can continue without a password: the vault is locked with this device key instead.",
      ),
    ).toBeInTheDocument();
  });

  it("describes the current method honestly when there is no password", () => {
    renderSetup({ encryptionSetupMode: "settings", accountsEncrypted: false });
    expect(
      screen.getByText("Current method: Device Key (no password)"),
    ).toBeInTheDocument();
  });

  it("surfaces the store's encryption error", () => {
    renderSetup({ encryptionSetupError: "Could not re-encrypt AccountData.json" });
    expect(screen.getByText("Could not re-encrypt AccountData.json")).toBeInTheDocument();
  });

  it("disables the apply button while encryption runs", () => {
    renderSetup({ applyingEncryption: true });
    expect(screen.getByRole("button", { name: "Applying encryption..." })).toBeDisabled();
  });
});
