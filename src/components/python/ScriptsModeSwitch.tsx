import { useCallback, useState } from "react";
import { Braces, Gamepad2 } from "lucide-react";
import { ENABLE_PYTHON_SCRIPTS } from "../../featureFlags";

export type ScriptsMode = "python" | "javascript";

/** Where the choice is remembered (per viewer, in the webview only). */
export const SCRIPTS_MODE_STORAGE_KEY = "ram_scripts_mode";

function readStoredMode(): ScriptsMode {
  try {
    return window.localStorage.getItem(SCRIPTS_MODE_STORAGE_KEY) === "python" ? "python" : "javascript";
  } catch {
    return "javascript";
  }
}

/**
 * The mode of the Scripts page. JavaScript is the default so the page behaves
 * exactly as before until someone switches; with the feature flag off it is
 * always JavaScript, whatever is stored.
 */
export function useScriptsMode(enabled: boolean = ENABLE_PYTHON_SCRIPTS): [ScriptsMode, (mode: ScriptsMode) => void] {
  const [stored, setStored] = useState<ScriptsMode>(readStoredMode);
  const setMode = useCallback((next: ScriptsMode) => {
    setStored(next);
    try {
      window.localStorage.setItem(SCRIPTS_MODE_STORAGE_KEY, next);
    } catch {
      // Private window or blocked storage: the choice just lasts until reload.
    }
  }, []);
  return [enabled ? stored : "javascript", setMode];
}

const OPTIONS: {
  id: ScriptsMode;
  title: string;
  description: string;
  icon: typeof Gamepad2;
}[] = [
  {
    id: "python",
    title: "Roblox Player automation (Python)",
    description: "Controls the Roblox windows of your accounts: reads pixels, finds images, sends keys and clicks.",
    icon: Gamepad2,
  },
  {
    id: "javascript",
    title: "MultiAlt automation (JavaScript)",
    description: "Runs inside MultiAlt and automates the app itself through the ram.* API.",
    icon: Braces,
  },
];

/** Segmented switch shown in the toolbar of the Scripts page. */
export function ScriptsModeSwitch({
  mode,
  onChange,
  enabled = ENABLE_PYTHON_SCRIPTS,
}: {
  mode: ScriptsMode;
  onChange: (mode: ScriptsMode) => void;
  enabled?: boolean;
}) {
  if (!enabled) return null;
  return (
    <div role="tablist" aria-label="Scripts mode" className="flex flex-wrap gap-2 py-2.5">
      {OPTIONS.map((option) => {
        const selected = option.id === mode;
        const Icon = option.icon;
        return (
          <button
            key={option.id}
            type="button"
            role="tab"
            aria-selected={selected}
            onClick={() => onChange(option.id)}
            className={`flex-1 basis-[300px] min-w-0 text-left rounded-lg border px-3 py-2 transition-colors ${
              selected
                ? "bg-sky-500/12 border-sky-500/40 text-zinc-100"
                : "bg-zinc-800/30 border-zinc-700/60 text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-200"
            }`}
          >
            <span className="flex items-center gap-2 text-[12.5px] font-medium">
              <Icon size={14} strokeWidth={1.9} className={selected ? "text-sky-300" : "text-zinc-500"} />
              {option.title}
            </span>
            <span className="mt-0.5 block text-[11.5px] leading-snug text-zinc-500">{option.description}</span>
          </button>
        );
      })}
    </div>
  );
}
