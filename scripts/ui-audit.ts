/**
 * Auditoria de telas pequenas: abre o harness (`bun run dev:ui`) num Edge/Chrome
 * headless, em vários tamanhos de janela, passa por cada tela e lista o que fica
 * cortado ou fora de alcance (`window.__harness.clipReport`). Salva um print de
 * cada tela.
 *
 *   bun run dev:ui                       # em outro terminal (porta 1420)
 *   bun run ui:audit                     # todas as telas, todos os tamanhos
 *   bun run ui:audit --only "Choose Game" --sizes 1093x575
 *
 * Saída: relatório no terminal e prints em `ui-audit-out/` (fora do git).
 * Os tamanhos são janelas reais de monitor comum, já tirando a barra de tarefas:
 * 1366x768 maximizado, 1920x1080 com escala de 150%, 1366x768 com 125% e o
 * mínimo da janela do app (tauri.conf.json: 750x450).
 *
 * Ferramenta de procura, não teste: quem decide se um achado é bug é quem lê.
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const OUT = join(ROOT, "ui-audit-out");
const BASE = process.env.UI_AUDIT_URL ?? "http://localhost:1420";
const PORT = 9300 + Math.floor(Math.random() * 500);

const SIZES: Record<string, [number, number]> = {
  "1366x728": [1366, 728],
  "1280x680": [1280, 680],
  "1093x575": [1093, 575],
  "750x450": [750, 450],
};

/**
 * Um passo é o texto de um botão/aba (primeira linha do texto visível), ou
 * `js:<código>` para o que não tem rótulo. Cada receita começa do zero (página
 * recarregada no cenário `tour`).
 */
const RECIPES: { name: string; steps: string[] }[] = [
  { name: "Accounts", steps: [] },
  { name: "Accounts - one selected", steps: ["text:TestAccount1"] },
  ...["Favorites", "Games", "Recent", "Servers", "Friends", "Follow", "Console", "Windows"].map((tab) => ({
    name: `Choose Game - ${tab}`,
    steps: ["text:TestAccount1", "Choose Game", tab],
  })),
  { name: "Session", steps: ["Session"] },
  { name: "AFK - AFK clicks", steps: ["AFK Mode", "AFK clicks"] },
  { name: "AFK - Auto Rejoin", steps: ["AFK Mode", "Auto Rejoin"] },
  { name: "Avatars - Build", steps: ["Avatars", "Build"] },
  { name: "Avatars - Distribute", steps: ["Avatars", "Distribute"] },
  { name: "Groups", steps: ["Groups", "js:await new Promise((r) => setTimeout(r, 1200))"] },
  {
    // Sem botão de buscar: a busca roda sozinha 500 ms depois de digitar.
    name: "Groups - search",
    steps: [
      "Groups",
      "js:const i = document.querySelector('[data-tour=groups-search] input'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(i, 'pet'); i.dispatchEvent(new Event('input', { bubbles: true })); await new Promise((r) => setTimeout(r, 1500))",
    ],
  },
  { name: "Scripts", steps: ["Scripts"] },
  { name: "Theme", steps: ["Theme"] },
  { name: "Nexus", steps: ["Nexus"] },
  ...["General", "Backups", "Developer", "WebServer", "Watcher", "Isolation", "Versions", "Optimization", "Misc"].map((tab) => ({
    name: `Settings - ${tab}`,
    steps: ["Settings", tab],
  })),
  { name: "What's new", steps: ["What's new"] },
  // O Help da barra lateral está escondido (ENABLE_HELP_BUTTON); o tutorial abre por aqui.
  { name: "Walkthrough", steps: ["Settings", "General", "Open Walkthrough"] },
  { name: "Add menu", steps: ["Add"] },
  ...["Quick Add", "Browser Login", "User:Pass Login", "Import Cookie", "Import Old Account Data", "Create Accounts", "Roblox Versions"].map((item) => ({
    name: `Dialog - ${item}`,
    steps: ["Add", item],
  })),
  // Histórico de sessões (ideia 6) no painel da conta.
  {
    name: "Account panel - History",
    steps: [
      "text:TestAccount1",
      "js:const b = document.querySelector('[data-tour=toolbar-panel]'); if (b.getAttribute('aria-label') !== 'Hide panel') b.click()",
    ],
  },
  // Presets de launch (ideia 13): lista vazia, editor novo e o editor vindo da Choose Game.
  { name: "Dialog - Presets", steps: ["Presets"] },
  { name: "Dialog - New preset", steps: ["Presets", "New preset", "js:document.querySelectorAll('[role=switch]').forEach((s) => s.getAttribute('aria-checked') === 'false' && s.click())"] },
  { name: "Dialog - Save as preset", steps: ["text:TestAccount1", "Choose Game", "Save as preset"] },
  { name: "Actions menu", steps: ["text:TestAccount1", "Actions"] },
  { name: "Dialog - Move to Group", steps: ["text:TestAccount1", "Actions", "📁 Move to Group"] },
  { name: "Dialog - Auto Rejoin", steps: ["text:TestAccount1", "Actions", "🤖 Auto Rejoin"] },
  { name: "Account menu", steps: ["text:TestAccount1", "Account"] },
  { name: "Dialog - AFK from Session", steps: ["Session", "AFK Mode"] },
  { name: "Tutorial - Accounts", steps: ["Tutorial"] },
  { name: "Tutorial - Choose Game", steps: ["text:TestAccount1", "Choose Game", "Tutorial"] },
  { name: "Settings - Check update", steps: ["Settings", "General", "Check Now"] },
];

const args = process.argv.slice(2);
const argValue = (flag: string) => {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : undefined;
};
const only = argValue("--only");
const sizeArg = argValue("--sizes");
const sizes = sizeArg ? sizeArg.split(",") : Object.keys(SIZES);
const recipes = only ? RECIPES.filter((r) => r.name.toLowerCase().includes(only.toLowerCase())) : RECIPES;

function browserPath(): string {
  const candidates = [
    process.env.UI_AUDIT_BROWSER,
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  ].filter(Boolean) as string[];
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error("Edge/Chrome não encontrado (defina UI_AUDIT_BROWSER)");
  return found;
}

class Cdp {
  private id = 0;
  private pending = new Map<number, (v: { result?: unknown; error?: { message: string } }) => void>();
  constructor(private ws: WebSocket) {
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(String(ev.data));
      // confirm()/alert() travam a página: aceita e segue.
      if (msg.method === "Page.javascriptDialogOpening") {
        this.ws.send(JSON.stringify({ id: ++this.id, method: "Page.handleJavaScriptDialog", params: { accept: true } }));
      }
      if (msg.id && this.pending.has(msg.id)) {
        this.pending.get(msg.id)!(msg);
        this.pending.delete(msg.id);
      }
    });
  }
  static async connect(url: string): Promise<Cdp> {
    const ws = new WebSocket(url);
    await new Promise((res, rej) => {
      ws.addEventListener("open", res, { once: true });
      ws.addEventListener("error", rej, { once: true });
    });
    return new Cdp(ws);
  }
  async send<T = unknown>(method: string, params: object = {}): Promise<T> {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    const msg = await Promise.race([
      new Promise<{ result?: unknown; error?: { message: string } }>((res) => this.pending.set(id, res)),
      sleep(15_000).then(() => ({ error: { message: "sem resposta em 15 s" } })),
    ]);
    if (msg.error) throw new Error(`${method}: ${msg.error.message}`);
    return msg.result as T;
  }
  async eval<T = unknown>(expression: string): Promise<T> {
    const r = await this.send<{ result: { value: T }; exceptionDetails?: { text: string; exception?: { description?: string } } }>(
      "Runtime.evaluate",
      { expression, awaitPromise: true, returnByValue: true },
    );
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  }
  close() {
    this.ws.close();
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Clica no botão/aba cujo texto (primeira linha) é exatamente `label`. */
function clickExpr(step: string): string {
  if (step.startsWith("js:")) return `(async () => { ${step.slice(3)} ; return true })()`;
  const byText = step.startsWith("text:");
  const label = JSON.stringify(byText ? step.slice(5) : step);
  const sel = byText ? "*" : "button,[role=tab],[role=menuitem],a";
  return `(() => {
    const els = [...document.querySelectorAll(${JSON.stringify(sel)})].filter((e) => {
      const r = e.getBoundingClientRect();
      if (!r.width || !r.height) return false;
      const t = (e.innerText || "").trim().split("\\n")[0].trim();
      return t === ${label} && (${byText} ? e.children.length === 0 : true);
    });
    if (!els.length) return false;
    els[0].scrollIntoView({ block: "center" });
    els[0].click();
    return true;
  })()`;
}

async function main() {
  mkdirSync(OUT, { recursive: true });
  const profile = mkdtempSync(join(tmpdir(), "ui-audit-"));
  const browser = spawn(browserPath(), [
    "--headless=new",
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--hide-scrollbars=false",
    "about:blank",
  ], { stdio: "ignore" });

  try {
    let wsUrl = "";
    for (let i = 0; i < 50 && !wsUrl; i++) {
      await sleep(200);
      try {
        const list = (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()) as { type: string; webSocketDebuggerUrl: string }[];
        wsUrl = list.find((t) => t.type === "page")?.webSocketDebuggerUrl ?? "";
      } catch {
        // ainda subindo
      }
    }
    if (!wsUrl) throw new Error("o navegador headless não abriu a porta de depuração");
    const cdp = await Cdp.connect(wsUrl);
    await cdp.send("Page.enable");
    await cdp.send("Runtime.enable");

    const report: Record<string, Record<string, unknown>> = {};
    let total = 0;
    for (const size of sizes) {
      const [w, h] = SIZES[size] ?? size.split("x").map(Number) as [number, number];
      await cdp.send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
      for (const recipe of recipes) {
        await cdp.send("Page.navigate", { url: `${BASE}/?scenario=tour&accounts=8` });
        let ready = false;
        for (let i = 0; i < 60 && !ready; i++) {
          await sleep(150);
          ready = await cdp.eval<boolean>(`!!(window.__harness && window.__harness.clipReport && document.querySelector("button"))`).catch(() => false);
        }
        if (!ready) throw new Error(`harness não carregou em ${BASE} — rode \`bun run dev:ui\``);
        await sleep(600);
        let missing: string | null = null;
        for (const step of recipe.steps) {
          const ok = await cdp.eval<boolean>(clickExpr(step)).catch(() => false);
          if (!ok) {
            missing = step;
            break;
          }
          await sleep(500);
        }
        await sleep(400);
        let findings: unknown[] = [];
        if (!missing) {
          try {
            findings = await cdp.eval<unknown[]>("window.__harness.clipReport()");
          } catch (e) {
            missing = `clipReport: ${(e as Error).message}`;
          }
        }
        // Print é extra: tela com animação contínua às vezes não devolve a tempo.
        const shot = await cdp.send<{ data: string }>("Page.captureScreenshot", { format: "png" }).catch(() => null);
        const file = `${size}__${recipe.name.replace(/[^a-z0-9]+/gi, "-")}.png`;
        if (shot) writeFileSync(join(OUT, file), Buffer.from(shot.data, "base64"));
        (report[size] ??= {})[recipe.name] = missing ? { error: `não achei "${missing}"` } : findings;
        total += findings.length;
        const mark = missing ? `ERRO (não achei "${missing}")` : findings.length ? `${findings.length} achado(s)` : "ok";
        const real = await cdp.eval<string>("innerWidth + \"x\" + innerHeight").catch(() => "?");
        if (real !== `${w}x${h}`) throw new Error(`janela emulada ${real}, esperava ${w}x${h}`);
        console.log(`${size.padEnd(9)} ${recipe.name.padEnd(28)} ${mark}`);
      }
    }
    writeFileSync(join(OUT, "report.json"), JSON.stringify(report, null, 2));
    console.log(`\n${total} achado(s). Relatório: ui-audit-out/report.json, prints em ui-audit-out/`);
    cdp.close();
  } finally {
    browser.kill();
  }
}

await main();
