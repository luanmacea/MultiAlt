/**
 * Roda a suíte de testes de UMA funcionalidade (frontend + backend).
 *
 *   bun run t launch          # só o que cobre launch
 *   bun run t --list          # lista as suítes
 *   bun run t --audit         # acusa teste que não está em nenhuma suíte
 *
 * A suíte completa continua sendo `bun run check`, obrigatória antes do commit.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { spawnSync } from "node:child_process";
import { SUITES, SUITE_NAMES } from "./test-suites";

const ROOT = join(import.meta.dir, "..");
const RUST_SRC = join(ROOT, "src-tauri", "src");
const FRONT_SRC = join(ROOT, "src");
const RELEASE_SCRIPTS = join(ROOT, ".github", "scripts");

function walk(dir: string, match: (p: string) => boolean): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === "target") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full, match));
    else if (match(full)) out.push(full);
  }
  return out;
}

/** Todos os `mod <nome>_tests` declarados no backend. */
function rustTestMods(): string[] {
  const mods = new Set<string>();
  for (const file of walk(RUST_SRC, (p) => p.endsWith(".rs"))) {
    const source = readFileSync(file, "utf8");
    for (const m of source.matchAll(/^\s*mod\s+([a-z0-9_]*tests)\s*\{/gm)) mods.add(m[1]);
  }
  return [...mods].sort();
}

/**
 * Todos os arquivos de teste que o vitest roda (frontend e scripts de
 * release), relativos à raiz, com "/".
 */
function frontTestFiles(): string[] {
  return [
    ...walk(FRONT_SRC, (p) => /\.test\.tsx?$/.test(p)),
    ...walk(RELEASE_SCRIPTS, (p) => /\.test\.mjs$/.test(p)),
  ]
    .map((p) => relative(ROOT, p).split(sep).join("/"))
    .sort();
}

function list(): void {
  const width = Math.max(...SUITE_NAMES.map((n) => n.length));
  console.log("Suítes disponíveis:\n");
  for (const name of SUITE_NAMES) {
    console.log(`  ${name.padEnd(width)}  ${SUITES[name].description}`);
  }
  console.log("\n  bun run t <suite>     roda uma suíte");
  console.log("  bun run check         roda tudo (obrigatório antes do commit)");
}

/** Acusa testes que nenhuma suíte cobre — senão a divisão apodrece em silêncio. */
function audit(): number {
  const mappedRust = new Set(Object.values(SUITES).flatMap((s) => s.rust));
  const orphanRust = rustTestMods().filter(
    (mod) => ![...mappedRust].some((filter) => mod.includes(filter) || filter.includes(mod))
  );

  const mappedFront = Object.values(SUITES).flatMap((s) => s.front);
  const orphanFront = frontTestFiles().filter(
    (file) => !mappedFront.some((entry) => file === entry || file.startsWith(entry + "/"))
  );

  if (orphanRust.length === 0 && orphanFront.length === 0) {
    console.log("Auditoria ok: toda suíte de teste está mapeada em scripts/test-suites.ts");
    return 0;
  }
  if (orphanRust.length > 0) {
    console.log("Módulos Rust fora de qualquer suíte:");
    for (const mod of orphanRust) console.log(`  - ${mod}`);
  }
  if (orphanFront.length > 0) {
    console.log("Arquivos de teste do frontend fora de qualquer suíte:");
    for (const file of orphanFront) console.log(`  - ${file}`);
  }
  console.log("\nAdicione-os a uma suíte em scripts/test-suites.ts.");
  return 1;
}

function run(command: string, args: string[], cwd: string): number {
  console.log(`\n$ ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd, stdio: "inherit", shell: true });
  return result.status ?? 1;
}

function runSuite(name: string): number {
  const suite = SUITES[name];
  if (!suite) {
    console.error(`Suíte desconhecida: ${name}\n`);
    list();
    return 1;
  }

  console.log(`Suíte "${name}": ${suite.description}`);
  let failed = 0;

  if (suite.front.length > 0) {
    // Arquivos inexistentes fazem o vitest falhar; filtra antes.
    const existing = suite.front.filter((entry) => {
      try {
        statSync(join(ROOT, entry));
        return true;
      } catch {
        return false;
      }
    });
    if (existing.length > 0) {
      failed += run("bun", ["x", "vitest", "run", ...existing], ROOT) === 0 ? 0 : 1;
    }
  }

  if (suite.rust.length > 0) {
    // Um cargo test por filtro: o libtest aceita só um filtro posicional.
    // `--lib` evita reconstruir os alvos de integração a cada filtro.
    for (const filter of suite.rust) {
      failed +=
        run("cargo", ["test", "--all-features", "--lib", filter], join(ROOT, "src-tauri")) === 0
          ? 0
          : 1;
    }
  }

  for (const target of suite.rustIntegration ?? []) {
    failed +=
      run("cargo", ["test", "--all-features", "--test", target], join(ROOT, "src-tauri")) === 0
        ? 0
        : 1;
  }

  if (failed > 0) {
    console.error(`\nSuíte "${name}": ${failed} comando(s) falharam.`);
    return 1;
  }
  console.log(`\nSuíte "${name}": ok. Rode "bun run check" antes de commitar.`);
  return 0;
}

const arg = process.argv[2];
if (!arg || arg === "--list" || arg === "-l") {
  list();
  process.exit(0);
}
process.exit(arg === "--audit" ? audit() : runSuite(arg));
