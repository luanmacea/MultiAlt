import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { EmergencyStopAlert } from "./EmergencyStopAlert";
import { setStore } from "../../test-utils/renderWithStore";

afterEach(cleanup);

/**
 * A parada por chacoalhar o mouse avisava num toast que sumia em um segundo
 * e numa linha amarela pequena (pedido do dono, 11/10/2026: "mal dá pra
 * ler"). Agora é um alerta no meio da janela, que fica até a pessoa fechar.
 */
describe("EmergencyStopAlert", () => {
  it("sem parada, não mostra nada", () => {
    setStore({ afkEmergencyStop: null });
    render(<EmergencyStopAlert />);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("mostra o alerta com o que parou e que nenhum cliente fechou", () => {
    setStore({ afkEmergencyStop: { reason: "shake", afk: true, recordings: true, atMs: 1 } });
    render(<EmergencyStopAlert />);
    const alert = screen.getByRole("alertdialog", { name: "Emergency stop" });
    expect(alert).toHaveTextContent("You shook the mouse, so MultiAlt stopped AFK mode and the recording.");
    expect(alert).toHaveTextContent("No Roblox client was closed.");
  });

  it("diz só o que estava rodando", () => {
    setStore({ afkEmergencyStop: { reason: "shake", afk: false, recordings: true, atMs: 1 } });
    render(<EmergencyStopAlert />);
    expect(screen.getByRole("alertdialog")).toHaveTextContent("You shook the mouse, so MultiAlt stopped the recording.");
  });

  it("OK fecha o alerta (e o aviso da tela junto)", async () => {
    const store = setStore({ afkEmergencyStop: { reason: "shake", afk: true, recordings: false, atMs: 1 } });
    render(<EmergencyStopAlert />);
    const ok = screen.getByRole("button", { name: "OK" });
    expect(ok).toHaveFocus();
    await userEvent.click(ok);
    expect(store.dismissAfkEmergencyStop).toHaveBeenCalled();
  });

  it("Esc também fecha", async () => {
    const store = setStore({ afkEmergencyStop: { reason: "shake", afk: true, recordings: false, atMs: 1 } });
    render(<EmergencyStopAlert />);
    await userEvent.keyboard("{Escape}");
    expect(store.dismissAfkEmergencyStop).toHaveBeenCalled();
  });
});
