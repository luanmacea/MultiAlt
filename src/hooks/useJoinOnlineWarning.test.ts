import { act, cleanup, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import path from "node:path";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
  isTauri: () => false,
  convertFileSrc: (p: string) => p,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

import i18n from "../i18n";
import enCommon from "../locales/en/common.json";
import { StoreProvider, useStore } from "../store";
import { PromptProvider } from "./usePrompt";
import { presenceLabelKey, useJoinOnlineWarning } from "./useJoinOnlineWarning";

function account(userId: number, overrides: Partial<Account> = {}): Account {
  return {
    Valid: true,
    SecurityToken: "t",
    Username: `user${userId}`,
    LastUse: new Date().toISOString(),
    Alias: "",
    Description: "",
    Password: "",
    Group: "",
    UserID: userId,
    Fields: {},
    LastAttemptedRefresh: new Date().toISOString(),
    BrowserTrackerID: "",
    ...overrides,
  };
}

let accountsData: Account[] = [];
let settingsData: Record<string, Record<string, string>> = {};
let presenceRows: Array<Record<string, number>> = [];
let presenceFails = false;

function invokeCalls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

/** A fila de toasts guarda objetos (`{ id, message, tone }`), não strings. */
function toastText(toasts: { message: string }[]) {
  return toasts.map((toast) => toast.message).join(" ");
}

async function renderWarning() {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(StoreProvider, null, createElement(PromptProvider, null, children));
  const view = renderHook(
    () => ({ confirmJoin: useJoinOnlineWarning(), store: useStore() }),
    { wrapper }
  );
  await waitFor(() => expect(view.result.current.store.initialized).toBe(true));
  return view;
}

beforeEach(() => {
  invokeMock.mockReset();
  accountsData = [account(1, { Alias: "Main" }), account(2), account(3), account(4), account(5)];
  settingsData = {};
  presenceRows = [];
  presenceFails = false;
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "needs_password":
        return false;
      case "get_accounts":
        return accountsData;
      case "get_all_settings":
        return settingsData;
      case "get_presence":
        if (presenceFails) throw new Error("presence down");
        return presenceRows;
      case "is_accounts_encrypted":
        return false;
      case "batched_get_avatar_headshots":
        return [];
      case "get_running_instances":
        return [];
      case "get_theme":
        return {};
      default:
        return null;
    }
  });
});

afterEach(() => {
  cleanup();
});

describe("useJoinOnlineWarning", () => {
  it("allows an empty selection without asking anything", async () => {
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([]);
    });

    expect(allowed).toBe(true);
    expect(invokeCalls("get_presence")).toHaveLength(0);
  });

  it("allows the join when the warning is disabled in settings", async () => {
    settingsData = { General: { WarnOnOnlineJoin: "false" } };
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([1]);
    });

    expect(allowed).toBe(true);
    expect(invokeCalls("get_presence")).toHaveLength(0);
  });

  it("allows the join when presence cannot be fetched", async () => {
    presenceFails = true;
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([1]);
    });

    expect(allowed).toBe(true);
  });

  it("allows the join when every account is offline", async () => {
    presenceRows = [{ userId: 1, userPresenceType: 0 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([1, 1]);
    });

    expect(allowed).toBe(true);
    expect(invokeCalls("get_presence")[0][1]).toEqual({ userIds: [1] });
    expect(document.querySelector(".fixed.inset-0")).toBeNull();
  });

  it("warns with the account alias and its presence label, and joins anyway", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 1, userPresenceType: 2 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText(/Main is currently In Game/)).toBeTruthy());
    await user.click(screen.getByText("Join Anyway"));

    await waitFor(() => expect(allowed).toBe(true));
    expect(invokeCalls("update_setting")).toHaveLength(0);
  });

  it("cancels the join when the user backs out", async () => {
    const user = userEvent.setup();
    presenceRows = [{ user_id: 2, user_presence_type: 1 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([2]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText(/user2 is currently Online/)).toBeTruthy());
    await user.click(screen.getByText("Cancel"));
    await waitFor(() => expect(allowed).toBe(false));
  });

  /** O aviso punha o alias real na tela mesmo com "Names hidden" ligado. */
  it("masks the account name while names are hidden", async () => {
    const user = userEvent.setup();
    settingsData = { General: { HideUsernames: "true", HiddenNameLetters: "0" } };
    presenceRows = [{ user_id: 1, user_presence_type: 2 }];
    const { result } = await renderWarning();
    await waitFor(() => expect(result.current.store.hideUsernames).toBe(true));

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText(/\*{12} is currently In Game/)).toBeTruthy());
    expect(screen.queryByText(/Main/)).toBeNull();
    await user.click(screen.getByText("Cancel"));

    await waitFor(() => expect(allowed).toBe(false));
  });

  it("summarizes several online accounts and truncates after four", async () => {
    const user = userEvent.setup();
    presenceRows = [1, 2, 3, 4, 5].map((id) => ({ userId: id, userPresenceType: 3 }));
    const { result } = await renderWarning();

    act(() => {
      void result.current.confirmJoin([1, 2, 3, 4, 5]);
    });

    await waitFor(() => expect(screen.getByText(/5 selected accounts are already online/)).toBeTruthy());
    expect(screen.getByText(/Main \(In Studio\)/)).toBeTruthy();
    expect(screen.getByText(/and 1 more/)).toBeTruthy();

    await user.click(screen.getByText("Cancel"));
  });

  it("names unknown user ids generically", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 999, userPresenceType: 1 }];
    const { result } = await renderWarning();

    act(() => {
      void result.current.confirmJoin([999]);
    });

    await waitFor(() => expect(screen.getByText(/User 999 is currently Online/)).toBeTruthy());
    await user.click(screen.getByText("Cancel"));
  });

  it("persists the opt-out when the user ticks the checkbox", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 1, userPresenceType: 1 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText("Don't show this warning again")).toBeTruthy());
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByText("Join Anyway"));

    await waitFor(() => expect(allowed).toBe(true));
    expect(invokeCalls("update_setting").map((c) => c[1])).toContainEqual({
      section: "General",
      key: "WarnOnOnlineJoin",
      value: "false",
    });
    await waitFor(() =>
      expect(toastText(result.current.store.toasts)).toContain("Online-join warning disabled")
    );
  });

  it("does not persist the opt-out when the join is cancelled", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 1, userPresenceType: 1 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByRole("checkbox")).toBeTruthy());
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByText("Cancel"));

    await waitFor(() => expect(allowed).toBe(false));
    expect(invokeCalls("update_setting")).toHaveLength(0);
  });
});

const en = enCommon as Record<string, string>;

/**
 * As chaves que o aviso **tem** de usar, escritas aqui de propósito: é o teste
 * que guarda a frase, não o hook. Uma letra fora de lugar no hook e a tradução
 * do catálogo não casa mais — a tela volta ao inglês, que é o bug que este
 * arquivo existe para impedir.
 */
const KEYS = {
  one: "{{name}} is currently {{state}}. Joining can disconnect its existing Roblox session. Continue anyway?",
  many:
    "{{count}} selected accounts are already online: {{list}}. Joining can disconnect their existing Roblox sessions. Continue anyway?",
  more: "{{list}} and {{n}} more",
  joinAnyway: "Join Anyway",
  cancel: "Cancel",
  optOut: "Don't show this warning again",
  optOutToast: "Online-join warning disabled",
  unknownUser: "User {{id}}",
} as const;

/** Nomes dos `{{placeholder}}` de uma frase, ordenados. */
function placeholders(text: string): string[] {
  return [...text.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g)].map((m) => m[1]).sort();
}

/**
 * Traduções **de teste**, não o catálogo do app: o "[pt]" na frente existe para
 * não confundir uma com a outra. Se esse texto aparece na tela, é porque o hook
 * entregou a *chave* ao diálogo e o `t()` a resolveu — é justamente isso que o
 * contrato cobra. Frase montada por template literal nunca casaria com a chave,
 * e sairia em inglês com o catálogo inteiro traduzido.
 */
const PT_FIXTURE: Record<string, string> = {
  [KEYS.one]: "[pt] {{name}} está {{state}} agora. Continuar?",
  [KEYS.many]: "[pt] {{count}} contas já estão online: {{list}}. Continuar?",
  [KEYS.more]: "{{list}} e mais {{n}}",
  [KEYS.joinAnyway]: "[pt] Entrar mesmo assim",
  [KEYS.optOut]: "[pt] Não mostrar este aviso",
  [KEYS.optOutToast]: "[pt] Aviso desativado",
  [KEYS.unknownUser]: "[pt] Usuário {{id}}",
};

function dialogText(): string {
  return document.querySelector(".fixed.inset-0")?.textContent ?? "";
}

const hookSource = readFileSync(
  path.join(process.cwd(), "src", "hooks", "useJoinOnlineWarning.ts"),
  "utf8"
);

describe("useJoinOnlineWarning: contrato de i18n", () => {
  describe("as chaves do aviso", () => {
    it("estão no código como literal, do jeito que o extrator de chaves acha", () => {
      for (const key of [KEYS.one, KEYS.many, KEYS.more, KEYS.unknownUser]) {
        expect(hookSource).toContain(`"${key}"`);
      }
      // `scripts/i18n/extract-keys.ts` descarta de propósito qualquer literal
      // com `${`: montar a frase com template literal é o que a deixava fora do
      // catálogo, por mais traduzido que o catálogo estivesse.
      for (const phrase of ["is currently", "already online", "Continue anyway?", "User {{id}}"]) {
        const interpolated = hookSource
          .split("\n")
          .filter((line) => line.includes(phrase) && line.includes("${"));
        expect(interpolated).toEqual([]);
      }
    });

    it("usam {{placeholder}} e nunca template literal", () => {
      for (const key of Object.values(KEYS)) {
        expect(key).not.toContain("${");
      }
      expect(placeholders(KEYS.one)).toEqual(["name", "state"]);
      expect(placeholders(KEYS.many)).toEqual(["count", "list"]);
      expect(placeholders(KEYS.more)).toEqual(["list", "n"]);
      expect(placeholders(KEYS.unknownUser)).toEqual(["id"]);
      for (const key of [KEYS.joinAnyway, KEYS.cancel, KEYS.optOut, KEYS.optOutToast]) {
        expect(placeholders(key)).toEqual([]);
      }
    });

    it("têm frase separada para uma conta e para várias, em vez de '(s)'", () => {
      expect(KEYS.one).not.toMatch(/\(s\)/);
      expect(KEYS.many).not.toMatch(/\(s\)/);
      expect(KEYS.one).toContain("session.");
      expect(KEYS.many).toContain("sessions.");
      expect(KEYS.one).not.toBe(KEYS.many);
    });

    it("são chaves do catálogo em inglês, que devolve a própria frase", () => {
      for (const key of Object.values(KEYS)) {
        // A chave nova ainda pode não estar no arquivo; estando, o valor em
        // inglês é a própria chave — é assim que `tr()` devolve a frase crua.
        if (key in en) expect(en[key]).toBe(key);
      }
      expect(en[KEYS.cancel]).toBe("Cancel");
    });

    it("descrevem a presença por chave de catálogo, não por texto solto", () => {
      for (const type of [0, 1, 2, 3]) {
        expect(en[presenceLabelKey(type)]).toBeDefined();
      }
      expect(presenceLabelKey(2)).toBe("In Game");
      expect(presenceLabelKey(3)).toBe("In Studio");
      expect(presenceLabelKey(1)).toBe("Online");
      expect(presenceLabelKey(0)).toBe("Offline");
    });
  });

  describe("com o app em português", () => {
    beforeEach(() => {
      settingsData = { General: { Language: "pt" } };
      i18n.addResourceBundle("pt", "translation", PT_FIXTURE, true, true);
    });

    afterEach(async () => {
      cleanup();
      await i18n.changeLanguage("en");
    });

    it("traduz a frase de uma conta e o estado dentro dela", async () => {
      const user = userEvent.setup();
      presenceRows = [{ userId: 1, userPresenceType: 2 }];
      const { result } = await renderWarning();

      act(() => {
        void result.current.confirmJoin([1]);
      });

      await waitFor(() => expect(dialogText()).toContain("[pt]"));
      expect(dialogText()).toContain("Main está Em jogo agora");
      expect(dialogText()).not.toContain("In Game");
      expect(dialogText()).not.toContain("{{");

      await user.click(screen.getByText("Cancelar"));
    });

    it("traduz a frase de várias contas, a lista e o resto truncado", async () => {
      const user = userEvent.setup();
      presenceRows = [1, 2, 3, 4, 5].map((id) => ({ userId: id, userPresenceType: 3 }));
      const { result } = await renderWarning();

      act(() => {
        void result.current.confirmJoin([1, 2, 3, 4, 5]);
      });

      await waitFor(() => expect(dialogText()).toContain("[pt]"));
      const text = dialogText();
      expect(text).toContain("[pt] 5 contas já estão online:");
      expect(text).toContain("Main (No Studio)");
      expect(text).toContain("e mais 1");
      expect(text).not.toContain("and 1 more");
      expect(text).not.toContain("In Studio");
      expect(text).not.toContain("{{");

      await user.click(screen.getByText("Cancelar"));
    });

    it("traduz os três rótulos do diálogo", async () => {
      const user = userEvent.setup();
      presenceRows = [{ userId: 1, userPresenceType: 1 }];
      const { result } = await renderWarning();

      act(() => {
        void result.current.confirmJoin([1]);
      });

      await waitFor(() => expect(screen.getByText("[pt] Entrar mesmo assim")).toBeTruthy());
      expect(screen.getByText("Cancelar")).toBeTruthy();
      expect(screen.getByText("[pt] Não mostrar este aviso")).toBeTruthy();
      expect(screen.queryByText("Join Anyway")).toBeNull();
      expect(screen.queryByText("Don't show this warning again")).toBeNull();

      await user.click(screen.getByText("Cancelar"));
    });

    it("traduz o toast do opt-out", async () => {
      const user = userEvent.setup();
      presenceRows = [{ userId: 1, userPresenceType: 1 }];
      const { result } = await renderWarning();

      act(() => {
        void result.current.confirmJoin([1]);
      });

      await waitFor(() => expect(screen.getByRole("checkbox")).toBeTruthy());
      await user.click(screen.getByRole("checkbox"));
      await user.click(screen.getByText("[pt] Entrar mesmo assim"));

      await waitFor(() =>
        expect(toastText(result.current.store.toasts)).toContain("[pt] Aviso desativado")
      );
    });

    it("traduz o nome genérico de um user id desconhecido", async () => {
      const user = userEvent.setup();
      presenceRows = [{ userId: 999, userPresenceType: 1 }];
      const { result } = await renderWarning();

      act(() => {
        void result.current.confirmJoin([999]);
      });

      await waitFor(() => expect(dialogText()).toContain("[pt]"));
      expect(dialogText()).toContain("Usuário 999 está Online agora");
      expect(dialogText()).not.toContain("User 999");

      await user.click(screen.getByText("Cancelar"));
    });
  });
});
