/**
 * Cenários do harness para o modo "Roblox Player automation (Python)" da página
 * Scripts: `?scenario=python-not-installed | python-installing | python-ready |
 * python-running | python-dev-mode`.
 *
 * Eles só entregam dados (runtime, scripts, execuções, linhas de log, valores do
 * dashboard, uma captura de janela desenhada na hora). Nada aqui implementa o
 * que a tela testa: instalar, parar, travar entrada e salvar arquivo de verdade
 * são do backend Rust. Depois de abrir o cenário, escolha Scripts na barra
 * lateral — o modo Python já fica selecionado.
 *
 * As contas do harness são as TestAccountN de `scenarios.ts`.
 */
import { harnessEmit, setInvokeHandler, type InvokeHandler } from "./bus";

interface HarnessAccount {
  UserID: number;
  Username: string;
}

type Args = Record<string, unknown>;

interface RunInfo {
  id: string;
  script: string;
  accounts: number[];
  mode: "one_at_a_time" | "all_together";
  trigger: { kind: "manual" } | { kind: "interval"; minutes: number } | { kind: "loop" };
  source: "app" | "terminal";
  state: string;
  startedMs: number;
  endedMs: number | null;
  nextRunMs: number | null;
  error: string | null;
}

interface LogLine {
  runId: string | null;
  userId: number | null;
  level: "log" | "warn" | "error" | "system" | "blocked";
  message: string;
  code: string | null;
  hint: string | null;
  ts: number;
}

const FARM_SOURCE = `import multialt
from multialt import accounts, sleep, stopped, dashboard

dashboard.farm_report()


def main(account):
    """Runs once per account."""
    while not stopped():
        with account.focused():
            account.press("E")
        dashboard.report_loop(account)
        sleep(30)
`;

const SCRIPTS: Record<string, string> = {
  "farm.py": FARM_SOURCE,
  "anti_afk.py": 'from multialt import sleep, stopped\n\n\ndef main(account):\n    while not stopped():\n        with account.focused():\n            account.press("Space")\n        sleep(240)\n',
  "claude_test.py": "def main(account):\n    account.log('hello from a terminal run')\n",
};

const BLOCKS = ["runner.py", "focus.py", "vision.py", "actions.py", "dashboard.py"];

const PACKAGES = ["numpy 2.1.3", "opencv-python-headless 4.10.0"];

/** Uma captura de janela fingindo ser o Roblox: gradiente, um botão amarelo e um texto. */
function fakeWindowPng(): { width: number; height: number; pngBase64: string } {
  const width = 960;
  const height = 540;
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    const sky = ctx.createLinearGradient(0, 0, 0, height);
    sky.addColorStop(0, "#5aa7ff");
    sky.addColorStop(1, "#bfe3ff");
    ctx.fillStyle = sky;
    ctx.fillRect(0, 0, width, height);
    ctx.fillStyle = "#3d7a2f";
    ctx.fillRect(0, height * 0.72, width, height * 0.28);
    ctx.fillStyle = "#ffc800";
    ctx.fillRect(width * 0.44, height * 0.8, width * 0.12, height * 0.09);
    ctx.fillStyle = "#222";
    ctx.font = "20px sans-serif";
    ctx.fillText("PLAY", width * 0.475, height * 0.86);
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(width * 0.04, height * 0.05, width * 0.2, height * 0.05);
  }
  const url = canvas.toDataURL("image/png");
  return { width, height, pngBase64: url.slice(url.indexOf(",") + 1) };
}

interface Options {
  installed: boolean;
  installing?: boolean;
  scripts?: boolean;
  running?: boolean;
  devMode?: boolean;
}

export function createPythonScenarios(
  base: InvokeHandler,
  accounts: HarnessAccount[]
): Record<string, () => void> {
  function boot(options: Options) {
    try {
      window.localStorage.setItem("ram_scripts_mode", "python");
    } catch {
      // Sem localStorage: o seletor da página abre em JavaScript e dá para trocar.
    }

    const now = Date.now();
    const scripts = new Map<string, { source: string; modifiedMs: number }>();
    if (options.scripts !== false) {
      for (const [name, source] of Object.entries(SCRIPTS)) scripts.set(name, { source, modifiedMs: now });
    }
    const blockModified = new Set<string>(options.devMode ? ["runner.py", "vision.py"] : []);
    const blockNewDefault = new Set<string>(options.devMode ? ["vision.py"] : []);
    const configs: Record<string, unknown> = {};
    let developerMode = options.devMode === true;
    let installed = options.installed;
    let installing = options.installing === true;

    const ids = accounts.slice(0, 3).map((a) => a.UserID);
    const runs: RunInfo[] = options.running
      ? [
          {
            id: "r1",
            script: "farm.py",
            accounts: ids.slice(0, 2),
            mode: "one_at_a_time",
            trigger: { kind: "loop" },
            source: "app",
            state: "running",
            startedMs: now - 12 * 60_000,
            endedMs: null,
            nextRunMs: null,
            error: null,
          },
          {
            id: "r2",
            script: "claude_test.py",
            accounts: ids.slice(0, 1),
            mode: "all_together",
            trigger: { kind: "interval", minutes: 5 },
            source: "terminal",
            state: "waiting",
            startedMs: now - 3 * 60_000,
            endedMs: now - 90_000,
            nextRunMs: now + 3.5 * 60_000,
            error: null,
          },
        ]
      : [];

    const logs: LogLine[] = [];
    const log = (line: Partial<LogLine> & { message: string }): LogLine => {
      const full: LogLine = {
        runId: "r1",
        userId: null,
        level: "log",
        code: null,
        hint: null,
        ts: Date.now(),
        ...line,
      };
      logs.push(full);
      if (logs.length > 5000) logs.shift();
      harnessEmit("python-log", full);
      return full;
    };

    // --- dashboard -------------------------------------------------------
    const loops: Record<number, number> = { [ids[0]]: 14, [ids[1]]: 11 };
    const series: [number, number][] = Array.from({ length: 12 }, (_, i) => [now - (12 - i) * 5 * 60_000, 20 + ((i * 7) % 9)]);
    const eventLines: [number, string][] = [
      [now - 60_000, "Main loop started"],
      [now - 30_000, "TestAccount1 reached the shop"],
    ];
    const dashboard = {
      script: "farm.py",
      layout: [
        { id: "accounts", type: "accounts", title: "Accounts", columns: ["Status", "Loops", "Last error", "Running for"] },
        { id: "total", type: "stat", title: "Total loops" },
        { id: "goal", type: "progress", title: "Daily goal", max: 100 },
        { id: "health", type: "badge", title: "Farm health" },
        { id: "rate", type: "line", title: "Loops per hour" },
        { id: "recent", type: "table", title: "Recent finds", columns: ["Time", "Item", "Rarity"] },
        { id: "events", type: "log", title: "Events", max: 8 },
      ] as unknown[],
      values: {} as Record<string, unknown>,
      series: {} as Record<string, unknown>,
      updatedMs: now,
    };
    const refreshDashboard = () => {
      const total = Object.values(loops).reduce((a, b) => a + b, 0);
      dashboard.values = {
        [`accounts@${ids[0]}`]: { Status: "Working", Loops: loops[ids[0]], "Last error": "-", "Running for": "12m" },
        [`accounts@${ids[1]}`]: { Status: "Waiting for focus", Loops: loops[ids[1]], "Last error": "not_foreground", "Running for": "12m" },
        total: { value: total, delta: 3 },
        goal: Math.min(100, total * 2),
        health: { level: loops[ids[1]] % 4 === 0 ? "warn" : "ok", text: loops[ids[1]] % 4 === 0 ? "Slow" : "Running" },
        recent: [
          ["10:42", "Dragon fruit", "Legendary"],
          ["10:31", "Kitsune", "Mythical"],
        ],
      };
      dashboard.series = { rate: series, events: eventLines };
      dashboard.updatedMs = Date.now();
    };
    if (options.running) refreshDashboard();

    // --- ao vivo ---------------------------------------------------------
    if (options.running) {
      const name = (i: number) => accounts[i]?.Username ?? "TestAccount";
      log({ runId: "r1", level: "system", message: "Run r1 started: farm.py on 2 accounts (one at a time, loop)" });
      log({ runId: "r1", userId: ids[0], level: "system", message: `Focus taken for ${name(0)}` });
      log({ runId: "r1", userId: ids[0], message: "Pressed E, waiting 30 s" });
      log({
        runId: "r1",
        userId: ids[1],
        level: "blocked",
        code: "not_foreground",
        message: `Blocked: ${name(1)}'s window is not in front. Input only goes to the focused window.`,
        hint: 'Wrap input in "with account.focused():".',
      });
      log({ runId: "r1", userId: ids[1], level: "warn", message: "Retrying with account.focused()" });
      log({ runId: "r2", userId: ids[0], message: "hello from a terminal run" });
      let tick = 0;
      window.setInterval(() => {
        tick += 1;
        const who = tick % 2 === 0 ? 0 : 1;
        log({ runId: "r1", userId: ids[who], message: `Loop ${loops[ids[who]] + 1} done` });
        if (tick % 5 === 0) log({ runId: "r1", userId: ids[1], level: "error", message: "ValueError: pixel did not match after 10 s" });
        loops[ids[who]] += 1;
        if (tick % 3 === 0) {
          series.push([Date.now(), 20 + ((tick * 5) % 11)]);
          if (series.length > 500) series.shift();
          eventLines.push([Date.now(), `Loop ${loops[ids[who]]} finished for ${name(who)}`]);
        }
        refreshDashboard();
        harnessEmit("python-dashboard-update", { ...dashboard });
      }, 1500);
    }

    // --- instalação simulada (só mostra o progresso) ----------------------
    function simulateProgress(done: () => void): void {
      const steps: [string, string, number, number][] = [
        ["python", "Downloading python-3.12.7-embed-amd64.zip", 12_000_000, 28_000_000],
        ["python", "Downloading python-3.12.7-embed-amd64.zip", 28_000_000, 28_000_000],
        ["pip", "Running get-pip.py", 0, 0],
        ["packages", "Collecting numpy", 0, 0],
        ["packages", "Collecting opencv-python-headless", 0, 0],
        ["ready", "Python is ready", 0, 0],
      ];
      let i = 0;
      const timer = window.setInterval(() => {
        const [stage, message, downloaded, total] = steps[i];
        harnessEmit("python-install-progress", { stage, message, downloaded, total });
        i += 1;
        if (i >= steps.length) {
          window.clearInterval(timer);
          done();
        }
      }, 900);
    }

    if (installing) {
      let step = 0;
      window.setInterval(() => {
        const stages = ["python", "pip", "packages"] as const;
        const stage = stages[Math.floor(step / 4) % 3];
        harnessEmit("python-install-progress", {
          stage,
          message: stage === "python" ? "Downloading python-3.12.7-embed-amd64.zip" : stage === "pip" ? "Running get-pip.py" : "Collecting opencv-python-headless",
          downloaded: stage === "python" ? ((step % 4) + 1) * 7_000_000 : 0,
          total: stage === "python" ? 28_000_000 : 0,
        });
        step += 1;
      }, 1200);
    }

    // --- backend de mentira ----------------------------------------------
    const handler: InvokeHandler = (cmd: string, args: Args) => {
      switch (cmd) {
        case "get_running_instances":
          return options.installed ? ids.map((userId, i) => ({ userId, pid: 4200 + i })) : [];
        case "python_runtime_status":
          return {
            installed,
            version: installed ? "3.12.7" : null,
            pythonExe: installed ? "C:\\Users\\you\\AppData\\Local\\MultiAlt\\python-runtime\\current\\python.exe" : null,
            packages: installed ? PACKAGES : [],
            installing,
          };
        case "python_install_runtime":
        case "python_reinstall_runtime":
          return new Promise<void>((resolve) => {
            installing = true;
            simulateProgress(() => {
              installed = true;
              installing = false;
              resolve();
            });
          });
        case "python_workspace_info":
          return {
            root: "C:\\Users\\you\\AppData\\Local\\MultiAlt\\python",
            scriptsDir: "C:\\Users\\you\\AppData\\Local\\MultiAlt\\python\\scripts",
            assetsDir: "C:\\Users\\you\\AppData\\Local\\MultiAlt\\python\\assets",
            pythonExe: "C:\\Users\\you\\AppData\\Local\\MultiAlt\\python-runtime\\current\\python.exe",
            cliCommand:
              '"C:\\Users\\you\\AppData\\Local\\MultiAlt\\python-runtime\\current\\python.exe" -m multialt run scripts/farm.py --account TestAccount1 --mode one_at_a_time',
          };
        case "python_list_scripts":
          return [...scripts.entries()].map(([name, file]) => ({ name, size: file.source.length, modifiedMs: file.modifiedMs }));
        case "python_read_script":
          return scripts.get(String(args.name))?.source ?? "";
        case "python_write_script":
          scripts.set(String(args.name), { source: String(args.source), modifiedMs: Date.now() });
          return undefined;
        case "python_create_script":
          scripts.set(String(args.name), { source: `# new ${String(args.template)} script\n`, modifiedMs: Date.now() });
          return undefined;
        case "python_delete_script":
          scripts.delete(String(args.name));
          return undefined;
        case "python_list_default_blocks":
          return BLOCKS.map((name) => ({ name, modified: blockModified.has(name), newDefaultAvailable: blockNewDefault.has(name) }));
        case "python_read_default_block":
          return `# ${String(args.name)}\n# (default block shown by the harness)\n\n\ndef run(module, ctx):\n    pass\n`;
        case "python_write_default_block":
          blockModified.add(String(args.name));
          return undefined;
        case "python_restore_default_block":
          blockModified.delete(String(args.name));
          blockNewDefault.delete(String(args.name));
          return undefined;
        case "python_open_workspace_folder":
        case "python_open_in_editor":
          return undefined;
        case "python_get_run_configs":
          return configs;
        case "python_save_run_config": {
          const config = args.config as { script: string };
          configs[config.script] = config;
          return undefined;
        }
        case "python_start_run": {
          const config = args.config as Pick<RunInfo, "script" | "accounts" | "mode" | "trigger">;
          const run: RunInfo = {
            id: `r${runs.length + 3}`,
            ...config,
            source: "app",
            state: "running",
            startedMs: Date.now(),
            endedMs: null,
            nextRunMs: null,
            error: null,
          };
          runs.push(run);
          harnessEmit("python-run-update", run);
          return run.id;
        }
        case "python_stop_run": {
          const run = runs.find((item) => item.id === args.runId);
          if (run) {
            run.state = "finished";
            run.endedMs = Date.now();
            run.nextRunMs = null;
            harnessEmit("python-run-update", run);
          }
          return undefined;
        }
        case "python_stop_all_runs":
          for (const run of runs) {
            if (run.state !== "finished" && run.state !== "failed") {
              run.state = "finished";
              run.endedMs = Date.now();
              run.nextRunMs = null;
              harnessEmit("python-run-update", run);
            }
          }
          return undefined;
        case "python_list_runs":
          return runs;
        case "python_get_dashboard":
          return options.running ? [dashboard] : [];
        case "python_clear_dashboard":
          return undefined;
        case "python_inspector_capture":
          return fakeWindowPng();
        case "python_save_asset":
          return `C:\\Users\\you\\AppData\\Local\\MultiAlt\\python\\assets\\${String(args.name)}`;
        case "python_set_developer_mode":
          developerMode = args.enabled === true;
          return undefined;
        case "python_get_logs":
          return logs;
        case "get_all_settings": {
          const all = base(cmd, args) as Record<string, Record<string, string>>;
          return { ...all, Python: { DeveloperMode: developerMode ? "true" : "false" } };
        }
        default:
          return base(cmd, args);
      }
    };
    setInvokeHandler(handler);
  }

  return {
    "python-not-installed": () => boot({ installed: false }),
    "python-installing": () => boot({ installed: false, installing: true }),
    "python-ready": () => boot({ installed: true }),
    "python-running": () => boot({ installed: true, running: true }),
    "python-dev-mode": () => boot({ installed: true, devMode: true }),
  };
}
