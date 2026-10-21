import { ArrowDownRight, ArrowUpRight } from "lucide-react";
import { UnsupportedValue, WidgetFrame } from "./WidgetFrame";
import { asNumber, isRecord, toText, type WidgetProps } from "./widgetUtils";

const title = (w: WidgetProps["widget"]) => w.title ?? w.id;

/** `stat`: a number, or `{ value, delta }` for a change arrow. */
export function StatWidget({ widget, values }: WidgetProps) {
  const raw = values[widget.id];
  let value: string | null = null;
  let delta: number | null = null;
  if (isRecord(raw)) {
    const n = asNumber(raw.value);
    value = n !== null ? String(n) : raw.value !== undefined ? toText(raw.value) : null;
    delta = asNumber(raw.delta);
  } else if (raw !== undefined && raw !== null) {
    value = toText(raw);
  }
  return (
    <WidgetFrame title={title(widget)} testId={`widget-${widget.id}`}>
      {value === null ? (
        <div className="text-[12px] text-zinc-600">No value yet</div>
      ) : (
        <div className="flex items-baseline gap-2">
          <span className="text-[26px] font-semibold leading-none tabular-nums text-zinc-100">{value}</span>
          {delta !== null && delta !== 0 ? (
            <span
              className={`inline-flex items-center gap-0.5 text-[12px] ${delta > 0 ? "text-emerald-400" : "text-red-400"}`}
              aria-label={`Change ${delta > 0 ? "+" : ""}${delta}`}
            >
              {delta > 0 ? <ArrowUpRight size={12} strokeWidth={2.2} /> : <ArrowDownRight size={12} strokeWidth={2.2} />}
              {delta > 0 ? "+" : ""}
              {delta}
            </span>
          ) : null}
        </div>
      )}
    </WidgetFrame>
  );
}

/** `progress`: a number from 0 to `max` (default 100). */
export function ProgressWidget({ widget, values }: WidgetProps) {
  const raw = values[widget.id];
  const value = asNumber(raw);
  const max = widget.max && widget.max > 0 ? widget.max : 100;
  return (
    <WidgetFrame title={title(widget)} testId={`widget-${widget.id}`}>
      {value === null ? (
        raw === undefined ? (
          <div className="text-[12px] text-zinc-600">No value yet</div>
        ) : (
          <UnsupportedValue what={toText(raw)} />
        )
      ) : (
        <>
          <div
            role="progressbar"
            aria-label={title(widget)}
            aria-valuemin={0}
            aria-valuemax={max}
            aria-valuenow={Math.min(max, Math.max(0, value))}
            className="h-2 overflow-hidden rounded-full bg-zinc-800"
          >
            <div className="h-full rounded-full bg-sky-400" style={{ width: `${Math.min(100, Math.max(0, (value / max) * 100))}%` }} />
          </div>
          <div className="mt-1 text-[11.5px] tabular-nums text-zinc-400">
            {value} / {max}
          </div>
        </>
      )}
    </WidgetFrame>
  );
}

/** `text`: plain text, kept as typed (line breaks included). */
export function TextWidget({ widget, values }: WidgetProps) {
  const raw = values[widget.id];
  return (
    <WidgetFrame title={title(widget)} wide testId={`widget-${widget.id}`}>
      {raw === undefined || raw === null || raw === "" ? (
        <div className="text-[12px] text-zinc-600">No value yet</div>
      ) : (
        <div className="whitespace-pre-wrap break-words text-[12.5px] text-zinc-300">{toText(raw)}</div>
      )}
    </WidgetFrame>
  );
}

const BADGE_STYLE: Record<string, string> = {
  ok: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300",
  warn: "border-amber-500/30 bg-amber-500/15 text-amber-300",
  error: "border-red-500/30 bg-red-500/15 text-red-300",
};

/** `badge`: `{ level: "ok" | "warn" | "error", text }`. */
export function BadgeWidget({ widget, values }: WidgetProps) {
  const raw = values[widget.id];
  let level: string | null = null;
  let text = "";
  if (isRecord(raw) && typeof raw.level === "string") {
    level = raw.level;
    text = toText(raw.text);
  }
  return (
    <WidgetFrame title={title(widget)} testId={`widget-${widget.id}`}>
      {raw === undefined || raw === null ? (
        <div className="text-[12px] text-zinc-600">No value yet</div>
      ) : level === null ? (
        <UnsupportedValue what="" />
      ) : (
        <span
          data-level={level}
          className={`inline-block rounded-md border px-2 py-0.5 text-[12px] font-medium ${
            BADGE_STYLE[level] ?? "border-zinc-600 bg-zinc-800 text-zinc-300"
          }`}
        >
          {text || level}
        </span>
      )}
    </WidgetFrame>
  );
}
