import { ShieldAlert } from "lucide-react";

export const PYTHON_PAGE_INTRO = (
  <>
    <span className="block text-[var(--panel-fg)] opacity-80">
      Python automation for the Roblox windows of your accounts
    </span>
    <span className="block mt-0.5">
      Scripts read pixels, find images and send keys and clicks to one window at a time. They do not inject code and
      do not read the game&apos;s memory.
    </span>
  </>
);

export function PythonAutomationView() {
  return (
    <div className="flex-1 min-w-0 min-h-0 flex flex-col">
      <div
        role="alert"
        className="mx-6 mt-4 flex items-start gap-2 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-[12px] text-red-300"
      >
        <ShieldAlert size={14} strokeWidth={1.9} className="mt-0.5 shrink-0" />
        <span>Python scripts have full access to your PC. Only run scripts you trust.</span>
      </div>
    </div>
  );
}
