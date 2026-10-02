import { describe, expect, it } from "vitest";
import {
  AVATAR_CATEGORIES,
  MAX_ACCESSORIES,
  REQUIRED_CATEGORIES,
  SKIN_COLORS,
  categoryOf,
  emptySelection,
  groupByCategory,
  newAvatarId,
  randomSkin,
  randomizeSelection,
  selectionFromAvatar,
  selectionItems,
  toggleItem,
  validateAvatarDraft,
  type AvatarCategory,
  type CatalogItemKind,
  type FreeCatalogItem,
  type Selection,
} from "./avatarBuilder";

let nextId = 1;
function item(kind: CatalogItemKind, typeId: number, name = `item${nextId}`): FreeCatalogItem {
  const id = nextId++;
  return { id, kind, typeId, name, collectibleItemId: "" };
}

/** Gerador determinístico (mulberry32) para os testes de sorteio. */
function seeded(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function fullCatalog(): Record<AvatarCategory, FreeCatalogItem[]> {
  const make = (kind: CatalogItemKind, typeId: number, n: number) =>
    Array.from({ length: n }, () => item(kind, typeId));
  return groupByCategory([
    ...make("Asset", 41, 4),
    ...make("Asset", 8, 4),
    ...make("Asset", 42, 2),
    ...make("Asset", 46, 3),
    ...make("Asset", 11, 4),
    ...make("Asset", 12, 4),
    ...make("Asset", 2, 4),
    ...make("Bundle", 1, 4),
    ...make("Bundle", 4, 4),
  ]);
}

describe("categorias do avatar", () => {
  it("mapeia o tipo do Roblox para a categoria e recusa o que está fora da lista", () => {
    expect(categoryOf(item("Asset", 41))).toBe("hair");
    expect(categoryOf(item("Asset", 8))).toBe("hat");
    for (const t of [42, 43, 44, 45, 46, 47]) expect(categoryOf(item("Asset", t))).toBe("accessory");
    expect(categoryOf(item("Asset", 11))).toBe("shirt");
    expect(categoryOf(item("Asset", 12))).toBe("pants");
    expect(categoryOf(item("Asset", 2))).toBe("tshirt");
    expect(categoryOf(item("Bundle", 1))).toBe("body");
    expect(categoryOf(item("Bundle", 4))).toBe("head");
    // roupa em camadas, emote e pacote de animação ficam de fora
    expect(categoryOf(item("Asset", 64))).toBeNull();
    expect(categoryOf(item("Asset", 61))).toBeNull();
    expect(categoryOf(item("Bundle", 2))).toBeNull();
    // o mesmo número em tipo errado não vale: bundle 41 não é cabelo
    expect(categoryOf(item("Bundle", 41))).toBeNull();
    expect(categoryOf(item("Asset", 1))).toBeNull();
  });

  it("agrupa por categoria, com todas as chaves presentes e sem os excluídos", () => {
    const hair = item("Asset", 41);
    const back = item("Asset", 46);
    const grouped = groupByCategory([hair, back, item("Asset", 64)]);
    expect(Object.keys(grouped)).toEqual([...AVATAR_CATEGORIES]);
    expect(grouped.hair).toEqual([hair]);
    expect(grouped.accessory).toEqual([back]);
    expect(grouped.shirt).toEqual([]);
  });

  it("expõe as constantes na ordem combinada", () => {
    expect(AVATAR_CATEGORIES).toEqual(["hair", "hat", "accessory", "shirt", "pants", "tshirt", "body", "head"]);
    expect(REQUIRED_CATEGORIES).toEqual(["shirt", "pants", "body"]);
    expect(MAX_ACCESSORIES).toBe(3);
    expect(SKIN_COLORS.map((c) => c.id)).toEqual([1030, 125, 18, 38, 217, 192, 5, 226]);
    expect(SKIN_COLORS[0]).toEqual({ id: 1030, hex: "#FFCC99" });
  });
});

describe("seleção do avatar", () => {
  it("começa vazia", () => {
    const sel = emptySelection();
    expect(Object.keys(sel)).toEqual([...AVATAR_CATEGORIES]);
    expect(selectionItems(sel)).toEqual([]);
  });

  it("slot único: escolher outro substitui, escolher o mesmo tira", () => {
    const a = item("Asset", 41);
    const b = item("Asset", 41);
    let sel = toggleItem(emptySelection(), a);
    expect(sel.hair).toEqual([a]);
    sel = toggleItem(sel, b);
    expect(sel.hair).toEqual([b]);
    sel = toggleItem(sel, b);
    expect(sel.hair).toEqual([]);
  });

  it("não muta a seleção recebida", () => {
    const base = emptySelection();
    toggleItem(base, item("Asset", 41));
    expect(base.hair).toEqual([]);
  });

  it("acessório liga e desliga até o limite de 3", () => {
    const acc = [item("Asset", 42), item("Asset", 43), item("Asset", 46), item("Asset", 47)];
    let sel = emptySelection();
    for (const a of acc) sel = toggleItem(sel, a);
    expect(sel.accessory).toEqual(acc.slice(0, MAX_ACCESSORIES));
    sel = toggleItem(sel, acc[1]);
    expect(sel.accessory).toEqual([acc[0], acc[2]]);
    sel = toggleItem(sel, acc[3]);
    expect(sel.accessory).toEqual([acc[0], acc[2], acc[3]]);
  });

  it("ignora item de categoria desconhecida", () => {
    const sel = emptySelection();
    expect(toggleItem(sel, item("Asset", 64))).toEqual(sel);
  });

  it("junta os itens na ordem das categorias", () => {
    const hair = item("Asset", 41);
    const shirt = item("Asset", 11);
    const body = item("Bundle", 1);
    let sel = emptySelection();
    for (const i of [body, shirt, hair]) sel = toggleItem(sel, i);
    expect(selectionItems(sel)).toEqual([hair, shirt, body]);
  });

  it("monta a seleção a partir de um avatar salvo, largando item de tipo fora da lista", () => {
    const hair = item("Asset", 41);
    const acc = [item("Asset", 42), item("Asset", 43)];
    const stray = item("Asset", 64);
    const sel = selectionFromAvatar({ id: "av_1", name: "x", items: [hair, ...acc, stray], skinColor: 18 });
    expect(sel.hair).toEqual([hair]);
    expect(sel.accessory).toEqual(acc);
    expect(selectionItems(sel)).not.toContain(stray);
  });
});

describe("sorteio", () => {
  it("é determinístico com a mesma semente", () => {
    const catalog = fullCatalog();
    const a = randomizeSelection(catalog, seeded(7));
    const b = randomizeSelection(catalog, seeded(7));
    expect(a).toEqual(b);
  });

  it("sempre preenche camisa, calça e corpo, e nunca passa de 2 acessórios sorteados", () => {
    const catalog = fullCatalog();
    for (let seed = 1; seed <= 200; seed++) {
      const sel = randomizeSelection(catalog, seeded(seed));
      for (const cat of REQUIRED_CATEGORIES) expect(sel[cat]).toHaveLength(1);
      for (const cat of AVATAR_CATEGORIES) {
        if (cat !== "accessory") expect(sel[cat].length).toBeLessThanOrEqual(1);
        for (const it of sel[cat]) expect(catalog[cat]).toContain(it);
      }
      expect(sel.accessory.length).toBeLessThanOrEqual(2);
      expect(new Set(sel.accessory.map((i) => i.id)).size).toBe(sel.accessory.length);
    }
  });

  it("os opcionais às vezes ficam vazios e às vezes não", () => {
    const catalog = fullCatalog();
    let empty = 0;
    let filled = 0;
    for (let seed = 1; seed <= 200; seed++) {
      const sel = randomizeSelection(catalog, seeded(seed));
      if (sel.hair.length === 0) empty++;
      else filled++;
    }
    expect(empty).toBeGreaterThan(30);
    expect(filled).toBeGreaterThan(60);
  });

  it("categoria sem item no catálogo fica vazia, sem quebrar", () => {
    const catalog = groupByCategory([]);
    const sel = randomizeSelection(catalog, seeded(3));
    expect(selectionItems(sel)).toEqual([]);
  });

  it("`only` sorteia só aquela categoria e mantém o resto da base", () => {
    const catalog = fullCatalog();
    const base: Selection = randomizeSelection(catalog, seeded(11));
    for (let seed = 1; seed <= 50; seed++) {
      const next = randomizeSelection(catalog, seeded(seed), "hair", base);
      for (const cat of AVATAR_CATEGORIES) {
        if (cat !== "hair") expect(next[cat]).toEqual(base[cat]);
      }
      for (const it of next.hair) expect(catalog.hair).toContain(it);
    }
  });

  it("`only` numa categoria obrigatória nunca a deixa vazia", () => {
    const catalog = fullCatalog();
    for (let seed = 1; seed <= 50; seed++) {
      expect(randomizeSelection(catalog, seeded(seed), "shirt", emptySelection()).shirt).toHaveLength(1);
    }
  });

  it("sorteia uma cor de pele da paleta", () => {
    const ids = new Set(SKIN_COLORS.map((c) => c.id));
    for (let seed = 1; seed <= 30; seed++) expect(ids.has(randomSkin(seeded(seed)))).toBe(true);
    expect(randomSkin(() => 0)).toBe(SKIN_COLORS[0].id);
    expect(randomSkin(() => 0.999999)).toBe(SKIN_COLORS[SKIN_COLORS.length - 1].id);
  });
});

describe("validação do rascunho", () => {
  function required(): Selection {
    let sel = emptySelection();
    for (const i of [item("Asset", 11), item("Asset", 12), item("Bundle", 1)]) sel = toggleItem(sel, i);
    return sel;
  }

  it("aceita nome e obrigatórios", () => {
    expect(validateAvatarDraft("Meu avatar", required())).toBeNull();
  });

  it("recusa nome vazio, só espaços ou maior que 60", () => {
    expect(validateAvatarDraft("", required())).toBe("avatars.errors.name");
    expect(validateAvatarDraft("   ", required())).toBe("avatars.errors.name");
    expect(validateAvatarDraft("a".repeat(61), required())).toBe("avatars.errors.name");
    expect(validateAvatarDraft(`  ${"a".repeat(60)}  `, required())).toBeNull();
  });

  it("recusa quando falta camisa, calça ou corpo", () => {
    expect(validateAvatarDraft("x", emptySelection())).toBe("avatars.errors.required");
    const sel = required();
    expect(validateAvatarDraft("x", { ...sel, pants: [] })).toBe("avatars.errors.required");
  });

  it("recusa mais de 12 itens", () => {
    const sel = required();
    const many: Selection = { ...sel, accessory: Array.from({ length: 10 }, () => item("Asset", 42)) };
    expect(validateAvatarDraft("x", many)).toBe("avatars.errors.tooMany");
  });
});

describe("id do avatar", () => {
  it("segue o formato aceito pelo backend e não se repete", () => {
    const ids = new Set<string>();
    for (let i = 0; i < 100; i++) {
      const id = newAvatarId();
      expect(id).toMatch(/^av_[A-Za-z0-9_-]{1,93}$/);
      ids.add(id);
    }
    expect(ids.size).toBe(100);
  });
});
