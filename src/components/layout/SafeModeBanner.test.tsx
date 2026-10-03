import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { SafeModeBanner } from "./SafeModeBanner";
import { invokeMock, resetTauriMocks, setInvokeMap } from "../../test-utils/tauriMocks";

/**
 * Quem cai no safe mode de vídeo roda com a GPU desligada. Sem esta faixa o
 * usuário não descobre isso nunca: o app pinta, o watchdog não dispara mais, e
 * a única saída seria apagar `webview.safemode` à mão.
 */
function renderBanner(state: { active: boolean; sticky: boolean } | undefined) {
  setInvokeMap({ get_webview_safe_mode: state });
  render(<SafeModeBanner />);
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

describe("SafeModeBanner", () => {
  it("não aparece quando o app está no modo normal", async () => {
    renderBanner({ active: false, sticky: false });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_webview_safe_mode"));
    expect(screen.queryByText("Graphics safe mode is on")).not.toBeInTheDocument();
  });

  it("não aparece quando o backend não responde", async () => {
    renderBanner(undefined);
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(screen.queryByText("Graphics safe mode is on")).not.toBeInTheDocument();
  });

  it("avisa que o safe mode está ligado e oferece a saída", async () => {
    renderBanner({ active: true, sticky: true });
    expect(await screen.findByText("Graphics safe mode is on")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Back to normal mode" })).toBeInTheDocument();
  });

  it("sem marcador não oferece saída: não há o que apagar", async () => {
    renderBanner({ active: true, sticky: false });
    expect(await screen.findByText("Graphics safe mode is on")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Back to normal mode" })).not.toBeInTheDocument();
  });

  it("avisa quando só a próxima abertura vem em safe mode", async () => {
    renderBanner({ active: false, sticky: true });
    expect(
      await screen.findByText("The next time MultiAlt opens it will use graphics safe mode")
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Back to normal mode" })).toBeInTheDocument();
  });

  it("o botão manda o backend apagar o marcador", async () => {
    renderBanner({ active: true, sticky: true });
    await userEvent.click(await screen.findByRole("button", { name: "Back to normal mode" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("leave_webview_safe_mode")
    );
  });

  it("erro do backend aparece na faixa, com o caminho do arquivo", async () => {
    setInvokeMap({
      get_webview_safe_mode: { active: true, sticky: true },
      leave_webview_safe_mode: () => {
        throw new Error("Could not delete C:\\dados\\webview.safemode: acesso negado.");
      },
    });
    render(<SafeModeBanner />);
    await userEvent.click(await screen.findByRole("button", { name: "Back to normal mode" }));
    expect(await screen.findByText(/webview\.safemode/)).toBeInTheDocument();
  });
});
