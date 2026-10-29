/**
 * Workspace de mentira do modo Python para o harness: uma árvore de arquivos na
 * memória com o mesmo formato que o backend entrega (`FsNode`, `GlobalModule`).
 * Só entrega dados e guarda o que a tela escreve; as regras de verdade (caminho
 * válido, lixeira, pasta gravável) são do backend Rust.
 */

export interface FsNodeOut {
  path: string;
  name: string;
  kind: "file" | "dir";
  size: number;
  modifiedMs: number;
  writable: boolean;
  pinned: boolean;
  runnable: boolean;
  children: FsNodeOut[] | null;
}

export interface GlobalFunctionOut {
  name: string;
  signature: string;
  doc: string;
}

export interface GlobalModuleOut {
  path: string;
  module: string;
  pinned: boolean;
  modified: boolean;
  newDefaultAvailable: boolean;
  functions: GlobalFunctionOut[];
}

const EXAMPLE_SOURCE = `# Example: walk forward, jump, open the menu, wait and close it - in a loop, per account.
# Open the Inspector tab to pick a point or a color on the game window, then use
# wait_pixel(account, x, y, color) or click(account, x, y) from the default blocks.
from multialt import dashboard, sleep, stopped
from multialt.defaults.actions import press_key
from lib.helpers import walk_forward

dashboard.farm_report()


def main(account):
    """Runs once per account until stopped."""
    while not stopped():
        with account.focused():
            walk_forward(account, 1.5)    # W
            press_key(account, "Space")   # jump
            press_key(account, "Escape")  # menu (Esc + L leaves the game: never press L)
        sleep(2)
        with account.focused():
            press_key(account, "Escape")  # close the menu
        dashboard.report_loop(account)
`;

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

const HELPERS_SOURCE = `from multialt import sleep


def walk_forward(account: Account, seconds: float = 1.0) -> None:
    """Hold W for a number of seconds."""
    account.key_down("W")
    sleep(seconds)
    account.key_up("W")


def click_when_ready(account: Account, x: int, y: int, timeout: float = 10) -> bool:
    """Wait until the point is not black, then click it."""
    pass
`;

const BLOCK_SOURCES: Record<string, string> = {
  "runner.py": `def run_script(module, ctx):
    """Call main(account) for every account in turn."""
    pass
`,
  "focus.py": `def focused(account):
    """Context manager: bring the window of the account to the front."""
    pass
`,
  "vision.py": `def wait_pixel(account, x, y, color, timeout=10):
    """Wait until the pixel at (x, y) has the color."""
    pass


def find_image(account, name, threshold=0.9):
    """Find an image from assets/ in the window."""
    pass
`,
  "actions.py": `def press_key(account, key):
    """Send one key to the focused window."""
    pass


def click(account, x, y):
    """Click at a point of the window."""
    pass
`,
  "dashboard.py": `def farm_report():
    """Declare the standard farm dashboard."""
    pass
`,
};

export const BLOCK_NAMES = Object.keys(BLOCK_SOURCES);

const EXAMPLE_FILES: Record<string, string> = {
  "walk_and_jump.py": EXAMPLE_SOURCE,
  "click_play_button.py": `# Example: find a button picture in the window, then click it.
from multialt.defaults.vision import find_image
from multialt.defaults.actions import click


def main(account):
    hit = find_image(account, "play_button.png")
    if hit:
        click(account, hit.x, hit.y)
`,
};

/** A small PNG drawn on a canvas, as the data URL the backend would send for assets. */
function fakeAssetPng(): string {
  const canvas = document.createElement("canvas");
  canvas.width = 96;
  canvas.height = 36;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    ctx.fillStyle = "#ffc800";
    ctx.fillRect(0, 0, 96, 36);
    ctx.fillStyle = "#222";
    ctx.font = "16px sans-serif";
    ctx.fillText("PLAY", 28, 24);
  }
  return canvas.toDataURL("image/png");
}

interface VfsFile {
  content: string;
  modifiedMs: number;
}

const WRITABLE_ROOTS = ["scripts", "lib", "assets"];

/** First line of the docstring and the signature of each top-level def, read from text like the backend does. */
export function parseFunctions(source: string): GlobalFunctionOut[] {
  const out: GlobalFunctionOut[] = [];
  const lines = source.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const match = /^def ([A-Za-z_][A-Za-z0-9_]*)\((.*?)\)( -> [^:]+)?:/.exec(lines[i]);
    if (!match || match[1].startsWith("_")) continue;
    const doc = /^"""(.*?)"""?$/.exec((lines[i + 1] ?? "").trim())?.[1] ?? "";
    // Like the backend: parameter annotations stay, the return annotation is dropped.
    out.push({ name: match[1], signature: `${match[1]}(${match[2]})`, doc });
  }
  return out;
}

export interface FakeWorkspace {
  tree(): FsNodeOut;
  read(path: string): string;
  readImage(path: string): string;
  write(path: string, content: string): void;
  create(path: string, kind: "file" | "dir"): void;
  rename(from: string, to: string): void;
  remove(path: string): void;
  scriptNames(): string[];
  globals(): GlobalModuleOut[];
  restoreBlock(name: string): void;
}

export function createFakeWorkspace(options: {
  scripts: boolean;
  developerMode: () => boolean;
  modifiedBlocks: Set<string>;
  newDefaultBlocks: Set<string>;
}): FakeWorkspace {
  const now = Date.now();
  const files = new Map<string, VfsFile>();
  const dirs = new Set<string>(["scripts", "lib", "assets", "examples", "multialt", "multialt/defaults"]);
  const png = fakeAssetPng();

  const put = (path: string, content: string, ageMs = 0) => files.set(path, { content, modifiedMs: now - ageMs });
  if (options.scripts) {
    put("scripts/example_walk_jump.py", EXAMPLE_SOURCE, 60_000);
    put("scripts/farm.py", FARM_SOURCE, 3_600_000);
    put("scripts/anti_afk.py", "from multialt import sleep, stopped\n\n\ndef main(account):\n    while not stopped():\n        with account.focused():\n            account.press(\"Space\")\n        sleep(240)\n", 7_200_000);
    put("scripts/claude_test.py", "def main(account):\n    account.log('hello from a terminal run')\n", 1_800_000);
  }
  put("lib/__init__.py", "", 60_000);
  put("lib/helpers.py", HELPERS_SOURCE, 60_000);
  put("assets/play_button.png", png, 120_000);
  put("assets/shop_icon.png", png, 180_000);
  for (const [name, content] of Object.entries(EXAMPLE_FILES)) put(`examples/${name}`, content, 86_400_000);
  for (const [name, content] of Object.entries(BLOCK_SOURCES)) put(`multialt/defaults/${name}`, content, 86_400_000);

  const topOf = (path: string) => path.split("/")[0];
  const isPinned = (path: string) =>
    path === "multialt" || path === "multialt/defaults" || path.startsWith("multialt/defaults/");
  const isWritable = (path: string) =>
    WRITABLE_ROOTS.includes(topOf(path)) || (isPinned(path) && options.developerMode());
  const parentOf = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf("/")));
  const nameOf = (path: string) => path.slice(path.lastIndexOf("/") + 1);

  function build(path: string): FsNodeOut {
    const childDirs = [...dirs].filter((dir) => dir !== path && parentOf(dir) === path && dir.length > 0);
    const childFiles = [...files.keys()].filter((file) => parentOf(file) === path);
    const children: FsNodeOut[] = [
      ...childDirs.sort().map(build),
      ...childFiles.sort().map((file) => {
        const entry = files.get(file) as VfsFile;
        return {
          path: file,
          name: nameOf(file),
          kind: "file" as const,
          size: entry.content.length,
          modifiedMs: entry.modifiedMs,
          writable: isWritable(file),
          pinned: isPinned(file),
          runnable: topOf(file) === "scripts" && parentOf(file) === "scripts" && file.endsWith(".py"),
          children: null,
        };
      }),
    ];
    return {
      path,
      name: nameOf(path),
      kind: "dir",
      size: 0,
      modifiedMs: now,
      writable: path === "" ? false : WRITABLE_ROOTS.includes(topOf(path)),
      pinned: isPinned(path),
      runnable: false,
      children,
    };
  }

  return {
    tree: () => build(""),
    read: (path) => files.get(path)?.content ?? "",
    readImage: () => png,
    write: (path, content) => put(path, content),
    create: (path, kind) => {
      if (kind === "dir") dirs.add(path);
      else put(path, "");
    },
    rename: (from, to) => {
      const entry = files.get(from);
      if (entry) {
        files.delete(from);
        files.set(to, { ...entry, modifiedMs: Date.now() });
      } else if (dirs.has(from)) {
        dirs.delete(from);
        dirs.add(to);
      }
    },
    remove: (path) => {
      files.delete(path);
      dirs.delete(path);
    },
    scriptNames: () => [...files.keys()].filter((path) => path.startsWith("scripts/") && !path.slice(8).includes("/")).map((path) => path.slice(8)),
    globals: () => {
      const modules: GlobalModuleOut[] = [];
      for (const [path, entry] of files) {
        const pinned = isPinned(path);
        if (!pinned && !(path.startsWith("lib/") && path.endsWith(".py"))) continue;
        const name = nameOf(path);
        if (name === "__init__.py" && entry.content.trim() === "") continue;
        modules.push({
          path,
          module: path.replace(/\.py$/, "").split("/").join("."),
          pinned,
          modified: pinned && options.modifiedBlocks.has(name),
          newDefaultAvailable: pinned && options.newDefaultBlocks.has(name),
          functions: parseFunctions(entry.content),
        });
      }
      return modules.sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.path.localeCompare(b.path));
    },
    restoreBlock: (name) => put(`multialt/defaults/${name}`, BLOCK_SOURCES[name] ?? ""),
  };
}
