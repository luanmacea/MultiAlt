import type { ReactNode } from "react";
import { Package, User } from "lucide-react";
import { SKIN_COLORS, type AvatarCategory, type FreeCatalogItem, type SavedAvatar } from "../../../avatarBuilder";

/** Resultado de uma conta no lote (`AvatarAccountResult` do backend, em camelCase). */
export interface AvatarAccountResult {
  userId: number;
  avatarId: string;
  status: "ok" | "skipped" | "failed";
  /** "challenge" | "cancelled" | texto livre do backend. */
  reason: string | null;
  claimed: number;
  missing: number;
}

/** Retrato completo do lote, que chega pelo evento `avatar-batch-state`. */
export interface AvatarBatchSnapshot {
  running: boolean;
  total: number;
  done: number;
  currentUserId: number | null;
  accounts: AvatarAccountResult[];
}

export const IDLE_BATCH: AvatarBatchSnapshot = {
  running: false,
  total: 0,
  done: 0,
  currentUserId: null,
  accounts: [],
};

/** Miniaturas por item: `undefined` = carregando, `null` = sem imagem. */
export type ThumbMap = Map<string, string | null>;

/** Rótulos na tela; a chave interna da categoria nunca aparece. */
export const CATEGORY_LABEL: Record<AvatarCategory, string> = {
  hair: "Hair",
  hat: "Hat",
  accessory: "Accessories",
  shirt: "Shirt",
  pants: "Pants",
  tshirt: "T-Shirt",
  body: "Body",
  head: "Head",
};

export const DEFAULT_SKIN = SKIN_COLORS[0].id;

/** Asset e bundle podem ter o mesmo número: a chave da miniatura é o par. */
export function thumbKey(item: Pick<FreeCatalogItem, "kind" | "id">): string {
  return `${item.kind}:${item.id}`;
}

export function skinHex(id: number | null): string {
  return SKIN_COLORS.find((c) => c.id === id)?.hex ?? SKIN_COLORS[0].hex;
}

/** Imagem do item: `undefined` = carregando, `null` = sem imagem. */
export function Thumb({ url, iconSize = 18 }: { url: string | null | undefined; iconSize?: number }) {
  if (url === undefined) return <div className="w-full h-full animate-pulse bg-[var(--panel-soft)]" />;
  if (url === null) {
    return (
      <div className="w-full h-full flex items-center justify-center theme-muted">
        <Package size={iconSize} strokeWidth={1.5} />
      </div>
    );
  }
  return <img src={url} alt="" loading="lazy" draggable={false} className="w-full h-full object-contain" />;
}

/** Grade 2x2 com as quatro primeiras peças de um avatar salvo. */
export function Mosaic({ avatar, thumbs }: { avatar: SavedAvatar; thumbs: ThumbMap }) {
  const cells = avatar.items.slice(0, 4);
  return (
    <div className="grid grid-cols-2 gap-px w-full h-full rounded-lg overflow-hidden bg-[var(--panel-soft)]">
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="bg-[var(--textboxes-bg)] overflow-hidden">
          {cells[i] ? <Thumb url={thumbs.get(thumbKey(cells[i]))} iconSize={12} /> : null}
        </div>
      ))}
    </div>
  );
}

export function Headshot({ url, size = 24 }: { url: string | undefined; size?: number }) {
  return (
    <span
      className="shrink-0 rounded-full overflow-hidden bg-[var(--panel-soft)] flex items-center justify-center theme-muted"
      style={{ width: size, height: size }}
    >
      {url ? (
        <img src={url} alt="" className="w-full h-full object-cover theme-avatar" draggable={false} />
      ) : (
        <User size={Math.round(size * 0.55)} strokeWidth={1.75} />
      )}
    </span>
  );
}

export function SectionTitle({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2 mb-2">
      <div className="text-[11px] font-semibold uppercase tracking-wide theme-muted">{children}</div>
      {aside}
    </div>
  );
}
