import { useState } from "react";
import {
  AVATAR_CATEGORIES,
  emptySelection,
  randomSkin,
  randomizeSelection,
  selectionFromAvatar,
  toggleItem,
  type AvatarCategory,
  type FreeCatalogItem,
  type SavedAvatar,
  type Selection,
} from "../../../avatarBuilder";
import { DEFAULT_SKIN } from "./shared";

export type CatalogByCategory = Record<AvatarCategory, FreeCatalogItem[]>;

/**
 * Rascunho do avatar no montador: seleção, pele, nome, qual salvo está sendo
 * editado e o erro de validação. Mora no diálogo (e não na aba) para não se
 * perder ao trocar de aba ou fechar e reabrir.
 */
export function useAvatarDraft() {
  const [activeCategory, setActiveCategory] = useState<AvatarCategory>(AVATAR_CATEGORIES[0]);
  const [selection, setSelection] = useState<Selection>(() => emptySelection());
  const [skinColor, setSkinColor] = useState<number>(DEFAULT_SKIN);
  const [name, setNameState] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  /** Chave i18n do problema que impediu o último Save. */
  const [error, setError] = useState<string | null>(null);

  return {
    activeCategory,
    setActiveCategory,
    selection,
    skinColor,
    setSkinColor,
    name,
    editingId,
    setEditingId,
    error,
    setError,
    setName(value: string) {
      setNameState(value);
      setError(null);
    },
    toggle(item: FreeCatalogItem) {
      setSelection((prev) => toggleItem(prev, item));
      setError(null);
    },
    randomizeAll(catalog: CatalogByCategory) {
      setSelection(randomizeSelection(catalog, Math.random));
      setSkinColor(randomSkin(Math.random));
      setError(null);
    },
    reroll(catalog: CatalogByCategory, category: AvatarCategory) {
      setSelection((prev) => randomizeSelection(catalog, Math.random, category, prev));
      setError(null);
    },
    startOver() {
      setSelection(emptySelection());
      setSkinColor(DEFAULT_SKIN);
      setNameState("");
      setEditingId(null);
      setError(null);
    },
    load(avatar: SavedAvatar) {
      setSelection(selectionFromAvatar(avatar));
      setSkinColor(avatar.skinColor ?? DEFAULT_SKIN);
      setNameState(avatar.name);
      setEditingId(avatar.id);
      setError(null);
    },
  };
}

export type AvatarDraft = ReturnType<typeof useAvatarDraft>;
