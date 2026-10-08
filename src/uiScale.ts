/**
 * Tamanho da interface (Settings › General › Interface size, chave
 * `General.InterfaceScale` do `RAMSettings.ini`).
 *
 * O zoom é o nativo do WebView (`setZoom`), que escala tudo por igual —
 * inclusive as classes em px do Tailwind (`text-[12px]`), que um truque de
 * `rem` no CSS não alcançaria. Ver `hooks/useUiScale.ts` e
 * `docs/features/ui-layout.md#tamanho-da-interface`.
 */

/** Opções gravadas no INI: `auto` ou a porcentagem fixa. */
export const UI_SCALE_SETTINGS = ["auto", "110", "100", "90", "80"] as const;
export type UiScaleSetting = (typeof UI_SCALE_SETTINGS)[number];

/** Janela lógica em que a interface fica em 100% no automático. */
export const UI_SCALE_REFERENCE = { width: 1440, height: 800 } as const;
export const UI_SCALE_AUTO_MIN = 0.8;
export const UI_SCALE_AUTO_MAX = 1;
/** O automático anda em degraus de 5%, sempre arredondando para baixo. */
const AUTO_STEP = 0.05;

export interface LogicalSize {
  width: number;
  height: number;
}

export function normalizeUiScaleSetting(raw: string | null | undefined): UiScaleSetting {
  const value = (raw ?? "").trim().toLowerCase();
  return (UI_SCALE_SETTINGS as readonly string[]).includes(value) ? (value as UiScaleSetting) : "auto";
}

/**
 * Fator automático pelo tamanho **lógico** da janela (pixels físicos divididos
 * pela escala do monitor). Não pode vir de `window.innerWidth`: esse muda com o
 * próprio zoom, e o zoom passaria a alimentar a conta que o decide.
 *
 * `min(w / 1440, h / 800)`, arredondado para baixo em passos de 5% e preso
 * entre 80% e 100%: 2560x1400 → 1.0; 1280x690 → 0.85; 750x450 (mínimo do app) → 0.8.
 */
export function autoUiScale(width: number, height: number): number {
  if (!(width > 0) || !(height > 0)) return 1;
  const raw = Math.min(width / UI_SCALE_REFERENCE.width, height / UI_SCALE_REFERENCE.height);
  // O epsilon segura o degrau exato (0.95 calculado como 0.9499999...).
  const steps = Math.floor(raw / AUTO_STEP + 1e-6);
  const stepped = steps / Math.round(1 / AUTO_STEP);
  return Math.min(UI_SCALE_AUTO_MAX, Math.max(UI_SCALE_AUTO_MIN, stepped));
}

/** Zoom a aplicar. Automático sem tamanho conhecido fica em 100%. */
export function resolveUiScale(setting: UiScaleSetting, size: LogicalSize | null): number {
  if (setting === "auto") return size ? autoUiScale(size.width, size.height) : 1;
  return Number(setting) / 100;
}
