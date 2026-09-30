/**
 * O veredito do `bun run scan`, separado para ter teste. Cada motor vira
 * limpo, marcou ou inconclusivo — e só "limpo nos dois" em **todo** arquivo é
 * "tudo limpo". Motor que não rodou nunca conta como limpo.
 */
export type EngineResult = "clean" | "flagged" | "inconclusive";

/** `scripts/defender-scan.ps1`: 0 limpo, 2 ameaça, qualquer outro = não deu para saber. */
export function defenderResult(exitCode: number): EngineResult {
  return exitCode === 0 ? "clean" : exitCode === 2 ? "flagged" : "inconclusive";
}

/** `scripts/virustotal.ts`: 0 nenhum motor marcou, 2 algum marcou, 1 erro/sem resultado. */
export function virusTotalResult(exitCode: number): EngineResult {
  return exitCode === 0 ? "clean" : exitCode === 2 ? "flagged" : "inconclusive";
}

export interface FileScan {
  file: string;
  defender: EngineResult;
  virustotal: EngineResult;
}

const LABEL: Record<EngineResult, string> = {
  clean: "limpo",
  flagged: "MARCOU",
  inconclusive: "inconclusivo",
};

/** Linhas do resumo final e o código de saída: 2 marcou, 1 inconclusivo, 0 limpo. */
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
  return { exitCode: 0, lines: [...lines, "RESULTADO: tudo limpo nos dois motores"] };
}
