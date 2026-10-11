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

/** Monitor lógico a partir do qual a interface fica em 100% no automático (abaixo, encolhe). */
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
 * Fator automático pelo tamanho **lógico** do monitor da janela (pixels
 * físicos divididos pela escala). Não pode vir de `window.innerWidth`: esse
 * muda com o próprio zoom, e o zoom passaria a alimentar a conta que o decide.
 *
 * `min(w / 1440, h / 800)`, arredondado para baixo em passos de 5% e preso
 * entre 80% e 100%: 2560x1440 → 1.0; 1920x1080 a 125% (1536x864) → 1.0;
 * 1366x768 → 0.9; 1920x1080 a 150% (1280x720) → 0.85.
 */
export function autoUiScale(width: number, height: number): number {
  if (!(width > 0) || !(height > 0)) return 1;
  const raw = Math.min(width / UI_SCALE_REFERENCE.width, height / UI_SCALE_REFERENCE.height);
  // O epsilon segura o degrau exato (0.95 calculado como 0.9499999...).
  const steps = Math.floor(raw / AUTO_STEP + 1e-6);
  const stepped = steps / Math.round(1 / AUTO_STEP);
  return Math.min(UI_SCALE_AUTO_MAX, Math.max(UI_SCALE_AUTO_MIN, stepped));
}

/**
 * Janela lógica a partir da qual o automático **cresce** a interface: o dobro
 * do desenho do app (1100x620 de área útil), para que a tela crescida ainda
 * tenha o espaço de uma janela padrão e não pareça uma lupa.
 */
export const UI_SCALE_GROW_REFERENCE = { width: 2200, height: 1240 } as const;
export const UI_SCALE_GROW_MAX = 2;

/**
 * Fator de crescimento pelo tamanho **lógico da janela** (área interna em
 * pixels físicos dividida pela escala do monitor — também não muda com o
 * zoom). `min(w / 2200, h / 1240)`, arredondado para o degrau de 5% mais
 * próximo e preso entre 100% e 200%.
 *
 * Só cresce: janela pequena (o padrão 1100x700, ou restaurada) fica em 100% e
 * quem decide se encolhe é o monitor (`autoUiScale`). Exemplos: 4K a 100%
 * maximizada (~3840x2090) → 170%; 2560x1440 maximizada (~2560x1390) → 110%;
 * 1920x1080 maximizada → 100%.
 */
export function growUiScale(width: number, height: number): number {
  if (!(width > 0) || !(height > 0)) return 1;
  const raw = Math.min(width / UI_SCALE_GROW_REFERENCE.width, height / UI_SCALE_GROW_REFERENCE.height);
  const steps = Math.round(raw / AUTO_STEP + 1e-6);
  const stepped = steps / Math.round(1 / AUTO_STEP);
  return Math.min(UI_SCALE_GROW_MAX, Math.max(1, stepped));
}

/**
 * Zoom a aplicar. Valor fixo ignora monitor e janela. Automático: janela
 * grande cresce (`growUiScale`); senão o monitor pequeno encolhe
 * (`autoUiScale`). Os dois nunca brigam — janela acima de 2200x1240 só existe
 * em monitor muito acima de 1440x800. Sem tamanho conhecido, 100%.
 */
export function resolveUiScale(
  setting: UiScaleSetting,
  monitor: LogicalSize | null,
  windowSize: LogicalSize | null = null
): number {
  if (setting !== "auto") return Number(setting) / 100;
  const grow = windowSize ? growUiScale(windowSize.width, windowSize.height) : 1;
  if (grow > 1) return grow;
  return monitor ? autoUiScale(monitor.width, monitor.height) : 1;
}
