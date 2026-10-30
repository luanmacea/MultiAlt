import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { Camera, Loader2, RefreshCw } from "lucide-react";
import { useStore } from "../../store";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { usePrompt } from "../../hooks/usePrompt";
import { Select } from "../ui/Select";
import { copyText } from "./copyText";
import { errorText, pythonApi, type FracRect, type InspectorFrame } from "./pythonApi";

/** A pointer move under this many pixels is a click, not the start of a drag. */
const DRAG_THRESHOLD_PX = 4;
/** Smallest crop worth saving, as a fraction of the window. */
const MIN_CROP_FRACTION = 0.004;

const ASSET_NAME_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;

export function fmt(n: number): string {
  return n.toFixed(3);
}

export function clickSnippet(x: number, y: number): string {
  return `account.click(${fmt(x)}, ${fmt(y)})`;
}

export function waitPixelSnippet(x: number, y: number, rgb: [number, number, number]): string {
  return `account.wait_pixel(${fmt(x)}, ${fmt(y)}, (${rgb.join(", ")}))`;
}

export function findImageSnippet(fileName: string): string {
  return `account.find_image("${fileName}")`;
}

/** Accepts `play_button` or `play_button.png`; `null` when it is not a safe file name. */
export function normalizeAssetName(raw: string): string | null {
  const base = raw.trim().replace(/\.png$/i, "");
  return ASSET_NAME_PATTERN.test(base) ? `${base}.png` : null;
}

interface Point {
  x: number;
  y: number;
}

function clamp01(n: number): number {
  return Math.min(1, Math.max(0, n));
}

const round4 = (n: number) => Math.round(n * 10000) / 10000;

function toRect(a: Point, b: Point): FracRect {
  return {
    x: round4(Math.min(a.x, b.x)),
    y: round4(Math.min(a.y, b.y)),
    w: round4(Math.abs(a.x - b.x)),
    h: round4(Math.abs(a.y - b.y)),
  };
}

/**
 * Inspector: a still capture of one client window. Hover shows the position as
 * fractions of the window and the color; click copies a ready-made snippet;
 * dragging a rectangle saves a crop to `assets/` for `find_image`.
 */
export function InspectorView() {
  const store = useStore();
  const labelOf = useAccountLabel();
  const prompt = usePrompt();

  const clients = useMemo(
    () => store.accounts.filter((a) => store.launchedByProgram.has(a.UserID) || store.adoptedClients.has(a.UserID)),
    [store.accounts, store.launchedByProgram, store.adoptedClients]
  );
  const [userId, setUserId] = useState<number | null>(null);
  const [frame, setFrame] = useState<InspectorFrame | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<{ x: number; y: number; rgb: [number, number, number] | null } | null>(null);
  const [drag, setDrag] = useState<{ from: Point; to: Point } | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const imgRef = useRef<HTMLImageElement>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const downRef = useRef<{ client: Point; frac: Point } | null>(null);

  // Keep the chosen account valid as clients come and go.
  useEffect(() => {
    if (userId !== null && clients.some((a) => a.UserID === userId)) return;
    setUserId(clients[0]?.UserID ?? null);
  }, [clients, userId]);

  const capture = useCallback(async (id: number) => {
    setLoading(true);
    setError(null);
    try {
      const next = await pythonApi.inspectorCapture(id);
      setFrame(next ?? null);
    } catch (e) {
      setFrame(null);
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    setFrame(null);
    setHover(null);
    if (userId !== null) void capture(userId);
  }, [userId, capture]);

  // Paint the frame into a canvas once, so hovering can read pixels cheaply.
  function onImageLoad() {
    const img = imgRef.current;
    if (!img) return;
    try {
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      if (!ctx) {
        canvasRef.current = null;
        return;
      }
      ctx.drawImage(img, 0, 0);
      canvasRef.current = canvas;
    } catch {
      canvasRef.current = null;
    }
  }

  function readRgb(fx: number, fy: number): [number, number, number] | null {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    try {
      const px = Math.min(canvas.width - 1, Math.floor(fx * canvas.width));
      const py = Math.min(canvas.height - 1, Math.floor(fy * canvas.height));
      const data = canvas.getContext("2d")?.getImageData(px, py, 1, 1).data;
      return data ? [data[0], data[1], data[2]] : null;
    } catch {
      return null;
    }
  }

  function fractionOf(event: MouseEvent<HTMLElement>): Point | null {
    const rect = imgRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0 || rect.height === 0) return null;
    return { x: clamp01((event.clientX - rect.left) / rect.width), y: clamp01((event.clientY - rect.top) / rect.height) };
  }

  function onMouseMove(event: MouseEvent<HTMLElement>) {
    const frac = fractionOf(event);
    if (!frac) return;
    setHover({ ...frac, rgb: readRgb(frac.x, frac.y) });
    const down = downRef.current;
    if (down) {
      const moved = Math.hypot(event.clientX - down.client.x, event.clientY - down.client.y);
      if (moved >= DRAG_THRESHOLD_PX) setDrag({ from: down.frac, to: frac });
    }
  }

  function onMouseDown(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0) return;
    const frac = fractionOf(event);
    if (!frac) return;
    event.preventDefault();
    downRef.current = { client: { x: event.clientX, y: event.clientY }, frac };
  }

  async function onMouseUp(event: MouseEvent<HTMLElement>) {
    const down = downRef.current;
    downRef.current = null;
    const frac = fractionOf(event);
    if (!down || !frac) {
      setDrag(null);
      return;
    }
    const moved = Math.hypot(event.clientX - down.client.x, event.clientY - down.client.y);
    if (moved < DRAG_THRESHOLD_PX) {
      setDrag(null);
      await copyPoint(frac, event.shiftKey);
      return;
    }
    const rect = toRect(down.frac, frac);
    setDrag({ from: down.frac, to: frac });
    await saveCrop(rect);
    setDrag(null);
  }

  async function copyPoint(frac: Point, shift: boolean) {
    if (shift) {
      const rgb = readRgb(frac.x, frac.y);
      if (!rgb) {
        store.addToast("Could not read the color at that point.", "warn");
        return;
      }
      const snippet = waitPixelSnippet(frac.x, frac.y, rgb);
      if (await copyText(snippet)) setNote(`Copied ${snippet}`);
      else store.addToast("The clipboard is not available.", "warn");
      return;
    }
    const snippet = clickSnippet(frac.x, frac.y);
    if (await copyText(snippet)) setNote(`Copied ${snippet}`);
    else store.addToast("The clipboard is not available.", "warn");
  }

  async function saveCrop(rect: FracRect) {
    if (userId === null) return;
    if (rect.w < MIN_CROP_FRACTION || rect.h < MIN_CROP_FRACTION) return;
    // The image the selection was drawn on: the backend cuts from that frame.
    const shownFrameId = frame?.frameId;
    if (shownFrameId === undefined) return;
    const raw = await prompt("Name for this image (letters, numbers, - and _)", "", { maxLength: 64 });
    if (raw === null) return;
    const fileName = normalizeAssetName(raw);
    if (!fileName) {
      store.addToast("Use only letters, numbers, - and _ for the image name.", "warn");
      return;
    }
    try {
      const path = await pythonApi.saveAsset(userId, rect, fileName, shownFrameId);
      const snippet = findImageSnippet(fileName);
      await copyText(snippet);
      store.addToast(`Saved ${path}. Use ${snippet} (copied).`, "success");
      setNote(`Saved ${fileName}`);
    } catch (e) {
      store.addToast(errorText(e), "error");
    }
  }

  const selection = drag ? toRect(drag.from, drag.to) : null;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Select
          className="w-[230px]"
          ariaLabel="Account to inspect"
          value={userId === null ? "" : String(userId)}
          options={clients.map((a) => ({ value: String(a.UserID), label: labelOf(a, a.UserID) }))}
          onChange={(value) => setUserId(Number(value))}
          disabled={clients.length === 0}
        />
        <button
          type="button"
          onClick={() => userId !== null && void capture(userId)}
          disabled={userId === null || loading}
          className="theme-btn inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px] disabled:cursor-not-allowed disabled:opacity-40"
        >
          {loading ? <Loader2 size={12} strokeWidth={2} className="animate-spin" /> : <RefreshCw size={12} strokeWidth={2} />}
          Refresh
        </button>
        <span className="text-[11.5px] text-zinc-500">
          Click copies a click, Shift+click copies a pixel check, drag saves an image for find_image.
        </span>
      </div>

      {clients.length === 0 ? (
        <div className="rounded-lg border border-dashed theme-border px-6 py-10 text-center">
          <Camera size={22} strokeWidth={1.6} className="mx-auto text-zinc-600" />
          <div className="mt-2 text-[13px] font-medium text-zinc-200">No running clients</div>
          <p className="mt-1 text-[12px] text-zinc-500">Launch an account from MultiAlt, then capture its window here.</p>
        </div>
      ) : error ? (
        <div role="alert" className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-[12px] text-red-300">
          {error}
        </div>
      ) : frame ? (
        <div className="space-y-2">
          <div className="flex min-h-[22px] flex-wrap items-center gap-x-4 text-[12px] tabular-nums text-zinc-400">
            {hover ? (
              <>
                <span data-testid="hover-position">
                  x {fmt(hover.x)}, y {fmt(hover.y)}
                </span>
                {hover.rgb ? (
                  <span className="inline-flex items-center gap-1.5" data-testid="hover-color">
                    <span
                      aria-hidden
                      className="inline-block h-3 w-3 rounded-sm border border-zinc-600"
                      style={{ backgroundColor: `rgb(${hover.rgb.join(",")})` }}
                    />
                    RGB {hover.rgb.join(", ")}
                  </span>
                ) : null}
              </>
            ) : (
              <span className="text-zinc-600">Move the pointer over the capture</span>
            )}
            {note ? <span className="ml-auto text-sky-300">{note}</span> : null}
          </div>
          <div
            className="relative inline-block max-w-full cursor-crosshair select-none overflow-hidden rounded-lg border theme-border"
            onMouseMove={onMouseMove}
            onMouseDown={onMouseDown}
            onMouseUp={(e) => void onMouseUp(e)}
            onMouseLeave={() => {
              setHover(null);
              downRef.current = null;
              setDrag(null);
            }}
          >
            <img
              ref={imgRef}
              onLoad={onImageLoad}
              src={`data:image/png;base64,${frame.pngBase64}`}
              alt="Capture of the Roblox window"
              draggable={false}
              className="block max-h-[560px] max-w-full"
            />
            {selection ? (
              <div
                data-testid="selection"
                className="pointer-events-none absolute border border-sky-300 bg-sky-400/20"
                style={{
                  left: `${selection.x * 100}%`,
                  top: `${selection.y * 100}%`,
                  width: `${selection.w * 100}%`,
                  height: `${selection.h * 100}%`,
                }}
              />
            ) : null}
          </div>
          <div className="text-[11px] text-zinc-600">
            {frame.width} x {frame.height} px
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-2 text-[12px] text-zinc-500">
          <Loader2 size={13} strokeWidth={2} className="animate-spin" />
          Capturing the window...
        </div>
      )}
    </div>
  );
}
