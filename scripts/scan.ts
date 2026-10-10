/**
 * Escaneia um ou mais arquivos nos DOIS motores: VirusTotal e Windows Defender
 * local. É o que o dono pediu para rodar ao terminar uma funcionalidade.
 *
 *   bun run scan <arquivo> [arquivo...]
 *   bun run scan --release      # todos os instaladores/exe do último build
 *   bun run scan --edition full <arquivo>...   # arquivos da edição completa
 *
 * Edição padrão (o default): qualquer motor que marque reprova. Edição
 * completa: o VirusTotal com só o Trapmine de ML (`*.ml.score`) marcando sai
 * "aceito (edição completa: só o Trapmine de ML)" e código 0; qualquer outro
 * motor, ou o Defender, reprova igual (ver scanVerdict.ts e
 * docs/development.md, "As duas edições").
 *
 * VirusTotal precisa da chave em %USERPROFILE%/.tauri/virustotal.key (ou
 * VT_API_KEY). O Defender roda pelo MpCmdRun, sem configuração.
 */
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import {
  defenderResult,
  parseEdition,
  scanSummary,
  scanTargets,
  virusTotalResult,
  type Edition,
  type FileScan,
} from "./scanVerdict";

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
let edicao: Edition = "standard";
try {
  edicao = parseEdition(args);
} catch (e) {
  console.error((e as Error).message);
  process.exit(1);
}
const alvos = args.includes("--release") ? releaseArtifacts() : scanTargets(args);

if (alvos.length === 0) {
  console.error("uso: bun run scan [--edition standard|full] <arquivo>... | bun run scan --release [--edition full]");
  process.exit(1);
}
console.log(`edição: ${edicao === "full" ? "completa (só o Trapmine de ML é aceito)" : "padrão (nenhum motor pode marcar)"}`);

/** Roda um comando herdando o terminal; devolve o código de saída. */
function run(cmd: string, cmdArgs: string[]): number {
  const r = spawnSync(cmd, cmdArgs, { cwd: ROOT, stdio: "inherit", shell: true });
  return r.status ?? 1;
}

const resultados: FileScan[] = [];

for (const alvo of alvos) {
  if (!existsSync(alvo) || !statSync(alvo).isFile()) {
    console.error(`\n! pulei (não é arquivo): ${alvo}`);
    continue;
  }
  console.log(`\n${"=".repeat(70)}\n### ${alvo}\n${"=".repeat(70)}`);

  console.log("\n--- Windows Defender (local) ---");
  const def = run("powershell", ["-NoProfile", "-File", "scripts/defender-scan.ps1", `"${alvo}"`]);

  console.log("\n--- VirusTotal ---");
  const vt = run("bun", ["scripts/virustotal.ts", `"${alvo}"`, "--edition", edicao]);
  resultados.push({ file: alvo, defender: defenderResult(def), virustotal: virusTotalResult(vt) });
}

// Resumo por arquivo no fim: a saída de cada motor rola para fora da tela, e
// "tudo limpo" só sai com os dois motores limpos em todo arquivo.
const resumo = scanSummary(resultados);
console.log(`\n${"=".repeat(70)}`);
console.log(resumo.lines.join("\n"));
process.exit(resumo.exitCode);
