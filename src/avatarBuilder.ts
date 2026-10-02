/**
 * Lógica pura do construtor de avatares gratuitos: categorias, seleção, sorteio e
 * validação do rascunho. Sem React e sem IPC — a tela só liga isto aos comandos.
 *
 * Os tipos espelham o que o backend serializa em camelCase (item do catálogo e
 * avatar salvo). O backend revalida tudo ao salvar (nome 1–60, 1–12 itens, sem id
 * repetido, id do avatar `[A-Za-z0-9_-]{1,96}`); aqui só se barra antes o que ele
 * recusaria.
 */

export type CatalogItemKind = "Asset" | "Bundle";

export interface FreeCatalogItem {
  id: number;
  kind: CatalogItemKind;
  typeId: number;
  name: string;
  collectibleItemId: string;
}

export interface SavedAvatar {
  id: string;
  name: string;
  items: FreeCatalogItem[];
  skinColor: number | null;
}

export type AvatarCategory = "hair" | "hat" | "accessory" | "shirt" | "pants" | "tshirt" | "body" | "head";

export const AVATAR_CATEGORIES: readonly AvatarCategory[] = [
  "hair",
  "hat",
  "accessory",
  "shirt",
  "pants",
  "tshirt",
  "body",
  "head",
];

/** Sem camisa, calça e corpo o avatar fica incompleto: o backend e a UI exigem os três. */
export const REQUIRED_CATEGORIES: readonly AvatarCategory[] = ["shirt", "pants", "body"];

export const MAX_ACCESSORIES = 3;

/** Teto de itens de um avatar salvo (o backend recusa mais que isso). */
const MAX_ITEMS = 12;
const MAX_NAME_LENGTH = 60;
/** Chance de uma categoria opcional ficar vazia no sorteio. */
const OPTIONAL_EMPTY_CHANCE = 0.4;
/** Acessórios sorteados: 0 a 2 (o 3º slot fica livre para o usuário). */
const MAX_RANDOM_ACCESSORIES = 2;

/** Paleta de pele (id BrickColor → hex). */
export const SKIN_COLORS: readonly { id: number; hex: string }[] = [
  { id: 1030, hex: "#FFCC99" },
  { id: 125, hex: "#EAB892" },
  { id: 18, hex: "#CC8E69" },
  { id: 38, hex: "#A05F35" },
  { id: 217, hex: "#7C5C46" },
  { id: 192, hex: "#694028" },
  { id: 5, hex: "#D7C59A" },
  { id: 226, hex: "#FDEA8D" },
];

/** Tipos de asset aceitos (42–47 são todos os acessórios) e de bundle (1 corpo, 4 cabeça). */
const ASSET_CATEGORY: Readonly<Record<number, AvatarCategory>> = {
  41: "hair",
  8: "hat",
  42: "accessory",
  43: "accessory",
  44: "accessory",
  45: "accessory",
  46: "accessory",
  47: "accessory",
  11: "shirt",
  12: "pants",
  2: "tshirt",
};
const BUNDLE_CATEGORY: Readonly<Record<number, AvatarCategory>> = {
  1: "body",
  4: "head",
};

/** Categoria do item, ou `null` se o tipo não está na lista permitida (roupa em camadas, emote...). */
export function categoryOf(item: FreeCatalogItem): AvatarCategory | null {
  const table = item.kind === "Bundle" ? BUNDLE_CATEGORY : ASSET_CATEGORY;
  return table[item.typeId] ?? null;
}

function emptyRecord(): Record<AvatarCategory, FreeCatalogItem[]> {
  return {
    hair: [],
    hat: [],
    accessory: [],
    shirt: [],
    pants: [],
    tshirt: [],
    body: [],
    head: [],
  };
}

/** Separa o catálogo por categoria (todas as chaves existem); item fora da lista some. */
export function groupByCategory(items: FreeCatalogItem[]): Record<AvatarCategory, FreeCatalogItem[]> {
  const grouped = emptyRecord();
  for (const item of items) {
    const category = categoryOf(item);
    if (category) grouped[category].push(item);
  }
  return grouped;
}

/** Acessório vai até `MAX_ACCESSORIES`; as demais categorias têm 0 ou 1 item. */
export type Selection = Record<AvatarCategory, FreeCatalogItem[]>;

export function emptySelection(): Selection {
  return emptyRecord();
}

/** Asset e bundle podem ter o mesmo número, então a identidade é o par. */
function sameItem(a: FreeCatalogItem, b: FreeCatalogItem): boolean {
  return a.id === b.id && a.kind === b.kind;
}

/**
 * Liga/desliga um item. Slot único: outro item substitui, o mesmo tira.
 * Acessório: liga até `MAX_ACCESSORIES` (cheio, o clique é ignorado) e desliga.
 */
export function toggleItem(sel: Selection, item: FreeCatalogItem): Selection {
  const category = categoryOf(item);
  if (!category) return sel;
  const current = sel[category];
  const present = current.some((c) => sameItem(c, item));
  if (category === "accessory") {
    if (present) return { ...sel, accessory: current.filter((c) => !sameItem(c, item)) };
    if (current.length >= MAX_ACCESSORIES) return sel;
    return { ...sel, accessory: [...current, item] };
  }
  return { ...sel, [category]: present ? [] : [item] };
}

function pick<T>(list: readonly T[], rand: () => number): T {
  return list[Math.min(list.length - 1, Math.floor(rand() * list.length))];
}

/** Sorteia uma categoria. Sempre consome `rand` da mesma forma para a mesma entrada. */
function rollCategory(
  category: AvatarCategory,
  pool: readonly FreeCatalogItem[],
  rand: () => number,
): FreeCatalogItem[] {
  if (pool.length === 0) return [];
  if (category === "accessory") {
    const wanted = Math.min(Math.floor(rand() * (MAX_RANDOM_ACCESSORIES + 1)), pool.length);
    const left = [...pool];
    const picked: FreeCatalogItem[] = [];
    for (let i = 0; i < wanted; i++) picked.push(left.splice(Math.floor(rand() * left.length), 1)[0]);
    return picked;
  }
  if (!REQUIRED_CATEGORIES.includes(category) && rand() < OPTIONAL_EMPTY_CHANCE) return [];
  return [pick(pool, rand)];
}

/**
 * Sorteia um avatar a partir do catálogo. Obrigatórias (camisa, calça, corpo) saem
 * sempre preenchidas quando o catálogo tem item; opcionais ficam vazias ~40% das
 * vezes; acessório leva 0 a 2 itens distintos. Com `only`, só essa categoria é
 * sorteada de novo e o resto vem de `base`. Determinístico dado `rand`.
 */
export function randomizeSelection(
  catalog: Record<AvatarCategory, FreeCatalogItem[]>,
  rand: () => number,
  only?: AvatarCategory,
  base?: Selection,
): Selection {
  const next: Selection = base ? { ...base } : emptySelection();
  for (const category of AVATAR_CATEGORIES) {
    if (only && category !== only) continue;
    next[category] = rollCategory(category, catalog[category] ?? [], rand);
  }
  return next;
}

export function randomSkin(rand: () => number): number {
  return pick(SKIN_COLORS, rand).id;
}

/** Itens da seleção, na ordem das categorias (a ordem em que o avatar é salvo). */
export function selectionItems(sel: Selection): FreeCatalogItem[] {
  return AVATAR_CATEGORIES.flatMap((category) => sel[category]);
}

/** Reconstrói a seleção de um avatar salvo; item de tipo fora da lista é descartado. */
export function selectionFromAvatar(avatar: SavedAvatar): Selection {
  const sel = emptySelection();
  for (const item of avatar.items) {
    const category = categoryOf(item);
    if (!category) continue;
    if (category === "accessory") {
      if (sel.accessory.length < MAX_ACCESSORIES && !sel.accessory.some((c) => sameItem(c, item))) {
        sel.accessory.push(item);
      }
    } else if (sel[category].length === 0) {
      sel[category].push(item);
    }
  }
  return sel;
}

/** Chave i18n do primeiro problema que o backend recusaria ao salvar, ou `null` se está ok. */
export function validateAvatarDraft(name: string, sel: Selection): string | null {
  const trimmed = name.trim();
  if (trimmed.length < 1 || trimmed.length > MAX_NAME_LENGTH) return "avatars.errors.name";
  if (REQUIRED_CATEGORIES.some((category) => sel[category].length === 0)) return "avatars.errors.required";
  if (selectionItems(sel).length > MAX_ITEMS) return "avatars.errors.tooMany";
  return null;
}

/** Id de avatar salvo: `av_` + tempo em base36 + sufixo aleatório (só `[a-z0-9_]`, cabe no limite do backend). */
export function newAvatarId(): string {
  const time = Date.now().toString(36);
  const random = Math.floor(Math.random() * 36 ** 6)
    .toString(36)
    .padStart(6, "0");
  return `av_${time}${random}`;
}
