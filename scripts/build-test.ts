/**
 * Executáveis de TESTE do dono, rápidos: perfil `test-exe` (o release sem LTO,
 * com a otimização dividida entre os núcleos e incremental) — ver
 * docs/development.md, "Build rápido de teste".
 *
 *   bun run build:test                      # as duas edições
 *   bun run build:test --edition standard   # só a padrão (ou full)
 *
 * Copia cada um para %USERPROFILE%/Desktop/MultiAlt-testes com o nome de
 * sempre; se o exe estiver aberto, grava ao lado como `.new`. Não escaneia:
 * este exe não é o que se publica. A versão que vai subir sai do `release`
 * (`bun run tauri build`) e passa pelo `bun run scan`.
 */
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { TEST_PROFILE, testBuildPlan } from "./buildTestPlan";

const ROOT = join(import.meta.dir, "..");
const BUILT = join(ROOT, "src-tauri", "target", TEST_PROFILE, "roblox-account-manager.exe");
const OUT = join(process.env.USERPROFILE ?? "", "Desktop", "MultiAlt-testes");

let plan;
try {
  plan = testBuildPlan(process.argv.slice(2));
} catch (e) {
  console.error((e as Error).message);
  process.exit(1);
}

mkdirSync(OUT, { recursive: true });
let failed = false;
for (const step of plan) {
  const started = Date.now();
  console.log(`== ${step.edition} (${TEST_PROFILE})`);
  const run = spawnSync("bun", ["run", ...step.tauriArgs], { cwd: ROOT, stdio: "inherit", windowsHide: true });
  if (run.status !== 0 || !existsSync(BUILT)) {
    console.error(`falhou: ${step.edition} (código ${run.status})`);
    failed = true;
    continue;
  }
  let target = join(OUT, step.fileName);
  try {
    copyFileSync(BUILT, target);
  } catch {
    // Aberto: o Windows não deixa sobrescrever um exe em uso.
    target = target.replace(/\.exe$/, ".new");
    copyFileSync(BUILT, target);
  }
  const { size, mtime } = statSync(target);
  const secs = Math.round((Date.now() - started) / 1000);
  console.log(
    `${target}  ${(size / 1048576).toFixed(1)} MB  ${mtime.toLocaleTimeString("pt-BR")}  (${Math.floor(secs / 60)}m${String(secs % 60).padStart(2, "0")}s)`
  );
}
process.exit(failed ? 1 : 0);
