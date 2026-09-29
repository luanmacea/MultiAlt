/**
 * Escaneia um ou mais arquivos nos DOIS motores: VirusTotal e Windows Defender
 * local. É o que o dono pediu para rodar ao terminar uma funcionalidade.
 *
 *   bun run scan <arquivo> [arquivo...]
 *   bun run scan --release      # todos os instaladores/exe do último build
 *
 * VirusTotal precisa da chave em %USERPROFILE%/.tauri/virustotal.key (ou
 * VT_API_KEY). O Defender roda pelo MpCmdRun, sem configuração.
 */
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const RELEASE = join(ROOT, "src-tauri", "target", "release");

function releaseArtifacts(): string[] {
  const out: string[] = [];
  const exe = join(RELEASE, "roblox-account-manager.exe");
  if (existsSync(exe)) out.push(exe);
  for (const [sub, ext] of [
    ["bundle/nsis", ".exe"],
    ["bundle/msi", ".msi"],
  ] as const) {
    const dir = join(RELEASE, ...sub.split("/"));
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      if (name.endsWith(ext)) out.push(join(dir, name));
    }
  }
  return out;
}

const args = process.argv.slice(2);
const alvos = args.includes("--release")
  ? releaseArtifacts()
  : args.filter((a) => !a.startsWith("--"));

if (alvos.length === 0) {
  console.error("uso: bun run scan <arquivo>... | bun run scan --release");
  process.exit(1);
}

/** Roda um comando herdando o terminal; devolve o código de saída. */
function run(cmd: string, cmdArgs: string[]): number {
  const r = spawnSync(cmd, cmdArgs, { cwd: ROOT, stdio: "inherit", shell: true });
  return r.status ?? 1;
}

let algumMarcou = false;

for (const alvo of alvos) {
  if (!existsSync(alvo) || !statSync(alvo).isFile()) {
    console.error(`\n! pulei (não é arquivo): ${alvo}`);
    continue;
  }
  console.log(`\n${"=".repeat(70)}\n### ${alvo}\n${"=".repeat(70)}`);

  console.log("\n--- Windows Defender (local) ---");
  const def = run("powershell", ["-NoProfile", "-File", "scripts/defender-scan.ps1", `"${alvo}"`]);
  if (def === 2) algumMarcou = true;

  console.log("\n--- VirusTotal ---");
  const vt = run("bun", ["scripts/virustotal.ts", `"${alvo}"`]);
  if (vt !== 0) algumMarcou = true;
}

console.log(`\n${"=".repeat(70)}`);
console.log(algumMarcou ? "RESULTADO: algum motor marcou — ver acima" : "RESULTADO: tudo limpo nos dois motores");
process.exit(algumMarcou ? 2 : 0);
