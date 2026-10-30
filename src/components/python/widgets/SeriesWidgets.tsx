import { WidgetFrame } from "./WidgetFrame";
import { parseSeries, parseTextSeries, type WidgetProps } from "./widgetUtils";

const title = (w: WidgetProps["widget"]) => w.title ?? w.id;

const CHART_W = 320;
const CHART_H = 96;
const PAD = 4;

function formatNumber(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(2);
}

function formatClock(ts: number): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleTimeString([], { hour12: false, hour: "2-digit", minute: "2-digit" });
}

/** `line`: one number per pushed point, drawn as an inline SVG (no chart library). */
export function LineWidget({ widget, series }: WidgetProps) {
  const points = parseSeries(series[widget.id]);
  const label = title(widget);
  if (points.length < 2) {
    return (
      <WidgetFrame title={label} wide testId={`widget-${widget.id}`}>
        <div className="text-[12px] text-zinc-600">
          {points.length === 0 ? "No points yet" : `Waiting for more points (last: ${formatNumber(points[0].value)})`}
        </div>
      </WidgetFrame>
    );
  }
  const minTs = points[0].ts;
  const maxTs = points[points.length - 1].ts;
  const values = points.map((p) => p.value);
  const minV = Math.min(...values);
  const maxV = Math.max(...values);
  const spanTs = maxTs - minTs || 1;
  const spanV = maxV - minV || 1;
  const x = (ts: number) => PAD + ((ts - minTs) / spanTs) * (CHART_W - PAD * 2);
  const y = (v: number) => CHART_H - PAD - ((v - minV) / spanV) * (CHART_H - PAD * 2);
  const path = points.map((p) => `${x(p.ts).toFixed(1)},${y(p.value).toFixed(1)}`).join(" ");
  const last = points[points.length - 1];
  return (
    <WidgetFrame title={label} wide testId={`widget-${widget.id}`}>
      <div className="flex items-start gap-3">
        <svg
          role="img"
          aria-label={`${label}: ${points.length} points, last value ${formatNumber(last.value)}`}
          viewBox={`0 0 ${CHART_W} ${CHART_H}`}
          className="h-[96px] min-w-0 flex-1 text-sky-400"
          preserveAspectRatio="none"
        >
          <line x1={PAD} x2={CHART_W - PAD} y1={CHART_H - PAD} y2={CHART_H - PAD} stroke="currentColor" strokeOpacity="0.15" />
          <polyline points={path} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
          <circle cx={x(last.ts)} cy={y(last.value)} r="2.4" fill="currentColor" />
        </svg>
        <div className="shrink-0 text-right text-[11px] tabular-nums text-zinc-500">
          <div className="text-[18px] font-semibold leading-none text-zinc-100">{formatNumber(last.value)}</div>
          <div className="mt-1.5">max {formatNumber(maxV)}</div>
          <div>min {formatNumber(minV)}</div>
        </div>
      </div>
      <div className="mt-1 flex justify-between text-[11px] text-zinc-600">
        <span>{formatClock(minTs)}</span>
        <span>{formatClock(maxTs)}</span>
      </div>
    </WidgetFrame>
  );
}

const DEFAULT_LOG_LINES = 50;

/** `log`: one string per pushed line; shows the newest `max` lines. */
export function LogWidget({ widget, series }: WidgetProps) {
  const all = parseTextSeries(series[widget.id]);
  const limit = widget.max && widget.max > 0 ? Math.floor(widget.max) : DEFAULT_LOG_LINES;
  const lines = all.slice(-limit);
  return (
    <WidgetFrame title={title(widget)} wide testId={`widget-${widget.id}`}>
      {lines.length === 0 ? (
        <div className="text-[12px] text-zinc-600">Nothing logged yet</div>
      ) : (
        <div className="max-h-[200px] overflow-y-auto rounded-md border theme-border bg-zinc-900/40 px-2.5 py-1.5 font-mono text-[11.5px] leading-[1.55] text-zinc-300">
          {lines.map((line, i) => (
            <div key={i} className="whitespace-pre-wrap break-words">
              {line.ts > 0 ? <span className="mr-2 text-zinc-600">{formatClock(line.ts)}</span> : null}
              {line.text}
            </div>
          ))}
        </div>
      )}
    </WidgetFrame>
  );
}
