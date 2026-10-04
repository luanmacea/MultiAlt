/**
 * Anda por um tutorial de tela inteiro, do botão Tutorial ao Done, conferindo
 * em cada passo que o alvo está na tela renderizada e que nenhum passo mexeu
 * em dado (só leitura passa pelo `invoke`).
 *
 * Uso, num teste de página que já tem os mocks dela:
 * ```ts
 * renderWithStore(<SessionPage active onLeave={vi.fn()} />, {...});
 * await walkTour("session", { invoke: invokeMock });
 * ```
 * A página precisa estar na tela; o `ScreenTourHost` é montado aqui, a não ser
 * que a árvore já o tenha (o App monta o dele: passe `withHost: false`).
 */
import { expect } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Mock } from "vitest";
import { ScreenTourHost } from "../components/tour/ScreenTour";
import { TOURS, type TourId } from "../components/tour/tours";

/** Comando que muda alguma coisa: nenhum passo de tutorial pode chamar. */
const MUTATING = /^(update_|save_|delete_|remove_|add_|create_|set_|start_|stop_|launch|join|kill|close_|cmd_kill|cancel_|restore_|import_|export_|install_|uninstall_|apply_|avatar_apply|avatar_save|avatar_delete|arrange_)/;

/**
 * Casam com o padrão acima mas não mexem em dado: `stop_server_scan` só para a
 * varredura de servidores quando a aba Servers sai da tela.
 */
const HARMLESS = new Set(["stop_server_scan"]);

export interface WalkTourOptions {
  /** Passos que podem cair no centro (alvo que o cenário do teste não tem). */
  allowMissing?: string[];
  /** O `invokeMock` do teste, para conferir que nada foi alterado. */
  invoke?: Mock;
  withHost?: boolean;
}

export async function walkTour(tourId: TourId, options: WalkTourOptions = {}) {
  const { allowMissing = [], invoke, withHost = true } = options;
  if (withHost) render(<ScreenTourHost />);

  const callsBefore = invoke?.mock.calls.length ?? 0;
  const tour = TOURS[tourId];

  const button = screen
    .getAllByRole("button", { name: /Tutorial/ })
    .find((el) => el.getAttribute("data-tour") === "tour-button");
  expect(button, "Tutorial button on the screen").toBeTruthy();
  await userEvent.click(button!);

  for (const [i, step] of tour.steps.entries()) {
    const dialog = await screen.findByRole("dialog");
    await waitFor(() => expect(dialog).toHaveAttribute("data-step-id", step.id));
    expect(within(dialog).getByText(`Step ${i + 1} of ${tour.steps.length}`)).toBeInTheDocument();
    if (!allowMissing.includes(step.id)) {
      await waitFor(() => expect(dialog, `anchor of step "${step.id}"`).toHaveAttribute("data-target-found", "true"));
    }
    const isLast = i === tour.steps.length - 1;
    await userEvent.click(within(dialog).getByRole("button", { name: isLast ? "Done" : /Next/ }));
  }

  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

  if (invoke) {
    const during = invoke.mock.calls.slice(callsBefore).map((call) => String(call[0]));
    expect(
      during.filter((cmd) => MUTATING.test(cmd) && !HARMLESS.has(cmd)),
      "commands that change data"
    ).toEqual([]);
  }
}
