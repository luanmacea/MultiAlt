/**
 * O veredito do `bun run scan`, separado para ter teste. Cada motor vira
 * limpo, marcou, aceito ou inconclusivo — e só "limpo" (ou "aceito") nos dois
 * em **todo** arquivo sai com código 0. Motor que não rodou nunca conta como
 * limpo.
 *
 * Duas edições (decisão do dono, 10/10/2026 — ver docs/development.md, "As
 * duas edições"):
 * - **padrão** (`MultiAlt-Setup.msi`, o default): qualquer motor que marque
 *   reprova;
 * - **completa** (`--edition full`): o VirusTotal com **só** o Trapmine
 *   marcando, e com rótulo de ML (`*.ml.score`), é "aceito". Qualquer outro
 *   motor, ou o Defender, reprova igual.
 */
export type EngineResult = "clean" | "flagged" | "accepted" | "inconclusive";

export type Edition = "standard" | "full";

/** Um motor do VirusTotal que marcou o arquivo (malicious ou suspicious). */
export interface Detection {
  engine: string;
  result: string | null;
  category: string;
}

/** `scripts/defender-scan.ps1`: 0 limpo, 2 ameaça, qualquer outro = não deu para saber. */
export function defenderResult(exitCode: number): EngineResult {
  return exitCode === 0 ? "clean" : exitCode === 2 ? "flagged" : "inconclusive";
}

/**
 * Código de saída do `scripts/virustotal.ts`: 0 nenhum motor marcou, 2 algum
 * marcou, 3 aceito na edição completa, 1 erro/sem resultado.
 */
const VT_EXIT: Record<EngineResult, number> = { clean: 0, inconclusive: 1, flagged: 2, accepted: 3 };

export function virusTotalExitCode(result: EngineResult): number {
  return VT_EXIT[result];
}

export function virusTotalResult(exitCode: number): EngineResult {
  const found = (Object.keys(VT_EXIT) as EngineResult[]).find((r) => VT_EXIT[r] === exitCode);
  return found ?? "inconclusive";
}

/** O Trapmine com o rótulo do modelo de ML (`malicious.moderate.ml.score` e afins). */
export function isTrapmineMlScore(d: Detection): boolean {
  return d.engine.trim().toLowerCase() === "trapmine" && /\.ml\.score$/i.test(d.result ?? "");
}

/** Veredito do VirusTotal a partir dos motores que marcaram. */
export function classifyVirusTotal(detections: Detection[], edition: Edition): EngineResult {
  if (detections.length === 0) return "clean";
  if (edition === "full" && detections.every(isTrapmineMlScore)) return "accepted";
  return "flagged";
}

/** `--edition full` / `--edition=full` (padrão: `standard`). Valor desconhecido é erro. */
export function parseEdition(args: string[]): Edition {
  let value: string | undefined;
  args.forEach((a, i) => {
    if (a === "--edition") value = args[i + 1] ?? "";
    else if (a.startsWith("--edition=")) value = a.slice("--edition=".length);
  });
  if (value === undefined) return "standard";
  if (value === "standard" || value === "full") return value;
  throw new Error(`edição desconhecida: "${value}" (use --edition standard ou --edition full)`);
}

/** Os arquivos da linha de comando: tudo que não é flag nem o valor do `--edition`. */
export function scanTargets(args: string[]): string[] {
  return args.filter((a, i) => !a.startsWith("--") && args[i - 1] !== "--edition");
}

export interface FileScan {
  file: string;
  defender: EngineResult;
  virustotal: EngineResult;
}

const LABEL: Record<EngineResult, string> = {
  clean: "limpo",
  flagged: "MARCOU",
  accepted: "aceito (edição completa: só o Trapmine de ML)",
  inconclusive: "inconclusivo",
};

/** Linhas do resumo final e o código de saída: 2 marcou, 1 inconclusivo, 0 limpo/aceito. */
export function scanSummary(rows: FileScan[]): { exitCode: number; lines: string[] } {
  const lines = rows.map(
    (r) => `Defender: ${LABEL[r.defender]}   VirusTotal: ${LABEL[r.virustotal]}   ${r.file}`
  );
  const all = rows.flatMap((r) => [r.defender, r.virustotal]);
  if (all.includes("flagged")) {
    return { exitCode: 2, lines: [...lines, "RESULTADO: algum motor marcou — ver acima"] };
  }
  if (rows.length === 0 || all.includes("inconclusive")) {
    return {
      exitCode: 1,
      lines: [...lines, "RESULTADO: inconclusivo — nem todo motor rodou em todo arquivo"],
    };
  }
  // O Defender nunca é "aceito": só o VirusTotal da edição completa chega aqui.
  if (rows.some((r) => r.defender === "accepted")) {
    return { exitCode: 2, lines: [...lines, "RESULTADO: algum motor marcou — ver acima"] };
  }
  if (all.includes("accepted")) {
    return {
      exitCode: 0,
      lines: [...lines, "RESULTADO: aceito na edição completa — Defender limpo, no VirusTotal só o Trapmine de ML"],
    };
  }
  return { exitCode: 0, lines: [...lines, "RESULTADO: tudo limpo nos dois motores"] };
}
