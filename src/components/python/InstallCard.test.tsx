import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { InstallCard, type InstallCardProps } from "./InstallCard";
import { usePythonRuntime } from "./usePythonRuntime";
import type { PythonRuntimeStatus } from "./pythonApi";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

const NOT_INSTALLED: PythonRuntimeStatus = { installed: false, version: null, pythonExe: null, packages: [], installing: false };
const INSTALLED: PythonRuntimeStatus = {
  installed: true,
  version: "3.12.7",
  pythonExe: "C:\Data\python-runtime\current\python.exe",
  packages: ["numpy 2.1.0", "opencv-python-headless 4.10.0"],
  installing: false,
};

function props(overrides: Partial<InstallCardProps> = {}): InstallCardProps {
  return {
    status: NOT_INSTALLED,
    progress: null,
    error: null,
    installing: false,
    onInstall: vi.fn(),
    onReinstall: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});
afterEach(cleanup);

describe("InstallCard", () => {
  it("offers the install button and says what it will download, without installing by itself", async () => {
    const p = props();
    render(<InstallCard {...p} />);
    expect(screen.getByText("Python is not installed")).toBeInTheDocument();
    expect(screen.getByText(/python.org/)).toBeInTheDocument();
    // pip is a wheel from PyPI (files.pythonhosted.org), like numpy and OpenCV: nothing from bootstrap.pypa.io.
    expect(screen.getByText("pip, from PyPI (files.pythonhosted.org)")).toBeInTheDocument();
    expect(screen.queryByText(/bootstrap\.pypa\.io/)).not.toBeInTheDocument();
    expect(screen.getByText(/numpy and opencv-python-headless/)).toHaveTextContent("PyPI");
    expect(p.onInstall).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: /Install Python/ }));
    expect(p.onInstall).toHaveBeenCalledTimes(1);
  });

  it("shows the stage and a progress bar while installing", () => {
    render(
      <InstallCard
        {...props({
          installing: true,
          progress: { stage: "python", message: "Downloading python.zip", downloaded: 10 * 1024 * 1024, total: 40 * 1024 * 1024 },
        })}
      />
    );
    expect(screen.getByText(/Downloading Python/)).toBeInTheDocument();
    expect(screen.getByText("Downloading python.zip")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25");
  });

  it("shows an indeterminate bar when the size is unknown", () => {
    render(<InstallCard {...props({ installing: true, progress: { stage: "packages", message: "pip install", downloaded: 0, total: 0 } })} />);
    expect(screen.getByRole("progressbar")).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByText(/Installing numpy and OpenCV/)).toBeInTheDocument();
  });

  it("shows the error with a retry button", async () => {
    const p = props({ error: "Could not reach python.org" });
    render(<InstallCard {...p} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Could not reach python.org");
    await userEvent.click(screen.getByRole("button", { name: /Try again/ }));
    expect(p.onInstall).toHaveBeenCalledTimes(1);
  });

  it("shows the installed version and packages", () => {
    render(<InstallCard {...props({ status: INSTALLED })} />);
    expect(screen.getByText(/Python 3.12.7 installed/)).toBeInTheDocument();
    expect(screen.getByText(/numpy 2.1.0/)).toBeInTheDocument();
  });

  it("copies the pip install hint", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    render(<InstallCard {...props({ status: INSTALLED })} />);
    await userEvent.click(screen.getByRole("button", { name: /Copy pip install command/ }));
    expect(writeText).toHaveBeenCalledWith('"C:\Data\python-runtime\current\python.exe" -m pip install <package>');
  });

  it("reinstalls only after the user confirms", async () => {
    const p = props({ status: INSTALLED });
    render(<InstallCard {...p} />);

    promptAnswers.confirm = false;
    await userEvent.click(screen.getByRole("button", { name: /Reinstall/ }));
    expect(p.onReinstall).not.toHaveBeenCalled();

    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: /Reinstall/ }));
    await waitFor(() => expect(p.onReinstall).toHaveBeenCalledTimes(1));
  });
});

function Harness() {
  const runtime = usePythonRuntime();
  return (
    <InstallCard
      status={runtime.status}
      progress={runtime.progress}
      error={runtime.error}
      installing={runtime.installing}
      onInstall={() => void runtime.install()}
      onReinstall={() => void runtime.reinstall()}
    />
  );
}

describe("usePythonRuntime", () => {
  it("downloads nothing until the button is pressed, then follows the progress events", async () => {
    let installed = false;
    let finish: () => void = () => {};
    setInvokeHandler((cmd) => {
      if (cmd === "python_runtime_status") return installed ? INSTALLED : NOT_INSTALLED;
      if (cmd === "python_install_runtime") {
        return new Promise<void>((resolve) => {
          finish = () => {
            installed = true;
            resolve();
          };
        });
      }
      return undefined;
    });

    render(<Harness />);
    expect(await screen.findByText("Python is not installed")).toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "python_install_runtime")).toBe(false);

    await userEvent.click(screen.getByRole("button", { name: /Install Python/ }));
    expect(invokeMock).toHaveBeenCalledWith("python_install_runtime");

    act(() => emitTauriEvent("python-install-progress", { stage: "pip", message: "Running get-pip", downloaded: 0, total: 0 }));
    expect(await screen.findByText("Running get-pip")).toBeInTheDocument();

    await act(async () => finish());
    expect(await screen.findByText(/Python 3.12.7 installed/)).toBeInTheDocument();
  });

  it("surfaces a failed install and lets the user retry", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "python_runtime_status") return NOT_INSTALLED;
      if (cmd === "python_install_runtime") throw "hash mismatch for python.zip";
      return undefined;
    });
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: /Install Python/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("hash mismatch for python.zip");
    expect(screen.getByRole("button", { name: /Try again/ })).toBeInTheDocument();
  });
});
