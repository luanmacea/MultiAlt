import { useStore } from "../../store";
import { isWindowsPlatform } from "../../utils/platform";
import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { TextField } from "../ui/TextField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";

export function WatcherTab({ s }: { s: UseSettingsReturn }) {
  // `ReadInterval` so e lido dentro de `#[cfg(target_os = "macos")]`
  // (commands/watcher.rs). No Windows o campo so enfeitava a tela.
  const showReadInterval = !isWindowsPlatform(useStore().platformCapabilities);

  // Cada campo abaixo so e lido pelo watcher quando o interruptor acima dele
  // esta ligado (commands/watcher.rs); editavel desligado, era valor morto.
  const exitIfNoConnection = s.getBool("Watcher", "ExitIfNoConnection");
  const closeOnLowMemory = s.getBool("Watcher", "CloseRbxMemory");
  const closeOnTitleMismatch = s.getBool("Watcher", "CloseRbxWindowTitle");

  return (
    <div className="space-y-0">
      <SectionLabel>Scanner</SectionLabel>
      <Toggle
        checked={s.getBool("Watcher", "Enabled")}
        onChange={(v) => s.setBool("Watcher", "Enabled", v)}
        label="Enable Roblox Watcher"
      />
      <NumberField
        value={s.getNumber("Watcher", "ScanInterval", 6)}
        onChange={(v) => s.setNumber("Watcher", "ScanInterval", v)}
        label="Scan Interval"
        min={1}
        max={60}
        suffix="sec"
      />
      {showReadInterval && (
        <NumberField
          value={s.getNumber("Watcher", "ReadInterval", 250)}
          onChange={(v) => s.setNumber("Watcher", "ReadInterval", v)}
          label="Read Interval"
          description="How often the log file is re-read while the watcher is running."
          min={50}
          max={5000}
          suffix="ms"
        />
      )}

      <Divider />
      <SectionLabel>Connection</SectionLabel>
      <Toggle
        checked={s.getBool("Watcher", "ExitIfNoConnection")}
        onChange={(v) => s.setBool("Watcher", "ExitIfNoConnection", v)}
        label="Exit If No Connection"
        description="Close Roblox if it loses connection to the server"
      />
      <NumberField
        value={s.getNumber("Watcher", "NoConnectionTimeout", 60)}
        onChange={(v) => s.setNumber("Watcher", "NoConnectionTimeout", v)}
        label="No Connection Timeout"
        description={!exitIfNoConnection ? "Requires Exit If No Connection" : undefined}
        min={5}
        max={600}
        suffix="sec"
        disabled={!exitIfNoConnection}
      />

      <Divider />
      <SectionLabel>Process Behavior</SectionLabel>

      <Toggle
        checked={s.getBool("Watcher", "ExitOnBeta")}
        onChange={(v) => s.setBool("Watcher", "ExitOnBeta", v)}
        label="Exit on Beta"
        description="Close if a beta version of Roblox is detected"
      />

      <Divider />
      <SectionLabel>Memory & Window</SectionLabel>

      <Toggle
        checked={s.getBool("Watcher", "CloseRbxMemory")}
        onChange={(v) => s.setBool("Watcher", "CloseRbxMemory", v)}
        label="Close If Memory Low"
        description="Terminate Roblox when its memory usage falls below the threshold"
      />
      <NumberField
        value={s.getNumber("Watcher", "MemoryLowValue", 200)}
        onChange={(v) => s.setNumber("Watcher", "MemoryLowValue", v)}
        label="Memory Threshold"
        description={!closeOnLowMemory ? "Requires Close If Memory Low" : undefined}
        min={50}
        max={2048}
        suffix="MB"
        disabled={!closeOnLowMemory}
      />
      <Toggle
        checked={s.getBool("Watcher", "CloseRbxWindowTitle")}
        onChange={(v) => s.setBool("Watcher", "CloseRbxWindowTitle", v)}
        label="Close If Window Title Mismatch"
      />
      <TextField
        value={s.get("Watcher", "ExpectedWindowTitle", "Roblox")}
        onChange={(v) => s.set("Watcher", "ExpectedWindowTitle", v)}
        label="Expected Title"
        placeholder="Roblox"
        disabled={!closeOnTitleMismatch}
      />
      <Toggle
        checked={s.getBool("Watcher", "SaveWindowPositions")}
        onChange={(v) => s.setBool("Watcher", "SaveWindowPositions", v)}
        label="Remember Window Positions"
      />
    </div>
  );
}
