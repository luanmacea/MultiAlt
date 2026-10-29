import type { DashboardWidget } from "../pythonApi";

/** What every widget gets to draw: its layout entry plus the section's data. */
export interface WidgetProps {
  widget: DashboardWidget;
  /** Keys are `"id"` or `"id@userId"`. */
  values: Record<string, unknown>;
  series: Record<string, unknown>;
  /** Name of an account for per-account rows (honors the hidden-names mode). */
  accountName: (userId: number) => string;
}

/** Same cap the backend applies to widget text. */
const MAX_TEXT_CHARS = 2048;

/**
 * Script data is never trusted as markup: everything is turned into a plain
 * string here and rendered as a React text node, so it can not inject HTML.
 */
export function toText(value: unknown): string {
  let text: string;
  if (value === null || value === undefined) text = "";
  else if (typeof value === "string") text = value;
  else if (typeof value === "number" || typeof value === "boolean") text = String(value);
  else {
    try {
      text = JSON.stringify(value) ?? "";
    } catch {
      text = String(value);
    }
  }
  return text.length > MAX_TEXT_CHARS ? `${text.slice(0, MAX_TEXT_CHARS)}...` : text;
}

export function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Widget layout entry from raw script data, or `null` when it has no usable id. */
export function parseWidget(raw: unknown): DashboardWidget | null {
  if (!isRecord(raw)) return null;
  const id = typeof raw.id === "string" ? raw.id : null;
  if (!id) return null;
  const columns = Array.isArray(raw.columns) ? raw.columns.map(toText) : undefined;
  return {
    id,
    type: typeof raw.type === "string" ? raw.type : "",
    title: typeof raw.title === "string" ? raw.title : undefined,
    columns,
    max: asNumber(raw.max) ?? undefined,
  };
}

export interface SeriesPoint {
  ts: number;
  value: number;
}

export function parseSeries(raw: unknown): SeriesPoint[] {
  if (!Array.isArray(raw)) return [];
  const points: SeriesPoint[] = [];
  for (const item of raw) {
    if (!Array.isArray(item) || item.length < 2) continue;
    const ts = asNumber(item[0]);
    const value = asNumber(item[1]);
    if (ts !== null && value !== null) points.push({ ts, value });
  }
  return points;
}

/** Lines of a `log` widget: `[[ts, "text"], ...]`, oldest first. */
export function parseTextSeries(raw: unknown): { ts: number; text: string }[] {
  if (!Array.isArray(raw)) return [];
  const lines: { ts: number; text: string }[] = [];
  for (const item of raw) {
    if (!Array.isArray(item) || item.length < 2) continue;
    lines.push({ ts: asNumber(item[0]) ?? 0, text: toText(item[1]) });
  }
  return lines;
}

/** `"id@123"` entries of one widget: user id -> value. */
export function perAccountValues(values: Record<string, unknown>, id: string): [number, unknown][] {
  const prefix = `${id}@`;
  const rows: [number, unknown][] = [];
  for (const [key, value] of Object.entries(values)) {
    if (!key.startsWith(prefix)) continue;
    const userId = Number(key.slice(prefix.length));
    if (Number.isInteger(userId)) rows.push([userId, value]);
  }
  return rows.sort((a, b) => a[0] - b[0]);
}

export const MAX_TABLE_ROWS = 200;
