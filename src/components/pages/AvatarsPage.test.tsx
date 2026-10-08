import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { AvatarsPage } from "./AvatarsPage";
import { ENABLE_AVATAR_BATCH } from "../../featureFlags";
import i18n, { DEFAULT_LANGUAGE } from "../../i18n/index";
import type { StoreValue } from "../../store";
import type { FreeCatalogItem, SavedAvatar } from "../../avatarBuilder";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeMap, type InvokeArgs } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { walkTour } from "../../test-utils/tourHelpers";

const ACCOUNTS = [
  makeAccount({ UserID: 11, Username: "alpha" }),
  makeAccount({ UserID: 22, Username: "bravo" }),
];

function asset(id: number, typeId: number, name: string): FreeCatalogItem {
  return { id, kind: "Asset", typeId, name, collectibleItemId: `c-${id}` };
}

function bundle(id: number, typeId: number, name: string): FreeCatalogItem {
  return { id, kind: "Bundle", typeId, name, collectibleItemId: `b-${id}` };
}

/** Uma peça por categoria obrigatória: o sorteio não tem escolha a fazer nelas. */
const SHIRT = asset(101, 11, "Blue Denim Shirt");
const PANTS = asset(102, 12, "Black Jeans");
const BODY = bundle(201, 1, "Rthro Boy");
const CATALOG: FreeCatalogItem[] = [
  asset(1, 41, "Brown Charmer Hair"),
  asset(2, 8, "Roblox Baseball Cap"),
  asset(3, 46, "Wings of Duty"),
  SHIRT,
  PANTS,
  asset(103, 2, "ROBLOX Jacket"),
  BODY,
  bundle(202, 4, "Classic Head"),
];

const SAVED: SavedAvatar = { id: "av_1", name: "Ninja", items: [SHIRT, PANTS, BODY], skinColor: 1030 };

const IDLE = { running: false, total: 0, done: 0, currentUserId: null, accounts: [] };

function wire(overrides: Record<string, unknown | ((args: InvokeArgs) => unknown)> = {}) {
  setInvokeMap({
    avatar_free_catalog: CATALOG,
    avatar_list_saved: [],
    get_avatar_batch_state: IDLE,
    batch_thumbnails: [],
    avatar_save: (args: InvokeArgs) => (args as { avatar: SavedAvatar }).avatar,
    avatar_delete: true,
    avatar_apply_batch: IDLE,
    ...overrides,
  });
}

function renderDialog(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ accounts: ACCOUNTS, ...overrides });
  render(<AvatarsPage active onLeave={() => {}} />);
  return store;
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});

afterEach(cleanup);

describe("AvatarsPage — montar", () => {
  /** Visto no harness (cenário tour, 03/10/2026): um estado de lote sem a lista
   * de contas derrubava a tela inteira com "reading length". */
  it("não quebra quando o estado do lote chega sem a lista de contas", async () => {
    wire({ get_avatar_batch_state: { running: false } });
    renderDialog();
    expect(await screen.findByRole("navigation", { name: "Categories" })).toBeInTheDocument();
  });

  it("mostra as categorias do catálogo com a contagem de cada uma", async () => {
    wire();
    renderDialog();
    const rail = await screen.findByRole("navigation", { name: "Categories" });
    for (const label of ["Hair", "Hat", "Accessories", "Shirt", "Pants", "T-Shirt", "Body", "Head"]) {
      expect(within(rail).getByRole("button", { name: new RegExp(`^${label}\\b`) })).toBeInTheDocument();
    }
    // A categoria aberta mostra os itens dela, com o nome.
    expect(await screen.findByRole("button", { name: "Brown Charmer Hair" })).toBeInTheDocument();
  });

  it("mostra o erro do catálogo com um botão de tentar de novo", async () => {
    let fail = true;
    wire({
      avatar_free_catalog: () => {
        if (fail) throw "network down";
        return CATALOG;
      },
    });
    renderDialog();
    const retry = await screen.findByRole("button", { name: "Try again" });
    fail = false;
    await userEvent.click(retry);
    expect(await screen.findByRole("navigation", { name: "Categories" })).toBeInTheDocument();
  });

  it("Randomize all preenche camisa, calça e corpo", async () => {
    wire();
    renderDialog();
    await screen.findByRole("navigation", { name: "Categories" });
    await userEvent.click(screen.getByRole("button", { name: "Randomize all" }));
    const preview = screen.getByRole("region", { name: "Preview" });
    expect(within(preview).getByText(SHIRT.name)).toBeInTheDocument();
    expect(within(preview).getByText(PANTS.name)).toBeInTheDocument();
    expect(within(preview).getByText(BODY.name)).toBeInTheDocument();
  });

  it("Save manda a seleção ao avatar_save e avisa", async () => {
    wire();
    const store = renderDialog();
    await screen.findByRole("navigation", { name: "Categories" });
    await userEvent.click(screen.getByRole("button", { name: "Randomize all" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Avatar name" }), "  Ninja ");
    await userEvent.click(screen.getByRole("button", { name: "Save avatar" }));

    const call = invokeMock.mock.calls.find(([cmd]) => cmd === "avatar_save");
    expect(call).toBeDefined();
    const avatar = (call![1] as { avatar: SavedAvatar }).avatar;
    expect(avatar.name).toBe("Ninja");
    expect(avatar.id).toMatch(/^av_/);
    expect(avatar.items).toEqual(expect.arrayContaining([SHIRT, PANTS, BODY]));
    expect(typeof avatar.skinColor).toBe("number");
    expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("Ninja"));
    // O salvo aparece na lista para carregar depois.
    expect(await screen.findByRole("button", { name: "Load Ninja" })).toBeInTheDocument();
  });

  it("Enter repetido no nome não salva duas vezes", async () => {
    // O avatar_save fica pendurado: o segundo Enter chega com o primeiro em andamento.
    wire({ avatar_save: () => new Promise(() => {}) });
    renderDialog();
    await screen.findByRole("navigation", { name: "Categories" });
    await userEvent.click(screen.getByRole("button", { name: "Randomize all" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Avatar name" }), "Ninja{Enter}{Enter}");
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "avatar_save")).toHaveLength(1);
  });

  it("não salva sem as peças obrigatórias e diz o que falta", async () => {
    wire();
    renderDialog();
    await screen.findByRole("navigation", { name: "Categories" });
    await userEvent.type(screen.getByRole("textbox", { name: "Avatar name" }), "Incomplete");
    await userEvent.click(screen.getByRole("button", { name: "Save avatar" }));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "avatar_save")).toBe(false);
    expect(screen.getByText(/shirt, pants and a body/i)).toBeInTheDocument();
  });

  it("apaga um avatar salvo só depois de confirmar", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog();
    const trash = await screen.findByRole("button", { name: "Delete Ninja" });
    promptAnswers.confirm = true;
    await userEvent.click(trash);
    expect(confirmMock).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("avatar_delete", { id: "av_1" });
    expect(screen.queryByRole("button", { name: "Load Ninja" })).not.toBeInTheDocument();
  });
});

describe("AvatarsPage — distribuir", () => {
  it("na edição completa a aba mostra o lote, sem o cartão da edição", async () => {
    expect(ENABLE_AVATAR_BATCH).toBe(true);
    wire({ avatar_list_saved: [SAVED] });
    renderDialog();
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    expect(await screen.findByText("Avatars to hand out")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply avatars" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Get the complete edition" })).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "get_avatar_batch_state")).toBe(true);
  });

  it("Apply avatars fica desligado sem conta marcada", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog({ selectedAccounts: [] });
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await screen.findByRole("checkbox", { name: "Ninja" });
    expect(screen.getByRole("button", { name: "Apply avatars" })).toBeDisabled();
    expect(screen.getByText(/check the accounts that should get an avatar/i)).toBeInTheDocument();
    // As contas aparecem no próprio diálogo, desmarcadas.
    expect(screen.getByRole("checkbox", { name: "alpha" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("checkbox", { name: "bravo" })).toHaveAttribute("aria-checked", "false");
  });

  it("marca as contas no próprio diálogo, a partir da seleção da lista principal", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog({ selectedAccounts: [ACCOUNTS[1]], selectedIds: new Set([22]) });
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await screen.findByRole("checkbox", { name: "Ninja" });
    expect(screen.getByRole("checkbox", { name: "bravo" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("checkbox", { name: "alpha" })).toHaveAttribute("aria-checked", "false");

    await userEvent.click(screen.getByRole("checkbox", { name: "alpha" }));
    await userEvent.click(screen.getByRole("button", { name: "Apply avatars" }));
    expect(invokeMock).toHaveBeenCalledWith("avatar_apply_batch", { userIds: [11, 22], avatarIds: ["av_1"] });
  });

  it("Select all e Select none marcam e desmarcam todas as contas", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog({ selectedAccounts: [] });
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await screen.findByRole("checkbox", { name: "Ninja" });

    await userEvent.click(screen.getByRole("button", { name: "Select all" }));
    expect(screen.getByRole("checkbox", { name: "alpha" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("checkbox", { name: "bravo" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("button", { name: "Apply avatars" })).toBeEnabled();

    await userEvent.click(screen.getByRole("button", { name: "Select none" }));
    expect(screen.getByRole("checkbox", { name: "alpha" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("button", { name: "Apply avatars" })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Select all" }));
    await userEvent.click(screen.getByRole("button", { name: "Apply avatars" }));
    expect(invokeMock).toHaveBeenCalledWith("avatar_apply_batch", { userIds: [11, 22], avatarIds: ["av_1"] });
  });

  it("Apply avatars manda as contas selecionadas e os avatares marcados", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog({ selectedAccounts: ACCOUNTS, selectedIds: new Set([11, 22]) });
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await screen.findByRole("checkbox", { name: "Ninja" });
    await userEvent.click(screen.getByRole("button", { name: "Apply avatars" }));
    expect(invokeMock).toHaveBeenCalledWith("avatar_apply_batch", { userIds: [11, 22], avatarIds: ["av_1"] });
  });

  it("mostra 'verificação exigida' na conta pulada por desafio e atualiza as fotos no fim", async () => {
    wire({ avatar_list_saved: [SAVED] });
    const store = renderDialog({ selectedAccounts: ACCOUNTS });
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await screen.findByRole("checkbox", { name: "Ninja" });

    act(() => {
      emitTauriEvent("avatar-batch-state", { running: true, total: 2, done: 0, currentUserId: 11, accounts: [] });
    });
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "0");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    act(() => {
      emitTauriEvent("avatar-batch-state", {
        running: false,
        total: 2,
        done: 2,
        currentUserId: null,
        accounts: [
          { userId: 11, avatarId: "av_1", status: "ok", reason: null, claimed: 3, missing: 0 },
          { userId: 22, avatarId: "av_1", status: "skipped", reason: "challenge", claimed: 0, missing: 0 },
        ],
      });
    });
    expect(screen.getByText(/verification required/i)).toBeInTheDocument();
    expect(screen.getByText("3 claimed")).toBeInTheDocument();
    expect(store.refreshAvatarHeadshots).toHaveBeenCalledWith([11, 22]);
  });

  /**
   * Virou página, mas continua montada com outra página aberta: o lote pode
   * acabar com o usuário na lista de contas, e as fotos ainda têm que ser
   * atualizadas.
   */
  it("atualiza as fotos no fim do lote mesmo com outra página aberta", () => {
    wire({ avatar_list_saved: [SAVED] });
    const store = setStore({ accounts: ACCOUNTS });
    render(<AvatarsPage active={false} onLeave={() => {}} />);
    expect(screen.queryByRole("heading", { name: "Avatars" })).not.toBeInTheDocument();

    act(() => {
      emitTauriEvent("avatar-batch-state", { running: true, total: 1, done: 0, currentUserId: 11, accounts: [] });
    });
    act(() => {
      emitTauriEvent("avatar-batch-state", {
        running: false,
        total: 1,
        done: 1,
        currentUserId: null,
        accounts: [{ userId: 11, avatarId: "av_1", status: "ok", reason: null, claimed: 1, missing: 0 }],
      });
    });
    expect(store.refreshAvatarHeadshots).toHaveBeenCalledWith([11]);
  });

  it("retoma um lote que já estava rodando ao abrir", async () => {
    wire({
      avatar_list_saved: [SAVED],
      get_avatar_batch_state: { running: true, total: 3, done: 1, currentUserId: 22, accounts: [
        { userId: 11, avatarId: "av_1", status: "ok", reason: null, claimed: 0, missing: 0 },
      ] },
    });
    renderDialog({ selectedAccounts: ACCOUNTS });
    // Com lote rodando a tela já abre em Distribute.
    expect(await screen.findByRole("progressbar")).toHaveAttribute("aria-valuenow", "1");
    expect(screen.getByRole("tab", { name: "Distribute" })).toHaveAttribute("aria-selected", "true");
  });

  it("um início recusado não repete o aviso de fim do lote anterior", async () => {
    // O lote anterior acabou; o backend recusa o novo e a tela volta ao estado real
    // (o retrato do lote antigo). Isso não é um lote terminando agora.
    const FINISHED = {
      running: false,
      total: 1,
      done: 1,
      currentUserId: null,
      accounts: [{ userId: 11, avatarId: "av_1", status: "ok", reason: null, claimed: 0, missing: 0 }],
    };
    wire({
      avatar_list_saved: [SAVED],
      get_avatar_batch_state: FINISHED,
      avatar_apply_batch: () => {
        throw "An avatar batch is already running";
      },
    });
    const store = renderDialog({ selectedAccounts: ACCOUNTS });
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await userEvent.click(await screen.findByRole("button", { name: "Clear results" }));
    await userEvent.click(screen.getByRole("button", { name: "Apply avatars" }));

    // A recuperação traz o resumo antigo de volta à tela.
    expect(await screen.findByRole("button", { name: "Clear results" })).toBeInTheDocument();
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === "get_avatar_batch_state")).toHaveLength(2)
    );
    expect(store.addToast).toHaveBeenCalledWith("An avatar batch is already running", "error");
    expect(store.addToast).not.toHaveBeenCalledWith(expect.stringMatching(/Avatar batch finished/));
    expect(store.refreshAvatarHeadshots).not.toHaveBeenCalled();
  });

  it("a recusa do backend chega traduzida", async () => {
    wire({
      avatar_list_saved: [SAVED],
      avatar_apply_batch: () => {
        throw "No saved avatar selected";
      },
    });
    await i18n.changeLanguage("pt");
    try {
      const store = renderDialog({ selectedAccounts: ACCOUNTS });
      await userEvent.click(screen.getByRole("tab", { name: "Distribuir" }));
      await screen.findByRole("checkbox", { name: "Ninja" });
      await userEvent.click(screen.getByRole("button", { name: "Aplicar avatares" }));
      await waitFor(() =>
        expect(store.addToast).toHaveBeenCalledWith("Nenhum avatar salvo selecionado", "error")
      );
    } finally {
      await i18n.changeLanguage(DEFAULT_LANGUAGE);
    }
  });
});

/**
 * Com "Names hidden" na toolbar, a lista de contas do Distribute mostrava o
 * alias, o username e a foto de cada conta; o progresso do lote, o nome da conta
 * da vez.
 */
describe("AvatarsPage — nomes ocultos", () => {
  const SECRET = [
    makeAccount({ UserID: 11, Username: "secretalpha", Alias: "AliasAlpha" }),
    makeAccount({ UserID: 22, Username: "secretbravo" }),
  ];
  const HIDDEN = {
    accounts: SECRET,
    selectedAccounts: SECRET,
    hideUsernames: true,
    hiddenNameLetters: 0,
    showAvatarsWhenHidden: false,
    avatarUrls: new Map([
      [11, "https://avatar.test/11.png"],
      [22, "https://avatar.test/22.png"],
    ]),
  };

  function expectNoRealName() {
    const html = document.body.innerHTML;
    for (const leak of ["secretalpha", "AliasAlpha", "secretbravo", "avatar.test"]) {
      expect(html).not.toContain(leak);
    }
  }

  it("a lista de contas e o lote em andamento não mostram nome nem foto", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog(HIDDEN);
    await userEvent.click(screen.getByRole("tab", { name: "Distribute" }));
    await screen.findByRole("checkbox", { name: "Ninja" });
    expect(screen.getAllByRole("checkbox", { name: "************" })).toHaveLength(2);
    expectNoRealName();

    act(() => {
      emitTauriEvent("avatar-batch-state", { running: true, total: 2, done: 1, currentUserId: 22, accounts: [
        { userId: 11, avatarId: "av_1", status: "ok", reason: null, claimed: 1, missing: 0 },
      ] });
    });
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    expectNoRealName();
  });
});

/** Tutorial da página: Build e Distribute, sem salvar nem aplicar nada. */
describe("AvatarsPage — tutorial", () => {
  it("walks every step of the Avatars tutorial with its part on screen", async () => {
    wire({ avatar_list_saved: [SAVED] });
    renderDialog();
    await screen.findByRole("navigation", { name: "Categories" });
    await walkTour("avatars", { invoke: invokeMock });
    expect(screen.getByRole("tab", { name: /Distribute/ })).toHaveAttribute("aria-selected", "true");
  });
});

