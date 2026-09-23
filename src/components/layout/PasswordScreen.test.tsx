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
import { resetTauriMocks } from "../../test-utils/tauriMocks";
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
    expect(store.unlock).toHaveBeenCalledWith("hunter2");
  });

  it("unlocks on Enter", async () => {
    const store = renderPasswordScreen();
    await userEvent.type(passwordBox(), "hunter2{Enter}");
    expect(store.unlock).toHaveBeenCalledWith("hunter2");
  });

  it("ignores Enter while the field is empty", async () => {
    const store = renderPasswordScreen();
    passwordBox().focus();
    await userEvent.keyboard("{Enter}");
    expect(store.unlock).not.toHaveBeenCalled();
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

  it("applies the default method without asking for a password", async () => {
    const store = renderSetup();
    await userEvent.click(screen.getByRole("button", { name: /Default Encryption/ }));
    expect(screen.queryByPlaceholderText("Create Encryption Password")).not.toBeInTheDocument();

    await userEvent.click(applyButton());
    expect(store.applyEncryptionMethod).toHaveBeenCalledWith("default", undefined);
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
